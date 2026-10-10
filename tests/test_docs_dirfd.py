"""A gravação do /doc salvar resiste à troca de uma pasta por link (AT-059).

A troca acontece num ponto fixo, entre a criação das pastas e a escrita
(`docs._antes_de_escrever`), então os testes são determinísticos. Cada caso roda
nos dois caminhos: por descritor de pasta (mkdirat/openat/renameat, Linux e
macOS) e pelo caminho conferido (o do Windows, forçado aqui com
`docs.DIR_FD = False`).
"""

import os
import shutil
import stat
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from hud import docs as D
from tests import test_docs as T
from tests.suporte import posix_only, symlink


class _Base:
    dir_fd = True

    def setUp(self):
        if self.dir_fd and not D.DIR_FD:
            self.skipTest("sem gravação relativa a pasta (openat) neste sistema")
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.base = Path(tmp.name)
        self.root = self.base / "hud"
        self.fora = self.base / "fora"
        self.movida = self.base / "movida"
        self.root.mkdir()
        self.fora.mkdir()
        p = mock.patch.object(D, "DIR_FD", self.dir_fd)
        p.start()
        self.addCleanup(p.stop)

    def props(self, *pares):
        return [D.DocProposal(a, c) for a, c in pares]

    def troca_no_meio(self, troca):
        feito = []

        def gancho(dest):
            if not feito:
                feito.append(dest)
                troca(dest)

        p = mock.patch.object(D, "_antes_de_escrever", gancho)
        p.start()
        self.addCleanup(p.stop)
        return feito

    def pasta_vira_link(self, rel):
        """Move a pasta `rel` para fora da raiz e põe no lugar um link para `fora`."""
        def troca(_dest):
            alvo = self.root / rel
            shutil.move(str(alvo), str(self.movida))
            symlink(self, self.fora, alvo)
        return troca

    def sem_tmp(self, *pastas):
        for pasta in pastas:
            for dirpath, _dirs, files in os.walk(pasta):
                self.assertEqual([f for f in files if f.endswith(".tmp")], [], dirpath)

    # ------------------------------------------------------------ troca por link

    def test_pasta_intermediaria_vira_link(self):
        feito = self.troca_no_meio(self.pasta_vira_link("a"))
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("a/b/c.md", "conteudo")))
        self.assertTrue(feito, "o gancho não rodou")
        self.assertEqual(os.listdir(self.fora), [])
        self.sem_tmp(self.movida)

    def test_pasta_do_arquivo_vira_link(self):
        feito = self.troca_no_meio(self.pasta_vira_link("a/b"))
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("a/b/c.md", "conteudo")))
        self.assertTrue(feito)
        self.assertEqual(os.listdir(self.fora), [])
        self.sem_tmp(self.movida)

    def test_pasta_existente_vira_link(self):
        (self.root / "docs").mkdir()
        T.touch(self.root / "docs/velha.md", "original")
        self.troca_no_meio(self.pasta_vira_link("docs"))
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("docs/velha.md", "novo")), sobrescrever=True)
        self.assertEqual(os.listdir(self.fora), [])
        self.assertEqual((self.movida / "velha.md").read_text(), "original")
        self.sem_tmp(self.movida)

    def test_pasta_apagada_e_trocada_por_link(self):
        def troca(_dest):
            shutil.rmtree(self.root / "a")
            symlink(self, self.fora, self.root / "a")

        self.troca_no_meio(troca)
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("a/c.md", "conteudo")))
        self.assertEqual(os.listdir(self.fora), [])

    def test_destino_vira_link(self):
        alvo = T.touch(self.fora / "alvo.md", "alvo")

        def troca(dest):
            symlink(self, alvo, dest)

        self.troca_no_meio(troca)
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("x/n.md", "novo")))
        self.assertEqual(alvo.read_text(), "alvo")
        self.assertEqual(os.listdir(self.fora), ["alvo.md"])
        self.sem_tmp(self.root)

    def test_destino_vira_pasta(self):
        self.troca_no_meio(lambda dest: dest.mkdir())
        with self.assertRaises(D.DocError):
            D.save_docs(self.root, self.props(("n.md", "novo")))
        self.sem_tmp(self.root)

    def test_raiz_vira_link(self):
        def troca(_dest):
            shutil.move(str(self.root), str(self.movida))
            symlink(self, self.fora, self.root)

        self.troca_no_meio(troca)
        try:
            D.save_docs(self.root, self.props(("n.md", "novo")))
        except D.DocError:
            pass
        # Com dir_fd a nota cai na raiz original (já aberta); sem, é recusada.
        self.assertEqual(os.listdir(self.fora), [])

    # ------------------------------------------------------------ sem troca

    def test_grava_varios_niveis_e_conteudo(self):
        conteudo = "# Ação ✓\n"
        got = D.save_docs(self.root, self.props(("a/b/c.md", conteudo), ("n.md", "n\n"),
                                                ("a/d.md", "d\n")))
        self.assertEqual(got, ["a/b/c.md", "n.md", "a/d.md"])
        self.assertEqual((self.root / "a/b/c.md").read_bytes(), conteudo.encode("utf-8"))
        self.assertEqual((self.root / "a/d.md").read_text(), "d\n")
        self.sem_tmp(self.root)

    @posix_only
    def test_modos_com_umask_077(self):
        (self.root / "ja").mkdir(mode=0o700)
        old = os.umask(0o077)
        try:
            D.save_docs(self.root, self.props(("a/b/c.md", "x"), ("ja/d.md", "y")))
        finally:
            os.umask(old)
        for d in ("a", "a/b"):
            self.assertEqual(stat.S_IMODE(os.stat(self.root / d).st_mode), 0o755, d)
        # pasta que já existia não muda de modo
        self.assertEqual(stat.S_IMODE(os.stat(self.root / "ja").st_mode), 0o700)
        for f in ("a/b/c.md", "ja/d.md"):
            self.assertEqual(stat.S_IMODE(os.stat(self.root / f).st_mode), 0o644, f)


class DirFdTest(_Base, unittest.TestCase):
    dir_fd = True


class SemDirFdTest(_Base, unittest.TestCase):
    dir_fd = False


class SaveSemDirFdTest(T.SaveTest):
    """Os testes de gravação de sempre, pelo caminho do Windows."""

    def setUp(self):
        super().setUp()
        p = mock.patch.object(D, "DIR_FD", False)
        p.start()
        self.addCleanup(p.stop)


if __name__ == "__main__":
    unittest.main()
