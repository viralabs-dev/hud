"""Uso do plano Claude: janela de 5 horas e semanal.

Duas fontes, sem nunca tocar no token OAuth:
- o cache que o plugin claude-usage-monitor do statusline grava em
  /tmp/claude-sl-usage-<sha1(config_home)[:12]>.json (só se for seu e ninguém
  mais puder escrevê-lo; os números são validados);
- os eventos `rate_limit_event` das conversas que o próprio HUD abre.
Vale o dado mais recente.

Uso do Codex: o último `rate_limits` dos rollouts em $CODEX_HOME/sessions
(eventos `token_count`, em `payload.rate_limits`), sem tocar em auth.json.
"""

import hashlib
import json
import os
import stat
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

from hud.text import clean_line


@dataclass
class Window:
    used: float           # 0–100
    resets_at: float | None  # epoch

    @property
    def left(self) -> float:
        return max(0.0, min(100.0, 100.0 - self.used))


@dataclass
class Usage:
    five_hour: Window
    seven_day: Window
    fetched_at: float
    source: str


def config_home() -> Path:
    env = os.environ.get("CLAUDE_CONFIG_DIR")
    return Path(env) if env else Path.home() / ".claude"


def default_cache_path() -> Path:
    key = hashlib.sha1(str(config_home()).encode(), usedforsecurity=False).hexdigest()[:12]
    return Path(tempfile.gettempdir()) / f"claude-sl-usage-{key}.json"


def _pct(v) -> float | None:
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        return None
    return float(v) if 0 <= v <= 100 else None


def parse_cache(raw: str) -> Usage | None:
    try:
        d = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(d, dict):
        return None
    five, week, at = _pct(d.get("five_hour_used")), _pct(d.get("seven_day_used")), d.get("fetched_at")
    if five is None or week is None or not isinstance(at, (int, float)):
        return None

    def reset(key: str) -> float | None:
        m = d.get(key)
        return at + m * 60 if isinstance(m, (int, float)) and m >= 0 else None

    return Usage(Window(five, reset("five_hour_reset_min")),
                 Window(week, reset("seven_day_reset_min")), float(at), "statusline")


def parse_event(info: dict) -> Usage | None:
    """`rate_limit_info` de um rate_limit_event do stream-json (utilização 0–1)."""
    wins = info.get("unifiedWindows") if isinstance(info, dict) else None
    if not isinstance(wins, dict):
        return None

    def win(name: str) -> Window | None:
        w = wins.get(name)
        if not isinstance(w, dict):
            return None
        u = w.get("utilization")
        if isinstance(u, bool) or not isinstance(u, (int, float)) or not 0 <= u <= 1:
            return None
        r = w.get("resetsAt")
        return Window(u * 100, float(r) if isinstance(r, (int, float)) else None)

    five, week = win("five_hour"), win("seven_day")
    if not five or not week:
        return None
    return Usage(five, week, time.time(), "HUD")


class UsageTracker:
    def __init__(self, path: Path | None = None):
        self.path = path or default_cache_path()
        self.current: Usage | None = None
        self._mtime = 0.0

    def poll(self) -> bool:
        """Relê o cache se mudou. Devolve True quando o dado mudou."""
        try:
            fd = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW)
        except OSError:
            return False
        with os.fdopen(fd, encoding="utf-8") as f:
            st = os.fstat(f.fileno())
            if (st.st_uid != os.getuid() or st.st_mode & stat.S_IWOTH
                    or not stat.S_ISREG(st.st_mode) or st.st_size > 4096
                    or st.st_mtime == self._mtime):
                return False
            self._mtime = st.st_mtime
            got = parse_cache(f.read())
        return self.offer(got)

    def offer(self, got: Usage | None) -> bool:
        if got and (not self.current or got.fetched_at >= self.current.fetched_at):
            self.current = got
            return True
        return False


def segments(left: float, n: int = 5) -> list[str]:
    """Barra de n segmentos de 100/n %: 'full', 'part' ou 'empty' cada."""
    step = 100 / n
    out = []
    for i in range(n):
        lo = i * step
        out.append("full" if left >= lo + step else "part" if left > lo else "empty")
    return out


def level(left: float) -> str:
    return "ok" if left > 75 else "warn" if left > 25 else "crit"


def until(ts: float | None, now: float | None = None) -> str:
    if ts is None:
        return ""
    sec = max(0, int(ts - (now or time.time())))
    d, rest = divmod(sec, 86400)
    h, m = rest // 3600, rest % 3600 // 60
    if d:
        return f"{d}d{h:02d}h"
    return f"{h}h{m:02d}" if h else f"{m}min"


# --- Codex ---------------------------------------------------------------

CODEX_TAIL = 256 * 1024
CODEX_DAYS = 3


@dataclass
class CodexWindow:
    label: str
    window_minutes: int
    used: float              # 0–100
    resets_at: float | None  # epoch

    @property
    def left(self) -> float:
        return max(0.0, min(100.0, 100.0 - self.used))


@dataclass
class CodexUsage:
    windows: list
    plan: str
    fetched_at: float


def codex_home() -> Path:
    env = os.environ.get("CODEX_HOME")
    return Path(env) if env else Path.home() / ".codex"


def window_label(minutes: int) -> str:
    if minutes == 10080:
        return "semana"
    if minutes >= 1440 and minutes % 1440 == 0:
        return f"{minutes // 1440}d"
    if minutes >= 60:
        return f"{round(minutes / 60)}h"
    return f"{minutes}min"


def _num(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _codex_window(w) -> CodexWindow | None:
    if not isinstance(w, dict):
        return None
    used, mins, reset = w.get("used_percent"), w.get("window_minutes"), w.get("resets_at")
    if not _num(used) or not 0 <= used <= 100:
        return None
    if not _num(mins) or mins <= 0 or mins != int(mins):
        return None
    if reset is not None and not _num(reset):
        return None
    mins = int(mins)
    return CodexWindow(window_label(mins), mins, float(used),
                       float(reset) if reset is not None else None)


def parse_codex_rate_limits(obj: dict, fetched_at: float | None = None) -> CodexUsage | None:
    """Valida um objeto `rate_limits` do Codex. Janela presente e inválida invalida tudo."""
    if not isinstance(obj, dict):
        return None
    windows = []
    for key in ("primary", "secondary"):
        raw = obj.get(key)
        if raw is None:
            continue
        w = _codex_window(raw)
        if w is None:
            return None
        windows.append(w)
    plan = obj.get("plan_type")
    plan = clean_line(plan)[:40] if isinstance(plan, str) else ""
    return CodexUsage(windows, plan, time.time() if fetched_at is None else fetched_at)


def _find_rate_limits(o, depth: int = 0):
    """Primeiro `rate_limits` (dict) em qualquer nível, até 6 de profundidade."""
    if depth > 6:
        return None
    if isinstance(o, dict):
        rl = o.get("rate_limits")
        if isinstance(rl, dict):
            return rl
        children = o.values()
    elif isinstance(o, list):
        children = o
    else:
        return None
    for v in children:
        if isinstance(v, (dict, list)):
            got = _find_rate_limits(v, depth + 1)
            if got is not None:
                return got
    return None


def _sorted_subdirs(path: str, width: int) -> list[str]:
    try:
        with os.scandir(path) as it:
            names = [e.name for e in it
                     if len(e.name) == width and e.name.isdigit() and e.is_dir(follow_symlinks=False)]
    except OSError:
        return []
    return sorted(names, reverse=True)


def latest_rollout(sessions_dir: Path, days: int = CODEX_DAYS) -> Path | None:
    """O rollout-*.jsonl de maior mtime nas `days` pastas AAAA/MM/DD mais recentes."""
    root = str(sessions_dir)
    dated: list[str] = []
    for y in _sorted_subdirs(root, 4):
        for m in _sorted_subdirs(os.path.join(root, y), 2):
            for d in _sorted_subdirs(os.path.join(root, y, m), 2):
                dated.append(os.path.join(root, y, m, d))
                if len(dated) >= days:
                    break
            if len(dated) >= days:
                break
        if len(dated) >= days:
            break
    best, best_mtime = None, -1.0
    for d in dated:
        try:
            with os.scandir(d) as it:
                for e in it:
                    if not (e.name.startswith("rollout-") and e.name.endswith(".jsonl")):
                        continue
                    if not e.is_file(follow_symlinks=False):
                        continue
                    mt = e.stat(follow_symlinks=False).st_mtime
                    if mt > best_mtime:
                        best, best_mtime = e.path, mt
        except OSError:
            continue
    return Path(best) if best else None


def read_codex_rollout(path: Path, tail: int = CODEX_TAIL) -> CodexUsage | None:
    """Último `rate_limits` válido nos últimos `tail` bytes de um rollout seguro."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    except OSError:
        return None
    with os.fdopen(fd, "rb") as f:
        st = os.fstat(f.fileno())
        if (st.st_uid != os.getuid() or st.st_mode & stat.S_IWOTH
                or not stat.S_ISREG(st.st_mode)):
            return None
        start = max(0, st.st_size - tail)
        f.seek(start)
        data = f.read(tail)
    lines = data.split(b"\n")
    if start > 0:
        lines = lines[1:]  # primeira linha cortada no meio
    for line in reversed(lines):
        if b'"rate_limits"' not in line:
            continue
        try:
            obj = json.loads(line)
        except ValueError:
            continue
        got = parse_codex_rate_limits(_find_rate_limits(obj), st.st_mtime)
        if got is not None:
            return got
    return None


class CodexUsageTracker:
    def __init__(self, sessions_dir: Path | None = None):
        self.sessions_dir = sessions_dir or codex_home() / "sessions"
        self.current: CodexUsage | None = None
        self._key: tuple | None = None

    def poll(self) -> bool:
        """Relê o rollout mais recente se ele (ou seu mtime) mudou. True quando o dado mudou."""
        path = latest_rollout(self.sessions_dir)
        if path is None:
            return False
        try:
            st = os.lstat(path)
        except OSError:
            return False
        key = (str(path), st.st_mtime, st.st_size)
        if key == self._key:
            return False
        self._key = key
        got = read_codex_rollout(path)
        if got is None or got == self.current:
            return False
        self.current = got
        return True
