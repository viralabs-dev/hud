"""Default-layout startup regression; private cwd fixtures, no personal data selection."""
import fcntl, os, pty, select, struct, subprocess, sys, tempfile, termios, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from tests.vt import Screen
BINARY = os.environ.get('HUD_RUST_BINARY', str(ROOT / 'rust/target/debug/hud-rs'))
with tempfile.TemporaryDirectory(prefix='hud-default-cwd-') as directory:
    root = Path(directory); root.chmod(0o700)
    for name in ['Pasta Com Espaços', 'UPPERCASE']: (root / name).mkdir(mode=0o700)
    for cwd in ['/', str(root / 'Pasta Com Espaços'), str(root / 'UPPERCASE')]:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
        before = termios.tcgetattr(slave); screen = Screen(24, 80)
        proc = subprocess.Popen([BINARY], cwd=cwd, stdin=slave, stdout=slave, stderr=slave,
                                env=dict(os.environ, TERM='xterm-256color', LANG='C.UTF-8'), start_new_session=True)
        labels = ['SISTEMA', 'COMANDOS', 'AGENDA', 'USO CLAUDE', 'USO CODEX', 'VAULT', 'SAÍDA']
        try:
            until = time.monotonic() + 10
            while time.monotonic() < until and not all(label in screen.text() for label in labels):
                if select.select([master], [], [], .1)[0]: screen.feed(os.read(master, 65536))
                if proc.poll() is not None: raise AssertionError((cwd, proc.returncode, screen.text()))
            assert all(label in screen.text() for label in labels), (cwd, screen.text())
            os.write(master, b'\x03')
            assert proc.wait(timeout=5) == 0
            assert termios.tcgetattr(slave) == before
            print('PASS default seven panels and terminal restore, cwd=', repr(cwd))
        finally:
            if proc.poll() is None: proc.kill(); proc.wait()
            os.close(master); os.close(slave)
