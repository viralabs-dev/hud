"""AT-082: aliases, selectors and Tab render with private fixtures; no commands/providers executed."""
import os,sys,json,shutil
from pathlib import Path
ROOT=Path(os.environ.get('HUD_RUST_ROOT',str(Path(__file__).resolve().parents[2])))
ns={'__name__':'hud_alias_harness'}
saved=sys.argv;sys.argv=[str(ROOT/'rust/scripts/check_settings_pty.py'),os.environ.get('HUD_RUST_BINARY',str(ROOT/'rust/target/debug/hud-rs'))]
exec(compile((ROOT/'rust/scripts/check_settings_pty.py').read_text().split('with tempfile.TemporaryDirectory',1)[0],'harness','exec'),ns);sys.argv=saved
Session=ns['Session'];private_file=ns['private_file'];tempfile=ns['tempfile']
with tempfile.TemporaryDirectory(prefix='hud-at082-pty-') as tmp:
 b=Path(tmp);b.chmod(0o700)
 for n in ['data','docs','agent']:(b/n).mkdir(mode=0o700)
 private_file(b/'data/notas.md','');private_file(b/'data/agenda.md','')
 fake=Path(os.environ.get('HUD_FAKE_AGENT',str(ROOT/'rust/target/debug/fake_agent')))
 shutil.copy2(fake,b/'fake_agent');(b/'fake_agent').chmod(0o700)
 private_file(b/'proposals.json','```hud-doc arquivo=Fixture.md\nação 日本語\n[[Link\\|alias]]\n```\n')
 names=['Alpha','Alpha extra','Beta']+[f'Fixture-{i}' for i in range(4,11)]
 private_file(b/'comandos.toml',''.join(f"[[command]]\nname='{n}'\nargv=['printf','never executed']\nconfirm=true\n" for n in names))
 args=['--data-dir',str(b/'data'),'--doc-root',str(b/'docs'),'--doc-proposals',str(b/'proposals.json'),'--command-file',str(b/'comandos.toml'),'--fake-agent',str(b/'fake_agent'),'--agent-dir',str(b/'agent')]
 s=Session(args)
 try:
  s.text('ENTRADA')
  s.command('/doc salvar');s.text('Document preview');assert not (b/'docs/Fixture.md').exists()
  s.command('/doc descartar');s.text('descartadas');s.command('/doc salvar');s.text('Nenhuma');assert not (b/'docs/Fixture.md').exists()
  s.close();s=Session(args);s.text('ENTRADA')
  s.command('/doc salvar');s.text('Document preview');s.command('/doc salvar');s.text('Document saved');assert (b/'docs/Fixture.md').read_text()=='ação 日本語\n[[Link\\|alias]]\n'
  s.command('/r alpha');s.pump(.3);assert not (b/'data/notas.md').read_bytes()
  s.command('/r 1');s.text('Confirm Alpha');s.command('cancel')
  s.command('/r BETA');s.text('Confirm Beta');s.command('cancel')
  s.command('/r F10');s.text('Confirm Fixture-10');s.command('cancel')
  s.send(b'\t');s.text('CLAUDE · LEITURA')
  s.send(b'\t');s.text('CODEX · LEITURA')
  s.send(b'\t');s.text('OPENCODE · LEITURA')
  s.send(b'\t');s.text('ENTRADA')
  s.send('draft 日本語'.encode());s.send(b'\t');s.text('CLAUDE · LEITURA');s.text('draft 日本語');s.send(b'\x1b')
  assert (b/'data/notas.md').read_bytes()==b''
  print('PASS AT082: rendered doc salvar/descartar, explicit review/save, selectors numeric/F10/case-partial, Tab notes/providers and draft preservation, no execution, termios restored')
 finally:s.close()
