"""Monitor de agentes: processos e sessões (só fixtures sintéticas)."""

import json
import os
import shutil
import sqlite3
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from hud import monitor as mon
from tests.suporte import home_env, posix_only, symlink

NOW = 1_800_000_000.0
SECRET = "CONTEUDO-SECRETO-DA-FERRAMENTA"


def iso(ts: float) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(ts))


def write_jsonl(path: Path, recs: list, mtime: float | None = None, extra: str = "") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(json.dumps(r) for r in recs) + "\n" + extra, encoding="utf-8")
    if os.name != "nt":
        os.chmod(path, 0o600)
    if mtime is not None:
        os.utime(path, (mtime, mtime))
    return path


def raw(pid, ppid, *argv, cpu_s=None, desde=0.0):
    return mon.RawProc(pid, ppid, list(argv), "/w", desde, cpu_s)


class IdentifyTest(unittest.TestCase):
    def test_agents_and_modes(self):
        cases = [
            (["claude"], ("claude", "interativo")),
            (["/opt/bin/claude", "-p", "oi"], ("claude", "headless")),
            (["claude", "--print", "--output-format", "stream-json"], ("claude", "headless")),
            (["node", "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js", "-p"],
             ("claude", "headless")),
            (["codex", "exec", "--json", "x"], ("codex", "headless")),
            (["/opt/codex", "app-server"], ("codex", "servidor")),
            (["codex", "exec-server", "--remote"], ("codex", "servidor")),
            (["codex"], ("codex", "interativo")),
            (["node", "/n/@openai/codex/bin/codex.js", "exec"], ("codex", "headless")),
            (["opencode", "run", "--format", "json"], ("opencode", "headless")),
            (["opencode", "serve"], ("opencode", "servidor")),
            (["C:\\Users\\x\\claude.exe"], ("claude", "interativo")),
        ]
        for argv, want in cases:
            with self.subTest(argv=argv):
                got = mon.identify(argv)
                self.assertIsNotNone(got)
                self.assertEqual((got[0], mon.mode_of(got[0], argv[got[1]:])), want)

    def test_not_agents(self):
        for argv in (["claude-desktop", "--type=zygote"], ["node", "server.js"],
                     ["/usr/lib/chatgpt/ChatGPT"], ["bash", "-c", "claude -p oi"], [],
                     ["node", "/x/claude-code-mcp/index.js"], ["vim", "codex.txt"]):
            with self.subTest(argv=argv):
                self.assertIsNone(mon.identify(argv))


class ClassifyTest(unittest.TestCase):
    def test_do_hud_dedupe_and_cpu(self):
        raws = [
            raw(1, 0, "init"),
            raw(100, 1, "python3", "-m", "hud"),
            raw(101, 100, "claude", "-p", "oi", cpu_s=2.0, desde=NOW - 100),
            raw(200, 1, "claude", cpu_s=50.0, desde=NOW - 1000),
            raw(300, 1, "node", "/n/@openai/codex/bin/codex.js", "exec"),
            raw(301, 300, "/n/@openai/codex/vendor/codex", "exec"),
            raw(400, 1, "bash"),
        ]
        prev: dict = {}
        out = mon.classify(raws, self_pid=100, cpu_prev=prev, now=NOW)
        self.assertEqual([(p.pid, p.agente, p.modo, p.do_hud) for p in out],
                         [(101, "claude", "headless", True), (200, "claude", "interativo", False),
                          (300, "codex", "headless", False)])
        self.assertEqual(out[0].cpu, 2.0)  # 1ª leitura: média da vida (2 s em 100 s)
        self.assertEqual(out[1].cpu, 5.0)
        self.assertEqual(out[0].cwd, "/w")
        raws[2].cpu_s = 3.0  # +1 s de CPU em 2 s
        out = mon.classify(raws, self_pid=100, cpu_prev=prev, now=NOW + 2)
        self.assertEqual(out[0].cpu, 50.0)
        mon.classify(raws[:1], cpu_prev=prev, now=NOW + 3)
        self.assertEqual(prev, {})  # morto sai do estado

    def test_parse_ps_and_etime(self):
        self.assertEqual(mon.parse_etime("05:03"), 303)
        self.assertEqual(mon.parse_etime("1:00:00"), 3600)
        self.assertEqual(mon.parse_etime("2-00:00:01"), 2 * 86400 + 1)
        self.assertIsNone(mon.parse_etime("x:1"))
        text = ("  10     1   501   01:00   3.5 /usr/local/bin/claude -p oi\n"
                "  11     1   0     01:00   1.0 /usr/local/bin/codex\n"
                "lixo\n"
                "  12    10   501   00:10   0.0 node /x/claude-code/cli.js\n")
        raws = mon.parse_ps(text, uid=501, now=NOW)
        self.assertEqual([r.pid for r in raws], [10, 12])
        self.assertEqual(raws[0].desde, NOW - 60)
        out = mon.classify(raws, self_pid=10, now=NOW)
        self.assertEqual([(p.pid, p.cpu, p.do_hud, p.cwd) for p in out],
                         [(12, 0.0, True, ""), (10, 3.5, False, "")])

    def test_scan_processes_never_raises(self):
        with mock.patch.object(mon, "linux_procs", side_effect=RuntimeError), \
                mock.patch.object(mon, "macos_procs", side_effect=RuntimeError):
            self.assertEqual(mon.scan_processes(), [])

    @unittest.skipUnless(sys.platform == "win32", "só no Windows")
    def test_windows_empty(self):
        self.assertEqual(mon.scan_processes(), [])


@posix_only
class FakeProcTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root, True)
        (self.root / "stat").write_text("cpu 1 2 3\nbtime 1000\n")

    def proc(self, pid, ppid, argv, utime=100, start=500, cwd=None):
        d = self.root / str(pid)
        d.mkdir()
        (d / "cmdline").write_bytes(b"\0".join(a.encode() for a in argv) + b"\0")
        rest = ["S", str(ppid)] + ["0"] * 9 + [str(utime), "0"] + ["0"] * 6 + [str(start), "0"]
        (d / "stat").write_text(f"{pid} (nome com ) espaço) " + " ".join(rest) + "\n")
        if cwd:
            symlink(self, cwd, d / "cwd")

    def test_reads_fake_proc(self):
        self.proc(10, 1, ["claude", "-p", "x"], cwd=str(self.root))
        self.proc(11, 10, ["sleep", "1"])
        (self.root / "self").mkdir()
        (self.root / "12").mkdir()  # sem arquivos: ignorado
        raws = mon.linux_procs(str(self.root), uid=os.getuid())
        by = {r.pid: r for r in raws}
        self.assertEqual(sorted(by), [10, 11])
        tck = os.sysconf("SC_CLK_TCK")
        self.assertEqual(by[10].ppid, 1)
        self.assertAlmostEqual(by[10].desde, 1000 + 500 / tck)
        self.assertAlmostEqual(by[10].cpu_s, 100 / tck)
        self.assertEqual(by[10].cwd, str(self.root))
        self.assertEqual(by[11].cwd, "")  # só lê o cwd de agente
        self.assertEqual(mon.linux_procs(str(self.root), uid=os.getuid() + 1), [])


class ClaudeSessionsTest(unittest.TestCase):
    def setUp(self):
        self.home = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.home, True)
        self.env = home_env(str(self.home))
        self.proj = self.home / ".claude" / "projects" / "-w-proj"

    def session(self, sid="s1", mtime=NOW - 10, extra=""):
        t = NOW - 600
        recs = [
            {"type": "mode", "sessionId": sid},
            {"type": "user", "isMeta": True, "cwd": "/w/proj", "sessionId": sid,
             "message": {"role": "user", "content": "<local-command>x</local-command>"}},
            {"type": "user", "cwd": "/w/proj", "sessionId": sid, "timestamp": iso(t),
             "message": {"role": "user", "content": "Arrume\x1b]52;c;x\x07 o build\tpor favor"}},
            {"type": "assistant", "cwd": "/w/proj", "timestamp": iso(t + 1), "message": {
                "role": "assistant", "content": [
                    {"type": "tool_use", "id": "t-sync", "name": "Task",
                     "input": {"description": "Ler o código", "subagent_type": "Explore",
                               "prompt": SECRET}},
                    {"type": "tool_use", "id": "t-bg", "name": "Agent",
                     "input": {"description": "Rodar testes", "subagent_type": "general-purpose",
                               "prompt": SECRET, "run_in_background": True}},
                    {"type": "tool_use", "id": "t-bg2", "name": "Agent",
                     "input": {"description": "Lane longa", "prompt": SECRET}},
                    {"type": "tool_use", "id": "t-bash", "name": "Bash",
                     "input": {"command": SECRET}}]}},
            {"type": "user", "timestamp": iso(t + 2), "toolUseResult": {"status": "completed"},
             "message": {"content": [{"type": "tool_result", "tool_use_id": "t-sync",
                                      "content": SECRET}]}},
            {"type": "user", "timestamp": iso(t + 2),
             "toolUseResult": {"status": "async_launched", "agentId": "a1"},
             "message": {"content": [{"type": "tool_result", "tool_use_id": "t-bg",
                                      "content": SECRET}]}},
            {"type": "user", "timestamp": iso(t + 2),
             "toolUseResult": {"status": "async_launched", "agentId": "a2"},
             "message": {"content": [{"type": "tool_result", "tool_use_id": "t-bg2",
                                      "content": SECRET}]}},
            {"type": "queue-operation", "operation": "enqueue",
             "content": "<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>t-bg</tool-use-id>"
                        "\n<status>completed</status>\n<result>" + SECRET + "</result>"},
            {"type": "ai-title", "aiTitle": "Arrumar o build", "sessionId": sid},
        ]
        return write_jsonl(self.proj / f"{sid}.jsonl", recs, mtime, extra)

    def scan(self, **kw):
        return mon.scan_sessions(now=NOW, env=self.env, **kw)

    def test_session_title_and_subagents(self):
        self.session(extra="{quebrado\n[1,2]\n")
        out = self.scan()
        self.assertEqual(len(out), 1)
        s = out[0]
        self.assertEqual((s.agente, s.id, s.pasta, s.titulo, s.ativa),
                         ("claude", "s1", "/w/proj", "Arrumar o build", True))
        self.assertEqual(s.atualizado, NOW - 10)
        subs = {a.descricao: a for a in s.subagentes}
        self.assertEqual(set(subs), {"Ler o código", "Rodar testes", "Lane longa"})
        self.assertFalse(subs["Ler o código"].rodando)
        self.assertFalse(subs["Rodar testes"].rodando)
        self.assertTrue(subs["Lane longa"].rodando)
        self.assertEqual(subs["Ler o código"].tipo, "Explore")
        self.assertEqual(s.subagentes[0].descricao, "Lane longa")  # rodando primeiro
        self.assertAlmostEqual(subs["Rodar testes"].desde, NOW - 599, delta=1)
        self.assertNotIn(SECRET, repr(out))

    def test_first_user_text_when_no_title(self):
        p = self.session()
        lines = [ln for ln in p.read_text().splitlines() if "ai-title" not in ln]
        p.write_text("\n".join(lines) + "\n")
        os.utime(p, (NOW - 10, NOW - 10))
        titulo = self.scan()[0].titulo
        self.assertEqual(titulo, "Arrume o build    por favor".replace("    ", " "))
        self.assertNotIn("\x1b", titulo)

    def test_window_and_activity(self):
        self.session("velha", mtime=NOW - 7200)
        self.session("parada", mtime=NOW - 1000)
        out = self.scan()
        self.assertEqual([(s.id, s.ativa) for s in out], [("parada", False)])
        self.assertEqual(len(self.scan(janela_s=86400)), 2)

    def test_stale_subagent_not_running(self):
        self.session(mtime=NOW - 10)
        out = mon.scan_sessions(now=NOW + 3000, janela_s=86400, env=self.env)
        self.assertFalse(any(a.rodando for a in out[0].subagentes))

    def test_subagent_meta_files(self):
        self.session(mtime=NOW - 2000)  # principal parada, subagente mexendo
        sub = self.proj / "s1" / "subagents"
        sub.mkdir(parents=True)
        (sub / "agent-a2.meta.json").write_text(json.dumps(
            {"agentType": "executor", "description": "Lane longa", "toolUseId": "t-bg2"}))
        write_jsonl(sub / "agent-a2.jsonl", [{"type": "assistant", "message": {"content": SECRET}}],
                    NOW - 5)
        wf = sub / "workflows" / "wf_1"
        wf.mkdir(parents=True)
        (wf / "agent-w1.meta.json").write_text(json.dumps(
            {"agentType": "workflow-subagent", "description": "Fase 1"}))
        write_jsonl(wf / "agent-w1.jsonl", [], NOW - 30)
        (sub / "agent-x.meta.json").write_text("{quebrado")
        os.utime(sub, (NOW - 5, NOW - 5))
        out = self.scan()
        self.assertEqual(len(out), 1)
        s = out[0]
        self.assertTrue(s.ativa)
        self.assertEqual(s.atualizado, NOW - 5)
        subs = {a.descricao: a for a in s.subagentes}
        self.assertEqual(subs["Lane longa"].tipo, "executor")  # o tool_use não trouxe tipo
        self.assertEqual(subs["Rodar testes"].tipo, "general-purpose")
        self.assertTrue(subs["Lane longa"].rodando)
        self.assertEqual(subs["Fase 1"].tipo, "workflow-subagent")
        self.assertNotIn(SECRET, repr(out))

    def test_cache_reuses_unchanged(self):
        self.session()
        cache: dict = {}
        first = self.scan(cache=cache)
        with mock.patch.object(mon, "_parse_claude", side_effect=AssertionError("releu")):
            self.assertEqual(self.scan(cache=cache), first)

    def test_config_dir_env(self):
        alt = self.home / "alt"
        write_jsonl(alt / "projects" / "p" / "z.jsonl",
                    [{"type": "ai-title", "aiTitle": "Alt"}], NOW - 1)
        env = dict(self.env, CLAUDE_CONFIG_DIR=str(alt))
        self.assertEqual([s.titulo for s in mon.scan_sessions(now=NOW, env=env)], ["Alt"])

    def test_file_budget(self):
        for i in range(5):
            self.session(f"s{i}", mtime=NOW - i)
        with mock.patch.object(mon, "MAX_FILES", 3):
            self.assertEqual(len(self.scan()), 3)

    def test_garbage_never_raises(self):
        (self.proj / "pasta.jsonl").mkdir(parents=True)
        (self.proj / "bin.jsonl").write_bytes(b"\xff\xfe\x00" * 1000)
        self.assertIsInstance(self.scan(), list)
        errors: list = []
        with mock.patch.object(mon, "claude_sessions", side_effect=RuntimeError("x")):
            self.assertEqual(mon.scan_sessions(now=NOW, env=self.env, errors=errors), [])
        self.assertEqual(errors, ["claude: RuntimeError"])

    @posix_only
    def test_links_and_foreign_files_ignored(self):
        outside = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, outside, True)
        target = write_jsonl(outside / "fora.jsonl", [{"type": "ai-title", "aiTitle": "Fora"}])
        self.proj.mkdir(parents=True)
        symlink(self, target, self.proj / "link.jsonl")
        symlink(self, outside, self.home / ".claude" / "projects" / "linkdir")
        p = self.session("aberta")
        os.chmod(p, 0o666)
        self.assertEqual(self.scan(), [])


class CodexSessionsTest(unittest.TestCase):
    def setUp(self):
        self.home = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.home, True)
        self.env = home_env(str(self.home))
        self.day = self.home / ".codex" / "sessions" / "2027" / "01" / "15"

    def rollout(self, sid, meta_extra=None, recs=(), mtime=NOW - 20, day=None):
        meta = {"id": sid, "cwd": "/w/cx", "timestamp": iso(NOW - 900),
                "base_instructions": {"text": SECRET * 10}, "source": "cli"}
        meta.update(meta_extra or {})
        lines = [{"timestamp": iso(NOW - 900), "type": "session_meta", "payload": meta}] + list(recs)
        name = f"rollout-2027-01-15T10-00-00-{sid}.jsonl"
        return write_jsonl((day or self.day) / name, lines, mtime)

    def test_parent_child_and_title(self):
        sid, kid = "019e0000-0000-7000-8000-000000000001", "019e0000-0000-7000-8000-000000000002"
        self.rollout(sid, recs=[
            {"type": "event_msg", "payload": {"type": "task_started"}},
            {"type": "event_msg", "payload": {"type": "item_completed", "item": {
                "type": "UserMessage", "content": [{"type": "text", "text": "# AGENTS.md\n" + SECRET},
                                                   {"type": "text", "text": "Revise o PR"}]}}},
            {"type": "event_msg", "timestamp": iso(NOW - 100), "payload": {
                "type": "item_completed", "item": {"type": "SubAgentActivity", "kind": "started",
                                                   "agent_thread_id": "t-x",
                                                   "agent_path": "/root/sem_arquivo"}}},
            {"type": "response_item", "payload": {"type": "function_call", "name": "spawn_agent",
                                                  "arguments": json.dumps({"message": SECRET})}},
        ])
        self.rollout(kid, {"source": {"subagent": {"thread_spawn": {
            "parent_thread_id": sid, "depth": 1, "agent_nickname": "Hume", "agent_role": "worker",
            "agent_path": "/root/revisor"}}}, "parent_thread_id": sid, "agent_nickname": "Hume",
            "agent_role": "worker"},
            recs=[{"type": "event_msg", "payload": {"type": "task_started"}}], mtime=NOW - 5)
        out = mon.scan_sessions(now=NOW, env=self.env)
        self.assertEqual(len(out), 1)
        s = out[0]
        self.assertEqual((s.agente, s.id, s.pasta, s.titulo, s.ativa),
                         ("codex", sid, "/w/cx", "Revise o PR", True))
        self.assertEqual([(a.descricao, a.tipo, a.rodando) for a in s.subagentes],
                         [("sem_arquivo", "", True), ("Hume", "worker", True)])  # mais novo primeiro
        self.assertNotIn(SECRET, repr(out))
        idx = self.home / ".codex" / "session_index.jsonl"
        write_jsonl(idx, [{"id": sid, "thread_name": "Revisão do PR", "updated_at": iso(NOW)}])
        self.assertEqual(mon.scan_sessions(now=NOW, env=self.env)[0].titulo, "Revisão do PR")

    def test_finished_child_and_window(self):
        sid = "019e0000-0000-7000-8000-00000000000a"
        self.rollout(sid, recs=[{"type": "event_msg", "payload": {"type": "task_complete"}}])
        self.rollout("019e0000-0000-7000-8000-00000000000b", {
            "source": {"subagent": {"other": "guardian"}}, "parent_thread_id": sid},
            recs=[{"type": "event_msg", "payload": {"type": "task_started"}},
                  {"type": "event_msg", "payload": {"type": "task_complete"}}])
        self.rollout("019e0000-0000-7000-8000-00000000000c", mtime=NOW - 9000)
        out = mon.scan_sessions(now=NOW, env=self.env)
        self.assertEqual([s.id for s in out], [sid])
        self.assertEqual([(a.descricao, a.rodando) for a in out[0].subagentes],
                         [("guardian", False)])

    def test_codex_home_and_broken_meta(self):
        alt = self.home / "cx"
        self.rollout("019e0000-0000-7000-8000-00000000000d",
                     day=alt / "sessions" / "2027" / "01" / "16")
        p = alt / "sessions" / "2027" / "01" / "16" / "rollout-x-019e0000-0000-7000-8000-00000000000e.jsonl"
        p.write_text("{quebrado\n" + json.dumps({"type": "turn_context",
                                                  "payload": {"cwd": "/w/tc"}}) + "\n")
        os.utime(p, (NOW - 1, NOW - 1))
        out = mon.scan_sessions(now=NOW, env=dict(self.env, CODEX_HOME=str(alt)))
        self.assertEqual([(s.id, s.pasta) for s in out],
                         [("019e0000-0000-7000-8000-00000000000e", "/w/tc"),
                          ("019e0000-0000-7000-8000-00000000000d", "/w/cx")])


class OpencodeSessionsTest(unittest.TestCase):
    def setUp(self):
        self.home = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.home, True)
        self.env = home_env(str(self.home))
        self.base = self.home / ".local" / "share" / "opencode"
        self.base.mkdir(parents=True)

    def db(self, rows, msgs=()):
        path = self.base / "opencode.db"
        con = sqlite3.connect(path)
        con.execute("CREATE TABLE session (id text PRIMARY KEY, project_id text, parent_id text, "
                    "slug text, directory text, title text, agent text, time_created integer, "
                    "time_updated integer, time_archived integer)")
        con.execute("CREATE TABLE message (id text PRIMARY KEY, session_id text, "
                    "time_created integer, time_updated integer, data text)")
        con.execute("CREATE TABLE credential (id text, value text)")
        con.execute("INSERT INTO credential VALUES ('c', ?)", (SECRET,))
        for r in rows:
            con.execute("INSERT INTO session (id, parent_id, directory, title, agent, time_created,"
                        " time_updated, time_archived) VALUES (?,?,?,?,?,?,?,?)", r)
        for i, (sid, data) in enumerate(msgs):
            con.execute("INSERT INTO message VALUES (?,?,?,?,?)",
                        (f"m{i}", sid, i, i, json.dumps(data)))
        con.commit()
        con.close()
        if os.name != "nt":
            os.chmod(path, 0o600)

    def test_db_sessions_and_children(self):
        ms = lambda t: int(t * 1000)  # noqa: E731
        self.db([
            ("p1", None, "/w/oc", "Corrigir o bug", "build", ms(NOW - 500), ms(NOW - 30), None),
            ("k1", "p1", "/w/oc", "Procurar testes (@explore subagent)", "explore",
             ms(NOW - 200), ms(NOW - 20), None),
            ("k2", "p1", "/w/oc", "Ler docs (@general subagent)", "general",
             ms(NOW - 400), ms(NOW - 300), None),
            ("arq", None, "/w/oc", "Arquivada", "build", ms(NOW - 50), ms(NOW - 40), ms(NOW)),
            ("velha", None, "/w/oc", "Velha", "build", ms(NOW - 9000), ms(NOW - 8000), None),
        ], msgs=[("k1", {"role": "assistant", "time": {"created": 1}, "summary": SECRET}),
                 ("k2", {"role": "assistant", "time": {"created": 1, "completed": 2}})])
        out = mon.scan_sessions(now=NOW, env=self.env)
        self.assertEqual([(s.agente, s.id, s.pasta, s.titulo, s.ativa) for s in out],
                         [("opencode", "p1", "/w/oc", "Corrigir o bug", True)])
        self.assertEqual([(a.descricao, a.tipo, a.rodando) for a in out[0].subagentes],
                         [("Procurar testes", "explore", True), ("Ler docs", "general", False)])
        self.assertNotIn(SECRET, repr(out))

    def test_broken_db_falls_back_to_storage(self):
        (self.base / "opencode.db").write_bytes(b"isto nao e sqlite" * 100)
        d = self.base / "storage" / "session" / "proj"
        d.mkdir(parents=True)
        (d / "ses_1.json").write_text(json.dumps({
            "id": "ses_1", "directory": "/w/old", "title": "Antiga",
            "time": {"created": (NOW - 100) * 1000, "updated": (NOW - 10) * 1000}}))
        (d / "ses_2.json").write_text(json.dumps({
            "id": "ses_2", "parentID": "ses_1", "title": "Filha (@explore subagent)",
            "time": {"created": (NOW - 50) * 1000, "updated": (NOW - 5) * 1000}}))
        (d / "lixo.json").write_text("[")
        for f in d.iterdir():
            os.utime(f, (NOW - 5, NOW - 5))
        out = mon.scan_sessions(now=NOW, env=self.env)
        self.assertEqual([(s.id, s.pasta, s.titulo) for s in out], [("ses_1", "/w/old", "Antiga")])
        self.assertEqual([(a.descricao, a.tipo, a.rodando) for a in out[0].subagentes],
                         [("Filha", "explore", True)])

    def test_xdg_data_home(self):
        alt = self.home / "xdg"
        (alt / "opencode").mkdir(parents=True)
        self.base = alt / "opencode"
        self.db([("x", None, "/w", "Via XDG", "build", 0, int((NOW - 1) * 1000), None)])
        out = mon.scan_sessions(now=NOW, env=dict(self.env, XDG_DATA_HOME=str(alt)))
        self.assertEqual([s.titulo for s in out], ["Via XDG"])


class SnapshotWatcherTest(unittest.TestCase):
    def setUp(self):
        self.home = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.home, True)

    def test_snapshot_empty_home(self):
        with mock.patch.object(mon, "scan_processes", return_value=[]):
            m = mon.snapshot(env=home_env(self.home))
        self.assertEqual((m.processos, m.sessoes, m.erro), ([], [], ""))
        self.assertGreater(m.lido_em, 0)

    def test_watcher_runs_and_stops(self):
        w = mon.MonitorWatcher(intervalo=0.05, env=home_env(self.home))
        with mock.patch.object(mon, "scan_processes", return_value=[]):
            w.start()
            deadline = time.monotonic() + 5
            while w.version < 2 and time.monotonic() < deadline:
                time.sleep(0.01)
            w.stop()
            w.join(2)
        self.assertGreaterEqual(w.version, 2)
        self.assertFalse(w.is_alive())
        self.assertIsInstance(w.snapshot, mon.Monitor)

    def test_watcher_scan_never_raises(self):
        w = mon.MonitorWatcher(env=home_env(self.home))
        with mock.patch.object(mon, "snapshot", side_effect=RuntimeError):
            self.assertEqual(w.scan().erro, "RuntimeError")


if __name__ == "__main__":
    unittest.main()
