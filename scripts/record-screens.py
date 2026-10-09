#!/usr/bin/env python3
"""Grava telas reais do HUD num pseudo-terminal, só com dados de demonstração.

    python3 scripts/record-screens.py            # grava e renderiza (PNG + GIF)
    python3 scripts/record-screens.py --sem-render

O que faz:
- recria do zero um ambiente sintético em /tmp/hud-demo: HOME falso, Vault de
  exemplo (com .obsidian e dois quadros Kanban), agenda com 3 compromissos,
  comandos inofensivos, cache falso de uso do Claude em $TMPDIR, Claude e
  Codex FALSOS (scripts que imprimem JSONL no formato real) e uma pasta de
  projeto de exemplo para o /pasta;
- copia o código do repositório para /tmp/hud-demo/app e roda o HUD de lá
  (`bin/hud -c config.toml`), assim nenhum caminho do checkout aparece na tela;
- abre o HUD num pty de 120×40 com TERM=xterm-256color, manda teclas de
  verdade e espera cada cena pelos títulos dos painéis (com prazo, como em
  tests/test_ui.py), sem depender da posição exata dos painéis;
- grava hud-demo.cast (asciinema v2), hud-demo.pty.txt (bytes crus) e
  hud-demo.metadata.json (revisão git, duração, eventos, sha256 do código e o
  instante de cada cena), mais o README-telas.md com a revisão gravada;
- varre tudo atrás de dados pessoais e só então publica em docs/telas/;
- chama scripts/render-screens.py para gerar os PNGs e o GIF.

Só biblioteca padrão. Não toca no Vault, em ~/.config/hud nem em
~/.local/share/hud, e não instala nada.
"""

import argparse
import codecs
import datetime as dt
import fcntl
import hashlib
import json
import locale
import os
import pty
import re
import select
import shutil
import signal
import stat
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from tests.vt import Screen  # noqa: E402  (emulador VT do próprio repositório)

DEMO = Path("/tmp/hud-demo")
OUT = ROOT / "docs" / "telas"
NAME = "hud-demo"
ROWS, COLS = 40, 120
WAIT = 15.0
GIT = "/usr/bin/git" if os.path.exists("/usr/bin/git") else "git"

F1 = b"\x1bOP"
ALT = {n: b"\x1b" + str(n).encode() for n in (1, 2, 3)}
WHEEL_UP = b"\x1b[<64;60;20M"
WHEEL_DOWN = b"\x1b[<65;60;20M"

# Padrões que não podem aparecer em nada do que é publicado. Os pessoais (usuário,
# home, e-mails do git de quem grava e o domínio deles) são lidos na hora, para
# nenhum dado de quem grava ficar escrito no repositório público.
GENERIC_FORBIDDEN = ["@gmail", "/home/", "sk-", "ghp_", "gho_", "github_pat_", "token", "BEGIN"]


def personal_patterns() -> list[str]:
    pats = {os.environ.get("USER", ""), os.environ.get("LOGNAME", ""), str(Path.home())}
    for scope in ("--global", "--local"):
        try:
            email = subprocess.run(["git", "-C", str(ROOT), "config", scope, "--get", "user.email"],
                                   capture_output=True, text=True, timeout=5).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            email = ""
        if "@" in email:
            local, domain = email.split("@", 1)
            pats |= {email, local, domain.split(".")[0]}
    # Termos a mais, só nesta máquina: HUD_RECORD_FORBIDDEN="termo1,termo2".
    pats |= set(os.environ.get("HUD_RECORD_FORBIDDEN", "").split(","))
    generic = {"gmail", "outlook", "hotmail", "yahoo", "root", "demo", "home", "user", "users"}
    return sorted(p for p in pats if len(p) >= 4 and p.lower() not in generic)


FORBIDDEN = GENERIC_FORBIDDEN + personal_patterns()

# Cenas: (arquivo do PNG, legenda). O instante de cada uma vai para o metadata.
SCENES = {
    "01-notas": "Tela inicial no modo notas: sistema, comandos, agenda, uso do Claude, Vault ao vivo e a saída na aba NOTAS.",
    "02-claude": "Aba e modo Claude (laranja): uma pergunta e a resposta do Claude falso.",
    "03-codex": "Aba e modo Codex (cinza): a pergunta e a resposta chegando do Codex falso.",
    "04-ajuda": "/ajuda na saída.",
    "05-rolagem": "Saída rolada para cima com a roda do mouse.",
    "06-pasta": "/pasta com um projeto de exemplo no lugar do Vault.",
}


# ── ambiente de demonstração ─────────────────────────────────────────────
def kanban(columns: dict[str, list[str]]) -> str:
    parts = ["---", "", "kanban-plugin: board", "", "---", ""]
    for col, cards in columns.items():
        parts.append(f"## {col}")
        parts.append("")
        done = col.lower().startswith("conclu")
        parts += [f"- [{'x' if done else ' '}] {c}" for c in cards]
        parts.append("")
    parts += ["", "%% kanban:settings", "```", '{"kanban-plugin":"board"}', "```", "%%", ""]
    return "\n".join(parts)


FAKE_CLAUDE = r'''#!/usr/bin/env python3
"""Claude FALSO da gravação: lê a pergunta e responde JSONL no formato do stream-json."""
import json, sys, time
sys.stdin.read()
def out(ev, pause=0.35):
    print(json.dumps(ev, ensure_ascii=False), flush=True)
    time.sleep(pause)
out({"type": "system", "subtype": "init", "session_id": "demo-sessao-1", "model": "modelo-demo",
     "slash_commands": ["review", "context", "resumo"]})
out({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Glob",
     "input": {"pattern": "**/Kanban*.md"}}]}})
out({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Read",
     "input": {"file_path": "Projetos/Kanban (Site novo).md"}}]}})
out({"type": "rate_limit_event", "rate_limit_info": {"unifiedWindows": {
     "five_hour": {"utilization": 0.36, "resetsAt": RESET5},
     "seven_day": {"utilization": 0.58, "resetsAt": RESET7}}}})
out({"type": "assistant", "message": {"content": [{"type": "text", "text":
     "Há dois cards bloqueados:\n"
     "• Site novo · Aprovar a paleta de cores (aguarda retorno do cliente)\n"
     "• Mudança de escritório · Contrato da internet (falta a assinatura)\n"
     "Sugestão: cobrar a paleta hoje; o contrato pode esperar a visita de quinta."}]}})
out({"type": "result", "subtype": "success", "is_error": False, "session_id": "demo-sessao-1",
     "total_cost_usd": 0.03}, 0)
'''

FAKE_CODEX = r'''#!/usr/bin/env python3
"""Codex FALSO da gravação: responde JSONL no formato do `codex exec --json`.
Depois da resposta fica "pensando" até ser interrompido com /parar."""
import json, sys, time
sys.stdin.read()
def out(ev, pause=0.4):
    print(json.dumps(ev, ensure_ascii=False), flush=True)
    time.sleep(pause)
out({"type": "thread.started", "thread_id": "demo-thread-1"})
out({"type": "item.started", "item": {"type": "command_execution", "command": "grep -rn TODO notas"}})
out({"type": "item.completed", "item": {"type": "command_execution", "command": "grep -rn TODO notas",
     "exit_code": 0}})
out({"type": "item.completed", "item": {"type": "agent_message", "text":
     "Encontrei 3 pendências nas notas:\n"
     "1. Reunião semanal: enviar a ata para o grupo\n"
     "2. Ideias: testar o protótipo no celular\n"
     "3. Leituras: terminar o capítulo 4"}})
time.sleep(600)
'''


def reset_demo() -> None:
    """Apaga e recria /tmp/hud-demo, recusando o que não for nosso."""
    if DEMO.is_symlink():
        sys.exit(f"{DEMO} é um link simbólico; apague à mão")
    if DEMO.exists():
        if DEMO.stat().st_uid != os.getuid():
            sys.exit(f"{DEMO} não é seu; apague à mão")
        shutil.rmtree(DEMO)
    DEMO.mkdir(mode=0o700)


def write(path: Path, text: str, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.write_text(text, encoding="utf-8")
    os.chmod(path, mode)


def copy_app() -> Path:
    """Copia os arquivos versionados (no estado do checkout) para DEMO/app."""
    app = DEMO / "app"
    try:
        names = subprocess.run([GIT, "ls-files", "-z"], cwd=ROOT, capture_output=True,
                               check=True).stdout.decode().split("\0")
    except (OSError, subprocess.CalledProcessError):
        names = [str(p.relative_to(ROOT)) for d in ("bin", "hud") for p in (ROOT / d).rglob("*")
                 if p.is_file() and "__pycache__" not in p.parts]
    for n in filter(None, names):
        src = ROOT / n
        if n.startswith("docs/") or not src.is_file():
            continue
        dst = app / n
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
    return app


def build_demo() -> dict:
    reset_demo()
    app = copy_app()
    home = DEMO / "home"
    claude_dir = home / ".claude"
    tmp = DEMO / "tmp"
    for d in (home, claude_dir, tmp):
        d.mkdir(mode=0o700, parents=True, exist_ok=True)

    # Vault de exemplo.
    vault = DEMO / "Vault"
    (vault / ".obsidian").mkdir(parents=True, mode=0o700)
    write(vault / ".obsidian" / "app.json", "{}\n")
    write(vault / "Projetos" / "Kanban (Site novo).md", kanban({
        "A fazer": ["Escrever textos da página inicial", "Escolher fotos do banco de imagens"],
        "Em andamento": ["Montar o protótipo navegável", "Revisar o formulário de contato"],
        "Bloqueado": ["Aprovar a paleta de cores"],
        "Concluído": ["Registrar o domínio", "Levantar requisitos"],
    }))
    write(vault / "Projetos" / "Kanban (Mudança de escritório).md", kanban({
        "A fazer": ["Pedir orçamento da mudança", "Etiquetar caixas"],
        "Em andamento": ["Separar os móveis que ficam"],
        "Bloqueado": ["Contrato da internet"],
        "Concluído": ["Visitar o novo endereço"],
    }))
    write(vault / "Reunião semanal.md",
          "# Reunião semanal\n\n- Pauta: andamento do site e da mudança\n- [ ] enviar a ata para o grupo\n")
    write(vault / "Ideias.md", "# Ideias\n\n- quadro de avisos na cozinha\n- [ ] testar o protótipo no celular\n")
    write(vault / "Leituras.md", "# Leituras\n\n- [ ] terminar o capítulo 4\n- [x] resumo do capítulo 3\n")
    write(vault / "Diário" / f"{dt.date.today().isoformat()}.md",
          "# Hoje\n\nCafé com a equipe, revisão do protótipo.\n")

    # Projeto de exemplo para o /pasta.
    proj = DEMO / "projeto-exemplo"
    write(proj / "README.md", "# Projeto exemplo\n\nUm site estático de demonstração.\n")
    write(proj / "Kanban (Projeto exemplo).md", kanban({
        "A fazer": ["Configurar o deploy"],
        "Em andamento": ["Página de contato", "Testes de acessibilidade"],
        "Bloqueado": ["Certificado do domínio"],
        "Concluído": ["Esqueleto do site"],
    }))
    write(proj / "docs" / "roteiro.md", "# Roteiro\n\n- [ ] publicar a versão 1\n- [ ] coletar opiniões\n")
    write(proj / "notas.txt", "lembrar: comprimir as imagens antes do deploy\n")

    # Agenda local com 3 compromissos.
    data = DEMO / "dados"
    data.mkdir(mode=0o700)
    today = dt.date.today()
    write(data / "agenda.md", "\n".join([
        f"- [ ] {today.isoformat()} 15:00 Revisão do protótipo com a equipe",
        f"- [ ] {(today + dt.timedelta(days=1)).isoformat()} 09:30 Dentista",
        f"- [ ] {(today + dt.timedelta(days=3)).isoformat()} 14:00 Visita ao novo escritório",
    ]) + "\n")

    # Cache falso de uso do Claude, no formato do hud/usage.py.
    now = time.time()
    key = hashlib.sha1(str(claude_dir).encode(), usedforsecurity=False).hexdigest()[:12]
    write(tmp / f"claude-sl-usage-{key}.json", json.dumps({
        "five_hour_used": 34, "seven_day_used": 57, "fetched_at": int(now),
        "five_hour_reset_min": 132, "seven_day_reset_min": 3 * 1440 + 310}) + "\n")

    # Claude e Codex falsos.
    bindir = DEMO / "bin"
    bindir.mkdir(mode=0o700)
    write(bindir / "claude", FAKE_CLAUDE.replace("RESET5", str(int(now + 132 * 60)))
          .replace("RESET7", str(int(now + (3 * 1440 + 310) * 60))), 0o700)
    write(bindir / "codex", FAKE_CODEX, 0o700)

    q = json.dumps
    config = DEMO / "config.toml"
    write(config, f"""\
vault = {q(str(vault))}
data_dir = {q(str(data))}
refresh_seconds = 1
vault_scan_seconds = 2

[[command]]
name = "Uptime"
argv = ["uptime"]

[[command]]
name = "Memória"
argv = ["free", "-h"]

[[command]]
name = "Data e hora"
argv = ["date", "+%d/%m/%Y %H:%M"]

[[command]]
name = "Kernel"
argv = ["uname", "-sr"]

[[command]]
name = "Eco de teste"
argv = ["echo", "olá do HUD"]

[[command]]
name = "Limpeza (simulada)"
argv = ["echo", "nada foi apagado"]
confirm = true

[claude]
executable = {q(str(bindir / "claude"))}

[codex]
executable = {q(str(bindir / "codex"))}
""")
    env = {
        "PATH": "/usr/bin:/bin",
        "HOME": str(home),
        "USER": "demo",
        "LOGNAME": "demo",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TERM": "xterm-256color",
        "TMPDIR": str(tmp),
        "CLAUDE_CONFIG_DIR": str(claude_dir),
        "ESCDELAY": "25",
    }
    # Dia da semana do relógio em português, se o sistema tiver o locale.
    try:
        locale.setlocale(locale.LC_TIME, "pt_BR.UTF-8")
        del env["LC_ALL"]
        env.update(LC_CTYPE="C.UTF-8", LC_TIME="pt_BR.UTF-8")
    except locale.Error:
        pass
    return {"app": app, "config": config, "env": env, "project": proj}


# ── sessão no pty ────────────────────────────────────────────────────────
class Session:
    def __init__(self, app: Path, config: Path, env: dict, cast_path: Path):
        self.screen = Screen(ROWS, COLS)
        self.raw = bytearray()
        self.events: list[dict] = []
        self.scenes: list[dict] = []
        self.dec = codecs.getincrementaldecoder("utf-8")("replace")
        self.cast = cast_path.open("w", encoding="utf-8")
        self.cast.write(json.dumps({
            "version": 2, "width": COLS, "height": ROWS, "timestamp": int(time.time()),
            "title": "HUD · sessão real num pty com dados de demonstração",
            "env": {"TERM": "xterm-256color"}}, ensure_ascii=False) + "\n")
        exe = app / "bin" / "hud"
        pid, fd = pty.fork()
        if pid == 0:
            try:
                fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
                os.chdir(DEMO)
                os.execve(str(exe), [str(exe), "-c", str(config)], env)
            finally:
                os._exit(127)
        self.pid, self.fd = pid, fd
        self.start = time.monotonic()
        self.status: int | None = None
        self.eof = False

    def now(self) -> float:
        return round(time.monotonic() - self.start, 6)

    def pump(self, timeout: float = 0.05) -> None:
        if self.eof:
            time.sleep(timeout)
            return
        r, _, _ = select.select([self.fd], [], [], timeout)
        if not r:
            return
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            data = b""
        if not data:
            self.eof = True
            return
        self.raw += data
        self.screen.feed(data)
        txt = self.dec.decode(data)
        if txt:
            self.cast.write(json.dumps([self.now(), "o", txt], ensure_ascii=False) + "\n")

    def idle(self, seconds: float) -> None:
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            self.pump(min(0.05, max(0.0, end - time.monotonic())))

    def send(self, label: str, data: bytes | str, pause: float = 0.6) -> None:
        data = data.encode() if isinstance(data, str) else data
        self.events.append({"time": self.now(), "label": label, "keys": data.decode("utf-8", "replace"),
                            "hex": data.hex()})
        os.write(self.fd, data)
        self.idle(pause)

    def type(self, label: str, line: str, pause: float = 0.6) -> None:
        """Digita como uma pessoa (uma letra por vez) e tecla Enter."""
        self.events.append({"time": self.now(), "label": label, "keys": line + "\r",
                            "hex": (line + "\r").encode().hex()})
        for ch in line:
            os.write(self.fd, ch.encode())
            self.idle(0.035)
        os.write(self.fd, b"\r")
        self.idle(pause)

    def wait_for(self, pred, what: str, timeout: float = WAIT) -> None:
        if isinstance(pred, str):
            text = pred
            pred = lambda s: text in s.text()  # noqa: E731
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self.pump()
            if pred(self.screen):
                return
        raise AssertionError(f"esperei {what} por {timeout:.0f}s; tela:\n{self.screen.text()}")

    def wait_all(self, *texts: str, timeout: float = WAIT) -> None:
        self.wait_for(lambda s: all(t in s.text() for t in texts), " + ".join(map(repr, texts)), timeout)

    def scene(self, name: str, hold: float = 1.2) -> None:
        """Marca a cena: o quadro do PNG é o estado da tela depois de `hold` s."""
        self.idle(hold)
        self.scenes.append({"name": name, "time": self.now(), "caption": SCENES[name]})

    def finish(self) -> int | None:
        end = time.monotonic() + WAIT
        while time.monotonic() < end:
            self.pump()
            pid, st = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.status = os.waitstatus_to_exitcode(st)
                break
        self.idle(0.2)
        return self.status

    def close(self) -> None:
        if self.status is None:
            try:
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
            except (ProcessLookupError, ChildProcessError):
                pass
        os.close(self.fd)
        self.cast.close()


def input_title(s) -> str:
    return s.line(s.rows - 3)


def run_session(demo: dict, stage: Path) -> dict:
    sess = Session(demo["app"], demo["config"], demo["env"], stage / f"{NAME}.cast")
    try:
        # 1. tela inicial no modo notas
        sess.wait_all("SISTEMA", "COMANDOS", "AGENDA", "USO CLAUDE", "VAULT · ao vivo", "SAÍDA", "ENTRADA")
        sess.wait_all("HUD pronto", "Site novo", "Montar o protótipo", "Dentista")
        sess.idle(1.0)
        sess.send("F1: roda o comando Uptime", F1)
        sess.wait_for("✓ Uptime", "fim do Uptime")
        sess.type("nota livre", "ligar para a gráfica sobre os folders")
        sess.wait_for("ligar para a gráfica", "nota na saída")
        sess.scene("01-notas")

        # 2. modo Claude (laranja), pergunta e resposta falsa
        sess.send("Alt+2: modo Claude", ALT[2])
        sess.wait_for(lambda s: "CLAUDE ·" in input_title(s), "entrada no modo Claude")
        sess.type("pergunta ao Claude", "quais cards estão bloqueados nos quadros?", 0.2)
        sess.wait_for("✓ claude", "resposta do Claude")
        sess.scene("02-claude")

        # 3. modo Codex (cinza)
        sess.send("Alt+3: modo Codex", ALT[3])
        sess.wait_for(lambda s: "CODEX ·" in input_title(s), "entrada no modo Codex")
        sess.type("pergunta ao Codex", "liste as pendências abertas nas notas", 0.2)
        sess.wait_for("terminar o capítulo 4", "resposta do Codex")
        sess.scene("03-codex", 0.8)
        sess.type("/parar: interrompe o Codex", "/parar")
        sess.wait_for(lambda s: "respondendo" not in input_title(s), "Codex parado")

        # 4. /ajuda
        sess.send("Alt+1: modo notas", ALT[1])
        sess.wait_for(lambda s: "ENTRADA" in input_title(s), "entrada no modo notas")
        sess.type("/limpar", "/limpar", 0.3)
        sess.type("/ajuda", "/ajuda")
        sess.wait_all("Segurança", "nunca escreve no Vault")
        sess.scene("04-ajuda")

        # 5. rolagem da saída
        for i in range(3):
            sess.send(f"roda do mouse para cima ({i + 1})", WHEEL_UP, 0.4)
        sess.wait_for(lambda s: re.search(r"↑ \d+ linhas", s.text()) is not None, "indicador de rolagem")
        sess.scene("05-rolagem")
        for i in range(3):
            sess.send(f"roda do mouse para baixo ({i + 1})", WHEEL_DOWN, 0.3)
        sess.wait_for(lambda s: re.search(r"↑ \d+ linhas", s.text()) is None, "saída de volta ao fim")

        # 6. /pasta com um projeto de exemplo
        sess.type("/pasta projeto de exemplo", f"/pasta {demo['project']}")
        sess.wait_all("PASTA · projeto-exemplo", "Página de contato")
        sess.scene("06-pasta", 1.5)

        sess.type("/pasta vault: volta ao Vault", "/pasta vault")
        sess.wait_for("VAULT · ao vivo", "volta ao Vault")
        sess.idle(0.8)
        sess.send("Ctrl+C: sai", b"\x03", 0)
        code = sess.finish()
        if code != 0:
            raise AssertionError(f"o HUD saiu com {code}")
    finally:
        sess.close()
    (stage / f"{NAME}.pty.txt").write_bytes(bytes(sess.raw))
    return {"events": sess.events, "scenes": sess.scenes, "duration_seconds": sess.now(),
            "exit_code": sess.status}


# ── metadados, texto e varredura ─────────────────────────────────────────
def git(*args: str) -> str:
    try:
        return subprocess.run([GIT, *args], cwd=ROOT, capture_output=True, text=True,
                              check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return ""


def code_hashes(app: Path) -> dict:
    files = [app / "bin" / "hud", *sorted((app / "hud").rglob("*.py"))]
    per = {str(p.relative_to(app)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    total = hashlib.sha256("".join(f"{h}  {n}\n" for n, h in per.items()).encode()).hexdigest()
    return {"bin_hud_sha256": per["bin/hud"], "code_sha256": total, "files_sha256": per}


def readme_fragment(meta: dict) -> str:
    rev, date = meta["revision_short"], meta["recorded_at"][:10]
    dirty = " (com mudanças locais não commitadas)" if meta["revision_dirty"] else ""
    lines = [
        "## Telas gravadas",
        "",
        "Execução real do HUD num pseudo-terminal (pty) de 120×40, com `TERM=xterm-256color` e teclas "
        "de verdade. Os PNGs e o GIF são quadros renderizados da gravação, não capturas de tela do "
        "desktop. Todos os dados são de demonstração: HOME, Vault, agenda, comandos e cache de uso "
        "sintéticos, e o Claude e o Codex são scripts falsos que respondem no formato real. Só os "
        "números do painel SISTEMA (CPU, memória, disco) são da máquina que gravou.",
        "",
        f"**Revisão gravada:** `{rev}`{dirty}, em {date}. Para gravar de novo: "
        "`python3 scripts/record-screens.py` (o HUD continua sem dependências; só a renderização "
        "usa Pillow, num ambiente virtual temporário).",
        "",
    ]
    for s in meta["scenes"]:
        lines += [f"![{s['caption']}](docs/telas/{s['name']}.png)", "", s["caption"], ""]
    lines += [f"![Sessão inteira, animada: notas, Claude, Codex, ajuda, rolagem e /pasta.](docs/telas/{NAME}.gif)",
              "", "Sessão inteira, animada: notas, Claude, Codex, ajuda, rolagem e /pasta.", "",
              f"A gravação crua fica em `docs/telas/{NAME}.cast` (asciinema v2), `{NAME}.pty.txt` "
              f"e `{NAME}.metadata.json` (revisão, duração, teclas enviadas e sha256 do código).", ""]
    return "\n".join(lines)


def scan(paths: list[Path]) -> list[str]:
    hits = []
    for p in paths:
        text = p.read_bytes().decode("utf-8", "replace")
        for pat in FORBIDDEN:
            for m in re.finditer(re.escape(pat), text, re.IGNORECASE):
                ctx = text[max(0, m.start() - 30):m.end() + 30].replace("\n", "\\n")
                hits.append(f"{p.name}: {pat!r} em …{ctx}…")
    return hits


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--sem-render", action="store_true", help="só grava; não gera PNG nem GIF")
    args = ap.parse_args()

    demo = build_demo()
    stage = DEMO / "saida"
    stage.mkdir(mode=0o700)
    started = dt.datetime.now().astimezone()
    result = run_session(demo, stage)

    meta = {
        "name": NAME,
        "capture": "execução real do HUD num pty; teclas e roda do mouse injetadas; sem desktop nem navegador",
        "data": "100% demonstração: HOME, Vault, agenda, comandos e cache de uso sintéticos em /tmp/hud-demo; "
                "Claude e Codex falsos; só os números do painel SISTEMA são da máquina",
        "recorded_at": started.isoformat(timespec="seconds"),
        "revision": git("rev-parse", "HEAD"),
        "revision_short": git("rev-parse", "--short", "HEAD"),
        "revision_dirty": bool(git("status", "--porcelain", "--", "bin", "hud")),
        "command": "bin/hud -c /tmp/hud-demo/config.toml",
        "terminal": {"rows": ROWS, "cols": COLS, "TERM": "xterm-256color"},
        "python": sys.version.split()[0],
        **code_hashes(demo["app"]),
        **result,
    }
    (stage / f"{NAME}.metadata.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n",
                                                 encoding="utf-8")
    (stage / "README-telas.md").write_text(readme_fragment(meta), encoding="utf-8")

    outputs = sorted(stage.iterdir())
    hits = scan(outputs)
    if hits:
        print("varredura de dados pessoais FALHOU; nada foi publicado em docs/telas:", file=sys.stderr)
        print("\n".join(hits), file=sys.stderr)
        print(f"(os arquivos ficaram em {stage})", file=sys.stderr)
        return 1
    OUT.mkdir(parents=True, exist_ok=True)
    for p in outputs:
        shutil.copyfile(p, OUT / p.name)
    print(f"varredura: 0 ocorrências de {len(FORBIDDEN)} padrões em {len(outputs)} arquivos")
    print(f"gravado em {OUT}: {', '.join(p.name for p in outputs)} · {meta['duration_seconds']:.1f}s")

    if args.sem_render:
        return 0
    return subprocess.call([sys.executable, str(ROOT / "scripts" / "render-screens.py")])


if __name__ == "__main__":
    sys.exit(main())
