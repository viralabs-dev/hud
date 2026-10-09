"""O que os testes precisam para rodar em Linux, macOS e Windows (CI)."""

import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

WINDOWS = sys.platform == "win32"

posix_only = unittest.skipIf(WINDOWS, "só POSIX (chmod, uid, pty)")
windows_only = unittest.skipUnless(WINDOWS, "só no Windows")

# Um comando que existe em toda máquina de cada sistema, e um que demora.
ECHO = "hostname" if WINDOWS else "echo"
ECHO_ARGV = ["hostname"] if WINDOWS else ["echo", "oi"]  # hostname com argumento tentaria renomear
SLEEP = ["ping", "-n", "30", "127.0.0.1"] if WINDOWS else ["sleep", "30"]


def home_env(home: str) -> dict[str, str]:
    """Variáveis que o expanduser lê: HOME no POSIX, USERPROFILE no Windows."""
    return {"HOME": home, "USERPROFILE": home}


def symlink(tc: unittest.TestCase, src, dst) -> None:
    """os.symlink, ou pula o teste onde criar link não é permitido (Windows sem privilégio)."""
    try:
        os.symlink(src, dst, target_is_directory=os.path.isdir(src))
    except (OSError, NotImplementedError) as e:
        tc.skipTest(f"sem permissão para criar link simbólico: {e}")


_FAKE = None


def fake_executable(name: str) -> str:
    """Executável falso (claude, codex) numa pasta só sua: script no POSIX,
    uma cópia do hostname.exe no Windows (lá só .exe roda direto)."""
    global _FAKE
    if _FAKE is None:
        _FAKE = tempfile.mkdtemp(prefix="hud-agentes-")
        os.chmod(_FAKE, 0o700)
    if WINDOWS:
        exe = Path(_FAKE) / f"{name}.exe"
        if not exe.exists():
            root = os.environ.get("SystemRoot", r"C:\Windows")
            shutil.copyfile(os.path.join(root, "System32", "hostname.exe"), exe)
        return str(exe)
    exe = Path(_FAKE) / name
    if not exe.exists():
        exe.write_text("#!/bin/sh\nexit 0\n")
        os.chmod(exe, 0o700)
    return str(exe)
