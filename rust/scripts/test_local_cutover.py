import tempfile, unittest
from pathlib import Path
from local_cutover import capture, switch, rollback, digest
class CutoverTests(unittest.TestCase):
    def test_binary_and_unicode_data_backup_switch_and_checked_rollback(self):
        with tempfile.TemporaryDirectory(prefix='hud-cutover-fixture-') as directory:
            root=Path(directory);root.chmod(0o700)
            old=root/'python-preserved';new=root/'rust-candidate';old.write_bytes(b'old preserved binary');new.write_bytes(b'new binary')
            links=[root/'hud',root/'hud-rs']
            for link in links:link.symlink_to(old)
            notes=root/'notas.md';agenda=root/'agenda.md'
            original={notes:'---\r\ntexto café 日本語\r\n'.encode(),agenda:b'prosa\r\n- [ ] 2026-10-10 tarefa\r\n'}
            for path,data in original.items():path.write_bytes(data);path.chmod(0o600)
            backup=root/'backup';capture(backup,links,original)
            switch(backup,{link:new for link in links})
            for link in links:self.assertEqual(digest(link),digest(new))
            for path in original:path.write_bytes(original[path]+b'fixture appended\n')
            expected={str(path):digest(path) for path in original}
            rollback(backup,restore_data=expected)
            for link in links:self.assertEqual(link.readlink(),old)
            for path,data in original.items():self.assertEqual(path.read_bytes(),data)
            self.assertEqual(old.read_bytes(),b'old preserved binary')
    def test_manual_launcher_or_data_change_is_refused(self):
        with tempfile.TemporaryDirectory(prefix='hud-cutover-fixture-') as directory:
            root=Path(directory);old=root/'old';new=root/'new';other=root/'manual'
            for p in [old,new,other]:p.write_bytes(p.name.encode())
            link=root/'hud';link.symlink_to(old);data=root/'notes';data.write_bytes(b'notes')
            backup=root/'backup';capture(backup,[link],[data]);switch(backup,{link:new})
            data.write_bytes(b'manual edit')
            with self.assertRaises(ValueError):rollback(backup,{str(data):'stale'})
            self.assertEqual(link.readlink(),new);self.assertEqual(data.read_bytes(),b'manual edit')
            link.unlink();link.symlink_to(other)
            with self.assertRaises(ValueError):rollback(backup)
            self.assertEqual(link.readlink(),other)
