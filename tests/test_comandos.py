"""comandos.toml: carga, propostas ```hud-comandos, diff e gravação."""

import os
import stat
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from hud import comandos, config
from tests.suporte import ECHO, ECHO_ARGV, WINDOWS, home_env, posix_only, symlink

CMD_A = f'[[command]]\nname = "A"\nargv = {list(ECHO_ARGV)!r}\n'.replace("'", '"')
CMD_B = f'[[command]]\nname = "B"\nargv = ["{ECHO}"]\ntimeout = 5\n'
CMD_C = f'[[command]]\nname = "C"\nargv = ["{ECHO}"]\nconfirm = true\n'


def write_private(p: Path, text: str) -> None:
    p.write_text(text, encoding="utf-8")
    if not WINDOWS:
        os.chmod(p, 0o600)


class Base(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(os.path.realpath(tmp.name))
        if not WINDOWS:
            os.chmod(self.dir, 0o700)
        # No Windows o arquivo privado precisa ficar "no perfil": o perfil é a pasta do teste.
        env = {**home_env(str(self.dir)), "APPDATA": str(self.dir), "LOCALAPPDATA": str(self.dir)}
        patcher = mock.patch.dict(os.environ, env)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.cfg_path = self.dir / "config.toml"
        self.cmd_path = self.dir / "comandos.toml"


class CargaTest(Base):
    def test_sem_nada_vale_o_padrao(self):
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "padrão")
        self.assertEqual(cfg.commands_file, self.cmd_path)
        self.assertGreaterEqual(len(cfg.commands), 3, cfg.warnings)

    def test_config_toml_sem_comandos_toml(self):
        write_private(self.cfg_path, CMD_A)
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "config.toml")
        self.assertEqual([c.name for c in cfg.commands], ["A"])

    def test_comandos_toml_substitui(self):
        write_private(self.cfg_path, CMD_A)
        write_private(self.cmd_path, CMD_B + CMD_C)
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "comandos.toml")
        self.assertEqual([(c.key, c.name) for c in cfg.commands], [("1", "B"), ("2", "C")])
        self.assertTrue(cfg.commands[1].confirm)

    def test_comando_recusado_vira_aviso(self):
        write_private(self.cmd_path, CMD_A + '[[command]]\nname = "x"\nargv = ["sudo", "id"]\n')
        cfg = config.load(self.cfg_path)
        self.assertEqual([c.name for c in cfg.commands], ["A"])
        self.assertTrue(any("sudo" in w for w in cfg.warnings), cfg.warnings)

    def test_toml_quebrado_cai_para_config(self):
        write_private(self.cfg_path, CMD_A)
        write_private(self.cmd_path, "[[command]\nname = ")
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "config.toml")
        self.assertTrue(any("comandos.toml ignorado" in w for w in cfg.warnings), cfg.warnings)

    @posix_only
    def test_aberto_a_outros_recusado(self):
        write_private(self.cmd_path, CMD_B)
        os.chmod(self.cmd_path, 0o666)
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "padrão")
        self.assertTrue(any("escrito por outros" in w for w in cfg.warnings), cfg.warnings)

    def test_fora_do_perfil_recusado_no_windows(self):
        if not WINDOWS:
            self.skipTest("só no Windows")
        write_private(self.cmd_path, CMD_B)
        with tempfile.TemporaryDirectory() as outro, mock.patch.dict(
                os.environ, {k: outro for k in ("USERPROFILE", "APPDATA", "LOCALAPPDATA")}):
            cfg = config.load(self.cfg_path)
        self.assertNotEqual(cfg.commands_source, "comandos.toml")

    def test_link_recusado(self):
        real = self.dir / "real.toml"
        write_private(real, CMD_B)
        symlink(self, real, self.cmd_path)
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "padrão")
        self.assertTrue(any("link" in w for w in cfg.warnings), cfg.warnings)

    def test_chave_desconhecida_recusada(self):
        write_private(self.cmd_path, '[[commands]]\nname = "x"\nargv = ["id"]\n')
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "padrão")
        self.assertTrue(any("chave desconhecida" in w for w in cfg.warnings), cfg.warnings)


def bloco(corpo: str, cerca: str = "```", fecha: str | None = None) -> str:
    return f"Proposta:\n\n{cerca}hud-comandos\n{corpo}{fecha or cerca}\n\nPronto."


class PropostaTest(Base):
    def test_valida(self):
        p = comandos.parse_proposals(bloco(CMD_A + CMD_B))
        self.assertIsNotNone(p)
        self.assertEqual(p.avisos, [])
        self.assertEqual([c.name for c in p.comandos], ["A", "B"])
        self.assertTrue(os.path.isabs(p.comandos[0].argv[0]))
        self.assertEqual(p.conteudo, CMD_A + CMD_B)

    def test_sem_bloco(self):
        self.assertIsNone(comandos.parse_proposals("nada aqui\n```toml\nx = 1\n```\n"))
        self.assertIsNone(comandos.parse_proposals("```hud-comandos\n" + CMD_A))  # sem fechar

    def test_cerca_de_tis_e_mais_longa(self):
        texto = "~~~~hud-comandos\n" + CMD_A + "```\nainda dentro\n" + "~~~~~\n"
        p = comandos.parse_proposals(texto)
        self.assertIn("ainda dentro", p.conteudo)  # ``` não fecha cerca de ~~~~

    def test_vale_a_ultima(self):
        p = comandos.parse_proposals(bloco(CMD_A) + bloco(CMD_B))
        self.assertEqual([c.name for c in p.comandos], ["B"])

    def test_invalidas(self):
        casos = {
            "shell": '[[command]]\nname = "x"\nargv = ["sh", "-c", "id"]\n',
            "proibido": '[[command]]\nname = "x"\nargv = ["sudo", "id"]\n',
            "argv vazio": '[[command]]\nname = "x"\nargv = []\n',
            "toml": "[[command]\nname = \n",
            "sem nome": '[[command]]\nargv = ["id"]\n',
            "timeout": f'[[command]]\nname = "x"\nargv = ["{ECHO}"]\ntimeout = 9999\n',
        }
        for caso, corpo in casos.items():
            with self.subTest(caso=caso):
                p = comandos.parse_proposals(bloco(corpo))
                self.assertIsNotNone(p)
                self.assertEqual(p.comandos, [])
                self.assertTrue(p.avisos)
                with self.assertRaises(comandos.CommandError):
                    comandos.salvar(self.cmd_path, corpo)
                self.assertFalse(self.cmd_path.exists())

    def test_muitos_comandos(self):
        corpo = "".join(f'[[command]]\nname = "c{i}"\nargv = ["{ECHO}"]\n' for i in range(11))
        p = comandos.parse_proposals(bloco(corpo))
        self.assertEqual(len(p.comandos), 10)
        self.assertTrue(p.avisos)

    def test_grande_demais(self):
        corpo = CMD_A + "# " + "x" * (64 * 1024) + "\n"
        self.assertIsNone(comandos.parse_proposals(bloco(corpo)))
        # A grande é descartada; a anterior, se houver, vale.
        p = comandos.parse_proposals(bloco(CMD_B) + bloco(corpo))
        self.assertEqual([c.name for c in p.comandos], ["B"])
        with self.assertRaises(comandos.CommandError):
            comandos.salvar(self.cmd_path, corpo)


class DiffTest(Base):
    def cmds(self, texto):
        cs, avisos = comandos.validar(texto)
        self.assertEqual(avisos, [])
        return cs

    def test_diff(self):
        atuais = self.cmds(CMD_A + CMD_B + CMD_C)
        novos = self.cmds(CMD_B.replace("timeout = 5", "timeout = 9") + CMD_A
                          + f'[[command]]\nname = "D"\nargv = ["{ECHO}"]\n')
        d = comandos.diff(atuais, novos)
        self.assertEqual([m for m, _ in d], ["~", "~", "+", "-"])
        self.assertTrue(d[0][1].startswith("F1 B · "), d)
        self.assertIn("era F2", d[0][1])
        self.assertIn("tempo-limite", d[0][1])
        self.assertIn("era F1", d[1][1])
        self.assertTrue(d[2][1].startswith("F3 D · "))
        self.assertTrue(d[3][1].startswith("F3 C · "))

    def test_igual(self):
        atuais = self.cmds(CMD_A + CMD_B)
        d = comandos.diff(atuais, self.cmds(CMD_A + CMD_B))
        self.assertEqual([m for m, _ in d], ["=", "="])
        self.assertEqual(d[0][1], f"F1 A · {' '.join(ECHO_ARGV)}")


class SalvarTest(Base):
    def test_salva_e_carrega(self):
        comandos.salvar(self.cmd_path, CMD_A + CMD_C)
        self.assertEqual(self.cmd_path.read_text(encoding="utf-8"), CMD_A + CMD_C)
        if not WINDOWS:
            self.assertEqual(stat.S_IMODE(self.cmd_path.stat().st_mode), 0o600)
        self.assertFalse((self.dir / ".comandos.toml.tmp").exists())
        cfg = config.load(self.cfg_path)
        self.assertEqual(cfg.commands_source, "comandos.toml")
        self.assertEqual([c.name for c in cfg.commands], ["A", "C"])

    def test_substitui_atomico(self):
        comandos.salvar(self.cmd_path, CMD_A)
        comandos.salvar(self.cmd_path, CMD_B)
        self.assertEqual(self.cmd_path.read_text(encoding="utf-8"), CMD_B)
        with self.assertRaises(comandos.CommandError):
            comandos.salvar(self.cmd_path, '[[command]]\nname = "x"\nargv = ["bash"]\n')
        self.assertEqual(self.cmd_path.read_text(encoding="utf-8"), CMD_B)

    def test_cria_a_pasta(self):
        destino = self.dir / "nova" / "comandos.toml"
        comandos.salvar(destino, CMD_A)
        self.assertTrue(destino.is_file())

    def test_recusa_link(self):
        real = self.dir / "real.toml"
        write_private(real, CMD_B)
        symlink(self, real, self.cmd_path)
        with self.assertRaises(comandos.CommandError):
            comandos.salvar(self.cmd_path, CMD_A)
        self.assertEqual(real.read_text(encoding="utf-8"), CMD_B)

    def test_recusa_outro_nome(self):
        with self.assertRaises(comandos.CommandError):
            comandos.salvar(self.dir / "config.toml", CMD_A)

    def test_windows_fora_do_perfil(self):
        if not WINDOWS:
            self.skipTest("só no Windows")
        with tempfile.TemporaryDirectory() as outro:
            with self.assertRaises(comandos.CommandError):
                comandos.salvar(Path(outro) / "comandos.toml", CMD_A)


if __name__ == "__main__":
    unittest.main()
