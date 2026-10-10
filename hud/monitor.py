"""Monitor de agentes: processos e sessões do Claude Code, Codex e OpenCode.

Só leitura, nada sai da máquina. Duas fontes:

Processos
- Linux: /proc/<pid>/ (cmdline, stat, readlink de cwd), só os processos do
  próprio usuário; não segue mais nada.
- macOS: `/bin/ps -Ao pid=,ppid=,uid=,etime=,pcpu=,args=` com argv fixo, PATH
  fixo e tempo-limite (como o `pmset` em metrics_macos). O cwd fica "" (ler
  exigiria lsof ou libproc, caros demais a cada 3 s).
- Windows: devolve [] por enquanto. O `tasklist` não traz a linha de comando, e
  sem ela não dá para separar `node … claude-code/cli.js` de outro node.
O agente sai do executável (`claude`, `codex`, `opencode`, também como script
de `node`/`bun`); o app de desktop (claude-desktop, ChatGPT) não conta.
`do_hud` marca os descendentes do processo do HUD.

Sessões (só arquivos com mtime dentro da janela, só o fim de cada arquivo)
- Claude Code: $CLAUDE_CONFIG_DIR ou ~/.claude → projects/<pasta>/<id>.jsonl.
  Subagentes: `tool_use` Agent/Task na conversa principal e os arquivos de
  projects/<pasta>/<id>/subagents/agent-*.meta.json (descrição e tipo). Um
  subagente acaba no `tool_result` do mesmo id (o lançado em segundo plano
  devolve `async_launched` e acaba na `<task-notification>` com `<status>`).
- Codex: $CODEX_HOME ou ~/.codex → sessions/AAAA/MM/DD/rollout-*.jsonl. O
  `session_meta` (1ª linha) dá id e cwd; o título vem do session_index.jsonl
  (`thread_name`). Subagentes são rollouts filhos (`source.subagent`,
  `parent_thread_id`, `agent_nickname`, `agent_role`) e os eventos
  `SubAgentActivity` (started/completed) do pai.
- OpenCode: $XDG_DATA_HOME/opencode ou ~/.local/share/opencode → opencode.db
  (SQLite, aberto só para leitura; só as tabelas `session` e `message`, nunca
  conta nem credencial). Subagentes são as sessões filhas (`parent_id`). O
  formato antigo (storage/session/*/*.json) também é lido.

Do conteúdo, só sai o título curto da sessão e a descrição do subagente, os
dois limpos por `clean_line` e cortados. Pedido, resposta e saída de
ferramenta nunca são guardados. Arquivo ilegível ou JSON quebrado é ignorado;
nada aqui levanta exceção para quem chama.
"""

import datetime as dt
import json
import os
import re
import stat
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import plataforma as plat
from .text import clean_line

AGENTES = ("claude", "codex", "opencode")
TAIL = 256 * 1024          # bytes lidos do fim de cada conversa
HEAD = 64 * 1024           # 1ª linha do rollout do Codex (session_meta)
META_MAX = 16 * 1024       # .meta.json de subagente
MAX_FILES = 200            # arquivos abertos por varredura
MAX_ENTRIES = 5000         # entradas de pasta olhadas por fonte
MAX_SESSIONS = 60
MAX_SUBS = 20              # subagentes por sessão
SUB_STALE_S = 180          # subagente sem atividade há mais que isso não está rodando
CODEX_DAYS = 7             # pastas AAAA/MM/DD do Codex olhadas
TITLE_MAX = 80
DESC_MAX = 120
PS = "/bin/ps"
_PS_ENV = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LC_ALL": "C"}
_DONE = re.compile(r"<tool-use-id>([^<]{1,200})</tool-use-id>")
_STATUS = re.compile(r"<status>([^<]{1,40})</status>")
_NOTIF_BLOCK = re.compile(r"<task-notification>.{0,4000}?</task-notification>", re.S)
_SUFFIX = re.compile(r"\s*\(@([\w.-]{1,60}) subagent\)\s*$")


@dataclass(frozen=True)
class AgentProc:
    agente: str          # "claude" | "codex" | "opencode"
    pid: int
    cwd: str             # "" se não der para ler
    desde: float         # epoch do início do processo (0 se não souber)
    cpu: float | None    # % recente (ou média da vida do processo na 1ª leitura)
    modo: str            # "headless" | "interativo" | "servidor"
    do_hud: bool         # descendente deste processo


@dataclass(frozen=True)
class SubAgent:
    descricao: str
    tipo: str
    rodando: bool
    desde: float


@dataclass(frozen=True)
class Sessao:
    agente: str
    id: str
    pasta: str
    atualizado: float
    titulo: str
    subagentes: tuple[SubAgent, ...]
    ativa: bool


@dataclass
class Monitor:
    processos: list[AgentProc] = field(default_factory=list)
    sessoes: list[Sessao] = field(default_factory=list)
    lido_em: float = 0.0
    erro: str = ""
    scan_ms: float = 0.0


def _short(s, n: int) -> str:
    if not isinstance(s, str):
        return ""
    s = " ".join(clean_line(s).split())
    return s if len(s) <= n else s[:n - 1].rstrip() + "…"


# ===================================================================== processos

@dataclass
class RawProc:
    pid: int
    ppid: int
    argv: list[str]
    cwd: str = ""
    desde: float = 0.0
    cpu_s: float | None = None   # tempo de CPU acumulado (s), se der
    pcpu: float | None = None    # % pronto (ps do macOS)


_WRAPPERS = {"node", "nodejs", "bun", "deno"}
_SERVER = {"app-server", "exec-server", "mcp-server", "mcp", "serve", "server", "proto", "acp"}


def _base(p: str) -> str:
    name = re.split(r"[\\/]", p.strip())[-1].lower()
    for ext in (".exe", ".cmd", ".js", ".mjs", ".cjs"):
        if name.endswith(ext):
            name = name[:-len(ext)]
    return name


def identify(argv: list[str]) -> tuple[str, int] | None:
    """(agente, índice do primeiro argumento do agente) ou None."""
    if not argv or not argv[0]:
        return None
    name = _base(argv[0])
    if name in AGENTES:
        return name, 1
    if name in _WRAPPERS:
        for i, a in enumerate(argv[1:5], 1):
            if a.startswith("-"):
                continue
            norm = a.replace("\\", "/").lower()
            if re.search(r"claude-code/cli\.m?js$", norm) or _base(a) == "claude":
                return "claude", i + 1
            if _base(a) in ("codex", "opencode"):
                return _base(a), i + 1
            return None
    return None


def mode_of(agente: str, args: list[str]) -> str:
    words = [a for a in args if not a.startswith("-")]
    flags = set(a.split("=", 1)[0] for a in args if a.startswith("-"))
    if any(w in _SERVER for w in words[:3]) or "--server" in flags:
        return "servidor"
    if agente == "claude" and flags & {"-p", "--print"}:
        return "headless"
    if agente == "codex" and words[:1] and words[0] in ("exec", "e"):
        return "headless"
    if agente == "opencode" and words[:1] and words[0] == "run":
        return "headless"
    return "interativo"


def classify(raws: list[RawProc], self_pid: int | None = None,
             cpu_prev: dict | None = None, now: float | None = None) -> list[AgentProc]:
    """Escolhe, entre os processos crus, os agentes. `cpu_prev` guarda a
    leitura anterior (pid → (cpu_s, instante)) para o % recente."""
    now = time.time() if now is None else now
    by_pid = {r.pid: r for r in raws}
    found: dict[int, tuple[str, int]] = {}
    for r in raws:
        got = identify(r.argv)
        if got:
            found[r.pid] = got

    def ancestors(pid: int):
        seen = set()
        cur = by_pid.get(pid)
        while cur and cur.ppid and cur.ppid not in seen and len(seen) < 64:
            seen.add(cur.ppid)
            yield cur.ppid
            cur = by_pid.get(cur.ppid)

    out = []
    alive = set()
    for pid, (agente, idx) in found.items():
        r = by_pid[pid]
        parent = found.get(r.ppid)
        # `node …/codex.js` abre o binário nativo do codex: conta uma vez só.
        if parent and parent[0] == agente and _base(by_pid[r.ppid].argv[0]) in _WRAPPERS:
            continue
        cpu = r.pcpu
        if r.cpu_s is not None:
            prev = cpu_prev.get(pid) if cpu_prev is not None else None
            if prev and now > prev[1] and r.cpu_s >= prev[0]:
                cpu = (r.cpu_s - prev[0]) / (now - prev[1]) * 100
            elif r.desde and now > r.desde:
                cpu = r.cpu_s / (now - r.desde) * 100
            if cpu_prev is not None:
                cpu_prev[pid] = (r.cpu_s, now)
                alive.add(pid)
        if cpu is not None:
            cpu = round(max(0.0, cpu), 1)
        do_hud = self_pid is not None and self_pid in ancestors(pid)
        out.append(AgentProc(agente, pid, r.cwd, r.desde, cpu,
                             mode_of(agente, r.argv[idx:]), do_hud))
    if cpu_prev is not None:
        for gone in set(cpu_prev) - alive:
            del cpu_prev[gone]
    out.sort(key=lambda p: (not p.do_hud, p.agente, -p.desde, p.pid))
    return out


def _read_bytes(path: str, limit: int = 65536) -> bytes:
    try:
        with open(path, "rb") as f:
            return f.read(limit)
    except OSError:
        return b""


def linux_procs(root: str = "/proc", uid: int | None = None) -> list[RawProc]:
    uid = os.getuid() if uid is None and hasattr(os, "getuid") else uid
    try:
        tck = os.sysconf("SC_CLK_TCK")
    except (ValueError, OSError, AttributeError):
        tck = 100
    btime = 0.0
    for line in _read_bytes(os.path.join(root, "stat"), 1 << 20).decode(errors="replace").splitlines():
        if line.startswith("btime "):
            try:
                btime = float(line.split()[1])
            except (ValueError, IndexError):
                pass
    out = []
    try:
        names = os.listdir(root)
    except OSError:
        return out
    for name in names:
        if not name.isdigit():
            continue
        base = os.path.join(root, name)
        try:
            if uid is not None and os.stat(base).st_uid != uid:
                continue
        except OSError:
            continue
        raw = _read_bytes(os.path.join(base, "stat"), 4096).decode(errors="replace")
        rp = raw.rfind(")")
        fields = raw[rp + 2:].split() if rp > 0 else []
        if len(fields) < 20:
            continue
        try:
            ppid = int(fields[1])
            cpu_s = (int(fields[11]) + int(fields[12])) / tck
            start = btime + int(fields[19]) / tck if btime else 0.0
        except ValueError:
            continue
        argv = [a.decode(errors="replace") for a in
                _read_bytes(os.path.join(base, "cmdline")).split(b"\0") if a]
        if not argv:
            continue
        rp = RawProc(int(name), ppid, argv, desde=start, cpu_s=cpu_s)
        if identify(argv):
            try:
                rp.cwd = os.readlink(os.path.join(base, "cwd"))
            except OSError:
                rp.cwd = ""
        out.append(rp)
    return out


def parse_etime(s: str) -> float | None:
    """[[dd-]hh:]mm:ss do ps → segundos."""
    days = 0
    if "-" in s:
        d, _, s = s.partition("-")
        if not d.isdigit():
            return None
        days = int(d)
    parts = s.split(":")
    if not 1 <= len(parts) <= 3 or not all(p.isdigit() for p in parts):
        return None
    secs = 0
    for p in parts:
        secs = secs * 60 + int(p)
    return days * 86400 + secs


def parse_ps(text: str, uid: int | None = None, now: float | None = None) -> list[RawProc]:
    now = time.time() if now is None else now
    out = []
    for line in text.splitlines():
        f = line.split(None, 5)
        if len(f) < 6:
            continue
        try:
            pid, ppid, puid = int(f[0]), int(f[1]), int(f[2])
            pcpu = float(f[4])
        except ValueError:
            continue
        if uid is not None and puid != uid:
            continue
        el = parse_etime(f[3])
        out.append(RawProc(pid, ppid, f[5].split(), "", now - el if el is not None else 0.0,
                           None, pcpu))
    return out


def macos_procs() -> list[RawProc]:
    try:
        r = subprocess.run([PS, "-Ao", "pid=,ppid=,uid=,etime=,pcpu=,args="],
                           stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, timeout=2.0, env=_PS_ENV,
                           text=True, errors="replace", check=False)
    except (OSError, subprocess.SubprocessError, ValueError):
        return []
    if r.returncode != 0:
        return []
    return parse_ps(r.stdout, os.getuid())


def scan_processes(self_pid: int | None = None, cpu_prev: dict | None = None) -> list[AgentProc]:
    """Agentes rodando agora. Windows: [] (ver o cabeçalho)."""
    try:
        if sys.platform.startswith("linux"):
            raws = linux_procs()
        elif sys.platform == "darwin":
            raws = macos_procs()
        else:
            return []
        return classify(raws, os.getpid() if self_pid is None else self_pid, cpu_prev)
    except Exception:  # a tela nunca cai por causa do monitor
        return []


# ===================================================================== arquivos

class _Budget:
    def __init__(self, n: int | None = None):
        self.left = MAX_FILES if n is None else n

    def take(self) -> bool:
        if self.left <= 0:
            return False
        self.left -= 1
        return True


def _scan(path: str):
    """Entradas de uma pasta sem seguir link: [(nome, caminho, é_pasta, mtime, size)]."""
    out = []
    try:
        with os.scandir(path) as it:
            for i, e in enumerate(it):
                if i >= MAX_ENTRIES:
                    break
                try:
                    st = e.stat(follow_symlinks=False)
                except OSError:
                    continue
                if stat.S_ISLNK(st.st_mode) or plat.is_reparse(st):
                    continue
                out.append((e.name, e.path, stat.S_ISDIR(st.st_mode), st.st_mtime, st.st_size))
    except OSError:
        pass
    return out


def _read_part(path: str, budget: _Budget, tail: int | None = None, head: int | None = None):
    """(linhas, mtime) do fim (`tail`) ou do começo (`head`) de um arquivo seguro."""
    if not budget.take():
        return None
    try:
        fd = plat.open_nofollow(path, os.O_RDONLY)
    except OSError:
        return None
    try:
        with os.fdopen(fd, "rb") as f:
            st = os.fstat(f.fileno())
            if not stat.S_ISREG(st.st_mode) or plat.foreign(st, path):
                return None
            if head is not None:
                data = f.read(head)
                lines = data.split(b"\n")
                if len(data) >= head:
                    lines = lines[:-1]  # última cortada no meio
                return lines, st.st_mtime
            start = max(0, st.st_size - (tail or TAIL))
            f.seek(start)
            data = f.read(tail or TAIL)
    except (OSError, ValueError):
        return None
    lines = data.split(b"\n")
    if start > 0:
        lines = lines[1:]  # primeira cortada no meio
    return lines, st.st_mtime


def _records(lines):
    for line in lines:
        line = line.strip()
        if not line or not line.startswith(b"{"):
            continue
        try:
            o = json.loads(line)
        except ValueError:
            continue
        if isinstance(o, dict):
            yield o


def _iso(ts) -> float:
    if not isinstance(ts, str) or not ts:
        return 0.0
    try:
        d = dt.datetime.fromisoformat(ts.replace("Z", "+00:00"))
    except ValueError:
        return 0.0
    if d.tzinfo is None:
        d = d.replace(tzinfo=dt.timezone.utc)
    return d.timestamp()


def _home(env) -> Path:
    h = env.get("HOME") or env.get("USERPROFILE")
    return Path(h) if h else Path.home()


def _user_text(content) -> str:
    """Texto digitado pelo usuário (não injetado pelo sistema), ou ""."""
    if isinstance(content, str):
        texts = [content]
    elif isinstance(content, list):
        texts = [c.get("text") for c in content
                 if isinstance(c, dict) and c.get("type") in ("text", "input_text")]
    else:
        return ""
    for t in texts:
        if isinstance(t, str):
            s = t.strip()
            if s and not s.startswith(("<", "#")):
                return s
    return ""


# ===================================================================== Claude

def _claude_session(path: str, mtime: float, budget: _Budget, now: float,
                    janela_s: float, ativa_s: float, stale_s: float, cache: dict):
    key = ("claude", path)
    sub_dir = path[:-len(".jsonl")]
    sub_files = []
    for name, p, is_dir, mt, _size in _scan(os.path.join(sub_dir, "subagents")):
        if not is_dir:
            sub_files.append((name, p, mt))
        elif name == "workflows":  # subagents/workflows/<wf>/agent-*.{jsonl,meta.json}
            for _n, wp, wdir, _m, _s in _scan(p)[:50]:
                if wdir:
                    sub_files += [(n, q, m) for n, q, isd, m, _ in _scan(wp) if not isd]
    last_sub = max((m for n, _p, m in sub_files if n.endswith(".jsonl")), default=0.0)
    updated = max(mtime, last_sub)
    if now - updated > janela_s:
        return None
    sig = (mtime, tuple(sorted((n, m) for n, _p, m in sub_files)))
    hit = cache.get(key)
    if hit and hit[0] == sig:
        parsed = hit[1]
    else:
        parsed = _parse_claude(path, sub_files, budget, now, janela_s)
        if parsed is None:
            return None
        cache[key] = (sig, parsed)
    sid, cwd, title, launched, finished, metas, sub_mtimes = parsed
    subs = []
    seen = set()
    for tid, (desc, tipo, since) in list(launched.items()) + [
            (k, v) for k, v in metas.items() if k not in launched]:
        if tid in seen:
            continue
        seen.add(tid)
        act = max(since, sub_mtimes.get(tid, 0.0))
        if now - act > janela_s and tid not in metas:
            continue
        if tid in metas:
            m = metas[tid]
            desc = desc or m[0]
            tipo = tipo or m[1]
            since = since or m[2]
        running = tid not in finished and now - act <= stale_s
        if desc:
            subs.append(SubAgent(desc, tipo, running, since))
    subs = _order_subs(subs, now, janela_s)
    return Sessao("claude", sid, cwd, updated, title, subs, now - updated <= ativa_s)


def _order_subs(subs: list[SubAgent], now: float, janela_s: float) -> tuple[SubAgent, ...]:
    subs = [s for s in subs if s.rodando or not s.desde or now - s.desde <= janela_s]
    subs.sort(key=lambda s: (not s.rodando, -s.desde))
    return tuple(subs[:MAX_SUBS])


def _parse_claude(path, sub_files, budget, now, janela_s):
    got = _read_part(path, budget, tail=TAIL)
    if got is None:
        return None
    lines, _mt = got
    sid = os.path.basename(path)[:-len(".jsonl")]
    cwd = ""
    titles = {}
    first_user = ""
    launched: dict[str, tuple[str, str, float]] = {}
    finished: set[str] = set()
    # O aviso de fim de um subagente pode chegar em qualquer registro (fila, mensagem
    # do usuário ou dentro do resultado de uma ferramenta): procura no texto cru.
    for line in lines:
        if b"<task-notification" in line:
            for blk in _NOTIF_BLOCK.finditer(line.decode("utf-8", "replace")):
                _notif(blk.group(0), finished)
    for o in _records(lines):
        t = o.get("type")
        if not cwd and isinstance(o.get("cwd"), str) and not o.get("isSidechain"):
            cwd = o["cwd"]
        if isinstance(o.get("sessionId"), str) and o["sessionId"]:
            sid = o["sessionId"]
        if t in ("custom-title", "ai-title", "summary"):
            v = o.get({"custom-title": "customTitle", "ai-title": "aiTitle"}.get(t, "summary"))
            if isinstance(v, str) and v.strip():
                titles[t] = v
            continue
        if t == "queue-operation":
            _notif(o.get("content"), finished)
            continue
        msg = o.get("message")
        if not isinstance(msg, dict):
            continue
        content = msg.get("content")
        if t == "user":
            if isinstance(content, str):
                _notif(content, finished)
            if not first_user and not o.get("isMeta") and not o.get("isSidechain") \
                    and not o.get("isCompactSummary"):
                first_user = _user_text(content)
            if isinstance(content, list):
                tur = o.get("toolUseResult")
                async_ = isinstance(tur, dict) and tur.get("status") == "async_launched"
                for c in content:
                    if isinstance(c, dict) and c.get("type") == "tool_result" and not async_:
                        tid = c.get("tool_use_id")
                        if isinstance(tid, str):
                            finished.add(tid)
        elif t == "assistant" and isinstance(content, list) and not o.get("isSidechain"):
            for c in content:
                if (isinstance(c, dict) and c.get("type") == "tool_use"
                        and c.get("name") in ("Task", "Agent") and isinstance(c.get("id"), str)):
                    inp = c.get("input") if isinstance(c.get("input"), dict) else {}
                    launched[c["id"]] = (_short(inp.get("description"), DESC_MAX),
                                         _short(inp.get("subagent_type"), 40),
                                         _iso(o.get("timestamp")))
    title = next((titles[k] for k in ("custom-title", "ai-title", "summary") if k in titles),
                 first_user)
    metas: dict[str, tuple[str, str, float]] = {}
    sub_mtimes: dict[str, float] = {}
    jsonl_mt = {n[:-len(".jsonl")]: m for n, _p, m in sub_files if n.endswith(".jsonl")}
    recent = sorted((m for m in sub_files if m[0].endswith(".meta.json")),
                    key=lambda m: -jsonl_mt.get(m[0][:-len(".meta.json")], m[2]))
    for name, p, mt in recent[:MAX_SUBS * 2]:
        act = jsonl_mt.get(name[:-len(".meta.json")], mt)
        if now - act > janela_s:
            continue
        got = _read_part(p, budget, head=META_MAX)
        if got is None:
            continue
        try:
            d = json.loads(b"\n".join(got[0]))
        except ValueError:
            continue
        if not isinstance(d, dict):
            continue
        tid = d.get("toolUseId") if isinstance(d.get("toolUseId"), str) else "meta:" + name
        metas[tid] = (_short(d.get("description"), DESC_MAX), _short(d.get("agentType"), 40), mt)
        sub_mtimes[tid] = act
    return sid, cwd, _short(title, TITLE_MAX), launched, finished, metas, sub_mtimes


def _notif(content, finished: set) -> None:
    if not isinstance(content, str) or "<task-notification" not in content:
        return
    tid, st = _DONE.search(content), _STATUS.search(content)
    if tid and st and st.group(1).strip() not in ("running", "started", "pending"):
        finished.add(tid.group(1).strip())


def claude_sessions(root: Path, now, janela_s, ativa_s, stale_s, budget, cache) -> list[Sessao]:
    projects = str(root / "projects")
    files = []
    for _n, proj, is_dir, _m, _s in _scan(projects):
        if not is_dir:
            continue
        for name, p, isd, mt, _size in _scan(proj):
            if not isd and name.endswith(".jsonl"):
                files.append((mt, p))
    out = []
    # A conversa principal pode estar parada enquanto um subagente trabalha:
    # olha as que têm a pasta de subagentes mexida na janela também.
    files.sort(reverse=True)
    for mt, p in files:
        if now - mt > janela_s:
            try:
                st = os.lstat(os.path.join(p[:-len(".jsonl")], "subagents"))
            except OSError:
                continue
            if now - st.st_mtime > janela_s:
                continue
        s = _claude_session(p, mt, budget, now, janela_s, ativa_s, stale_s, cache)
        if s:
            out.append(s)
        if len(out) >= MAX_SESSIONS:
            break
    return out


# ===================================================================== Codex

def _dated_dirs(root: str, n: int) -> list[str]:
    def sub(p, w):
        return sorted((name for name, _q, d, _m, _s in _scan(p)
                       if d and len(name) == w and name.isdigit()), reverse=True)
    out = []
    for y in sub(root, 4):
        for m in sub(os.path.join(root, y), 2):
            for d in sub(os.path.join(root, y, m), 2):
                out.append(os.path.join(root, y, m, d))
                if len(out) >= n:
                    return out
    return out


def _parse_codex(path, budget):
    head = _read_part(path, budget, head=HEAD)
    meta = {}
    if head:
        for o in _records(head[0][:1]):
            if o.get("type") == "session_meta" and isinstance(o.get("payload"), dict):
                meta = o["payload"]
    got = _read_part(path, budget, tail=TAIL)
    if got is None:
        return None
    cwd = meta.get("cwd") if isinstance(meta.get("cwd"), str) else ""
    first_user = ""
    turn = ""          # último evento de turno: "started" | "done"
    acts: dict[str, list] = {}   # thread id → [descrição, rodando, desde]
    for o in _records(got[0]):
        pl = o.get("payload")
        if not isinstance(pl, dict):
            continue
        t, pt = o.get("type"), pl.get("type")
        if t == "turn_context" and not cwd and isinstance(pl.get("cwd"), str):
            cwd = pl["cwd"]
        if t != "event_msg":
            continue
        if pt == "task_started":
            turn = "started"
        elif pt in ("task_complete", "turn_aborted"):
            turn = "done"
        elif pt == "thread_settings_applied" and not cwd:
            ts = pl.get("thread_settings")
            if isinstance(ts, dict) and isinstance(ts.get("cwd"), str):
                cwd = ts["cwd"]
        elif pt == "user_message" and not first_user:
            first_user = _user_text(pl.get("message"))
        elif pt == "item_completed" and isinstance(pl.get("item"), dict):
            it = pl["item"]
            if it.get("type") == "UserMessage" and not first_user:
                first_user = _user_text(it.get("content"))
            elif it.get("type") == "SubAgentActivity" and isinstance(it.get("agent_thread_id"), str):
                a = acts.setdefault(it["agent_thread_id"], ["", False, _iso(o.get("timestamp"))])
                path_ = it.get("agent_path")
                if isinstance(path_, str) and path_:
                    a[0] = path_.rstrip("/").rsplit("/", 1)[-1]
                if it.get("kind") == "started":
                    a[1] = True
                elif it.get("kind") == "completed":
                    a[1] = False
    src = meta.get("source")
    child = None
    if isinstance(src, dict) and "subagent" in src:
        sp = src["subagent"]
        spawn = sp.get("thread_spawn") if isinstance(sp, dict) else None
        spawn = spawn if isinstance(spawn, dict) else {}
        parent = meta.get("parent_thread_id") or spawn.get("parent_thread_id")
        if isinstance(parent, str) and parent:
            name = meta.get("agent_nickname") or spawn.get("agent_nickname")
            path_ = meta.get("agent_path") or spawn.get("agent_path")
            if not name and isinstance(path_, str):
                name = path_.rstrip("/").rsplit("/", 1)[-1]
            if not name and isinstance(sp, dict) and isinstance(sp.get("other"), str):
                name = sp["other"]
            if not name and isinstance(sp, str):
                name = sp
            role = meta.get("agent_role") or spawn.get("agent_role")
            child = (parent, _short(name, DESC_MAX) or "subagente", _short(role, 40))
    sid = meta.get("id") if isinstance(meta.get("id"), str) else ""
    if not sid:
        m = re.search(r"([0-9a-f]{8}-[0-9a-f-]{27})\.jsonl$", path)
        sid = m.group(1) if m else os.path.basename(path)
    return (sid, cwd, _short(first_user, TITLE_MAX), turn == "started",
            _iso(meta.get("timestamp")), child,
            {k: (_short(v[0], DESC_MAX), v[1], v[2]) for k, v in acts.items()})


def _codex_titles(root: Path, budget) -> dict[str, str]:
    got = _read_part(str(root / "session_index.jsonl"), budget, tail=TAIL)
    out = {}
    for o in _records(got[0] if got else []):
        if isinstance(o.get("id"), str) and isinstance(o.get("thread_name"), str):
            out[o["id"]] = o["thread_name"]
    return out


def codex_sessions(root: Path, now, janela_s, ativa_s, stale_s, budget, cache) -> list[Sessao]:
    files = []
    for d in _dated_dirs(str(root / "sessions"), CODEX_DAYS):
        for name, p, isd, mt, _size in _scan(d):
            if (not isd and name.startswith("rollout-") and name.endswith(".jsonl")
                    and now - mt <= janela_s):
                files.append((mt, p))
    if not files:
        return []
    files.sort(reverse=True)
    titles = _codex_titles(root, budget)
    parsed = []
    for mt, p in files[:MAX_SESSIONS * 2]:
        hit = cache.get(("codex", p))
        if hit and hit[0] == mt:
            info = hit[1]
        else:
            info = _parse_codex(p, budget)
            if info is None:
                continue
            cache[("codex", p)] = (mt, info)
        parsed.append((mt, info))
    tops: dict[str, list] = {}
    children: dict[str, list[SubAgent]] = {}
    for mt, (sid, cwd, title, turn_on, since, child, acts) in parsed:
        if child:
            running = turn_on and now - mt <= stale_s
            children.setdefault(child[0], []).append(
                SubAgent(child[1], child[2], running, since or mt))
            children.setdefault("ids:" + child[0], []).append(sid)
            continue
        tops[sid] = [mt, cwd, titles.get(sid) or title, acts]
    out = []
    for sid, (mt, cwd, title, acts) in tops.items():
        subs = list(children.get(sid, []))
        known = set(children.get("ids:" + sid, []))
        for tid, (desc, running, since) in acts.items():
            if tid not in known and desc:
                subs.append(SubAgent(desc, "", running and now - mt <= stale_s, since))
        out.append(Sessao("codex", sid, cwd, mt, _short(title, TITLE_MAX),
                          _order_subs(subs, now, janela_s), now - mt <= ativa_s))
    # Filho cujo pai está fora da janela: aparece sozinho.
    for mt, (sid, cwd, title, _t, _s, child, _a) in parsed:
        if child and child[0] not in tops:
            out.append(Sessao("codex", sid, cwd, mt, _short(titles.get(sid) or title or child[1],
                                                            TITLE_MAX), (), now - mt <= ativa_s))
    return out


# ===================================================================== OpenCode

def _oc_desc(title: str, agent) -> tuple[str, str]:
    m = _SUFFIX.search(title or "")
    tipo = m.group(1) if m else (agent if isinstance(agent, str) else "")
    return _short(_SUFFIX.sub("", title or ""), DESC_MAX), _short(tipo, 40)


def _oc_rows_db(db: str, now: float, janela_s: float, budget) -> list[dict] | None:
    try:
        import sqlite3
    except ImportError:
        return None
    try:
        st = os.lstat(db)
    except OSError:
        return None
    if not stat.S_ISREG(st.st_mode) or plat.foreign(st, db) or not budget.take():
        return None
    since_ms = int((now - janela_s) * 1000)
    try:
        uri = Path(db).resolve().as_uri() + "?mode=ro"
        con = sqlite3.connect(uri, uri=True, timeout=0.5)
    except (sqlite3.Error, OSError, ValueError):
        return None
    rows = []
    try:
        cols = {r[1] for r in con.execute("PRAGMA table_info(session)")}
        need = {"id", "parent_id", "directory", "title", "time_created", "time_updated"}
        if not need <= cols:
            return []
        extra = ", agent" if "agent" in cols else ", NULL"
        arch = " AND time_archived IS NULL" if "time_archived" in cols else ""
        cur = con.execute(
            "SELECT id, parent_id, directory, title, time_created, time_updated" + extra +
            " FROM session WHERE time_updated >= ?" + arch +
            " ORDER BY time_updated DESC LIMIT ?", (since_ms, MAX_SESSIONS * 4))
        for sid, parent, d, title, tc, tu, agent in cur.fetchall():
            rows.append({"id": sid, "parent": parent, "dir": d, "title": title,
                         "created": (tc or 0) / 1000, "updated": (tu or 0) / 1000,
                         "agent": agent, "open": None})
        has_msg = con.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='message'"
                              ).fetchone()
        for r in rows:
            if not r["parent"] or not has_msg:
                continue
            m = con.execute("SELECT data FROM message WHERE session_id = ? "
                            "ORDER BY time_created DESC LIMIT 1", (r["id"],)).fetchone()
            r["open"] = _oc_open(m[0] if m else None)
    except sqlite3.Error:
        return rows
    finally:
        con.close()
    return rows


def _oc_open(data) -> bool | None:
    """Última mensagem do assistente ainda sem `time.completed`?"""
    try:
        d = json.loads(data) if isinstance(data, (str, bytes)) else data
    except ValueError:
        return None
    if not isinstance(d, dict) or not isinstance(d.get("time"), dict):
        return None
    if d.get("role") != "assistant":
        return True  # o pedido chegou e a resposta ainda não
    return not d["time"].get("completed")


def _oc_rows_storage(base: str, now: float, janela_s: float, budget) -> list[dict]:
    rows = []
    for _n, proj, isd, _m, _s in _scan(os.path.join(base, "session")):
        if not isd:
            continue
        for name, p, d, mt, _size in _scan(proj):
            if d or not name.endswith(".json") or now - mt > janela_s:
                continue
            got = _read_part(p, budget, head=META_MAX)
            if not got:
                continue
            try:
                o = json.loads(b"\n".join(got[0]))
            except ValueError:
                continue
            if not isinstance(o, dict) or not isinstance(o.get("id"), str):
                continue
            tm = o.get("time") if isinstance(o.get("time"), dict) else {}

            def ms(v):
                return v / 1000 if isinstance(v, (int, float)) and not isinstance(v, bool) else 0.0
            rows.append({"id": o["id"], "parent": o.get("parentID"),
                         "dir": o.get("directory") if isinstance(o.get("directory"), str) else "",
                         "title": o.get("title") if isinstance(o.get("title"), str) else "",
                         "created": ms(tm.get("created")), "updated": ms(tm.get("updated")) or mt,
                         "agent": None, "open": None})
    return rows


def opencode_sessions(base: Path, now, janela_s, ativa_s, stale_s, budget) -> list[Sessao]:
    rows = _oc_rows_db(str(base / "opencode.db"), now, janela_s, budget)
    if not rows:
        rows = _oc_rows_storage(str(base / "storage"), now, janela_s, budget)
    kids: dict[str, list[SubAgent]] = {}
    tops = []
    for r in rows:
        if r["parent"]:
            desc, tipo = _oc_desc(r["title"], r["agent"])
            recent = now - r["updated"] <= stale_s
            running = recent and (r["open"] if r["open"] is not None else now - r["updated"] <= ativa_s)
            kids.setdefault(r["parent"], []).append(SubAgent(desc or "subagente", tipo, bool(running),
                                                             r["created"]))
        else:
            tops.append(r)
    out = []
    ids = {r["id"] for r in tops}
    for r in tops:
        out.append(Sessao("opencode", r["id"], _short(r["dir"], 4096), r["updated"],
                          _short(r["title"], TITLE_MAX), _order_subs(kids.get(r["id"], []), now, janela_s),
                          now - r["updated"] <= ativa_s))
    for r in rows:  # filho cujo pai saiu da janela
        if r["parent"] and r["parent"] not in ids:
            out.append(Sessao("opencode", r["id"], _short(r["dir"], 4096), r["updated"],
                              _short(r["title"], TITLE_MAX), (), now - r["updated"] <= ativa_s))
    return out


# ===================================================================== juntando

def agent_roots(env=None) -> dict[str, Path]:
    env = os.environ if env is None else env
    home = _home(env)
    xdg = env.get("XDG_DATA_HOME")
    return {
        "claude": Path(env["CLAUDE_CONFIG_DIR"]) if env.get("CLAUDE_CONFIG_DIR") else home / ".claude",
        "codex": Path(env["CODEX_HOME"]) if env.get("CODEX_HOME") else home / ".codex",
        "opencode": (Path(xdg) if xdg else home / ".local" / "share") / "opencode",
    }


def scan_sessions(now: float | None = None, janela_s: float = 3600, ativa_s: float = 180,
                  env=None, cache: dict | None = None, errors: list | None = None) -> list[Sessao]:
    """Sessões com atividade na janela, mais recentes primeiro. `cache` (opcional,
    guardado por quem chama) evita reler arquivo que não mudou."""
    now = time.time() if now is None else now
    cache = {} if cache is None else cache
    budget = _Budget()
    stale = max(ativa_s, SUB_STALE_S)
    roots = agent_roots(env)
    out: list[Sessao] = []
    for name, fn in (("claude", lambda: claude_sessions(roots["claude"], now, janela_s, ativa_s,
                                                        stale, budget, cache)),
                     ("codex", lambda: codex_sessions(roots["codex"], now, janela_s, ativa_s,
                                                      stale, budget, cache)),
                     ("opencode", lambda: opencode_sessions(roots["opencode"], now, janela_s,
                                                            ativa_s, stale, budget))):
        try:
            out += fn()
        except Exception as e:  # um agente com formato estranho não derruba os outros
            if errors is not None:
                errors.append(f"{name}: {type(e).__name__}")
    live = {k for k in cache if os.path.exists(k[1])} if len(cache) > 4 * MAX_FILES else None
    if live is not None:
        for k in set(cache) - live:
            del cache[k]
    out.sort(key=lambda s: -s.atualizado)
    return out[:MAX_SESSIONS]


def snapshot(self_pid: int | None = None, now: float | None = None, janela_s: float = 3600,
             ativa_s: float = 180, env=None, cpu_prev: dict | None = None,
             cache: dict | None = None) -> Monitor:
    t0 = time.monotonic()
    errors: list[str] = []
    try:
        procs = scan_processes(self_pid, cpu_prev)
    except Exception as e:
        procs, _ = [], errors.append(f"processos: {type(e).__name__}")
    try:
        sess = scan_sessions(now, janela_s, ativa_s, env, cache, errors)
    except Exception as e:
        sess, _ = [], errors.append(f"sessões: {type(e).__name__}")
    return Monitor(procs, sess, time.time(), "; ".join(errors),
                   (time.monotonic() - t0) * 1000)


class MonitorWatcher(threading.Thread):
    """Como o VaultWatcher: relê a cada `intervalo` s; a tela pega `snapshot`."""

    def __init__(self, intervalo: float = 3.0, self_pid: int | None = None,
                 janela_s: float = 3600, ativa_s: float = 180, env=None):
        super().__init__(daemon=True, name="monitor")
        self.intervalo = intervalo
        self.self_pid = os.getpid() if self_pid is None else self_pid
        self.janela_s = janela_s
        self.ativa_s = ativa_s
        self.env = env
        self.version = 0
        self._lock = threading.Lock()
        self._snap = Monitor()
        self._wake = threading.Event()
        self._stop = threading.Event()
        self._cpu: dict = {}
        self._cache: dict = {}

    @property
    def snapshot(self) -> Monitor:
        with self._lock:
            return self._snap

    def refresh(self) -> None:
        self._wake.set()

    def stop(self) -> None:
        self._stop.set()
        self._wake.set()

    def scan(self) -> Monitor:
        try:
            return snapshot(self.self_pid, None, self.janela_s, self.ativa_s, self.env,
                            self._cpu, self._cache)
        except Exception as e:
            return Monitor([], [], time.time(), type(e).__name__)

    def run(self) -> None:
        while not self._stop.is_set():
            snap = self.scan()
            with self._lock:
                self._snap = snap
                self.version += 1
            self._wake.wait(self.intervalo)
            self._wake.clear()
