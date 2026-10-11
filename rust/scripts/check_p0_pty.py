"""P0 regressions using only private synthetic files: notes and displayed agenda identities."""
import fcntl, os, pty, re, select, struct, subprocess, sys, tempfile, termios, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT))
from tests.vt import Screen
BINARY=os.environ.get('HUD_RUST_BINARY',str(ROOT/'rust/target/debug/hud-rs'))
with tempfile.TemporaryDirectory(prefix='hud-p0-pty-') as directory:
    root=Path(directory);root.chmod(0o700)
    data=root/'data';vault=root/'vault'
    for path in [data,vault]:path.mkdir(mode=0o700)
    notes=data/'notas.md';agenda=data/'agenda.md'
    notes_raw='---\r\ntitle: Notas café\r\n---\r\ntexto livre 日本語\r\n- [ ] tarefa manual\r\nsem newline'.encode()
    agenda_raw='---\r\ntitle: Agenda café\r\n---\r\nprosa manual\r\n- [ ] 2026-10-12 último 日本語\r\n- [ ] 2026-10-10 primeiro café\r\n# rodapé\r\n'.encode()
    notes.write_bytes(notes_raw);notes.chmod(0o600);agenda.write_bytes(agenda_raw);agenda.chmod(0o600)
    (vault/'fixture.md').write_text('- [ ] Vault 📅 2026-10-09\n');(vault/'fixture.md').chmod(0o600)
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,120,0,0));before=termios.tcgetattr(slave);screen=Screen(40,120)
    proc=subprocess.Popen([BINARY,'--data-dir',str(data),'--vault-dir',str(vault)],stdin=slave,stdout=slave,stderr=slave,env=dict(os.environ,TERM='xterm-256color',LANG='C.UTF-8'),start_new_session=True)
    def pump():
        if select.select([master],[],[],.05)[0]: screen.feed(os.read(master,65536))
    def wait(predicate,label):
        until=time.monotonic()+6
        while time.monotonic()<until:
            if predicate():return
            if proc.poll() is not None:raise AssertionError(f'{label}: exited {proc.returncode}\n{screen.text()}')
            pump()
        raise AssertionError(label+'\n'+screen.text())
    def send(text):os.write(master,text.encode())
    def command(text):send(text+'\r')
    try:
        wait(lambda:'Vault' in screen.text(),'mixed agenda visible')
        command('quero ação 日本語 🐱')
        wait(lambda:b'quero ' in notes.read_bytes(),'direct note text appended')
        after=notes.read_bytes();assert after.startswith(notes_raw);assert after.endswith(' quero ação 日本語 🐱\r\n'.encode());assert re.search(rb'\r\n- \d{4}-\d{2}-\d{2} \d{2}:\d{2} ',after[len(notes_raw):])
        command('/notas 1');wait(lambda: bool(re.search(r'[•-] \d{4}-\d{2}-\d{2} \d{2}:\d{2} quero',screen.text())),'local /notas N')
        send('rascunho descartado');os.write(master,b'\x1b');time.sleep(.2);pump();assert notes.read_bytes()==after
        command('/rm 1');wait(lambda:'read-only' in screen.text(),'Vault deletion refused');assert agenda.read_bytes()==agenda_raw
        for text in ['/rm 0','/ok -1','/rm 99','/rm 日本語']:
            command(text);wait(lambda:'index' in screen.text(),'invalid index rejected');assert agenda.read_bytes()==agenda_raw
        command('/ok 2');wait(lambda:b'[x]' in agenda.read_bytes(),'sorted visible local identity');assert agenda.read_bytes()==agenda_raw.replace(b'[ ] 2026-10-10',b'[x] 2026-10-10')
        command('/rm 2');wait(lambda:'último'.encode() not in agenda.read_bytes(),'remove next displayed local identity');assert b'prosa manual\r\n' in agenda.read_bytes() and b'# rodap' in agenda.read_bytes();assert 'primeiro café'.encode() in agenda.read_bytes()
        # A manual external edit after the displayed snapshot must be retained.
        manual=agenda.read_bytes()+b'manual novo\r\n- [ ] 2026-10-11 externo\r\n';agenda.write_bytes(manual)
        command('/ok 1');wait(lambda:'read-only' in screen.text(),'Vault remains read-only');assert agenda.read_bytes()==manual
        command('/agenda');wait(lambda:'externo' in screen.text(),'explicit local refresh')
        command('/ok 2');wait(lambda:b'[x] 2026-10-11' in agenda.read_bytes(),'refreshed stable identity');assert agenda.read_bytes().startswith(manual.split(b'- [ ] 2026-10-11')[0])
        os.write(master,b'\x03');proc.wait(timeout=5);assert proc.returncode==0;assert termios.tcgetattr(slave)==before
        # Reload proves note bytes and the completed agenda survived restart.
        proc=subprocess.Popen([BINARY,'--data-dir',str(data)],stdin=slave,stdout=slave,stderr=slave,env=dict(os.environ,TERM='xterm-256color'),start_new_session=True);screen=Screen(40,120)
        wait(lambda:'Nada marcado' in screen.text(),'restart has no pending local tasks');command('/notas 1');wait(lambda:'quero ação' in screen.text(),'restart retained notes');assert notes.read_bytes()==after
        os.write(master,b'\x03');proc.wait(timeout=5);assert proc.returncode==0;assert termios.tcgetattr(slave)==before
        print('PASS P0 PTY: direct text/q/Unicode, raw frontmatter+CRLF, /notas N, Esc, Vault refusal, invalid/1-based filtered identities, remove/reorder, manual refresh, persistence, terminal restore')
    finally:
        if proc.poll() is None:proc.kill();proc.wait()
        os.close(master);os.close(slave)
