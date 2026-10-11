"""Headless synthetic mouse/keyboard test. Not physical terminal validation.
Uses the existing Python VT emulator as a migration reference test helper.
"""
import json
import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from tests.vt import Screen

binary = Path(os.environ.get("HUD_RUST_BINARY", str(ROOT / "rust/target/debug/hud-rs"))).resolve()
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
original = termios.tcgetattr(slave)
screen = Screen(24, 80)
env = dict(os.environ, TERM="xterm-256color", LANG="C.UTF-8")
fixture = tempfile.TemporaryDirectory(prefix="hud-rust-pty-")
state_dir = str(Path(fixture.name) / "state")
command_file=Path(fixture.name)/"commands.json"
command_file.write_text(json.dumps([
    {"name":"Greeting","argv":["/usr/bin/printf","%s\\n","Hello fixture 日本語"],"cwd":fixture.name,"confirm":True},
    {"name":"Slow","argv":["/usr/bin/sleep","2"],"cwd":fixture.name,"timeout_seconds":5},
]))
command_file.chmod(0o600)
vault_dir=Path(fixture.name)/"vault"
vault_dir.mkdir()
note=vault_dir/"fixture.md"
note.write_text("# Fixture\n- [ ] 2026-10-11 item\n")
original_note=note.read_bytes()
proc = subprocess.Popen([str(binary), "--state-dir", state_dir, "--data-dir", state_dir,"--command-file",str(command_file),"--vault-dir",str(vault_dir)], stdin=slave, stdout=slave, stderr=slave,
                        env=env, start_new_session=True)
def wait(predicate, label):
    until = time.monotonic() + 15
    while time.monotonic() < until:
        if predicate():
            return
        if select.select([master], [], [], 0.05)[0]:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            screen.feed(data)
    raise AssertionError(label + "\n" + screen.text())
def send(data):
    os.write(master, data)
    if data == b"\x1b":
        time.sleep(0.15)  # allow standalone Esc decoding before another sequence
try:
    wait(lambda: screen.find("system") and screen.find("vault"), "initial panels")
    origin = screen.find("system")
    send(b"\x1b[<0;4;1M")
    wait(lambda: screen.find("Dragging title"), "press on title starts drag")
    send(b"\x1b[<32;51;6M")
    wait(lambda: screen.find("DROP HERE"), "motion visibly previews destination")
    assert screen.find("system")[1] == origin[1]
    send(b"\x1b[<0;51;6m")
    wait(lambda: screen.find("system") and screen.find("system")[1] > 35, "release swaps panels")
    send(b"\x1bz")
    wait(lambda: not screen.find("agenda"), "Alt+Z emphasis")
    send(b"\x1b")
    wait(lambda: screen.find("agenda"), "Esc restores layout")
    send(b"\t")
    send(b"\x1bz")
    wait(lambda: screen.find("output") and not screen.find("system"), "Tab changes focused panel before emphasis")
    send(b"\x1b")
    wait(lambda: screen.find("system"), "Esc restores all panels")
    send(b"\x1b[Z")
    send(b"\x1bz")
    wait(lambda: screen.find("system") and not screen.find("output"), "Shift-Tab returns focus")
    send(b"\x1b")
    wait(lambda: screen.find("output"), "Esc restores again")
    send(b"\x1b[<0;51;1M")
    wait(lambda: screen.find("Dragging title"), "restart drag before resize")
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 20, 0, 0))
    os.killpg(proc.pid, signal.SIGWINCH)
    screen = Screen(8,20)
    wait(lambda: "Terminal too small" in screen.text(), "small resize is rendered")
    send(b"\x1b[<0;5;5m")
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    os.killpg(proc.pid, signal.SIGWINCH)
    screen = Screen(24,80)
    wait(lambda: screen.find("system") and screen.find("system")[1] > 35, "resize cancelled stale drag without changing layout")
    send(":agenda add 2026-10-11 14:30 revisão 日本語\r".encode())
    wait(lambda: screen.find("Agenda saved"), "explicit Unicode agenda gesture saves")
    assert "revisão 日本語" in (Path(state_dir)/"agenda.md").read_text()
    send(b":run Greeting\r")
    wait(lambda: screen.find("Confirm Greeting"), "configured command requires explicit confirmation")
    assert not screen.find("Hello fixture")
    send(b"\x1b")
    send(b":yes\r")
    wait(lambda: screen.find("No command awaits"), "Esc discarded pending confirmation")
    send(b":run Greeting\r")
    wait(lambda: screen.find("Confirm Greeting"), "confirmation can be requested again")
    send(b":yes\r")
    wait(lambda: screen.find("Hello fixture"), "explicit yes runs configured literal argv")
    send(b":run Slow\r")
    wait(lambda: screen.find("Running Slow"), "worker starts without blocking terminal")
    send(b"\x1b")
    send(b"\t\x1bz")
    wait(lambda: screen.find("cancelled=true"), "Esc cancels real command worker")
    send(b"\x1b")
    wait(lambda: screen.find("agenda"), "restore from output emphasis")
    assert note.read_bytes()==original_note, "Vault scan never writes"
    send(b"\x03")
    wait(lambda: proc.poll() is not None, "clean process exit while draining terminal")
    assert proc.returncode == 0
    assert termios.tcgetattr(slave) == original, "terminal settings restored"
    screen = Screen(24, 80)
    proc = subprocess.Popen([str(binary), "--state-dir", state_dir, "--data-dir", state_dir], stdin=slave, stdout=slave, stderr=slave,
                            env=env, start_new_session=True)
    wait(lambda: screen.find("system") and screen.find("system")[1] > 35, "saved layout restored on restart")
    wait(lambda: screen.find("revisão"), "Unicode agenda restored on restart")
    send(b"\x03")
    wait(lambda: proc.poll() is not None, "clean process exit while draining terminal")
    assert proc.returncode == 0
    assert termios.tcgetattr(slave) == original
    print("PTY OK: title press, motion target, release swap, keyboard focus, emphasis, cancel, resize, clean exit/restore, saved layout and Unicode agenda/restart, command confirmation/cancellation, read-only Vault")
finally:
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait()
    os.close(master)
    os.close(slave)
    fixture.cleanup()
