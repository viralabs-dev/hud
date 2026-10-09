"""Indicadores do sistema lidos direto de /proc e /sys (só leitura)."""

import os
import socket
import time
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path


def _read(path: str) -> str:
    try:
        with open(path) as f:
            return f.read()
    except OSError:
        return ""


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
    load: tuple[float, float, float] = (0.0, 0.0, 0.0)
    procs: int = 0
    rx_rate: float = 0.0
    tx_rate: float = 0.0
    uptime: float = 0.0
    temp: float | None = None
    battery: tuple[int, str] | None = None
    disks: list[Disk] = field(default_factory=list)


class Metrics:
    def __init__(self, extra_paths: list[Path] | None = None, history: int = 120):
        self.hostname = socket.gethostname()
        self.cpu_hist: deque[float] = deque(maxlen=history)
        self.net_hist: deque[float] = deque(maxlen=history)
        self._cpu_prev: tuple[int, int] | None = None
        self._net_prev: tuple[int, int, float] | None = None
        self._paths = [Path("/"), Path.home(), *(extra_paths or [])]
        self.last = Sample(cores=os.cpu_count() or 1)

    def sample(self) -> Sample:
        s = Sample(cores=os.cpu_count() or 1)
        s.cpu = self._cpu()
        self._mem(s)
        parts = _read("/proc/loadavg").split()
        if len(parts) >= 4:
            s.load = (float(parts[0]), float(parts[1]), float(parts[2]))
            s.procs = int(parts[3].split("/")[1])
        s.rx_rate, s.tx_rate = self._net()
        up = _read("/proc/uptime").split()
        s.uptime = float(up[0]) if up else 0.0
        s.temp = self._temp()
        s.battery = self._battery()
        s.disks = self._disks()
        self.cpu_hist.append(s.cpu)
        self.net_hist.append(s.rx_rate + s.tx_rate)
        self.last = s
        return s

    def _cpu(self) -> float:
        line = _read("/proc/stat").split("\n", 1)[0].split()
        if not line or line[0] != "cpu":
            return 0.0
        vals = list(map(int, line[1:]))
        idle = vals[3] + (vals[4] if len(vals) > 4 else 0)
        total = sum(vals[:8])
        prev, self._cpu_prev = self._cpu_prev, (idle, total)
        if not prev or total == prev[1]:
            return 0.0
        return max(0.0, min(100.0, 100.0 * (1 - (idle - prev[0]) / (total - prev[1]))))

    def _mem(self, s: Sample) -> None:
        info = {}
        for line in _read("/proc/meminfo").splitlines():
            k, _, v = line.partition(":")
            info[k] = int(v.split()[0]) * 1024 if v.split() else 0
        s.mem_total = info.get("MemTotal", 0)
        s.mem_used = s.mem_total - info.get("MemAvailable", 0)
        s.swap_total = info.get("SwapTotal", 0)
        s.swap_used = s.swap_total - info.get("SwapFree", 0)

    def _net(self) -> tuple[float, float]:
        rx = tx = 0
        for line in _read("/proc/net/dev").splitlines()[2:]:
            name, _, rest = line.partition(":")
            name = name.strip()
            if name == "lo" or name.startswith(("veth", "docker", "br-", "virbr")):
                continue
            f = rest.split()
            if len(f) >= 9:
                rx += int(f[0])
                tx += int(f[8])
        now = time.monotonic()
        prev, self._net_prev = self._net_prev, (rx, tx, now)
        if not prev or now <= prev[2]:
            return 0.0, 0.0
        dt = now - prev[2]
        return max(0.0, (rx - prev[0]) / dt), max(0.0, (tx - prev[1]) / dt)

    def _temp(self) -> float | None:
        best = None
        base = Path("/sys/class/hwmon")
        try:
            dirs = list(base.iterdir())
        except OSError:
            return None
        preferred = ("coretemp", "k10temp", "zenpower", "cpu_thermal", "acpitz")
        for want in preferred:
            for d in dirs:
                if _read(str(d / "name")).strip() != want:
                    continue
                for f in d.glob("temp*_input"):
                    try:
                        v = int(_read(str(f)).strip()) / 1000
                    except ValueError:
                        continue
                    if 0 < v < 150:
                        best = v if best is None else max(best, v)
            if best is not None:
                return best
        return None

    def _battery(self) -> tuple[int, str] | None:
        for d in Path("/sys/class/power_supply").glob("BAT*"):
            cap = _read(str(d / "capacity")).strip()
            if cap.isdigit():
                return int(cap), _read(str(d / "status")).strip()
        return None

    def _disks(self) -> list[Disk]:
        seen, out = set(), []
        for p in self._paths:
            try:
                st = os.stat(p)
                if st.st_dev in seen:
                    continue
                seen.add(st.st_dev)
                v = os.statvfs(p)
            except OSError:
                continue
            total = v.f_blocks * v.f_frsize
            used = total - v.f_bavail * v.f_frsize
            label = "/" if str(p) == "/" else ("~" if p == Path.home() else p.name)
            out.append(Disk(label, used, total))
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
