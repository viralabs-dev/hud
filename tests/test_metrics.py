"""Indicadores do sistema: backend Linux, parsers do macOS, estruturas do Windows.

Os testes de parser e de estrutura rodam em qualquer sistema. Os que chamam as
APIs de verdade só rodam no sistema delas (skipUnless) e quem os prova é o CI
de macOS e Windows.
"""

import ctypes
import importlib
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

from hud import metrics, metrics_linux, metrics_macos as mac, metrics_windows as win
from hud.metrics import Metrics, Sample

# --- saídas de exemplo no formato documentado pela Apple (números ilustrativos)

VM_STAT = """\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                3000.
Pages active:                            200000.
Pages inactive:                          190000.
Pages speculative:                         5000.
Pages throttled:                              0.
Pages wired down:                         80000.
Pages purgeable:                          10000.
"Translation faults":                 900000000.
Pages copy-on-write:                   20000000.
Pages zero filled:                    400000000.
Pages reactivated:                      5000000.
Pages purged:                           1000000.
File-backed pages:                       150000.
Anonymous pages:                         240000.
Pages stored in compressor:              300000.
Pages occupied by compressor:            100000.
Decompressions:                         7000000.
Compressions:                           9000000.
Pageins:                               30000000.
Pageouts:                                200000.
Swapins:                                  10000.
Swapouts:                                 20000.
"""

SWAPUSAGE = "vm.swapusage: total = 2048.00M  used = 1024.50M  free = 1023.50M  (encrypted)\n"

BOOTTIME = "kern.boottime: { sec = 1700000000, usec = 250000 } Tue Nov 14 22:13:20 2023\n"

PMSET_DISCHARGING = """\
Now drawing from 'Battery Power'
 -InternalBattery-0 (id=4653155)\t85%; discharging; 4:20 remaining present: true
"""
PMSET_CHARGING = """\
Now drawing from 'AC Power'
 -InternalBattery-0 (id=4653155)\t42%; charging; 1:05 remaining present: true
"""
PMSET_CHARGED = """\
Now drawing from 'AC Power'
 -InternalBattery-0 (id=4653155)\t100%; charged; 0:00 remaining present: true
"""
PMSET_AC_ATTACHED = """\
Now drawing from 'AC Power'
 -InternalBattery-0 (id=4653155)\t80%; AC attached; not charging present: true
"""
PMSET_DESKTOP = "Now drawing from 'AC Power'\n"

NETSTAT_IB = """\
Name       Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll
lo0        16384 <Link#1>                         50000     0    9000000    50000     0    9000000     0
lo0        16384 127           localhost          50000     -    9000000    50000     -    9000000     -
gif0*      1280  <Link#2>                             0     0          0        0     0          0     0
en0        1500  <Link#4>    a0:b1:c2:d3:e4:f5  1200000     0 1500000000   800000     0  200000000     0
en0        1500  192.168.1     192.168.1.20     1200000     -     900000   800000     -     700000     -
en1        1500  <Link#5>    a0:b1:c2:d3:e4:f6     1000     0     100000      500     0      50000     0
utun0      1380  <Link#12>                         3000     0     400000     3000     0     300000     0
awdl0      1484  <Link#9>    1a:2b:3c:4d:5e:6f      100     0      20000      100     0      10000     0
"""


class FakeBackend:
    """Backend controlável para testar a montagem do Sample."""

    name = "fake"

    def __init__(self):
        self.cpu = [(100, 1000), (150, 1100)]
        self.net = [(1000, 500, 10.0), (3000, 1500, 12.0)]
        self.load = None
        self.disks = {}

    def root_paths(self):
        return [Path("/raiz")]

    def root_label(self, p):
        return "R:"

    def cpu_times(self):
        return self.cpu.pop(0) if self.cpu else None

    def memory(self):
        return 10, 100, 1, 50

    def load_procs(self):
        return self.load, 7

    def net_bytes(self):
        return self.net.pop(0) if self.net else None

    def uptime(self):
        raise RuntimeError("quebrou")

    def temp(self):
        return None

    def battery(self):
        return 50, "Discharging"

    def disk_usage(self, p):
        return self.disks.get(str(p))


class MetricsTest(unittest.TestCase):
    def test_backend_da_plataforma(self):
        want = {"darwin": mac.Backend, "win32": win.Backend}.get(sys.platform, metrics_linux.Backend)
        self.assertIs(metrics._Backend, want)
        self.assertEqual(Metrics().platform, sys.platform)

    def test_sample_com_backend_falso(self):
        b = FakeBackend()
        m = Metrics(backend=b)
        self.assertFalse(m.last.load_ok)
        self.assertFalse(m.last.net_ok)
        s1 = m.sample()
        self.assertEqual(s1.cpu, 0.0)
        self.assertTrue(s1.net_ok)
        self.assertEqual((s1.rx_rate, s1.tx_rate), (0.0, 0.0))
        s2 = m.sample()
        self.assertAlmostEqual(s2.cpu, 50.0)  # idle +50 em total +100
        self.assertEqual((s2.rx_rate, s2.tx_rate), (1000.0, 500.0))
        self.assertEqual((s2.mem_used, s2.mem_total, s2.swap_used, s2.swap_total), (10, 100, 1, 50))
        # Sem load average: zeros e load_ok False (a UI mostra "—").
        self.assertEqual(s2.load, (0.0, 0.0, 0.0))
        self.assertFalse(s2.load_ok)
        self.assertEqual(s2.procs, 7)
        # Indicador que levanta vira o padrão, sem exceção para a UI.
        self.assertEqual(s2.uptime, 0.0)
        self.assertEqual(s2.battery, (50, "Discharging"))
        self.assertIs(m.last, s2)
        self.assertEqual(list(m.cpu_hist), [0.0, s2.cpu])
        self.assertEqual(list(m.net_hist), [0.0, 1500.0])
        # Sem contadores de rede: zeros e net_ok False.
        s3 = m.sample()
        self.assertFalse(s3.net_ok)
        self.assertEqual((s3.rx_rate, s3.tx_rate), (0.0, 0.0))
        self.assertEqual(s3.cpu, 0.0)

    def test_load_disponivel(self):
        b = FakeBackend()
        b.load = (1.0, 2.0, 3.0)
        s = Metrics(backend=b).sample()
        self.assertEqual(s.load, (1.0, 2.0, 3.0))
        self.assertTrue(s.load_ok)

    def test_leitura_em_cache_repete_a_taxa(self):
        b = FakeBackend()
        b.net = [(0, 0, 1.0), (2000, 1000, 2.0), (2000, 1000, 2.0), (2000, 1000, 2.0)]
        m = Metrics(backend=b)
        m.sample()
        self.assertEqual((m.sample().rx_rate, m.last.tx_rate), (2000.0, 1000.0))
        self.assertEqual((m.sample().rx_rate, m.last.tx_rate), (2000.0, 1000.0))

    def test_discos_sem_repetir_volume(self):
        b = FakeBackend()
        home = str(Path.home())
        b.disks = {"/raiz": ("C", 30, 100), home: ("C", 30, 100),
                   "/dados/vault": ("D", 5, 10), "/quebrado": ("E", None, None)}
        m = Metrics(extra_paths=[Path("/dados/vault"), Path("/quebrado"), Path("/sumiu")], backend=b)
        disks = m.sample().disks
        self.assertEqual([(d.label, d.used, d.total) for d in disks], [("R:", 30, 100), ("vault", 5, 10)])
        self.assertEqual(disks[0].pct, 30.0)

    def test_simula_importacao_em_outras_plataformas(self):
        # Só confere seleção e importação; as APIs de verdade ficam para o CI.
        try:
            for plat, mod in (("darwin", mac), ("win32", win)):
                with mock.patch.object(sys, "platform", plat), \
                        mock.patch.object(mac, "run", return_value=""), \
                        mock.patch.object(mac, "_load_libsystem", return_value=None):
                    m = importlib.reload(metrics)
                    self.assertIs(m._Backend, mod.Backend)
                    if sys.platform not in ("darwin", "win32"):
                        s = m.Metrics().sample()
                        self.assertIsInstance(s, m.Sample)
                        self.assertFalse(s.load_ok and plat == "win32")
        finally:
            importlib.reload(metrics)


class LinuxBackendTest(unittest.TestCase):
    FILES = {
        "/proc/stat": "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4\n",
        "/proc/meminfo": "MemTotal:       16000000 kB\nMemAvailable:    4000000 kB\n"
                         "SwapTotal:       2000000 kB\nSwapFree:        1500000 kB\n",
        "/proc/loadavg": "0.50 0.75 1.00 2/345 6789\n",
        "/proc/net/dev": "Inter-|   Receive\n face |bytes\n"
                         "    lo: 999 0 0 0 0 0 0 0 999 0 0 0 0 0 0 0\n"
                         "  eth0: 1000 0 0 0 0 0 0 0 400 0 0 0 0 0 0 0\n"
                         "veth1: 5000 0 0 0 0 0 0 0 5000 0 0 0 0 0 0 0\n",
        "/proc/uptime": "12345.67 99999.00\n",
    }

    def test_le_proc(self):
        files = dict(self.FILES)
        clock = iter([100.0, 102.0])
        b = metrics_linux.Backend()
        with mock.patch.object(metrics_linux, "_read", side_effect=lambda p: files.get(p, "")), \
                mock.patch.object(metrics_linux.time, "monotonic", side_effect=lambda: next(clock)), \
                mock.patch.object(b, "temp", return_value=None), \
                mock.patch.object(b, "battery", return_value=None):
            m = Metrics(backend=b)
            m.sample()
            files["/proc/stat"] = "cpu  150 0 100 850 50 0 0 0 0 0\n"
            files["/proc/net/dev"] = files["/proc/net/dev"].replace("eth0: 1000", "eth0: 3000").replace(
                " 400 0", " 1400 0")
            s = m.sample()
        self.assertAlmostEqual(s.cpu, 100 * (1 - 50 / 150))
        self.assertEqual(s.mem_total, 16000000 * 1024)
        self.assertEqual(s.mem_used, 12000000 * 1024)
        self.assertEqual((s.swap_used, s.swap_total), (500000 * 1024, 2000000 * 1024))
        self.assertEqual(s.load, (0.5, 0.75, 1.0))
        self.assertTrue(s.load_ok)
        self.assertEqual(s.procs, 345)
        self.assertEqual((s.rx_rate, s.tx_rate), (1000.0, 500.0))
        self.assertTrue(s.net_ok)
        self.assertEqual(s.uptime, 12345.67)

    def test_proc_vazio_nao_quebra(self):
        b = metrics_linux.Backend()
        with mock.patch.object(metrics_linux, "_read", return_value=""):
            s = Metrics(backend=b).sample()
        self.assertEqual((s.cpu, s.mem_total, s.procs, s.uptime), (0.0, 0, 0, 0.0))
        self.assertFalse(s.load_ok)

    @unittest.skipUnless(sys.platform.startswith("linux"), "só Linux")
    def test_linux_de_verdade(self):
        m = Metrics(extra_paths=[Path("/")])
        m.sample()
        s = m.sample()
        self.assertGreater(s.mem_total, 0)
        self.assertTrue(s.load_ok and s.net_ok)
        self.assertGreater(s.procs, 0)
        self.assertEqual(s.disks[0].label, "/")
        self.assertEqual(len([d for d in s.disks if d.label == "/"]), 1)


class MacParsersTest(unittest.TestCase):
    def test_vm_stat(self):
        page, pages = mac.parse_vm_stat(VM_STAT)
        self.assertEqual(page, 16384)
        self.assertEqual(pages["Pages free"], 3000)
        self.assertEqual(pages["Translation faults"], 900000000)
        self.assertEqual(pages["Anonymous pages"], 240000)
        total = 16 * 1024 ** 3
        used = mac.vm_used(page, pages, total)
        self.assertEqual(used, (240000 - 10000 + 80000 + 100000) * 16384)
        self.assertEqual(mac.vm_used(page, pages, 1000), 1000)  # limitado ao total
        self.assertEqual(mac.parse_vm_stat(""), (4096, {}))

    def test_swapusage(self):
        self.assertEqual(mac.parse_swapusage(SWAPUSAGE),
                         (int(1024.5 * 1024 ** 2), 2048 * 1024 ** 2))
        self.assertEqual(mac.parse_swapusage("vm.swapusage: total = 0.00M  used = 0.00M  free = 0.00M  "),
                         (0, 0))
        self.assertIsNone(mac.parse_swapusage(""))

    def test_boottime(self):
        self.assertEqual(mac.parse_boottime(BOOTTIME), 1700000000.25)
        self.assertIsNone(mac.parse_boottime("lixo"))

    def test_pmset(self):
        self.assertEqual(mac.parse_pmset_batt(PMSET_DISCHARGING), (85, "Discharging"))
        self.assertEqual(mac.parse_pmset_batt(PMSET_CHARGING), (42, "Charging"))
        self.assertEqual(mac.parse_pmset_batt(PMSET_CHARGED), (100, "Full"))
        self.assertEqual(mac.parse_pmset_batt(PMSET_AC_ATTACHED), (80, "Not charging"))
        self.assertIsNone(mac.parse_pmset_batt(PMSET_DESKTOP))
        self.assertIsNone(mac.parse_pmset_batt(""))

    def test_netstat_ib(self):
        # Só as linhas <Link#N>, sem lo0, gif0, utun0 e awdl0.
        self.assertEqual(mac.parse_netstat_ib(NETSTAT_IB), (1500000000 + 100000, 200000000 + 50000))
        self.assertEqual(mac.parse_netstat_ib(""), (0, 0))

    def test_cache(self):
        calls = []
        c = mac.Cached(lambda: calls.append(1) or len(calls), 60)
        self.assertEqual((c.get(), c.get()), (1, 1))
        c.at -= 61
        self.assertEqual(c.get(), 2)

    def test_run_tolerante(self):
        self.assertEqual(mac.run(["/caminho/que/nao/existe"]), "")

    def test_fallback_por_comando(self):
        outs = {
            (mac.SYSCTL, "-n", "hw.memsize"): "17179869184\n",
            (mac.VM_STAT,): VM_STAT,
            (mac.SYSCTL, "vm.swapusage"): SWAPUSAGE,
            (mac.SYSCTL, "kern.boottime"): BOOTTIME,
            (mac.PS, "-A", "-o", "pid="): "    1\n  100\n  200\n",
            (mac.NETSTAT, "-ib"): NETSTAT_IB,
            (mac.PMSET, "-g", "batt"): PMSET_DISCHARGING,
        }
        with mock.patch.object(mac, "run", side_effect=lambda argv, timeout=1.5: outs.get(tuple(argv), "")):
            b = mac.Backend(lib=False)
            self.assertIsNone(b.cpu_times())
            used, total, sw_used, sw_total = b.memory()
            self.assertEqual(total, 17179869184)
            self.assertEqual(used, (240000 - 10000 + 80000 + 100000) * 16384)
            self.assertEqual(sw_total, 2048 * 1024 ** 2)
            self.assertEqual(b.load_procs()[1], 3)
            rx, tx, at = b.net_bytes()
            self.assertEqual((rx, tx), (1500100000, 200050000))
            self.assertEqual(b.net_bytes()[2], at)  # cache: mesmo instante
            self.assertGreater(b.uptime(), 0)
            self.assertEqual(b.battery(), (85, "Discharging"))
            self.assertIsNone(b.temp())
            s = Metrics(backend=b).sample()
            self.assertTrue(s.net_ok)

    def test_estruturas(self):
        self.assertEqual(ctypes.sizeof(mac.VmStatistics64), 152)
        self.assertEqual(mac.HOST_VM_INFO64_COUNT, 38)
        self.assertEqual(ctypes.sizeof(mac.XswUsage), 32)
        self.assertEqual(ctypes.sizeof(mac.Timeval), 16)
        self.assertEqual(mac.IfData.ifi_ibytes.offset, 40)
        self.assertEqual(mac.IfData.ifi_obytes.offset, 44)


@unittest.skipUnless(sys.platform == "darwin", "só macOS (CI)")
class MacRealTest(unittest.TestCase):
    def test_ctypes_carrega(self):
        b = mac.Backend()
        self.assertIsNotNone(b.lib)
        self.assertGreater(b.page, 0)

    def test_indicadores(self):
        b = mac.Backend()
        idle, total = b.cpu_times()
        self.assertGreater(total, idle)
        used, total, sw_used, sw_total = b.memory()
        self.assertGreater(total, 0)
        self.assertTrue(0 < used <= total)
        self.assertTrue(0 <= sw_used <= max(sw_total, sw_used))
        memsize = mac.run([mac.SYSCTL, "-n", "hw.memsize"]).strip()
        if memsize:
            self.assertEqual(total, int(memsize))
        load, procs = b.load_procs()
        self.assertEqual(len(load), 3)
        self.assertGreater(procs, 0)
        self.assertIsNotNone(b.net_bytes())
        self.assertGreater(b.uptime(), 0)
        batt = b.battery()
        if batt is not None:
            self.assertIn(batt[1], ("Charging", "Discharging", "Full", "Not charging"))

    def test_ctypes_bate_com_os_comandos(self):
        b = mac.Backend()
        fb = mac.Backend(lib=False)
        self.assertEqual(b.memory()[1], fb.memory()[1])
        self.assertAlmostEqual(b.uptime(), fb.uptime(), delta=5)

    def test_metrics(self):
        m = Metrics()
        m.sample()
        s = m.sample()
        self.assertTrue(s.load_ok and s.net_ok)
        self.assertIsNone(s.temp)
        self.assertEqual([d.label for d in s.disks][:1], ["/"])


class WindowsStructsTest(unittest.TestCase):
    """Layout das estruturas (tipos de largura fixa: vale em qualquer sistema)."""

    def test_tamanhos_e_deslocamentos(self):
        self.assertEqual(ctypes.sizeof(win.MemoryStatusEx), 64)
        self.assertEqual(ctypes.sizeof(win.SystemPowerStatus), 12)
        self.assertEqual(ctypes.sizeof(win.Guid), 16)
        self.assertEqual(ctypes.sizeof(win.MibIfRow2), 1352)
        self.assertEqual(win.MibIfRow2.Alias.offset, 28)
        self.assertEqual(win.MibIfRow2.InterfaceAndOperStatusFlags.offset, 1152)
        self.assertEqual(win.MibIfRow2.OperStatus.offset, 1156)
        self.assertEqual(win.MibIfRow2.InOctets.offset, 1208)
        self.assertEqual(win.MibIfRow2.OutOctets.offset, 1280)

    def test_swap_aproximado(self):
        m = win.MemoryStatusEx(ullTotalPhys=16, ullAvailPhys=6,
                               ullTotalPageFile=20, ullAvailPageFile=8)
        self.assertEqual(win.swap_from_memstatus(m), (2, 4))  # commit 12 - RAM 10
        m = win.MemoryStatusEx(ullTotalPhys=16, ullAvailPhys=6,
                               ullTotalPageFile=16, ullAvailPageFile=8)
        self.assertEqual(win.swap_from_memstatus(m), (0, 0))

    def test_bateria(self):
        self.assertEqual(win.battery_from_status(0, 0, 70), (70, "Discharging"))
        self.assertEqual(win.battery_from_status(1, 8, 40), (40, "Charging"))
        self.assertEqual(win.battery_from_status(1, 1, 100), (100, "Full"))
        self.assertEqual(win.battery_from_status(1, 1, 80), (80, "Not charging"))
        self.assertIsNone(win.battery_from_status(1, 128, 255))  # sem bateria
        self.assertIsNone(win.battery_from_status(255, 255, 255))  # desconhecido

    def test_filtro_de_interface(self):
        row = win.MibIfRow2(InterfaceAndOperStatusFlags=1, OperStatus=1, Type=6)
        self.assertTrue(win.row_counts(row))
        row.InterfaceAndOperStatusFlags = 3  # filtro (WFP/QoS): repete o adaptador
        self.assertFalse(win.row_counts(row))
        row.InterfaceAndOperStatusFlags, row.OperStatus = 1, 2  # desligada
        self.assertFalse(win.row_counts(row))
        row.OperStatus, row.InterfaceAndOperStatusFlags = 1, 0  # virtual
        self.assertFalse(win.row_counts(row))

    @unittest.skipIf(sys.platform == "win32", "no Windows as DLLs carregam")
    def test_fora_do_windows_nao_carrega(self):
        b = win.Backend()
        self.assertIsNone(b.k32)
        self.assertIsNone(b.cpu_times())
        self.assertEqual(b.memory(), (0, 0, 0, 0))
        self.assertIsNone(b.net_bytes())


@unittest.skipUnless(sys.platform == "win32", "só Windows (CI)")
class WindowsRealTest(unittest.TestCase):
    def test_indicadores(self):
        b = win.Backend()
        self.assertIsNotNone(b.k32)
        idle, total = b.cpu_times()
        self.assertGreater(total, idle)
        used, total, sw_used, sw_total = b.memory()
        self.assertTrue(0 < used <= total)
        self.assertTrue(0 <= sw_used <= sw_total)
        load, procs = b.load_procs()
        self.assertIsNone(load)
        self.assertGreater(procs, 0)
        self.assertGreater(b.uptime(), 0)
        root = b.root_paths()[0]
        key, used, total = b.disk_usage(root)
        self.assertTrue(0 < used <= total)
        self.assertEqual(b.root_label(root), os.environ.get("SystemDrive", "C:"))
        self.assertIsNotNone(b.iphlp)
        self.assertIsNotNone(b.net_bytes())

    def test_metrics(self):
        m = Metrics()
        m.sample()
        s = m.sample()
        self.assertFalse(s.load_ok)
        self.assertEqual(s.load, (0.0, 0.0, 0.0))
        self.assertTrue(s.net_ok)
        self.assertIsNone(s.temp)
        self.assertTrue(s.disks)
        self.assertEqual(len({d.label for d in s.disks}), len(s.disks))


if __name__ == "__main__":
    unittest.main()
