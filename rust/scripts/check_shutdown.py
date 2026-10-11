"""Linux PTY regression: normal HUD exit restores terminal and reaps active fixture.
Uses a private copy of sleep; never executes a real provider or user command file.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time


def check(binary):
    with tempfile.TemporaryDirectory(prefix="hud-shutdown-review-") as temporary:
        root = Path(temporary)
        fixture = root / "wait-fixture"
        shutil.copyfile(shutil.which("sleep") or "/usr/bin/sleep", fixture)
        fixture.chmod(0o700)
        config = root / "commands.json"
        config.write_text(json.dumps([{"name": "Wait", "argv": [str(fixture), "30"],
                                     "cwd": str(root), "timeout_seconds": 40}]))
        config.chmod(0o600)
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
        original = termios.tcgetattr(slave)
        process = subprocess.Popen([str(binary), "--command-file", str(config)],
                                   stdin=slave, stdout=slave, stderr=slave,
                                   env=dict(os.environ, TERM="xterm-256color"))
        children = set()
        try:
            deadline = time.monotonic() + 5
            initial = b""
            while b"Wait" not in initial:
                assert process.poll() is None, "HUD exited before interaction"
                assert time.monotonic() < deadline, "no ready command panel"
                if select.select([master], [], [], .05)[0]:
                    initial += os.read(master, 65536)
            os.write(master, b":run Wait\r")
            while not children:
                assert time.monotonic() < deadline, "fixture was not spawned"
                for path in Path(f"/proc/{process.pid}/task").glob("*/children"):
                    children.update(int(value) for value in path.read_text().split())
                if select.select([master], [], [], .01)[0]:
                    os.read(master, 65536)
            os.write(master, b"\x03")
            assert process.wait(timeout=3) == 0
            assert termios.tcgetattr(slave) == original, "terminal modes not restored"
            remaining = [pid for pid in children if Path(f"/proc/{pid}").exists()]
            assert not remaining, f"active command survived normal quit: {remaining}"
            print("normal quit: active fixture reaped; terminal modes restored")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            for pid in children:
                path = Path(f"/proc/{pid}/cmdline")
                try:
                    if path.read_bytes().split(b"\0")[0] == os.fsencode(fixture):
                        os.kill(pid, signal.SIGKILL)
                except (FileNotFoundError, ProcessLookupError):
                    pass
            os.close(master)
            os.close(slave)


if __name__ == "__main__":
    check(Path(sys.argv[1]).resolve())
