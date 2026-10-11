#!/usr/bin/env python3
"""Actual Python/Rust PTY recordings using isolated HOME and synthetic private data.
No GUI, actual agent or command execution. Output text/cells + raw ANSI, not image mockups.
Usage: capture_visual_pty.py --binary BIN --output NEW_DIRECTORY
"""
import argparse,fcntl,json,os,pty,select,signal,struct,subprocess,sys,tempfile,termios,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT))
from tests.vt import Screen

def capture(command,env,rows,cols,output,label):
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',rows,cols,0,0));before=termios.tcgetattr(slave);screen=Screen(rows,cols);raw=bytearray()
    proc=subprocess.Popen(command,stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True,cwd=ROOT)
    def pump(seconds):
        until=time.monotonic()+seconds
        while time.monotonic()<until:
            if select.select([master],[],[],min(.05,max(0,until-time.monotonic())))[0]:
                data=os.read(master,65536);raw.extend(data);screen.feed(data)
    def save(state):
        prefix=output/f'{label}-{cols}x{rows}-{state}'
        prefix.with_suffix('.txt').write_text(screen.text()+'\n')
        prefix.with_suffix('.json').write_text(json.dumps({'label':label,'columns':cols,'rows':rows,'state':state,'cursor':[screen.y,screen.x],'cells':screen.cells},ensure_ascii=False)+'\n')
        prefix.with_suffix('.ansi').write_bytes(raw)
    try:
        pump(2.0);assert proc.poll() is None,screen.text();assert all(name in screen.text() for name in ['SISTEMA','COMANDOS','AGENDA','VAULT','SAÍDA']),screen.text();save('dashboard')
        os.write(master,('rascunho café 日本語 🐱 '+ 'ação '*40).encode());pump(.4);save('composer')
        os.write(master,b'\x1b');pump(.25)
        # Same actual input: Tab focuses the first panel in both implementations.
        os.write(master,b'\t');pump(.3);save('focus')
        os.write(master,b'\x03');proc.wait(timeout=5);assert proc.returncode==0;assert termios.tcgetattr(slave)==before
    finally:
        if proc.poll() is None:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
        os.close(master);os.close(slave)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args();args.output.mkdir(mode=0o700)
    with tempfile.TemporaryDirectory(prefix='hud-visual-reference-') as directory:
        root=Path(directory);root.chmod(0o700);vault=root/'vault';vault.mkdir(mode=0o700);(vault/'.obsidian').mkdir(mode=0o700);home=root/'home';home.mkdir(mode=0o700)
        (vault/'Nota fixture.md').write_text('# Nota fixture\nTexto café 日本語\n');(vault/'Nota fixture.md').chmod(0o600)
        env=dict(os.environ,HOME=str(home),XDG_CONFIG_HOME=str(home/'config'),XDG_DATA_HOME=str(home/'share'),XDG_STATE_HOME=str(home/'state'),TERM='xterm-256color',LANG='C.UTF-8')
        for cols,rows in [(80,24),(120,40),(160,50)]:
            for name in ['python','rust']:
                data=root/f'{name}-{cols}';data.mkdir(mode=0o700)
                (data/'agenda.md').write_text('- [ ] 2026-10-12 revisar café 日本語\n');(data/'agenda.md').chmod(0o600)
                (data/'notas.md').write_text('- 2026-10-11 10:00 nota fixture\n');(data/'notas.md').chmod(0o600)
                if name=='python':
                    config=root/f'python-{cols}.toml';config.write_text(f"vault={json.dumps(str(vault))}\ndata_dir={json.dumps(str(data))}\ncommand=[]\n[claude]\nenabled=false\n[codex]\nenabled=false\n[opencode]\nenabled=false\n");config.chmod(0o600)
                    command=[str(ROOT/'bin/hud'),'--config',str(config)]
                else:command=[str(args.binary.resolve()),'--vault-dir',str(vault),'--data-dir',str(data)]
                capture(command,env,rows,cols,args.output,name)
        (args.output/'README.txt').write_text('Actual live Python and Rust PTY captures; same dimensions/private fixture inputs. Metrics vary between process timestamps. Raw ANSI preserves colors/attributes; JSON/text VT extraction retains foreground/positions but drops bold, dim and background. No physical-terminal claim. HOME isolated; providers disabled; no commands approved.\n')
    print('PASS live PTY visual captures: three sizes × two applications × dashboard/composer/focus; isolated data, terminal restored')
if __name__=='__main__':main()
