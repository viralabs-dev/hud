"""Testes da tela: o HUD de verdade num pseudo-terminal, lido por um emulador VT.

Tudo é isolado: configuração, Vault, pasta de dados, HOME, TMPDIR e
CLAUDE_CONFIG_DIR ficam numa pasta temporária, e o Claude e o Codex são
scripts falsos que imprimem JSONL no formato real e gravam o argv recebido.
Nada usa o Vault, ~/.local/share/hud, ~/.config/hud nem a rede.
"""

import fcntl
import json
import os
import pty
import select
import shutil
import signal
import stat
import struct
import tempfile
import termios
import time
import unittest
from pathlib import Path

from tests.vt import Screen

ROOT = Path(__file__).resolve().parent.parent
# HUD_BIN (opcional): roda os testes contra outro executável, como o binário
# autocontido (build/pyinstaller/dist/hud, ver scripts/release.sh). Sem ela, usa bin/hud.
HUD_BIN = Path(os.environ.get("HUD_BIN") or ROOT / "bin" / "hud").resolve()
ROWS, COLS = 40, 120
WAIT = 10.0

F2 = b"\x1bOQ"
ALT = {n: b"\x1b" + str(n).encode() for n in (1, 2, 3)}
WHEEL_UP = b"\x1b[<64;10;10M"
WHEEL_DOWN = b"\x1b[<65;10;10M"
ORANGE, GRAY = 208, 248

KANBAN = """---
kanban-plugin: board
---

## A fazer

- [ ] Card a fazer

## Em andamento

- [ ] Card andando no teste

## Concluído

- [x] Card feito

%% kanban:settings
```
{"kanban-plugin":"board"}
```
%%
"""

FAKE_CLAUDE = """#!/usr/bin/env python3
import json, sys
data = sys.stdin.read()
with open(LOG, "a", encoding="utf-8") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "stdin": data}) + "\\n")
for ev in (
    {"type": "system", "subtype": "init", "session_id": "s1", "model": "m",
     "slash_commands": ["review", "context"]},
    {"type": "assistant", "message": {"content": [{"type": "text", "text": "resposta falsa"}]}},
    *([{"type": "assistant", "message": {"content": [{"type": "text", "text": PROPOSTA_JSON}]}}]
      if "PROPOSTA" in data else []),
    {"type": "result", "subtype": "success", "is_error": False, "session_id": "s1",
     "total_cost_usd": 0.01},
):
    print(json.dumps(ev), flush=True)
"""

# O que o agente responde quando segue a skill hud-custom dentro do HUD.
PROPOSTA_TEXTO = """Pronto:

```hud-custom nome=proposta arquivo=layout.toml
nome = "proposta"
descricao = "Proposta vinda do agente falso"
[[coluna]]
paineis = ["sistema", "uso_claude"]
[[coluna]]
paineis = ["saida"]
```

Digite /custom salvar e depois /custom proposta."""

LAYOUT_ECO = """nome = "eco"
descricao = "Painel de comando para testar a confiança"
[[coluna]]
paineis = ["sistema", "eco"]
[[coluna]]
paineis = ["saida", "vault"]
[[painel]]
id = "eco"
titulo = "ECO PROPRIO"
tipo = "comando"
argv = ["echo", "saida-do-painel-eco"]
intervalo = 5
"""

FAKE_CODEX = """#!/usr/bin/env python3
import json, sys
data = sys.stdin.read()
with open(LOG, "a", encoding="utf-8") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "stdin": data}) + "\\n")
for ev in (
    {"type": "thread.started", "thread_id": "t1"},
    {"type": "item.completed", "item": {"type": "agent_message", "text": "oi codex falso"}},
    {"type": "turn.completed", "usage": {"input_tokens": 10, "output_tokens": 5}},
):
    print(json.dumps(ev), flush=True)
"""


def _winsize(fd: int, rows: int, cols: int) -> None:
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


class HudProcess:
    """Um HUD rodando num pty, com a tela emulada."""

    def __init__(self, config: Path, env: dict, rows: int = ROWS, cols: int = COLS):
        self.screen = Screen(rows, cols)
        self.raw = bytearray()
        self.status: int | None = None
        self.eof = False
        pid, fd = pty.fork()
        if pid == 0:  # filho: tamanho antes do exec, para não haver corrida
            try:
                _winsize(0, rows, cols)
                os.execve(str(HUD_BIN), [str(HUD_BIN), "-c", str(config)], env)
            finally:
                os._exit(127)
        self.pid, self.fd = pid, fd

    # ── E/S ──────────────────────────────────────────────────────────────
    def pump(self, timeout: float = 0.05) -> None:
        if self.eof:
            time.sleep(timeout)
            return
        r, _, _ = select.select([self.fd], [], [], timeout)
        if not r:
            return
        try:
            data = os.read(self.fd, 65536)
        except OSError:  # EIO: o filho fechou o terminal
            data = b""
        if not data:
            self.eof = True
            return
        self.raw += data
        self.screen.feed(data)

    def send(self, data: bytes | str) -> None:
        os.write(self.fd, data.encode() if isinstance(data, str) else data)

    def type(self, line: str) -> None:
        self.send(line + "\r")

    def resize(self, rows: int, cols: int) -> None:
        self.screen.resize(rows, cols)
        _winsize(self.fd, rows, cols)
        try:
            os.kill(self.pid, signal.SIGWINCH)
        except ProcessLookupError:
            pass

    def wait_for(self, pred, timeout: float = WAIT, what: str = "") -> None:
        if isinstance(pred, str):
            text, what = pred, what or repr(pred)
            pred = lambda s: text in s.text()  # noqa: E731
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self.pump()
            if pred(self.screen):
                return
        raise AssertionError(f"esperei {what or 'condição'} por {timeout:.0f}s; tela:\n{self.screen.text()}")

    def wait_exit(self, timeout: float = WAIT) -> int | None:
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self.pump()
            pid, st = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.status = os.waitstatus_to_exitcode(st)
                return self.status
        return None

    def close(self) -> None:
        if self.status is None:
            try:
                self.send(b"\x03")
            except OSError:
                pass
            if self.wait_exit(3) is None:
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
        os.close(self.fd)


class UiTest(unittest.TestCase):
    """Cada teste abre um HUD novo com pasta de dados nova."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = Path(tempfile.mkdtemp(prefix="hud-ui-"))
        os.chmod(cls.tmp, 0o700)
        cls.home = cls.tmp / "home"
        cls.home.mkdir(mode=0o700)
        cls.vault = cls.tmp / "vault"
        (cls.vault / ".obsidian").mkdir(parents=True)
        (cls.vault / "Projeto").mkdir()
        (cls.vault / "Projeto" / "Kanban (Teste).md").write_text(KANBAN, encoding="utf-8")
        (cls.vault / "Nota um.md").write_text("# Nota um\n\ntexto da nota um\n", encoding="utf-8")
        (cls.vault / "Nota dois.md").write_text("- [ ] tarefa 📅 2030-01-02\n", encoding="utf-8")
        cls.other = cls.tmp / "outra-pasta"
        cls.other.mkdir()
        (cls.other / "leia.txt").write_text("arquivo de texto\n", encoding="utf-8")
        bindir = cls.tmp / "bin"
        bindir.mkdir(mode=0o700)
        cls.claude_log = cls.tmp / "claude.log"
        cls.codex_log = cls.tmp / "codex.log"
        for name, src, log in (("claude", FAKE_CLAUDE, cls.claude_log),
                               ("codex", FAKE_CODEX, cls.codex_log)):
            exe = bindir / name
            exe.write_text(src.replace("LOG", json.dumps(str(log)))
                           .replace("PROPOSTA_JSON", json.dumps(PROPOSTA_TEXTO)), encoding="utf-8")
            os.chmod(exe, 0o700)
        cls.claude_exe, cls.codex_exe = bindir / "claude", bindir / "codex"
        cls.env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": str(cls.home),
            "USER": os.environ.get("USER", "teste"),
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TERM": "xterm-256color",
            "TMPDIR": str(cls.tmp),
            "CLAUDE_CONFIG_DIR": str(cls.tmp / "claude-config"),
            "ESCDELAY": "25",
        }
        cls.custom = cls.tmp / "custom"
        (cls.custom / "eco").mkdir(parents=True)
        (cls.custom / "eco" / "layout.toml").write_text(LAYOUT_ECO, encoding="utf-8")
        shutil.copytree(ROOT / "custom" / "foco", cls.custom / "foco")
        cls.n = 0

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def setUp(self):
        UiTest.n += 1
        self.data = self.tmp / f"dados{self.n}"
        self.config = self.tmp / f"config{self.n}.toml"
        q = json.dumps
        self.config.write_text(f"""\
vault = {q(str(self.vault))}
data_dir = {q(str(self.data))}
refresh_seconds = 1
vault_scan_seconds = 2
custom_dir = {q(str(self.custom))}

[[command]]
name = "Eco"
argv = ["echo", "ola-do-eco"]

[[command]]
name = "Hostil"
argv = ["printf", '\\033]0;titulo\\007\\033[2Jlimpo-fim\\n']

[[command]]
name = "Perigoso"
argv = ["echo", "rodou-confirmado"]
confirm = true

[claude]
executable = {q(str(self.claude_exe))}

[codex]
executable = {q(str(self.codex_exe))}
""", encoding="utf-8")
        os.chmod(self.config, 0o600)
        for log in (self.claude_log, self.codex_log):
            log.unlink(missing_ok=True)
        self.hud = HudProcess(self.config, self.env)
        self.addCleanup(self.hud.close)
        self.hud.wait_for("ENTRADA")
        self.hud.wait_for("HUD pronto")

    # ── auxiliares ───────────────────────────────────────────────────────
    def input_title(self) -> tuple[int, str]:
        """Linha e texto do título da caixa de entrada (3 últimas linhas)."""
        s = self.hud.screen
        r = s.rows - 3
        return r, s.line(r)

    def wait_input(self, title: str, prompt: str, color=None) -> int:
        """Espera a caixa de entrada inteira: título, prompt e a cor da borda
        (em cima e embaixo). Tudo no mesmo predicado, porque um redesenho pode
        chegar em pedaços. Devolve a linha do título."""
        def ok(s) -> bool:
            r = s.rows - 3
            top, bottom = s.fg_at(r, 0), s.fg_at(r + 2, 0)
            colored = (top == bottom == color) if color is not None else (
                top not in (ORANGE, GRAY) and bottom not in (ORANGE, GRAY))
            return (title in s.line(r) and s.line(r)[0] == "╭" and s.line(r + 2)[0] == "╰"
                    and s.line(r + 1)[2] == prompt and colored)
        self.hud.wait_for(ok, what=f"entrada {title!r} {prompt} cor {color}")
        return self.hud.screen.rows - 3

    def calls(self, log: Path) -> list[dict]:
        if not log.exists():
            return []
        return [json.loads(x) for x in log.read_text(encoding="utf-8").splitlines()]

    def count(self, text: str) -> int:
        return self.hud.screen.text().count(text)

    # ── testes ───────────────────────────────────────────────────────────
    def test_01_paineis_com_titulo(self):
        for t in ("SISTEMA", "COMANDOS", "AGENDA", "USO CLAUDE", "VAULT · ao vivo", "SAÍDA", "ENTRADA"):
            self.hud.wait_for(t)
        self.hud.wait_for("Teste")  # o quadro Kanban de exemplo
        self.hud.wait_for("Card andando no teste")
        text = self.hud.screen.text()
        for cmd in ("Eco", "Hostil", "Perigoso"):
            self.assertIn(cmd, text)
        self.wait_input("ENTRADA", "›")
        self.assertIsNone(self.hud.screen.title)

    def test_02_texto_vira_nota_e_ajuda(self):
        self.hud.type("minha nota de teste")
        self.hud.wait_for("› minha nota de teste")
        notes = self.data / "notas.md"
        self.assertTrue(notes.exists())
        self.assertEqual(stat.S_IMODE(notes.stat().st_mode), 0o600)
        self.assertIn("minha nota de teste", notes.read_text(encoding="utf-8"))
        self.hud.type("/ajuda")
        self.hud.wait_for("Segurança")
        self.hud.wait_for("nunca escreve no Vault")

    def test_03_comandos_e_escape_hostil(self):
        self.hud.send(F2)
        self.hud.wait_for("✓ Hostil")
        self.hud.wait_for("limpo-fim")
        # "titulo" só aparece como texto inerte na linha "$ printf \033]0;titulo…",
        # nunca precedido de um ESC de verdade.
        for line in self.hud.screen.text().splitlines():
            if "titulo" in line:
                self.assertIn("$ printf \\033]0;titulo\\007", line)
        self.assertIn("│ limpo-fim ", self.hud.screen.text())
        self.assertIsNone(self.hud.screen.title)
        self.assertEqual(self.hud.screen.osc, [])
        raw = bytes(self.hud.raw)
        self.assertNotIn(b"\x1b]", raw)  # nenhum OSC chegou ao terminal
        self.assertNotIn(b"\x1b[2Jlimpo", raw)
        self.hud.type("/r 1")
        self.hud.wait_for("ola-do-eco")
        self.hud.wait_for("✓ Eco")

    def test_04_confirmacao(self):
        self.hud.type("/r 3")
        self.hud.wait_for(lambda s: "confirmar: Perigoso?" in self.input_title()[1], what="pedido de confirmação")
        self.hud.send("n")
        self.hud.wait_for("Perigoso: cancelado")
        self.assertNotIn("rodou-confirmado", self.hud.screen.text())
        self.wait_input("ENTRADA", "›")
        self.hud.type("/r 3")
        self.hud.wait_for(lambda s: "confirmar: Perigoso?" in self.input_title()[1], what="confirmação de novo")
        self.hud.send("s")
        self.hud.wait_for("rodou-confirmado")
        self.hud.wait_for("✓ Perigoso")

    def test_05_modos_claude_e_codex(self):
        s = self.hud.screen
        self.hud.send(ALT[2])
        r = self.wait_input("CLAUDE · LEITURA", "✦", ORANGE)
        self.assertEqual(s.fg_at(r + 1, 2), ORANGE)  # o prompt também

        self.hud.type("pergunta um")
        self.hud.wait_for("resposta falsa")
        self.hud.wait_for("✓ claude")
        self.hud.type("pergunta dois")
        self.hud.wait_for(lambda s: self.count("✓ claude") >= 2, what="segunda resposta")
        calls = self.calls(self.claude_log)
        self.assertEqual([c["stdin"] for c in calls], ["pergunta um", "pergunta dois"])
        for c in calls:
            self.assertNotIn("pergunta um", c["argv"])
            self.assertNotIn("pergunta dois", c["argv"])
            self.assertIn("--strict-mcp-config", c["argv"])
            self.assertIn("-p", c["argv"])
        self.assertNotIn("--resume", calls[0]["argv"])
        i = calls[1]["argv"].index("--resume")
        self.assertEqual(calls[1]["argv"][i + 1], "s1")

        self.hud.send(ALT[3])
        self.wait_input("CODEX · LEITURA", "✦", GRAY)
        self.hud.type("pergunta codex")
        self.hud.wait_for("oi codex falso")
        self.hud.wait_for("✓ codex")
        calls = self.calls(self.codex_log)
        # Primeira pergunta da conversa: o Codex não tem prompt de sistema, então o
        # contexto do HUD (skill hud-custom) vem antes do texto do usuário.
        self.assertTrue(calls[0]["stdin"].startswith("[Contexto do HUD:"))
        self.assertIn("hud-custom", calls[0]["stdin"])
        self.assertTrue(calls[0]["stdin"].endswith("\n\npergunta codex"))
        self.assertEqual(calls[0]["argv"][:1], ["exec"])
        self.assertEqual(calls[0]["argv"][-1], "-")
        self.assertNotIn("pergunta codex", calls[0]["argv"])

        self.hud.send(ALT[1])
        self.wait_input("ENTRADA", "›")

    def test_06_slash_desconhecido_vai_ao_claude(self):
        self.hud.send(ALT[2])
        self.wait_input("CLAUDE · LEITURA", "✦", ORANGE)
        self.hud.type("/review x")
        self.hud.wait_for("✓ claude")
        self.assertIn("você › /review x", self.hud.screen.text())
        self.assertNotIn("não conheço", self.hud.screen.text())
        self.assertEqual([c["stdin"] for c in self.calls(self.claude_log)], ["/review x"])

    def test_07_troca_de_pasta(self):
        self.hud.wait_for("VAULT · ao vivo")
        self.hud.type(f"/pasta {self.other}")
        self.hud.wait_for("PASTA · outra-pasta")
        self.assertEqual((self.data / "pasta").read_text(encoding="utf-8").strip(), str(self.other))
        self.hud.type("/pasta vault")
        self.hud.wait_for("VAULT · ao vivo")
        self.assertNotIn("PASTA · outra-pasta", self.hud.screen.text())
        self.assertFalse((self.data / "pasta").exists())

    def test_11_custom_lista_usa_confia_e_volta(self):
        self.hud.send(ALT[2])  # os comandos / do HUD valem também no modo Claude
        self.hud.type("/custom lista")
        self.hud.wait_for("foco")
        self.hud.wait_for("eco")
        self.hud.type("/custom foco")
        self.hud.wait_for("LEMBRETES")
        self.assertEqual((self.data / "custom").read_text(encoding="utf-8").strip(), "foco")
        self.assertEqual(self.calls(self.claude_log), [])  # nada foi para o agente
        # Painel de comando: recusar mantém desligado; aceitar roda e grava a confiança.
        self.hud.type("/custom eco")
        self.hud.wait_for("confiar nos comandos de eco")
        self.hud.send("n")
        self.hud.wait_for("ECO PROPRIO")
        self.hud.wait_for("não confiado")
        # O comando aparece na SAÍDA (“$ echo …”) para revisão; no painel, nada rodou.
        self.assertNotIn("│ saida-do-painel-eco", self.hud.screen.text())
        self.hud.type("/custom eco")
        self.hud.wait_for("confiar nos comandos de eco")
        self.hud.send("s")
        self.hud.wait_for("│ saida-do-painel-eco")
        self.assertIn("eco", json.loads((self.data / "custom_confianca.json").read_text()))
        self.hud.type("/custom padrao")
        self.hud.wait_for("AGENDA")
        self.assertFalse((self.data / "custom").exists())

    def test_12_proposta_do_agente_vira_customizacao(self):
        self.hud.send(ALT[2])
        self.hud.type("me faça uma PROPOSTA")
        self.hud.wait_for("proposta de customização")
        self.hud.type("/custom salvar")
        self.hud.wait_for("customização proposta gravada")
        layout = self.custom / "proposta" / "layout.toml"
        self.assertTrue(layout.is_file())
        self.assertEqual(stat.S_IMODE(layout.stat().st_mode), 0o644)
        self.hud.type("/custom proposta")
        self.hud.wait_for(lambda s: "AGENDA" not in s.text() and "SAÍDA" in s.text(), what="layout da proposta")
        shutil.rmtree(self.custom / "proposta")

    def test_08_roda_do_mouse_rola_a_saida(self):
        self.hud.type("/ajuda")
        self.hud.wait_for("Segurança")
        self.hud.wait_for("roda do mouse · PgUp/PgDn")
        self.hud.send(WHEEL_UP)
        self.hud.wait_for("↑ 3 linhas")
        self.hud.send(WHEEL_UP)
        self.hud.wait_for("↑ 6 linhas")
        self.hud.send(WHEEL_DOWN)
        self.hud.wait_for("↑ 3 linhas")
        self.hud.send(WHEEL_DOWN)
        self.hud.wait_for(lambda s: "roda do mouse · PgUp/PgDn" in s.text() and "linhas · roda" not in s.text(),
                          what="saída de volta ao fim")

    def test_09_terminal_pequeno_e_volta(self):
        self.hud.resize(20, 70)
        self.hud.wait_for("Terminal pequeno (70×20)")
        self.hud.resize(ROWS, COLS)
        self.hud.wait_for(lambda s: all(t in s.text() for t in ("SISTEMA", "SAÍDA", "ENTRADA"))
                          and "Terminal pequeno" not in s.text(), what="tela redesenhada")
        self.wait_input("ENTRADA", "›")

    def test_10_ctrl_c_sai_com_zero(self):
        self.hud.send(b"\x03")
        self.assertEqual(self.hud.wait_exit(), 0)


class ScreenTest(unittest.TestCase):
    """O emulador em si, com sequências escritas à mão."""

    def test_cup_el_ech_rep(self):
        s = Screen(3, 10)
        s.feed(b"\x1b[2;3Habc\x1b[2;4H\x1b[1X")
        self.assertEqual(s.line(1), "  a c     ")
        s.feed(b"\x1b[3;1Hx\x1b[4b\x1b[3;3H\x1b[K")
        self.assertEqual(s.line(2), "xx        ")

    def test_scroll_region_su_sd(self):
        s = Screen(4, 3)
        s.feed(b"\x1b[1;1Haaa\x1b[2;1Hbbb\x1b[3;1Hccc\x1b[4;1Hddd")
        s.feed(b"\x1b[2;3r\x1b[1S")
        self.assertEqual([s.line(r) for r in range(4)], ["aaa", "ccc", "   ", "ddd"])
        s.feed(b"\x1b[1T")
        self.assertEqual([s.line(r) for r in range(4)], ["aaa", "   ", "ccc", "ddd"])

    def test_sgr_colors_and_wide_chars(self):
        s = Screen(1, 8)
        s.feed("\x1b[38;5;208m日\x1b[39mz\x1b[31mq\x1b[0m".encode())
        self.assertEqual(s.line(0), "日zq    ")  # "日" ocupa 2 células
        self.assertEqual((s.fg_at(0, 0), s.fg_at(0, 2), s.fg_at(0, 3)), (208, None, 1))

    def test_osc_title_is_recorded(self):
        s = Screen(1, 5)
        s.feed(b"\x1b]0;oi\x07ok")
        self.assertEqual((s.title, s.line(0)), ("oi", "ok   "))


if __name__ == "__main__":
    unittest.main()
