import datetime as dt
import os
import queue
import stat
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from hud import agenda as ag
from hud import config
from hud.runner import Runner
from hud.text import clean, fit, width, wrap
from hud.vault import VaultWatcher, parse_board, parse_tasks
from tests.suporte import ECHO, ECHO_ARGV, SLEEP, WINDOWS, fake_executable, posix_only, symlink, windows_only

TODAY = dt.date(2026, 10, 8)  # quinta


class TextTest(unittest.TestCase):
    def test_strips_escape_sequences(self):
        evil = ("ok\x1b]0;título falso\x07 \x1b]52;c;ZXZpbA==\x1b\\fim"
                "\x1b[31mvermelho\x1b[0m \x1bP+q\x1b\\ ‮abc \x00\x07\x9b")
        out = clean(evil)
        self.assertNotIn("\x1b", out)
        self.assertNotIn("‮", out)
        self.assertNotIn("\x9b", out)
        self.assertIn("vermelho", out)
        self.assertIn("fim", out)

    def test_carriage_return_keeps_last_segment(self):
        self.assertEqual(clean("10%\r50%\r100%\nok"), "100%\nok")

    def test_fit_and_wrap_respect_columns(self):
        self.assertEqual(width(fit("日本語テキスト", 5)), 5)
        for line in wrap("uma frase comprida que precisa quebrar " * 3, 20):
            self.assertLessEqual(width(line), 20)


class AgendaTest(unittest.TestCase):
    def test_parse_entry(self):
        i = ag.parse_entry("amanhã 14h dentista", TODAY)
        self.assertEqual((i.date, i.time, i.text), (dt.date(2026, 10, 9), "14:00", "dentista"))
        i = ag.parse_entry("sex 9:30 revisar PR", TODAY)
        self.assertEqual((i.date, i.time), (dt.date(2026, 10, 9), "09:30"))
        i = ag.parse_entry("qui call", TODAY)
        self.assertEqual(i.date, dt.date(2026, 10, 15))  # próxima quinta, não hoje
        i = ag.parse_entry("3/1 ano novo", TODAY)
        self.assertEqual(i.date, dt.date(2027, 1, 3))
        i = ag.parse_entry("+2 14 pessoas na reunião", TODAY)
        self.assertEqual((i.time, i.text), (None, "14 pessoas na reunião"))

    def test_parse_errors(self):
        for bad in ("", "ontem algo", "hoje", "hoje 25:00", "31/02 x"):
            with self.assertRaises(ValueError):
                ag.parse_entry(bad, TODAY)

    def test_roundtrip_file_is_private(self):
        with tempfile.TemporaryDirectory() as d:
            a = ag.Agenda(Path(d) / "sub" / "agenda.md")
            a.add(ag.parse_entry("hoje 08:00 café", TODAY))
            a.add(ag.parse_entry("amanhã treino", TODAY))
            a.mark_done(a.items[0])
            b = ag.Agenda(a.path)
            self.assertEqual([(i.text, i.done) for i in b.items], [("café", True), ("treino", False)])
            if not WINDOWS:
                self.assertEqual(stat.S_IMODE(a.path.stat().st_mode), 0o600)

    def test_control_chars_do_not_break_the_file(self):
        i = ag.parse_entry("hoje x\x1b[2J\n- [ ] 2020-01-01 injetado", TODAY)
        self.assertNotIn("\n", i.text)
        self.assertNotIn("\x1b", i.text)


BOARD = """---

kanban-plugin: board
title: Kanban (Teste)

---

## Como usar

- [ ] regra que não conta

## A fazer

- [ ] **AT-1** — primeira
- [ ] AT-2 — [[Notas/AT-2 — segunda|segunda]]
	  continuação

## Em andamento

- [ ] **AT-3** — rodando

## Bloqueado

## Concluído

- [x] AT-0 — feito

%% kanban:settings
```
{"kanban-plugin":"board"}
```
%%
"""


class VaultTest(unittest.TestCase):
    def test_parse_board(self):
        b = parse_board(BOARD, "x/Kanban (Teste).md")
        self.assertEqual(b.name, "Teste")
        self.assertEqual(b.counts, {"todo": 2, "doing": 1, "blocked": 0, "done": 1})
        self.assertEqual(b.doing, ["AT-3 — rodando"])
        self.assertIsNone(parse_board("# nota comum\n- [ ] a", "n.md"))

    def test_parse_tasks(self):
        t = parse_tasks("- [ ] pagar boleto 📅 2026-10-10\n- [x] feito 📅 2026-10-01\n"
                        "- [ ] reunião @{2026-10-09} @@{15:00}\n- [ ] sem data", "n.md")
        self.assertEqual([(i.text, i.date.day, i.time) for i in t],
                         [("pagar boleto", 10, None), ("reunião", 9, "15:00")])

    def test_scan_skips_hidden_symlinks_and_counts_conflicts(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as outside:
            root = Path(d)
            (root / "p").mkdir()
            (root / "p" / "Kanban (Teste).md").write_text(BOARD, encoding="utf-8")
            (root / "p" / "a.sync-conflict-1.md").write_text("x")
            (root / ".obsidian").mkdir()
            (root / ".obsidian" / "z.md").write_text("x")
            Path(outside, "segredo.md").write_text("segredo")
            symlink(self, outside, root / "link")
            symlink(self, Path(outside, "segredo.md"), root / "p" / "s.md")
            w = VaultWatcher(root)
            snap = w.scan()
            self.assertTrue(snap.ok)
            self.assertEqual(snap.notes, 2)
            self.assertEqual(len(snap.conflicts), 1)
            self.assertEqual([b.name for b in snap.boards], ["Teste"])
            self.assertEqual(w.search("segredo"), [])


class ConfigTest(unittest.TestCase):
    def test_forbidden_and_missing_executables(self):
        for argv in (["sudo", "ls"], ["bash", "-c", "id"], ["/bin/sh"], ["nao-existe-xyz"]):
            with self.assertRaises(config.ConfigError):
                config.build_command(0, {"name": "x", "argv": argv})

    def test_resolves_to_absolute_path(self):
        c = config.build_command(0, {"name": "eco", "argv": [ECHO, "oi"]})
        self.assertTrue(os.path.isabs(c.argv[0]))

    def test_default_commands_resolve(self):
        """Os comandos padrão do sistema em que o teste roda existem e passam nas regras."""
        cfg = config.load(Path(tempfile.gettempdir()) / "hud-nao-existe.toml")
        self.assertEqual(cfg.source, "padrão")
        self.assertGreaterEqual(len(cfg.commands), 3, cfg.warnings)

    @windows_only
    def test_windows_config_outside_profile_is_ignored(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as outro:
            p = Path(d) / "config.toml"
            p.write_text('[[command]]\nname = "x"\nargv = ["whoami"]\n')
            self.assertEqual([c.name for c in config.load(p).commands], ["x"])
            fora = {k: os.path.join(outro, "perfil") for k in ("USERPROFILE", "APPDATA", "LOCALAPPDATA")}
            with mock.patch.dict(os.environ, fora):
                cfg = config.load(p)
            self.assertEqual(cfg.source, "padrão")
            self.assertTrue(any("fora do seu perfil" in w for w in cfg.warnings), cfg.warnings)

    @windows_only
    def test_windows_refuses_cmd_and_batch(self):
        for argv in (["cmd", "/c", "dir"], ["CMD.EXE"], ["powershell"], ["PowerShell.exe", "-c", "1"],
                     ["wsl"], ["C:\\Windows\\System32\\cmd.exe"], ["rundll32"], ["cmd.exe."]):
            with self.assertRaises(config.ConfigError, msg=argv):
                config.build_command(0, {"name": "x", "argv": argv})
        with tempfile.TemporaryDirectory() as d:
            bat = Path(d) / "x.bat"
            bat.write_text("@echo off\r\n")
            with self.assertRaises(config.ConfigError):
                config.build_command(0, {"name": "x", "argv": [str(bat)]})

    @posix_only
    def test_rejects_world_writable_config(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "config.toml"
            p.write_text('[[command]]\nname = "x"\nargv = ["id"]\n')
            os.chmod(p, 0o666)
            cfg = config.load(p)
            self.assertEqual(cfg.source, "padrão")
            self.assertTrue(any("escrito por outros" in w for w in cfg.warnings))
            os.chmod(p, 0o600)
            cfg = config.load(p)
            self.assertEqual([c.name for c in cfg.commands], ["x"])


@posix_only
class RunnerPosixTest(unittest.TestCase):
    def run_cmd(self, argv, **kw):
        q = queue.Queue()
        cmd = config.Command(key="1", name="t", argv=tuple(argv), **kw)
        Runner(q).start(cmd)
        out = q.get(timeout=10)
        end = q.get(timeout=10)
        return out[2], end

    def test_output_is_cleaned_and_env_is_minimal(self):
        os.environ["LD_PRELOAD_TESTE"] = "x"
        lines, end = self.run_cmd([config.resolve_executable("printenv")])
        self.assertEqual(end[2], 0)
        names = {l.split("=", 1)[0] for l in lines}
        self.assertNotIn("LD_PRELOAD_TESTE", names)
        self.assertIn("PATH=" + config.SAFE_PATH, lines)
        lines, _ = self.run_cmd([config.resolve_executable("printf"), "a\\033]0;x\\007b"])
        self.assertEqual(lines, ["ab"])

    def test_line_cap(self):
        lines, end = self.run_cmd([config.resolve_executable("seq"), "1000"], max_lines=10)
        self.assertEqual(len(lines), 10)
        self.assertTrue(end[5])


class RunnerTest(unittest.TestCase):
    """Roda em todos os sistemas (no Windows: CREATE_NEW_PROCESS_GROUP e taskkill)."""
    run_cmd = RunnerPosixTest.run_cmd

    def test_runs_and_ends(self):
        lines, end = self.run_cmd([config.resolve_executable(ECHO), *ECHO_ARGV[1:]])
        self.assertEqual(end[2], 0)
        self.assertTrue(lines and lines[0].strip())

    def test_timeout_kills(self):
        t0 = time.monotonic()
        _, end = self.run_cmd([config.resolve_executable(SLEEP[0]), *SLEEP[1:]], timeout=0.5)
        self.assertTrue(end[4])
        self.assertLess(time.monotonic() - t0, 5)

    @windows_only
    def test_windows_env_and_line_cap(self):
        from hud.runner import safe_env
        env = safe_env()
        self.assertEqual(env["PATH"], config.SAFE_PATH)
        for k in ("SystemRoot", "TEMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "PATHEXT", "COMSPEC"):
            self.assertIn(k, env)
        lines, end = self.run_cmd([config.resolve_executable("tasklist"), "/FO", "TABLE"], max_lines=3)
        self.assertEqual(len(lines), 3)
        self.assertTrue(end[5])


if __name__ == "__main__":
    unittest.main()


from hud import usage as us
from hud.agent import agent_env
from hud.claude import ClaudeConfig, allow_rules, build_argv, rule_path
from hud import codex as cx


class UsageTest(unittest.TestCase):
    def test_segments_and_levels(self):
        self.assertEqual(us.segments(100), ["full"] * 5)
        self.assertEqual(us.segments(0), ["empty"] * 5)
        self.assertEqual(us.segments(86), ["full"] * 4 + ["part"])
        self.assertEqual(us.segments(20), ["full"] + ["empty"] * 4)
        self.assertEqual([us.level(v) for v in (100, 76, 75, 50, 26, 25, 0)],
                         ["ok", "ok", "warn", "warn", "warn", "crit", "crit"])

    def test_parse_cache_and_event(self):
        u = us.parse_cache('{"five_hour_used": 14.0, "seven_day_used": 12.0, '
                           '"five_hour_reset_min": 60, "fetched_at": 1000}')
        self.assertEqual((u.five_hour.left, u.seven_day.left, u.five_hour.resets_at), (86, 88, 4600))
        self.assertIsNone(us.parse_cache('{"five_hour_used": 900, "seven_day_used": 1, "fetched_at": 1}'))
        self.assertIsNone(us.parse_cache("não é json"))
        e = us.parse_event({"unifiedWindows": {"five_hour": {"utilization": 0.15, "resetsAt": 5},
                                               "seven_day": {"utilization": 0.12}}})
        self.assertAlmostEqual(e.five_hour.left, 85)
        self.assertIsNone(us.parse_event({"unifiedWindows": {"five_hour": {"utilization": 3}}}))

    @posix_only
    def test_tracker_rejects_foreign_writable_cache(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "c.json"
            p.write_text('{"five_hour_used": 50, "seven_day_used": 10, "fetched_at": 1}')
            os.chmod(p, 0o666)
            self.assertFalse(us.UsageTracker(p).poll())
            os.chmod(p, 0o600)
            t = us.UsageTracker(p)
            self.assertTrue(t.poll())
            self.assertEqual(t.current.five_hour.left, 50)


    @windows_only
    def test_tracker_windows_outside_profile(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as outro:
            p = Path(d) / "c.json"
            p.write_text('{"five_hour_used": 50, "seven_day_used": 10, "fetched_at": 1}')
            fora = {k: os.path.join(outro, "perfil") for k in ("USERPROFILE", "APPDATA", "LOCALAPPDATA")}
            with mock.patch.dict(os.environ, fora):
                self.assertFalse(us.UsageTracker(p).poll())
            self.assertTrue(us.UsageTracker(p).poll())


def fake_agent(name: str) -> str:
    """Executável falso para os testes não dependerem de claude/codex instalados (o CI não tem)."""
    return fake_executable(name)


class ClaudeTest(unittest.TestCase):
    def test_argv_is_read_only_without_mcp_and_prompt_not_in_argv(self):
        argv = build_argv(ClaudeConfig(executable="/x/claude", cwd="/"), "abc")
        self.assertIn("--strict-mcp-config", argv)
        self.assertEqual(argv[argv.index("--permission-mode") + 1], "dontAsk")
        self.assertEqual(argv[argv.index("--tools") + 1], "Read,Grep,Glob")
        self.assertEqual(argv[-2:], ["--resume", "abc"])

    @posix_only
    def test_reads_are_scoped_and_secrets_denied(self):
        home = os.path.expanduser("~")
        cfg = ClaudeConfig(executable="/x/claude", cwd="/", read_dirs=(home + "/Vault", "/srv/dados"))
        self.assertEqual(allow_rules(cfg)[:2], ["Read(~/Vault/**)", "Read(//srv/dados/**)"])
        self.assertNotIn("Read", allow_rules(cfg))
        argv = build_argv(cfg)
        self.assertIn("Read(~/.ssh/**)", argv)
        self.assertIn("Grep(**/.env)", argv)
        self.assertEqual(rule_path("~"), "~")

    def test_env_drops_injection_vars(self):
        os.environ["LD_PRELOAD"] = "/tmp/x.so"
        os.environ["ANTHROPIC_MODEL_TESTE"] = "y"
        try:
            env = agent_env(("ANTHROPIC_",))
        finally:
            del os.environ["LD_PRELOAD"]
        self.assertNotIn("LD_PRELOAD", env)
        self.assertNotIn("CLAUDECODE", env)
        self.assertEqual(env["ANTHROPIC_MODEL_TESTE"], "y")

    def test_full_profile_is_your_claude_code_but_keeps_secrets_and_budget(self):
        cfg = ClaudeConfig(executable="/x/claude", cwd="/", read_dirs=("/srv",))
        argv = build_argv(cfg, profile="completo")
        self.assertNotIn("--strict-mcp-config", argv)
        self.assertNotIn("--tools", argv)
        self.assertEqual(argv[argv.index("--permission-mode") + 1], "auto")
        self.assertIn("Read(~/.ssh/**)", argv)
        self.assertIn("--max-budget-usd", argv)

    def test_config_profiles(self):
        with self.assertRaises(config.ConfigError):
            config.build_claude({"executable": fake_agent("claude"), "full_permission_mode": "bypassPermissions"})
        with self.assertRaises(config.ConfigError):
            config.build_claude({"executable": fake_agent("claude"), "profile": "tudo"})
        self.assertEqual(config.build_claude({"executable": fake_agent("claude"), "profile": "completo"}).profile, "completo")

    def test_config_rejects_bad_tools(self):
        with self.assertRaises(config.ConfigError):
            config.build_claude({"executable": fake_agent("claude"), "tools": ["Read; rm -rf ~"]})


class CodexTest(unittest.TestCase):
    def test_argv_read_only_and_resume(self):
        cfg = cx.CodexConfig(executable="/x/codex", cwd="/")
        argv = cx.build_argv(cfg)
        self.assertEqual(argv[:2], ["/x/codex", "exec"])
        self.assertIn('sandbox_mode="read-only"', argv)
        self.assertEqual(argv[-1], "-")  # prompt pela entrada padrão
        argv = cx.build_argv(cfg, "abc")
        self.assertEqual(argv[2:4], ["resume", "abc"])
        self.assertIn('sandbox_mode="read-only"', argv)
        self.assertNotIn('sandbox_mode="read-only"', cx.build_argv(cfg, profile="completo"))

    def test_events(self):
        q = queue.Queue()
        c = cx.Codex(cx.CodexConfig(executable="/x/codex", cwd="/"), q)
        c.handle({"type": "thread.started", "thread_id": "t1"})
        c.handle({"type": "item.started", "item": {"type": "command_execution", "command": "ls \x1b[2J"}})
        c.handle({"type": "item.completed", "item": {"type": "agent_message", "text": "oi\x1b]0;x\x07"}})
        end = c.handle({"type": "turn.completed", "usage": {"input_tokens": 1500, "output_tokens": 500}})
        self.assertEqual(c.session.id, "t1")
        self.assertEqual([q.get_nowait() for _ in range(2)],
                         [("agent_tool", "codex", "$ ls "), ("agent_text", "codex", "oi")])
        self.assertEqual(c.finish(end), (True, "", "conversa 2k tokens"))


class FolderTest(unittest.TestCase):
    def test_validate_folder(self):
        for bad in ("/", "/proc", "/etc", "/etc/ssh", "/nao/existe", ""):
            with self.assertRaises(config.ConfigError):
                config.validate_folder(bad)
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(config.validate_folder(d), Path(os.path.realpath(d)))

    def test_remembered_folder_precedence_and_agents_follow(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as pasta:
            cfgp = Path(d) / "c.toml"
            # Strings literais do TOML ('...'): caminhos do Windows têm barra invertida.
            cfgp.write_text(f"data_dir = '{d}/dados'\nvault = '{d}'\n"
                            f"[claude]\nexecutable = '{fake_agent('claude')}'\n"
                            f"[codex]\nexecutable = '{fake_agent('codex')}'\n")
            os.chmod(cfgp, 0o600)
            self.assertEqual(config.load(cfgp).vault, Path(d))
            config.remember_folder(Path(d) / "dados", Path(pasta))
            if not WINDOWS:
                self.assertEqual(stat.S_IMODE((Path(d) / "dados" / "pasta").stat().st_mode), 0o600)
            cfg = config.load(cfgp)
            real = os.path.realpath(pasta)
            self.assertEqual((str(cfg.vault), cfg.folder_source), (real, "escolhida com /pasta"))
            self.assertEqual(cfg.claude.read_dirs, (real,))
            self.assertEqual(cfg.codex.cwd, real)
            self.assertEqual(config.load(cfgp, folder=d).folder_source, "linha de comando")
            config.remember_folder(Path(d) / "dados", None)
            self.assertEqual(config.load(cfgp).folder_source, "config")

    def test_generic_folder_scan(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "notas.txt").write_text("- [ ] pagar 📅 2026-10-10", encoding="utf-8")
            (root / "codigo.py").write_text("x")
            (root / "node_modules").mkdir()
            (root / "node_modules" / "lixo.md").write_text("x")
            w = VaultWatcher(root)
            snap = w.scan()
            self.assertFalse(snap.obsidian)
            self.assertEqual((snap.notes, len(snap.tasks)), (1, 1))
            (root / ".obsidian").mkdir()
            self.assertTrue(w.scan().obsidian)
