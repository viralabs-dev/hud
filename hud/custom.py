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
import signal
import stat
import subprocess
import threading
import time
from dataclasses import dataclass
from pathlib import Path

from . import layout as lay
from .config import ConfigError, check_private_file
from .layout import Layout, LayoutError, NAME_RE, TEXT_FILE_RE
from .runner import OUTPUT_CAP, safe_env
from .text import clean

TRUST_FILE = "custom_confianca.json"
CHOICE_FILE = "custom"
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
    return Path("~/.local/share/hud/custom").expanduser()


def _dir(root: Path, nome: str) -> Path:
    if not valid_name(nome):
        raise CustomError(f"nome inválido: {nome!r} (a-z, 0-9, _ e -, até 40)")
    return Path(root) / nome


def list_customs(root: Path) -> list[tuple[str, str, str | None]]:
    root = Path(root)
    if not root.is_dir():
        return []
    out = []
    for entry in sorted(os.scandir(root), key=lambda e: e.name):
        if entry.name.startswith(".") or not (entry.is_dir() or entry.is_symlink()):
            continue
        try:
            out.append((entry.name, load_custom(root, entry.name).descricao, None))
        except (LayoutError, OSError) as e:
            out.append((entry.name, "", str(e)))
    return out


def load_custom(root: Path, nome: str) -> Layout:
    return lay.load(_dir(root, nome))


def digest(root: Path, nome: str) -> str:
    return hashlib.sha256(lay.read_small(_dir(root, nome) / "layout.toml")).hexdigest()


def _write_private(path: Path, text: str) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp")
    try:
        tmp.unlink()
    except FileNotFoundError:
        pass
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(text)
    os.replace(tmp, path)


class Trust:
    """Registro `{nome: sha256}` das customizações cujos comandos podem rodar."""

    def __init__(self, data_dir: Path):
        self.path = Path(data_dir) / TRUST_FILE

    def _read(self) -> dict[str, str]:
        try:
            if self.path.is_symlink():
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


def remembered(data_dir: Path) -> str | None:
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
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o644)
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
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
    if d.is_symlink():
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
        if (d / arq).is_symlink():
            raise CustomError(f"{arq} é link simbólico")
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    if root.is_symlink() or not root.is_dir():
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
    text = clean(raw.decode("utf-8", "replace")).rstrip("\n")
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
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
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
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
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
                start_new_session=True, close_fds=True, shell=False,
            )
            with self._lock:
                self._st[pid].proc = proc
            if self._stop.is_set():
                os.killpg(proc.pid, signal.SIGKILL)

            def kill() -> None:
                nonlocal timed_out
                timed_out = True
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

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


def skill_dir() -> Path | None:
    """A skill hud-custom do repositório (a mesma ligada em ~/.claude e ~/.codex)."""
    p = Path(__file__).resolve().parent.parent / "skills" / "hud-custom"
    return p if (p / "SKILL.md").is_file() else None


def agent_context(root: Path) -> str:
    """O que o Claude e o Codex precisam saber para propor customizações de dentro do HUD."""
    skill = skill_dir()
    where = f" ({skill / 'SKILL.md'})" if skill else ""
    return (
        f"Você está dentro do HUD de terminal. Para mudar o layout do HUD, siga a skill "
        f"hud-custom{where}; as customizações ficam em {root}. Dentro do HUD não grave "
        "arquivos: responda com um bloco ```hud-custom nome=<nome> arquivo=<arquivo> por "
        "arquivo e diga ao usuário para digitar /custom salvar e depois /custom <nome>."
    )
