import os
import tempfile
import unittest
from pathlib import Path

from hud import skills as sk
from tests.suporte import WINDOWS, symlink


def make(base: Path, pasta: str, texto: str | bytes) -> Path:
    d = base / pasta
    d.mkdir(parents=True, exist_ok=True)
    f = d / "SKILL.md"
    if isinstance(texto, bytes):
        f.write_bytes(texto)
    else:
        f.write_text(texto, encoding="utf-8")
    return d


def front(nome=None, desc=None) -> str:
    out = ["---"]
    if nome is not None:
        out.append(f"name: {nome}")
    if desc is not None:
        out.append(f"description: {desc}")
    return "\n".join(out + ["---", "# corpo", ""])


class DiscoverTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.hud, self.claude, self.codex = (self.tmp / n for n in ("hud", "claude", "codex"))
        for d in (self.hud, self.claude, self.codex):
            d.mkdir()
        self.dirs = [("hud", self.hud), ("claude", self.claude), ("codex", self.codex)]

    def tearDown(self):
        self._tmp.cleanup()

    def test_multiple_origins_and_duplicates(self):
        make(self.hud, "hud-custom", front("hud-custom", "do HUD"))
        make(self.claude, "hud-custom", front("hud-custom", "cópia do Claude"))
        make(self.codex, "HUD-Custom", front(None, "pasta com outra caixa"))
        make(self.claude, "zeta", front(desc="z"))
        make(self.codex, "alfa", front("alfa", "a"))
        found = sk.discover(self.dirs)
        self.assertEqual([s.nome for s in found], ["alfa", "hud-custom", "zeta"])
        hc = found[1]
        self.assertEqual(hc.origens, ("hud", "claude", "codex"))
        self.assertEqual(hc.pasta, self.hud / "hud-custom")
        self.assertEqual(hc.arquivo, self.hud / "hud-custom" / "SKILL.md")
        self.assertEqual(hc.descricao, "do HUD")
        self.assertEqual(found[2].nome, "zeta")  # sem name: o nome da pasta
        self.assertEqual(found[0].origens, ("codex",))

    def test_missing_dir_hidden_and_files_ignored(self):
        make(self.claude, ".oculta", front("oculta"))
        make(self.claude, "sem-skill-md", front("x")).joinpath("SKILL.md").unlink()
        (self.claude / "solto.md").write_text("x")
        make(self.claude, "boa", front())
        found = sk.discover([("hud", self.tmp / "nao-existe")] + self.dirs)
        self.assertEqual([s.nome for s in found], ["boa"])

    def test_invalid_names_ignored(self):
        make(self.claude, "com espaco", front())
        make(self.claude, "ok", front("nome com espaço"))
        make(self.claude, "-traco", front())
        make(self.claude, "longo", front("a" * 65))
        make(self.claude, "x", front("pt:br_1.0-a"))
        self.assertEqual([s.nome for s in sk.discover(self.dirs)], ["pt:br_1.0-a"])

    def test_folder_link_followed(self):
        real = make(self.tmp / "fora", "minha", front("minha", "via link"))
        symlink(self, real, self.claude / "minha")
        found = sk.discover(self.dirs)
        self.assertEqual([(s.nome, s.descricao) for s in found], [("minha", "via link")])
        self.assertEqual(found[0].pasta, self.claude / "minha")

    def test_skill_md_link_to_regular_file_accepted(self):
        # Escolha documentada no módulo: o SKILL.md pode ser link se, resolvido,
        # for arquivo comum (a pasta já pode ser link para qualquer lugar).
        alvo = self.tmp / "alvo.md"
        alvo.write_text(front("ligada"), encoding="utf-8")
        d = self.claude / "ligada"
        d.mkdir()
        symlink(self, alvo, d / "SKILL.md")
        self.assertEqual([s.nome for s in sk.discover(self.dirs)], ["ligada"])

    def test_skill_md_link_to_non_regular_refused(self):
        d = self.claude / "pasta-no-lugar"
        d.mkdir()
        (self.tmp / "uma-pasta").mkdir()
        symlink(self, self.tmp / "uma-pasta", d / "SKILL.md")
        e = self.claude / "quebrado"
        e.mkdir()
        symlink(self, self.tmp / "nao-existe.md", e / "SKILL.md")
        self.assertEqual(sk.discover(self.dirs), [])

    @unittest.skipIf(WINDOWS or not hasattr(os, "mkfifo"), "FIFO só no POSIX")
    def test_fifo_refused_without_hanging(self):
        d = self.claude / "fifo"
        d.mkdir()
        os.mkfifo(d / "SKILL.md")
        self.assertEqual(sk.discover(self.dirs), [])

    def test_limit(self):
        for i in range(sk.MAX_SKILLS + 5):
            make(self.claude, f"s{i:04d}", "x")
        self.assertEqual(len(sk.discover(self.dirs)), sk.MAX_SKILLS)


class FrontmatterTest(unittest.TestCase):
    def test_quotes(self):
        m = sk.parse_frontmatter('---\nname: "a-b"\ndescription: \'Faz: isto\'\n---\n')
        self.assertEqual(m, {"name": "a-b", "description": "Faz: isto"})

    def test_folded_block(self):
        texto = ("---\nname: x\ndescription: >\n  Primeira linha\n  segunda linha\n\n"
                 "  terceira\nother: 1\n---\ncorpo\n")
        m = sk.parse_frontmatter(texto)
        self.assertEqual(m["description"], "Primeira linha segunda linha terceira")
        self.assertEqual(m["other"], "1")

    def test_literal_block_with_modifier(self):
        m = sk.parse_frontmatter("---\ndescription: |-\n  um\n  dois\n---\n")
        self.assertEqual(m["description"], "um dois")

    def test_no_frontmatter(self):
        self.assertEqual(sk.parse_frontmatter("# só corpo\nname: x\n"), {})

    def test_description_cleaned_and_cut(self):
        with tempfile.TemporaryDirectory() as t:
            make(Path(t), "s", front("s", "a\x1b]0;mal\x07b " + "c" * 300))
            (s,) = sk.discover([("claude", Path(t))])
        self.assertTrue(s.descricao.startswith("ab c"))
        self.assertEqual(len(s.descricao), sk.MAX_DESC)


class ReadAndPromptTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.base = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def one(self, texto):
        make(self.base, "s", texto)
        (s,) = sk.discover([("claude", self.base)])
        return s

    def test_read_ok(self):
        s = self.one(front("s") + "ção\n")
        self.assertTrue(sk.read_skill(s).endswith("ção\n"))

    def test_size_limit(self):
        s = self.one(front("s") + "x" * 2000)
        with self.assertRaises(sk.SkillError):
            sk.read_skill(s, limit=1024)
        self.assertIn("corpo", sk.read_skill(s, limit=4096))

    def test_invalid_utf8(self):
        s = self.one(front("s").encode() + b"\xff\xfe")
        with self.assertRaises(sk.SkillError) as cm:
            sk.read_skill(s)
        self.assertIn("UTF-8", str(cm.exception))

    def test_read_refuses_folder_swapped_in(self):
        s = self.one(front("s"))
        s.arquivo.unlink()
        s.arquivo.mkdir()
        with self.assertRaises(sk.SkillError):
            sk.read_skill(s)

    def test_find(self):
        a = sk.Skill("Hud-Custom", "", ("hud",), Path("a"), Path("a/SKILL.md"))
        b = sk.Skill("outra", "", ("claude",), Path("b"), Path("b/SKILL.md"))
        self.assertIs(sk.find([a, b], "hud-custom"), a)
        self.assertIs(sk.find([a, b], "OUTRA"), b)
        self.assertIsNone(sk.find([a, b], "hud"))

    def test_prompt_exact(self):
        s = sk.Skill("x", "", ("hud",), Path("/p/x"), Path("/p/x/SKILL.md"))
        pasta = str(Path("/p/x"))
        cab = (f"[Skill x — siga as instruções abaixo para atender ao pedido. "
               f"Arquivos auxiliares da skill ficam em {pasta}.]\nTEXTO\n[Fim da skill x]\n\n")
        self.assertEqual(sk.skill_prompt(s, "TEXTO", "faça y"), cab + "Pedido: faça y")
        self.assertEqual(sk.skill_prompt(s, "TEXTO", "faça y", "INTENÇÃO"),
                         cab + "INTENÇÃO\n\nPedido: faça y")
        self.assertEqual(sk.skill_prompt(s, "TEXTO", "  "),
                         cab + "Pedido: (sem texto: pergunte o que o usuário quer ou mostre "
                               "o que a skill faz)")


class SkillDirsTest(unittest.TestCase):
    def test_env(self):
        env = {"CLAUDE_CONFIG_DIR": "/c", "CODEX_HOME": "/x"}
        self.assertEqual(sk.skill_dirs(Path("/h/skills"), env), [
            ("hud", Path("/h/skills")), ("claude", Path("/c/skills")), ("codex", Path("/x/skills"))])

    def test_defaults_without_hud(self):
        home = Path.home()
        self.assertEqual(sk.skill_dirs(None, {}), [
            ("claude", home / ".claude" / "skills"), ("codex", home / ".codex" / "skills")])


if __name__ == "__main__":
    unittest.main()
