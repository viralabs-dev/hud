import importlib.util
from pathlib import Path
import tempfile
import unittest
spec=importlib.util.spec_from_file_location('package',Path(__file__).with_name('package_local.py'))
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
class PackageTests(unittest.TestCase):
    def test_existing_output_refused_unchanged(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); output=p/'archive'; output.write_bytes(b'preserve')
            with self.assertRaises(ValueError): m.package(p,p/'binary',output,'missing-cargo')
            self.assertEqual(output.read_bytes(),b'preserve')
    def test_non_elf_refused_before_cargo(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp); binary=p/'binary'; binary.write_bytes(b'#!/bin/sh')
            with self.assertRaises(ValueError): m.package(p,binary,p/'archive','missing-cargo')
            self.assertFalse((p/'archive').exists())
if __name__=='__main__': unittest.main()
