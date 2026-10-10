"""O roteiro de validação da tela no Windows não pode ficar desatualizado em silêncio.

`docs/roteiro-windows.md` é executado à mão num Windows; aqui só se confere a
consistência: que ele cita cada tecla, aba e comando / que o HUD oferece hoje
(lidos de `hud/ui.py` em tempo de execução), que os passos estão bem formados
e que o condutor `scripts/roteiro-windows.ps1` é ASCII puro e bate com o roteiro.
"""

import json
import re
import unittest
from pathlib import Path

try:
    from hud import ui
except ImportError as e:  # sem curses (Windows sem windows-curses)
    raise unittest.SkipTest(f"hud.ui não importa: {e}")

ROOT = Path(__file__).resolve().parent.parent
ROTEIRO = ROOT / "docs" / "roteiro-windows.md"
CONDUTOR = ROOT / "scripts" / "roteiro-windows.ps1"

# Comandos / que a ajuda cita mas são do Claude Code, não do HUD.
EXTERNOS = {"/review"}
SETAS = {"up": "↑", "down": "↓", "left": "←", "right": "→"}
STEP = re.compile(r"^### (P(\d+)) · (.+)$", re.M)
FIELDS = ("Sessão", "Faça", "Confira", "Resultado")


def unescape_ps(s: str) -> str:
    """O que o `[regex]::Unescape` do condutor faz com \\uXXXX (pares substitutos inclusive)."""
    return json.loads('"' + s.replace('"', '\\"') + '"')


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

    def test_tamanho_minimo(self):
        self.assertTrue(f"O HUD precisa de {ui.MIN_W}×{ui.MIN_H}." in self.md, "tamanho mínimo desatualizado")


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


class Condutor(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw = CONDUTOR.read_bytes()
        cls.ps = cls.raw.decode("ascii")
        cls.md = ROTEIRO.read_text(encoding="utf-8")

    def test_so_ascii_sem_bom(self):
        # O Windows PowerShell 5.1 lê .ps1 sem BOM como ANSI.
        self.assertFalse(self.raw.startswith(b"\xef\xbb\xbf"))
        self.assertTrue(all(b < 0x80 for b in self.raw))

    def test_le_o_roteiro(self):
        self.assertTrue("'roteiro-windows.md'" in self.ps)

    def test_modos_iguais_aos_do_roteiro(self):
        m = re.search(r"^\$Modos = @\(([^)]*)\)", self.ps, re.M)
        self.assertIsNotNone(m)
        modos = set(re.findall(r"'(\w+)'", m.group(1)))
        usados = set(re.findall(r"abre o HUD \((\w+)\)", self.md))
        self.assertEqual(modos, usados)

    def test_linha_de_teste_no_roteiro(self):
        m = re.search(r"^\$LinhaTeste = U '([^']*)'", self.ps, re.M)
        self.assertIsNotNone(m)
        linha = unescape_ps(m.group(1))
        self.assertIn("🚀", linha)
        self.assertTrue(f"`{linha}`" in self.md, "a linha de teste do condutor não está no roteiro")

    def test_comandos_do_painel_no_roteiro(self):
        nomes = [unescape_ps(n) for n in re.findall(r'^name = "([^"]*)"$', self.ps, re.M)]
        self.assertEqual(len(nomes), 10, "um comando de teste para cada F1–F10")
        for i, nome in enumerate(nomes, 1):
            with self.subTest(tecla=f"F{i}"):
                self.assertTrue(f"| F{i} | `{nome}` |" in self.md, f"F{i} `{nome}` fora da tabela do roteiro")

    def test_nao_eleva_nem_muda_o_sistema(self):
        # Nada de política de execução, elevação, registro, variável de usuário/máquina ou download.
        for proibido in (r"Set-ExecutionPolicy", r"-Verb\s+RunAs", r"Set-ItemProperty", r"New-ItemProperty",
                         r"SetEnvironmentVariable\([^)]*,\s*'?(User|Machine)", r"\bsetx\b",
                         r"Invoke-WebRequest", r"Invoke-RestMethod", r"\birm\b", r"\biwr\b", r"Install-"):
            with self.subTest(proibido=proibido):
                self.assertIsNone(re.search(proibido, self.ps, re.I))

if __name__ == "__main__":
    unittest.main()
