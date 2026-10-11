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

with tempfile.TemporaryDirectory(prefix="hud-extended-pty-") as folder:
    base = Path(folder)
    custom_state, docs, agent = [base / name for name in ("custom-state", "docs", "agent")]
    for directory in (custom_state, docs, agent): directory.mkdir(mode=0o700)
    custom = {"version":1,"name":"pty-fixture-metadata","columns":[
        {"width":35,"slots":[{"id":"system","height":11},{"id":"note","weight":1}]},
        {"slots":[{"id":"vault","weight":1},{"id":"output","weight":1}]}],
        "panels":[{"id":"note","title":"Fixture note","type":"text","text":"ação 日本語","lines":2}]}
    config = base / "custom-input.json"
    private_file(config, json.dumps(custom, ensure_ascii=False))
    original_config = config.read_bytes()
    proposals = base / "proposals.txt"
    content = "ação 日本語\n[[Link\\|alias]]\n"
    private_file(proposals, '````hud-doc arquivo="Nota.md"\n'+content+'````\n')
    original_proposal = proposals.read_bytes()
    args = ["--custom-file",str(config),"--custom-state-dir",str(custom_state),
            "--doc-root",str(docs),"--doc-proposals",str(proposals),
            "--fake-agent",str(FAKE),"--agent-dir",str(agent)]
    session = Session(args)
    try:
        session.text("Fixture note")
        assert session.screen.find("system")[0] == 0
        assert session.screen.find("Fixture note")[0] == 11, "fixed panel height"
        assert session.screen.find("vault")[1] == 45, "35 percent column width plus padded border title"
        session.mouse(0, 3, 0)
        session.text("Dragging title")
        session.mouse(32, 60, 14)
        session.text("DROP HERE")
        session.mouse(0, 60, 14, True)
        session.text("Custom layout and metadata saved")
        saved = json.loads((custom_state/"custom.json").read_text())
        assert saved["name"] == custom["name"]
        for expected in custom["panels"]:
            actual = next(panel for panel in saved["panels"] if panel["id"] == expected["id"])
            assert all(actual[key] == value for key,value in expected.items()), "all original feed metadata preserved"
        assert saved["columns"][0]["width"] == 35
        assert saved["columns"][0]["slots"][0]["id"] == "vault"
        assert saved["columns"][1]["slots"][0]["id"] == "system"
        # Focus and emphasize output before approvals (focus changes revoke approvals).
        session.mouse(0, 65, 40)
        session.send(b"\x1bz")
        session.wait(lambda: session.screen.find("output") and not session.screen.find("Fixture note"), "output emphasis")
        session.command("doc preview 1")
        session.text("Document preview: Nota.md")
        session.text("New file")
        assert not (docs/"Nota.md").exists(), "preview must never write"
        session.command("doc save")
        session.text("Document saved: Nota.md")
        assert (docs/"Nota.md").read_text() == content
        for gesture in ("escape", "focus", "unrelated"):
            session.command("doc preview 1")
            session.text("Explicit approval: doc overwrite")
            if gesture == "escape": session.send(b"\x1b")
            elif gesture == "focus": session.send(b"\t")
            else:
                session.command("agent status")
                session.text("session=")
            session.command("doc overwrite")
            # Restore output emphasis after Escape/Tab revoked approval, before checking output.
            if gesture != "unrelated":
                session.send(b"\x1b")
                session.mouse(0, 65, 40)
                session.send(b"\x1bz")
            session.text("No pending document preview")
            assert (docs/"Nota.md").read_text() == content
        for provider in ("codex", "claude", "opencode"):
            session.command("agent "+provider)
            session.command("ask fixture 日本語")
            session.text("Confirm FAKE")
            early_before = session.screen.text().count("early")
            session.command("yes")
            session.wait(lambda: session.screen.text().count("early") > early_before, "new fake streams before exit", timeout=2)
            session.text("session=fixture-session")
            session.command("ask second fixture")
            session.text("Confirm FAKE")
            session.command("yes")
            session.text("resumed")
            session.text("cancelled=false")
            session.command("ask cancelled fixture")
            session.text("Confirm FAKE")
            session.command("yes")
            session.command("cancel")
            session.command("agent status")
            session.text("cancelled=true")
        # Late agent poll must not overwrite a document preview source.
        session.command("ask final fixture")
        session.text("Confirm FAKE")
        session.command("yes")
        session.text("early")
        session.command("doc preview 1")
        session.text("Document preview: Nota.md")
        session.pump(0.8)
        assert "Document preview: Nota.md" in session.screen.text(), "agent poll replaced document preview"
        assert (docs/"Nota.md").read_text() == content
        assert config.read_bytes() == original_config and proposals.read_bytes() == original_proposal
    finally:
        session.close(check=sys.exc_info()[0] is None)
    # Restart with selected input plus state: persisted order takes precedence, metadata intact.
    session = Session(args)
    try:
        session.text("Fixture note")
        assert session.screen.find("vault")[1] < 42
        assert session.screen.find("system")[1] > 42
        assert json.loads((custom_state/"custom.json").read_text()) == saved
    finally:
        session.close(check=sys.exc_info()[0] is None)
print("EXTENDED PTY OK: custom width/height, drag/state/restart metadata, doc create and approval revocation, all three fake streams/session/cancel, output source isolation, terminal restore")
