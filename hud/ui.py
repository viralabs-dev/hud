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
from dataclasses import dataclass
from typing import Callable
from pathlib import Path

from . import agenda as ag
from . import plataforma as plat
from .config import Command, Config, ConfigError, remember_folder, validate_folder
from . import config as cfgmod
from .metrics import Metrics, human_bytes, human_duration
from . import custom as cu
from . import comandos as cmdx
from . import docs as dc
from . import markdown as md
from . import monitor as mon
from . import skills as sk
from . import layout as lay
from . import usage as us
from .claude import Claude
from .codex import Codex
from .opencode import Opencode
from .runner import Runner
from .text import clean_line, fit, pad, width, wrap, wrap_hanging, wrap_runs
from .vault import VaultWatcher

SPARK = "▁▂▃▄▅▆▇█"
SPIN = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
MIN_W, MIN_H = 80, 24
MAX_INPUT = 500
# PDCurses (windows-curses) pode não ter as constantes da roda: sem elas, sem roda.
WHEEL_UP = getattr(curses, "BUTTON4_PRESSED", 0)
WHEEL_DOWN = getattr(curses, "BUTTON5_PRESSED", 0x200000)
WHEEL_STEP = 3
PRESS = getattr(curses, "BUTTON1_PRESSED", 0)
RELEASE = getattr(curses, "BUTTON1_RELEASED", 0)
CLICK = PRESS | getattr(curses, "BUTTON1_CLICKED", 0)
# Alt+setas movem o foco entre as caixas (nomes do terminfo para Alt = modificador 3).
FOCUS_KEYS = {"kUP3": "up", "kDN3": "down", "kLFT3": "left", "kRIT3": "right"}
MODE_KEYS = {"1": "notas", "2": "claude", "3": "codex", "4": "opencode"}  # Alt+1/2/3/4
TABS = tuple(MODE_KEYS.values())  # cada modo tem a sua aba na SAÍDA
# PDCurses manda Alt+N como uma tecla só (ALT_1...); o ncurses manda Esc + N.
ALT_KEYS = {getattr(curses, f"ALT_{n}"): m for n, m in MODE_KEYS.items() if hasattr(curses, f"ALT_{n}")}

# Comandos / do HUD para as sugestões da ENTRADA (a ajuda completa está em HELP).
SLASH = [
    ("/ajuda", "tudo o que a entrada aceita"), ("/ag", "marca na agenda: /ag amanhã 14h texto"),
    ("/b", "busca no Vault"), ("/c", "pergunta ao Claude"), ("/comandos", "comandos / do Claude Code"),
    ("/conflitos", "conflitos de sync do Vault"), ("/custom", "lista, usa ou salva customizações"),
    ("/doc", "planeja documentação · /doc salvar grava"), ("/limpar", "limpa a aba aberta"),
    ("/notas", "últimas notas"), ("/novo", "conversa nova com o agente"), ("/o", "pergunta ao OpenCode"),
    ("/ok", "conclui um item da agenda"), ("/parar", "interrompe o agente"),
    ("/pasta", "lê outra pasta · /pasta vault volta"), ("/perfil", "leitura ou completo"),
    ("/project", "lista ou cria projetos"), ("/r", "roda um comando do painel"),
    ("/rm", "apaga um item da agenda"), ("/sair", "fecha o HUD"), ("/skill", "usa uma skill pelo nome"),
    ("/skills", "lista as skills"), ("/vault", "relê a pasta agora"), ("/x", "pergunta ao Codex"),
    ("/agentes", "agentes e subagentes rodando (Alt+5)"), ("/cmd", "comandos do painel e onde editar"),
    ("/proposta", "prévia do que um agente propôs (s aplica)"),
]
SLASH.sort()

HELP = """\
Teclas
  F1–F10 ou /r N · roda o comando do painel
  Alt+1 notas · Alt+2 Claude · Alt+3 Codex · Alt+4 OpenCode · Tab alterna entre eles (ou clique na aba)
  A SAÍDA tem uma aba por modo; ● = aba com saída nova ainda não vista
  Enter · envia a linha · ↑/↓ · histórico · Esc · limpa a linha
  / mostra os comandos na borda da entrada · Tab completa o comando (fora disso, alterna o modo)
Caixas
  Alt+setas · move o foco entre as caixas (ou clique numa caixa) · Alt+Z · ênfase: a caixa em foco na área toda
  arraste o título de uma caixa e solte sobre outra · troca as duas (vira a sua customização, lembrada)
  Alt+5 ou /agentes · os agentes e subagentes rodando (Claude, Codex, OpenCode) · Esc ou Alt+Z volta
  roda do mouse ou PgUp/PgDn · rola a aba aberta · Ctrl+L · redesenha · Ctrl+C · sai
Entrada
  texto livre · vira nota com data e hora (notas.md, fora do Vault)
  /r N ou /r nome · roda um comando do painel
  /ag quando [hora] texto · ex.: /ag amanhã 14h dentista, /ag sex revisar PR, /ag 12/10 9:30 call
  /ok N · conclui o item N da agenda · /rm N · apaga
  /b termo · busca no Vault (nome e conteúdo, só leitura)
  /notas [N] · últimas notas · /conflitos · conflitos de sync do Vault
  /vault · relê a pasta agora · /limpar · limpa a aba aberta · /sair
  /pasta caminho · lê outra pasta local no lugar do Vault (lembrada) · /pasta vault · volta ao Vault
  /custom lista · customizações · /custom nome · usa · /custom padrao · volta · /custom salvar · prévia da proposta (s grava)
  /cmd · comandos do painel e o arquivo deles · /proposta · prévia do que o agente propôs (comandos ou layout); s aplica
Skills e projetos (com o agente do modo; nas notas, o Claude ou o Codex)
  /skills [filtro] · lista as skills do HUD, do Claude Code e do Codex · /skill nome [pedido] · usa uma skill
  /project · lista os projetos da pasta · /project pedido · cria um projeto novo no modelo do Vault, ou consulta
  /doc pedido · planeja e escreve documentação no modelo, ou consulta (skill projeto-docs)
  /doc salvar · grava na pasta os arquivos .md que o agente propôs (pede s para sobrescrever) · /doc descartar
  Os comandos / do HUD valem em qualquer modo (notas, Claude, Codex, OpenCode); só os do Claude vão para ele.
Claude Code (laranja), Codex (cinza) e OpenCode (lilás)
  /c pergunta · ao Claude · /x pergunta · ao Codex · /o pergunta · ao OpenCode (a conversa continua)
  Tab · alterna notas → Claude → Codex → OpenCode: no modo de um agente, texto livre vai para ele
  No modo Claude, comandos / que o HUD não conhece vão para o Claude (/review, skills…);
  //comando força mandar ao agente um nome que o HUD também usa (ex.: //clear)
  /comandos · lista os comandos / do Claude · /novo · nova conversa · /parar · interrompe
  /perfil leitura|completo · troca o perfil do agente do modo atual nesta sessão
    leitura: Claude só lê o Vault, sem MCP · Codex em sandbox read-only · OpenCode sem edição, bash e web
    completo: o mesmo Claude Code / Codex / OpenCode do seu terminal (ferramentas, MCP, skills)
Segurança
  Só rodam os comandos da lista: sem shell, PATH e ambiente fixos, tempo-limite e saída limitada.
  O HUD só escreve na pasta (o Vault ou a de /pasta) com /doc salvar: arquivos .md propostos, sem sair da pasta.
  Texto de fora tem as sequências de escape removidas."""


@dataclass
class Ask:
    """Pergunta s/N na entrada (o `name` vira o título da caixa)."""
    name: str
    yes: Callable[[], None]
    no: Callable[[], None] | None = None


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
        # Uma saída por aba (notas, claude, codex), cada uma com a sua rolagem.
        self.outs: dict[str, deque[tuple[str, str]]] = {t: deque(maxlen=3000) for t in TABS}
        self.scrolls: dict[str, int] = dict.fromkeys(TABS, 0)
        self.unread: set[str] = set()
        self.cmd_tabs: dict[str, str] = {}  # comando do painel → aba onde foi lançado
        self.cmd_started: dict[str, float] = {}  # comando → quando começou (painel COMANDOS)
        self.cmd_last: dict[str, tuple[bool, str, float]] = {}  # comando → (ok, resumo, quando acabou)
        self.search_tab = "notas"
        self.tab_hits: list[tuple[int, int, int, str]] = []  # (linha, x0, x1, aba) para o clique
        self.rects: dict[str, tuple[int, int, int, int]] = {}  # caixas da última tela (y, x, h, w)
        self.current = ""  # caixa sendo desenhada (título do bloco, foco)
        self.focus = ""  # caixa em foco (Alt+setas, clique)
        self.emphasis = False  # a caixa em foco ocupa a área toda (Alt+Z)
        self.drag = ""  # caixa sendo arrastada pelo título
        self.monitor: mon.MonitorWatcher | None = None  # só liga quando o painel AGENTES aparece
        self.cmd_proposal: cmdx.Proposta | None = None  # comandos propostos por um agente
        self.cmd_preview: cmdx.Proposta | None = None   # em prévia no painel COMANDOS, esperando "s"
        self.layout_before: tuple | None = None  # (layout, nome, feed) para voltar se a prévia for recusada
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
        if cfg.opencode:
            self.agents["opencode"] = Opencode(cfg.opencode, self.events)
        self.mode = "notas"  # ou o nome de um agente
        self.usage = us.UsageTracker()
        self.codex_usage = us.CodexUsageTracker()
        self.codex_usage.poll()
        self.custom_root = cfg.custom_dir or cu.custom_root()
        self.layout: lay.Layout = lay.DEFAULT
        self.custom_name = ""
        self.feed: cu.PanelFeed | None = None
        self.proposals: list[cu.Proposal] = []
        self.doc_proposals: dict[str, dc.DocProposal] = {}  # arquivo → proposta (vale a última)
        self.clip: tuple[int, int, int] | None = None  # (y0, y1, x1) do painel em desenho
        self.layout_note = ""
        # Os agentes leem as customizações e as skills do HUD (layouts, modelos de documentação).
        self.agent_extra_reads = tuple(str(p) for p in (self.custom_root, cu.templates_root(), cu.skills_root())
                                       if p and p.is_dir())
        for agent in self.agents.values():
            agent.context = self.agent_context()
            if hasattr(agent.cfg, "read_dirs") and agent.cfg.follow_folder:
                agent.cfg = dataclasses.replace(
                    agent.cfg, read_dirs=tuple(dict.fromkeys((*agent.cfg.read_dirs, *self.agent_extra_reads))))

    def agent_context(self) -> str:
        return (cu.agent_context(self.custom_root) + " Para criar projetos ou documentação na pasta do HUD "
                f"({self.cfg.vault}), siga a skill projeto-docs: dentro do HUD responda com um bloco "
                "````hud-doc arquivo=\"<caminho relativo à pasta>\" por arquivo .md, sem gravar nada, e diga ao "
                "usuário para digitar /doc salvar.")

    # ── saída ────────────────────────────────────────────────────────────
    @property
    def out(self) -> deque[tuple[str, str]]:
        """A saída da aba aberta (a do modo atual)."""
        return self.outs[self.mode]

    @property
    def scroll(self) -> int:
        return self.scrolls[self.mode]

    @scroll.setter
    def scroll(self, value: int) -> None:
        self.scrolls[self.mode] = value

    def tabs(self) -> list[str]:
        return ["notas", *(t for t in TABS if t in self.agents)]

    def say(self, text: str, style: str = "text", tab: str | None = None) -> None:
        tab = tab if tab in self.outs else self.mode
        for line in text.split("\n"):
            self.outs[tab].append((style, line))
        if self.scrolls[tab]:
            self.scrolls[tab] += text.count("\n") + 1
        if tab != self.mode:
            self.unread.add(tab)

    def item(self, head: str, text: str = "", tab: str | None = None, head_style: str = "accent",
             text_style: str = "text", col: int = 18) -> None:
        """Uma linha de lista em duas colunas: `head` à esquerda e `text` quebrando
        dentro da segunda coluna (o recuo acompanha a largura da SAÍDA)."""
        tab = tab if tab in self.outs else self.mode
        self.outs[tab].append(("item", text, head, head_style, col, text_style))
        if self.scrolls[tab]:
            self.scrolls[tab] += 1
        if tab != self.mode:
            self.unread.add(tab)

    def say_md(self, text: str, agent: str) -> None:
        """Resposta de um agente: Markdown em linhas com estilo, na aba dele."""
        tab = agent if agent in self.outs else self.mode
        blocks = md.render(text)
        for b in blocks:
            self.outs[tab].append(("md", b, agent))
        if self.scrolls[tab]:
            self.scrolls[tab] += len(blocks)
        if tab != self.mode:
            self.unread.add(tab)

    def header(self, text: str, tab: str | None = None) -> None:
        tab = tab if tab in self.outs else self.mode
        self.say("", "blank", tab)
        self.say(f"{time.strftime('%H:%M:%S')}  {text}", "head", tab)

    # ── ciclo ────────────────────────────────────────────────────────────
    def run(self, scr) -> None:
        self.scr = scr
        try:
            curses.curs_set(1)
        except curses.error:  # terminal sem cursor configurável
            pass
        scr.keypad(True)
        scr.timeout(200)
        # Só a roda importa. Com o mouse ligado o terminal passa os cliques
        # para o HUD; para selecionar texto, Shift+arrastar.
        try:
            curses.mousemask(WHEEL_UP | WHEEL_DOWN | CLICK | RELEASE)
            curses.mouseinterval(0)
        except (curses.error, AttributeError):  # PDCurses/terminal sem mouse
            pass
        self._colors()
        self.vault.start()
        self.say("HUD pronto. Digite /ajuda para ver tudo o que a entrada aceita.", "dim")
        self.say(f"configuração: {self.cfg.source} · dados: {self.cfg.data_dir}", "dim")
        try:
            name = cu.remembered(self.cfg.data_dir)
        except Exception as e:  # noqa: BLE001 — arquivo estranho não impede a tela
            name = None
            self.say(f"customização lembrada ignorada: {e}", "warn")
        if name:
            self.use_custom(name, startup=True)
        for w in self.cfg.warnings:
            self.say(f"⚠ {w}", "warn")
        last_sample = 0.0
        last_vault = -1
        last_mon = -1
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
                if self.monitor and self.monitor.version != last_mon:
                    last_mon = self.monitor.version
                    dirty = True
                if dirty:
                    self.draw()
                try:
                    ch = scr.get_wch()
                except curses.error:
                    continue
                if ch == curses.KEY_RESIZE and plat.WINDOWS:
                    try:  # PDCurses só atualiza o tamanho da tela com resize_term
                        curses.resize_term(0, 0)
                    except (curses.error, AttributeError):
                        pass
                self.key(ch)
                self.draw()
        except KeyboardInterrupt:
            pass
        finally:
            self.vault.stop()
            if self.feed:
                self.feed.stop()
            for agent in self.agents.values():
                agent.stop()
            if self.monitor:
                self.monitor.stop()

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
                    self.say(line, "text", self.cmd_tabs.get(cmd.key))
            elif kind == "cmd_end":
                _, cmd, rc, dur, timed_out, truncated = ev
                tab = self.cmd_tabs.get(cmd.key)
                self.cmd_last[cmd.key] = (rc == 0 and not timed_out,
                                          "tempo esgotado" if timed_out else f"código {rc}" if rc else "",
                                          time.time())
                extra = " · saída cortada" if truncated else ""
                if timed_out:
                    self.say(f"✗ {cmd.name}: interrompido após {cmd.timeout:.0f}s{extra}", "crit", tab)
                elif rc == 0:
                    self.say(f"✓ {cmd.name} · {dur:.1f}s{extra}", "ok", tab)
                else:
                    self.say(f"✗ {cmd.name} · código {rc} · {dur:.1f}s{extra}", "warn", tab)
            elif kind == "agent_text":
                self.say_md(ev[2], ev[1])
                found = cu.parse_proposals(ev[2])
                if found:
                    self.proposals = found
                    nomes = ", ".join(sorted({f"{p.nome}/{p.arquivo}" for p in found}))
                    self.say(f"✦ proposta de customização: {nomes} · /proposta mostra a prévia", "ok", ev[1])
                cp = cmdx.parse_proposals(ev[2])
                if cp:
                    self.cmd_proposal = cp
                    extra = f" · {len(cp.avisos)} recusado(s)" if cp.avisos else ""
                    self.say(f"✦ proposta de comandos: {len(cp.comandos)} comando(s){extra} · "
                             "/proposta mostra a prévia", "ok", ev[1])
                docs = dc.parse_doc_proposals(ev[2])
                if docs:
                    for d in docs:
                        self.doc_proposals[d.arquivo] = d
                    n = len(self.doc_proposals)
                    self.say(f"✦ proposta: {n} arquivo(s) · /doc salvar · /doc descartar", "ok", ev[1])
                    for arq in list(self.doc_proposals)[:12]:
                        self.say(f"    {arq}", "dim", ev[1])
                    if n > 12:
                        self.say(f"    … e mais {n - 12}", "dim", ev[1])
            elif kind == "agent_tool":
                self.say(f"  ⚙ {ev[2]}", f"{ev[1]}_dim", ev[1])
            elif kind == "agent_denied":
                self.say(f"  ⊘ negado: {ev[2]}", "warn", ev[1])
            elif kind == "usage_event":
                self.usage.offer(us.parse_event(ev[1]))
            elif kind == "agent_end":
                _, name, ok, msg, dur, summary = ev
                tail = (f" · {dur:.0f}s" if dur is not None else "") + (f" · {summary}" if summary else "")
                buf = self.outs.get(name)
                if buf and buf[-1][0] == "md":  # respiro entre a resposta e o fecho
                    self.say("", "blank", name)
                if ok:
                    self.say(f"✓ {name}{tail}", name, name)
                else:
                    self.say(f"✗ {name}: {msg}{tail}", "warn", name)
            elif kind == "search":
                _, term, hits = ev
                tab = self.search_tab
                if not hits:
                    self.say(f"nada encontrado para “{term}”", "dim", tab)
                for rel, n, line in hits:
                    loc = f"{rel}:{n}" if n else rel
                    self.say(loc, "accent", tab)
                    if line:
                        self.say(f"    {line.strip()}", "text", tab)
                if hits:
                    self.say(f"{len(hits)} resultado(s)", "dim", tab)

    # ── cores ────────────────────────────────────────────────────────────
    def _colors(self) -> None:
        self.c: dict[str, int] = {}
        try:
            curses.start_color()
            bg = -1
            try:
                curses.use_default_colors()
            except curses.error:  # sem fundo padrão do terminal: fundo preto
                bg = curses.COLOR_BLACK
            pairs = {"accent": curses.COLOR_CYAN, "ok": curses.COLOR_GREEN,
                     "warn": curses.COLOR_YELLOW, "crit": curses.COLOR_RED,
                     "mag": curses.COLOR_MAGENTA, "blue": curses.COLOR_BLUE}
            # Laranja = Claude, cinza = Codex; em terminal de 8 cores, amarelo e branco.
            rich = curses.COLORS >= 256
            pairs["claude"] = 208 if rich else curses.COLOR_YELLOW
            pairs["codex"] = 248 if rich else curses.COLOR_WHITE
            pairs["codex_dim"] = 242 if rich else curses.COLOR_WHITE
            pairs["opencode"] = 141 if rich else curses.COLOR_MAGENTA  # lilás
            for i, (k, col) in enumerate(pairs.items(), 1):
                curses.init_pair(i, col if col < curses.COLORS else curses.COLOR_WHITE, bg)
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
            "opencode": self.c.get("opencode", 0), "opencode_dim": self.c.get("opencode", 0) | A.A_DIM,
            # Markdown das respostas: código no texto padrão em negrito, para destacar da cor do agente.
            "code": A.A_BOLD, "codeline": A.A_NORMAL,
            "focus": self.c.get("accent", 0) | A.A_BOLD,  # borda da caixa em foco
        }
        italic = getattr(A, "A_ITALIC", A.A_DIM)  # PDCurses pode não ter itálico
        self.style["text_dim"] = A.A_DIM
        for agent in ("text", "claude", "codex", "opencode"):
            base = self.style[agent]
            self.style[f"{agent}_bold"] = base | A.A_BOLD
            self.style[f"{agent}_italic"] = base | italic
            self.style[f"{agent}_head"] = base | A.A_BOLD | A.A_UNDERLINE

    def level(self, pct: float) -> str:
        return "crit" if pct >= 90 else "warn" if pct >= 70 else "ok"

    # ── desenho ──────────────────────────────────────────────────────────
    def put(self, y: int, x: int, s: str, style: str | int = "text", w: int | None = None) -> int:
        H, W = self.scr.getmaxyx()
        if self.clip:
            y0, y1, x1 = self.clip
            if y < y0 or y >= y1:
                return x
            W = min(W, x1)
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

    def shows(self, part: str) -> bool:
        """[[bloco]] mostrar = [...] da caixa sendo desenhada (sem a lista, mostra tudo)."""
        pid = "vault" if self.current == "pasta" else self.current
        blk = self.layout.blocos.get(pid) if pid else None
        return not blk or blk.mostrar is None or part in blk.mostrar

    def box(self, y: int, x: int, h: int, w: int, title: str, right: str = "",
            color: str | None = None) -> None:
        pid = self.current
        blk = self.layout.blocos.get("vault" if pid == "pasta" else pid) if pid else None
        if blk and blk.titulo:  # [[bloco]] titulo = "…"
            title = blk.titulo
        focused = bool(pid) and pid == self.focus
        if focused and self.emphasis:
            right = "Alt+Z volta · " + right if right else "Alt+Z volta"
        b = self.style["focus"] if focused else self.style[color or "border"]
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
        body = H - 3
        cx = self.codex_usage.current
        auto = {"comandos": len(self.cfg.commands) + 2,
                "uso_codex": max(3, len(cx.windows) + 2) if cx and cx.windows else 3}
        self.layout_note = ""
        try:
            rects = lay.compute(self.layout, W, body, auto)
        except lay.TooSmall:
            self.layout_note = f"“{self.custom_name}” não cabe em {W}×{H}: usando o padrão"
            try:
                rects = lay.compute(lay.DEFAULT, W, body, auto)
            except lay.TooSmall:
                rects = {}
        draw = {"sistema": self.draw_system, "comandos": self.draw_commands,
                "agenda": self.draw_agenda, "uso_claude": self.draw_usage,
                "uso_codex": self.draw_usage_codex, "vault": self.draw_vault,
                "saida": self.draw_output, "agentes": self.draw_agents}
        if self.emphasis and self.focus and (self.focus in rects or self.focus in draw):
            rects = {self.focus: (0, 0, body, W)}  # ênfase: só a caixa em foco, na área toda
        elif self.focus not in rects:
            self.focus, self.emphasis = "", False
        self.rects = rects
        for pid, (y, x, h, w) in rects.items():
            self.clip = (y, y + h, x + w)
            self.current = pid
            try:
                if pid in draw:
                    draw[pid](y, x, h, w)
                else:
                    self.draw_custom(pid, y, x, h, w)
            finally:
                self.clip = None
                self.current = ""
        cy, cx = self.draw_input(body, 0, 3, W)
        try:
            scr.move(cy, cx)
        except curses.error:
            pass
        scr.refresh()

    def draw_system(self, y: int, x: int, h: int, w: int) -> None:
        s = self.metrics.last
        self.box(y, x, h, w, "SISTEMA", self.metrics.hostname)
        iw, x0, lab = w - 4, x + 2, 6
        up = f" ligado há {human_duration(s.uptime)} "
        # O relógio encolhe (sem dia da semana, depois só a hora) para não cobrir o "ligado há".
        for fmt in (" %a %d/%m  %H:%M:%S ", " %d/%m %H:%M:%S ", " %H:%M:%S ", " %H:%M "):
            clock = time.strftime(fmt)
            if width(up) + width(clock) + 4 <= w:
                break
        self.put(y + h - 1, x + 2, up, "dim", w - 4 - width(clock))
        self.put(y + h - 1, x + w - 1 - width(clock), clock, "bold")

        mem_pct = 100 * s.mem_used / s.mem_total if s.mem_total else 0
        meters = [("CPU", s.cpu, f"{s.cores} núcleos"),
                  ("MEM", mem_pct, f"{human_bytes(s.mem_used)}/{human_bytes(s.mem_total)}")]
        if s.swap_total:
            meters.append(("SWAP", 100 * s.swap_used / s.swap_total,
                           f"{human_bytes(s.swap_used)}/{human_bytes(s.swap_total)}"))
        disks = [(fit(d.label, lab - 1, False), d.pct, f"{human_bytes(d.total - d.used)} livre") for d in s.disks]
        # Colunas alinhadas: barras da mesma largura, % numa coluna, detalhe à direita.
        tail_w = min(max(width(t) for _, _, t in meters + disks), max(0, iw - lab - 5 - 8))
        bw = max(4, iw - lab - 5 - (tail_w + 1 if tail_w else 0))

        def meter(row: int, name: str, pct: float, tail: str) -> None:
            self.put(row, x0, name, "dim")
            self.bar(row, x0 + lab, bw, pct)
            self.put(row, x0 + lab + bw, f"{pct:4.0f}%", self.level(pct))
            if tail_w:
                t = fit(tail, tail_w)
                self.put(row, x0 + iw - width(t), t, "dim")

        def spark(row: int) -> None:  # histórico da CPU sob a barra, cada ponto na cor do nível
            nx = x0 + lab
            for v in list(self.metrics.cpu_hist)[-bw:]:
                nx = self.put(row, nx, SPARK[min(7, int(v / 100 * 8))], self.level(v))

        def load(row: int) -> None:
            self.put(row, x0, "LOAD", "dim")
            procs = f"{s.procs} proc"
            if not getattr(s, "load_ok", True):  # Windows não tem load average
                self.put(row, x0 + lab, "—", "dim")
            else:
                l1, l5, l15 = s.load
                trend = "↑" if l1 > l5 * 1.1 else "↓" if l1 < l5 * 0.9 else "→"
                nx = self.put(row, x0 + lab, f"{l1:.2f}", self.level(100 * l1 / max(1, s.cores)))
                nx = self.put(row, nx, f" {trend} ", "dim")
                self.put(row, nx, f"{l5:.2f}  {l15:.2f}", "dim", x0 + iw - nx - width(procs) - 1)
            self.put(row, x0 + iw - width(procs), procs, "dim")

        def net(row: int) -> None:
            self.put(row, x0, "REDE", "dim")
            if not getattr(s, "net_ok", True):
                self.put(row, x0 + lab, "—", "dim")
                return
            nx = self.put(row, x0 + lab, f"↓ {human_bytes(s.rx_rate, '/s')}", "accent")
            nx = self.put(row, nx, f"  ↑ {human_bytes(s.tx_rate, '/s')}", "mag")
            room = x0 + iw - nx - 2
            hist = list(self.metrics.net_hist)[-room:] if room >= 6 else []
            top = max(hist, default=0)
            if top > 0:  # tráfego relativo ao pico da janela
                line = "".join(SPARK[min(7, int(v / top * 7.99))] for v in hist)
                self.put(row, x0 + iw - width(line), line, "dim")

        def sensors(row: int) -> None:
            nx = x0
            if s.temp is not None:
                self.put(row, x0, "TEMP", "dim")
                st = "crit" if s.temp >= 85 else "warn" if s.temp >= 70 else "ok"
                nx = self.put(row, x0 + lab, f"{s.temp:.0f}°C", st) + 3
            if s.battery:
                pct, state = s.battery
                status = {"Charging": "carregando", "Discharging": "na bateria", "Full": "cheia",
                          "Not charging": "parada"}.get(state, state.lower())
                if s.temp is None:
                    nx = x0
                nx = self.put(row, nx, "BAT", "dim")
                nx = self.put(row, max(nx + 1, x0 + lab) if s.temp is None else nx + 1, f"{pct}%",
                              "crit" if pct <= 15 and state == "Discharging" else "text")
                self.put(row, nx, f" {status}", "dim", x0 + iw - nx)

        # Linhas por prioridade (1 = sempre); entram as que couberem, na ordem da tela.
        rows: list[tuple[int, object]] = []
        if self.shows("cpu"):
            rows.append((1, lambda r, m=meters[0]: meter(r, *m)))
        if self.shows("historico"):
            rows.append((4, spark))
        if self.shows("mem"):
            rows.append((1, lambda r, m=meters[1]: meter(r, *m)))
        if s.swap_total and self.shows("swap"):
            rows.append((3, lambda r, m=meters[2]: meter(r, *m)))
        if self.shows("disco"):
            for k, d in enumerate(disks[:3]):
                rows.append((2 if k == 0 else 5 + k, lambda r, d=d: meter(r, *d)))
        rows.append((9, lambda r: None))  # respiro entre medidores e texto, se couber
        if self.shows("load"):
            rows.append((2, load))
        if self.shows("rede"):
            rows.append((2, net))
        if (s.temp is not None or s.battery) and self.shows("sensores"):
            rows.append((3, sensors))
        room = h - 2
        keep = sorted(range(len(rows)), key=lambda k: (rows[k][0], k))[:room]
        r = y + 1
        for k in sorted(keep):
            rows[k][1](r)
            r += 1

    def draw_agents(self, y: int, x: int, h: int, w: int) -> None:
        """Os agentes rodando na máquina (processos) e as sessões recentes com os subagentes."""
        if self.monitor is None:
            self.monitor = mon.MonitorWatcher()
            self.monitor.start()
        snap = self.monitor.snapshot
        ativas = sum(1 for se in snap.sessoes if se.ativa)
        self.box(y, x, h, w, "AGENTES",
                 f"{len(snap.processos)} processo(s) · {ativas} sessão(ões) ativa(s)" if snap.lido_em else "lendo…")
        x0, iw, r, end = x + 2, w - 4, y + 1, y + h - 1
        if not snap.lido_em:
            self.put(r, x0, "lendo processos e sessões…", "dim", iw)
            return
        now = time.time()
        home = str(Path.home())

        def short(path: str) -> str:
            path = clean_line(path or "")
            return "~" + path[len(home):] if path.startswith(home) else path

        def color(agente: str) -> str:
            return agente if agente in ("claude", "codex", "opencode") else "text"

        if self.shows("processos") and r < end:
            self.put(r, x0, "PROCESSOS", "head")
            r += 1
            if not snap.processos:
                msg = "sem leitura de processos no Windows" if plat.WINDOWS else "nenhum agente rodando"
                self.put(r, x0, msg, "dim", iw)
                r += 1
            for pr in snap.processos:
                if r >= end:
                    break
                cpu = f"{pr.cpu:.0f}%" if pr.cpu is not None else ""
                age = human_duration(now - pr.desde) if pr.desde else ""
                tail = "  ".join(t for t in (age, cpu) if t)
                nx = self.put(r, x0, "● " if pr.do_hud else "  ", "ok")
                nx = self.put(r, nx, f"{pr.agente:<9}", color(pr.agente))
                nx = self.put(r, nx, f"{pr.modo:<11}", "dim")
                self.put(r, nx, short(pr.cwd), "text", x0 + iw - nx - width(tail) - 1)
                self.put(r, x0 + iw - width(tail), tail, "dim")
                r += 1
            r += 1
        if self.shows("sessoes") and r < end:
            self.put(r, x0, "SESSÕES", "head")
            r += 1
            sessoes = sorted(snap.sessoes, key=lambda se: (not se.ativa, -se.atualizado))
            if not sessoes:
                self.put(r, x0, "nenhuma sessão na última hora", "dim", iw)
            for se in sessoes:
                if r >= end:
                    break
                ago = _ago(now - se.atualizado)
                nx = self.put(r, x0, "● " if se.ativa else "○ ", "ok" if se.ativa else "dim")
                nx = self.put(r, nx, f"{se.agente:<9}", color(se.agente))
                titulo = se.titulo or short(se.pasta)
                self.put(r, nx, titulo, "text" if se.ativa else "dim", x0 + iw - nx - width(ago) - 1)
                self.put(r, x0 + iw - width(ago), ago, "dim")
                r += 1
                # Subagentes: os que rodam e até 2 concluídos de cada sessão.
                shown = [sa for sa in se.subagentes if sa.rodando] + [sa for sa in se.subagentes if not sa.rodando][:2]
                for sa in shown:
                    if r >= end:
                        break
                    mark = SPIN[self.tick % len(SPIN)] if sa.rodando else "✓"
                    nx = self.put(r, x0 + 2, f"↳ {mark} ", "warn" if sa.rodando else "dim")
                    label = sa.descricao + (f" · {sa.tipo}" if sa.tipo else "")
                    self.put(r, nx, label, "text" if sa.rodando else "dim", x0 + iw - nx)
                    r += 1
                extra = len(se.subagentes) - len(shown)
                if extra > 0 and r < end:
                    self.put(r, x0 + 4, f"… mais {extra} subagente(s) concluído(s)", "dim", iw - 4)
                    r += 1

    def draw_commands(self, y: int, x: int, h: int, w: int) -> None:
        if self.cmd_preview:  # prévia: como o painel vai ficar, com o que entra, muda e sai
            self.box(y, x, h, w, "COMANDOS · prévia", "s aplica · outra tecla descarta", "warn")
            marks = cmdx.diff(self.cfg.commands, self.cmd_preview.comandos)
            for i, (mark, text) in enumerate(marks[: h - 2]):
                st = {"+": "ok", "-": "crit", "~": "warn"}.get(mark, "text")
                nx = self.put(y + 1 + i, x + 2, f"{mark} ", st)
                self.put(y + 1 + i, nx, text, st if mark != "=" else "text", w - 6)
            return
        self.box(y, x, h, w, "COMANDOS", "F1–F10 · /r N")
        if not self.cfg.commands:
            self.put(y + 1, x + 2, "nenhum comando permitido", "dim")
            self.put(y + 2, x + 2, "[[command]] no config.toml", "dim", w - 4)
        now = time.time()
        for i, c in enumerate(self.cfg.commands[: h - 2]):
            r, right = y + 1 + i, x + w - 2
            # À direita: rodando (com o tempo) ou o último resultado, com há quanto tempo.
            if self.runner.is_running(c):
                status = (f"{SPIN[self.tick % len(SPIN)]} {int(now - self.cmd_started.get(c.key, now))}s", "warn")
            elif c.key in self.cmd_last:
                ok, why, at = self.cmd_last[c.key]
                status = (f"✓ {_ago(now - at)}" if ok else f"✗ {why} · {_ago(now - at)}", "ok" if ok else "crit")
            else:
                status = ("", "dim")
            nx = self.put(r, x + 2, f"F{i + 1:<3}" if i < 10 else f"{c.key:<4}", "accent")
            room = right - nx - (width(status[0]) + 1 if status[0] else 0) - (2 if c.confirm else 0)
            nx = self.put(r, nx, c.name, "bold" if status[1] == "warn" else "text", room)
            if c.confirm:  # pede s antes de rodar
                self.put(r, nx + 1, "!", "warn")
            if status[0]:
                self.put(r, right - width(status[0]), status[0], status[1])

    def draw_agenda(self, y: int, x: int, h: int, w: int) -> None:
        today = dt.date.today()
        snap = self.vault.snapshot
        items = ag.upcoming(self.agenda.items, snap.tasks, today)
        blk = self.layout.blocos.get("agenda")
        if blk and blk.dias:  # [[bloco]] dias = N: só os próximos N dias (atrasados continuam)
            items = [i for i in items if (i.date - today).days < blk.dias]
        self.agenda_view = items
        late = sum(1 for i in items if i.date < today)
        today_n = sum(1 for i in items if i.date == today)
        self.box(y, x, h, w, "AGENDA", f"{today_n} hoje" + (f" · {late} atrasado(s)" if late else ""))
        iw, r, end = w - 4, y + 1, y + h - 1
        if not items:
            for k, (t, st) in enumerate((("Nada marcado.", "dim"), ("/ag amanhã 14h dentista", "accent"),
                                         ("/ag sex revisar PR", "accent"),
                                         ("tarefas com data no Vault entram aqui", "dim"))):
                if r + k < end:
                    self.put(r + k, x + 2, t, st, iw)
            return
        now_hm = time.strftime("%H:%M")
        # O próximo compromisso de hoje (com hora, ainda não passado) fica em destaque.
        nxt = next((i for i in items if i.date == today and i.time and i.time >= now_hm), None)
        text_x = x + 2 + 9  # "NN HH:MM "
        tw = iw - 9
        # Quebra títulos longos em 2 linhas só se tudo couber assim.
        days = len({i.date for i in items})
        extra = sum(1 for i in items if width(i.text) + 3 > tw)
        wrap2 = len(items) + days + extra <= end - r
        last = None
        for n, it in enumerate(items, 1):
            if r >= end:
                break
            if it.date != last:
                if r >= end - 1:
                    break
                lbl = ag.day_label(it.date, today)
                self.put(r, x + 2, lbl, "crit" if it.date < today else "head" if it.date == today else "dim")
                r += 1
                last = it.date
            past = it.date < today or (it.date == today and it.time is not None and it.time < now_hm)
            style = "warn" if it.date < today else "dim" if past else "bold" if it is nxt else "text"
            nx = self.put(r, x + 2, f"{n:>2} ", "dim")
            self.put(r, nx, f"{it.time or '':5} ", "dim" if past else "accent")
            tail = ""
            if it is nxt:
                hh, mm = map(int, it.time.split(":"))
                mins = hh * 60 + mm - (dt.datetime.now().hour * 60 + dt.datetime.now().minute)
                tail = "agora" if mins <= 0 else f"em {mins}min" if mins < 60 else f"em {mins // 60}h{mins % 60:02d}"
            label = it.text + (" ◆" if it.source else "")
            room = tw - (width(tail) + 1 if tail else 0)
            parts = wrap(label, room) if wrap2 else [label]
            self.put(r, text_x, parts[0], style, room)
            if tail:
                self.put(r, x + 2 + iw - width(tail), tail, "warn")
            r += 1
            if len(parts) > 1 and r < end:
                self.put(r, text_x, " ".join(parts[1:]), style, tw)
                r += 1

    def draw_vault(self, y: int, x: int, h: int, w: int) -> None:
        snap = self.vault.snapshot
        ago = f"lido há {int(time.time() - snap.scanned_at)}s" if snap.scanned_at else "lendo…"
        root = self.cfg.vault
        title = "VAULT · ao vivo" if snap.obsidian or not snap.ok else f"PASTA · {clean_line(root.name)} · ao vivo"
        self.box(y, x, h, w, title, ago)
        x0, iw, r, end = x + 2, w - 4, y + 1, y + h - 1
        home = str(Path.home())
        shown_root = "~" + str(root)[len(home):] if str(root).startswith(home) else str(root)
        self.put(end, x + 2, f" {shown_root} · varredura {snap.scan_ms:.0f}ms ", "dim", iw)
        if not snap.ok:
            self.put(r, x0, snap.error or f"lendo {root}…", "warn" if snap.error else "dim")
            return
        # Resumo: o tamanho da pasta à esquerda; andamento e bloqueados de todos os quadros à direita.
        unit = "notas" if snap.obsidian else "arquivos de texto"
        show_summary = self.shows("resumo")
        if not show_summary:
            r -= 2  # sem a linha de resumo (e o respiro dela)
        nx = self.put(r, x0, f"{snap.notes}{'+' if snap.capped else ''} {unit}", "bold") if show_summary else x0
        nx = self.put(r, nx, f" · {snap.today} hoje · {len(snap.boards)} quadro(s)", "dim")
        doing = sum(b.counts.get("doing", 0) for b in snap.boards)
        blocked = sum(b.counts.get("blocked", 0) for b in snap.boards)
        right = [(f"▶ {doing} em andamento", "warn" if doing else "dim"),
                 ("  ", "dim"), (f"■ {blocked} bloq.", "crit" if blocked else "dim")]
        rw = sum(width(t) for t, _ in right)
        if show_summary and snap.boards and nx + 2 + rw <= x0 + iw:
            rx = x0 + iw - rw
            for t, st in right:
                rx = self.put(r, rx, t, st)
        if snap.conflicts:
            r += 1
            self.put(r, x0, f"⚠ {len(snap.conflicts)} conflito(s) de sync · /conflitos", "warn")
        r += 2
        wide = iw >= 110
        lw = iw // 2 - 1 if wide else iw
        # Uma lista só de atenção: primeiro o que está andando (▶), depois o bloqueado (■).
        items = ([("▶", "warn", b.name, t) for b in snap.boards for t in b.doing]
                 + [("■", "crit", b.name, t) for b in snap.boards for t in b.blocked])
        # Divisão do espaço (empilhado): recentes com 2 ou 3 linhas, a tabela com ~3/5
        # do resto e a lista de atenção com o que sobra.
        space = end - r
        recent_room = (0 if wide or not snap.recent or space < 10
                       else min(len(snap.recent), 3 if space >= 14 else 2) + 2)
        if not self.shows("atencao"):
            items = []
        if not self.shows("recentes"):
            recent_room = 0
        if snap.boards and r < end and self.shows("quadros"):
            limit = len(snap.boards) if wide else max(3, (space - recent_room) * 3 // 5 - 1)
            r = self._vault_boards(r, x0, lw, snap.boards, limit) + 1
        top = r
        lists_end = end if wide else end - recent_room
        if items and lists_end - r >= 2:
            self.put(r, x0, "ATENÇÃO", "head")  # os totais já estão no resumo
            r += 1
            bw = min(max(width(it[2]) for it in items) + 1, max(12, lw // 3))  # até 1/3 da largura
            room = lists_end - r - (1 if lists_end - r >= 4 else 0)
            show = items if len(items) <= room else items[:max(0, room - 1)]
            for mark, color, board, t in show:
                nx = self.put(r, x0, mark + " ", color)
                nx = self.put(r, nx, fit(board, bw - 1).ljust(bw), color)
                self.put(r, nx, t, "text", lw - bw - 2)
                r += 1
            if len(show) < len(items):
                rest = items[len(show):]
                d = sum(1 for it in rest if it[0] == "▶")
                parts = [f"{d} em andamento" if d else "", f"{len(rest) - d} bloqueado(s)" if len(rest) > d else ""]
                self.put(r, x0, "… mais " + " · ".join(p for p in parts if p), "dim")
                r += 1
        # Recentes logo depois da lista, com o espaço que sobrar (ou à direita, em tela larga).
        rx, rr = (x0 + lw + 2, top) if wide else (x0, r + 1 if r > top else r)
        if snap.recent and rr < end and self.shows("recentes"):
            self.put(rr, rx, "RECENTES", "head")
            rr += 1
            now = time.time()
            room_w = iw - (rx - x0)
            for rel, m in snap.recent:
                if rr >= end:
                    break
                p = Path(rel)
                nx = self.put(rr, rx, f"{_ago(now - m):>6}  ", "dim")
                nx = self.put(rr, nx, clean_line(p.stem), "text", room_w - 8)
                folder = str(p.parent)
                if folder != "." and nx + 4 < rx + room_w:
                    self.put(rr, nx, f"  {clean_line(folder)}", "dim", rx + room_w - nx)
                rr += 1

    def _vault_boards(self, r: int, x0: int, w: int, boards, limit: int) -> int:
        """Tabela dos quadros: nome, a fazer, andamento, bloqueados, feitos e uma barra
        do que já foi concluído. Devolve a próxima linha livre."""
        cols = [("todo", "fazer"), ("doing", "andam"), ("blocked", "bloq"), ("done", "feito")]
        longest = max((width(b.name) for b in boards), default=8)
        name_w = max(8, min(26, longest + 2, w - 6 * len(cols)))
        bar_w = max(0, min(12, w - name_w - 6 * len(cols) - 7))  # barra + " 100%"
        self.put(r, x0, pad("QUADRO", name_w), "dim")
        for i, (_, lbl) in enumerate(cols):
            self.put(r, x0 + name_w + i * 6, f"{lbl:>6}", "dim")
        r += 1
        show = boards if len(boards) <= limit else boards[:max(1, limit - 1)]
        for b in show:
            self.put(r, x0, pad(fit(b.name, name_w - 1), name_w), "text")
            for i, (k, _) in enumerate(cols):
                n = b.counts.get(k, 0)
                st = {"doing": "warn", "blocked": "crit"}.get(k, "text") if n else "dim"
                self.put(r, x0 + name_w + i * 6, f"{n if n else '·':>6}", st)
            total = sum(b.counts.get(k, 0) for k, _ in cols)
            if bar_w >= 4 and total:
                frac = b.counts.get("done", 0) / total
                full = round(bar_w * frac)
                bx = self.put(r, x0 + name_w + 6 * len(cols) + 2, "█" * full, "ok")
                bx = self.put(r, bx, "░" * (bar_w - full), "dim")
                self.put(r, bx, f" {frac:>4.0%}", "dim")
            r += 1
        if len(show) < len(boards):
            self.put(r, x0, f"… +{len(boards) - len(show)} quadro(s)", "dim")
            r += 1
        return r

    def output_lines(self, w: int) -> list[list[tuple[str, str]]]:
        """Cada linha da tela é uma lista de trechos (estilo, texto)."""
        lines: list[list[tuple[str, str]]] = []
        for entry in self.out:
            if entry[0] == "md":
                lines += _md_lines(entry[1], entry[2], w)
                continue
            if entry[0] != "item":
                style, text = entry
                lines += [[(style, part)] for part in wrap_hanging(text, w) or [""]]
                continue
            _, text, head, head_style, col, text_style = entry
            col = max(4, min(col, w // 2))
            head = fit(head, col - 1)
            parts = wrap(text, w - col) or [""]
            lines.append([(head_style, head + " " * (col - width(head))), (text_style, parts[0])])
            lines += [[(text_style, " " * col + part)] for part in parts[1:]]
        return lines

    def draw_output(self, y: int, x: int, h: int, w: int) -> None:
        iw, ih = w - 4, h - 2
        lines = self.output_lines(iw)
        max_scroll = max(0, len(lines) - ih)
        self.scroll = min(self.scroll, max_scroll)
        start = len(lines) - ih - self.scroll
        view = lines[max(0, start): max(0, start) + ih]
        self.box(y, x, h, w, "SAÍDA")
        nx = self.draw_tabs(y, x + 2 + width(" SAÍDA ") + 1, x + w - 2)
        right = f"↑ {self.scroll} linhas · PgDn volta" if self.scroll else "roda · PgUp/PgDn"
        if self.emphasis and self.focus == "saida":
            right = "Alt+Z volta · " + right
        room = x + w - 2 - nx - 2
        if room >= 8:
            r = fit(f" {right} ", room)
            self.put(y, x + w - 2 - width(r), r, "dim")
        for i, segs in enumerate(view):
            cx = x + 2
            for style, text in segs:
                cx = self.put(y + 1 + i, cx, text, style)

    def draw_tabs(self, y: int, x: int, end: int) -> int:
        """As abas na borda da SAÍDA: a aberta em destaque, ● nas que têm saída nova."""
        self.tab_hits = []
        num = {m: k for k, m in MODE_KEYS.items()}
        for tab in self.tabs():
            label = f" {num[tab]} {tab.upper()}{' ●' if tab in self.unread else ''} "
            if x + width(label) > end:
                break
            color = {"notas": "accent"}.get(tab, tab)
            if tab == self.mode:
                attr = self.style[color] | curses.A_REVERSE | curses.A_BOLD
            else:
                attr = self.style[color] | (curses.A_BOLD if tab in self.unread else curses.A_DIM)
            self.put(y, x, label, attr)
            self.tab_hits.append((y, x, x + width(label), tab))
            x += width(label) + 1
        return x

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

    def draw_custom(self, pid: str, y: int, x: int, h: int, w: int) -> None:
        spec = self.layout.paineis.get(pid)
        if not spec:
            return
        status = self.feed.status(pid) if self.feed else ""
        self.box(y, x, h, w, spec.titulo, status)
        lines = self.feed.lines(pid) if self.feed else []
        room = h - 2
        if spec.tipo == "texto" and lines:  # .md/.txt da customização, formatado como as respostas
            rendered = [seg for b in md.render("\n".join(lines)) for seg in _md_lines(b, "text", w - 4)]
            for i, segs in enumerate(rendered[:room]):
                cx = x + 2
                for st, t in segs:
                    cx = self.put(y + 1 + i, cx, t, st)
            return
        shown = lines[-room:]
        style = "warn" if status == "não confiado" else "text"
        for i, line in enumerate(shown):
            self.put(y + 1 + i, x + 2, line, style, w - 4)
        if not shown:
            self.put(y + 1, x + 2, "(vazio)" if status != "não confiado" else
                     f"comando não confiado: /custom {self.custom_name} para revisar", "dim", w - 4)

    def apply_layout(self, layout: lay.Layout, name: str, trusted: bool) -> None:
        if self.feed:
            self.feed.stop()
        self.feed = None
        self.layout, self.custom_name = layout, name
        if layout.paineis:
            base = cu.find(self.custom_root, name) if name else None
            self.feed = cu.PanelFeed(layout, base, trusted)
            self.feed.start()

    def use_custom(self, name: str, startup: bool = False) -> None:
        try:
            layout = cu.load_custom(self.custom_root, name)
            dig = cu.digest(self.custom_root, name)
        except (lay.LayoutError, OSError) as e:
            self.say(f"customização “{name}”: {e}", "warn")
            return
        cmds = cu.needs_trust(layout)
        trust = cu.Trust(self.cfg.data_dir)
        for aviso in layout.avisos:
            self.say(f"  aviso: {aviso}", "dim")

        def done(trusted: bool) -> None:
            self.apply_layout(layout, name, trusted)
            try:
                cu.remember(self.cfg.data_dir, name)
            except OSError as e:
                self.say(f"não consegui lembrar a customização: {e}", "warn")
            extra = "" if trusted or not cmds else " · comandos desligados até confiar"
            self.say(f"customização: {name}{extra}", "ok")

        if not cmds or trust.is_trusted(name, dig):
            done(True)
            return
        if startup:
            done(False)
            self.say(f"“{name}” mudou ou ainda não foi confiada: /custom {name} para revisar os comandos", "warn")
            return
        self.header(f"“{name}” quer rodar {len(cmds)} comando(s) a cada intervalo:")
        for argv in cmds:
            self.say("  $ " + " ".join([os.path.basename(argv[0]), *argv[1:]]), "warn")
        self.say("s = confiar e ativar · outra tecla = usar sem os comandos", "dim")

        def yes() -> None:
            try:
                trust.trust(name, dig)
            except OSError as e:
                self.say(f"não consegui gravar a confiança: {e}", "warn")
            done(True)

        self.pending = Ask(f"confiar nos comandos de {name}", yes, lambda: done(False))

    def custom_cmd(self, arg: str) -> None:
        sub = arg.split()[0].lower() if arg else ""
        if not sub:
            atual = self.custom_name or "padrao (embutido)"
            self.say(f"customização: {atual} · pasta: {self.custom_root} · /custom lista · /custom <nome> · /custom padrao · /custom salvar", "dim")
        elif sub == "lista":
            self.header(f"customizações em {self.custom_root}")
            items = cu.list_customs(self.custom_root)
            if not items:
                self.say("nenhuma ainda · peça a um agente: “crie uma customização …” (skill hud-custom)", "dim")
            col = min(22, max((width(n) for n, *_ in items), default=0) + 5)
            for nome, desc, erro, embutido in items:
                mark = "●" if nome == self.custom_name else " "
                desc = f"{desc} · vem com o HUD" if embutido else desc
                self.item(f"{mark} {nome}", f"✗ {erro}" if erro else desc,
                          head_style="warn" if erro else "accent" if mark == "●" else "bold",
                          text_style="warn" if erro else "text", col=col)
        elif sub in ("padrao", "padrão"):
            self.apply_layout(lay.DEFAULT, "", True)
            try:
                cu.remember(self.cfg.data_dir, None)
            except OSError:
                pass
            self.say("customização: padrão embutido", "ok")
        elif sub == "salvar":
            self.preview_layout()
        elif cu.valid_name(sub):
            self.use_custom(sub)
        else:
            self.say(f"nome inválido: {sub} (letras minúsculas, números, - e _)", "warn")

    def preview_proposal(self) -> None:
        if self.cmd_proposal:
            self.preview_commands()
        elif self.proposals:
            self.preview_layout()
        elif self.doc_proposals:
            self.say("a proposta é de documentação: /doc salvar mostra os arquivos e grava", "dim")
        else:
            self.say("nenhuma proposta pendente · peça a um agente (skills hud-comandos, hud-custom)", "dim")

    def preview_commands(self) -> None:
        p = self.cmd_proposal
        self.header(f"prévia dos comandos ({len(p.comandos)}) · {self.cfg.commands_file}")
        for mark, text in cmdx.diff(self.cfg.commands, p.comandos):
            self.say(f"  {mark} {text}", {"+": "ok", "-": "crit", "~": "warn"}.get(mark, "text"))
        for a in p.avisos:
            self.say(f"  ✗ {a}", "warn")
        if p.avisos:
            self.say("a proposta tem comandos recusados: peça ao agente para corrigir", "warn")
            return
        self.cmd_preview = p

        def yes() -> None:
            try:
                cmdx.salvar(self.cfg.commands_file, p.conteudo)
            except (cmdx.CommandError, OSError) as e:
                self.say(f"comandos não gravados: {e}", "warn")
            else:
                self.cfg.commands = list(p.comandos)
                self.cfg.commands_source = "comandos.toml"
                self.cmd_proposal = None
                self.say(f"✓ {len(p.comandos)} comando(s) gravados em {self.cfg.commands_file}", "ok")
            self.cmd_preview = None

        def no() -> None:
            self.cmd_preview = None
            self.say("prévia descartada · a proposta continua em /proposta", "dim")

        self.pending = Ask("aplicar os comandos propostos", yes, no)

    def preview_layout(self) -> None:
        """Mostra o layout proposto na tela; só grava e usa com "s"."""
        if not self.proposals:
            self.say("nenhuma proposta pendente · peça a um agente (skill hud-custom) e depois /proposta", "dim")
            return
        nome = self.proposals[0].nome
        prop = next((p for p in self.proposals if p.arquivo == "layout.toml"), None)
        try:
            preview = lay.parse_text(prop.conteudo, nome, None) if prop else cu.load_custom(self.custom_root, nome)
        except (lay.LayoutError, OSError) as e:
            self.say(f"proposta recusada: {e}", "warn")
            return
        self.layout_before = (self.layout, self.custom_name, self.feed)
        self.feed = None  # na prévia, os painéis próprios ainda não rodam
        self.layout, self.custom_name = preview, nome

        def restore() -> None:
            if self.layout_before:
                self.layout, self.custom_name, self.feed = self.layout_before
                self.layout_before = None

        def yes() -> None:
            restore()
            self.save_proposals(overwrite=True, then_use=True)

        def no() -> None:
            restore()
            self.say("prévia descartada · /proposta mostra de novo", "dim")

        self.say(f"prévia de “{nome}” na tela · s grava e usa · outra tecla volta", "warn")
        self.pending = Ask(f"gravar e usar “{nome}”", yes, no)

    def cmd_cmd(self, arg: str) -> None:
        sub = arg.split()[0].lower() if arg else ""
        if sub in ("recarregar", "reler"):
            try:
                text = cfgmod.read_commands_file(self.cfg.commands_file)
                raw = cfgmod.parse_commands(text)
                avisos: list[str] = []
                cmds = cfgmod.build_commands(raw.get("command", []), avisos)
            except (cfgmod.ConfigError, OSError) as e:
                self.say(f"{self.cfg.commands_file}: {e}", "warn")
                return
            self.cfg.commands, self.cfg.commands_source = cmds, "comandos.toml"
            for a in avisos:
                self.say(f"  aviso: {a}", "warn")
            self.say(f"✓ {len(cmds)} comando(s) relidos de {self.cfg.commands_file}", "ok")
            return
        self.header(f"comandos ({len(self.cfg.commands)}) · fonte: {self.cfg.commands_source}")
        for i, c in enumerate(self.cfg.commands):
            argv = " ".join([os.path.basename(c.argv[0]), *c.argv[1:]])
            self.item(f"F{i + 1}  {c.name}" + (" !" if c.confirm else ""), argv, head_style="accent",
                      text_style="dim", col=26)
        self.say(f"arquivo: {self.cfg.commands_file}", "dim")
        self.say("peça a um agente (skill hud-comandos) · /proposta mostra a prévia · /cmd recarregar relê", "dim")

    def save_proposals(self, overwrite: bool = False, then_use: bool = False) -> None:
        if not self.proposals:
            self.say("nenhuma proposta pendente · peça a um agente (skill hud-custom) e depois /custom salvar", "dim")
            return
        try:
            nome, files = cu.save_proposals(self.custom_root, self.proposals, sobrescrever=overwrite)
        except cu.AlreadyExists:
            nome = self.proposals[0].nome
            self.pending = Ask(f"sobrescrever a customização {nome}", lambda: self.save_proposals(True))
            return
        except (lay.LayoutError, OSError) as e:
            self.say(f"proposta recusada: {e}", "warn")
            return
        self.proposals = []
        self.say(f"✓ customização {nome} gravada ({', '.join(files)}) em {self.custom_root / nome}", "ok")
        if then_use:
            self.use_custom(nome)
        else:
            self.say(f"/custom {nome} para usar", "dim")

    def usage_row(self, r: int, x: int, w: int, label: str, left: float, resets_at) -> None:
        glyph = {"full": "██", "part": "▒▒", "empty": "░░"}
        st = us.level(left)
        nx = self.put(r, x + 2, f"{label:<7}", "dim")
        nx = self.put(r, nx, f"{left:3.0f}% ", st)
        for seg in us.segments(left):
            nx = self.put(r, nx, glyph[seg], st if seg != "empty" else "dim") + 1
        reset = us.until(resets_at)
        if reset:
            # À direita, igual nos dois painéis: quanto falta e quando zera.
            when = time.localtime(resets_at)
            clock = time.strftime("%H:%M" if resets_at - time.time() < 86400 else "%a %H:%M", when)
            t = f"↺ {reset} · {clock}"
            if width(t) > x + w - 3 - nx:
                t = f"↺ {reset}"
            self.put(r, x + w - 2 - width(t), t, "dim", x + w - 2 - nx)

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

    def slash_suggestions(self) -> list[tuple[str, str]]:
        """Comandos / que combinam com o que está digitado (sem espaço ainda)."""
        t = self.inp
        if not t.startswith("/") or t.startswith("//") or " " in t:
            return []
        found = [(c, d) for c, d in SLASH if c.startswith(t.lower())]
        agent = self.agents.get(self.mode)
        if self.mode == "claude" and agent:  # os do Claude Code entram no modo Claude
            names = {c for c, _ in found}
            found += [("/" + c, "Claude Code") for c in agent.session.slash_commands
                      if ("/" + c).startswith(t) and "/" + c not in names]
        return found

    def complete_slash(self) -> bool:
        """Tab com "/…" na entrada: completa até onde todos concordam (ou o único)."""
        found = self.slash_suggestions()
        if not found:
            return False
        names = [c for c, _ in found]
        common = os.path.commonprefix(names)
        self.inp = names[0] + " " if len(names) == 1 else max(common, self.inp, key=len)
        self.cur = len(self.inp)
        return True

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
            hint = "Alt+1 notas · Alt+2/3/4 agentes · /perfil · /novo · /parar"
        else:
            title = "ENTRADA" + "".join(f" · {n} {spin}" for n in busy)
            hint = self.layout_note or "texto = nota · Alt+2 Claude · Alt+3 Codex · Alt+4 OpenCode · /ajuda"
        self.box(y, x, h, w, title, hint, color)
        if agent and agent.profile == "completo":
            self.put(y, x + 4 + width(title) + 2, " ⚠ ferramentas completas ", "warn")
        iw = w - 6
        prompt_x = x + 2
        tx = prompt_x + 2
        self.put(y + 1, prompt_x, "✦" if agent else "›", color or "accent")
        if not self.inp and not self.pending:
            # Linha vazia: o que dá para fazer neste modo, esmaecido.
            names = {"claude": "o Claude", "codex": "o Codex", "opencode": "o OpenCode"}
            ph = (f"pergunte a {names.get(self.mode, self.mode)} · / para comandos · Tab muda de modo"
                  if agent else "escreva uma nota · / para comandos · /ajuda mostra tudo")
            self.put(y + 1, tx, ph, "dim", iw)
        # Janela horizontal que acompanha o cursor, com … onde há texto escondido.
        before = self.inp[: self.cur]
        start = 0
        while width(before[start:]) > iw - 2:
            start += 1
        shown = self.inp[start:]
        cut = width(shown) > iw
        if cut:
            shown = fit(shown, iw - 1, ellipsis=False)
        if start:
            self.put(y + 1, tx - 1, "…", "dim")
        self.put(y + 1, tx, shown, "text")
        if cut:
            self.put(y + 1, tx + width(shown), "…", "dim")
        # Borda de baixo: sugestões de /, posição no histórico ou o tamanho do texto.
        bottom = y + h - 1
        sugg = self.slash_suggestions() if not self.pending else []
        if sugg:
            if len(sugg) == 1:
                cmd, desc = sugg[0]
                nx = self.put(bottom, x + 2, f" {cmd} ", "accent")
                self.put(bottom, nx, f"{desc} · Tab completa ", "dim", x + w - 2 - nx)
            else:
                nx = x + 2
                for k, (cmd, _) in enumerate(sugg):
                    item = f" {cmd} "
                    if nx + width(item) > x + w - 18:
                        self.put(bottom, nx, f" +{len(sugg) - k} ", "dim")
                        break
                    nx = self.put(bottom, nx, item, "accent" if k == 0 else "text")
                self.put(bottom, x + w - 2 - width(" Tab completa "), " Tab completa ", "dim")
        elif self.history and self.hist_i < len(self.history) and self.inp == self.history[self.hist_i]:
            self.put(bottom, x + 2, f" histórico {self.hist_i + 1}/{len(self.history)} · ↑/↓ ", "dim")
        elif width(self.inp) > iw:
            self.put(bottom, x + 2, f" {len(self.inp)} caracteres ", "dim")
        return y + 1, tx + width(self.inp[start: self.cur])

    # ── teclado ──────────────────────────────────────────────────────────
    def key(self, ch) -> None:
        if ch == curses.KEY_MOUSE:
            self.mouse()
            return
        if self.pending:
            if ch == curses.KEY_RESIZE:
                return
            item, self.pending = self.pending, None
            yes = ch in ("s", "S", "y", "Y")
            if isinstance(item, Ask):
                if yes:
                    item.yes()
                elif item.no:
                    item.no()
                else:
                    self.say(f"{item.name}: cancelado", "dim")
            elif yes:
                self.launch(item, confirmed=True)
            else:
                self.say(f"{item.name}: cancelado", "dim")
            return
        if ch == curses.KEY_RESIZE:
            return
        if ch == "\t" and self.complete_slash():
            return
        if ch == "\t":
            modes = ["notas", *self.agents]
            if len(modes) == 1:
                self.say("Claude e Codex desligados (veja os avisos no início ou hud --check)", "warn")
                return
            self.set_mode(modes[(modes.index(self.mode) + 1) % len(modes)])
            return
        if isinstance(ch, int) and ch in ALT_KEYS:
            self.set_mode(ALT_KEYS[ch])
            return
        if isinstance(ch, int):
            try:
                kname = curses.keyname(ch).decode()
            except (curses.error, ValueError, AttributeError):
                kname = ""
            if kname in FOCUS_KEYS:
                self.move_focus(FOCUS_KEYS[kname])
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
            arrows = {curses.KEY_UP: "up", curses.KEY_DOWN: "down", curses.KEY_LEFT: "left",
                      curses.KEY_RIGHT: "right"}
            if isinstance(nxt, str) and nxt in MODE_KEYS:
                self.set_mode(MODE_KEYS[nxt])
            elif nxt == "5":  # Alt+5: os agentes rodando, na área toda
                self.show_agents()
            elif isinstance(nxt, str) and nxt.lower() == "z":  # Alt+Z: ênfase na caixa em foco
                self.toggle_emphasis()
            elif nxt in arrows:  # Alt+seta em terminais que mandam Esc + seta
                self.move_focus(arrows[nxt])
            elif nxt == "[":
                self.raw_mouse()
            elif nxt is None:
                if self.drag:
                    self.drag = ""
                elif not self.inp and self.emphasis:
                    self.emphasis = False
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
        self.unread.discard(mode)

    def click(self, row: int, col: int) -> None:
        """Botão apertado: aba da SAÍDA, foco na caixa e, no título, começo de arraste."""
        for y, x0, x1, tab in self.tab_hits:
            if row == y and x0 <= col < x1:
                self.set_mode(tab)
                return
        pid = lay.panel_at(self.rects, row, col)
        if pid:
            self.focus = pid
            if lay.on_title(self.rects, row, col) == pid and not self.emphasis:
                self.drag = pid

    def release(self, row: int, col: int) -> None:
        """Botão solto: se começou num título e caiu em outro lugar, move a caixa."""
        dragged, self.drag = self.drag, ""
        if not dragged or row < 0:
            return
        try:
            target = lay.drop_target(self.layout, self.rects, row, col, dragged)
        except lay.LayoutError:
            target = None
        if target:
            self.apply_drag(dragged, target)

    def move_focus(self, direction: str) -> None:
        if not self.rects:
            return
        if not self.focus or self.focus not in self.rects:
            self.focus = "saida" if "saida" in self.rects else next(iter(self.rects))
            return
        nxt = lay.neighbor(self.rects, self.focus, direction)
        if nxt:
            self.focus = nxt

    def toggle_emphasis(self) -> None:
        if not self.focus:
            self.focus = "saida"
        self.emphasis = not self.emphasis

    def show_agents(self) -> None:
        self.focus, self.emphasis = "agentes", True

    def apply_drag(self, dragged: str, target: tuple) -> None:
        """Aplica o arraste e guarda o resultado como customização do usuário."""
        try:
            new = (lay.swap(self.layout, dragged, target[1]) if target[0] == "swap"
                   else lay.move(self.layout, dragged, *target[1:]))
        except lay.LayoutError as e:
            self.say(f"não dá para mover {dragged}: {e}", "warn")
            return
        name = self.custom_name or "pessoal"
        new = dataclasses.replace(new, nome=name)
        was_trusted = bool(self.feed and self.feed.trusted)
        try:
            cu.save_proposals(self.custom_root, [cu.Proposal(name, "layout.toml", lay.dumps(new))],
                              sobrescrever=True)
            if was_trusted:  # o arraste não muda os comandos: a confiança continua
                cu.Trust(self.cfg.data_dir).trust(name, cu.digest(self.custom_root, name))
            self.apply_layout(cu.load_custom(self.custom_root, name), name, was_trusted or not self.feed)
            cu.remember(self.cfg.data_dir, name)
        except (lay.LayoutError, OSError) as e:
            self.say(f"layout não gravado: {e}", "warn")
            return
        self.say(f"caixa {dragged} movida · layout “{name}” gravado em {self.custom_root / name} · "
                 "/custom padrao volta ao original", "dim")

    def mouse(self) -> None:
        try:
            _, mx, my, _, bstate = curses.getmouse()
        except curses.error:
            return
        if bstate & PRESS:
            self.click(my, mx)
        elif RELEASE and bstate & RELEASE:
            self.release(my, mx)
        elif bstate & CLICK:
            self.click(my, mx)
            self.release(my, mx)
        elif bstate & WHEEL_UP:
            self.scroll += WHEEL_STEP
        elif bstate & WHEEL_DOWN:
            self.scroll = max(0, self.scroll - WHEEL_STEP)

    def wheel(self, button: int, col: int = -1, row: int = -1) -> None:
        """Botão do protocolo xterm: 64 = roda para cima, 65 = para baixo, 0 = clique esquerdo."""
        if button == 0 and row >= 0:
            self.click(row, col)
        elif button & 64 and not button & 128:
            if button & 3 == 0:
                self.scroll += WHEEL_STEP
            elif button & 3 == 1:
                self.scroll = max(0, self.scroll - WHEEL_STEP)

    def raw_mouse(self) -> None:
        """Roda do mouse que o curses não traduziu em KEY_MOUSE.

        O ncurses do macOS (5.7) não conhece o formato SGR (ESC[<b;x;yM) e devolve
        a sequência crua: ESC, depois "[<64;10;10M". Aqui lemos o resto, sem
        esperar, e tratamos SGR e o formato antigo X10 (ESC[M seguido de 3 bytes).
        Outra sequência ESC[ continua sendo descartada como antes.
        """
        def nxt():
            try:
                c = self.scr.get_wch()
            except curses.error:
                return None
            return c if isinstance(c, str) else None

        self.scr.nodelay(True)
        try:
            c = nxt()
            if c == "<":
                buf = ""
                while len(buf) < 24:
                    c = nxt()
                    if c is None or c in "Mm":
                        break
                    buf += c
                else:
                    return
                parts = buf.split(";")
                full = len(parts) == 3 and all(q.isdigit() for q in parts)
                if c == "M":
                    if full:
                        self.wheel(int(parts[0]), int(parts[1]) - 1, int(parts[2]) - 1)
                    elif parts[0].isdigit():
                        self.wheel(int(parts[0]))
                elif c == "m" and full and int(parts[0]) & 3 == 0 and not int(parts[0]) & 64:
                    self.release(int(parts[2]) - 1, int(parts[1]) - 1)  # botão esquerdo solto
            elif c == "M":
                b, cx, cy = nxt(), nxt(), nxt()  # botão, coluna e linha (+32)
                if b and ord(b) - 32 == 3:  # X10: soltar (sem dizer qual botão)
                    self.release(ord(cy) - 33 if cy else -1, ord(cx) - 33 if cx else -1)
                elif b:
                    self.wheel(ord(b) - 32, ord(cx) - 33 if cx else -1, ord(cy) - 33 if cy else -1)
            elif c is not None:
                try:
                    curses.unget_wch(c)
                except (curses.error, AttributeError):
                    pass
        finally:
            self.scr.timeout(200)

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
        self.cmd_tabs[cmd.key] = self.mode
        self.cmd_started[cmd.key] = time.time()
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

    def ask(self, name: str, prompt: str, shown: str | None = None) -> None:
        agent = self.agents.get(name)
        if not agent:
            self.say(f"{name} desligado (veja os avisos no início ou hud --check)", "warn")
            return
        if not prompt:
            self.say(f"uso: /{ {'claude': 'c', 'codex': 'x', 'opencode': 'o'}.get(name, name)} pergunta", "warn")
            return
        if agent.busy:
            self.say(f"o {name} ainda está respondendo · /parar interrompe", "warn")
            return
        cont = "continua a conversa" if agent.session.id else "conversa nova"
        self.scrolls[name] = 0
        self.say("", "blank", name)
        self.say(f"{time.strftime('%H:%M:%S')}  ✦ {name} · {agent.profile} · {cont}", name, name)
        self.say(f"você › {shown or prompt}", name, name)
        if name != self.mode:
            n = next(k for k, m in MODE_KEYS.items() if m == name)
            self.say(f"→ pergunta enviada ao {name}: a resposta sai na aba {name.upper()} (Alt+{n})", "dim")
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
            for line in HELP.split("\n"):  # títulos de seção em destaque
                self.say(line, "text" if line.startswith(" ") else "bold")
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
        elif verb in ("/o", "/opencode"):
            self.ask("opencode", arg)
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
                self.say("  " + "  ".join("/" + c for c in cmds), "claude")
                self.say("no modo Claude, digite o comando direto; nomes que o HUD usa vão com //", "dim")
        elif verb == "/proposta":
            self.preview_proposal()
        elif verb == "/cmd":
            self.cmd_cmd(arg)
        elif verb == "/agentes":
            self.show_agents()
        elif verb == "/skills":
            self.list_skills(arg)
        elif verb == "/skill":
            self.use_skill(arg)
        elif verb in ("/project", "/projeto", "/projetos"):
            self.project_cmd(arg)
        elif verb in ("/doc", "/docs"):
            self.doc_cmd(arg)
        elif verb == "/custom":
            self.custom_cmd(arg)
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
            extra = ({"read_dirs": (str(target), *self.agent_extra_reads)}
                     if hasattr(agent.cfg, "read_dirs") else {})
            agent.cfg = dataclasses.replace(agent.cfg, cwd=str(target), **extra)
            agent.context = self.agent_context()
            agent.new_session()
            moved.append(name)
        self.say(f"pasta: {target}" + (" (de volta ao Vault)" if back else " · lembrada para a próxima vez"), "ok")
        if moved:
            self.say(f"{' e '.join(moved)} agora trabalham e leem nesta pasta; conversa nova", "dim")
        if self.doc_proposals:
            self.doc_proposals = {}
            self.say("propostas de documentação descartadas (eram para a pasta anterior)", "dim")
        if target == Path.home():
            self.say("⚠ é a sua home inteira: varredura pesada e os agentes leem tudo fora da lista de segredos", "warn")

    # ── skills, projetos e documentação ──────────────────────────────────
    def skills(self) -> list[sk.Skill]:
        return sk.discover(sk.skill_dirs(cu.skills_root()))

    def doc_agent(self) -> str:
        """O agente do modo; nas notas, o Claude (ou o Codex, se só ele estiver ligado)."""
        if self.mode in self.agents:
            return self.mode
        return "claude" if "claude" in self.agents else next(iter(self.agents), "")

    def list_skills(self, filtro: str = "") -> None:
        items = [s for s in self.skills() if filtro.lower() in f"{s.nome} {s.descricao}".lower()]
        self.header(f"skills ({len(items)})" + (f" com “{filtro}”" if filtro else ""))
        if not items:
            self.say("nenhuma · as skills ficam em ~/.claude/skills, ~/.codex/skills e nas do HUD", "dim")
        col = min(26, max((width(s.nome) for s in items), default=0) + 3)
        for s in items:
            # nome · de onde vem; a descrição embaixo, recuada.
            self.item(s.nome, " · ".join(s.origens), head_style="bold" if "hud" in s.origens else "accent",
                      text_style="dim", col=col)
            if s.descricao:
                self.say("    " + s.descricao, "text")
        if items:
            self.say("/skill nome [pedido] · usa com o agente do modo", "dim")

    def use_skill(self, arg: str, intencao: str = "", shown: str = "") -> None:
        nome, _, pedido = arg.partition(" ")
        if not nome:
            self.say("uso: /skill nome [pedido] · /skills lista as skills", "warn")
            return
        name = self.doc_agent()
        if not name:
            self.say("Claude e Codex desligados: as skills precisam de um agente (veja hud --check)", "warn")
            return
        skill = sk.find(self.skills(), nome)
        if not skill:
            self.say(f"não achei a skill “{nome}” · /skills lista as que existem", "warn")
            return
        try:
            texto = sk.read_skill(skill)
        except (sk.SkillError, OSError) as e:
            self.say(f"skill {skill.nome}: {e}", "warn")
            return
        if not self.agents[name].busy:
            self.doc_proposals = {}  # uma resposta nova traz a proposta inteira
        self.ask(name, sk.skill_prompt(skill, texto, pedido.strip(), intencao),
                 shown or f"/skill {skill.nome} {pedido.strip()}".rstrip())

    def projects_summary(self) -> tuple[list[dc.Project], str]:
        try:
            projs = dc.find_projects(self.cfg.vault)
        except OSError:
            projs = []
        lista = "; ".join(f"{p.nome} ({p.pasta})" for p in projs[:60]) or "nenhum"
        return projs, lista

    def project_cmd(self, arg: str) -> None:
        projs, lista = self.projects_summary()
        if not arg or arg.lower() in ("lista", "listar"):
            self.header(f"projetos em {self.cfg.vault} ({len(projs)})")
            col = min(30, max((width(p.nome) for p in projs), default=0) + 6)
            boards = {b.rel: b for b in self.vault.snapshot.boards}
            area = None
            for p in sorted(projs, key=lambda p: (p.pasta.rpartition("/")[0].lower(), p.nome.lower())):
                pai = p.pasta.rpartition("/")[0]
                if pai != area:  # agrupa pela área (a pasta de cima)
                    area = pai
                    self.say(f"  {pai or '.'}/", "dim")
                b = boards.get(p.kanban or "") or next(  # quadro em outra subpasta do projeto
                    (b for rel, b in boards.items() if rel.startswith(p.pasta + "/")), None)
                if b:  # o Kanban do projeto, como no painel do Vault
                    c = b.counts
                    resumo = (f"{c.get('todo', 0)} a fazer · {c.get('doing', 0)} andam. · "
                              f"{c.get('blocked', 0)} bloq. · {c.get('done', 0)} feito")
                else:
                    resumo = "Kanban ainda não lido" if p.kanban else "sem Kanban"
                self.item("    " + p.nome, resumo, text_style="dim" if not b else "text", col=col)
            if not projs:
                self.say("nenhum projeto no modelo (pasta com NN-backlog/Kanban (Nome).md ou Nome.md)", "dim")
            self.say("/project pedido · cria ou consulta com o agente (projeto-docs)", "dim")
            return
        self.use_skill(f"projeto-docs {arg}", (
            f"Intenção: /project. Se o pedido for para criar um projeto, monte o projeto completo no modelo, na "
            f"pasta do HUD ({self.cfg.vault}), ao lado dos projetos que já existem; se for para listar ou ler, só "
            f"responda, sem blocos de arquivo. Projetos encontrados pelo HUD: {lista}."), f"/project {arg}")

    def doc_cmd(self, arg: str) -> None:
        sub = arg.split()[0].lower() if arg else ""
        if not sub:
            n = len(self.doc_proposals)
            self.say(f"uso: /doc pedido · /doc salvar · /doc descartar"
                     + (f" · {n} arquivo(s) proposto(s) esperando" if n else ""), "dim")
        elif sub == "salvar":
            self.save_docs()
        elif sub in ("descartar", "limpar"):
            self.doc_proposals = {}
            self.say("propostas de documentação descartadas", "dim")
        else:
            _, lista = self.projects_summary()
            self.use_skill(f"projeto-docs {arg}", (
                f"Intenção: /doc. Se o pedido for para documentar, planeje primeiro (o que criar ou atualizar e por "
                f"quê) e depois mande os arquivos no modelo, na pasta do HUD ({self.cfg.vault}); se for para listar "
                f"ou ler, só responda, sem blocos de arquivo. Projetos encontrados pelo HUD: {lista}."), f"/doc {arg}")

    def save_docs(self, overwrite: bool = False) -> None:
        props = list(self.doc_proposals.values())
        if not props:
            self.say("nenhuma proposta de documentação · peça com /doc ou /project e depois /doc salvar", "dim")
            return
        root = self.cfg.vault
        try:
            if not overwrite:
                _, existentes = dc.plan(root, props)
                if existentes:
                    self.header(f"{len(existentes)} arquivo(s) já existem em {root}:")
                    for e in existentes[:20]:
                        self.say(f"  {e}", "warn")
                    if len(existentes) > 20:
                        self.say(f"  … e mais {len(existentes) - 20}", "warn")
                    self.pending = Ask(f"sobrescrever {len(existentes)} arquivo(s)", lambda: self.save_docs(True))
                    return
            gravados = dc.save_docs(root, props, sobrescrever=overwrite)
        except (dc.DocError, OSError) as e:
            self.say(f"documentação não gravada: {e}", "warn")
            return
        self.doc_proposals = {}
        self.header(f"✓ {len(gravados)} arquivo(s) gravado(s) em {root}")
        for g in gravados:
            self.say(f"  {g}", "ok")

    def set_profile(self, arg: str) -> None:
        parts = arg.split()
        want = next((p for p in parts if p in ("leitura", "completo")), "")
        name, agent = self.current_agent(next((p for p in parts if p in self.agents), ""))
        if not agent:
            self.say("nenhum agente ligado", "warn")
            return
        if not want:
            self.say(f"{name}: perfil {agent.profile} · uso: /perfil leitura|completo [claude|codex|opencode]", "dim")
            return
        if agent.busy:
            self.say(f"o {name} está respondendo · /parar antes de trocar o perfil", "warn")
            return
        agent.profile = want
        agent.new_session()
        if want == "completo":
            what = {"claude": "ferramentas, MCP, skills e comandos / do seu Claude Code, permission-mode "
                              f"{getattr(agent.cfg, 'full_permission_mode', '')}",
                    "codex": "sandbox, aprovações e MCP do seu ~/.codex/config.toml",
                    "opencode": "permissões do seu opencode.json"}.get(name, "configuração do seu terminal")
            self.say(f"⚠ {name} no perfil completo: {what}. Nova conversa.", "warn")
        else:
            self.say(f"{name} no perfil leitura. Nova conversa.", name)
        # O que o perfil leitura NÃO garante (docs/privacidade-agentes.md).
        limite = {"codex": "o sandbox read-only não prende a leitura: o Codex lê o disco todo",
                  "opencode": "lê a pasta toda fora dos segredos por nome",
                  "claude": "lê só as pastas permitidas, com segredos negados"}.get(name)
        if limite:
            self.say(f"  {limite}; o que ele lê vai para o provedor do modelo · docs/privacidade-agentes.md", "dim")

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
        self.search_tab = self.mode
        self.header(f"busca no Vault: “{term}”")
        threading.Thread(
            target=lambda: self.events.put(("search", term, self.vault.search(term))),
            daemon=True,
        ).start()

    def note(self, text: str) -> None:
        stamp = time.strftime("%Y-%m-%d %H:%M")
        try:
            why = plat.private_location_error(self.notes_path)
            if why:
                raise PermissionError(why)
            fd = plat.open_nofollow(self.notes_path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
            with os.fdopen(fd, "a", encoding="utf-8") as f:
                f.write(f"- {stamp} {text}\n")
        except OSError as e:
            self.say(f"não consegui gravar a nota: {e}", "crit")
            return
        self.out.append(("you", f"{time.strftime('%H:%M')} › {text}"))

    def show_notes(self, arg: str) -> None:
        n = int(arg) if arg.isdigit() else 15
        try:
            if plat.private_location_error(self.notes_path) or (plat.WINDOWS and plat.is_link(self.notes_path)):
                raise FileNotFoundError(self.notes_path)  # Windows: fora do perfil ou link
            lines = self.notes_path.read_text(encoding="utf-8").splitlines()
        except FileNotFoundError:
            lines = []
        self.header(f"últimas notas ({self.notes_path})")
        for line in lines[-n:] or ["nenhuma nota ainda"]:
            self.say(clean_line(line), "text")


# Markdown → estilos da tela: a cor é a do agente; ênfase e títulos por atributo.
def _md_style(kind: str, agent: str) -> str:
    return {"text": agent, "bold": f"{agent}_bold", "italic": f"{agent}_italic",
            "head": f"{agent}_head", "quote": f"{agent}_dim", "dim": "dim",
            "code": "code", "codeline": "codeline"}.get(kind, agent)


def _md_lines(block, agent: str, w: int) -> list[list[tuple[str, str]]]:
    def styled(runs):
        return [(_md_style(k, agent), t) for k, t in runs]

    if isinstance(block, md.Line):
        if not block.runs:
            return [[]]
        return wrap_runs(styled(block.runs), w, block.hang) or [[]]
    # Tabela: colunas alinhadas se couber; senão, cada linha vira "a · b · c".
    rows = [[md.plain(c) for c in row] for row in block.rows]
    ncol = max(len(r) for r in rows)
    widths = [max((width(r[i]) if i < len(r) else 0) for r in rows) for i in range(ncol)]
    if sum(widths) + 3 * (ncol - 1) <= w:
        out = []
        for n, row in enumerate(rows):
            cells = [pad(row[i] if i < len(row) else "", widths[i]) for i in range(ncol)]
            style = f"{agent}_bold" if n == 0 and block.header else agent
            segs: list[tuple[str, str]] = []
            for i, c in enumerate(cells):
                if i:
                    segs.append(("dim", " │ "))
                segs.append((style, c))
            out.append(segs)
            if n == 0 and block.header:
                out.append([("dim", "─┼─".join("─" * x for x in widths))])
        return out
    out = []
    for n, row in enumerate(block.rows):
        runs: list[tuple[str, str]] = [("text", "• ")]
        for i, cell in enumerate(row):
            if i:
                runs.append(("dim", " · "))
            runs += [("bold" if n == 0 and block.header else k, t) for k, t in cell]
        out += wrap_runs(styled(runs), w, 2)
    return out


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
