"""Layout da tela: colunas e painéis, lidos de `custom/<nome>/layout.toml`.

O arquivo diz onde cada painel fica e quais painéis próprios existem. Ele é
validado por inteiro antes de qualquer uso: um layout errado nunca chega à
tela pela metade. `compute` transforma o layout em caixas (y, x, h, w).
"""

import errno
import fnmatch
import os
import re
import stat
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

from .config import ConfigError, build_command
from .text import clean_line

NAME_RE = re.compile(r"[a-z0-9][a-z0-9_-]{0,39}")
MAX_FILE = 64 * 1024
MIN_H = 3
MIN_COL_W = 30

# id → (altura, peso). Altura "auto" vem calculada pela UI em `compute(auto=)`.
BUILTIN: dict[str, tuple[int | str | None, float | None]] = {
    "sistema": (11, None),
    "comandos": ("auto", None),
    "agenda": (None, 1.0),
    "uso_claude": (4, None),
    "uso_codex": ("auto", None),
    "vault": (None, 1.0),
    "saida": (None, 1.0),
}
ALIASES = {"pasta": "vault"}
TIPOS = ("texto", "arquivo", "comando")

DENIED_DIRS = ("~/.ssh", "~/.gnupg", "~/.aws", "~/.config/gh", "~/.kube",
               "~/.password-store", "~/.local/share/keyrings")
DENIED_FILES = ("~/.claude/.credentials.json", "~/.claude.json", "~/.codex/auth.json",
                "~/.netrc", "~/.pgpass")
DENIED_NAMES = (".env*", "*.pem", "*.key", "id_rsa*", "id_ed25519*")
TEXT_FILE_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}\.(md|txt)")


class LayoutError(Exception):
    pass


class TooSmall(Exception):
    """O terminal não comporta o layout."""


@dataclass(frozen=True)
class Slot:
    id: str
    altura: int | str | None = None   # int fixo ou "auto"
    peso: float | None = None


@dataclass(frozen=True)
class Column:
    largura: float | None
    slots: tuple[Slot, ...]
    # Limites em colunas de terminal; só o layout embutido usa (não vem do TOML).
    min_cols: int | None = None
    max_cols: int | None = None


@dataclass(frozen=True)
class PanelSpec:
    id: str
    titulo: str
    tipo: str
    arquivo: str | None = None
    caminho: str | None = None
    linhas: int = 200
    argv: tuple[str, ...] = ()
    intervalo: float = 30.0
    timeout: float = 10.0


@dataclass
class Layout:
    nome: str
    descricao: str = ""
    autor: str = ""
    colunas: list[Column] = field(default_factory=list)
    paineis: dict[str, PanelSpec] = field(default_factory=dict)
    avisos: list[str] = field(default_factory=list)

    def ids(self) -> list[str]:
        return [s.id for c in self.colunas for s in c.slots]


def _num(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _int(v) -> bool:
    return isinstance(v, int) and not isinstance(v, bool)


def _within(p: str, root: str) -> bool:
    return p == root or p.startswith(root.rstrip("/") + "/")


def denied_path(raw: str) -> str | None:
    """Motivo da recusa de um caminho para painel `arquivo`, ou None se pode ler."""
    exp = os.path.expanduser(raw)
    real = os.path.realpath(exp)
    for name in {os.path.basename(exp), os.path.basename(real)}:
        if any(fnmatch.fnmatch(name, pat) for pat in DENIED_NAMES):
            return f"'{raw}': nome de arquivo de segredo"
    for d in DENIED_DIRS + DENIED_FILES:
        e = os.path.expanduser(d)
        for root in {e, os.path.realpath(e)}:
            if _within(real, root) or _within(os.path.normpath(exp), root):
                return f"'{raw}': caminho de credenciais ({d})"
    return None


def _slot(raw, ci: int) -> Slot:
    if isinstance(raw, str):
        sid, altura, peso = raw, None, None
    elif isinstance(raw, dict):
        extra = set(raw) - {"id", "altura", "peso"}
        if extra:
            raise LayoutError(f"coluna {ci}: chave desconhecida em painel: {', '.join(sorted(extra))}")
        sid, altura, peso = raw.get("id"), raw.get("altura"), raw.get("peso")
    else:
        raise LayoutError(f"coluna {ci}: cada painel é um id ou uma tabela {{ id = ... }}")
    if not isinstance(sid, str) or not sid:
        raise LayoutError(f"coluna {ci}: painel sem 'id'")
    sid = ALIASES.get(sid, sid)
    if altura is not None and peso is not None:
        raise LayoutError(f"'{sid}': use 'altura' ou 'peso', não os dois")
    if altura is not None and not (_int(altura) and 3 <= altura <= 60):
        raise LayoutError(f"'{sid}': 'altura' é um inteiro de 3 a 60")
    if peso is not None and not (_num(peso) and 0 < peso <= 100):
        raise LayoutError(f"'{sid}': 'peso' é um número maior que 0 e até 100")
    if altura is None and peso is None:
        altura, peso = BUILTIN.get(sid, (None, 1.0))
    return Slot(sid, altura, float(peso) if peso is not None else None)


def _panel(raw, base_dir: Path | None, avisos: list[str]) -> PanelSpec:
    if not isinstance(raw, dict):
        raise LayoutError("[[painel]] precisa ser uma tabela")
    pid = raw.get("id")
    if not isinstance(pid, str) or not NAME_RE.fullmatch(pid):
        raise LayoutError(f"[[painel]]: id {pid!r} inválido (a-z, 0-9, _ e -, até 40)")
    if pid in BUILTIN or pid in ALIASES:
        raise LayoutError(f"[[painel]]: '{pid}' é um painel embutido")
    allowed = {"id", "titulo", "tipo", "arquivo", "caminho", "linhas", "argv",
               "intervalo", "timeout"}
    extra = set(raw) - allowed
    if extra:
        raise LayoutError(f"painel '{pid}': chave desconhecida: {', '.join(sorted(extra))}")
    titulo = raw.get("titulo", pid.upper())
    if not isinstance(titulo, str) or not titulo.strip():
        raise LayoutError(f"painel '{pid}': 'titulo' precisa ser texto")
    titulo = clean_line(titulo).strip()
    if len(titulo) > 30:
        raise LayoutError(f"painel '{pid}': 'titulo' tem até 30 caracteres")
    tipo = raw.get("tipo")
    if tipo not in TIPOS:
        raise LayoutError(f"painel '{pid}': 'tipo' precisa ser {', '.join(TIPOS)}")

    if tipo == "texto":
        arq = raw.get("arquivo")
        if not isinstance(arq, str) or not TEXT_FILE_RE.fullmatch(arq) or ".." in arq:
            raise LayoutError(f"painel '{pid}': 'arquivo' é só um nome terminado em .md ou .txt")
        if base_dir is not None and not (base_dir / arq).is_file():
            avisos.append(f"painel '{pid}': {arq} não existe na pasta")
        return PanelSpec(pid, titulo, tipo, arquivo=arq)

    if tipo == "arquivo":
        cam = raw.get("caminho")
        if not isinstance(cam, str) or not cam or "\x00" in cam:
            raise LayoutError(f"painel '{pid}': falta 'caminho'")
        if not os.path.isabs(os.path.expanduser(cam)):
            raise LayoutError(f"painel '{pid}': 'caminho' precisa ser absoluto ou começar com ~")
        why = denied_path(cam)
        if why:
            raise LayoutError(f"painel '{pid}': {why}")
        linhas = raw.get("linhas", 200)
        if not (_int(linhas) and 1 <= linhas <= 2000):
            raise LayoutError(f"painel '{pid}': 'linhas' de 1 a 2000")
        return PanelSpec(pid, titulo, tipo, caminho=cam, linhas=linhas)

    argv = raw.get("argv")
    intervalo = raw.get("intervalo", 30)
    timeout = raw.get("timeout", 10)
    if not (_num(intervalo) and 5 <= intervalo <= 3600):
        raise LayoutError(f"painel '{pid}': 'intervalo' de 5 a 3600 segundos")
    if not (_num(timeout) and 1 <= timeout <= 60):
        raise LayoutError(f"painel '{pid}': 'timeout' de 1 a 60 segundos")
    if argv is None:
        raise LayoutError(f"painel '{pid}': falta 'argv'")
    try:
        cmd = build_command(0, {"name": titulo, "argv": argv, "timeout": timeout})
    except ConfigError as e:
        raise LayoutError(f"painel '{pid}': {e}") from None
    return PanelSpec(pid, titulo, tipo, argv=tuple(cmd.argv),
                     intervalo=float(intervalo), timeout=float(timeout))


def _meta(data: dict, key: str, cut: int) -> str:
    v = data.get(key, "")
    if not isinstance(v, str):
        raise LayoutError(f"'{key}' precisa ser texto")
    return clean_line(v).strip()[:cut]


def parse(data: dict, nome: str, base_dir: Path | None = None) -> Layout:
    if not isinstance(data, dict):
        raise LayoutError("layout precisa ser uma tabela TOML")
    avisos: list[str] = []
    extra = set(data) - {"nome", "descricao", "autor", "coluna", "painel"}
    if extra:
        avisos.append(f"chave ignorada: {', '.join(sorted(extra))}")
    declared = _meta(data, "nome", 80)
    lay = Layout(nome=clean_line(nome).strip()[:80], descricao=_meta(data, "descricao", 200),
                 autor=_meta(data, "autor", 60), avisos=avisos)
    if declared and declared != lay.nome:
        avisos.append(f"nome '{declared}' difere da pasta '{lay.nome}'; vale o da pasta")

    raw_panels = data.get("painel", [])
    if not isinstance(raw_panels, list):
        raise LayoutError("[[painel]] precisa ser uma lista de tabelas")
    for raw in raw_panels:
        p = _panel(raw, base_dir, avisos)
        if p.id in lay.paineis:
            raise LayoutError(f"[[painel]] '{p.id}' declarado duas vezes")
        lay.paineis[p.id] = p

    cols = data.get("coluna")
    if not isinstance(cols, list) or not 1 <= len(cols) <= 4:
        raise LayoutError("o layout precisa de 1 a 4 [[coluna]]")
    seen: set[str] = set()
    declared_w = []
    for ci, col in enumerate(cols, 1):
        if not isinstance(col, dict):
            raise LayoutError(f"coluna {ci}: precisa ser uma tabela")
        extra = set(col) - {"largura", "paineis"}
        if extra:
            raise LayoutError(f"coluna {ci}: chave desconhecida: {', '.join(sorted(extra))}")
        larg = col.get("largura")
        if larg is not None:
            if not (_num(larg) and 15 <= larg <= 85):
                raise LayoutError(f"coluna {ci}: 'largura' de 15 a 85 (%)")
            declared_w.append(float(larg))
        raw_slots = col.get("paineis")
        if not isinstance(raw_slots, list) or not 1 <= len(raw_slots) <= 8:
            raise LayoutError(f"coluna {ci}: de 1 a 8 painéis")
        slots = []
        for raw in raw_slots:
            s = _slot(raw, ci)
            if s.id not in BUILTIN and s.id not in lay.paineis:
                raise LayoutError(f"'{s.id}' não é painel embutido nem está em [[painel]]")
            if s.id in seen:
                raise LayoutError(f"'{s.id}' aparece mais de uma vez")
            seen.add(s.id)
            slots.append(s)
        lay.colunas.append(Column(float(larg) if larg is not None else None, tuple(slots)))
    total = sum(declared_w)
    if len(declared_w) < len(cols) and total > 90:
        raise LayoutError("larguras somam mais de 90% e ainda há coluna sem largura")
    if total > 100:
        raise LayoutError("larguras somam mais de 100%")
    if "saida" not in seen:
        raise LayoutError("o painel 'saida' é obrigatório")
    for pid in lay.paineis:
        if pid not in seen:
            avisos.append(f"[[painel]] '{pid}' não está em nenhuma coluna")
    return lay


def parse_text(text: str | bytes, nome: str, base_dir: Path | None = None) -> Layout:
    if isinstance(text, bytes):
        if len(text) > MAX_FILE:
            raise LayoutError("layout.toml passa de 64 KB")
        try:
            text = text.decode("utf-8")
        except UnicodeDecodeError:
            raise LayoutError("layout.toml não é UTF-8") from None
    if len(text.encode("utf-8")) > MAX_FILE:
        raise LayoutError("layout.toml passa de 64 KB")
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise LayoutError(f"TOML inválido: {e}") from None
    return parse(data, nome, base_dir)


def read_small(path: Path, limit: int = MAX_FILE) -> bytes:
    """Lê um arquivo regular de até `limit` bytes sem seguir link simbólico."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    except OSError as e:
        if e.errno == errno.ELOOP:
            raise LayoutError(f"{path.name} é link simbólico") from None
        raise LayoutError(f"{path.name}: {e.strerror}") from None
    with os.fdopen(fd, "rb") as fh:
        st = os.fstat(fh.fileno())
        if not stat.S_ISREG(st.st_mode):
            raise LayoutError(f"{path.name} não é arquivo comum")
        if st.st_size > limit:
            raise LayoutError(f"{path.name} passa de {limit // 1024} KB")
        data = fh.read(limit + 1)
    if len(data) > limit:
        raise LayoutError(f"{path.name} passa de {limit // 1024} KB")
    return data


def load(dir: Path) -> Layout:
    dir = Path(dir)
    if dir.is_symlink():
        raise LayoutError(f"{dir.name} é link simbólico")
    if not dir.is_dir():
        raise LayoutError(f"{dir} não é uma pasta")
    lay = parse_text(read_small(dir / "layout.toml"), dir.name, dir)
    for entry in sorted(os.scandir(dir), key=lambda e: e.name):
        if entry.is_symlink():
            raise LayoutError(f"{entry.name} é link simbólico")
        if entry.is_dir():
            raise LayoutError(f"{entry.name}: subpastas não são permitidas")
        if entry.name != "layout.toml" and not TEXT_FILE_RE.fullmatch(entry.name):
            lay.avisos.append(f"arquivo ignorado: {entry.name}")
    return lay


def _widths(layout: Layout, W: int) -> list[int]:
    cols = layout.colunas
    ws: list[int | None] = []
    for c in cols:
        if c.largura is None:
            ws.append(None)
            continue
        w = round(W * c.largura / 100)
        if c.min_cols is not None:
            w = max(w, c.min_cols)
        if c.max_cols is not None:
            w = min(w, c.max_cols)
        ws.append(w)
    free = [i for i, w in enumerate(ws) if w is None]
    rest = W - sum(w for w in ws if w is not None)
    for i in free:
        ws[i] = rest // len(free)
    ws[-1] = W - sum(ws[:-1])  # a última absorve o arredondamento
    for w in ws:
        if w < MIN_COL_W:
            raise TooSmall(f"coluna com {w} de largura; o mínimo é {MIN_COL_W}")
    return ws  # type: ignore[return-value]


def _heights(slots: tuple[Slot, ...], H: int, auto: dict[str, int]) -> list[int]:
    fixed: dict[int, int] = {}
    weighted = [i for i, s in enumerate(slots) if s.peso is not None]
    for i, s in enumerate(slots):
        if s.peso is None:
            h = auto.get(s.id, MIN_H) if s.altura == "auto" else int(s.altura or MIN_H)
            fixed[i] = max(MIN_H, int(h))
    room = H - MIN_H * len(weighted)
    if sum(fixed.values()) > room:
        for i in sorted(fixed, reverse=True):
            over = sum(fixed.values()) - room
            if over <= 0:
                break
            fixed[i] -= min(over, fixed[i] - MIN_H)
        if sum(fixed.values()) > room:
            raise TooSmall(f"a coluna precisa de {sum(fixed.values()) + H - room} linhas e tem {H}")
    hs = [0] * len(slots)
    for i, h in fixed.items():
        hs[i] = h
    left = H - sum(fixed.values())
    if not weighted:
        hs[-1] += left  # sem peso: o último cresce até o fim
        return hs
    total_w = sum(slots[i].peso for i in weighted)  # type: ignore[misc]
    R = left
    for k, i in enumerate(weighted):
        after = len(weighted) - k - 1
        if after == 0:
            hs[i] = left
            break
        h = max(MIN_H, int(R * slots[i].peso / total_w))  # type: ignore[operator]
        h = min(h, left - MIN_H * after)
        hs[i] = h
        left -= h
    return hs


def compute(layout: Layout, W: int, H: int,
            auto: dict[str, int] | None = None) -> dict[str, tuple[int, int, int, int]]:
    """Caixas (y, x, h, w) de cada painel. `H` é a altura sem a entrada."""
    auto = auto or {}
    out: dict[str, tuple[int, int, int, int]] = {}
    x = 0
    for col, w in zip(layout.colunas, _widths(layout, W)):
        y = 0
        for slot, h in zip(col.slots, _heights(col.slots, H, auto)):
            out[slot.id] = (y, x, h, w)
            y += h
        x += w
    return out


DEFAULT = Layout(
    nome="padrao",
    descricao="Layout embutido: sistema, comandos, agenda e uso à esquerda; pasta e saída à direita",
    colunas=[
        Column(36.0, (Slot("sistema", 11), Slot("comandos", "auto"), Slot("agenda", None, 1.0),
                      Slot("uso_claude", 4), Slot("uso_codex", "auto")),
               min_cols=38, max_cols=56),
        Column(None, (Slot("vault", None, 55.0), Slot("saida", None, 45.0))),
    ],
)
