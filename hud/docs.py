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
    """Cria as pastas que faltam (755), conferindo cada nível."""
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


def _write(dest: Path, data: bytes) -> None:
    tmp = dest.with_name(f".{dest.name}.{secrets.token_hex(4)}.tmp")
    fd = plat.open_nofollow(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | plat.O_CLOEXEC
                            | plat.O_BINARY, 0o644)
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
            if not plat.WINDOWS:  # no Windows o modo só liga e desliga somente-leitura
                os.fchmod(fh.fileno(), 0o644)
        if plat.is_link(dest) or (os.path.lexists(dest) and not os.path.isfile(dest)):
            raise DocError(f"{dest} virou link ou deixou de ser arquivo comum")
        os.replace(tmp, dest)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise


def save_docs(root: Path, proposals: list[DocProposal], sobrescrever: bool = False) -> list[str]:
    """Grava as notas na pasta do HUD; confere tudo antes de escrever qualquer uma."""
    root = Path(root)
    _novos, existentes = plan(root, proposals)
    if existentes and not sobrescrever:
        raise AlreadyExists(existentes)
    real_root = _check_root(root)
    out = []
    for p in _unique(proposals):
        parts = p.arquivo.split("/")
        folder = _mkdirs(root, real_root, parts[:-1])
        _write(folder / parts[-1], p.conteudo.encode("utf-8"))
        out.append(p.arquivo)
    return out
