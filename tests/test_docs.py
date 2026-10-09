import os
import shutil
import stat
import tempfile
import unittest
from pathlib import Path

from hud import docs as D
from tests.suporte import posix_only, symlink


def touch(path: Path, texto: str = "x") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(texto, encoding="utf-8")
    return path


class FindProjectsTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_varied_structures(self):
        r = self.root
        touch(r / "personal/projetos/Hud/06-backlog/Kanban (Hud).md")
        touch(r / "personal/projetos/Mudarro/07-backlog/Kanban (Mudarro App).md")
        touch(r / "work/Outro/09-Backlog/Kanban (Outro).md")
        touch(r / "work/Moc/MOC.md")  # nome da pasta com outra caixa
        (r / "work/Moc/01-visao").mkdir()
        touch(r / "work/SemNN/SemNN.md")  # MOC sem subpasta NN: não é projeto
        (r / "work/SemNN/notas").mkdir()
        touch(r / "work/SoNN/01-a/x.md")  # NN sem MOC: não é projeto
        touch(r / "work/Backlog-sem-kanban/06-backlog/outra.md")
        got = D.find_projects(r)
        self.assertEqual(got, [
            D.Project("Hud", "personal/projetos/Hud", "personal/projetos/Hud/06-backlog/Kanban (Hud).md"),
            D.Project("Mudarro App", "personal/projetos/Mudarro",
                      "personal/projetos/Mudarro/07-backlog/Kanban (Mudarro App).md"),
            D.Project("MOC", "work/Moc", None),
            D.Project("Outro", "work/Outro", "work/Outro/09-Backlog/Kanban (Outro).md"),
        ])

    def test_hidden_and_skip_dirs_ignored(self):
        touch(self.root / ".obsidian/P/06-backlog/Kanban (P).md")
        touch(self.root / ".oculta/P/06-backlog/Kanban (P).md")
        touch(self.root / "node_modules/P/06-backlog/Kanban (P).md")
        touch(self.root / "a/.trash/06-backlog/Kanban (Lixo).md")
        self.assertEqual(D.find_projects(self.root), [])

    def test_does_not_descend_into_project(self):
        touch(self.root / "P/06-backlog/Kanban (P).md")
        touch(self.root / "P/sub/Q/06-backlog/Kanban (Q).md")
        self.assertEqual([p.nome for p in D.find_projects(self.root)], ["P"])

    def test_link_not_followed(self):
        fora = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, fora)
        touch(fora / "P/06-backlog/Kanban (P).md")
        symlink(self, fora / "P", self.root / "P")
        symlink(self, fora, self.root / "fora")
        self.assertEqual(D.find_projects(self.root), [])

    def test_kanban_link_ignored(self):
        touch(self.root / "alvo.md")
        (self.root / "P/06-backlog").mkdir(parents=True)
        symlink(self, self.root / "alvo.md", self.root / "P/06-backlog/Kanban (P).md")
        self.assertEqual(D.find_projects(self.root), [])

    def test_depth_and_limit(self):
        touch(self.root / "a/b/c/P/06-backlog/Kanban (P).md")
        self.assertEqual(len(D.find_projects(self.root, max_depth=4)), 1)
        self.assertEqual(D.find_projects(self.root, max_depth=3), [])
        for i in range(5):
            touch(self.root / f"x{i}/06-backlog/Kanban (X{i}).md")
        self.assertEqual(len(D.find_projects(self.root, limit=3)), 3)

    def test_missing_root(self):
        self.assertEqual(D.find_projects(self.root / "nao"), [])


class ParseTest(unittest.TestCase):
    def test_four_backticks_with_inner_fence(self):
        texto = ("Aqui está:\n\n````hud-doc arquivo=\"docs/Visão geral (Hud) — rascunho.md\"\n"
                 "# Título\n\n```python\nprint(1)\n```\n````\nfim\n")
        (p,) = D.parse_doc_proposals(texto)
        self.assertEqual(p.arquivo, "docs/Visão geral (Hud) — rascunho.md")
        self.assertEqual(p.conteudo, "# Título\n\n```python\nprint(1)\n```\n")

    def test_attr_forms_and_tildes(self):
        texto = ("```hud-doc arquivo='a b.md'\nA\n```\n"
                 "~~~~hud-doc arquivo=c/d.md\nC\n~~~~~\n"
                 "```hud-doc arquivo=\"e.md\" outro=1\nE\n```\n")
        got = D.parse_doc_proposals(texto)
        self.assertEqual([(p.arquivo, p.conteudo) for p in got],
                         [("a b.md", "A\n"), ("c/d.md", "C\n"), ("e.md", "E\n")])

    def test_shorter_fence_does_not_close(self):
        texto = "````hud-doc arquivo=a.md\n```\nx\n```\n"
        self.assertEqual(D.parse_doc_proposals(texto), [])  # nunca fechou

    def test_hostile_paths_discarded(self):
        hostis = ["../x.md", "/etc/x.md", "a\\b.md", "C:x.md", ".obsidian/x.md", "a/./b.md",
                  "CON.md", "con.notas.md", "LPT9.md", "x.txt", "a./b.md", "a /b.md", "a//b.md",
                  "a/../b.md", "x\x1b.md", "x‮.md", "", "a" * 121 + ".md",
                  "/".join(["d"] * 8) + "/x.md", "a<b.md", "a?.md"]
        texto = "".join(f"```hud-doc arquivo=\"{h}\"\nx\n```\n" for h in hostis)
        self.assertEqual(D.parse_doc_proposals(texto), [])
        for h in hostis:
            self.assertFalse(D.valid_path(h), h)

    def test_valid_paths(self):
        for ok in ["x.md", "X.MD", "06-backlog/Kanban (Hud).md", "Notas/Ação – teste.md",
                   "/".join(["d"] * 7) + "/x.md", "CONSOLE.md", "COM10.md", "a" * 117 + ".md"]:
            self.assertTrue(D.valid_path(ok), ok)

    def test_unclosed_and_repeated(self):
        texto = ("```hud-doc arquivo=a.md\nv1\n```\n```hud-doc arquivo=b.md\nB\n```\n"
                 "```hud-doc arquivo=a.md\nv2\n```\n```hud-doc arquivo=c.md\nnunca fecha\n")
        got = D.parse_doc_proposals(texto)
        self.assertEqual([(p.arquivo, p.conteudo) for p in got], [("b.md", "B\n"), ("a.md", "v2\n")])

    def test_limits(self):
        grande = "x" * D.MAX_DOC
        texto = f"```hud-doc arquivo=g.md\n{grande}\n```\n```hud-doc arquivo=p.md\nok\n```\n"
        self.assertEqual([p.arquivo for p in D.parse_doc_proposals(texto)], ["p.md"])
        muitos = "".join(f"```hud-doc arquivo=n{i}.md\nx\n```\n" for i in range(D.MAX_DOCS + 10))
        self.assertEqual(len(D.parse_doc_proposals(muitos)), D.MAX_DOCS)
        meio = "y" * (D.MAX_DOC - 10)
        total = "".join(f"```hud-doc arquivo=t{i}.md\n{meio}\n```\n" for i in range(20))
        got = D.parse_doc_proposals(total)
        self.assertEqual(len(got), D.MAX_TOTAL // (D.MAX_DOC - 9))
        self.assertLessEqual(sum(len(p.conteudo.encode()) for p in got), D.MAX_TOTAL)

    def test_crlf(self):
        (p,) = D.parse_doc_proposals("```hud-doc arquivo=a.md\r\nl1\r\n```\r\n")
        self.assertEqual(p.conteudo, "l1\n")


class SaveTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name) / "hud"
        self.root.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def props(self, *pares):
        return [D.DocProposal(a, c) for a, c in pares]

    def test_plan_new_and_existing(self):
        touch(self.root / "docs/velha.md")
        novos, existentes = D.plan(self.root, self.props(("docs/velha.md", "a"),
                                                         ("docs/nova.md", "b"), ("x/y/z.md", "c")))
        self.assertEqual((novos, existentes), (["docs/nova.md", "x/y/z.md"], ["docs/velha.md"]))

    def test_already_exists_writes_nothing(self):
        touch(self.root / "velha.md", "original")
        with self.assertRaises(D.AlreadyExists) as cm:
            D.save_docs(self.root, self.props(("nova/n.md", "n"), ("velha.md", "novo")))
        self.assertEqual(cm.exception.existentes, ["velha.md"])
        self.assertFalse((self.root / "nova").exists())
        self.assertEqual((self.root / "velha.md").read_text(), "original")

    def test_overwrite_and_exact_content(self):
        touch(self.root / "velha.md", "original")
        conteudo = "# Ação ✓\n\n```\ncódigo\n```\n"
        got = D.save_docs(self.root, self.props(("velha.md", conteudo), ("Nova (a) — b/n.md", "n\n")),
                          sobrescrever=True)
        self.assertEqual(got, ["velha.md", "Nova (a) — b/n.md"])
        self.assertEqual((self.root / "velha.md").read_bytes(), conteudo.encode("utf-8"))
        self.assertEqual((self.root / "Nova (a) — b/n.md").read_bytes(), b"n\n")
        self.assertEqual([f for f in os.listdir(self.root) if f.endswith(".tmp")], [])

    def test_invalid_path_refused_before_writing(self):
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("ok.md", "x"), ("../fora.md", "y")))
        self.assertEqual(os.listdir(self.root), [])

    def test_link_in_middle(self):
        fora = Path(self._tmp.name) / "fora"
        fora.mkdir()
        symlink(self, fora, self.root / "docs")
        with self.assertRaises(D.DocError):
            D.plan(self.root, self.props(("docs/x.md", "x")))
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("docs/x.md", "x")), sobrescrever=True)
        self.assertEqual(os.listdir(fora), [])

    def test_link_at_destination(self):
        alvo = touch(Path(self._tmp.name) / "alvo.md", "alvo")
        symlink(self, alvo, self.root / "x.md")
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("x.md", "novo")), sobrescrever=True)
        self.assertEqual(alvo.read_text(), "alvo")

    def test_existing_non_regular(self):
        (self.root / "pasta.md").mkdir()
        touch(self.root / "arq")
        with self.assertRaises(D.DocError):
            D.plan(self.root, self.props(("pasta.md", "x")))
        with self.assertRaises(D.DocError):
            D.plan(self.root, self.props(("arq/x.md", "x")))

    def test_root_link_or_missing(self):
        link = Path(self._tmp.name) / "link"
        symlink(self, self.root, link)
        with self.assertRaises(D.DocError):
            D.save_docs(link, self.props(("x.md", "x")))
        with self.assertRaises(D.DocError):
            D.save_docs(Path(self._tmp.name) / "nao", self.props(("x.md", "x")))
        self.assertEqual(os.listdir(self.root), [])

    def test_destination_becomes_link_before_replace(self):
        alvo = touch(Path(self._tmp.name) / "alvo.md", "alvo")
        dest = self.root / "x.md"
        orig = D.plat.open_nofollow

        def troca(path, flags, mode=0o777):
            fd = orig(path, flags, mode)
            if not dest.is_symlink():
                symlink(self, alvo, dest)
            return fd

        D.plat.open_nofollow = troca
        try:
            with self.assertRaises(D.DocError):
                D.save_docs(self.root, self.props(("x.md", "novo")))
        finally:
            D.plat.open_nofollow = orig
        self.assertEqual(alvo.read_text(), "alvo")
        self.assertEqual([f for f in os.listdir(self.root) if f.endswith(".tmp")], [])

    @posix_only
    def test_modes_with_umask_077(self):
        old = os.umask(0o077)
        try:
            D.save_docs(self.root, self.props(("a/b/c.md", "x")))
        finally:
            os.umask(old)
        for d in ("a", "a/b"):
            self.assertEqual(stat.S_IMODE(os.stat(self.root / d).st_mode), 0o755, d)
        self.assertEqual(stat.S_IMODE(os.stat(self.root / "a/b/c.md").st_mode), 0o644)

    def test_empty(self):
        self.assertEqual(D.save_docs(self.root, []), [])
        self.assertEqual(D.plan(self.root, []), ([], []))


if __name__ == "__main__":
    unittest.main()
