import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('importer', Path(__file__).with_name('import_python_layout.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
GOOD = b'# preserved comment\nnome = "fixture"\n[[coluna]]\nlargura = 36\npaineis = ["sistema", "agenda"]\n[[coluna]]\npaineis = ["pasta", "saida"]\n'

class ImportTests(unittest.TestCase):
    def test_preview_schema_alias_and_explicit_loss(self):
        data, report = m.preview(GOOD)
        self.assertEqual(data, b'HUD-LAYOUT 1\nsystem,agenda\nvault,output\n')
        self.assertTrue(report['warnings'])
    def test_invalid_inputs(self):
        for bad in [b'\xff', b'xx', GOOD + b'x=1\n', GOOD.replace(b'"saida"', b'"agenda"'), GOOD.replace(b'"saida"', b'"missing"'), b'x' * (m.LIMIT+1), GOOD + b'[[bloco]]\nid="agenda"\n']:
            with self.subTest(bad=bad[:40]), self.assertRaises(Exception):
                m.preview(bad)
    def test_dry_run_no_target_mutation(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); source=p/'source'; source.write_bytes(GOOD); source.chmod(0o644)
            self.assertEqual(m.main(['--source', str(source), '--target', str(p/'state')]), 0)
            self.assertFalse((p/'state').exists())
            self.assertEqual(source.read_bytes(), GOOD)
    def test_backup_exact_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); source=p/'source'; source.write_bytes(GOOD); source.chmod(0o644)
            state=p/'state'; encoded,_=m.preview(GOOD)
            m.commit(state, GOOD, encoded)
            self.assertEqual((state/'python-layout.original.toml').read_bytes(), GOOD)
            self.assertEqual((state/'layout.state').read_bytes(), encoded)
            self.assertEqual((state/'layout.state').stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError): m.commit(state, b'changed', encoded)
            self.assertEqual(source.read_bytes(), GOOD)
            self.assertEqual((state/'python-layout.original.toml').read_bytes(), GOOD)
    def test_links_and_traversal_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); source=p/'source'; source.write_bytes(GOOD); source.chmod(0o644)
            (p/'link').symlink_to(source)
            with self.assertRaises(OSError): m.read_source(p/'link')
            (p/'parent').symlink_to(p, target_is_directory=True)
            with self.assertRaises(OSError): m.read_source(p/'parent/source')
            with self.assertRaises(ValueError): m.read_source(p/'../source')
    def test_failure_preserves_backup_and_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); encoded,_=m.preview(GOOD)
            real_open=m.os.open
            def fail(name,*a,**kw):
                if name == '.layout.import.tmp': raise OSError('injected write failure')
                return real_open(name,*a,**kw)
            with patch.object(m.os, 'open', side_effect=fail), self.assertRaises(OSError):
                m.commit(p/'state', GOOD, encoded)
            self.assertEqual((p/'state/python-layout.original.toml').read_bytes(), GOOD)
            self.assertFalse((p/'state/layout.state').exists())
            with self.assertRaises(FileExistsError): m.commit(p/'state', GOOD, encoded)

    def test_source_writable_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'source'; source.write_bytes(GOOD); source.chmod(0o666)
            with self.assertRaises(ValueError): m.read_source(source)
            source.chmod(0o644)
            self.assertEqual(m.read_source(source), GOOD)
    def test_rename_failure_backup_cleanup_and_recover(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); encoded,_=m.preview(GOOD)
            with patch.object(m.os, 'rename', side_effect=OSError('rename injected')), self.assertRaises(OSError):
                m.commit(p/'failed', GOOD, encoded)
            self.assertEqual(list((p/'failed').iterdir()), [p/'failed/python-layout.original.toml'])
            original=m.read_source(p/'failed/python-layout.original.toml')
            recovered,_=m.preview(original)
            m.commit(p/'recovered', original, recovered)
            self.assertEqual((p/'recovered/layout.state').read_bytes(), encoded)
            self.assertEqual((p/'recovered/python-layout.original.toml').read_bytes(), GOOD)
    def test_nested_column_keys_refused(self):
        with self.assertRaises(Exception):
            m.preview(GOOD.replace(b'largura = 36', b'largura = 36\nevil = true'))

if __name__ == '__main__': unittest.main()
