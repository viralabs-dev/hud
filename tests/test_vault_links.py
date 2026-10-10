"""A leitura do Vault resiste à troca de links durante a varredura (AT-056).

A troca acontece num ponto fixo, entre a checagem (lstat) e a abertura
(`vault._antes_de_abrir`), então os testes são determinísticos. Cada caso roda
nos dois caminhos de leitura: por descritor de pasta (`os.fwalk` + abertura
relativa, POSIX) e por caminho conferido (o do Windows, forçado aqui com
`vault.DIR_FD = False`).
"""

import os
import shutil
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from hud import vault
from hud.vault import VaultWatcher, read_note

from tests.suporte import WINDOWS, posix_only, symlink

MARCA = "MARCADOR-FORA-DO-VAULT"
FORA = f"---\nkanban-plugin: board\ntitle: {MARCA}\n---\n## A fazer\n- [ ] {MARCA} 📅 2026-10-10\n"
DENTRO = "nota comum\n- [ ] tarefa de dentro 📅 2026-10-11\n"


def dir_link(tc, src, dst) -> None:
    """Link para pasta: simbólico ou, no Windows sem privilégio, junção."""
    try:
        os.symlink(src, dst, target_is_directory=True)
        return
    except (OSError, NotImplementedError) as e:
        erro = e
    if WINDOWS:
        cmd = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "cmd.exe")
        r = subprocess.run([cmd, "/c", "mklink", "/J", str(dst), str(src)], capture_output=True)
        if r.returncode == 0:
            return
    tc.skipTest(f"sem permissão para criar link de pasta: {erro}")


class _Base:
    dir_fd = True

    def setUp(self):
        if self.dir_fd and not vault.DIR_FD:
            self.skipTest("sem abertura relativa a pasta (openat) neste sistema")
        self.root = Path(tempfile.mkdtemp(prefix="hud-vault-"))
        self.fora = Path(tempfile.mkdtemp(prefix="hud-fora-"))
        self.addCleanup(shutil.rmtree, self.root, True)
        self.addCleanup(shutil.rmtree, self.fora, True)
        (self.root / "sub").mkdir()
        self.nota = self.root / "sub" / "nota.md"
        self.nota.write_text(DENTRO, encoding="utf-8")
        (self.fora / "nota.md").write_text(FORA, encoding="utf-8")
        p = mock.patch.object(vault, "DIR_FD", self.dir_fd)
        p.start()
        self.addCleanup(p.stop)

    def troca_no_meio(self, troca):
        """Roda `troca(path)` uma vez, quando a varredura vai abrir a nota."""
        feito = []

        def gancho(path):
            if os.path.basename(path) == "nota.md" and not feito:
                feito.append(path)
                troca(path)

        p = mock.patch.object(vault, "_antes_de_abrir", gancho)
        p.start()
        self.addCleanup(p.stop)
        return feito

    def sem_marca(self, snap):
        self.assertTrue(snap.ok, snap.error)
        self.assertNotIn(MARCA, repr(snap.tasks) + repr(snap.boards))

    # ------------------------------------------------------------ troca por link

    def test_arquivo_vira_link_entre_checagem_e_abertura(self):
        def troca(path):
            os.unlink(path)
            symlink(self, self.fora / "nota.md", path)

        feito = self.troca_no_meio(troca)
        snap = VaultWatcher(self.root).scan()
        self.assertTrue(feito, "o gancho não rodou")
        self.sem_marca(snap)
        self.assertEqual(snap.boards, [])

    def test_arquivo_vira_link_na_busca(self):
        def troca(path):
            os.unlink(path)
            symlink(self, self.fora / "nota.md", path)

        feito = self.troca_no_meio(troca)
        hits = VaultWatcher(self.root).search(MARCA)
        self.assertTrue(feito)
        self.assertEqual(hits, [])

    def test_pasta_do_caminho_vira_link(self):
        def troca(path):
            os.rename(self.root / "sub", self.root / "sub-antiga")
            dir_link(self, self.fora, self.root / "sub")

        feito = self.troca_no_meio(troca)
        snap = VaultWatcher(self.root).scan()
        self.assertTrue(feito)
        self.sem_marca(snap)

    def test_pasta_do_caminho_vira_link_na_busca(self):
        def troca(path):
            os.rename(self.root / "sub", self.root / "sub-antiga")
            dir_link(self, self.fora, self.root / "sub")

        feito = self.troca_no_meio(troca)
        self.assertEqual(VaultWatcher(self.root).search(MARCA), [])
        self.assertTrue(feito)

    def test_trocado_por_outro_arquivo_comum_nao_entra_no_cache(self):
        """Outro inode no lugar: a leitura recusa e a próxima varredura relê."""
        def troca(path):
            tmp = self.root / "sub" / "outro.tmp"
            tmp.write_text(FORA, encoding="utf-8")
            os.replace(tmp, path)

        self.troca_no_meio(troca)
        w = VaultWatcher(self.root)
        self.sem_marca(w.scan())
        self.assertNotIn("sub/nota.md", {k.replace(os.sep, "/") for k in w._cache})
        # Agora é a nota de verdade (está dentro do Vault): entra na próxima.
        self.assertIn(MARCA, repr(w.scan().tasks))

    # ------------------------------------------------------------ links parados

    def test_links_parados_nao_sao_lidos(self):
        symlink(self, self.fora / "nota.md", self.root / "link.md")
        dir_link(self, self.fora, self.root / "pasta-link")
        w = VaultWatcher(self.root)
        snap = w.scan()
        self.sem_marca(snap)
        self.assertEqual(snap.notes, 1)
        self.assertEqual(w.search(MARCA), [])

    # ------------------------------------------------------------ tamanho

    def test_arquivo_maior_que_o_limite(self):
        with mock.patch.object(vault, "MAX_NOTE_BYTES", 64):
            (self.root / "grande.md").write_text(f"- [ ] {MARCA} 📅 2026-10-10\n" + "x" * 200, encoding="utf-8")
            w = VaultWatcher(self.root)
            self.sem_marca(w.scan())
            self.assertEqual([h for h in w.search(MARCA) if h[1]], [])

    def test_arquivo_cresce_entre_checagem_e_abertura(self):
        def troca(path):
            with open(path, "a", encoding="utf-8") as f:
                f.write(f"- [ ] {MARCA} 📅 2026-10-10\n" + "x" * 200)

        with mock.patch.object(vault, "MAX_NOTE_BYTES", 128):
            self.troca_no_meio(troca)
            self.sem_marca(VaultWatcher(self.root).scan())

    def test_read_note_corta_no_limite_mesmo_crescendo(self):
        self.nota.write_text("a" * 100, encoding="utf-8")
        self.assertIsNone(read_note(str(self.nota), limit=99))
        self.assertEqual(read_note(str(self.nota), limit=100), "a" * 100)

    def test_read_note_normaliza_quebras_como_o_modo_texto(self):
        self.nota.write_bytes(b"a\r\nb\rc\n\xff")
        self.assertEqual(read_note(str(self.nota)), "a\nb\nc\n\ufffd")


class PorCaminhoConferido(_Base, unittest.TestCase):
    """O caminho do Windows (sem openat), exercitado em todo sistema."""
    dir_fd = False


class PorDescritorDePasta(_Base, unittest.TestCase):
    dir_fd = True


@posix_only
class FifoTest(unittest.TestCase):
    """FIFO com nome de nota não pode travar a varredura nem a busca."""

    def _sem_travar(self, fn):
        out = []
        t = threading.Thread(target=lambda: out.append(fn()), daemon=True)
        t.start()
        t.join(10)
        self.assertFalse(t.is_alive(), "travou num FIFO")
        return out[0]

    def _casos(self):
        return [True, False] if vault.DIR_FD else [False]

    def test_fifo_parado(self):
        for dir_fd in self._casos():
            with self.subTest(dir_fd=dir_fd), tempfile.TemporaryDirectory() as d, \
                    mock.patch.object(vault, "DIR_FD", dir_fd):
                os.mkfifo(os.path.join(d, "fila.md"))
                Path(d, "nota.md").write_text(DENTRO, encoding="utf-8")
                w = VaultWatcher(Path(d))
                snap = self._sem_travar(w.scan)
                self.assertEqual(snap.notes, 1)
                self.assertEqual(self._sem_travar(lambda: w.search("qualquer")), [])

    def test_vira_fifo_entre_checagem_e_abertura(self):
        for dir_fd in self._casos():
            with self.subTest(dir_fd=dir_fd), tempfile.TemporaryDirectory() as d, \
                    mock.patch.object(vault, "DIR_FD", dir_fd):
                nota = Path(d, "nota.md")
                nota.write_text(DENTRO, encoding="utf-8")

                def gancho(path):
                    if os.path.exists(path) and not os.path.isfile(path):
                        return
                    os.unlink(path)
                    os.mkfifo(path)

                with mock.patch.object(vault, "_antes_de_abrir", gancho):
                    snap = self._sem_travar(VaultWatcher(Path(d)).scan)
                self.assertTrue(snap.ok)
                self.assertEqual(snap.tasks, [])

    def test_read_note_recusa_fifo_e_pasta(self):
        with tempfile.TemporaryDirectory() as d:
            os.mkfifo(os.path.join(d, "f.md"))
            self.assertIsNone(self._sem_travar(lambda: read_note(os.path.join(d, "f.md"))))
            self.assertIsNone(read_note(d))


class ReadNoteTest(unittest.TestCase):
    def test_link_no_ultimo_componente(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as fora:
            Path(fora, "s.md").write_text(MARCA, encoding="utf-8")
            symlink(self, Path(fora, "s.md"), Path(d, "l.md"))
            self.assertIsNone(read_note(os.path.join(d, "l.md")))

    def test_inode_diferente_da_checagem(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = Path(d, "a.md"), Path(d, "b.md")
            a.write_text("a", encoding="utf-8")
            b.write_text("b", encoding="utf-8")
            self.assertIsNone(read_note(str(a), expect=os.lstat(b)))
            self.assertEqual(read_note(str(a), expect=os.lstat(a)), "a")

    def test_fora_da_raiz_resolvida(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as fora:
            Path(fora, "s.md").write_text(MARCA, encoding="utf-8")
            caminho = os.path.join(fora, "s.md")
            self.assertIsNone(read_note(caminho, root_real=os.path.realpath(d)))
            self.assertEqual(read_note(caminho, root_real=os.path.realpath(fora)), MARCA)


if __name__ == "__main__":
    unittest.main()
