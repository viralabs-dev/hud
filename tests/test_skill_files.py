"""Skill hud-custom e customizações de exemplo em custom/."""

import importlib.util
import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILL = ROOT / "skills" / "hud-custom" / "SKILL.md"
CUSTOM = ROOT / "custom"
BUILTIN = {"sistema", "comandos", "agenda", "uso_claude", "uso_codex", "vault", "pasta", "saida"}
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,39}$")
HAS_LAYOUT = importlib.util.find_spec("hud.layout") is not None


def frontmatter(text: str) -> dict[str, str]:
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        raise ValueError("sem frontmatter")
    end = lines.index("---", 1)
    data = {}
    for line in lines[1:end]:
        if not line.strip():
            continue
        key, sep, value = line.partition(":")
        if not sep or not key.strip() or key != key.strip():
            raise ValueError(f"linha inválida no frontmatter: {line!r}")
        data[key] = value.strip()
    return data


def layouts() -> list[Path]:
    return sorted(CUSTOM.glob("*/layout.toml"))


def panel_ids(entry) -> str:
    return entry if isinstance(entry, str) else entry["id"]


class SkillTest(unittest.TestCase):
    def test_frontmatter(self):
        meta = frontmatter(SKILL.read_text(encoding="utf-8"))
        self.assertEqual(meta.get("name"), "hud-custom")
        self.assertGreater(len(meta.get("description", "")), 40)

    def test_sem_dados_pessoais(self):
        files = [SKILL, *(p for p in CUSTOM.rglob("*") if p.is_file())]
        for path in files:
            text = path.read_text(encoding="utf-8")
            with self.subTest(arquivo=str(path.relative_to(ROOT))):
                self.assertNotRegex(text, r"/home/[a-z]")
                self.assertNotRegex(text, r"[\w.+-]+@[\w-]+\.[\w.]+")


class CustomTest(unittest.TestCase):
    def test_tem_exemplos(self):
        names = {p.parent.name for p in layouts()}
        self.assertTrue({"padrao", "foco", "monitor"} <= names, names)

    def test_layouts_validos(self):
        for path in layouts():
            with self.subTest(custom=path.parent.name):
                self.assertTrue(NAME_RE.match(path.parent.name))
                with path.open("rb") as f:
                    data = tomllib.load(f)
                self.assertEqual(data.get("nome"), path.parent.name)
                cols = data.get("coluna")
                self.assertIsInstance(cols, list)
                self.assertTrue(1 <= len(cols) <= 4)
                own = {p["id"]: p for p in data.get("painel", [])}
                self.assertFalse(own.keys() & BUILTIN)
                used = [panel_ids(e) for c in cols for e in c["paineis"]]
                self.assertIn("saida", used)
                self.assertEqual(len(used), len(set(used)), "id repetido")
                for pid in used:
                    self.assertTrue(pid in BUILTIN or pid in own, f"id desconhecido: {pid}")
                for p in own.values():
                    if p.get("tipo") == "texto":
                        self.assertTrue((path.parent / p["arquivo"]).is_file(), p["arquivo"])

    def test_padrao_igual_ao_embutido(self):
        with (CUSTOM / "padrao" / "layout.toml").open("rb") as f:
            cols = tomllib.load(f)["coluna"]
        self.assertEqual(cols[0]["largura"], 36)
        self.assertEqual([panel_ids(e) for e in cols[0]["paineis"]],
                         ["sistema", "comandos", "agenda", "uso_claude", "uso_codex"])
        self.assertEqual(cols[1]["paineis"], [{"id": "vault", "peso": 55}, {"id": "saida", "peso": 45}])

    @unittest.skipUnless(HAS_LAYOUT, "hud.layout ainda não existe")
    def test_hud_layout_load(self):
        from hud import layout

        for path in layouts():
            with self.subTest(custom=path.parent.name):
                try:
                    layout.load(path)
                except (TypeError, layout.LayoutError) as first:
                    # load pode receber a pasta da customização em vez do arquivo
                    try:
                        layout.load(path.parent)
                    except Exception:
                        raise first from None


if __name__ == "__main__":
    unittest.main()
