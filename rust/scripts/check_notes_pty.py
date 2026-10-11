"""Fixture-only note approval/cancellation and cached Vault smoke in a synthetic PTY."""
import fcntl, os, pty, select, struct, subprocess, sys, tempfile, termios, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT))
from tests.vt import Screen
with tempfile.TemporaryDirectory(prefix='hud-notes-pty-') as directory:
    root=Path(directory); root.chmod(0o700)
    for name,content in [('.hud-note-fixture','HUD-NOTE-FIXTURE 1\n'),('ação.md','- [ ] [[Café|alias]]\r\nmanual\r\n')]:
        (root/name).write_bytes(content.encode());(root/name).chmod(0o600)
    original=(root/'ação.md').read_bytes()
    master,slave=pty.openpty(); fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,120,0,0));saved=termios.tcgetattr(slave);screen=Screen(36,120)
    proc=subprocess.Popen([os.environ.get('HUD_RUST_BINARY',str(ROOT/'rust/target/debug/hud-rs')),'--vault-dir',str(root),'--note-fixture-dir',str(root)],stdin=slave,stdout=slave,stderr=slave,env=dict(os.environ,TERM='xterm-256color'),start_new_session=True)
    def wait(predicate,label):
        end=time.monotonic()+8
        while time.monotonic()<end:
            if predicate(): return
            if proc.poll() is not None: raise AssertionError(f'exit {proc.returncode}: {screen.text()}')
            if select.select([master],[],[],.05)[0]: screen.feed(os.read(master,65536))
        raise AssertionError(label+'\n'+screen.text())
    def command(text): os.write(master,(':'+text+'\r').encode())
    try:
        wait(lambda:screen.find('1 notas'),'initial snapshot')
        command('note toggle done 1 ação.md');wait(lambda:screen.find('FIXTURE PREVIEW'),'preview');assert (root/'ação.md').read_bytes()==original
        os.write(master,b'\x1b');time.sleep(.2);command('note apply');wait(lambda:screen.find('no pending'),'Esc revoked');assert (root/'ação.md').read_bytes()==original
        command('note toggle done 1 ação.md');wait(lambda:screen.find('FIXTURE PREVIEW'),'second preview');os.write(master,b'\t');command('note apply');wait(lambda:screen.find('no pending'),'focus revoked');assert (root/'ação.md').read_bytes()==original
        command('note toggle done 1 ação.md');wait(lambda:screen.find('FIXTURE PREVIEW'),'third preview');command('note apply');wait(lambda:(root/'ação.md').read_bytes()!=original,'approved save');assert (root/'ação.md').read_bytes()==original.replace(b'[ ]',b'[x]',1)
        command('vault refresh');wait(lambda:screen.find('Vault refresh queued'),'refresh queued');time.sleep(.4);wait(lambda:screen.find('1 notas'),'refreshed snapshot')
        command('vault recent ação');wait(lambda:screen.find('ação.md'),'recent filter')
        os.write(master,b'\x03');proc.wait(timeout=5);assert proc.returncode==0;assert termios.tcgetattr(slave)==saved
        print('PASS notes fixture preview/Esc/focus/approval/CRLF/Unicode/refresh/recent/termios')
    finally:
        if proc.poll() is None: proc.kill();proc.wait()
        os.close(master);os.close(slave)
