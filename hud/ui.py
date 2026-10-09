"""A tela única do HUD (curses).

┌ SISTEMA ────────┐┌ VAULT ─────────────────────┐
│                 ││                            │
├ COMANDOS ───────┤├ SAÍDA ─────────────────────┤
├ AGENDA ─────────┤│                            │
└─────────────────┘└────────────────────────────┘
┌ › entrada ───────────────────────────────────┐
"""

import curses
import dataclasses
import datetime as dt
import os
import queue
import threading
import time
from collections import deque
from pathlib import Path

from . import agenda as ag
from .config import Command, Config, ConfigError, remember_folder, validate_folder
from .metrics import Metrics, human_bytes, human_duration
from . import usage as us
from .claude import Claude
from .codex import Codex
from .runner import Runner
from .text import clean_line, fit, pad, width, wrap
from .vault import VaultWatcher

SPARK = "▁▂▃▄▅▆▇█"
SPIN = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
MIN_W, MIN_H = 80, 24
MAX_INPUT = 500
WHEEL_UP = curses.BUTTON4_PRESSED
WHEEL_DOWN = getattr(curses, "BUTTON5_PRESSED", 0x200000)
WHEEL_STEP = 3
MODE_KEYS = {"1": "notas", "2": "claude", "3": "codex"}  # Alt+1/2/3

HELP = """\
Teclas
  F1–F10 ou /r N · roda o comando do painel
  Alt+1 notas · Alt+2 Claude · Alt+3 Codex · Tab alterna entre os três
  Enter · envia a linha · ↑/↓ · histórico · Esc · limpa a linha
  roda do mouse ou PgUp/PgDn · rola a saída · Ctrl+L · redesenha · Ctrl+C · sai
Entrada
  texto livre · vira nota com data e hora (notas.md, fora do Vault)
  /r N ou /r nome · roda um comando do painel
  /ag quando [hora] texto · ex.: /ag amanhã 14h dentista, /ag sex revisar PR, /ag 12/10 9:30 call
  /ok N · conclui o item N da agenda · /rm N · apaga
  /b termo · busca no Vault (nome e conteúdo, só leitura)
  /notas [N] · últimas notas · /conflitos · conflitos de sync do Vault
  /vault · relê a pasta agora · /limpar · limpa a saída · /sair
  /pasta caminho · lê outra pasta local no lugar do Vault (lembrada) · /pasta vault · volta ao Vault
Claude Code (laranja) e Codex (cinza)
  /c pergunta · pergunta ao Claude · /x pergunta · pergunta ao Codex (a conversa continua)
  Tab · alterna notas → Claude → Codex: no modo de um agente, texto livre vai para ele
  No modo Claude, comandos / que o HUD não conhece vão para o Claude (/review, skills…);
  //comando força mandar ao agente um nome que o HUD também usa (ex.: //clear)
  /comandos · lista os comandos / do Claude · /novo · nova conversa · /parar · interrompe
  /perfil leitura|completo · troca o perfil do agente do modo atual nesta sessão
    leitura: Claude só lê o Vault, sem MCP · Codex em sandbox read-only
    completo: o mesmo Claude Code / Codex do seu terminal (ferramentas, MCP, skills)
Segurança
  Só rodam os comandos da lista: sem shell, PATH e ambiente fixos, tempo-limite e saída limitada.
  O HUD nunca escreve no Vault. Texto de fora tem as sequências de escape removidas."""


class Hud:
    def __init__(self, cfg: Config):
        self.cfg = cfg
        self.events: queue.Queue = queue.Queue()
        self.runner = Runner(self.events)
        self.metrics = Metrics(extra_paths=[cfg.vault] if cfg.vault.is_dir() else [])
        self.vault = VaultWatcher(cfg.vault, cfg.vault_scan)
        cfg.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.agenda = ag.Agenda(cfg.data_dir / "agenda.md")
        self.agenda.prune(dt.date.today())
        self.notes_path = cfg.data_dir / "notas.md"
        self.out: deque[tuple[str, str]] = deque(maxlen=3000)
        self.scroll = 0
        self.inp = ""
        self.cur = 0
        self.history: list[str] = []
        self.hist_i = 0
        self.pending: Command | None = None
        self.agenda_view: list[ag.Item] = []
        self.quit = False
        self.tick = 0
        self.agents: dict[str, Claude | Codex] = {}
        if cfg.claude:
            self.agents["claude"] = Claude(cfg.claude, self.events)
        if cfg.codex:
            self.agents["codex"] = Codex(cfg.codex, self.events)
        self.mode = "notas"  # ou o nome de um agente
        self.usage = us.UsageTracker()
        self.codex_usage = us.CodexUsageTracker()
        self.codex_usage.poll()

    # ── saída ────────────────────────────────────────────────────────────
    def say(self, text: str, style: str = "text") -> None:
        for line in text.split("\n"):
            self.out.append((style, line))
        if self.scroll:
            self.scroll += text.count("\n") + 1

    def header(self, text: str) -> None:
        self.out.append(("blank", ""))
        self.out.append(("head", f"{time.strftime('%H:%M:%S')}  {text}"))

    # ── ciclo ────────────────────────────────────────────────────────────
    def run(self, scr) -> None:
        self.scr = scr
        curses.curs_set(1)
        scr.keypad(True)
        scr.timeout(200)
        # Só a roda importa. Com o mouse ligado o terminal passa os cliques
        # para o HUD; para selecionar texto, Shift+arrastar.
        curses.mousemask(WHEEL_UP | WHEEL_DOWN)
        curses.mouseinterval(0)
        self._colors()
        self.vault.start()
        self.say("HUD pronto. Digite /ajuda para ver tudo o que a entrada aceita.", "dim")
        self.say(f"configuração: {self.cfg.source} · dados: {self.cfg.data_dir}", "dim")
        for w in self.cfg.warnings:
            self.say(f"⚠ {w}", "warn")
        last_sample = 0.0
        last_vault = -1
        try:
            while not self.quit:
                now = time.monotonic()
                dirty = self._drain()
                if now - last_sample >= self.cfg.refresh:
                    self.metrics.sample()
                    self.usage.poll()
                    if self.tick % 5 == 0:
                        self.codex_usage.poll()
                    last_sample = now
                    self.tick += 1
                    dirty = True
                if self.vault.version != last_vault:
                    last_vault = self.vault.version
                    dirty = True
                if dirty:
                    self.draw()
                try:
                    ch = scr.get_wch()
                except curses.error:
                    continue
                self.key(ch)
                self.draw()
        except KeyboardInterrupt:
            pass
        finally:
            self.vault.stop()
            for agent in self.agents.values():
                agent.stop()

    def _drain(self) -> bool:
        got = False
        while True:
            try:
                ev = self.events.get_nowait()
            except queue.Empty:
                return got
            got = True
            kind = ev[0]
            if kind == "cmd_out":
                _, cmd, lines = ev
                for line in lines:
                    self.say(line, "text")
            elif kind == "cmd_end":
                _, cmd, rc, dur, timed_out, truncated = ev
                extra = " · saída cortada" if truncated else ""
                if timed_out:
                    self.say(f"✗ {cmd.name}: interrompido após {cmd.timeout:.0f}s{extra}", "crit")
                elif rc == 0:
                    self.say(f"✓ {cmd.name} · {dur:.1f}s{extra}", "ok")
                else:
                    self.say(f"✗ {cmd.name} · código {rc} · {dur:.1f}s{extra}", "warn")
            elif kind == "agent_text":
                self.say(ev[2], ev[1])
            elif kind == "agent_tool":
                self.say(f"  ⚙ {ev[2]}", f"{ev[1]}_dim")
            elif kind == "agent_denied":
                self.say(f"  ⊘ negado: {ev[2]}", "warn")
            elif kind == "usage_event":
                self.usage.offer(us.parse_event(ev[1]))
            elif kind == "agent_end":
                _, name, ok, msg, dur, summary = ev
                tail = (f" · {dur:.0f}s" if dur is not None else "") + (f" · {summary}" if summary else "")
                if ok:
                    self.say(f"✓ {name}{tail}", name)
                else:
                    self.say(f"✗ {name}: {msg}{tail}", "warn")
            elif kind == "search":
                _, term, hits = ev
                if not hits:
                    self.say(f"nada encontrado para “{term}”", "dim")
                for rel, n, line in hits:
                    loc = f"{rel}:{n}" if n else rel
                    self.say(f"{loc}", "accent")
                    if line:
                        self.say(f"    {line}", "text")
                if hits:
                    self.say(f"{len(hits)} resultado(s)", "dim")

    # ── cores ────────────────────────────────────────────────────────────
    def _colors(self) -> None:
        self.c: dict[str, int] = {}
        try:
            curses.start_color()
            curses.use_default_colors()
            pairs = {"accent": curses.COLOR_CYAN, "ok": curses.COLOR_GREEN,
                     "warn": curses.COLOR_YELLOW, "crit": curses.COLOR_RED,
                     "mag": curses.COLOR_MAGENTA, "blue": curses.COLOR_BLUE}
            # Laranja = Claude, cinza = Codex; em terminal de 8 cores, amarelo e branco.
            rich = curses.COLORS >= 256
            pairs["claude"] = 208 if rich else curses.COLOR_YELLOW
            pairs["codex"] = 248 if rich else curses.COLOR_WHITE
            pairs["codex_dim"] = 242 if rich else curses.COLOR_WHITE
            for i, (k, col) in enumerate(pairs.items(), 1):
                curses.init_pair(i, col, -1)
                self.c[k] = curses.color_pair(i)
        except curses.error:
            pass
        A = curses
        self.style = {
            "text": A.A_NORMAL, "dim": A.A_DIM, "bold": A.A_BOLD,
            "accent": self.c.get("accent", 0), "ok": self.c.get("ok", 0),
            "warn": self.c.get("warn", A.A_BOLD), "crit": self.c.get("crit", A.A_BOLD) | A.A_BOLD,
            "head": self.c.get("accent", 0) | A.A_BOLD, "you": self.c.get("mag", 0),
            "border": self.c.get("blue", 0), "title": self.c.get("accent", 0) | A.A_BOLD,
            "blank": A.A_NORMAL,
            "claude": self.c.get("claude", A.A_BOLD), "claude_dim": self.c.get("claude", 0) | A.A_DIM,
            "codex": self.c.get("codex", 0), "codex_dim": self.c.get("codex_dim", A.A_DIM),
        }

    def level(self, pct: float) -> str:
        return "crit" if pct >= 90 else "warn" if pct >= 70 else "ok"

    # ── desenho ──────────────────────────────────────────────────────────
    def put(self, y: int, x: int, s: str, style: str | int = "text", w: int | None = None) -> int:
        H, W = self.scr.getmaxyx()
        if y < 0 or y >= H or x >= W:
            return x
        room = W - x if w is None else min(w, W - x)
        s = fit(s, room)
        attr = self.style.get(style, 0) if isinstance(style, str) else style
        try:
            self.scr.addstr(y, x, s, attr)
        except curses.error:
            pass  # escrever na última célula da tela sempre "falha"
        return x + width(s)

    def box(self, y: int, x: int, h: int, w: int, title: str, right: str = "",
            color: str | None = None) -> None:
        b = self.style[color or "border"]
        self.put(y, x, "╭" + "─" * (w - 2) + "╮", b)
        for i in range(1, h - 1):
            self.put(y + i, x, "│", b)
            self.put(y + i, x + w - 1, "│", b)
        self.put(y + h - 1, x, "╰" + "─" * (w - 2) + "╯", b)
        self.put(y, x + 2, f" {title} ", (self.style[color] | curses.A_BOLD) if color else "title")
        if right:
            r = fit(f" {right} ", w - width(title) - 8)
            self.put(y, x + w - 2 - width(r), r, "dim")

    def bar(self, y: int, x: int, w: int, pct: float) -> None:
        filled = int(round(w * pct / 100))
        self.put(y, x, "█" * filled, self.level(pct))
        self.put(y, x + filled, "░" * (w - filled), "dim")

    def draw(self) -> None:
        scr = self.scr
        scr.erase()
        H, W = scr.getmaxyx()
        if W < MIN_W or H < MIN_H:
            self.put(0, 0, f"Terminal pequeno ({W}×{H}). O HUD precisa de {MIN_W}×{MIN_H}.", "warn")
            self.put(1, 0, "Aumente a janela ou Ctrl+C para sair.", "dim")
            scr.refresh()
            return
        lw = max(38, min(56, W * 36 // 100))
        rw = W - lw
        body = H - 3
        sys_h = 11
        use_h = 4
        cx = self.codex_usage.current
        cdx_h = max(3, len(cx.windows) + 2) if cx and cx.windows else 4
        cmd_h = min(len(self.cfg.commands) + 2, max(4, body - sys_h - use_h - cdx_h - 5))
        ag_h = body - sys_h - cmd_h - use_h - cdx_h
        vault_h = max(10, body * 55 // 100)
        out_h = body - vault_h

        self.draw_system(0, 0, sys_h, lw)
        self.draw_commands(sys_h, 0, cmd_h, lw)
        self.draw_agenda(sys_h + cmd_h, 0, ag_h, lw)
        self.draw_usage(sys_h + cmd_h + ag_h, 0, use_h, lw)
        self.draw_usage_codex(sys_h + cmd_h + ag_h + use_h, 0, cdx_h, lw)
        self.draw_vault(0, lw, vault_h, rw)
        self.draw_output(vault_h, lw, out_h, rw)
        cy, cx = self.draw_input(body, 0, 3, W)
        try:
            scr.move(cy, cx)
        except curses.error:
            pass
        scr.refresh()

    def draw_system(self, y: int, x: int, h: int, w: int) -> None:
        s = self.metrics.last
        self.box(y, x, h, w, "SISTEMA", self.metrics.hostname)
        iw = w - 4
        x0, r = x + 2, y + 1
        lab = 6
        bw = max(8, iw - lab - 18)

        def meter(row: int, name: str, pct: float, tail: str) -> None:
            self.put(row, x0, name, "dim")
            self.bar(row, x0 + lab, bw, pct)
            self.put(row, x0 + lab + bw + 1, f"{pct:3.0f}%", self.level(pct))
            self.put(row, x0 + lab + bw + 6, tail, "dim", iw - lab - bw - 6)

        meter(r, "CPU", s.cpu, f"{s.cores} núcleos")
        hist = list(self.metrics.cpu_hist)[-(iw - lab):]
        spark = "".join(SPARK[min(7, int(v / 100 * 8))] for v in hist)
        self.put(r + 1, x0 + lab, spark, "accent")
        mem_pct = 100 * s.mem_used / s.mem_total if s.mem_total else 0
        meter(r + 2, "MEM", mem_pct, f"{human_bytes(s.mem_used)}/{human_bytes(s.mem_total)}")
        if s.swap_total:
            sw = 100 * s.swap_used / s.swap_total
            meter(r + 3, "SWAP", sw, f"{human_bytes(s.swap_used)}/{human_bytes(s.swap_total)}")
        else:
            self.put(r + 3, x0, "SWAP", "dim")
            self.put(r + 3, x0 + lab, "sem swap", "dim")
        row = r + 4
        for d in s.disks[:2]:
            meter(row, fit(d.label, lab - 1, False), d.pct,
                  f"{human_bytes(d.total - d.used)} livre")
            row += 1
        row = r + 6
        l1, l5, l15 = s.load
        lp = 100 * l1 / s.cores
        self.put(row, x0, "LOAD", "dim")
        nx = self.put(row, x0 + lab, f"{l1:.2f}", self.level(lp))
        self.put(row, nx, f"  {l5:.2f}  {l15:.2f} · {s.procs} proc", "dim", x0 + iw - nx)
        self.put(row + 1, x0, "REDE", "dim")
        nx = self.put(row + 1, x0 + lab, f"↓ {human_bytes(s.rx_rate, '/s')}", "accent")
        self.put(row + 1, nx, f"   ↑ {human_bytes(s.tx_rate, '/s')}", "mag")
        parts: list[tuple[str, str]] = []
        if s.temp is not None:
            parts.append((f"{s.temp:.0f}°C", "crit" if s.temp >= 85 else "warn" if s.temp >= 70 else "ok"))
        if s.battery:
            pct, st = s.battery
            status = {"Charging": "carregando", "Discharging": "na bateria", "Full": "cheia",
                      "Not charging": "parada"}.get(st, st.lower())
            parts.append((f"bat {pct}% {status}", "crit" if pct <= 15 and st == "Discharging" else "text"))
        self.put(row + 2, x0, "TEMP" if s.temp is not None else "BAT", "dim")
        nx = x0 + lab
        for i, (t, st) in enumerate(parts):
            nx = self.put(row + 2, nx, ("  ·  " if i else ""), "dim")
            nx = self.put(row + 2, nx, t, st)
        self.put(y + h - 1, x + 2, f" ligado há {human_duration(s.uptime)} ", "dim")
        clock = time.strftime(" %a %d/%m  %H:%M:%S ")
        self.put(y + h - 1, x + w - 1 - width(clock), clock, "bold")

    def draw_commands(self, y: int, x: int, h: int, w: int) -> None:
        self.box(y, x, h, w, "COMANDOS", "F1–F10 · /r N")
        if not self.cfg.commands:
            self.put(y + 1, x + 2, "nenhum comando permitido", "dim")
        for i, c in enumerate(self.cfg.commands[: h - 2]):
            r = y + 1 + i
            running = self.runner.is_running(c)
            nx = self.put(r, x + 2, f"{c.key}", "accent")
            nx = self.put(r, nx + 1, SPIN[self.tick % len(SPIN)] if running else " ", "warn")
            nx = self.put(r, nx + 1, c.name, "bold" if running else "text", w - 10)
            if c.confirm:
                self.put(r, x + w - 4, "!", "warn")

    def draw_agenda(self, y: int, x: int, h: int, w: int) -> None:
        today = dt.date.today()
        snap = self.vault.snapshot
        items = ag.upcoming(self.agenda.items, snap.tasks, today)
        self.agenda_view = items
        late = sum(1 for i in items if i.date < today)
        today_n = sum(1 for i in items if i.date == today)
        self.box(y, x, h, w, "AGENDA", f"{today_n} hoje" + (f" · {late} atrasado(s)" if late else ""))
        iw, r, last = w - 4, y + 1, None
        if not items:
            self.put(r, x + 2, "Nada marcado.", "dim")
            self.put(r + 1, x + 2, "/ag amanhã 14h dentista", "accent")
            return
        for n, it in enumerate(items, 1):
            if r >= y + h - 1:
                break
            if it.date != last:
                if r >= y + h - 2:
                    break
                lbl = ag.day_label(it.date, today)
                self.put(r, x + 2, lbl, "crit" if it.date < today else "head" if it.date == today else "dim")
                r += 1
                last = it.date
            nx = self.put(r, x + 2, f"{n:>2} ", "dim")
            nx = self.put(r, nx, f"{it.time or '     '} ", "accent")
            label = it.text + ("  ◆" if it.source else "")
            self.put(r, nx, label, "warn" if it.date < today else "text", iw - (nx - x - 2))
            r += 1

    def draw_vault(self, y: int, x: int, h: int, w: int) -> None:
        snap = self.vault.snapshot
        ago = f"lido há {int(time.time() - snap.scanned_at)}s" if snap.scanned_at else "lendo…"
        root = self.cfg.vault
        title = "VAULT · ao vivo" if snap.obsidian or not snap.ok else f"PASTA · {clean_line(root.name)} · ao vivo"
        self.box(y, x, h, w, title, ago)
        x0, iw, r, end = x + 2, w - 4, y + 1, y + h - 1
        if not snap.ok:
            self.put(r, x0, snap.error or f"lendo {root}…", "warn" if snap.error else "dim")
            return
        unit = "notas" if snap.obsidian else "arquivos de texto"
        nx = self.put(r, x0, f"{snap.notes}{'+' if snap.capped else ''} {unit}", "bold")
        nx = self.put(r, nx, f" · {snap.today} mexidos hoje · {len(snap.boards)} quadros", "dim")
        if snap.conflicts:
            self.put(r, nx, f"  ⚠ {len(snap.conflicts)} conflito(s) de sync", "warn")
        r += 2
        if not snap.boards:
            r -= 1
        name_w = max(10, min(18, iw - 34))
        cols = [("todo", "a fazer"), ("doing", "andam."), ("blocked", "bloq."), ("done", "feito")]
        if snap.boards:
            self.put(r, x0, pad("QUADRO", name_w), "dim")
            for i, (_, lbl) in enumerate(cols):
                self.put(r, x0 + name_w + i * 8, f"{lbl:>7}", "dim")
            r += 1
        for b in snap.boards:
            if r >= end - 3:
                break
            self.put(r, x0, pad(b.name, name_w), "text")
            for i, (k, _) in enumerate(cols):
                n = b.counts.get(k)
                st = ("warn" if k == "doing" else "crit" if k == "blocked" else "dim") if n else "dim"
                self.put(r, x0 + name_w + i * 8, f"{'—' if n is None else n:>7}", st)
            r += 1
        r += 1
        doing = [(b.name, t) for b in snap.boards for t in b.doing]
        wide = iw >= 110
        lw = iw // 2 - 1 if wide else iw
        top = r
        if doing and r < end:
            self.put(r, x0, f"EM ANDAMENTO ({len(doing)})", "head")
            r += 1
            # Empilhado, metade do espaço restante fica para os recentes.
            room = end - r if wide else max(2, (end - r - 2) // 2)
            shown = doing if len(doing) <= room else doing[: max(1, room - 1)]
            for board, title in shown:
                nx = self.put(r, x0, fit(board, 8, False).ljust(9), "warn")
                self.put(r, nx, title, "text", lw - 9)
                r += 1
            if len(shown) < len(doing):
                self.put(r, x0, f"… mais {len(doing) - len(shown)}", "dim")
                r += 1
            if not wide:
                r += 1
        rx, rr = (x0 + lw + 2, top) if wide else (x0, r)
        if rr < end:
            self.put(rr, rx, "RECENTES", "head")
            rr += 1
            now = time.time()
            for rel, m in snap.recent:
                if rr >= end:
                    break
                p = Path(rel)
                age = _ago(now - m)
                nx = self.put(rr, rx, f"{age:>6} ", "dim")
                nx = self.put(rr, nx, clean_line(p.stem), "text", (iw if not wide else lw) - 7)
                rr += 1
        self.put(end, x + 2, f" {self.cfg.vault} · varredura {snap.scan_ms:.0f}ms ", "dim")

    def output_lines(self, w: int) -> list[tuple[str, str]]:
        lines: list[tuple[str, str]] = []
        for style, text in self.out:
            for part in wrap(text, w) or [""]:
                lines.append((style, part))
        return lines

    def draw_output(self, y: int, x: int, h: int, w: int) -> None:
        iw, ih = w - 4, h - 2
        lines = self.output_lines(iw)
        max_scroll = max(0, len(lines) - ih)
        self.scroll = min(self.scroll, max_scroll)
        start = len(lines) - ih - self.scroll
        view = lines[max(0, start): max(0, start) + ih]
        right = f"↑ {self.scroll} linhas · roda ou PgDn volta" if self.scroll else "roda do mouse · PgUp/PgDn"
        self.box(y, x, h, w, "SAÍDA", right)
        for i, (style, text) in enumerate(view):
            self.put(y + 1 + i, x + 2, text, style)

    def draw_usage(self, y: int, x: int, h: int, w: int) -> None:
        u = self.usage.current
        age = f"há {_ago(time.time() - u.fetched_at)}" if u else ""
        self.box(y, x, h, w, "USO CLAUDE", age.replace("há agora", "agora"))
        if not u:
            self.put(y + 1, x + 2, "sem dado ainda: abra o Claude Code", "dim")
            self.put(y + 2, x + 2, "ou pergunte algo com /c", "dim")
            return
        for i, (label, win) in enumerate((("5h", u.five_hour), ("semana", u.seven_day))):
            self.usage_row(y + 1 + i, x, w, label, win.left, win.resets_at)

    def usage_row(self, r: int, x: int, w: int, label: str, left: float, resets_at) -> None:
        glyph = {"full": "██", "part": "▒▒", "empty": "░░"}
        st = us.level(left)
        nx = self.put(r, x + 2, f"{label:<7}", "dim")
        nx = self.put(r, nx, f"{left:3.0f}% ", st)
        for seg in us.segments(left):
            nx = self.put(r, nx, glyph[seg], st if seg != "empty" else "dim") + 1
        reset = us.until(resets_at)
        if reset:
            self.put(r, nx + 1, f"↺ {reset}", "dim", x + w - 2 - nx - 1)

    def draw_usage_codex(self, y: int, x: int, h: int, w: int) -> None:
        u = self.codex_usage.current
        right = (f"{u.plan} · " if u and u.plan else "") + (
            f"há {_ago(time.time() - u.fetched_at)}".replace("há agora", "agora") if u else "")
        self.box(y, x, h, w, "USO CODEX", right)
        if not u or not u.windows:
            self.put(y + 1, x + 2, "sem dado ainda: use o Codex (/x)", "dim")
            return
        for i, win in enumerate(u.windows[: h - 2]):
            self.usage_row(y + 1 + i, x, w, win.label, win.left, win.resets_at)

    def draw_input(self, y: int, x: int, h: int, w: int) -> tuple[int, int]:
        spin = SPIN[self.tick % len(SPIN)]
        busy = [n for n, a in self.agents.items() if a.busy]
        agent = self.agents.get(self.mode)
        color = None
        if self.pending:
            title, hint = f"confirmar: {self.pending.name}?", "s = sim · qualquer outra tecla cancela"
        elif agent:
            color = self.mode
            prof = agent.profile.upper()
            title = f"{self.mode.upper()} · {prof}" + (f" {spin} respondendo" if agent.busy else "")
            hint = "Alt+1 notas · Alt+2/3 agentes · /perfil · /novo · /parar"
        else:
            title = "ENTRADA" + "".join(f" · {n} {spin}" for n in busy)
            hint = "texto = nota · Alt+2 Claude · Alt+3 Codex · /ajuda"
        self.box(y, x, h, w, title, hint, color)
        if agent and agent.profile == "completo":
            self.put(y, x + 4 + width(title) + 2, " ⚠ ferramentas completas ", "warn")
        iw = w - 6
        prompt_x = x + 2
        self.put(y + 1, prompt_x, "✦" if agent else "›", color or "accent")
        # Janela horizontal que acompanha o cursor.
        before = self.inp[: self.cur]
        start = 0
        while width(before[start:]) > iw - 1:
            start += 1
        shown = self.inp[start:]
        self.put(y + 1, prompt_x + 2, shown, "text", iw)
        return y + 1, prompt_x + 2 + width(self.inp[start: self.cur])

    # ── teclado ──────────────────────────────────────────────────────────
    def key(self, ch) -> None:
        if ch == curses.KEY_MOUSE:
            self.mouse()
            return
        if self.pending:
            cmd, self.pending = self.pending, None
            if ch in ("s", "S", "y", "Y"):
                self.launch(cmd, confirmed=True)
            else:
                self.say(f"{cmd.name}: cancelado", "dim")
            return
        if ch == curses.KEY_RESIZE:
            return
        if ch == "\t":
            modes = ["notas", *self.agents]
            if len(modes) == 1:
                self.say("Claude e Codex desligados (veja os avisos no início ou hud --check)", "warn")
                return
            self.mode = modes[(modes.index(self.mode) + 1) % len(modes)]
            return
        if isinstance(ch, int):
            if curses.KEY_F1 <= ch <= curses.KEY_F0 + 10:
                self.run_slot(ch - curses.KEY_F1)
            elif ch in (curses.KEY_BACKSPACE,):
                self.backspace()
            elif ch == curses.KEY_DC:
                self.inp = self.inp[: self.cur] + self.inp[self.cur + 1:]
            elif ch == curses.KEY_LEFT:
                self.cur = max(0, self.cur - 1)
            elif ch == curses.KEY_RIGHT:
                self.cur = min(len(self.inp), self.cur + 1)
            elif ch == curses.KEY_HOME:
                self.cur = 0
            elif ch == curses.KEY_END:
                self.cur = len(self.inp)
            elif ch == curses.KEY_UP:
                self.hist(-1)
            elif ch == curses.KEY_DOWN:
                self.hist(1)
            elif ch == curses.KEY_PPAGE:
                self.scroll += max(1, self.scr.getmaxyx()[0] // 3)
            elif ch == curses.KEY_NPAGE:
                self.scroll = max(0, self.scroll - max(1, self.scr.getmaxyx()[0] // 3))
            elif ch == curses.KEY_ENTER:
                self.submit()
            return
        if ch in ("\n", "\r"):
            self.submit()
        elif ch in ("\x7f", "\b"):
            self.backspace()
        elif ch == "\x1b":
            self.scr.nodelay(True)
            try:
                nxt = self.scr.get_wch()
            except curses.error:
                nxt = None
            finally:
                self.scr.timeout(200)
            if isinstance(nxt, str) and nxt in MODE_KEYS:
                self.set_mode(MODE_KEYS[nxt])
            elif nxt is None:
                self.inp, self.cur = "", 0
        elif ch == "\x15":  # Ctrl+U
            self.inp, self.cur = "", 0
        elif ch == "\x0c":  # Ctrl+L
            self.scr.clear()
        elif ch == "\x01":
            self.cur = 0
        elif ch == "\x05":
            self.cur = len(self.inp)
        elif ch.isprintable() and len(self.inp) < MAX_INPUT:
            self.inp = self.inp[: self.cur] + ch + self.inp[self.cur:]
            self.cur += 1

    def set_mode(self, mode: str) -> None:
        if mode != "notas" and mode not in self.agents:
            self.say(f"{mode} desligado (veja os avisos no início ou hud --check)", "warn")
            return
        self.mode = mode

    def mouse(self) -> None:
        try:
            _, _, _, _, bstate = curses.getmouse()
        except curses.error:
            return
        if bstate & WHEEL_UP:
            self.scroll += WHEEL_STEP
        elif bstate & WHEEL_DOWN:
            self.scroll = max(0, self.scroll - WHEEL_STEP)

    def backspace(self) -> None:
        if self.cur:
            self.inp = self.inp[: self.cur - 1] + self.inp[self.cur:]
            self.cur -= 1

    def hist(self, d: int) -> None:
        if not self.history:
            return
        self.hist_i = max(0, min(len(self.history), self.hist_i + d))
        self.inp = self.history[self.hist_i] if self.hist_i < len(self.history) else ""
        self.cur = len(self.inp)

    # ── ações ────────────────────────────────────────────────────────────
    def run_slot(self, i: int) -> None:
        if 0 <= i < len(self.cfg.commands):
            self.launch(self.cfg.commands[i])

    def launch(self, cmd: Command, confirmed: bool = False) -> None:
        if cmd.confirm and not confirmed:
            self.pending = cmd
            return
        if self.runner.is_running(cmd):
            self.say(f"{cmd.name} ainda está rodando", "dim")
            return
        self.header(f"▶ [{cmd.key}] {cmd.name}")
        self.say("$ " + " ".join([os.path.basename(cmd.argv[0]), *cmd.argv[1:]]), "dim")
        self.scroll = 0
        self.runner.start(cmd)

    def submit(self) -> None:
        line = clean_line(self.inp).strip()
        self.inp, self.cur = "", 0
        if not line:
            return
        if not self.history or self.history[-1] != line:
            self.history.append(line)
            self.history = self.history[-200:]
        self.hist_i = len(self.history)
        self.scroll = 0
        if self.mode in self.agents and line.startswith("//"):
            self.ask(self.mode, line[1:])
        elif line.startswith("/") or line == "?":
            self.slash(line)
        elif self.mode in self.agents:
            self.ask(self.mode, line)
        else:
            self.note(line)

    def ask(self, name: str, prompt: str) -> None:
        agent = self.agents.get(name)
        if not agent:
            self.say(f"{name} desligado (veja os avisos no início ou hud --check)", "warn")
            return
        if not prompt:
            self.say(f"uso: /{name[0] if name == 'claude' else 'x'} pergunta", "warn")
            return
        if agent.busy:
            self.say(f"o {name} ainda está respondendo · /parar interrompe", "warn")
            return
        cont = "continua a conversa" if agent.session.id else "conversa nova"
        self.out.append(("blank", ""))
        self.out.append((name, f"{time.strftime('%H:%M:%S')}  ✦ {name} · {agent.profile} · {cont}"))
        self.out.append((name, f"você › {prompt}"))
        agent.ask(prompt)

    def current_agent(self, arg: str = ""):
        name = arg if arg in self.agents else self.mode if self.mode in self.agents else (
            "claude" if "claude" in self.agents else next(iter(self.agents), ""))
        return name, self.agents.get(name)

    def slash(self, line: str) -> None:
        verb, _, arg = line.partition(" ")
        verb, arg = verb.lower(), arg.strip()
        if verb in ("/ajuda", "/help", "/h", "?"):
            self.header("ajuda")
            self.say(HELP)
        elif verb in ("/sair", "/q", "/quit"):
            self.quit = True
        elif verb in ("/limpar", "/clear", "/cls"):
            self.out.clear()
        elif verb in ("/r", "/run"):
            self.run_by_name(arg)
        elif verb in ("/ag", "/agenda"):
            self.agenda_add(arg)
        elif verb in ("/ok", "/feito"):
            self.agenda_edit(arg, done=True)
        elif verb in ("/rm", "/apagar"):
            self.agenda_edit(arg, done=False)
        elif verb in ("/b", "/buscar", "/busca"):
            self.search(arg)
        elif verb == "/notas":
            self.show_notes(arg)
        elif verb == "/conflitos":
            snap = self.vault.snapshot
            self.header(f"conflitos de sync ({len(snap.conflicts)})")
            for c in snap.conflicts or ["nenhum"]:
                self.say(c, "warn" if snap.conflicts else "dim")
        elif verb in ("/c", "/claude"):
            self.ask("claude", arg)
        elif verb in ("/x", "/codex"):
            self.ask("codex", arg)
        elif verb == "/novo":
            name, agent = self.current_agent(arg)
            if agent:
                agent.new_session()
                self.say(f"próxima pergunta abre uma conversa nova com o {name}", "dim")
        elif verb == "/parar":
            stopped = [n for n, a in self.agents.items() if (not arg or n == arg) and a.stop()]
            if not stopped:
                self.say("nenhum agente está respondendo agora", "dim")
        elif verb == "/perfil":
            self.set_profile(arg)
        elif verb == "/comandos":
            agent = self.agents.get("claude")
            cmds = agent.session.slash_commands if agent else []
            self.header("comandos / do Claude Code")
            if not cmds:
                self.say("a lista chega com a primeira resposta do Claude (pergunte algo com /c)", "dim")
            else:
                self.say("  ".join("/" + c for c in cmds), "claude")
                self.say("no modo Claude, digite o comando direto; nomes que o HUD usa vão com //", "dim")
        elif verb == "/pasta":
            self.change_folder(arg)
        elif verb == "/vault":
            self.vault.refresh()
            self.say("relendo o Vault…", "dim")
        elif self.mode in self.agents:
            self.ask(self.mode, line)
        else:
            self.say(f"não conheço {verb}. /ajuda lista o que existe.", "warn")

    def change_folder(self, arg: str) -> None:
        if not arg:
            self.say(f"pasta: {self.cfg.vault} ({self.cfg.folder_source}) · uso: /pasta caminho · /pasta vault volta para {self.cfg.default_vault}", "dim")
            return
        back = arg.lower() in ("vault", "padrão", "padrao")
        try:
            target = validate_folder(self.cfg.default_vault if back else arg)
        except ConfigError as e:
            self.say(str(e), "warn")
            return
        busy = [n for n, a in self.agents.items() if a.busy]
        if busy:
            self.say(f"{', '.join(busy)} respondendo · /parar antes de trocar de pasta", "warn")
            return
        self.cfg.vault = target
        self.cfg.folder_source = "config" if back else "escolhida com /pasta"
        self.vault.set_root(target)
        try:
            remember_folder(self.cfg.data_dir, None if back else target)
        except OSError as e:
            self.say(f"não consegui lembrar a pasta para a próxima vez: {e}", "warn")
        moved = []
        for name, agent in self.agents.items():
            if not agent.cfg.follow_folder:
                continue
            extra = {"read_dirs": (str(target),)} if hasattr(agent.cfg, "read_dirs") else {}
            agent.cfg = dataclasses.replace(agent.cfg, cwd=str(target), **extra)
            agent.new_session()
            moved.append(name)
        self.say(f"pasta: {target}" + (" (de volta ao Vault)" if back else " · lembrada para a próxima vez"), "ok")
        if moved:
            self.say(f"{' e '.join(moved)} agora trabalham e leem nesta pasta; conversa nova", "dim")
        if target == Path.home():
            self.say("⚠ é a sua home inteira: varredura pesada e os agentes leem tudo fora da lista de segredos", "warn")

    def set_profile(self, arg: str) -> None:
        parts = arg.split()
        want = next((p for p in parts if p in ("leitura", "completo")), "")
        name, agent = self.current_agent(next((p for p in parts if p in self.agents), ""))
        if not agent:
            self.say("nenhum agente ligado", "warn")
            return
        if not want:
            self.say(f"{name}: perfil {agent.profile} · uso: /perfil leitura|completo [claude|codex]", "dim")
            return
        if agent.busy:
            self.say(f"o {name} está respondendo · /parar antes de trocar o perfil", "warn")
            return
        agent.profile = want
        agent.new_session()
        if want == "completo":
            what = ("ferramentas, MCP, skills e comandos / do seu Claude Code, permission-mode "
                    f"{agent.cfg.full_permission_mode}" if name == "claude"
                    else "sandbox e aprovações do seu ~/.codex/config.toml")
            self.say(f"⚠ {name} no perfil completo: {what}. Nova conversa.", "warn")
        else:
            self.say(f"{name} no perfil leitura. Nova conversa.", name)

    def run_by_name(self, arg: str) -> None:
        if not arg:
            self.say("uso: /r N ou /r parte-do-nome", "warn")
            return
        cmds = self.cfg.commands
        match = [c for c in cmds if c.key == arg] or \
                [c for c in cmds if arg.lower() in c.name.lower()]
        if len(match) == 1:
            self.launch(match[0])
        elif not match:
            self.say(f"nenhum comando com “{arg}”", "warn")
        else:
            self.say("mais de um: " + ", ".join(f"[{c.key}] {c.name}" for c in match), "warn")

    def agenda_add(self, arg: str) -> None:
        try:
            item = ag.parse_entry(arg, dt.date.today())
        except ValueError as e:
            self.say(str(e), "warn")
            return
        try:
            self.agenda.add(item)
        except OSError as e:
            self.say(f"não consegui gravar a agenda: {e}", "crit")
            return
        when = ag.day_label(item.date, dt.date.today()) + (f" {item.time}" if item.time else "")
        self.say(f"＋ agenda: {when} — {item.text}", "ok")

    def agenda_edit(self, arg: str, done: bool) -> None:
        if not arg.isdigit() or not 1 <= int(arg) <= len(self.agenda_view):
            self.say("informe o número do item mostrado na agenda", "warn")
            return
        item = self.agenda_view[int(arg) - 1]
        if item.source:
            self.say(f"esse item vem do Vault ({item.source}); o HUD não escreve no Vault", "warn")
            return
        try:
            (self.agenda.mark_done if done else self.agenda.remove)(item)
        except (OSError, ValueError) as e:
            self.say(f"não consegui gravar a agenda: {e}", "crit")
            return
        self.say(f"{'✓ concluído' if done else '－ apagado'}: {item.text}", "ok" if done else "dim")

    def search(self, term: str) -> None:
        if len(term) < 2:
            self.say("uso: /b termo (mínimo 2 letras)", "warn")
            return
        self.header(f"busca no Vault: “{term}”")
        threading.Thread(
            target=lambda: self.events.put(("search", term, self.vault.search(term))),
            daemon=True,
        ).start()

    def note(self, text: str) -> None:
        stamp = time.strftime("%Y-%m-%d %H:%M")
        try:
            fd = os.open(self.notes_path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, "a", encoding="utf-8") as f:
                f.write(f"- {stamp} {text}\n")
        except OSError as e:
            self.say(f"não consegui gravar a nota: {e}", "crit")
            return
        self.out.append(("you", f"{time.strftime('%H:%M')} › {text}"))

    def show_notes(self, arg: str) -> None:
        n = int(arg) if arg.isdigit() else 15
        try:
            lines = self.notes_path.read_text(encoding="utf-8").splitlines()
        except FileNotFoundError:
            lines = []
        self.header(f"últimas notas ({self.notes_path})")
        for line in lines[-n:] or ["nenhuma nota ainda"]:
            self.say(clean_line(line), "text")


def _ago(sec: float) -> str:
    if sec < 60:
        return "agora"
    if sec < 3600:
        return f"{int(sec // 60)}min"
    if sec < 86400:
        return f"{int(sec // 3600)}h"
    return f"{int(sec // 86400)}d"


def main(cfg: Config) -> None:
    os.environ.setdefault("ESCDELAY", "25")
    curses.wrapper(Hud(cfg).run)
