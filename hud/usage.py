"""Uso do plano Claude: janela de 5 horas e semanal.

Duas fontes, sem nunca tocar no token OAuth:
- o cache que o plugin claude-usage-monitor do statusline grava em
  /tmp/claude-sl-usage-<sha1(config_home)[:12]>.json (só se for seu e ninguém
  mais puder escrevê-lo; os números são validados);
- os eventos `rate_limit_event` das conversas que o próprio HUD abre.
Vale o dado mais recente.
"""

import hashlib
import json
import os
import stat
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


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
