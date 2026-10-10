"""O roteiro de validação da tela no COSMIC Terminal não pode ficar desatualizado em silêncio.

`docs/roteiro-cosmic.md` é executado à mão num desktop COSMIC; aqui só se
confere a consistência: que ele cita cada tecla, aba e comando / que o HUD
oferece hoje (lidos de `hud/ui.py` em tempo de execução), que os passos estão
bem formados, que o condutor `scripts/roteiro-cosmic.sh` passa no `bash -n` e
que o `--ensaio` roda até o fim numa pasta temporária, gera o relatório e não
escreve no HOME (que no teste também é uma pasta temporária).
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

try:
    from hud import ui
except ImportError as e:  # sem curses (Windows sem windows-curses)
    raise unittest.SkipTest(f"hud.ui não importa: {e}")

ROOT = Path(__file__).resolve().parent.parent
ROTEIRO = ROOT / "docs" / "roteiro-cosmic.md"
CONDUTOR = ROOT / "scripts" / "roteiro-cosmic.sh"
BASH = shutil.which("bash")

# Comandos / que a ajuda cita mas são do Claude Code, não do HUD.
EXTERNOS = {"/review"}
SETAS = {"up": "↑", "down": "↓", "left": "←", "right": "→"}
STEP = re.compile(r"^### (P(\d+)) · (.+)$", re.M)
FIELDS = ("Sessão", "Faça", "Confira", "Resultado")


class RoteiroCitaOHud(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.md = ROTEIRO.read_text(encoding="utf-8")

    def cita(self, token: str) -> bool:
        return re.search(r"(?<![\w/])" + re.escape(token) + r"(?![\w])", self.md) is not None

    def test_cada_comando_slash(self):
        faltam = [c for c, _ in ui.SLASH if not self.cita(c)]
        self.assertEqual(faltam, [], "comandos / do HUD (SLASH) que o roteiro não cita")

    def test_comandos_da_ajuda(self):
        citados = set(re.findall(r"(?<![\w/])/[a-z]+", ui.HELP)) - EXTERNOS
        faltam = sorted(c for c in citados if not self.cita(c))
        self.assertEqual(faltam, [], "comandos / da ajuda (HELP) que o roteiro não cita")

    def test_teclas_da_ajuda(self):
        teclas = set(re.findall(r"(?:Alt|Ctrl)\+(?:setas|[A-Za-z0-9])|F\d+–F\d+|PgUp|PgDn|\bTab\b|\bEsc\b"
                                r"|\bEnter\b|↑/↓", ui.HELP))
        self.assertIn("Alt+Z", teclas)  # o padrão ainda acha as teclas da ajuda
        faltam = sorted(t for t in teclas if t not in self.md)
        self.assertEqual(faltam, [], "teclas da ajuda (HELP) que o roteiro não cita")

    def test_cada_tecla_de_funcao(self):
        m = re.search(r"F(\d+)–F(\d+)", ui.HELP)
        self.assertIsNotNone(m, "a ajuda não cita mais F1–F10")
        faltam = [f"F{n}" for n in range(int(m.group(1)), int(m.group(2)) + 1) if not self.cita(f"F{n}")]
        self.assertEqual(faltam, [])
        self.assertTrue(self.cita("F11"), "F11 (tela cheia do cosmic-term) fora do roteiro")

    def test_alt_numeros_todos(self):
        # Alt+1 a Alt+0: os do HUD (modos, Alt+5) e os livres, que não podem fazer nada.
        faltam = [f"Alt+{n}" for n in "1234567890" if not self.cita(f"Alt+{n}")]
        self.assertEqual(faltam, [])

    def test_modos_e_abas(self):
        for n, modo in ui.MODE_KEYS.items():
            with self.subTest(modo=modo):
                for texto in (f"Alt+{n}", f"{n} {modo.upper()}",  # a aba na borda da SAÍDA
                              f"{modo.upper()} · LEITURA" if modo != "notas" else "ENTRADA"):
                    self.assertTrue(texto in self.md, f"o roteiro não cita {texto}")

    def test_alt_setas(self):
        for direcao in set(ui.FOCUS_KEYS.values()):
            with self.subTest(direcao=direcao):
                self.assertTrue(f"Alt+{SETAS[direcao]}" in self.md, f"o roteiro não cita Alt+{SETAS[direcao]}")

    def test_acoes_alt_do_pdcurses(self):
        # ALT_ACTIONS só existe no PDCurses, mas cada ação dele tem um passo aqui.
        for tecla in ("Alt+5", "Alt+Z", "/agentes"):
            self.assertTrue(self.cita(tecla), tecla)

    def test_tamanho_minimo(self):
        self.assertTrue(f"O HUD precisa de {ui.MIN_W}×{ui.MIN_H}." in self.md, "tamanho mínimo desatualizado")

    def test_mouse_e_cores(self):
        for texto in ("xterm-256color", "roda", "arraste", "clique", "256", "Shift+arrastar"):
            with self.subTest(texto=texto):
                self.assertIn(texto, self.md)

    def test_alternativa_para_tecla_que_nao_chega(self):
        self.assertIn("## Se uma tecla não chegar", self.md)
        self.assertIn("`cat -v`", self.md)


class RoteiroBemFormado(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.md = ROTEIRO.read_text(encoding="utf-8")
        cls.steps = list(STEP.finditer(cls.md))

    def bodies(self):
        for i, m in enumerate(self.steps):
            end = self.steps[i + 1].start() if i + 1 < len(self.steps) else self.md.find("\n## ", m.end())
            yield m.group(1), self.md[m.end(): end if end > 0 else len(self.md)]

    def test_numeracao_seguida(self):
        nums = [int(m.group(2)) for m in self.steps]
        self.assertGreaterEqual(len(nums), 30)
        self.assertEqual(nums, list(range(1, len(nums) + 1)))
        self.assertTrue(all(len(m.group(2)) == 2 for m in self.steps), "Pnn com dois dígitos")

    def test_cada_passo_tem_os_campos(self):
        for pid, body in self.bodies():
            with self.subTest(passo=pid):
                for campo in FIELDS:
                    achados = re.findall(rf"^- \*\*{campo}:\*\* ?(.*)$", body, re.M)
                    self.assertEqual(len(achados), 1, f"{pid}: campo {campo}")
                    if campo != "Resultado":
                        self.assertTrue(achados[0].strip(), f"{pid}: {campo} vazio")
                self.assertTrue("☐ ok · ☐ falhou · observação:" in body, f"{pid}: campo de resultado")

    def test_sessoes_em_ordem(self):
        """'HUD aberto (x)' só vem depois de 'abre o HUD (x)' ou de outro 'HUD aberto (x)'."""
        aberto = None
        for pid, body in self.bodies():
            sessao = re.search(r"^- \*\*Sessão:\*\* (.*)$", body, re.M).group(1).strip()
            with self.subTest(passo=pid, sessao=sessao):
                m = re.fullmatch(r"(abre o HUD|HUD aberto) \((base|abas)\)|fora do HUD", sessao)
                self.assertIsNotNone(m, "sessão desconhecida")
                if sessao == "fora do HUD":
                    aberto = None
                elif m.group(1) == "abre o HUD":
                    aberto = m.group(2)
                else:
                    self.assertEqual(aberto, m.group(2), "HUD aberto sem abrir antes")
            if re.search(r"`/sair`|Ctrl\+C", re.search(r"^- \*\*Faça:\*\* (.*)$", body, re.M).group(1)):
                aberto = None  # o passo fecha o HUD

    def test_passos_citados_existem(self):
        ids = {m.group(1) for m in self.steps}
        citados = set(re.findall(r"\bP\d{2}\b", self.md))
        self.assertEqual(sorted(citados - ids), [])

    def test_sem_dados_pessoais(self):
        # Repositório público: nada de caminho de casa real nem e-mail.
        self.assertIsNone(re.search(r"(?<![>\w])/home/\w|/Users/\w|[\w.+-]+@[\w-]+\.\w", self.md))


class Condutor(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sh = CONDUTOR.read_text(encoding="utf-8")
        cls.md = ROTEIRO.read_text(encoding="utf-8")

    def test_executavel_com_bash(self):
        self.assertTrue(self.sh.startswith("#!/usr/bin/env bash\n"))
        if os.name == "posix":
            self.assertTrue(os.access(CONDUTOR, os.X_OK), "scripts/roteiro-cosmic.sh sem permissão de execução")

    @unittest.skipUnless(BASH, "sem bash")
    def test_bash_n(self):
        r = subprocess.run([BASH, "-n", str(CONDUTOR)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_le_o_roteiro(self):
        self.assertIn('docs/roteiro-cosmic.md"', self.sh)

    def test_modos_iguais_aos_do_roteiro(self):
        m = re.search(r"^MODOS=\(([^)]*)\)", self.sh, re.M)
        self.assertIsNotNone(m)
        usados = set(re.findall(r"abre o HUD \((\w+)\)", self.md))
        self.assertEqual(set(m.group(1).split()), usados)

    def test_linha_de_teste_no_roteiro(self):
        m = re.search(r"^LINHA_TESTE='([^']*)'", self.sh, re.M)
        self.assertIsNotNone(m)
        self.assertIn("🚀", m.group(1))
        self.assertTrue(f"`{m.group(1)}`" in self.md, "a linha de teste do condutor não está no roteiro")

    def test_comandos_do_painel_no_roteiro(self):
        nomes = re.findall(r'^name = "([^"]*)"$', self.sh, re.M)
        self.assertEqual(len(nomes), 10, "um comando de teste para cada F1–F10")
        for i, nome in enumerate(nomes, 1):
            with self.subTest(tecla=f"F{i}"):
                self.assertTrue(f"| F{i} | `{nome}` |" in self.md, f"F{i} `{nome}` fora da tabela do roteiro")

    def test_nao_instala_nem_muda_o_sistema(self):
        for proibido in (r"\bsudo\b", r"\bapt(-get)?\b", r"\bflatpak\b", r"\bpip3? install", r"\bcurl\b",
                         r"\bwget\b", r"\bgsettings\b", r"\bdconf\b", r"cosmic-settings",
                         r"\.config/cosmic", r"\bchsh\b", r">>?\s*~/"):
            with self.subTest(proibido=proibido):
                self.assertIsNone(re.search(proibido, self.sh))

    def test_sem_dados_pessoais(self):
        self.assertIsNone(re.search(r"(?<![>\w])/home/\w|/Users/\w|[\w.+-]+@[\w-]+\.\w", self.sh))


@unittest.skipUnless(BASH and sys.platform.startswith("linux"), "o ensaio usa bash e coreutils GNU (Linux)")
class Ensaio(unittest.TestCase):
    """O --ensaio de verdade: pasta e HOME temporários, sem perguntar nem abrir janela."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = Path(tempfile.mkdtemp(prefix="hud-roteiro-cosmic-"))
        cls.home = cls.tmp / "casa"
        cls.home.mkdir()
        cls.pasta = cls.tmp / "pasta"
        env = {k: v for k, v in os.environ.items() if not k.startswith(("XDG_", "CLAUDE", "CODEX"))}
        # PATH sem cosmic-term nem wl-copy: o ensaio não pode depender deles.
        env.update(HOME=str(cls.home), PATH="/usr/bin:/bin", LANG="C.UTF-8", LC_ALL="C.UTF-8")
        cls.r = subprocess.run(
            [BASH, str(CONDUTOR), "--ensaio", "--hud", str(ROOT / "bin" / "hud"), "--pasta", str(cls.pasta)],
            capture_output=True, text=True, env=env, stdin=subprocess.DEVNULL, timeout=120)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def relatorio(self) -> str:
        achados = sorted(self.pasta.glob("roteiro-cosmic-*.md"))
        self.assertEqual(len(achados), 1, self.r.stdout + self.r.stderr)
        return achados[0].read_text(encoding="utf-8")

    def test_roda_ate_o_fim(self):
        self.assertEqual(self.r.returncode, 0, self.r.stdout + self.r.stderr)
        self.assertIn("relatório: ", self.r.stdout)

    def test_relatorio_com_todos_os_passos(self):
        texto = self.relatorio()
        n = len(STEP.findall(ROTEIRO.read_text(encoding="utf-8")))
        self.assertIn(f"{n} de {n} passos registrados", texto)
        self.assertIn("**Ensaio:**", texto)
        for item in ("hud --version", "cosmic-term --version", "TERM (janela do condutor)", "Tamanho"):
            self.assertIn(f"| {item}", texto)
        self.assertRegex(texto, r"\| hud --version \| hud \d+\.\d+")
        self.assertIn("pastas reais do HUD sem mudança", texto)

    def test_check_da_config_de_teste(self):
        m = re.search(r"^== hud --check .*?^\(saiu com (\d+)\)$", self.r.stdout, re.M | re.S)
        self.assertIsNotNone(m, self.r.stdout)
        self.assertEqual(m.group(1), "0")
        self.assertIn("[0] Falha de propósito", m.group(0))
        self.assertNotIn("aviso:", m.group(0))

    def test_dados_sinteticos_e_lancadores(self):
        casa = self.pasta / "home"
        for rel in (".config/hud/config.toml", ".config/hud/config-abas.toml", ".local/share/hud/agenda.md",
                    "Vault/.obsidian", "Vault/Projetos/Roteiro/05-backlog/Kanban (Roteiro).md",
                    "Outra pasta/leia-me.md", "texto-longo.txt", "custom"):
            self.assertTrue((casa / rel).exists(), rel)
        for m in ("base", "abas"):
            lanc = (self.pasta / f"abrir-hud-{m}.sh").read_text(encoding="utf-8")
            self.assertIn(f"HOME='{casa}'", lanc)
            self.assertIn(f"config{'-abas' if m == 'abas' else ''}.toml'", lanc)
            r = subprocess.run([BASH, "-n", str(self.pasta / f"abrir-hud-{m}.sh")], capture_output=True)
            self.assertEqual(r.returncode, 0)
        self.assertEqual(os.stat(self.pasta).st_mode & 0o077, 0, "pasta de teste visível a outros")

    def test_nao_toca_no_home(self):
        self.assertEqual(list(self.home.iterdir()), [], "o ensaio escreveu no HOME")

    def test_recusa_pasta_dentro_da_config_real(self):
        env = dict(os.environ, HOME=str(self.home), PATH="/usr/bin:/bin")
        for alvo in (self.home, self.home / ".config" / "hud" / "x", self.home / "Vault" / "teste"):
            with self.subTest(alvo=str(alvo)):
                r = subprocess.run([BASH, str(CONDUTOR), "--ensaio", "--hud", str(ROOT / "bin" / "hud"),
                                    "--pasta", str(alvo)], capture_output=True, text=True, env=env,
                                   stdin=subprocess.DEVNULL, timeout=60)
                self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertEqual(list(self.home.iterdir()), [])

    def test_recusa_pasta_alheia(self):
        alheia = self.tmp / "alheia"
        alheia.mkdir()
        (alheia / "meu.txt").write_text("não apague")
        env = dict(os.environ, HOME=str(self.home), PATH="/usr/bin:/bin")
        r = subprocess.run([BASH, str(CONDUTOR), "--ensaio", "--hud", str(ROOT / "bin" / "hud"),
                            "--pasta", str(alheia)], capture_output=True, text=True, env=env,
                           stdin=subprocess.DEVNULL, timeout=60)
        self.assertEqual(r.returncode, 2)
        self.assertIn("não foi criada por este script", r.stderr)
        self.assertEqual([p.name for p in alheia.iterdir()], ["meu.txt"])


if __name__ == "__main__":
    unittest.main()
