# -*- mode: python -*-
# Binário autocontido do HUD (PyInstaller --onefile): o interpretador Python, a
# biblioteca padrão que o HUD usa e o módulo _curses (com a libncursesw/libtinfo
# do sistema de build) num executável só. Quem instala não precisa de Python.
#
# Uso: scripts/release.sh <tag> (que chama `python -m PyInstaller packaging/hud.spec`).
# Plataformas: linux amd64 e arm64. O HUD lê /proc e /sys, então macOS fica fora.
import os

ROOT = os.path.abspath(os.path.join(SPECPATH, ".."))

# Módulos que o HUD não usa. Tirá-los deixa o binário menor e evita embutir
# bibliotecas de terceiros (OpenSSL, libffi, SQLite, Tcl/Tk, readline (GPL),
# expat, liblzma, libbz2, libmpdec, libuuid, gdbm (GPL)). O hashlib cai nos
# algoritmos internos do CPython quando _hashlib (OpenSSL) não está presente.
EXCLUDES = [
    "_bz2", "_ctypes", "_curses_panel", "_dbm", "_decimal", "_gdbm", "_hashlib",
    "_lzma", "_sqlite3", "_ssl", "_tkinter", "_uuid", "pyexpat", "readline",
    "bz2", "ctypes", "curses.panel", "dbm", "decimal", "ensurepip", "idlelib",
    "lib2to3", "lzma", "multiprocessing", "pydoc", "pydoc_data", "sqlite3", "ssl",
    "tkinter", "turtle", "turtledemo", "unittest", "uuid", "venv", "xml",
    "xmlrpc", "zoneinfo", "http", "urllib.request", "email",
]

a = Analysis(
    [os.path.join(SPECPATH, "hud_entry.py")],
    pathex=[ROOT],
    binaries=[],
    datas=[],
    hiddenimports=["curses", "_curses"],
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
    name="hud",
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
)
