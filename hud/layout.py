"""Layout da tela: colunas e painéis, lidos de `custom/<nome>/layout.toml`.

O arquivo diz onde cada painel fica e quais painéis próprios existem. Ele é
validado por inteiro antes de qualquer uso: um layout errado nunca chega à
tela pela metade. `compute` transforma o layout em caixas (y, x, h, w).
"""

import errno
import fnmatch
import json
import os
import re
import stat
import tomllib
from dataclasses import dataclass, field, replace
from pathlib import Path

from . import plataforma as plat
from .config import ConfigError, build_command
from .text import clean_line

NAME_RE = re.compile(r"[a-z0-9][a-z0-9_-]{0,39}")
MAX_FILE = 64 * 1024
MIN_H = 3
MIN_COL_W = 30
MAX_COLS = 4
MAX_SLOTS = 8

# id → (altura, peso). Altura "auto" vem calculada pela UI em `compute(auto=)`.
BUILTIN: dict[str, tuple[int | str | None, float | None]] = {
    "sistema": (11, None),
    "comandos": ("auto", None),
    "agenda": (None, 1.0),
    "uso_claude": (4, None),
    "uso_codex": ("auto", None),
    "vault": (None, 1.0),
    "saida": (None, 1.0),
    "agentes": (None, 1.0),
}
ALIASES = {"pasta": "vault"}
# Blocos: o que cada painel embutido aceita em `mostrar` (na ordem em que o
# `dumps` escreve). Quem não está aqui só aceita `titulo` (agenda também `dias`).
MOSTRAR: dict[str, tuple[str, ...]] = {
    "sistema": ("cpu", "historico", "mem", "swap", "disco", "load", "rede", "sensores"),
    "vault": ("resumo", "quadros", "atencao", "recentes"),
    "agentes": ("processos", "sessoes"),
}
TIPOS = ("texto", "arquivo", "comando")

DENIED_DIRS = ("~/.ssh", "~/.gnupg", "~/.aws", "~/.config/gh", "~/.kube",
               "~/.password-store", "~/.local/share/keyrings")
if plat.WINDOWS:  # cofre de credenciais, chaves DPAPI, gh e navegadores
    DENIED_DIRS += ("~/AppData/Roaming/Microsoft/Credentials", "~/AppData/Local/Microsoft/Credentials",
                    "~/AppData/Roaming/Microsoft/Protect", "~/AppData/Roaming/GitHub CLI",
                    "~/AppData/Local/Google/Chrome/User Data", "~/AppData/Roaming/Mozilla")
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


@dataclass(frozen=True)
class BlocoSpec:
    """Opções de um painel embutido, vindas de `[[bloco]]`."""
    id: str
    titulo: str = ""                       # vazio = o título padrão
    mostrar: frozenset[str] | None = None  # None = tudo
    dias: int | None = None                # só agenda


@dataclass
class Layout:
    nome: str
    descricao: str = ""
    autor: str = ""
    colunas: list[Column] = field(default_factory=list)
    paineis: dict[str, PanelSpec] = field(default_factory=dict)
    avisos: list[str] = field(default_factory=list)
    blocos: dict[str, BlocoSpec] = field(default_factory=dict)

    def ids(self) -> list[str]:
        return [s.id for c in self.colunas for s in c.slots]


def _num(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _int(v) -> bool:
    return isinstance(v, int) and not isinstance(v, bool)


def _within(p: str, root: str) -> bool:
    # normcase: no Windows, ~/.SSH é a mesma pasta que ~/.ssh (no POSIX não muda nada).
    p, root = os.path.normcase(p), os.path.normcase(root)
    return p == root or p.startswith(root.rstrip(os.sep) + os.sep)


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


def _bloco(raw) -> BlocoSpec:
    if not isinstance(raw, dict):
        raise LayoutError("[[bloco]] precisa ser uma tabela")
    bid = raw.get("id")
    if not isinstance(bid, str) or not bid:
        raise LayoutError("[[bloco]] sem 'id'")
    bid = ALIASES.get(bid, bid)
    if bid not in BUILTIN:
        raise LayoutError(f"[[bloco]] '{bid}': não é painel embutido "
                          f"({', '.join(BUILTIN)}); painel próprio se ajusta em [[painel]]")
    allowed = {"id", "titulo"}
    if bid in MOSTRAR:
        allowed.add("mostrar")
    if bid == "agenda":
        allowed.add("dias")
    extra = set(raw) - allowed
    if extra:
        raise LayoutError(f"bloco '{bid}': chave desconhecida: {', '.join(sorted(extra))} "
                          f"(aceita {', '.join(sorted(allowed))})")
    titulo = raw.get("titulo", "")
    if not isinstance(titulo, str):
        raise LayoutError(f"bloco '{bid}': 'titulo' precisa ser texto")
    titulo = clean_line(titulo).strip()
    if len(titulo) > 30:
        raise LayoutError(f"bloco '{bid}': 'titulo' tem até 30 caracteres")
    mostrar = None
    if "mostrar" in raw:
        valores = raw["mostrar"]
        if not isinstance(valores, list) or not valores or not all(isinstance(v, str) for v in valores):
            raise LayoutError(f"bloco '{bid}': 'mostrar' é uma lista não vazia de textos")
        bad = [v for v in valores if v not in MOSTRAR[bid]]
        if bad:
            raise LayoutError(f"bloco '{bid}': valor desconhecido em 'mostrar': {', '.join(bad)} "
                              f"(aceita {', '.join(MOSTRAR[bid])})")
        if len(set(valores)) != len(valores):
            raise LayoutError(f"bloco '{bid}': valor repetido em 'mostrar'")
        mostrar = frozenset(valores)
    dias = raw.get("dias")
    if dias is not None and not (_int(dias) and 1 <= dias <= 30):
        raise LayoutError(f"bloco '{bid}': 'dias' é um inteiro de 1 a 30")
    return BlocoSpec(bid, titulo, mostrar, dias)


def _meta(data: dict, key: str, cut: int) -> str:
    v = data.get(key, "")
    if not isinstance(v, str):
        raise LayoutError(f"'{key}' precisa ser texto")
    return clean_line(v).strip()[:cut]


def parse(data: dict, nome: str, base_dir: Path | None = None) -> Layout:
    if not isinstance(data, dict):
        raise LayoutError("layout precisa ser uma tabela TOML")
    avisos: list[str] = []
    extra = set(data) - {"nome", "descricao", "autor", "coluna", "painel", "bloco"}
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

    raw_blocos = data.get("bloco", [])
    if not isinstance(raw_blocos, list):
        raise LayoutError("[[bloco]] precisa ser uma lista de tabelas")
    for raw in raw_blocos:
        b = _bloco(raw)
        if b.id in lay.blocos:
            raise LayoutError(f"[[bloco]] '{b.id}' declarado duas vezes")
        lay.blocos[b.id] = b

    cols = data.get("coluna")
    if not isinstance(cols, list) or not 1 <= len(cols) <= MAX_COLS:
        raise LayoutError(f"o layout precisa de 1 a {MAX_COLS} [[coluna]]")
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
        if not isinstance(raw_slots, list) or not 1 <= len(raw_slots) <= MAX_SLOTS:
            raise LayoutError(f"coluna {ci}: de 1 a {MAX_SLOTS} painéis")
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
    for bid in lay.blocos:
        if bid not in seen:
            avisos.append(f"[[bloco]] '{bid}' não está em nenhuma coluna")
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
        fd = plat.open_nofollow(path, os.O_RDONLY | plat.O_NONBLOCK | plat.O_CLOEXEC)
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
    if plat.is_link(dir):
        raise LayoutError(f"{dir.name} é link simbólico")
    if not dir.is_dir():
        raise LayoutError(f"{dir} não é uma pasta")
    lay = parse_text(read_small(dir / "layout.toml"), dir.name, dir)
    for entry in sorted(os.scandir(dir), key=lambda e: e.name):
        if entry.is_symlink() or plat.is_link(entry.path):
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


# ---------------------------------------------------------------------------
# Operações sobre o layout (arraste de painéis) e escrita em TOML.
# Todas são puras: devolvem um Layout novo e nunca mexem no recebido.
# ---------------------------------------------------------------------------

def check(layout: Layout) -> None:
    """Levanta LayoutError se o layout não passaria pelo `parse`."""
    cols = layout.colunas
    if not 1 <= len(cols) <= MAX_COLS:
        raise LayoutError(f"o layout precisa de 1 a {MAX_COLS} colunas")
    seen: set[str] = set()
    declared = []
    for ci, col in enumerate(cols, 1):
        if not 1 <= len(col.slots) <= MAX_SLOTS:
            raise LayoutError(f"coluna {ci}: de 1 a {MAX_SLOTS} painéis")
        if col.largura is not None:
            if not 15 <= col.largura <= 85:
                raise LayoutError(f"coluna {ci}: 'largura' de 15 a 85 (%)")
            declared.append(col.largura)
        for s in col.slots:
            if s.id not in BUILTIN and s.id not in layout.paineis:
                raise LayoutError(f"'{s.id}' não é painel embutido nem está em [[painel]]")
            if s.id in seen:
                raise LayoutError(f"'{s.id}' aparece mais de uma vez")
            seen.add(s.id)
    total = sum(declared)
    if len(declared) < len(cols) and total > 90:
        raise LayoutError("larguras somam mais de 90% e ainda há coluna sem largura")
    if total > 100:
        raise LayoutError("larguras somam mais de 100%")
    if "saida" not in seen:
        raise LayoutError("o painel 'saida' é obrigatório")


def _with_cols(layout: Layout, cols: list[Column]) -> Layout:
    new = Layout(nome=layout.nome, descricao=layout.descricao, autor=layout.autor,
                 colunas=list(cols), paineis=dict(layout.paineis), avisos=list(layout.avisos),
                 blocos=dict(layout.blocos))
    check(new)
    return new


def _where(layout: Layout, pid: str) -> tuple[int, int]:
    pid = ALIASES.get(pid, pid)
    for ci, col in enumerate(layout.colunas):
        for si, s in enumerate(col.slots):
            if s.id == pid:
                return ci, si
    raise LayoutError(f"'{pid}' não está no layout")


def swap(layout: Layout, a: str, b: str) -> Layout:
    """Troca as posições de dois painéis; cada um leva a sua altura/peso."""
    (ca, sa), (cb, sb) = _where(layout, a), _where(layout, b)
    slots = [list(c.slots) for c in layout.colunas]
    slots[ca][sa], slots[cb][sb] = layout.colunas[cb].slots[sb], layout.colunas[ca].slots[sa]
    return _with_cols(layout, [replace(c, slots=tuple(s)) for c, s in zip(layout.colunas, slots)])


def _pct(v: float) -> float:
    return float(min(85.0, max(15.0, round(v, 1))))


def move(layout: Layout, pid: str, col: int, pos: int) -> Layout:
    """Põe `pid` na coluna `col` (0-based), na posição `pos`.

    `col` conta as colunas do layout atual; `pos` conta os painéis da coluna
    de destino já sem o painel arrastado (e é limitado ao tamanho dela).
    `col == len(colunas)` cria uma coluna nova à direita. A coluna que fica
    vazia some.

    Larguras: a coluna nova nasce sem `largura` (divide o resto com as outras
    sem largura); se as larguras declaradas passarem do que deixa 1/n da tela
    para ela (ou de 90%), encolhem todas na mesma proporção. Quando some uma
    coluna e não sobra nenhuma sem largura, a última passa a ficar com o resto
    e as demais crescem na mesma proporção, mantendo a relação entre elas.
    """
    ci, si = _where(layout, pid)
    n = len(layout.colunas)
    if not _int(col) or not 0 <= col <= n:
        raise LayoutError(f"coluna {col} não existe (de 0 a {n})")
    if not _int(pos):
        raise LayoutError("a posição precisa ser um inteiro")
    src = layout.colunas[ci]
    slot = src.slots[si]
    cols: list[Column | None] = list(layout.colunas)
    cols[ci] = replace(src, slots=src.slots[:si] + src.slots[si + 1:])
    if col == n:
        if n >= MAX_COLS and len(src.slots) > 1:
            raise LayoutError(f"no máximo {MAX_COLS} colunas")
        cols.append(Column(None, (slot,)))
    else:
        target = cols[col]
        assert target is not None
        p = max(0, min(pos, len(target.slots)))
        cols[col] = replace(target, slots=target.slots[:p] + (slot,) + target.slots[p:])
    removed = not cols[ci].slots  # type: ignore[union-attr]
    if removed:
        cols[ci] = None
    live = [c for c in cols if c is not None]
    if col == n:
        declared = sum(c.largura for c in live if c.largura is not None)
        limit = min(90.0, 100.0 * (len(live) - 1) / len(live))
        if declared > limit:
            k = limit / declared
            live = [replace(c, largura=_pct(c.largura * k)) if c.largura is not None else c
                    for c in live]
    if removed and all(c.largura is not None for c in live):
        if len(live) == 1:
            live = [replace(live[0], largura=None)]
        else:
            total = sum(c.largura for c in live)  # type: ignore[misc]
            live = [replace(c, largura=_pct(c.largura * 100.0 / total)) for c in live[:-1]] \
                + [replace(live[-1], largura=None)]
    return _with_cols(layout, live)


def _str(v: str) -> str:
    return json.dumps(v, ensure_ascii=False)  # string básica do TOML (mesmos escapes)


def _numstr(v: float) -> str:
    return str(int(v)) if float(v).is_integer() else repr(float(v))


def _slot_toml(s: Slot) -> str:
    if (s.altura, s.peso) == BUILTIN.get(s.id, (None, 1.0)):
        return _str(s.id)
    if s.peso is not None:
        return f"{{ id = {_str(s.id)}, peso = {_numstr(s.peso)} }}"
    if _int(s.altura):
        return f"{{ id = {_str(s.id)}, altura = {s.altura} }}"
    return _str(s.id)  # "auto" só existe como padrão de painel embutido


def dumps(layout: Layout) -> str:
    """O layout inteiro em TOML; `parse_text(dumps(L), L.nome)` reproduz L.

    Fica de fora só o que o TOML não expressa: `min_cols`/`max_cols` (só do
    DEFAULT) e os avisos.
    """
    check(layout)
    out = [f"nome = {_str(layout.nome)}"]
    if layout.descricao:
        out.append(f"descricao = {_str(layout.descricao)}")
    if layout.autor:
        out.append(f"autor = {_str(layout.autor)}")
    for col in layout.colunas:
        out += ["", "[[coluna]]"]
        if col.largura is not None:
            out.append(f"largura = {_numstr(col.largura)}")
        out.append("paineis = [")
        out += [f"  {_slot_toml(s)}," for s in col.slots]
        out.append("]")
    for p in layout.paineis.values():
        out += ["", "[[painel]]", f"id = {_str(p.id)}", f"titulo = {_str(p.titulo)}",
                f"tipo = {_str(p.tipo)}"]
        if p.tipo == "texto":
            out.append(f"arquivo = {_str(p.arquivo or '')}")
        elif p.tipo == "arquivo":
            out += [f"caminho = {_str(p.caminho or '')}", f"linhas = {p.linhas}"]
        else:
            out += [f"argv = [{', '.join(_str(a) for a in p.argv)}]",
                    f"intervalo = {_numstr(p.intervalo)}", f"timeout = {_numstr(p.timeout)}"]
    for b in layout.blocos.values():
        out += ["", "[[bloco]]", f"id = {_str(b.id)}"]
        if b.titulo:
            out.append(f"titulo = {_str(b.titulo)}")
        if b.mostrar is not None:
            vals = [v for v in MOSTRAR.get(b.id, ()) if v in b.mostrar]
            out.append(f"mostrar = [{', '.join(_str(v) for v in vals)}]")
        if b.dias is not None:
            out.append(f"dias = {b.dias}")
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------
# Geometria para mouse e foco. `rects` é o que `compute` devolve:
# {id: (y, x, h, w)}, e (row, col) são células nessa mesma área.
# ---------------------------------------------------------------------------

Rects = dict[str, tuple[int, int, int, int]]
DIRECTIONS = ("up", "down", "left", "right")


def panel_at(rects: Rects, row: int, col: int) -> str | None:
    """O painel sob a célula (row, col)."""
    for pid, (y, x, h, w) in rects.items():
        if y <= row < y + h and x <= col < x + w:
            return pid
    return None


def on_title(rects: Rects, row: int, col: int) -> str | None:
    """O painel cuja borda de cima (a linha do título) passa por (row, col)."""
    for pid, (y, x, _h, w) in rects.items():
        if row == y and x <= col < x + w:
            return pid
    return None


def neighbor(rects: Rects, pid: str, direction: str) -> str | None:
    """O vizinho em `direction`: primeiro quem se sobrepõe na outra dimensão,
    depois a menor distância, a maior sobreposição e o centro mais próximo."""
    if direction not in DIRECTIONS:
        raise ValueError(f"direção inválida: {direction!r}")
    pid = ALIASES.get(pid, pid)
    if pid not in rects:
        return None
    y0, x0, h0, w0 = rects[pid]
    best: tuple | None = None
    for other, (y, x, h, w) in rects.items():
        if other == pid:
            continue
        if direction == "up":
            gap = y0 - (y + h)
        elif direction == "down":
            gap = y - (y0 + h0)
        elif direction == "left":
            gap = x0 - (x + w)
        else:
            gap = x - (x0 + w0)
        if gap < 0:
            continue
        if direction in ("up", "down"):
            overlap = min(x + w, x0 + w0) - max(x, x0)
            center = abs((2 * x + w) - (2 * x0 + w0))
        else:
            overlap = min(y + h, y0 + h0) - max(y, y0)
            center = abs((2 * y + h) - (2 * y0 + h0))
        key = (overlap <= 0, gap, -overlap, center, other)
        if best is None or key < best:
            best = key
    return best[-1] if best else None


def _shape(layout: Layout) -> list:
    return [(c.largura, c.slots) for c in layout.colunas]


def drop_target(layout: Layout, rects: Rects, row: int, col: int,
                dragged: str) -> tuple | None:
    """O que acontece ao soltar `dragged` em (row, col).

    - última coluna de células da tela, com menos de MAX_COLS colunas:
      ("move", len(colunas), 0), coluna nova à direita;
    - faixa de cima de um painel (a linha do título, ou 1/4 da altura):
      ("move", coluna, posição) para entrar antes dele;
    - faixa de baixo (a última linha, ou 1/4 da altura): ("move", ...) depois dele;
    - meio de outro painel: ("swap", outro).
    Devolve None fora dos painéis, no meio do próprio painel, ou quando o
    resultado seria igual ao layout atual ou inválido. Passar a tupla para
    `move(layout, dragged, *t[1:])` / `swap(layout, dragged, t[1])` aplica.
    """
    dragged = ALIASES.get(dragged, dragged)
    target = panel_at(rects, row, col)
    if target is None or dragged not in rects:
        return None
    y, x, h, w = rects[target]
    right_edge = max(rx + rw for (_, rx, _, rw) in rects.values())
    band = max(1, h // 4)
    action: tuple
    if col == right_edge - 1 and len(layout.colunas) < MAX_COLS:
        action = ("move", len(layout.colunas), 0)
    elif row < y + band or row >= y + h - band:
        if target == dragged:
            return None
        ci, _ = _where(layout, target)
        rest = [s.id for s in layout.colunas[ci].slots if s.id != dragged]
        action = ("move", ci, rest.index(target) + (0 if row < y + band else 1))
    elif target == dragged:
        return None
    else:
        action = ("swap", target)
    try:
        new = move(layout, dragged, action[1], action[2]) if action[0] == "move" \
            else swap(layout, dragged, action[1])
    except LayoutError:
        return None
    return None if _shape(new) == _shape(layout) else action
