"""Adapter settings PTY regression: private synthetic TOML, skill, project, folder and fake profiles.
Never validates a physical terminal; uses only private temporary fixtures.
Usage: HUD_RUST_ROOT=/path/hud python3 check_extended.py /path/hud-rs
"""
import shutil
import fcntl
import json
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

ROOT = Path(os.environ.get("HUD_RUST_ROOT", Path(__file__).resolve().parents[2])).resolve()
sys.path.insert(0, str(ROOT))
from tests.vt import Screen
BINARY = Path(sys.argv[1] if len(sys.argv) > 1 else os.environ.get("HUD_RUST_BINARY", ROOT / "rust/target/debug/hud-rs")).resolve()
FAKE = Path(os.environ.get("HUD_FAKE_AGENT", ROOT / "rust/target/debug/fake_agent")).resolve()
ROWS, COLS = 60, 120

class Session:
    def __init__(self, args):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.original = termios.tcgetattr(self.slave)
        self.screen = Screen(ROWS, COLS)
        self.proc = subprocess.Popen([str(BINARY), *args], stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                     env=dict(os.environ, TERM="xterm-256color", LANG="C.UTF-8"), start_new_session=True)
    def pump(self, seconds=0.05):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if select.select([self.master], [], [], min(0.05, max(0, end-time.monotonic())))[0]:
                self.screen.feed(os.read(self.master, 65536))
    def wait(self, predicate, label, timeout=5):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if predicate(): return
            self.pump()
        raise AssertionError(label + "\n" + self.screen.text())
    def text(self, needle, label=None, timeout=5):
        self.wait(lambda: needle in self.screen.text(), label or needle, timeout)
    def send(self, data):
        os.write(self.master, data)
        if data == b"\x1b": self.pump(0.18)
    def command(self, text):
        self.send((":"+text+"\r").encode())
    def mouse(self, code, col, row, release=False):
        self.send(f"\x1b[<{code};{col+1};{row+1}{'m' if release else 'M'}".encode())
    def close(self, check=True):
        try:
            if self.proc.poll() is None:
                self.send(b"\x03")
                self.wait(lambda: self.proc.poll() is not None, "clean exit")
            if check: assert self.proc.returncode == 0
            assert termios.tcgetattr(self.slave) == self.original, "terminal settings restored"
        finally:
            if self.proc.poll() is None:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait()
            os.close(self.master)
            os.close(self.slave)

def private_file(path, data):
    path.write_text(data)
    path.chmod(0o600)

with tempfile.TemporaryDirectory(prefix="hud-settings-pty-") as directory:
    base=Path(directory);base.chmod(0o700)
    vault,other,data=[base/name for name in ["vault","other","data"]]
    for folder in [vault,other,data]:folder.mkdir(mode=0o700)
    selected_fake=base/"verified-fake"
    shutil.copyfile(FAKE,selected_fake);selected_fake.chmod(0o700)
    settings=base/"settings.toml"
    private_file(settings, f"""[claude]
executable = "/nonexistent/claude"
model = "stream"
tools = ["Read", "Glob"]
profile = "completo"
full_permission_mode = "plan"
max_budget_usd = 1.25
timeout = 17
[codex]
executable = "/nonexistent/codex"
model = "stream"
timeout = 19
[opencode]
executable = "/nonexistent/opencode"
cwd = "{vault}"
enabled = false
[context]
custom_root = "{vault}"
templates_root = "{vault}"
""")
    context=base/"context.md";private_file(context,"Complete initial fixture 日本語\n")
    args=["--vault-default-dir",str(vault),"--data-dir",str(data),"--fake-agent",str(selected_fake),"--agent-dir",str(vault),"--agent-config",str(settings),"--agent-context",str(context)]
    session=Session(args)
    try:
        session.text("SAÍDA");session.send(b"\x1b2");session.command("ask fixture settings")
        session.text("timeout=17s");session.text("budget=1.25");session.text("mode=plan");session.text("Confirm FAKE Claude")
        assert not session.screen.find("session=fixture-session")
        session.command("yes");session.text("session=fixture-session");session.pump(.7)
        session.command("/pasta "+str(other));session.text("pasta")
        session.command("ask after folder");session.text("Confirm FAKE Claude");session.command("yes");session.text("session=fixture-session");session.pump(.7)
        assert not session.screen.find("resumed"),"folder change resets follower session"
        session.send(b"\x1b3");session.command("ask codex settings");session.text("timeout=19s");session.command("yes");session.text("session=fixture-session");session.pump(.7)
        session.send(b"\x1b4");session.command("ask must not execute");session.text("disabled by explicit settings")
        assert not (data/"notas.md").exists()
        print("PASS private TOML settings via CLI/TUI: per-tab budget/mode/timeout, three contexts, followers reset, disabled provider, only verified fake, restore")
    finally:session.close()
