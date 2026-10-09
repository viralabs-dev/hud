"""Skill projeto-docs: frontmatter, modelos, Kanban e ausência de dados pessoais."""

import json
import os
import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILL_DIR = ROOT / "skills" / "projeto-docs"
SKILL = SKILL_DIR / "SKILL.md"
MODELOS = SKILL_DIR / "modelos"
KANBAN = MODELOS / "kanban.md"

COLUNAS = ["Como usar", "Plano de entrega", "A fazer", "Em andamento", "Bloqueado", "Concluído"]

# Padrões que nunca podem ir para o repositório público, valham em qualquer máquina.
# Montados por partes para este arquivo não casar com a varredura que ele mesmo descreve.
GENERIC_FORBIDDEN = ["/" + "home/[a-z]", "/" + "Users/[a-z]", "gh" + "p_", "gh" + "o_",
                     "github" + "_pat_", "sk" + "-ant", r"[\w.+-]+@[\w-]+\.[\w.]+"]
# Nomes comuns demais para servirem de padrão pessoal (usuários de CI, provedores de e-mail).
GENERIC_NAMES = {"gmail", "outlook", "hotmail", "yahoo", "root", "demo", "home", "user", "users",
                 "runner", "runneradmin", "ubuntu", "admin", "github", "actions"}


def personal_patterns() -> list[str]:
    """Dados de quem roda o teste, montados em tempo de execução (nada literal aqui)."""
    pats = {os.environ.get(k, "") for k in ("USER", "LOGNAME", "USERNAME")}
    pats.add(str(Path.home()))
    for key in ("user.email", "user.name"):
        try:
            value = subprocess.run(["git", "-C", str(ROOT), "config", "--get", key],
                                   capture_output=True, text=True, timeout=5).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            value = ""
        if key == "user.email" and "@" in value:
            local, domain = value.split("@", 1)
            pats |= {value, local, domain.split(".")[0]}
        elif value:
            pats.add(value)
    pats |= set(os.environ.get("HUD_RECORD_FORBIDDEN", "").split(","))
    return sorted(p for p in pats if len(p) >= 4 and p.lower() not in GENERIC_NAMES)


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


def skill_files() -> list[Path]:
    return sorted(p for p in SKILL_DIR.rglob("*") if p.is_file())


class SkillProjetoDocsTest(unittest.TestCase):
    def setUp(self):
        self.text = SKILL.read_text(encoding="utf-8")

    def test_frontmatter(self):
        meta = frontmatter(self.text)
        self.assertEqual(meta.get("name"), "projeto-docs")
        self.assertGreater(len(meta.get("description", "")), 80)

    def test_tamanho(self):
        self.assertLess(len(self.text.encode("utf-8")), 40 * 1024)

    def test_modelos_citados_existem_e_vice_versa(self):
        citados = set(re.findall(r"modelos/([\w.-]+\.md)", self.text))
        existentes = {p.name for p in MODELOS.glob("*.md")}
        self.assertTrue(citados, "o SKILL.md não cita nenhum modelo")
        self.assertEqual(citados - existentes, set(), "modelo citado que não existe")
        self.assertEqual(existentes - citados, set(), "modelo que o SKILL.md não cita")

    def test_modelos_tem_frontmatter(self):
        for path in sorted(MODELOS.glob("*.md")):
            with self.subTest(modelo=path.name):
                text = path.read_text(encoding="utf-8")
                if path == KANBAN:
                    self.assertTrue(text.startswith("---\n\nkanban-plugin: board\n"))
                    continue
                meta = frontmatter(text)
                for key in ("title", "aliases", "tags", "atualizado"):
                    self.assertIn(key, meta)

    def test_documenta_hud_doc_e_doc_salvar(self):
        self.assertIn('````hud-doc arquivo="', self.text)
        self.assertIn("/doc salvar", self.text)
        self.assertIn("quatro crases", self.text)
        for comando in ("/project", "/doc"):
            self.assertIn(comando, self.text)


class KanbanModeloTest(unittest.TestCase):
    def setUp(self):
        self.text = KANBAN.read_text(encoding="utf-8")
        head, sep, self.settings = self.text.partition("%% kanban:settings")
        self.assertTrue(sep, "sem bloco kanban:settings")
        self.colunas = re.findall(r"^## (.+)$", head, flags=re.M)

    def test_colunas_na_ordem(self):
        posicoes = [self.colunas.index(c) for c in COLUNAS]
        self.assertEqual(posicoes, sorted(posicoes), self.colunas)
        self.assertEqual(posicoes, list(range(posicoes[0], posicoes[0] + len(COLUNAS))),
                         "as colunas obrigatórias devem vir juntas, nesta ordem")

    def test_list_collapse_uma_posicao_por_coluna(self):
        m = re.search(r"^```\n(\{.*\})\n```\n%%\s*$", self.settings, flags=re.M | re.S)
        self.assertIsNotNone(m, "cerca do settings deve ser ``` sem linguagem")
        data = json.loads(m.group(1))
        self.assertEqual(data.get("kanban-plugin"), "board")
        collapse = data.get("list-collapse")
        self.assertIsInstance(collapse, list)
        self.assertEqual(len(collapse), len(self.colunas))
        self.assertTrue(all(isinstance(v, bool) for v in collapse))


class SemDadosPessoaisTest(unittest.TestCase):
    def test_sem_dados_pessoais(self):
        pessoais = [re.escape(p) for p in personal_patterns()]
        files = [*skill_files(), Path(__file__)]
        for path in files:
            text = path.read_text(encoding="utf-8")
            # Este arquivo cita os padrões genéricos de propósito; dele só se cobra o pessoal.
            for pat in (pessoais if path == Path(__file__) else GENERIC_FORBIDDEN + pessoais):
                with self.subTest(arquivo=str(path.relative_to(ROOT)), padrao=pat):
                    self.assertIsNone(re.search(pat, text, flags=re.I))


if __name__ == "__main__":
    unittest.main()
