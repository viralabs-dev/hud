"""Skill hud-comandos (e a seção de blocos da hud-custom): formato e nada pessoal."""

import re
import tomllib
import unittest
from pathlib import Path

from hud import comandos
from tests.test_skill_projeto_docs import GENERIC_FORBIDDEN, frontmatter, personal_patterns

ROOT = Path(__file__).resolve().parent.parent
SKILL = ROOT / "skills" / "hud-comandos" / "SKILL.md"
CUSTOM_SKILL = ROOT / "skills" / "hud-custom" / "SKILL.md"


class SkillHudComandosTest(unittest.TestCase):
    def setUp(self):
        self.text = SKILL.read_text(encoding="utf-8")

    def test_frontmatter(self):
        meta = frontmatter(self.text)
        self.assertEqual(meta.get("name"), "hud-comandos")
        self.assertGreater(len(meta.get("description", "")), 80)
        self.assertIn("COMANDOS", meta["description"])

    def test_documenta_cerca_e_fluxo(self):
        self.assertIn("```` ```hud-comandos ````", self.text)
        self.assertIn("comandos.toml", self.text)
        self.assertIn("hud --check", self.text)
        for campo in ("name", "argv", "timeout", "confirm", "max_lines", "cwd"):
            self.assertRegex(self.text, rf"(?m)^{campo} = ")

    def test_exemplos_toml_validos(self):
        blocos = re.findall(r"```toml\n(.*?)```", self.text, flags=re.S)
        self.assertTrue(blocos)
        for b in blocos:
            data = tomllib.loads(b)
            self.assertEqual(set(data), {"command"})

    def test_exemplo_de_resposta_e_uma_proposta(self):
        exemplo = self.text.split("Exemplo literal de resposta:", 1)[1]
        p = comandos.parse_proposals(exemplo)
        self.assertIsNotNone(p)
        self.assertEqual([c for c in tomllib.loads(p.conteudo)], ["command"])

    def test_hud_custom_documenta_blocos_e_previa(self):
        text = CUSTOM_SKILL.read_text(encoding="utf-8")
        self.assertIn("[[bloco]]", text)
        self.assertIn("pré-visualização", text)
        for valor in ("historico", "sensores", "quadros", "atencao", "processos", "sessoes"):
            self.assertIn(f"`{valor}`", text)

    def test_sem_dados_pessoais(self):
        pats = GENERIC_FORBIDDEN + [re.escape(p) for p in personal_patterns()]
        for path in (SKILL, CUSTOM_SKILL, ROOT / "hud" / "comandos.py",
                     ROOT / "tests" / "test_comandos.py"):
            text = path.read_text(encoding="utf-8")
            for pat in pats:
                with self.subTest(arquivo=path.name, padrao=pat):
                    self.assertIsNone(re.search(pat, text, flags=re.I))


if __name__ == "__main__":
    unittest.main()
