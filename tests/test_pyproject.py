"""Empacotamento: pyproject.toml válido, sem dependências, entrada em modo isolado."""

import os
import tomllib
import unittest
from pathlib import Path

import hud

ROOT = Path(__file__).resolve().parent.parent


class PyprojectTest(unittest.TestCase):
    def setUp(self):
        with (ROOT / "pyproject.toml").open("rb") as f:
            self.data = tomllib.load(f)
        self.project = self.data["project"]
        self.setuptools = self.data["tool"]["setuptools"]

    def test_metadata(self):
        self.assertEqual(self.project["name"], "hud")
        self.assertEqual(self.project["requires-python"], ">=3.11")
        self.assertEqual(self.data["build-system"]["build-backend"], "setuptools.build_meta")
        self.assertIn("version", self.project["dynamic"])
        self.assertEqual(self.project["license"], "MIT")
        self.assertEqual(self.project["license-files"], ["LICENSE"])
        self.assertIn("MIT License", (ROOT / "LICENSE").read_text(encoding="utf-8"))
        self.assertEqual(self.data["tool"]["setuptools"]["dynamic"]["version"], {"attr": "hud.__version__"})
        self.assertRegex(hud.__version__, r"^\d+\.\d+\.\d+$")  # a tag vX.Y.Z do release tem de bater

    def test_sem_dependencias(self):
        self.assertEqual(self.project.get("dependencies", []), [])
        self.assertNotIn("optional-dependencies", self.project)

    def test_so_o_pacote_hud(self):
        # Além do código, só os dados embutidos: os modelos de custom/ e a skill.
        self.assertEqual(self.setuptools["packages"], ["hud", "hud._modelos", "hud._skill"])
        self.assertEqual(self.setuptools["package-dir"],
                         {"hud._modelos": "custom", "hud._skill": "skills/hud-custom"})
        for pkg, src in self.setuptools["package-dir"].items():
            self.assertFalse(any((ROOT / src).rglob("*.py")), f"{src} não pode ter código")
        self.assertTrue((ROOT / "custom" / "foco" / "layout.toml").is_file())
        self.assertTrue((ROOT / "skills" / "hud-custom" / "SKILL.md").is_file())

    def test_entrada_isolada(self):
        # O comando `hud` é scripts/hud, que roda o Python do venv com -I e
        # chama hud.__main__ (main). Um [project.scripts] geraria um script sem
        # -I, que carregaria módulos plantados via PYTHONPATH.
        self.assertNotIn("scripts", self.project)
        self.assertNotIn("gui-scripts", self.project)
        self.assertEqual(self.setuptools["script-files"], ["scripts/hud"])
        script = ROOT / "scripts" / "hud"
        self.assertTrue(os.access(script, os.X_OK))
        self.assertIn('exec "$here/python" -I -m hud "$@"', script.read_text(encoding="utf-8"))
        main_py = (ROOT / "hud" / "__main__.py").read_text(encoding="utf-8")
        self.assertIn("def main()", main_py)
        self.assertIn('if __name__ == "__main__":', main_py)


if __name__ == "__main__":
    unittest.main()
