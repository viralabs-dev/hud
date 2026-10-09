"""Backend macOS dos indicadores, sem dependência externa e sem sudo.

Escolhas:

- CPU: host_statistics(HOST_CPU_LOAD_INFO) via ctypes na libSystem. Custa
  microssegundos; `top -l 1` leva ~1 s e não cabe no refresh de 1 s.
- Memória: hw.memsize (sysctlbyname) + host_statistics64(HOST_VM_INFO64).
  "Usada" segue o Monitor de Atividade: anônimas - purgáveis + wired +
  ocupadas pelo compressor. Swap: sysctlbyname("vm.swapusage").
- Load: os.getloadavg(). Processos: proc_listallpids (libproc, na libSystem).
- Rede: getifaddrs, somando ifi_ibytes/ifi_obytes das entradas AF_LINK, sem
  lo0 e sem túneis/interfaces virtuais (utun, awdl, llw, bridge, gif, stf,
  anpi), que repetiriam o tráfego da interface física. Os contadores do
  if_data são de 32 bits: a volta é corrigida por interface.
- Uptime: sysctlbyname("kern.boottime"). Disco: os.statvfs.
- Bateria: `pmset -g batt` (subprocess, cache de 10 s). Ler o IOKit exigiria
  CoreFoundation via ctypes, frágil demais para ganhar alguns milissegundos.
- Temperatura: None. Só sai do SMC (IOKit, chaves não documentadas e que mudam
  entre Intel e Apple Silicon) ou de `powermetrics`, que exige sudo.

Se a libSystem não carregar por ctypes, cada indicador cai para um comando
(argv fixo, caminho absoluto, timeout curto, cache) cujos parsers estão aqui e
são testados em tests/test_metrics.py. CPU não tem fallback: fica em 0.
"""

import ctypes
import os
import re
import subprocess
import time
from pathlib import Path

SYSCTL = "/usr/sbin/sysctl"
VM_STAT = "/usr/bin/vm_stat"
NETSTAT = "/usr/sbin/netstat"
PMSET = "/usr/bin/pmset"
PS = "/bin/ps"
LIBSYSTEM = "/usr/lib/libSystem.B.dylib"

_ENV = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LC_ALL": "C"}
_SKIP_IF = ("lo", "gif", "stf", "utun", "awdl", "llw", "bridge", "anpi")
_U32 = 1 << 32


# --- comandos (fallback e bateria) -------------------------------------------

def run(argv: list[str], timeout: float = 1.5) -> str:
    """Saída de um comando fixo; "" em qualquer erro (nunca levanta)."""
    try:
        r = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, timeout=timeout, env=_ENV,
                           text=True, errors="replace", check=False)
    except (OSError, subprocess.SubprocessError, ValueError):
        return ""
    return r.stdout if r.returncode == 0 else ""


class Cached:
    """Guarda o resultado de fn por ttl segundos (com o instante da leitura)."""

    def __init__(self, fn, ttl: float):
        self.fn, self.ttl = fn, ttl
        self.at: float | None = None
        self.value = None

    def get(self):
        now = time.monotonic()
        if self.at is None or now - self.at >= self.ttl:
            self.value, self.at = self.fn(), now
        return self.value


# --- parsers (puros, testados em qualquer sistema) ---------------------------

def parse_vm_stat(text: str) -> tuple[int, dict[str, int]]:
    """(tamanho da página, {"Pages free": n, ...}) da saída do vm_stat."""
    m = re.search(r"page size of (\d+) bytes", text)
    page = int(m.group(1)) if m else 4096
    pages: dict[str, int] = {}
    for line in text.splitlines():
        k, sep, v = line.rpartition(":")
        v = v.strip().rstrip(".")
        if sep and v.isdigit():
            pages[k.strip().strip('"')] = int(v)
    return page, pages


def vm_used(page: int, pages: dict[str, int], total: int) -> int:
    """Memória usada como o Monitor de Atividade: app + wired + comprimida."""
    wired = pages.get("Pages wired down", 0)
    comp = pages.get("Pages occupied by compressor", 0)
    if "Anonymous pages" in pages:
        app = pages["Anonymous pages"] - pages.get("Pages purgeable", 0)
    else:
        app = pages.get("Pages active", 0)
    used = max(0, app + wired + comp) * page
    return min(used, total) if total else used


_UNITS = {"B": 1, "K": 1024, "M": 1024 ** 2, "G": 1024 ** 3, "T": 1024 ** 4}


def parse_swapusage(text: str) -> tuple[int, int] | None:
    """(usado, total) em bytes de `sysctl vm.swapusage`."""
    vals = {}
    for k, num, unit in re.findall(r"(total|used|free)\s*=\s*([\d.]+)([BKMGT]?)", text):
        vals[k] = int(float(num) * _UNITS.get(unit or "B", 1))
    if "total" not in vals or "used" not in vals:
        return None
    return vals["used"], vals["total"]


def parse_boottime(text: str) -> float | None:
    """Epoch do boot de `sysctl kern.boottime` ({ sec = N, usec = M } ...)."""
    m = re.search(r"sec\s*=\s*(\d+)(?:,\s*usec\s*=\s*(\d+))?", text)
    if not m:
        return None
    return int(m.group(1)) + int(m.group(2) or 0) / 1e6


_BATT_STATE = {
    "charging": "Charging",
    "finishing charge": "Charging",
    "discharging": "Discharging",
    "charged": "Full",
    "ac attached": "Not charging",
}


def parse_pmset_batt(text: str) -> tuple[int, str] | None:
    """(pct, status no vocabulário do Linux) de `pmset -g batt`; None sem bateria."""
    for line in text.splitlines():
        m = re.search(r"(\d+)%;\s*([^;]+);", line)
        if not m or "InternalBattery" not in line:
            continue
        state = m.group(2).strip().lower()
        if "drawing from" in text and "AC Power" in text and state not in _BATT_STATE:
            status = "Not charging"
        else:
            status = _BATT_STATE.get(state, state.capitalize())
        return min(100, int(m.group(1))), status
    return None


def parse_netstat_ib(text: str) -> tuple[int, int]:
    """(rx, tx) somados das linhas <Link#N> de `netstat -ib`, sem as interfaces puladas."""
    rx = tx = 0
    for line in text.splitlines():
        f = line.split()
        if len(f) < 10 or not f[2].startswith("<Link#"):
            continue
        if f[0].rstrip("*").startswith(_SKIP_IF):
            continue
        # Do fim: Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll (Address pode faltar).
        try:
            rx += int(f[-5])
            tx += int(f[-2])
        except ValueError:
            continue
    return rx, tx


def count_ps(text: str) -> int:
    return sum(1 for line in text.splitlines() if line.strip())


# --- ctypes -----------------------------------------------------------------

HOST_CPU_LOAD_INFO = 3
HOST_VM_INFO64 = 4
CPU_STATE_IDLE = 2
AF_LINK = 18


class VmStatistics64(ctypes.Structure):
    # <mach/vm_statistics.h>, struct vm_statistics64 (natural_t = uint32).
    _fields_ = [
        ("free_count", ctypes.c_uint32),
        ("active_count", ctypes.c_uint32),
        ("inactive_count", ctypes.c_uint32),
        ("wire_count", ctypes.c_uint32),
        ("zero_fill_count", ctypes.c_uint64),
        ("reactivations", ctypes.c_uint64),
        ("pageins", ctypes.c_uint64),
        ("pageouts", ctypes.c_uint64),
        ("faults", ctypes.c_uint64),
        ("cow_faults", ctypes.c_uint64),
        ("lookups", ctypes.c_uint64),
        ("hits", ctypes.c_uint64),
        ("purges", ctypes.c_uint64),
        ("purgeable_count", ctypes.c_uint32),
        ("speculative_count", ctypes.c_uint32),
        ("decompressions", ctypes.c_uint64),
        ("compressions", ctypes.c_uint64),
        ("swapins", ctypes.c_uint64),
        ("swapouts", ctypes.c_uint64),
        ("compressor_page_count", ctypes.c_uint32),
        ("throttled_count", ctypes.c_uint32),
        ("external_page_count", ctypes.c_uint32),
        ("internal_page_count", ctypes.c_uint32),
        ("total_uncompressed_pages_in_compressor", ctypes.c_uint64),
    ]


HOST_VM_INFO64_COUNT = ctypes.sizeof(VmStatistics64) // 4  # 38


class XswUsage(ctypes.Structure):
    # <sys/sysctl.h>, struct xsw_usage (vm.swapusage).
    _fields_ = [
        ("xsu_total", ctypes.c_uint64),
        ("xsu_avail", ctypes.c_uint64),
        ("xsu_used", ctypes.c_uint64),
        ("xsu_pagesize", ctypes.c_uint32),
        ("xsu_encrypted", ctypes.c_int32),
    ]


class Timeval(ctypes.Structure):
    # struct timeval em 64 bits (kern.boottime).
    _fields_ = [("tv_sec", ctypes.c_int64), ("tv_usec", ctypes.c_int32)]


class Sockaddr(ctypes.Structure):
    _fields_ = [("sa_len", ctypes.c_uint8), ("sa_family", ctypes.c_uint8),
                ("sa_data", ctypes.c_char * 14)]


class IfData(ctypes.Structure):
    # <net/if_var.h>, struct if_data (começo; só lemos até ifi_obytes).
    _fields_ = [
        ("ifi_type", ctypes.c_uint8),
        ("ifi_typelen", ctypes.c_uint8),
        ("ifi_physical", ctypes.c_uint8),
        ("ifi_addrlen", ctypes.c_uint8),
        ("ifi_hdrlen", ctypes.c_uint8),
        ("ifi_recvquota", ctypes.c_uint8),
        ("ifi_xmitquota", ctypes.c_uint8),
        ("ifi_unused1", ctypes.c_uint8),
        ("ifi_mtu", ctypes.c_uint32),
        ("ifi_metric", ctypes.c_uint32),
        ("ifi_baudrate", ctypes.c_uint32),
        ("ifi_ipackets", ctypes.c_uint32),
        ("ifi_ierrors", ctypes.c_uint32),
        ("ifi_opackets", ctypes.c_uint32),
        ("ifi_oerrors", ctypes.c_uint32),
        ("ifi_collisions", ctypes.c_uint32),
        ("ifi_ibytes", ctypes.c_uint32),
        ("ifi_obytes", ctypes.c_uint32),
    ]


class Ifaddrs(ctypes.Structure):
    pass


Ifaddrs._fields_ = [
    ("ifa_next", ctypes.POINTER(Ifaddrs)),
    ("ifa_name", ctypes.c_char_p),
    ("ifa_flags", ctypes.c_uint),
    ("ifa_addr", ctypes.POINTER(Sockaddr)),
    ("ifa_netmask", ctypes.POINTER(Sockaddr)),
    ("ifa_dstaddr", ctypes.POINTER(Sockaddr)),
    ("ifa_data", ctypes.c_void_p),
]


def _load_libsystem():
    try:
        lib = ctypes.CDLL(LIBSYSTEM, use_errno=True)
        lib.mach_host_self.restype = ctypes.c_uint32
        lib.mach_host_self.argtypes = []
        for fn in (lib.host_statistics, lib.host_statistics64):
            fn.restype = ctypes.c_int
            fn.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_void_p,
                           ctypes.POINTER(ctypes.c_uint32)]
        lib.host_page_size.restype = ctypes.c_int
        lib.host_page_size.argtypes = [ctypes.c_uint32, ctypes.POINTER(ctypes.c_size_t)]
        lib.sysctlbyname.restype = ctypes.c_int
        lib.sysctlbyname.argtypes = [ctypes.c_char_p, ctypes.c_void_p,
                                     ctypes.POINTER(ctypes.c_size_t), ctypes.c_void_p,
                                     ctypes.c_size_t]
        lib.getifaddrs.restype = ctypes.c_int
        lib.getifaddrs.argtypes = [ctypes.POINTER(ctypes.POINTER(Ifaddrs))]
        lib.freeifaddrs.restype = None
        lib.freeifaddrs.argtypes = [ctypes.POINTER(Ifaddrs)]
    except (OSError, AttributeError):
        return None
    try:  # opcional: sem ele, a contagem de processos cai para `ps -A`
        lib.proc_listallpids.restype = ctypes.c_int
        lib.proc_listallpids.argtypes = [ctypes.c_void_p, ctypes.c_int]
        lib.has_listpids = True
    except AttributeError:
        lib.has_listpids = False
    return lib


class Backend:
    name = "darwin"

    def __init__(self, lib=None):
        # lib=False desliga o ctypes (testes do fallback por comando).
        self.lib = _load_libsystem() if lib is None else (lib or None)
        self.host = 0
        self.page = 0
        if self.lib is not None:
            try:
                self.host = self.lib.mach_host_self()  # uma vez: cada chamada soma uma referência
                size = ctypes.c_size_t(0)
                if self.lib.host_page_size(self.host, ctypes.byref(size)) == 0:
                    self.page = size.value
            except Exception:
                self.lib = None
        self._cpu_raw: list[int] | None = None
        self._cpu_acc = [0, 0, 0, 0]
        self._if_raw: dict[str, tuple[int, int]] | None = None
        self._if_acc = [0, 0]
        self._memsize = Cached(lambda: _int(run([SYSCTL, "-n", "hw.memsize"])), 3600)
        self._vm_stat = Cached(lambda: parse_vm_stat(run([VM_STAT])), 2)
        self._swap = Cached(lambda: parse_swapusage(run([SYSCTL, "vm.swapusage"])), 2)
        self._boot = Cached(lambda: parse_boottime(run([SYSCTL, "kern.boottime"])), 60)
        self._ps = Cached(lambda: count_ps(run([PS, "-A", "-o", "pid="])), 2)
        self._netstat = Cached(lambda: (*parse_netstat_ib(run([NETSTAT, "-ib"])), time.monotonic()), 2)
        self._pmset = Cached(lambda: parse_pmset_batt(run([PMSET, "-g", "batt"])), 10)

    # sysctlbyname -----------------------------------------------------------

    def _sysctl(self, name: str, obj) -> bool:
        size = ctypes.c_size_t(ctypes.sizeof(obj))
        ok = self.lib.sysctlbyname(name.encode(), ctypes.byref(obj), ctypes.byref(size), None, 0)
        return ok == 0

    def _sysctl_uint(self, name: str) -> int | None:
        buf = ctypes.c_uint64(0)
        size = ctypes.c_size_t(8)
        if self.lib.sysctlbyname(name.encode(), ctypes.byref(buf), ctypes.byref(size), None, 0):
            return None
        return buf.value & 0xFFFFFFFF if size.value == 4 else buf.value

    # indicadores ------------------------------------------------------------

    def root_paths(self) -> list[Path]:
        return [Path("/")]

    def root_label(self, p: Path) -> str:
        return "/"

    def cpu_times(self) -> tuple[int, int] | None:
        if self.lib is None:
            return None
        ticks = (ctypes.c_uint32 * 4)()
        count = ctypes.c_uint32(4)
        if self.lib.host_statistics(self.host, HOST_CPU_LOAD_INFO, ticks, ctypes.byref(count)):
            return None
        raw = list(ticks)
        if self._cpu_raw is not None:  # contadores de 32 bits: soma a diferença módulo 2^32
            for i in range(4):
                self._cpu_acc[i] += (raw[i] - self._cpu_raw[i]) % _U32
        else:
            self._cpu_acc = raw[:]
        self._cpu_raw = raw
        return self._cpu_acc[CPU_STATE_IDLE], sum(self._cpu_acc)

    def memory(self) -> tuple[int, int, int, int]:
        total = used = swap_used = swap_total = 0
        if self.lib is not None:
            total = self._sysctl_uint("hw.memsize") or 0
            vm = VmStatistics64()
            count = ctypes.c_uint32(HOST_VM_INFO64_COUNT)
            if self.page and not self.lib.host_statistics64(
                    self.host, HOST_VM_INFO64, ctypes.byref(vm), ctypes.byref(count)):
                used = vm_used(self.page, {
                    "Anonymous pages": vm.internal_page_count,
                    "Pages purgeable": vm.purgeable_count,
                    "Pages wired down": vm.wire_count,
                    "Pages occupied by compressor": vm.compressor_page_count,
                }, total)
            sw = XswUsage()
            if self._sysctl("vm.swapusage", sw):
                swap_used, swap_total = sw.xsu_used, sw.xsu_total
            return used, total, swap_used, swap_total
        total = self._memsize.get() or 0
        page, pages = self._vm_stat.get()
        used = vm_used(page, pages, total) if pages else 0
        swap_used, swap_total = self._swap.get() or (0, 0)
        return used, total, swap_used, swap_total

    def load_procs(self) -> tuple[tuple[float, float, float] | None, int]:
        try:
            load = os.getloadavg()
        except OSError:
            load = None
        return load, self._procs()

    def _procs(self) -> int:
        if self.lib is not None and getattr(self.lib, "has_listpids", False):
            est = self.lib.proc_listallpids(None, 0)
            if est > 0:
                n = est + 64
                buf = (ctypes.c_int * n)()
                got = self.lib.proc_listallpids(buf, ctypes.sizeof(buf))
                if got > 0:
                    return got
        return self._ps.get() or 0

    def net_bytes(self) -> tuple[int, int, float] | None:
        if self.lib is None:
            rx, tx, at = self._netstat.get()
            return (rx, tx, at) if (rx or tx) else None
        head = ctypes.POINTER(Ifaddrs)()
        if self.lib.getifaddrs(ctypes.byref(head)) != 0:
            return None
        cur: dict[str, tuple[int, int]] = {}
        try:
            node = head
            while node:
                ifa = node.contents
                node = ifa.ifa_next
                if not ifa.ifa_addr or not ifa.ifa_data or ifa.ifa_addr.contents.sa_family != AF_LINK:
                    continue
                name = (ifa.ifa_name or b"").decode(errors="replace")
                if name.startswith(_SKIP_IF):
                    continue
                d = IfData.from_address(ifa.ifa_data)
                cur[name] = (d.ifi_ibytes, d.ifi_obytes)
        finally:
            self.lib.freeifaddrs(head)
        now = time.monotonic()
        for name, (rx, tx) in cur.items():
            prev = None if self._if_raw is None else self._if_raw.get(name)
            if prev is None:
                if self._if_raw is not None:  # interface nova depois da 1ª leitura: entra sem pico
                    continue
                self._if_acc[0] += rx
                self._if_acc[1] += tx
            else:
                self._if_acc[0] += (rx - prev[0]) % _U32
                self._if_acc[1] += (tx - prev[1]) % _U32
        self._if_raw = cur
        return self._if_acc[0], self._if_acc[1], now

    def uptime(self) -> float:
        boot = None
        if self.lib is not None:
            tv = Timeval()
            if self._sysctl("kern.boottime", tv):
                boot = tv.tv_sec + tv.tv_usec / 1e6
        else:
            boot = self._boot.get()
        return max(0.0, time.time() - boot) if boot else 0.0

    def temp(self) -> float | None:
        return None  # exige SMC/IOKit ou powermetrics com sudo

    def battery(self) -> tuple[int, str] | None:
        return self._pmset.get()

    def disk_usage(self, p: Path) -> tuple[object, int | None, int | None] | None:
        # No APFS, "/" (sistema, só leitura) e a HOME (Data) são volumes
        # diferentes do mesmo contêiner: o statvfs dá o mesmo tamanho e o
        # mesmo livre. A chave é o tamanho, para não mostrar o disco duas vezes.
        try:
            v = os.statvfs(p)
        except OSError:
            return None
        total = v.f_blocks * v.f_frsize
        return ("blocks", total), total - v.f_bavail * v.f_frsize, total


def _int(text: str) -> int | None:
    text = text.strip()
    return int(text) if text.isdigit() else None
