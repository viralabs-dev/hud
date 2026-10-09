# -*- mode: python -*-
# Binário autocontido do HUD (PyInstaller --onefile): o interpretador Python, a
# biblioteca padrão que o HUD usa e o módulo _curses num executável só. Quem
# instala não precisa de Python.
#
# Uso: scripts/release.sh <tag> (que chama `python -m PyInstaller packaging/hud.spec`).
# Plataformas (o PyInstaller não faz build cruzado; cada uma é montada na própria):
#   linux   amd64/arm64  _curses do CPython + libncursesw/libtinfo do sistema
#   darwin  arm64/amd64  _curses do Python.org + o ncurses que vem com ele
#   windows amd64        _curses do windows-curses (PDCurses ligado estaticamente
#                        na própria .pyd; o wheel não traz DLL separada)
# O que cada binário embute está em THIRD_PARTY_NOTICES.md; packaging/bibliotecas.py
# confere, depois do build, que toda biblioteca embutida tem licença em LICENSES/.
import os
import sys

ROOT = os.path.abspath(os.path.join(SPECPATH, ".."))

if sys.platform.startswith("linux"):
    PLATAFORMA = "linux"
elif sys.platform == "darwin":
    PLATAFORMA = "darwin"
elif sys.platform == "win32":
    PLATAFORMA = "windows"
else:
    raise SystemExit(f"hud.spec: plataforma sem binário: {sys.platform}")

# Módulos que o HUD não usa. Tirá-los deixa o binário menor e evita embutir
# bibliotecas de terceiros (OpenSSL, SQLite, Tcl/Tk, readline (GPL), expat,
# liblzma, libbz2, libmpdec, libuuid, gdbm (GPL)). O hashlib cai nos algoritmos
# internos do CPython quando _hashlib (OpenSSL) não está presente.
EXCLUDES = [
    "_bz2", "_dbm", "_decimal", "_gdbm", "_hashlib",
    "_lzma", "_sqlite3", "_ssl", "_tkinter", "_uuid", "pyexpat", "readline",
    "bz2", "dbm", "decimal", "ensurepip", "idlelib",
    "lib2to3", "lzma", "multiprocessing", "pydoc", "pydoc_data", "sqlite3", "ssl",
    "tkinter", "turtle", "turtledemo", "unittest", "uuid", "venv", "xml",
    "xmlrpc", "zoneinfo", "http", "urllib.request", "email",
]
HIDDEN = ["curses", "_curses"]

if PLATAFORMA == "linux":
    # No Linux os indicadores vêm de /proc e /sys: sem ctypes (e sem libffi).
    # O painel do curses também fica fora (evita a libpanelw).
    EXCLUDES += ["_ctypes", "ctypes", "_curses_panel", "curses.panel"]
elif PLATAFORMA == "darwin":
    # hud/metrics.py usa ctypes (sysctl, mach) no macOS. O _ctypes do Python.org
    # usa a libffi do sistema (/usr/lib), que não é embutida.
    EXCLUDES += ["_curses_panel", "curses.panel"]
    HIDDEN += ["ctypes", "ctypes.util"]
else:
    # hud/metrics.py usa ctypes (kernel32, psapi) no Windows; o _ctypes do
    # Python.org traz a libffi-8.dll. O windows-curses tem _curses e
    # _curses_panel (as duas .pyd com o PDCurses estático).
    HIDDEN += ["ctypes", "ctypes.wintypes", "_curses_panel", "curses.panel"]

a = Analysis(
    [os.path.join(SPECPATH, "hud_entry.py")],
    pathex=[ROOT],
    binaries=[],
    datas=[],
    hiddenimports=HIDDEN,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[os.path.join(SPECPATH, "rthook_ambiente.py")],
    excludes=EXCLUDES,
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="hud",  # no Windows o PyInstaller acrescenta .exe
    debug=False,
    # No onefile há dois processos: o bootloader (pai) e o Python (filho), no
    # mesmo grupo. O Ctrl+C do terminal já chega aos dois; se o pai também
    # repassasse o SIGINT, o HUD levaria dois KeyboardInterrupt e sairia com
    # traceback no meio da limpeza. Com True, o pai ignora e só espera o filho.
    bootloader_ignore_signals=True,
    strip=False,
    upx=False,
    runtime_tmpdir=None,
    console=True,
    # macOS: binário da arquitetura do Python que monta (o Python.org é
    # universal2, mas cada arquitetura é montada e testada no próprio runner).
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
