"""Backend Windows dos indicadores, só ctypes (sem pywin32 nem psutil).

- CPU: GetSystemTimes (o tempo de kernel já inclui o ocioso).
- Memória: GlobalMemoryStatusEx. O Windows não tem um "swap" separado: o
  aproximamos pelo arquivo de paginação, total = ullTotalPageFile -
  ullTotalPhys (o limite de commit menos a RAM) e usado = commit em uso menos a
  RAM em uso, limitado a [0, total].
- Disco: GetDiskFreeSpaceExW (os.statvfs não existe no Windows), para a
  unidade do sistema (%SystemDrive%) e a da HOME, sem repetir a unidade.
- Uptime: GetTickCount64. Bateria: GetSystemPowerStatus.
- Rede: GetIfTable2 (iphlpapi), somando InOctets/OutOctets das interfaces de
  hardware ativas (HardwareInterface=1, FilterInterface=0, OperStatus=Up). As
  interfaces de filtro (WFP, QoS...) repetem o tráfego do adaptador e ficam
  fora, assim como loopback, VPN e adaptadores virtuais.
- Processos: K32EnumProcesses (contagem). Load: None (não existe no Windows).
- Temperatura: None (WMI MSAcpi_ThermalZoneTemperature exige admin e quase
  nunca reflete a CPU).

As estruturas usam tipos de largura fixa (c_uint32/c_uint64/c_uint16) e não
wintypes, para o layout ser o mesmo quando o módulo é importado no Linux
(tests/test_metrics.py confere tamanhos e deslocamentos em qualquer sistema).
As DLLs só são carregadas em Backend(), nunca na importação.
"""

import ctypes
import os
import time
from pathlib import Path

IF_TYPE_SOFTWARE_LOOPBACK = 24
IF_OPER_STATUS_UP = 1
BATTERY_FLAG_CHARGING = 8
BATTERY_FLAG_NO_BATTERY = 128
BATTERY_FLAG_UNKNOWN = 255
BATTERY_PERCENT_UNKNOWN = 255


class MemoryStatusEx(ctypes.Structure):
    _fields_ = [
        ("dwLength", ctypes.c_uint32),
        ("dwMemoryLoad", ctypes.c_uint32),
        ("ullTotalPhys", ctypes.c_uint64),
        ("ullAvailPhys", ctypes.c_uint64),
        ("ullTotalPageFile", ctypes.c_uint64),
        ("ullAvailPageFile", ctypes.c_uint64),
        ("ullTotalVirtual", ctypes.c_uint64),
        ("ullAvailVirtual", ctypes.c_uint64),
        ("ullAvailExtendedVirtual", ctypes.c_uint64),
    ]


class SystemPowerStatus(ctypes.Structure):
    _fields_ = [
        ("ACLineStatus", ctypes.c_uint8),
        ("BatteryFlag", ctypes.c_uint8),
        ("BatteryLifePercent", ctypes.c_uint8),
        ("SystemStatusFlag", ctypes.c_uint8),
        ("BatteryLifeTime", ctypes.c_uint32),
        ("BatteryFullLifeTime", ctypes.c_uint32),
    ]


class Guid(ctypes.Structure):
    _fields_ = [("Data1", ctypes.c_uint32), ("Data2", ctypes.c_uint16),
                ("Data3", ctypes.c_uint16), ("Data4", ctypes.c_uint8 * 8)]


IF_MAX_STRING_SIZE = 256
IF_MAX_PHYS_ADDRESS_LENGTH = 32


class MibIfRow2(ctypes.Structure):
    # <netioapi.h>, MIB_IF_ROW2 (1352 bytes em x64 e arm64). WCHAR = uint16.
    _fields_ = [
        ("InterfaceLuid", ctypes.c_uint64),
        ("InterfaceIndex", ctypes.c_uint32),
        ("InterfaceGuid", Guid),
        ("Alias", ctypes.c_uint16 * (IF_MAX_STRING_SIZE + 1)),
        ("Description", ctypes.c_uint16 * (IF_MAX_STRING_SIZE + 1)),
        ("PhysicalAddressLength", ctypes.c_uint32),
        ("PhysicalAddress", ctypes.c_uint8 * IF_MAX_PHYS_ADDRESS_LENGTH),
        ("PermanentPhysicalAddress", ctypes.c_uint8 * IF_MAX_PHYS_ADDRESS_LENGTH),
        ("Mtu", ctypes.c_uint32),
        ("Type", ctypes.c_uint32),
        ("TunnelType", ctypes.c_int32),
        ("MediaType", ctypes.c_int32),
        ("PhysicalMediumType", ctypes.c_int32),
        ("AccessType", ctypes.c_int32),
        ("DirectionType", ctypes.c_int32),
        # Campos de bit (MSVC aloca do bit menos significativo):
        # 0 HardwareInterface, 1 FilterInterface, 2 ConnectorPresent, ...
        ("InterfaceAndOperStatusFlags", ctypes.c_uint8),
        ("OperStatus", ctypes.c_int32),
        ("AdminStatus", ctypes.c_int32),
        ("MediaConnectState", ctypes.c_int32),
        ("NetworkGuid", Guid),
        ("ConnectionType", ctypes.c_int32),
        ("TransmitLinkSpeed", ctypes.c_uint64),
        ("ReceiveLinkSpeed", ctypes.c_uint64),
        ("InOctets", ctypes.c_uint64),
        ("InUcastPkts", ctypes.c_uint64),
        ("InNUcastPkts", ctypes.c_uint64),
        ("InDiscards", ctypes.c_uint64),
        ("InErrors", ctypes.c_uint64),
        ("InUnknownProtos", ctypes.c_uint64),
        ("InUcastOctets", ctypes.c_uint64),
        ("InMulticastOctets", ctypes.c_uint64),
        ("InBroadcastOctets", ctypes.c_uint64),
        ("OutOctets", ctypes.c_uint64),
        ("OutUcastPkts", ctypes.c_uint64),
        ("OutNUcastPkts", ctypes.c_uint64),
        ("OutDiscards", ctypes.c_uint64),
        ("OutErrors", ctypes.c_uint64),
        ("OutUcastOctets", ctypes.c_uint64),
        ("OutMulticastOctets", ctypes.c_uint64),
        ("OutBroadcastOctets", ctypes.c_uint64),
        ("OutQLen", ctypes.c_uint64),
    ]


MIB_IF_TABLE2_ROWS_OFFSET = 8  # ULONG NumEntries + preenchimento até o alinhamento de 8


def swap_from_memstatus(m: MemoryStatusEx) -> tuple[int, int]:
    """(usado, total) do "swap" aproximado pelo arquivo de paginação."""
    total = max(0, m.ullTotalPageFile - m.ullTotalPhys)
    commit = m.ullTotalPageFile - m.ullAvailPageFile
    ram = m.ullTotalPhys - m.ullAvailPhys
    return min(total, max(0, commit - ram)), total


def battery_from_status(ac: int, flag: int, pct: int) -> tuple[int, str] | None:
    """(pct, status do Linux) de SYSTEM_POWER_STATUS; None sem bateria."""
    if flag == BATTERY_FLAG_UNKNOWN or flag & BATTERY_FLAG_NO_BATTERY or pct == BATTERY_PERCENT_UNKNOWN:
        return None
    if flag & BATTERY_FLAG_CHARGING:
        status = "Charging"
    elif ac == 1:
        status = "Full" if pct >= 100 else "Not charging"
    else:
        status = "Discharging"
    return min(100, pct), status


def row_counts(row: MibIfRow2) -> bool:
    """A interface entra na soma da rede?"""
    flags = row.InterfaceAndOperStatusFlags
    return (bool(flags & 1) and not flags & 2 and row.OperStatus == IF_OPER_STATUS_UP
            and row.Type != IF_TYPE_SOFTWARE_LOOPBACK)


def _system_dll(kernel32, name: str):
    """Carrega a DLL pelo caminho do System32 (nunca da pasta atual)."""
    buf = ctypes.create_unicode_buffer(260)
    n = kernel32.GetSystemDirectoryW(buf, 260)
    path = os.path.join(buf.value, name) if 0 < n < 260 else name
    return ctypes.WinDLL(path, use_last_error=True)


class Backend:
    name = "win32"

    def __init__(self):
        self.k32 = self.iphlp = None
        try:
            k32 = ctypes.WinDLL("kernel32", use_last_error=True)
            u64p = ctypes.POINTER(ctypes.c_uint64)
            k32.GetSystemDirectoryW.restype = ctypes.c_uint32
            k32.GetSystemDirectoryW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32]
            k32.GetSystemTimes.restype = ctypes.c_int
            k32.GetSystemTimes.argtypes = [u64p, u64p, u64p]
            k32.GlobalMemoryStatusEx.restype = ctypes.c_int
            k32.GlobalMemoryStatusEx.argtypes = [ctypes.POINTER(MemoryStatusEx)]
            k32.GetDiskFreeSpaceExW.restype = ctypes.c_int
            k32.GetDiskFreeSpaceExW.argtypes = [ctypes.c_wchar_p, u64p, u64p, u64p]
            k32.GetTickCount64.restype = ctypes.c_uint64
            k32.GetTickCount64.argtypes = []
            k32.GetSystemPowerStatus.restype = ctypes.c_int
            k32.GetSystemPowerStatus.argtypes = [ctypes.POINTER(SystemPowerStatus)]
            k32.K32EnumProcesses.restype = ctypes.c_int
            k32.K32EnumProcesses.argtypes = [ctypes.POINTER(ctypes.c_uint32), ctypes.c_uint32,
                                             ctypes.POINTER(ctypes.c_uint32)]
            self.k32 = k32
        except (OSError, AttributeError):
            return
        try:
            ip = _system_dll(self.k32, "iphlpapi.dll")
            ip.GetIfTable2.restype = ctypes.c_uint32
            ip.GetIfTable2.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
            ip.FreeMibTable.restype = None
            ip.FreeMibTable.argtypes = [ctypes.c_void_p]
            self.iphlp = ip
        except (OSError, AttributeError):
            self.iphlp = None

    def root_paths(self) -> list[Path]:
        return [Path(os.environ.get("SystemDrive", "C:") + "\\")]

    def root_label(self, p: Path) -> str:
        return p.drive or str(p)

    def cpu_times(self) -> tuple[int, int] | None:
        if self.k32 is None:
            return None
        idle, kernel, user = ctypes.c_uint64(), ctypes.c_uint64(), ctypes.c_uint64()
        if not self.k32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user)):
            return None
        return idle.value, kernel.value + user.value

    def memory(self) -> tuple[int, int, int, int]:
        if self.k32 is None:
            return 0, 0, 0, 0
        m = MemoryStatusEx()
        m.dwLength = ctypes.sizeof(MemoryStatusEx)
        if not self.k32.GlobalMemoryStatusEx(ctypes.byref(m)):
            return 0, 0, 0, 0
        swap_used, swap_total = swap_from_memstatus(m)
        return m.ullTotalPhys - m.ullAvailPhys, m.ullTotalPhys, swap_used, swap_total

    def load_procs(self) -> tuple[None, int]:
        return None, self._procs()

    def _procs(self) -> int:
        if self.k32 is None:
            return 0
        n = 1024
        while n <= 1 << 20:
            pids = (ctypes.c_uint32 * n)()
            needed = ctypes.c_uint32(0)
            if not self.k32.K32EnumProcesses(pids, ctypes.sizeof(pids), ctypes.byref(needed)):
                return 0
            if needed.value < ctypes.sizeof(pids):
                return needed.value // 4
            n *= 2
        return 0

    def net_bytes(self) -> tuple[int, int, float] | None:
        if self.iphlp is None:
            return None
        table = ctypes.c_void_p()
        if self.iphlp.GetIfTable2(ctypes.byref(table)) != 0 or not table.value:
            return None
        rx = tx = 0
        try:
            n = ctypes.c_uint32.from_address(table.value).value
            rows = (MibIfRow2 * n).from_address(table.value + MIB_IF_TABLE2_ROWS_OFFSET)
            for row in rows:
                if row_counts(row):
                    rx += row.InOctets
                    tx += row.OutOctets
        finally:
            self.iphlp.FreeMibTable(table)
        return rx, tx, time.monotonic()

    def uptime(self) -> float:
        return self.k32.GetTickCount64() / 1000 if self.k32 is not None else 0.0

    def temp(self) -> float | None:
        return None

    def battery(self) -> tuple[int, str] | None:
        if self.k32 is None:
            return None
        st = SystemPowerStatus()
        if not self.k32.GetSystemPowerStatus(ctypes.byref(st)):
            return None
        return battery_from_status(st.ACLineStatus, st.BatteryFlag, st.BatteryLifePercent)

    def disk_usage(self, p: Path) -> tuple[object, int | None, int | None] | None:
        if self.k32 is None:
            return None
        drive = os.path.splitdrive(os.path.abspath(p))[0]
        if not drive:
            return None
        root = drive.rstrip("\\") + "\\"
        free_caller, total, free = ctypes.c_uint64(), ctypes.c_uint64(), ctypes.c_uint64()
        if not self.k32.GetDiskFreeSpaceExW(root, ctypes.byref(free_caller),
                                            ctypes.byref(total), ctypes.byref(free)):
            return None
        return drive.upper(), total.value - free_caller.value, total.value
