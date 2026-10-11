"""Adapter settings PTY regression: private synthetic TOML, skill, project, folder and fake profiles.
Never validates a physical terminal; uses only private temporary fixtures.
Usage: HUD_RUST_ROOT=/path/hud python3 check_extended.py /path/hud-rs
"""
import hashlib
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

with tempfile.TemporaryDirectory(prefix="hud-subscription-pty-") as directory:
    base=Path(directory);base.chmod(0o700)
    vault,data,home=[base/name for name in ["vault","data","home"]]
    for folder in [vault,data,home]:folder.mkdir(mode=0o700)
    fixture=base/"contract-fixture";shutil.copyfile(FAKE,fixture);fixture.chmod(0o700)
    digest=hashlib.sha256(fixture.read_bytes()).hexdigest()
    entries=[]
    for p,v in [("codex","codex-cli 0.160.1"),("claude","2.1.296 (Claude Code)")]:
        entries.append({"provider":p,"executable":str(fixture),"sha256":digest,"version_output":v,"subscription":{"mode":"existing_subscription","home":str(home),"extra_usage":"unknown","managed_policy":"reviewed_codex_subscription_configuration" if p=="codex" else "reviewed_no_executable_hooks","reviewed_cwd":str(vault)}})
    manifest=base/"launch.json";private_file(manifest,json.dumps({"version":1,"providers":entries}))
    os.environ["HOME"]=str(home)
    for key in ["OPENAI_API_KEY","ANTHROPIC_API_KEY","CODEX_TOKEN","OPENCODE_API_KEY","HUD_SUPERVISOR_SECRET"]:os.environ[key]="synthetic-canary-only"
    args=["--vault-default-dir",str(vault),"--data-dir",str(data),"--real-agent-manifest",str(manifest),"--agent-dir",str(vault)]
    session=Session(args)
    try:
        session.text("SAÍDA");session.text("CLIs verificadas");session.send(b"\x1b3")
        session.command("ask blocked");session.text("agent detect")
        private_file(home/"hud-auth-fixture.txt","codex-cli 0.160.1\nLogged in using ChatGPT")
        session.command("agent detect");session.text("no prompt/login")
        session.command("yes");session.text("Auth metadata: Subscription")
        session.command("ask billing blocked");session.text("cobrança extra desconhecida")
        assert not session.screen.find("fixture response")
    finally:session.close()
    for entry in entries:entry["subscription"]["extra_usage"]="disabled_confirmed_by_user"
    private_file(manifest,json.dumps({"version":1,"providers":entries}))
    session=Session(args)
    try:
        session.text("SAÍDA");session.text("CLIs verificadas")
        for provider,key,version,status in [("Codex",b"\x1b3","codex-cli 0.160.1","Logged in using ChatGPT"),("Claude",b"\x1b2","2.1.296 (Claude Code)",json.dumps({"loggedIn":True,"authMethod":"claude.ai","token":"CANARY-DO-NOT-DISPLAY"}))]:
            private_file(home/"hud-auth-fixture.txt",version+"\n"+status)
            session.send(key);session.command("agent detect");session.text("no prompt/login");session.command("yes");session.text("Auth metadata: Subscription")
            assert "CANARY-DO-NOT-DISPLAY" not in session.screen.text()
            session.command("ask synthetic subscription");session.text("yes starts process")
            session.command("agent detect");session.text("no prompt/login");session.command("yes");session.text("Auth metadata: Subscription")
            session.command("yes");session.pump(.2)
            assert "fixture response" not in session.screen.text(),"auth confirmation cannot launch revoked prompt"
            session.command("ask synthetic subscription");session.command("yes");session.text("fixture response");session.text("REAL finished")
            session.command("ask synthetic resume");session.command("yes");session.text("resumed")
        assert not (data/"notas.md").exists()
    finally:session.close()
print("subscription PTY: explicit version/auth detection, unknown billing block, two synthetic providers, revoked prompt, canary filtering, resume and terminal cleanup passed")
