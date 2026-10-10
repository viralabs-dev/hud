"""Documentação de projetos: achar os projetos da pasta e gravar as notas que o agente propõe.

Dentro do HUD o agente não grava arquivos: responde com blocos
````hud-doc arquivo="caminho"` (quatro crases, para poder ter ``` dentro), e o
HUD grava na pasta só depois que você confirma. Todo caminho é relativo à pasta
do HUD, conferido componente a componente (sem link, sem sair da raiz, sem nome
que o Windows recusa) e gravado de forma atômica.
"""

import os
import re
import secrets
import stat
from dataclasses import dataclass
from pathlib import Path

from . import plataforma as plat
from .text import clean_line
from .vault import SKIP_DIRS

MAX_DOC = 256 * 1024
MAX_DOCS = 200
MAX_TOTAL = 4 * 1024 * 1024
MAX_PART = 120
MAX_LEVELS = 8
MAX_DIRS = 20_000  # teto da varredura de projetos, para uma pasta enorme não travar a tela

_KANBAN = re.compile(r"^Kanban \((.+)\)\.md$")
_NN = re.compile(r"^\d{2}-.")
_FENCE = re.compile(r"^[ \t]*(`{3,}|~{3,})hud-doc(?:[ \t]+(.*))?$")
_ATTR = re.compile(r"""(\w+)=("[^"]*"|'[^']*'|\S+)""")
# Além de ":" e "\", o que o Windows não aceita em nome de arquivo.
_BAD_CHARS = set('\\:<>"|?*')
# Gravação por descritor de pasta (mkdirat/openat/renameat): Linux e macOS.
DIR_FD = (not plat.WINDOWS and os.open in os.supports_dir_fd and os.mkdir in os.supports_dir_fd
          and os.rename in os.supports_dir_fd and os.unlink in os.supports_dir_fd
          and os.stat in os.supports_dir_fd and os.stat in os.supports_follow_symlinks
          and hasattr(os, "fchmod"))
O_DIRECTORY = getattr(os, "O_DIRECTORY", 0)
_RESERVED = {"CON", "PRN", "AUX", "NUL"} | {f"{p}{n}" for p in ("COM", "LPT") for n in range(1, 10)}


class DocError(Exception):
    pass


class AlreadyExists(DocError):
    """Algum arquivo já existe; peça confirmação e use sobrescrever=True."""

    def __init__(self, existentes: list[str]):
        self.existentes = list(existentes)
        super().__init__("já existe: " + ", ".join(self.existentes))


# ---------------------------------------------------------------- projetos

@dataclass(frozen=True)
class Project:
    nome: str
    pasta: str
    kanban: str | None


def _subdirs(path: str) -> list[os.DirEntry]:
    """Subpastas visíveis, sem seguir link (nem junção no Windows)."""
    try:
        entries = list(os.scandir(path))
    except OSError:
        return []
    out = []
    for e in entries:
        try:
            if (e.name.startswith(".") or e.name in SKIP_DIRS or e.is_symlink()
                    or not e.is_dir(follow_symlinks=False) or plat.is_link(e.path)):
                continue
        except OSError:
            continue
        out.append(e)
    return sorted(out, key=lambda e: e.name)


def _files(path: str) -> list[str]:
    """Nomes dos arquivos comuns (sem link) da pasta."""
    try:
        entries = list(os.scandir(path))
    except OSError:
        return []
    out = []
    for e in entries:
        try:
            if e.is_file(follow_symlinks=False) and not plat.is_link(e.path):
                out.append(e.name)
        except OSError:
            continue
    return sorted(out)


def _project(path: str, rel: str, subdirs: list[os.DirEntry]) -> Project | None:
    nome_pasta = os.path.basename(path)
    for d in subdirs:
        if "backlog" not in d.name.lower():
            continue
        for f in _files(d.path):
            m = _KANBAN.match(f)
            if m:
                return Project(m.group(1), rel, f"{rel}/{d.name}/{f}")
    if not any(_NN.match(d.name) for d in subdirs):
        return None
    for f in _files(path):
        stem, ext = os.path.splitext(f)
        if ext.lower() == ".md" and stem.lower() == nome_pasta.lower():
            return Project(stem, rel, None)
    return None


def find_projects(root: Path, max_depth: int = 5, limit: int = 200) -> list[Project]:
    """Pastas de projeto abaixo da raiz (a raiz em si não conta).

    Projeto: tem uma subpasta "*backlog*" com um "Kanban (X).md", ou tem a nota
    MOC com o nome da pasta e subpastas "NN-algo". Não desce dentro de projeto.
    """
    root = str(root)
    if plat.is_link(root) or not os.path.isdir(root):
        return []
    out: list[Project] = []
    pending = [(root, "", 0)]
    seen = 0
    while pending and len(out) < limit and seen < MAX_DIRS:
        path, rel, depth = pending.pop()
        seen += 1
        subdirs = _subdirs(path)
        if rel:
            p = _project(path, rel, subdirs)
            if p is not None:
                out.append(p)
                continue
        if depth < max_depth:
            for d in reversed(subdirs):
                pending.append((d.path, f"{rel}/{d.name}" if rel else d.name, depth + 1))
    return sorted(out, key=lambda p: p.pasta)


# ---------------------------------------------------------------- propostas

@dataclass(frozen=True)
class DocProposal:
    arquivo: str
    conteudo: str


def valid_path(arquivo: str) -> bool:
    """Caminho relativo de nota `.md` que dá para gravar igual em Linux, macOS e Windows."""
    if not isinstance(arquivo, str) or not arquivo or clean_line(arquivo) != arquivo:
        return False
    if not arquivo.lower().endswith(".md") or _BAD_CHARS & set(arquivo):
        return False
    parts = arquivo.split("/")
    if len(parts) > MAX_LEVELS:
        return False
    for p in parts:
        if not p or len(p) > MAX_PART or p.startswith(".") or p.endswith((" ", ".")):
            return False
        if p.split(".", 1)[0].rstrip(" ").upper() in _RESERVED:
            return False
    return True


def _attr(v: str) -> str:
    if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
        return v[1:-1]
    return v


def parse_doc_proposals(texto: str) -> list[DocProposal]:
    """Blocos ```hud-doc arquivo="caminho" da resposta de um agente.

    A cerca fecha numa linha só com o mesmo caractere, de comprimento igual ou
    maior. Bloco não fechado ou com caminho inválido é descartado; o mesmo
    arquivo repetido vale o último; o que passa dos limites fica de fora.
    """
    found: dict[str, DocProposal] = {}
    lines = texto.replace("\r\n", "\n").split("\n")
    i = 0
    while i < len(lines):
        m = _FENCE.match(lines[i])
        i += 1
        if not m:
            continue
        fence = m.group(1)
        attrs = {k: _attr(v) for k, v in _ATTR.findall(m.group(2) or "")}
        body: list[str] = []
        closed = False
        while i < len(lines):
            line = lines[i]
            i += 1
            s = line.strip()
            if s and s[0] == fence[0] and len(s) >= len(fence) and set(s) == {fence[0]}:
                closed = True
                break
            body.append(line)
        arquivo = attrs.get("arquivo", "")
        if not closed or not valid_path(arquivo):
            continue
        conteudo = "\n".join(body) + "\n"
        if len(conteudo.encode("utf-8")) > MAX_DOC:
            continue
        found.pop(arquivo, None)  # o último vale, e na posição dele
        found[arquivo] = DocProposal(arquivo, conteudo)
    out, total = [], 0
    for p in found.values():
        size = len(p.conteudo.encode("utf-8"))
        if len(out) >= MAX_DOCS or total + size > MAX_TOTAL:
            break
        out.append(p)
        total += size
    return out


# ---------------------------------------------------------------- gravação

def _check_root(root: Path) -> str:
    if plat.is_link(root):
        raise DocError(f"{root} é link simbólico")
    if not os.path.isdir(root):
        raise DocError(f"{root} não é uma pasta")
    return os.path.realpath(root)


def _inside(path: Path, real_root: str) -> bool:
    return plat.inside(os.path.realpath(path), [real_root])


def _unique(proposals: list[DocProposal]) -> list[DocProposal]:
    found: dict[str, DocProposal] = {}
    for p in proposals:
        found.pop(p.arquivo, None)
        found[p.arquivo] = p
    return list(found.values())


def plan(root: Path, proposals: list[DocProposal]) -> tuple[list[str], list[str]]:
    """(novos, existentes), sem gravar nada; DocError se algum destino não serve."""
    root = Path(root)
    real_root = _check_root(root)
    novos, existentes = [], []
    total = 0
    props = _unique(proposals)
    if len(props) > MAX_DOCS:
        raise DocError(f"mais de {MAX_DOCS} arquivos")
    for p in props:
        if not valid_path(p.arquivo):
            raise DocError(f"caminho não permitido: {p.arquivo!r}")
        size = len(p.conteudo.encode("utf-8"))
        total += size
        if size > MAX_DOC:
            raise DocError(f"{p.arquivo} passa de {MAX_DOC // 1024} KB")
        cur = root
        parts = p.arquivo.split("/")
        for part in parts[:-1]:
            cur = cur / part
            if plat.is_link(cur):
                raise DocError(f"{p.arquivo}: {part} é link simbólico")
            if not os.path.lexists(cur):
                break  # o resto será criado
            if not cur.is_dir():
                raise DocError(f"{p.arquivo}: {part} existe e não é pasta")
            if not _inside(cur, real_root):
                raise DocError(f"{p.arquivo} sai da pasta do HUD")
        dest = root / p.arquivo
        if plat.is_link(dest):
            raise DocError(f"{p.arquivo} é link simbólico")
        try:
            st = os.lstat(dest)
        except FileNotFoundError:
            novos.append(p.arquivo)
            continue
        except NotADirectoryError:
            raise DocError(f"{p.arquivo}: o caminho passa por um arquivo") from None
        if not stat.S_ISREG(st.st_mode):
            raise DocError(f"{p.arquivo} existe e não é arquivo comum")
        existentes.append(p.arquivo)
    if total > MAX_TOTAL:
        raise DocError(f"as notas passam de {MAX_TOTAL // (1024 * 1024)} MB no total")
    return novos, existentes


def _mkdirs(root: Path, real_root: str, parts: list[str]) -> Path:
    """Cria as pastas que faltam (755), conferindo cada nível pelo caminho (sem dir_fd)."""
    cur = root
    for part in parts:
        cur = cur / part
        try:
            os.mkdir(cur, 0o755)
            created = True
        except FileExistsError:
            created = False
        if plat.is_link(cur) or not cur.is_dir():
            raise DocError(f"{cur} virou link ou deixou de ser pasta")
        if not _inside(cur, real_root):
            raise DocError(f"{cur} sai da pasta do HUD")
        if created and not plat.WINDOWS:
            os.chmod(cur, 0o755)  # 755 mesmo com umask 077: é documentação do projeto
    return cur


def _antes_de_escrever(dest: Path) -> None:
    """Entre a criação das pastas e a escrita de uma nota. Não faz nada: é o
    ponto em que os testes trocam uma pasta por um link, de forma determinística."""


def _folder_inside(folder: Path, real_root: str) -> bool:
    return not plat.is_link(folder) and folder.is_dir() and _inside(folder, real_root)


def _write(dest: Path, data: bytes, real_root: str) -> None:
    """Gravação pelo caminho (Windows, ou sem dir_fd): estreita a janela, sem garantia.

    Confere que a pasta continua dentro da raiz antes de abrir o temporário,
    logo antes e logo depois do `os.replace`. Uma pasta trocada por link ou
    junção entre essas conferências ainda passa (ADR-HUD-013).
    """
    folder = dest.parent
    if not _folder_inside(folder, real_root):
        raise DocError(f"{folder} virou link ou saiu da pasta do HUD")
    tmp = dest.with_name(f".{dest.name}.{secrets.token_hex(4)}.tmp")
    fd = plat.open_nofollow(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | plat.O_CLOEXEC
                            | plat.O_BINARY, 0o644)
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
            fh.flush()
            os.fsync(fh.fileno())
            if not plat.WINDOWS:  # no Windows o modo só liga e desliga somente-leitura
                os.fchmod(fh.fileno(), 0o644)
        if plat.is_link(dest) or (os.path.lexists(dest) and not os.path.isfile(dest)):
            raise DocError(f"{dest} virou link ou deixou de ser arquivo comum")
        if not _folder_inside(folder, real_root):
            raise DocError(f"{folder} virou link ou saiu da pasta do HUD")
        os.replace(tmp, dest)
    except BaseException:
        try:
            tmp.unlink(missing_ok=True)
        except OSError:
            pass
        raise
    if not _inside(folder, real_root):
        try:
            os.unlink(dest)
        except OSError:
            pass
        raise DocError(f"{folder} saiu da pasta do HUD durante a gravação; {dest.name} apagado")


# ------------------------------------------------- gravação por descritor (POSIX)

def _open_dir(name, dir_fd: int | None = None) -> int:
    """Abre uma pasta sem seguir link e confere pelo fstat que é pasta."""
    flags = os.O_RDONLY | O_DIRECTORY | plat.O_CLOEXEC
    if dir_fd is None:
        fd = plat.open_nofollow(name, flags)
    else:
        fd = os.open(name, flags | plat.O_NOFOLLOW, dir_fd=dir_fd)
    try:
        if not stat.S_ISDIR(os.fstat(fd).st_mode):
            raise DocError(f"{name} não é pasta")
    except BaseException:
        os.close(fd)
        raise
    return fd


def _mkdirs_fd(top: int, parts: list[str], fds: list[int]) -> None:
    """Cria e abre cada nível relativo ao anterior (mkdirat/openat com O_NOFOLLOW).

    Os descritores abertos vão para `fds` (quem chama fecha), começando pela raiz.
    """
    cur = top
    for part in parts:
        try:
            os.mkdir(part, 0o755, dir_fd=cur)
            created = True
        except FileExistsError:
            created = False
        try:
            fd = _open_dir(part, dir_fd=cur)
        except OSError as e:  # ELOOP/ENOTDIR: virou link ou não é pasta
            raise DocError(f"{part} virou link ou deixou de ser pasta: {e}") from None
        fds.append(fd)
        if created:
            os.fchmod(fd, 0o755)  # 755 mesmo com umask 077: é documentação do projeto
        cur = fd


def _same_chain(fds: list[int], parts: list[str]) -> bool:
    """Cada pasta aberta ainda é a que está no caminho, sem link no meio."""
    for parent, fd, part in zip(fds, fds[1:], parts):
        try:
            st = os.stat(part, dir_fd=parent, follow_symlinks=False)
        except OSError:
            return False
        aberto = os.fstat(fd)
        if not stat.S_ISDIR(st.st_mode) or (st.st_dev, st.st_ino) != (aberto.st_dev, aberto.st_ino):
            return False
    return True


def _write_fd(fds: list[int], parts: list[str], name: str, data: bytes) -> None:
    """Temporário e `os.replace` relativos à pasta já aberta: nada cai fora da raiz."""
    folder = fds[-1]
    tmp = f".{name}.{secrets.token_hex(4)}.tmp"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | plat.O_NOFOLLOW | plat.O_CLOEXEC,
                 0o644, dir_fd=folder)
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
            fh.flush()
            os.fsync(fh.fileno())
            os.fchmod(fh.fileno(), 0o644)
        if not _same_chain(fds, parts):
            raise DocError(f"{'/'.join(parts)}: uma pasta virou link ou foi trocada durante a gravação")
        try:
            st = os.stat(name, dir_fd=folder, follow_symlinks=False)
        except FileNotFoundError:
            st = None
        if st is not None and not stat.S_ISREG(st.st_mode):
            raise DocError(f"{name} virou link ou deixou de ser arquivo comum")
        os.replace(tmp, name, src_dir_fd=folder, dst_dir_fd=folder)
    except BaseException:
        try:
            os.unlink(tmp, dir_fd=folder)
        except OSError:
            pass
        raise


def _save_fd(root: Path, props: list[DocProposal]) -> list[str]:
    try:
        top = _open_dir(root)
    except OSError as e:
        raise DocError(f"{root} virou link ou deixou de ser pasta: {e}") from None
    out = []
    try:
        for p in props:
            parts = p.arquivo.split("/")
            fds = [top]
            try:
                _mkdirs_fd(top, parts[:-1], fds)
                _antes_de_escrever(root / p.arquivo)
                _write_fd(fds, parts[:-1], parts[-1], p.conteudo.encode("utf-8"))
            except OSError as e:
                raise DocError(f"{p.arquivo}: {e}") from None
            finally:
                for fd in fds[1:]:
                    os.close(fd)
            out.append(p.arquivo)
    finally:
        os.close(top)
    return out


def save_docs(root: Path, proposals: list[DocProposal], sobrescrever: bool = False) -> list[str]:
    """Grava as notas na pasta do HUD; confere tudo antes de escrever qualquer uma.

    Com dir_fd (Linux, macOS) cada pasta é criada e aberta relativa à anterior e
    o temporário e o `os.replace` são relativos à pasta aberta, então trocar uma
    pasta por link no meio não leva a escrita para fora. Sem dir_fd (Windows) a
    gravação é pelo caminho, com conferências antes e depois (sem garantia).
    """
    root = Path(root)
    _novos, existentes = plan(root, proposals)
    if existentes and not sobrescrever:
        raise AlreadyExists(existentes)
    real_root = _check_root(root)
    props = _unique(proposals)
    if DIR_FD:
        return _save_fd(root, props)
    out = []
    for p in props:
        parts = p.arquivo.split("/")
        folder = _mkdirs(root, real_root, parts[:-1])
        _antes_de_escrever(root / p.arquivo)
        _write(folder / parts[-1], p.conteudo.encode("utf-8"), real_root)
        out.append(p.arquivo)
    return out
