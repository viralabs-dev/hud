"""Customizações da tela: pastas `custom/<nome>/`, confiança e painéis próprios.

Uma customização é só dados (layout e textos), exceto os painéis `comando`,
que rodam programas. Esses só rodam depois que você confia no conteúdo exato
do `layout.toml` (sha256); se o arquivo mudar, a confiança cai.
"""

import errno
import hashlib
import json
import os
import re
import stat
import subprocess
import threading
import time
from dataclasses import dataclass
from pathlib import Path

from . import layout as lay
from . import plataforma as plat
from .config import ConfigError, check_private_file
from .layout import Layout, LayoutError, NAME_RE, TEXT_FILE_RE
from .runner import OUTPUT_CAP, safe_env
from .text import clean

TRUST_FILE = "custom_confianca.json"
CHOICE_FILE = "custom-escolhida"
# Até a v0.17.0 a escolha ficava em `custom`, o mesmo caminho da pasta de
# customizações do HUD instalado (`custom_root`): uma das duas sempre falhava.
LEGACY_CHOICE_FILE = "custom"
TAIL_BYTES = 256 * 1024
MAX_LINES = 2000


class CustomError(LayoutError):
    pass


class AlreadyExists(CustomError):
    """A pasta da customização já existe; peça confirmação e use sobrescrever=True."""


def valid_name(nome: str) -> bool:
    return isinstance(nome, str) and NAME_RE.fullmatch(nome) is not None


def custom_root(repo_hint: Path | None = None) -> Path:
    repo = Path(repo_hint) if repo_hint is not None else Path(__file__).resolve().parent.parent
    if (repo / "pyproject.toml").is_file():
        return repo / "custom"
    return plat.default_data_dir() / "custom"


# O binário e o pacote do pipx trazem uma cópia de custom/ e de skills/ dentro do
# pacote hud (hud/_modelos e hud/_skills; packaging/hud.spec e pyproject.toml). No
# repositório elas não existem: lá custom/ já é a raiz e as skills são as de skills/.
_PACOTE = Path(__file__).resolve().parent


def templates_root() -> Path | None:
    """Os modelos que vêm com o HUD, somente leitura (None rodando do repositório)."""
    p = _PACOTE / "_modelos"
    return p if p.is_dir() else None


def _dir(root: Path, nome: str) -> Path:
    if not valid_name(nome):
        raise CustomError(f"nome inválido: {nome!r} (a-z, 0-9, _ e -, até 40)")
    return Path(root) / nome


def find(root: Path, nome: str) -> Path:
    """A pasta da customização: a do usuário ou, se não houver, o modelo embutido."""
    d = _dir(root, nome)
    if d.exists() or d.is_symlink():
        return d
    t = templates_root()
    if t is not None and (t / nome / "layout.toml").is_file():
        return t / nome
    return d


def _scan(root: Path) -> list[str]:
    if not root.is_dir():
        return []
    return [e.name for e in os.scandir(root) if not e.name.startswith(".")
            and (e.is_dir() or e.is_symlink() or plat.is_link(e.path))]


def list_customs(root: Path) -> list[tuple[str, str, str | None, bool]]:
    """(nome, descrição, erro, embutido): as do usuário e os modelos que elas não cobrem."""
    root = Path(root)
    t = templates_root()
    mine = set(_scan(root))
    nomes = mine | ({n for n in _scan(t) if valid_name(n)} if t is not None else set())
    out = []
    for nome in sorted(nomes):
        embutido = nome not in mine
        try:
            d = (t if embutido else root) / nome
            out.append((nome, lay.load(d).descricao, None, embutido))
        except (LayoutError, OSError) as e:
            out.append((nome, "", str(e), embutido))
    return out


def load_custom(root: Path, nome: str) -> Layout:
    return lay.load(find(root, nome))


def digest(root: Path, nome: str) -> str:
    return hashlib.sha256(lay.read_small(find(root, nome) / "layout.toml")).hexdigest()


def _write_private(path: Path, text: str) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp")
    try:
        tmp.unlink()
    except FileNotFoundError:
        pass
    fd = plat.open_nofollow(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | plat.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(text)
    os.replace(tmp, path)


class Trust:
    """Registro `{nome: sha256}` das customizações cujos comandos podem rodar."""

    def __init__(self, data_dir: Path):
        self.path = Path(data_dir) / TRUST_FILE

    def _read(self) -> dict[str, str]:
        try:
            if plat.is_link(self.path):
                return {}
            check_private_file(self.path)
            data = json.loads(lay.read_small(self.path).decode("utf-8"))
        except (OSError, ConfigError, LayoutError, ValueError):
            return {}  # na dúvida, nada é confiado
        if not isinstance(data, dict):
            return {}
        return {k: v for k, v in data.items() if isinstance(k, str) and isinstance(v, str)}

    def is_trusted(self, nome: str, digest: str) -> bool:
        return bool(digest) and self._read().get(nome) == digest

    def trust(self, nome: str, digest: str) -> None:
        if not valid_name(nome):
            raise CustomError(f"nome inválido: {nome!r}")
        data = self._read()
        data[nome] = digest
        _write_private(self.path, json.dumps(data, indent=1, sort_keys=True) + "\n")


def needs_trust(layout: Layout) -> list[tuple[str, ...]]:
    """Os argv que rodariam: o que a UI mostra antes de pedir "s"."""
    used = set(layout.ids())
    return [p.argv for p in layout.paineis.values() if p.tipo == "comando" and p.id in used]


def _migrate_choice(data_dir: Path) -> None:
    """Move a escolha do nome antigo (`custom`, arquivo) para `custom-escolhida`."""
    old, new = Path(data_dir) / LEGACY_CHOICE_FILE, Path(data_dir) / CHOICE_FILE
    if old.is_symlink() or not old.is_file() or new.exists():
        return
    check_private_file(old)
    os.replace(old, new)


def remembered(data_dir: Path) -> str | None:
    _migrate_choice(data_dir)
    f = Path(data_dir) / CHOICE_FILE
    if not f.exists():
        return None
    check_private_file(f)
    try:
        nome = lay.read_small(f, 256).decode("utf-8", "replace").strip()
    except LayoutError as e:
        raise ConfigError(str(e)) from None
    if not valid_name(nome):
        raise ConfigError(f"{f}: nome de customização inválido")
    return nome


def remember(data_dir: Path, nome: str | None) -> None:
    """Grava (600) a customização escolhida com /custom, ou volta ao padrão com None."""
    f = Path(data_dir) / CHOICE_FILE
    if nome is None:
        f.unlink(missing_ok=True)
        return
    if not valid_name(nome):
        raise CustomError(f"nome inválido: {nome!r}")
    _write_private(f, nome + "\n")


# ---------------------------------------------------------------- propostas

@dataclass(frozen=True)
class Proposal:
    nome: str
    arquivo: str
    conteudo: str


_FENCE = re.compile(r"^[ \t]*(`{3,}|~{3,})hud-custom(?:[ \t]+(.*))?$")
_ATTR = re.compile(r"""(\w+)=("[^"]*"|'[^']*'|\S+)""")


def valid_file(arquivo: str) -> bool:
    return isinstance(arquivo, str) and (
        arquivo == "layout.toml" or TEXT_FILE_RE.fullmatch(arquivo) is not None)


def parse_proposals(texto: str) -> list[Proposal]:
    """Blocos ```hud-custom nome=X arquivo=Y da resposta de um agente.

    Bloco com nome ou arquivo inválido (``../``, ``/``, ``.py``...) é descartado.
    """
    found: dict[tuple[str, str], Proposal] = {}
    lines = texto.split("\n")
    i = 0
    while i < len(lines):
        m = _FENCE.match(lines[i])
        i += 1
        if not m:
            continue
        fence = m.group(1)
        attrs = {k: v.strip("\"'") for k, v in _ATTR.findall(m.group(2) or "")}
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
        nome, arquivo = attrs.get("nome", ""), attrs.get("arquivo", "")
        if not closed or not valid_name(nome) or not valid_file(arquivo):
            continue
        conteudo = "\n".join(body) + "\n"
        if len(conteudo.encode("utf-8")) > lay.MAX_FILE:
            continue
        found[(nome, arquivo)] = Proposal(nome, arquivo, conteudo)
    return list(found.values())


def _write_shared(path: Path, data: bytes) -> None:
    tmp = path.with_name(f".{path.name}.tmp")
    try:
        tmp.unlink()
    except FileNotFoundError:
        pass
    fd = plat.open_nofollow(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | plat.O_CLOEXEC, 0o644)
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
            if not plat.WINDOWS:  # no Windows o modo só liga e desliga somente-leitura
                os.fchmod(fh.fileno(), 0o644)  # 644 mesmo com umask 077: é para compartilhar
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise
    os.replace(tmp, path)


def save_proposals(root: Path, proposals: list[Proposal],
                   sobrescrever: bool = False) -> tuple[str, list[str]]:
    if not proposals:
        raise CustomError("nenhuma proposta para salvar")
    nomes = {p.nome for p in proposals}
    if len(nomes) != 1:
        raise CustomError(f"propostas de customizações diferentes: {', '.join(sorted(nomes))}")
    nome = nomes.pop()
    d = _dir(root, nome)
    files: dict[str, bytes] = {}
    for p in proposals:
        if not valid_file(p.arquivo) or "/" in p.arquivo or ".." in p.arquivo:
            raise CustomError(f"arquivo não permitido: {p.arquivo!r}")
        data = p.conteudo.encode("utf-8")
        if len(data) > lay.MAX_FILE:
            raise CustomError(f"{p.arquivo} passa de 64 KB")
        files[p.arquivo] = data
    exists = d.exists() or d.is_symlink()
    if plat.is_link(d):
        raise CustomError(f"{d} é link simbólico")
    if exists and not d.is_dir():
        raise CustomError(f"{d} existe e não é pasta")
    if "layout.toml" in files:
        lay.parse_text(files["layout.toml"], nome, None)  # LayoutError sobe sem gravar nada
    elif not exists:
        raise CustomError("falta o layout.toml da customização")
    if exists and not sobrescrever:
        raise AlreadyExists(f"custom/{nome} já existe")
    for arq in files:
        if plat.is_link(d / arq):
            raise CustomError(f"{arq} é link simbólico")
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    if plat.is_link(root) or not root.is_dir():
        raise CustomError(f"{root} não é uma pasta")
    if not exists:
        d.mkdir()
        os.chmod(d, 0o755)
    if os.path.realpath(d) != os.path.join(os.path.realpath(root), nome):
        raise CustomError(f"{d} sai da raiz das customizações")
    order = sorted(files, key=lambda a: a == "layout.toml")  # layout por último
    for arq in order:
        _write_shared(d / arq, files[arq])
    return nome, order


# ------------------------------------------------------------ painéis próprios

def _ago(sec: float) -> str:
    sec = max(0, int(sec))
    if sec < 60:
        return f"há {sec}s"
    if sec < 3600:
        return f"há {sec // 60}min"
    return f"há {sec // 3600}h"


def _lines(raw: bytes, limit: int) -> list[str]:
    text = clean(plat.decode_output(raw)).rstrip("\n")
    lines = text.split("\n") if text else []
    return lines[-limit:]


class _State:
    __slots__ = ("lines", "error", "updated", "sig", "next_run", "running", "proc", "note")

    def __init__(self):
        self.lines: list[str] = []
        self.error: str | None = None
        self.updated: float | None = None
        self.sig = None
        self.next_run = 0.0
        self.running = False
        self.proc: subprocess.Popen | None = None
        self.note = ""


class PanelFeed:
    """Mantém, numa thread, o conteúdo dos painéis próprios usados no layout."""

    def __init__(self, layout: Layout, base_dir: Path | None, trusted: bool, tick: float = 1.0):
        self.layout = layout
        self.base_dir = Path(base_dir) if base_dir is not None else None
        self.trusted = trusted
        self.tick = tick
        used = set(layout.ids())
        self.panels = {pid: p for pid, p in layout.paineis.items() if pid in used}
        self._st = {pid: _State() for pid in self.panels}
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        if not trusted:
            msg = f"comando não confiado: /custom {layout.nome} para revisar"
            for pid, p in self.panels.items():
                if p.tipo == "comando":
                    self._st[pid].lines = [msg]
                    self._st[pid].note = "não confiado"

    # API para a UI
    def start(self) -> None:
        if self._thread is None and self.panels:
            self._thread = threading.Thread(target=self._loop, name="hud-panels", daemon=True)
            self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        with self._lock:
            procs = [s.proc for s in self._st.values() if s.proc is not None]
        for proc in procs:
            plat.kill_tree(proc)
        if self._thread is not None:
            self._thread.join(timeout=2)

    def lines(self, pid: str) -> list[str]:
        with self._lock:
            st = self._st.get(pid)
            return list(st.lines) if st else []

    def status(self, pid: str) -> str:
        with self._lock:
            st = self._st.get(pid)
            if st is None:
                return ""
            if st.error:
                return f"erro: {st.error}"
            if st.note and st.updated is None:
                return st.note
            if st.updated is None:
                return "carregando"
            age = _ago(time.time() - st.updated)
            return f"{st.note} · {age}" if st.note else age

    # trabalho
    def _loop(self) -> None:
        while not self._stop.is_set():
            for pid, p in self.panels.items():
                if self._stop.is_set():
                    break
                try:
                    if p.tipo == "texto":
                        self._read_text(pid, p)
                    elif p.tipo == "arquivo":
                        self._read_tail(pid, p)
                    elif self.trusted:
                        self._maybe_run(pid, p)
                except Exception as e:  # um painel com problema não derruba os outros
                    self._fail(pid, type(e).__name__)
            self._stop.wait(self.tick)

    def _fail(self, pid: str, msg: str) -> None:
        with self._lock:
            st = self._st[pid]
            st.error, st.lines, st.sig = msg, [], None

    def _set(self, pid: str, lines: list[str], sig=None, note: str = "",
             when: float | None = None) -> None:
        with self._lock:
            st = self._st[pid]
            st.lines, st.error, st.sig, st.note = lines, None, sig, note
            st.updated = when if when is not None else time.time()

    def _sig(self, pid: str):
        with self._lock:
            return self._st[pid].sig

    def _open(self, path: str):
        """Abre só arquivo comum, sem seguir link no último componente e sem travar em FIFO."""
        fd = plat.open_nofollow(path, os.O_RDONLY | plat.O_NONBLOCK | plat.O_CLOEXEC)
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            os.close(fd)
            raise OSError(errno.EINVAL, "não é arquivo comum")
        return fd, st

    def _read_text(self, pid: str, p) -> None:
        if self.base_dir is None:
            return self._fail(pid, "sem pasta da customização")
        path = self.base_dir / p.arquivo
        try:
            fd, st = self._open(str(path))
        except OSError as e:
            if e.errno == errno.ELOOP:
                return self._fail(pid, "link simbólico")
            return self._fail(pid, "arquivo não encontrado" if e.errno == errno.ENOENT
                              else (e.strerror or "não abre"))
        with os.fdopen(fd, "rb") as fh:
            sig = (st.st_mtime_ns, st.st_size, st.st_ino)
            if sig == self._sig(pid):
                return
            if st.st_size > lay.MAX_FILE:
                return self._fail(pid, f"{p.arquivo} passa de 64 KB")
            data = fh.read(lay.MAX_FILE + 1)[: lay.MAX_FILE]
        self._set(pid, _lines(data, MAX_LINES), sig, when=st.st_mtime)

    def _read_tail(self, pid: str, p) -> None:
        why = lay.denied_path(p.caminho)  # de novo: um link pode ter mudado de alvo
        if why:
            return self._fail(pid, "caminho negado")
        real = os.path.realpath(os.path.expanduser(p.caminho))
        try:
            fd, st = self._open(real)
        except OSError as e:
            if e.errno == errno.ENOENT:
                return self._fail(pid, "arquivo não encontrado")
            return self._fail(pid, e.strerror or "não abre")
        with os.fdopen(fd, "rb") as fh:
            sig = (st.st_mtime_ns, st.st_size, st.st_ino)
            if sig == self._sig(pid):
                return
            start = max(0, st.st_size - TAIL_BYTES)
            fh.seek(start)
            data = fh.read(TAIL_BYTES)
        if start > 0:
            nl = data.find(b"\n")
            data = data[nl + 1:] if nl >= 0 else b""  # descarta a linha cortada
        self._set(pid, _lines(data, p.linhas), sig, when=st.st_mtime)

    def _maybe_run(self, pid: str, p) -> None:
        now = time.monotonic()
        with self._lock:
            st = self._st[pid]
            if st.running or now < st.next_run:
                return
            st.running = True
            st.next_run = now + p.intervalo
        threading.Thread(target=self._run, args=(pid, p), daemon=True).start()

    def _run(self, pid: str, p) -> None:
        buf = bytearray()
        timed_out = False
        try:
            proc = subprocess.Popen(
                list(p.argv), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, env=safe_env(), cwd=os.path.expanduser("~"),
                close_fds=True, shell=False, **plat.popen_group_kwargs(),
            )
            with self._lock:
                self._st[pid].proc = proc
            if self._stop.is_set():
                plat.kill_tree(proc)

            def kill() -> None:
                nonlocal timed_out
                timed_out = True
                plat.kill_tree(proc)

            timer = threading.Timer(p.timeout, kill)
            timer.daemon = True
            timer.start()
            try:
                while chunk := proc.stdout.read1(65536):
                    room = OUTPUT_CAP - len(buf)
                    if room > 0:
                        buf += chunk[:room]
                rc = proc.wait()
            finally:
                timer.cancel()
                proc.stdout.close()
            lines = _lines(bytes(buf), MAX_LINES)
            if timed_out:
                with self._lock:
                    st = self._st[pid]
                    st.lines, st.error = lines, f"tempo esgotado ({p.timeout:.0f}s)"
            else:
                self._set(pid, lines, note=f"saiu com {rc}" if rc else "")
        except OSError as e:
            self._fail(pid, f"falha ao iniciar: {e.strerror}")
        finally:
            with self._lock:
                self._st[pid].running = False
                self._st[pid].proc = None


SKILL_MAX = 1 << 20  # arquivo de skill maior que isso não é copiado


def _skill_names(root: Path) -> list[str]:
    """As skills de uma pasta: subpastas não ocultas, que não são link, com SKILL.md."""
    try:
        entries = sorted(root.iterdir())
    except OSError:
        return []
    return [p.name for p in entries
            if not p.name.startswith(".") and not plat.is_link(p) and p.is_dir()
            and (p / "SKILL.md").is_file() and not plat.is_link(p / "SKILL.md")]


def skills_root() -> Path | None:
    """As skills que vêm com o HUD: skills/ do repositório ou a cópia embutida (hud/_skills)."""
    repo = _PACOTE.parent / "skills"
    if _skill_names(repo):
        return repo
    p = _PACOTE / "_skills"
    return p if p.is_dir() else None


def skill_dir() -> Path | None:
    """A skill hud-custom: a do repositório ou a cópia embutida no binário/pipx."""
    root = skills_root()
    if root is not None and "hud-custom" in _skill_names(root):
        return root / "hud-custom"
    return None


def skill_homes(env=None) -> list[Path]:
    """Onde o Claude Code e o Codex procuram skills do usuário."""
    env = os.environ if env is None else env
    home = Path.home()
    claude = Path(env.get("CLAUDE_CONFIG_DIR") or home / ".claude")
    codex = Path(env.get("CODEX_HOME") or home / ".codex")
    return [claude / "skills", codex / "skills"]


def _skill_files(src: Path) -> list[Path]:
    """Os arquivos de uma skill (caminhos relativos): comuns, não ocultos, até 1 MB, sem links."""
    out = []
    for dirpath, dirnames, filenames in os.walk(src, followlinks=False):
        base = Path(dirpath)
        dirnames[:] = sorted(n for n in dirnames
                             if not n.startswith(".") and not plat.is_link(base / n))
        for n in sorted(filenames):
            f = base / n
            if n.startswith(".") or n.endswith(".tmp"):
                continue
            try:
                st = os.lstat(f)
            except OSError:
                continue
            if stat.S_ISREG(st.st_mode) and not plat.is_reparse(st) and st.st_size <= SKILL_MAX:
                out.append(f.relative_to(src))
    return out


def _mkdir_nofollow(path: Path) -> None:
    """Cria a pasta (o pai já existe) sem seguir link: uma que já é link é recusada."""
    if plat.is_link(path):
        raise CustomError(f"{path} é link simbólico")
    try:
        path.mkdir()
    except FileExistsError:
        pass
    else:
        if not plat.WINDOWS:
            os.chmod(path, 0o755)
    if plat.is_link(path) or not path.is_dir():
        raise CustomError(f"{path} não é uma pasta")


def _install_one(src: Path, home: Path, nome: str) -> str:
    t = home / nome
    files = _skill_files(src)
    if not home.is_dir():
        _mkdir_nofollow(home)  # ~/.claude/skills (a pasta do agente já existe)
    _mkdir_nofollow(t)
    if os.path.realpath(t) != os.path.join(os.path.realpath(home), nome):
        raise CustomError(f"{t} sai da pasta de skills")
    for rel in files:
        d = t
        for part in rel.parts[:-1]:
            d = d / part
            _mkdir_nofollow(d)
        dst = t / rel
        if plat.is_link(dst):
            raise CustomError(f"{dst} é link simbólico")
        if dst.exists() and not dst.is_file():
            raise CustomError(f"{dst} existe e não é arquivo")
        _write_shared(dst, (src / rel).read_bytes())
    return "instalada"


def install_skill(homes: list[Path] | None = None) -> list[tuple[Path, str, bool]]:
    """Copia as skills do HUD para o Claude e o Codex (hud --instalar-skill).

    Devolve (destino, situação, ok) por skill e por agente. Só instala onde a
    pasta do agente já existe (~/.claude, ~/.codex). Um destino que é link (a
    instalação pelo repositório, ln -s) é mantido como está. Repetir atualiza;
    arquivo que saiu da skill fica no destino.
    """
    root = skills_root()
    nomes = _skill_names(root) if root is not None else []
    if not nomes:
        raise CustomError("nenhuma skill veio com este HUD")
    out = []
    for home in skill_homes() if homes is None else homes:
        agent_home = home.parent
        for nome in nomes:
            t = home / nome
            if not agent_home.is_dir():
                out.append((t, f"{agent_home} não existe: pulado", False))
            elif plat.is_link(t):
                out.append((t, "já é um link (instalação pelo repositório): mantido", True))
            elif t.exists() and not t.is_dir():
                out.append((t, "existe e não é pasta: pulado", False))
            else:
                try:
                    out.append((t, _install_one(root / nome, home, nome), True))
                except (CustomError, OSError) as e:
                    out.append((t, f"recusado: {e}", False))
    return out


def agent_context(root: Path) -> str:
    """O que o Claude e o Codex precisam saber para propor customizações de dentro do HUD."""
    skill = skill_dir()
    where = f" ({skill / 'SKILL.md'})" if skill else ""
    t = templates_root()
    modelos = f" (os modelos que vêm com o HUD, só para ler, ficam em {t})" if t else ""
    return (
        f"Você está dentro do HUD de terminal. Para mudar o layout do HUD, siga a skill "
        f"hud-custom{where}; as customizações ficam em {root}{modelos}. Dentro do HUD não grave "
        "arquivos: responda com um bloco ```hud-custom nome=<nome> arquivo=<arquivo> por "
        "arquivo e diga ao usuário para digitar /custom salvar e depois /custom <nome>."
    )
