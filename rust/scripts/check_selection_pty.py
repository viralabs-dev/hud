"""Remembered selection with trust directory restores without --custom-file."""
import fcntl,json,os,pty,select,struct,subprocess,sys,tempfile,termios,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT))
from tests.vt import Screen
binary=os.environ.get('HUD_RUST_BINARY',str(ROOT/'rust/target/debug/hud-rs'))
with tempfile.TemporaryDirectory(prefix='hud-selection-pty-') as directory:
    root=Path(directory);root.chmod(0o700)
    for name in ['selection','trust']:(root/name).mkdir(mode=0o700)
    config=root/'custom.json';config.write_text(json.dumps({'version':1,'name':'remembered','columns':[{'slots':[{'id':'note','weight':1},{'id':'output','weight':1}]}],'panels':[{'id':'note','title':'Remembered','type':'text','text':'selection fixture restored'}]}));config.chmod(0o600)
    for explicit in [True,False]:
        master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,100,0,0));saved=termios.tcgetattr(slave);screen=Screen(24,100)
        args=[binary,'--custom-selection-dir',str(root/'selection'),'--feed-trust-dir',str(root/'trust')]
        if explicit:args+=['--custom-file',str(config)]
        proc=subprocess.Popen(args,stdin=slave,stdout=slave,stderr=slave,env=dict(os.environ,TERM='xterm-256color'),start_new_session=True)
        try:
            end=time.monotonic()+8
            while not screen.find('selection fixture restored'):
                if proc.poll() is not None or time.monotonic()>end:raise AssertionError(screen.text())
                if select.select([master],[],[],.1)[0]:screen.feed(os.read(master,65536))
            os.write(master,b'\x03');proc.wait(timeout=5);assert proc.returncode==0;assert termios.tcgetattr(slave)==saved
        finally:
            if proc.poll() is None:proc.kill();proc.wait()
            os.close(master);os.close(slave)
    print('PASS remembered selection + trust directory restores without custom-file; text feed; termios')
