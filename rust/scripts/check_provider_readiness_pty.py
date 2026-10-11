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

# AT096: fixtures only. No native CLI/auth discovery or provider prompts.
with tempfile.TemporaryDirectory(prefix="hud-provider-readiness-") as directory:
    base=Path(directory);base.chmod(0o700)
    vault,data,home=[base/name for name in ["vault","data","home"]]
    for folder in [vault,data,home]:folder.mkdir(mode=0o700)
    plain=["--vault-default-dir",str(vault),"--data-dir",str(data)]
    session=Session(plain)
    try:
        session.text("SAÍDA")
        for name,command in [("Claude","/c hello"),("Codex","/x hello"),("OpenCode","/o hello")]:
            session.command(command);session.text(name+": backend não configurado");session.text("--real-agent-manifest");session.text("--agent-dir")
        session.command("/agentes");session.text("Claude:");session.text("Codex:");session.text("OpenCode:");session.text("auth Unknown")
    finally:session.close()
    fixture=base/"native-fixture";shutil.copyfile(FAKE,fixture);fixture.chmod(0o700)
    digest=hashlib.sha256(fixture.read_bytes()).hexdigest();rows=[]
    for provider,version in [("claude","2.1.296 (Claude Code)"),("codex","codex-cli 0.160.1"),("opencode","1.19.0")]:
        row={"provider":provider,"executable":str(fixture),"sha256":digest,"version_output":version}
        if provider!="opencode":row["subscription"]={"mode":"existing_subscription","home":str(home),"extra_usage":"unknown","managed_policy":"unknown"}
        rows.append(row)
    manifest=base/"manifest.json";private_file(manifest,json.dumps({"version":1,"providers":rows}))
    session=Session(plain+["--real-agent-manifest",str(manifest),"--agent-dir",str(vault)])
    try:
        session.text("SAÍDA");session.text("CLIs verificadas")
        for name,key,policy in [("Claude",b"\x1b2","política Claude não revisada"),("Codex",b"\x1b3","configuração Codex não revisada")]:
            session.send(key);session.text(name+": indisponível para prompt");session.text("autenticação ainda não verificada");session.text("cobrança extra desconhecida");session.text(policy)
            session.command("ask blocked");session.text("autenticação ainda não verificada");assert "fixture response" not in session.screen.text()
        session.send(b"\x1b4");session.text("OpenCode: assinatura nativa não suportada");session.command("ask blocked");session.text("assinatura nativa não suportada")
        assert not (home/"hud-auth-fixture.txt").exists(),"startup does not need any auth probe fixture"
        session.command("/agentes");session.text("Claude:");session.text("Codex:");session.text("OpenCode:");session.text("Backend REAL verificado")
    finally:session.close()
print("provider readiness PTY: missing backend /c /x /o, per-provider auth/billing/policy and unsupported OpenCode inventory passed; no probes/prompts")
