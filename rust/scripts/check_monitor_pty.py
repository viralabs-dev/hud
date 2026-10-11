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

import sqlite3
with tempfile.TemporaryDirectory(prefix='hud-sqlite-monitor-pty-') as folder:
    base=Path(folder);root=base/'roots';proc=base/'proc'
    for directory in [root,root/'codex',root/'codex'/'sessions',root/'claude',root/'opencode',root/'opencode'/'storage',root/'opencode'/'storage'/'session',root/'opencode'/'storage'/'session'/'project',proc,proc/'10',proc/'11']:directory.mkdir(mode=0o700)
    private_file(root/'codex'/'sessions'/'rollout-fixture.jsonl',json.dumps({'type':'session_meta','payload':{'id':'codex-fixture','cwd':'/tmp/codex-project'}})+'\n')
    private_file(root/'claude'/'session.jsonl',json.dumps({'type':'user','sessionId':'claude-fixture','cwd':'/tmp/claude-project'})+'\n')
    private_file(root/'opencode'/'auth.json','SECRET_CREDENTIAL_FIXTURE')
    private_file(root/'opencode'/'storage'/'session'/'project'/'legacy.json',json.dumps({'id':'legacy-fixture','directory':'/tmp/legacy-project','time':{'updated':int(time.time()*1000)}}))
    for pid,comm in [(10,'codex'),(11,'unrelated')]:
        private_file(proc/str(pid)/'comm',comm+'\n');private_file(proc/str(pid)/'stat',f'{pid} (fixture) S 0\n')
        private_file(proc/str(pid)/'environ','SECRET_ENV');private_file(proc/str(pid)/'cmdline','SECRET_ARGUMENT')
    db=root/'opencode'/'opencode.db'
    connection=sqlite3.connect(db);connection.execute('CREATE TABLE session(id TEXT,parent_id TEXT,directory TEXT,title TEXT,time_created INTEGER,time_updated INTEGER,time_archived INTEGER)')
    now=int(time.time()*1000)
    connection.executemany('INSERT INTO session VALUES(?,?,?,?,?,?,?)',[('sqlite-parent',None,'/tmp/sqlite-project','SECRET_TITLE',now,now,None),('sqlite-child','sqlite-parent','/tmp/sqlite-project','SECRET_CHILD',now,now,None),('old-row',None,'/tmp/old-project','SECRET_OLD',now-4000000,now-4000000,None)])
    connection.execute('CREATE TABLE message(data TEXT)');connection.execute("INSERT INTO message VALUES('SECRET_PROMPT')");connection.commit();connection.close();db.chmod(0o600)
    config=base/'custom.toml';private_file(config,"nome='monitor-fixture'\n[[coluna]]\nlargura=40\npaineis=['agentes']\n[[coluna]]\npaineis=['saida']\n[[bloco]]\nid='agentes'\nmostrar=['processos','sessoes']\n")
    original=db.read_bytes()
    session=Session(['--custom-file',str(config),'--monitor-root',str(root),'--monitor-opencode-db',str(db),'--monitor-process-root',str(proc)])
    closed=False
    try:
        session.text('sqlite-parent');session.text('child sqlite-child');session.text('PID 10');session.text('claude-fixture');session.text('legacy-fixture')
        assert 'SECRET' not in session.screen.text() and 'old-row' not in session.screen.text()
        assert db.read_bytes()==original and not db.with_name('opencode.db-shm').exists()
        session.command('/agentes');session.text('Project /tmp/sqlite-project');session.text('4 sessions')
        # Change only explicitly selected fixture; next periodic worker refresh must update the UI.
        connection=sqlite3.connect(db);connection.execute('INSERT INTO session VALUES(?,?,?,?,?,?,?)',('refresh-row',None,'/tmp/refreshed-project','SECRET_REFRESH',now,now+1,None));connection.commit();connection.close()
        session.text('refresh-row',timeout=8)
        refreshed=db.read_bytes();assert 'SECRET' not in session.screen.text()
        db.write_bytes(b'malformed SECRET');session.text('Read error Some',timeout=8);assert 'refresh-row' in session.screen.text(), 'last valid snapshot retained'
        assert db.read_bytes()==b'malformed SECRET', 'monitor must not modify selected fixture'
        session.command('/sair');session.wait(lambda:session.proc.poll() is not None,'slash quit');session.close();closed=True
        print('PASS SQLite/explicit discovery/provider+project grouping/children/window/processes/secret omission/source unchanged/periodic refresh/error retain/shutdown+termios')
    finally:
        if not closed:session.close(False)
