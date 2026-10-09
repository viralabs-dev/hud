"""Indicadores do sistema (CPU, memória, disco, rede, bateria...), só leitura.

A API (Metrics, Sample, Disk) é a mesma em todo sistema; quem lê os números é
um backend por plataforma, escolhido uma vez na importação por sys.platform:

- linux (e o resto): metrics_linux, /proc e /sys;
- darwin: metrics_macos, ctypes na libSystem (Mach, sysctl, getifaddrs) e
  pmset -g batt para a bateria;
- win32: metrics_windows, ctypes em kernel32 e iphlpapi.

Os imports são estáticos (dentro de if) para o PyInstaller enxergar os três.
"""

import os
import socket
import sys
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path

PLATFORM = sys.platform

if PLATFORM == "darwin":
    from .metrics_macos import Backend as _Backend
elif PLATFORM == "win32":
    from .metrics_windows import Backend as _Backend
else:
    from .metrics_linux import Backend as _Backend


@dataclass
class Disk:
    label: str
    used: int
    total: int

    @property
    def pct(self) -> float:
        return 100.0 * self.used / self.total if self.total else 0.0


@dataclass
class Sample:
    cpu: float = 0.0
    cores: int = 1
    mem_used: int = 0
    mem_total: int = 0
    swap_used: int = 0
    swap_total: int = 0
    # Sem load average (Windows) ou sem contadores de rede, os números ficam
    # em zero e a flag correspondente em False: a UI mostra "—" no lugar.
    load: tuple[float, float, float] = (0.0, 0.0, 0.0)
    load_ok: bool = False
    procs: int = 0
    rx_rate: float = 0.0
    tx_rate: float = 0.0
    net_ok: bool = False
    uptime: float = 0.0
    temp: float | None = None
    # (pct, status) com os status do Linux: "Charging", "Discharging", "Full",
    # "Not charging" (os backends de macOS e Windows traduzem para eles).
    battery: tuple[int, str] | None = None
    disks: list[Disk] = field(default_factory=list)


def _safe(fn, default):
    """Um indicador que falha vira o padrão; a UI nunca recebe exceção."""
    try:
        return fn()
    except Exception:
        return default


class Metrics:
    def __init__(self, extra_paths: list[Path] | None = None, history: int = 120,
                 backend=None):
        self.hostname = socket.gethostname()
        self.platform = PLATFORM
        self.cpu_hist: deque[float] = deque(maxlen=history)
        self.net_hist: deque[float] = deque(maxlen=history)
        self._b = backend if backend is not None else _Backend()
        self._cpu_prev: tuple[int, int] | None = None
        self._net_prev: tuple[int, int, float] | None = None
        self._net_rate: tuple[float, float] = (0.0, 0.0)
        self._roots = _safe(self._b.root_paths, [])
        self._paths = [*self._roots, Path.home(), *(extra_paths or [])]
        self.last = Sample(cores=os.cpu_count() or 1)

    def sample(self) -> Sample:
        b = self._b
        s = Sample(cores=os.cpu_count() or 1)
        s.cpu = self._cpu()
        mem = _safe(b.memory, None)
        if mem:
            s.mem_used, s.mem_total, s.swap_used, s.swap_total = mem
        load, s.procs = _safe(b.load_procs, (None, 0))
        if load is not None:
            s.load, s.load_ok = load, True
        net = self._net()
        if net is not None:
            (s.rx_rate, s.tx_rate), s.net_ok = net, True
        s.uptime = _safe(b.uptime, 0.0)
        s.temp = _safe(b.temp, None)
        s.battery = _safe(b.battery, None)
        s.disks = self._disks()
        self.cpu_hist.append(s.cpu)
        self.net_hist.append(s.rx_rate + s.tx_rate)
        self.last = s
        return s

    def _cpu(self) -> float:
        cur = _safe(self._b.cpu_times, None)
        if cur is None:
            return 0.0
        idle, total = cur
        prev, self._cpu_prev = self._cpu_prev, (idle, total)
        if not prev or total == prev[1]:
            return 0.0
        return max(0.0, min(100.0, 100.0 * (1 - (idle - prev[0]) / (total - prev[1]))))

    def _net(self) -> tuple[float, float] | None:
        cur = _safe(self._b.net_bytes, None)
        if cur is None:
            self._net_prev = None
            return None
        rx, tx, now = cur
        prev = self._net_prev
        if prev and now == prev[2]:
            # Leitura em cache (fallback por subprocess no macOS): repete a
            # última taxa em vez de mostrar zero e depois o dobro.
            return self._net_rate
        self._net_prev = (rx, tx, now)
        if not prev or now <= prev[2]:
            rate = (0.0, 0.0)
        else:
            dt = now - prev[2]
            rate = (max(0.0, (rx - prev[0]) / dt), max(0.0, (tx - prev[1]) / dt))
        self._net_rate = rate
        return rate

    def _label(self, p: Path) -> str:
        if p in self._roots:
            return _safe(lambda: self._b.root_label(p), str(p))
        return "~" if p == Path.home() else (p.name or str(p))

    def _disks(self) -> list[Disk]:
        seen, out = set(), []
        for p in self._paths:
            r = _safe(lambda: self._b.disk_usage(p), None)
            if r is None:
                continue
            key, used, total = r
            if key in seen:
                continue
            seen.add(key)
            if used is None or total is None:
                continue
            out.append(Disk(self._label(p), used, total))
        return out


def human_bytes(n: float, suffix: str = "") -> str:
    for unit in ("B", "K", "M", "G", "T"):
        if abs(n) < 1024 or unit == "T":
            return f"{n:.0f}{unit}{suffix}" if unit == "B" else f"{n:.1f}{unit}{suffix}"
        n /= 1024
    return f"{n:.1f}P{suffix}"


def human_duration(sec: float) -> str:
    sec = int(sec)
    d, sec = divmod(sec, 86400)
    h, sec = divmod(sec, 3600)
    m = sec // 60
    if d:
        return f"{d}d {h}h"
    if h:
        return f"{h}h {m}min"
    return f"{m}min"
