"""Backend Linux dos indicadores: lê /proc e /sys (só leitura).

É o código que vivia em metrics.py, movido sem mudar o resultado. O backend
devolve números crus; quem monta o Sample e calcula as taxas é metrics.Metrics.
"""

import os
import time
from pathlib import Path


def _read(path: str) -> str:
    try:
        with open(path) as f:
            return f.read()
    except OSError:
        return ""


class Backend:
    name = "linux"

    def root_paths(self) -> list[Path]:
        return [Path("/")]

    def root_label(self, p: Path) -> str:
        return "/"

    def cpu_times(self) -> tuple[int, int] | None:
        """(idle, total) acumulados; metrics calcula o uso pela diferença."""
        line = _read("/proc/stat").split("\n", 1)[0].split()
        if not line or line[0] != "cpu":
            return None
        vals = list(map(int, line[1:]))
        idle = vals[3] + (vals[4] if len(vals) > 4 else 0)
        return idle, sum(vals[:8])

    def memory(self) -> tuple[int, int, int, int]:
        """(mem_used, mem_total, swap_used, swap_total) em bytes."""
        info = {}
        for line in _read("/proc/meminfo").splitlines():
            k, _, v = line.partition(":")
            info[k] = int(v.split()[0]) * 1024 if v.split() else 0
        total = info.get("MemTotal", 0)
        swap = info.get("SwapTotal", 0)
        return (total - info.get("MemAvailable", 0), total,
                swap - info.get("SwapFree", 0), swap)

    def load_procs(self) -> tuple[tuple[float, float, float] | None, int]:
        parts = _read("/proc/loadavg").split()
        if len(parts) >= 4:
            return (float(parts[0]), float(parts[1]), float(parts[2])), int(parts[3].split("/")[1])
        return None, 0

    def net_bytes(self) -> tuple[int, int, float] | None:
        """(rx, tx, instante) acumulados, sem loopback nem pontes/veths de contêiner."""
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
        return rx, tx, time.monotonic()

    def uptime(self) -> float:
        up = _read("/proc/uptime").split()
        return float(up[0]) if up else 0.0

    def temp(self) -> float | None:
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

    def battery(self) -> tuple[int, str] | None:
        for d in Path("/sys/class/power_supply").glob("BAT*"):
            cap = _read(str(d / "capacity")).strip()
            if cap.isdigit():
                return int(cap), _read(str(d / "status")).strip()
        return None

    def disk_usage(self, p: Path) -> tuple[object, int | None, int | None] | None:
        """(chave do volume, usado, total). None: caminho inacessível.

        Usado/total None: o volume existe (a chave conta como vista) mas o
        statvfs falhou, como no código original.
        """
        try:
            key = os.stat(p).st_dev
        except OSError:
            return None
        try:
            v = os.statvfs(p)
        except OSError:
            return key, None, None
        total = v.f_blocks * v.f_frsize
        return key, total - v.f_bavail * v.f_frsize, total
