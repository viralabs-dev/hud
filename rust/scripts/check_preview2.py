"""Synthetic PTY integration: custom geometry/state, doc approval and fake agents.
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

with tempfile.TemporaryDirectory(prefix="hud-preview2-pty-") as folder:
    base=Path(folder); state=base/'agent'; state.mkdir(mode=0o700)
    skills=base/'skills';skills.mkdir(mode=0o700);skill=skills/'sample';skill.mkdir(mode=0o700)
    private_file(skill/'SKILL.md','---\nname: FixtureSkill\ndescription: original description\n---\n')
    usage=base/'claude-sl-usage-fixture.json'
    def cache(used): private_file(usage,json.dumps({'fetched_at':time.time(),'five_hour_used':used,'seven_day_used':30,'five_hour_reset_min':10}))
    cache(20)
    monitor=base/'session.json'
    private_file(monitor,json.dumps({'id':'legacy-parent','directory':str(state),'time':{'updated':20},'title':'SECRET'}))
    proposals=base/'markdown.md'
    private_file(proposals,'# Café 日本語\n**forte** e *leve* `literal`\n| Nome | Valor |\n| --- | --- |\n| 日本語 | 42 |\n> quote\n- item\n')
    config=base/'custom.toml'
    private_file(config, "nome='preview2-fixture'\n[[coluna]]\nlargura=36\npaineis=['sistema','uso_claude','vault']\n[[coluna]]\npaineis=['saida']\n[[bloco]]\nid='sistema'\nmostrar=['cpu']\n[[bloco]]\nid='vault'\nmostrar=['recentes']\n")
    session=Session(['--custom-file',str(config),'--claude-cache',str(usage),'--skills-dir',str(skills),'--monitor-file',str(monitor),'--fake-agent',str(FAKE),'--agent-dir',str(state)])
    try:
        session.text('80.0%')
        session.text('CPU')
        assert 'Memory' not in session.screen.text(), 'show filter'
        session.command('/ajuda');session.text(os.environ.get('HUD_PREVIEW_LABEL','Prévia 2'))
        session.command('/skills');session.text('original description')
        private_file(skill/'SKILL.md','---\nname: FixtureSkill\ndescription: refreshed description\n---\n')
        session.command('resources refresh');session.pump(.2)
        session.command('/skills');session.text('refreshed description')
        cache(50);session.command('resources refresh');session.text('50.0%')
        session.command('/uso');session.text('300 min window')
        private_file(usage,'malformed');session.command('resources refresh');session.pump(.3)
        assert '50.0%' in session.screen.text(), 'retain last valid data'
        session.command('/agentes');session.text('legacy-parent');assert 'SECRET' not in session.screen.text()
        session.command('/perfil completo');session.text('disabled')
        session.command('/x ação 日本語');session.text('Confirm FAKE')
        assert 'fixture response' not in session.screen.text()
        session.command('yes');session.text('fixture response')
        session.command('/novo');session.text('reset explicitly')
        session.command('/sair');session.wait(lambda:session.proc.poll() is not None,'slash quit');session.close()
        print('PASS preview2 aliases/native panel IDs/show/resources refresh+invalid retain/skills/OpenCode metadata/fake confirmation/reset/termios')
    except BaseException:
        session.close(False);raise
