import json
import os
import stat
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from hud import custom as cu
from hud import layout as L
from hud import plataforma as plat
from tests.suporte import ECHO, ECHO_ARGV, SLEEP, WINDOWS, home_env, posix_only, symlink, windows_only

BASE = {"coluna": [{"paineis": ["sistema", "saida"]}]}


def lay(**over):
    d = {"nome": "t", "coluna": [{"paineis": ["sistema", "saida"]}]}
    d.update(over)
    return d


def _try_unlink(path: Path) -> bool:
    try:
        path.unlink()
    except PermissionError:
        return False
    return True


def wait_for(cond, timeout=5.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if cond():
            return True
        time.sleep(0.05)
    return False


class ParseTest(unittest.TestCase):
    def ok(self, data, nome="t", base=None):
        return L.parse(data, nome, base)

    def bad(self, data, frag=None):
        with self.assertRaises(L.LayoutError) as cm:
            L.parse(data, "t", None)
        if frag:
            self.assertIn(frag, str(cm.exception))

    def test_valid_full_example(self):
        data = {
            "nome": "foco", "descricao": "x\x1b]0;evil\x07y", "autor": "daniel",
            "coluna": [
                {"largura": 34, "paineis": [{"id": "sistema", "altura": 11}, "comandos",
                                            {"id": "agenda", "peso": 1}, "uso_claude", "uso_codex"]},
                {"paineis": [{"id": "pasta", "peso": 55}, {"id": "saida", "peso": 45}, "lembretes"]},
            ],
            "painel": [
                {"id": "lembretes", "titulo": "LEMBRETES", "tipo": "texto", "arquivo": "lembretes.md"},
                {"id": "eco", "tipo": "comando", "argv": [ECHO, "oi"], "intervalo": 5, "timeout": 2},
            ],
        }
        lay_ = self.ok(data, "foco")
        self.assertEqual(lay_.descricao, "xy")
        self.assertEqual(lay_.colunas[1].slots[0].id, "vault")  # alias
        self.assertEqual(lay_.colunas[0].slots[1].altura, "auto")
        self.assertEqual(lay_.colunas[1].slots[2].peso, 1.0)  # padrão do painel próprio
        self.assertTrue(os.path.isabs(lay_.paineis["eco"].argv[0]))
        self.assertTrue(any("'eco'" in a for a in lay_.avisos))  # declarado e não usado
        self.assertEqual(lay_.paineis["eco"].titulo, "ECO")

    def test_name_from_folder_and_meta_cut(self):
        lay_ = L.parse(lay(nome="outro", descricao="d" * 300, autor="a" * 100), "foco", None)
        self.assertEqual(lay_.nome, "foco")
        self.assertTrue(any("difere" in a for a in lay_.avisos))
        self.assertEqual((len(lay_.descricao), len(lay_.autor)), (200, 60))

    def test_columns_and_panels_count(self):
        self.bad({"coluna": []}, "1 a 4")
        self.bad({"coluna": [{"paineis": ["saida"]}] * 5}, "1 a 4")
        self.bad({"coluna": [{"paineis": []}]}, "1 a 8")
        many = ["sistema", "comandos", "agenda", "uso_claude", "uso_codex", "vault", "saida", "x"]
        self.bad({"coluna": [{"paineis": many + ["y"]}]}, "1 a 8")
        self.ok({"coluna": [{"paineis": ["saida"]}] + [{"paineis": [b]} for b in ("vault", "agenda", "sistema")]})

    def test_saida_required_and_unique_and_known(self):
        self.bad({"coluna": [{"paineis": ["sistema"]}]}, "saida")
        self.bad({"coluna": [{"paineis": ["saida", "saida"]}]}, "mais de uma vez")
        self.bad({"coluna": [{"paineis": ["vault", "pasta", "saida"]}]}, "mais de uma vez")
        self.bad({"coluna": [{"paineis": ["saida", "fantasma"]}]}, "fantasma")

    def test_altura_peso(self):
        def col(slot):
            return {"coluna": [{"paineis": [slot, "saida"]}]}
        self.ok(col({"id": "sistema", "altura": 3}))
        self.ok(col({"id": "sistema", "altura": 60}))
        self.ok(col({"id": "sistema", "peso": 0.5}))
        self.ok(col({"id": "sistema", "peso": 100}))
        for bad in ({"altura": 2}, {"altura": 61}, {"altura": 5.5}, {"altura": True},
                    {"altura": "auto"}, {"peso": 0}, {"peso": 101}, {"peso": "1"},
                    {"altura": 5, "peso": 1}, {"cor": 1}):
            self.bad(col({"id": "sistema", **bad}))

    def test_largura(self):
        def cols(*ls):
            out = [{"paineis": ["saida"]}]
            out += [{"paineis": [b]} for b in ("vault", "agenda", "sistema")[:len(ls) - 1]]
            for c, l in zip(out, ls):
                if l is not None:
                    c["largura"] = l
            return {"coluna": out}
        self.ok(cols(15, None))
        self.ok(cols(85, None))
        self.bad(cols(14, None), "15 a 85")
        self.bad(cols(86, None), "15 a 85")
        self.ok(cols(45, 45, None))
        self.bad(cols(46, 45, None), "90%")
        self.ok(cols(50, 50))
        self.bad(cols(50, 51), "100%")

    def test_custom_panels(self):
        def with_panel(p):
            return {"coluna": [{"paineis": ["saida", p.get("id", "x")]}], "painel": [p]}
        self.ok(with_panel({"id": "notas", "tipo": "texto", "arquivo": "a.txt"}))
        self.bad(with_panel({"id": "Notas", "tipo": "texto", "arquivo": "a.md"}), "inválido")
        self.bad(with_panel({"id": "agenda", "tipo": "texto", "arquivo": "a.md"}), "embutido")
        self.bad(with_panel({"id": "pasta", "tipo": "texto", "arquivo": "a.md"}), "embutido")
        self.bad(with_panel({"id": "x", "tipo": "web"}), "tipo")
        self.bad(with_panel({"id": "x", "tipo": "texto", "arquivo": "a.md", "titulo": "t" * 31}), "30")
        for arq in ("../a.md", "sub/a.md", "a.py", "", ".a.md", None):
            self.bad(with_panel({"id": "x", "tipo": "texto", "arquivo": arq}))
        self.bad(with_panel({"id": "x", "tipo": "arquivo"}), "caminho")
        self.bad(with_panel({"id": "x", "tipo": "arquivo", "caminho": "rel/log"}), "absoluto")
        # "/var/log/syslog" não é absoluto no Windows (falta a unidade)
        log = r"C:\Logs\app.log" if WINDOWS else "/var/log/syslog"
        self.ok(with_panel({"id": "x", "tipo": "arquivo", "caminho": log}))
        self.bad(with_panel({"id": "x", "tipo": "arquivo", "caminho": log, "linhas": 2001}))
        self.bad(with_panel({"id": "x", "tipo": "comando"}), "argv")
        self.bad(with_panel({"id": "x", "tipo": "comando", "argv": ["sudo", "ls"]}), "não é permitido")
        self.bad(with_panel({"id": "x", "tipo": "comando", "argv": ["bash", "-c", "id"]}))
        self.bad(with_panel({"id": "x", "tipo": "comando", "argv": [ECHO], "intervalo": 4}), "intervalo")
        self.bad(with_panel({"id": "x", "tipo": "comando", "argv": [ECHO], "timeout": 61}), "timeout")
        self.bad({"coluna": [{"paineis": ["saida"]}],
                  "painel": [{"id": "x", "tipo": "texto", "arquivo": "a.md"}] * 2}, "duas vezes")

    def test_denied_paths(self):
        with tempfile.TemporaryDirectory() as home:
            os.makedirs(f"{home}/.ssh")
            os.makedirs(f"{home}/logs")
            symlink(self, f"{home}/.ssh", f"{home}/logs/disfarce")
            with mock.patch.dict(os.environ, home_env(home)):
                for cam in ("~/.ssh", "~/.ssh/config", f"{home}/.aws/credentials",
                            "~/.claude/.credentials.json", "~/.claude.json", "~/.codex/auth.json",
                            "~/.netrc", "~/.pgpass", "~/.local/share/keyrings/x",
                            "~/proj/.env.local", "/srv/x.pem", "/srv/server.key",
                            "/srv/id_rsa.pub", "/srv/id_ed25519", "~/logs/disfarce/known_hosts"):
                    self.assertIsNotNone(L.denied_path(cam), cam)
                    data = {"coluna": [{"paineis": ["saida", "x"]}],
                            "painel": [{"id": "x", "tipo": "arquivo", "caminho": cam}]}
                    self.bad(data)
                self.assertIsNone(L.denied_path("~/logs/app.log"))
                self.assertIsNone(L.denied_path("~/.sshfoo"))

    def test_toml_errors_and_size(self):
        with self.assertRaises(L.LayoutError) as cm:
            L.parse_text("coluna = [", "t")
        self.assertIn("TOML inválido", str(cm.exception))
        with self.assertRaises(L.LayoutError):
            L.parse_text("# " + "x" * L.MAX_FILE, "t")

    def test_load_refuses_symlinks_and_subdirs(self):
        with tempfile.TemporaryDirectory() as d:
            c = Path(d) / "a"
            c.mkdir()
            real = Path(d) / "real.toml"
            real.write_text('[[coluna]]\npaineis = ["saida"]\n')
            symlink(self, real, c / "layout.toml")
            with self.assertRaises(L.LayoutError):
                L.load(c)
            (c / "layout.toml").unlink()
            (c / "layout.toml").write_text(real.read_text())
            self.assertEqual(L.load(c).nome, "a")
            (c / "sub").mkdir()
            with self.assertRaises(L.LayoutError):
                L.load(c)
            (c / "sub").rmdir()
            (c / "big.md").write_text("x" * (L.MAX_FILE + 1))
            L.load(c)  # o texto é checado ao ler, no PanelFeed


class ComputeTest(unittest.TestCase):
    def assert_tiles(self, boxes, W, H):
        grid = [[0] * W for _ in range(H)]
        for y, x, h, w in boxes.values():
            self.assertGreaterEqual(h, 3)
            for r in range(y, y + h):
                for c in range(x, x + w):
                    grid[r][c] += 1
        self.assertTrue(all(v == 1 for row in grid for v in row), "caixas não cobrem a tela")

    def test_default_120x37(self):
        b = L.compute(L.DEFAULT, 120, 37, {"comandos": 11, "uso_codex": 4})
        self.assert_tiles(b, 120, 37)
        self.assertEqual(b["sistema"], (0, 0, 11, 43))
        self.assertEqual(b["comandos"], (11, 0, 11, 43))
        self.assertEqual(b["agenda"], (22, 0, 7, 43))
        self.assertEqual(b["uso_claude"], (29, 0, 4, 43))
        self.assertEqual(b["uso_codex"], (33, 0, 4, 43))
        self.assertEqual(b["vault"], (0, 43, 20, 77))
        self.assertEqual(b["saida"], (20, 43, 17, 77))

    def test_default_80x21_shrinks_fixed_from_the_bottom(self):
        b = L.compute(L.DEFAULT, 80, 21, {"comandos": 11, "uso_codex": 4})
        self.assert_tiles(b, 80, 21)
        self.assertEqual(b["sistema"][3], 38)  # clamp do layout embutido
        self.assertEqual([b[i][2] for i in ("sistema", "comandos", "agenda", "uso_claude", "uso_codex")],
                         [9, 3, 3, 3, 3])
        with self.assertRaises(L.TooSmall):
            L.compute(L.DEFAULT, 80, 14, {"comandos": 11, "uso_codex": 4})

    def test_widths(self):
        lay_ = L.parse({"coluna": [{"largura": 34, "paineis": ["sistema"]}, {"paineis": ["vault"]},
                                   {"paineis": ["saida"]}]}, "t", None)
        b = L.compute(lay_, 121, 30)
        self.assertEqual([b[i][3] for i in ("sistema", "vault", "saida")], [41, 40, 40])
        self.assertEqual([b[i][1] for i in ("sistema", "vault", "saida")], [0, 41, 81])
        self.assert_tiles(b, 121, 30)
        with self.assertRaises(L.TooSmall):
            L.compute(lay_, 89, 30)

    def test_weights_auto_and_growth(self):
        lay_ = L.parse({"coluna": [
            {"paineis": [{"id": "comandos"}, {"id": "vault", "peso": 3}, {"id": "agenda", "peso": 1}]},
            {"paineis": [{"id": "sistema", "altura": 5}, {"id": "saida", "altura": 4}]},
        ]}, "t", None)
        b = L.compute(lay_, 80, 30, {"comandos": 6})
        self.assertEqual([b[i][2] for i in ("comandos", "vault", "agenda")], [6, 18, 6])
        self.assertEqual(b["saida"], (5, 40, 25, 40))  # sem peso: o último cresce
        self.assert_tiles(b, 80, 30)
        tiny = L.parse({"coluna": [{"paineis": [{"id": "vault", "peso": 100},
                                                {"id": "saida", "peso": 1}]}]}, "t", None)
        b = L.compute(tiny, 40, 20)
        self.assertEqual((b["vault"][2], b["saida"][2]), (17, 3))  # mínimo de 3
        with self.assertRaises(L.TooSmall):
            L.compute(tiny, 40, 5)


class TrustTest(unittest.TestCase):
    @windows_only
    def test_trust_windows_outside_profile(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as outro:
            t = cu.Trust(Path(d) / "dados")
            t.trust("foco", "abc")
            self.assertTrue(t.is_trusted("foco", "abc"))
            fora = {k: os.path.join(outro, "perfil") for k in ("USERPROFILE", "APPDATA", "LOCALAPPDATA")}
            with mock.patch.dict(os.environ, fora):
                self.assertFalse(t.is_trusted("foco", "abc"))  # fora do perfil não vale

    @posix_only
    def test_trust_roundtrip_and_permissions(self):
        with tempfile.TemporaryDirectory() as d:
            t = cu.Trust(Path(d) / "dados")
            self.assertFalse(t.is_trusted("foco", "abc"))
            t.trust("foco", "abc")
            self.assertTrue(t.is_trusted("foco", "abc"))
            self.assertFalse(t.is_trusted("foco", "outro"))
            f = Path(d) / "dados" / cu.TRUST_FILE
            self.assertEqual(stat.S_IMODE(f.stat().st_mode), 0o600)
            self.assertEqual(json.loads(f.read_text()), {"foco": "abc"})
            os.chmod(f, 0o666)
            self.assertFalse(t.is_trusted("foco", "abc"))  # arquivo aberto a outros não vale

    def test_digest_changes_with_content(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "foco").mkdir()
            (Path(d) / "foco" / "layout.toml").write_text('[[coluna]]\npaineis = ["saida"]\n')
            a = cu.digest(Path(d), "foco")
            (Path(d) / "foco" / "layout.toml").write_text('[[coluna]]\npaineis = ["saida", "vault"]\n')
            self.assertNotEqual(a, cu.digest(Path(d), "foco"))

    def test_remember(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIsNone(cu.remembered(Path(d)))
            cu.remember(Path(d), "foco")
            self.assertEqual(cu.remembered(Path(d)), "foco")
            if not WINDOWS:
                self.assertEqual(stat.S_IMODE((Path(d) / "custom").stat().st_mode), 0o600)
            cu.remember(Path(d), None)
            self.assertIsNone(cu.remembered(Path(d)))
            with self.assertRaises(cu.CustomError):
                cu.remember(Path(d), "../x")

    def test_needs_trust_and_root(self):
        lay_ = L.parse({"coluna": [{"paineis": ["saida", "eco"]}], "painel": [
            {"id": "eco", "tipo": "comando", "argv": [ECHO, "oi"]}]}, "t", None)
        self.assertEqual(len(cu.needs_trust(lay_)), 1)
        self.assertEqual(cu.needs_trust(lay_)[0][1:], ("oi",))
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(cu.custom_root(Path(d)), plat.default_data_dir() / "custom")
            if not WINDOWS:
                self.assertEqual(cu.custom_root(Path(d)), Path("~/.local/share/hud/custom").expanduser())
            (Path(d) / "pyproject.toml").write_text("")
            self.assertEqual(cu.custom_root(Path(d)), Path(d) / "custom")


LAYOUT = '''nome = "foco"
descricao = "teste"
[[coluna]]
paineis = ["saida", "notas"]
[[painel]]
id = "notas"
tipo = "texto"
arquivo = "notas.md"
'''


class ProposalTest(unittest.TestCase):
    def test_parse_proposals(self):
        texto = (
            "Aqui está:\n```hud-custom nome=foco arquivo=layout.toml\n" + LAYOUT + "```\n"
            "```hud-custom nome=foco arquivo=notas.md\n# oi\n```\n"
            "```hud-custom nome=foco arquivo=../../.bashrc\nrm -rf ~\n```\n"
            "```hud-custom nome=../x arquivo=layout.toml\nx\n```\n"
            "```hud-custom nome=Foco arquivo=layout.toml\nx\n```\n"
            "```hud-custom nome=foco arquivo=evil.py\nimport os\n```\n"
            "```hud-custom nome=foco arquivo=sub/a.md\nx\n```\n"
            "```hud-custom nome=foco arquivo=.env.md\nx\n```\n"
            "```hud-custom nome=foco arquivo=sem-fim.md\nnunca fecha\n"
        )
        ps = cu.parse_proposals(texto)
        self.assertEqual([(p.nome, p.arquivo) for p in ps], [("foco", "layout.toml"), ("foco", "notas.md")])
        self.assertEqual(ps[0].conteudo, LAYOUT)
        self.assertEqual(ps[1].conteudo, "# oi\n")

    def test_save_proposals(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "custom"
            old = os.umask(0o077)
            try:
                ps = [cu.Proposal("foco", "notas.md", "# oi\n"), cu.Proposal("foco", "layout.toml", LAYOUT)]
                nome, files = cu.save_proposals(root, ps)
            finally:
                os.umask(old)
            self.assertEqual((nome, files), ("foco", ["notas.md", "layout.toml"]))
            if not WINDOWS:  # no Windows o modo não diz quem lê
                for f in files:
                    self.assertEqual(stat.S_IMODE((root / "foco" / f).stat().st_mode), 0o644)
                self.assertEqual(stat.S_IMODE((root / "foco").stat().st_mode), 0o755)
            self.assertEqual(cu.load_custom(root, "foco").paineis["notas"].arquivo, "notas.md")
            ps2 = [cu.Proposal("foco", "notas.md", "# novo\n")]
            with self.assertRaises(cu.AlreadyExists):
                cu.save_proposals(root, ps2)
            self.assertEqual((root / "foco" / "notas.md").read_text(), "# oi\n")
            cu.save_proposals(root, ps2, sobrescrever=True)
            self.assertEqual((root / "foco" / "notas.md").read_text(), "# novo\n")
            if not WINDOWS:
                self.assertEqual(stat.S_IMODE((root / "foco" / "notas.md").stat().st_mode), 0o644)

    def test_save_refuses(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "custom"
            bad_sets = [
                [],
                [cu.Proposal("foco", "layout.toml", LAYOUT), cu.Proposal("outro", "a.md", "x")],
                [cu.Proposal("../foco", "layout.toml", LAYOUT)],
                [cu.Proposal("foco", "../layout.toml", LAYOUT)],
                [cu.Proposal("foco", "a/b.md", "x"), cu.Proposal("foco", "layout.toml", LAYOUT)],
                [cu.Proposal("foco", "x.py", "x"), cu.Proposal("foco", "layout.toml", LAYOUT)],
                [cu.Proposal("foco", "layout.toml", "[[coluna]]\npaineis = [\"vault\"]\n")],  # sem saida
                [cu.Proposal("foco", "notas.md", "x")],  # pasta nova sem layout
            ]
            for ps in bad_sets:
                with self.assertRaises(L.LayoutError, msg=repr(ps)):
                    cu.save_proposals(root, ps)
            self.assertFalse((root / "foco").exists())
            root.mkdir()
            symlink(self, d, root / "link")
            with self.assertRaises(cu.CustomError):
                cu.save_proposals(root, [cu.Proposal("link", "layout.toml", LAYOUT)], sobrescrever=True)
            (root / "foco").mkdir()
            symlink(self, "/etc/passwd", root / "foco" / "notas.md")
            with self.assertRaises(cu.CustomError):
                cu.save_proposals(root, [cu.Proposal("foco", "notas.md", "x"),
                                         cu.Proposal("foco", "layout.toml", LAYOUT)], sobrescrever=True)


class ListTest(unittest.TestCase):
    def test_list_customs(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            self.assertEqual(cu.list_customs(root / "nada"), [])
            for n, body in (("b-ok", LAYOUT), ("a-ruim", "coluna = [")):
                (root / n).mkdir()
                (root / n / "layout.toml").write_text(body)
            (root / "vazia").mkdir()
            symlink(self, root / "b-ok", root / "c-link")
            (root / "solto.txt").write_text("x")
            got = cu.list_customs(root)
            self.assertEqual([g[0] for g in got], ["a-ruim", "b-ok", "c-link", "vazia"])
            self.assertIn("TOML", got[0][2])
            self.assertEqual(got[1][1:], ("teste", None))
            self.assertIsNotNone(got[2][2])
            self.assertIsNotNone(got[3][2])


class PanelFeedTest(unittest.TestCase):
    def make(self, d, painel, extra_files=()):
        base = Path(d) / "c"
        base.mkdir(exist_ok=True)
        for name, body in extra_files:
            (base / name).write_text(body)
        lay_ = L.parse({"coluna": [{"paineis": ["saida", painel["id"]]}], "painel": [painel]}, "c", base)
        return lay_, base

    def test_texto_rereads_on_change(self):
        with tempfile.TemporaryDirectory() as d:
            lay_, base = self.make(d, {"id": "n", "tipo": "texto", "arquivo": "n.md"},
                                   [("n.md", "linha 1\n\x1b]0;x\x07limpa\n")])
            f = cu.PanelFeed(lay_, base, trusted=False, tick=0.05)
            f.start()
            try:
                self.assertTrue(wait_for(lambda: f.lines("n") == ["linha 1", "limpa"]))
                self.assertTrue(f.status("n").startswith("há "))
                (base / "n.md").write_text("nova\n")
                os.utime(base / "n.md", (time.time() + 5, time.time() + 5))
                self.assertTrue(wait_for(lambda: f.lines("n") == ["nova"]))
                # No Windows apagar falha enquanto o feed está com o arquivo aberto: tenta de novo.
                self.assertTrue(wait_for(lambda: _try_unlink(base / "n.md")))
                self.assertTrue(wait_for(lambda: f.status("n").startswith("erro:")))
            finally:
                f.stop()

    def test_arquivo_tail_and_denied(self):
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "app.log"
            log.write_text("".join(f"l{i}\n" for i in range(500)))
            lay_, base = self.make(d, {"id": "log", "tipo": "arquivo", "caminho": str(log), "linhas": 3})
            f = cu.PanelFeed(lay_, base, trusted=False, tick=0.05)
            f.start()
            try:
                self.assertTrue(wait_for(lambda: f.lines("log") == ["l497", "l498", "l499"]))
            finally:
                f.stop()
            # O alvo vira segredo depois do parse (link trocado): o feed recusa e não mostra nada.
            os.makedirs(f"{d}/home/.ssh")
            (Path(d) / "home/.ssh/id_x").write_text("SEGREDO\n")
            log.unlink()
            symlink(self, f"{d}/home/.ssh/id_x", log)
            with mock.patch.dict(os.environ, home_env(f"{d}/home")):
                f = cu.PanelFeed(lay_, base, trusted=True, tick=0.05)
                f.start()
                try:
                    self.assertTrue(wait_for(lambda: f.status("log") == "erro: caminho negado"))
                    self.assertEqual(f.lines("log"), [])
                finally:
                    f.stop()

    def test_arquivo_big_reads_only_tail(self):
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "big.log"
            with open(log, "w") as fh:
                fh.write("x" * 1_000_000 + "\n")
                fh.write("fim\n")
            lay_, base = self.make(d, {"id": "log", "tipo": "arquivo", "caminho": str(log)})
            f = cu.PanelFeed(lay_, base, trusted=False, tick=0.05)
            f.start()
            try:
                self.assertTrue(wait_for(lambda: f.lines("log") == ["fim"]))
            finally:
                f.stop()

    def test_comando_trusted_and_untrusted(self):
        with tempfile.TemporaryDirectory() as d:
            argv = ECHO_ARGV if WINDOWS else ["echo", "oi\x1b[31m"]
            lay_, base = self.make(d, {"id": "eco", "tipo": "comando", "argv": argv,
                                       "intervalo": 5, "timeout": 2})
            f = cu.PanelFeed(lay_, base, trusted=False, tick=0.05)
            f.start()
            try:
                time.sleep(0.3)
                self.assertEqual(f.lines("eco"), ["comando não confiado: /custom c para revisar"])
                self.assertEqual(f.status("eco"), "não confiado")
            finally:
                f.stop()
            f = cu.PanelFeed(lay_, base, trusted=True, tick=0.05)
            f.start()
            try:
                if WINDOWS:
                    self.assertTrue(wait_for(lambda: bool(f.lines("eco"))))
                else:
                    self.assertTrue(wait_for(lambda: f.lines("eco") == ["oi"]))
                self.assertTrue(f.status("eco").startswith("há "))
            finally:
                f.stop()

    def test_comando_timeout(self):
        with tempfile.TemporaryDirectory() as d:
            lay_, base = self.make(d, {"id": "s", "tipo": "comando", "argv": SLEEP,
                                       "timeout": 1})
            f = cu.PanelFeed(lay_, base, trusted=True, tick=0.05)
            f.start()
            try:
                self.assertTrue(wait_for(lambda: "tempo esgotado" in f.status("s"), 5))
            finally:
                f.stop()


if __name__ == "__main__":
    unittest.main()
