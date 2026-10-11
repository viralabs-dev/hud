"""Workflow PTY regression: private synthetic TOML, skill, project, folder and fake profiles.
Never validates a physical terminal; uses only private temporary fixtures.
Usage: HUD_RUST_ROOT=/path/hud python3 check_extended.py /path/hud-rs
"""
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

with tempfile.TemporaryDirectory(prefix="hud-workflows-pty-") as directory:
    base=Path(directory);base.chmod(0o700)
    vault,other,data,agent,skills=[base/name for name in ["vault","other","data","agent","skills"]]
    for folder in [vault,other,data,agent,skills]:folder.mkdir(mode=0o700)
    skill=skills/"projeto-docs";skill.mkdir(mode=0o700)
    private_file(skill/"SKILL.md","---\nname: projeto-docs\ndescription: workflow fixture\n---\nInstruções completas 日本語\n")
    backlog=vault/"projects"/"Hud"/"06-backlog";backlog.mkdir(parents=True)
    private_file(backlog/"Kanban (Hud).md","# Kanban (Hud)\n\n## A fazer\n- [ ] fixture\n")
    original=(backlog/"Kanban (Hud).md").read_bytes()
    commands=base/"comandos.toml"
    private_file(commands,''.join(f"[[command]]\nname='item-{10-i:02}'\nargv=['/usr/bin/printf','never approved']\nconfirm=true\n" for i in range(10)))
    config=base/"config.toml";private_file(config,"[[command]]\nname='ignored sibling'\nargv=['/usr/bin/printf']\nconfirm=true\n")
    args=["--vault-default-dir",str(vault),"--data-dir",str(data),"--skills-dir",str(skills),"--command-file",str(config),"--fake-agent",str(FAKE),"--agent-dir",str(agent)]
    session=Session(args)
    try:
        session.text("SAÍDA");session.pump(.5)
        session.send(b"\x1bOP");session.text("Confirm item-10");session.command("cancel")
        session.send(b"\x1b[21~");session.text("Confirm item-01");session.command("cancel")
        assert not (data/"notas.md").exists(), "commands were never approved"
        session.command("/skill PROJETO-DOCS analisar 日本語");session.text("Confirm FAKE");session.command("yes");session.text("session=fixture-session");session.pump(.65)
        session.command("/project lista");session.text("Hud · projects/Hud")
        session.command("/project analisar");session.text("Confirm FAKE");session.command("yes");session.text("session=fixture-session");session.pump(.65)
        session.send(b"\x1b2");session.command("/perfil completo codex");session.text("FAKE Codex: perfil Full")
        session.command("pergunta normal");session.text("Confirm FAKE Claude")
        session.command("/pasta "+str(other));session.text("Workflow recusado");assert not (data/"pasta.state").exists()
        session.command("cancel");session.command("/pasta /etc");session.text("sistema recusada")
        session.command("/pasta fixo codex");session.text("pasta fixo nesta sessão")
        relative=os.path.relpath(other,Path.home());session.command("/pasta ~/"+relative)
        session.wait(lambda:(data/"pasta.state").exists() and json.loads((data/"pasta.state").read_text())==str(other),"remembered expanded folder")
        session.command("/pasta");session.text("/pasta caminho")
        session.close();session=Session(args);session.text("SAÍDA");session.command("/pasta");session.text("other")
        session.command("/pasta vault");session.wait(lambda:json.loads((data/"pasta.state").read_text()) is None,"return default clears remembered selection")
        session.send(b"\x1b3");session.command("/perfil completo");session.text("FAKE Codex: perfil Full")
        session.command("fixture full");session.text("Confirm FAKE Codex");session.command("yes");session.text("session=fixture-session");session.pump(.65)
        assert (backlog/"Kanban (Hud).md").read_bytes()==original
        assert not (data/"notas.md").exists()
        print("PASS workflows PTY: Python TOML precedence/F1/F10 no command approval, complete skill/project confirmation fake, profile target/destination/Full fake, busy pending refusal, system/~ folder, private remembered restart/default, terminal restore")
    finally:session.close()
