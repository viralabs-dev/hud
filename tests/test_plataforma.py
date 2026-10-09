"""Regras por plataforma, testadas em qualquer sistema.

As funções de hud.plataforma recebem `env`/`system`/`windows` justamente para
estes testes simularem o Windows e o macOS numa máquina Linux (e vice-versa).
O que depende do sistema de verdade (taskkill, junções, PDCurses) só o CI
em windows-latest e macos-latest prova.
"""

import errno
import ntpath
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from hud import config
from hud import plataforma as plat
from hud.claude import win_rule_path
from tests.suporte import WINDOWS, posix_only, symlink

WIN_ENV = {
    "SystemRoot": r"C:\Windows",
    "SYSTEMDRIVE": "C:",
    "USERPROFILE": r"C:\Users\ana",
    "APPDATA": r"C:\Users\ana\AppData\Roaming",
    "LOCALAPPDATA": r"C:\Users\ana\AppData\Local",
    "TEMP": r"C:\Users\ana\AppData\Local\Temp",
    "TMP": r"C:\Users\ana\AppData\Local\Temp",
    "PATHEXT": ".COM;.EXE;.BAT;.CMD;.VBS;.JS;.PS1",
    "ComSpec": r"C:\Windows\system32\cmd.exe",
    "USERNAME": "ana",
    "ProgramFiles": r"C:\Program Files",
    "ProgramFiles(x86)": r"C:\Program Files (x86)",
    "ProgramData": r"C:\ProgramData",
    "LD_PRELOAD": "/tmp/x.so",
    "PYTHONPATH": r"C:\evil",
    "GITHUB_TOKEN": "segredo",
}


def same(p: str) -> str:
    return p


class EnvTest(unittest.TestCase):
    def test_windows_safe_path(self):
        self.assertEqual(plat.windows_safe_path(WIN_ENV), ";".join([
            r"C:\Windows\System32", r"C:\Windows", r"C:\Windows\System32\Wbem",
            r"C:\Windows\System32\WindowsPowerShell\v1.0"]))
        # SystemRoot estranho (relativo, UNC) não entra: cai no padrão.
        self.assertTrue(plat.windows_safe_path({"SystemRoot": r"\\srv\x"}).startswith(r"C:\Windows"))
        self.assertTrue(plat.windows_safe_path({}).startswith(r"C:\Windows\System32"))

    def test_windows_base_env_is_minimal(self):
        env = plat.windows_base_env(WIN_ENV)
        for k in ("SystemRoot", "SYSTEMDRIVE", "TEMP", "TMP", "USERPROFILE", "APPDATA",
                  "LOCALAPPDATA", "PATHEXT", "COMSPEC"):
            self.assertIn(k, env)
        self.assertEqual(env["COMSPEC"], WIN_ENV["ComSpec"])  # sem diferenciar maiúsculas
        for k in ("LD_PRELOAD", "PYTHONPATH", "GITHUB_TOKEN", "ProgramFiles"):
            self.assertNotIn(k, env)

    def test_runner_env_on_windows(self):
        from hud import runner
        with mock.patch.object(plat, "WINDOWS", True), mock.patch.dict(os.environ, WIN_ENV):
            env = runner.safe_env()
        self.assertEqual(env["PATH"], runner.SAFE_PATH)
        self.assertEqual(env["SystemRoot"], r"C:\Windows")
        self.assertNotIn("LD_PRELOAD", env)
        self.assertNotIn("GITHUB_TOKEN", env)

    def test_agent_env_on_windows_keeps_user_path(self):
        from hud.agent import agent_env
        with mock.patch.object(plat, "WINDOWS", True), \
                mock.patch.dict(os.environ, dict(WIN_ENV, PATH=r"C:\Users\ana\AppData\Roaming\npm")):
            env = agent_env(("ANTHROPIC_",))
        self.assertEqual(env["PATH"], r"C:\Users\ana\AppData\Roaming\npm")
        self.assertEqual(env["APPDATA"], WIN_ENV["APPDATA"])
        self.assertNotIn("LD_PRELOAD", env)

    @unittest.skipIf(WINDOWS, "o SAFE_PATH do POSIX vale fora do Windows")
    def test_posix_unchanged(self):
        self.assertEqual(config.SAFE_PATH, "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")


class ForbiddenTest(unittest.TestCase):
    def test_windows_names_ignore_case_extension_and_tricks(self):
        for name in ("cmd", "CMD.EXE", "cmd.exe.", "cmd.exe ", r"C:\Windows\System32\cmd.exe",
                     "C:/Windows/System32/CMD.exe", "cmd.exe::$DATA", "powershell", "PowerShell.exe",
                     "pwsh.exe", "wsl", "bash.exe", "wscript", "cscript.exe", "mshta", "rundll32",
                     "runas", "start", "explorer.exe", "sh", "env", "sudo"):
            self.assertTrue(plat.is_forbidden(name, config.FORBIDDEN, windows=True), name)
        for name in ("ipconfig", "tasklist.exe", "systeminfo", "hostname"):
            self.assertFalse(plat.is_forbidden(name, config.FORBIDDEN, windows=True), name)

    def test_posix_names_unchanged(self):
        self.assertTrue(plat.is_forbidden("/bin/bash", config.FORBIDDEN, windows=False))
        self.assertFalse(plat.is_forbidden("cmd", config.FORBIDDEN, windows=False))
        self.assertFalse(plat.is_forbidden("Bash", config.FORBIDDEN, windows=False))

    def test_allowed_exts_follow_pathext(self):
        self.assertEqual(plat.allowed_exts(plat.COMMAND_EXTS, WIN_ENV), [".exe"])
        self.assertEqual(plat.allowed_exts(plat.AGENT_EXTS, WIN_ENV), [".exe", ".cmd"])
        self.assertEqual(plat.allowed_exts(plat.AGENT_EXTS, {"PATHEXT": ".EXE"}), [".exe"])
        self.assertEqual(plat.allowed_exts(plat.COMMAND_EXTS, {"PATHEXT": ".BAT;.CMD"}), [])

    def test_find_windows(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            for d, f in ((a, "tool.cmd"), (a, "tool.bat"), (b, "tool.exe"), (a, "only.bat")):
                Path(d, f).write_text("x")
            # Como no Windows: a ordem das pastas manda, e dentro de uma pasta a das
            # extensões; .bat nunca (não está em exts).
            Path(a, "tool.exe").write_text("x")
            self.assertEqual(plat.find_windows("tool", [a, b], [".exe", ".cmd"]), os.path.join(a, "tool.exe"))
            os.unlink(os.path.join(a, "tool.exe"))
            self.assertEqual(plat.find_windows("tool", [a, b], [".exe", ".cmd"]), os.path.join(a, "tool.cmd"))
            self.assertEqual(plat.find_windows("tool", [b, a], [".exe", ".cmd"]), os.path.join(b, "tool.exe"))
            self.assertIsNone(plat.find_windows("only", [a, b], [".exe", ".cmd"]))
            self.assertIsNone(plat.find_windows("tool.bat", [a], [".exe"]))
            self.assertIsNone(plat.find_windows(r"sub\tool", [a], [".exe"]))
            self.assertIsNone(plat.find_windows(r"\\srv\share\tool.exe", [a], [".exe"]))
            self.assertIsNone(plat.find_windows("", [a], [".exe"]))
            with mock.patch("os.getcwd", return_value=a):  # nunca a pasta atual
                self.assertIsNone(plat.find_windows("tool", [], [".exe", ".cmd"]))


class FoldersTest(unittest.TestCase):
    def test_config_and_data_paths(self):
        self.assertEqual(str(plat.default_config_path("windows", WIN_ENV)),
                         str(Path(ntpath.join(WIN_ENV["APPDATA"], "hud", "config.toml"))))
        self.assertEqual(str(plat.default_data_dir("windows", WIN_ENV)),
                         str(Path(ntpath.join(WIN_ENV["LOCALAPPDATA"], "hud"))))
        for s in ("linux", "macos"):
            self.assertEqual(plat.default_config_path(s), Path("~/.config/hud/config.toml").expanduser())
            self.assertEqual(plat.default_data_dir(s), Path("~/.local/share/hud").expanduser())

    def test_system_folders_windows(self):
        for p in ("C:\\", "D:\\", r"C:\Windows", r"c:\windows\system32", r"C:\Program Files",
                  r"C:\Program Files (x86)\App", r"D:\Program Files\x", r"C:\ProgramData\x",
                  r"\\srv\share"):
            self.assertTrue(plat.system_folder(p, "windows", WIN_ENV), p)
        for p in (r"C:\Users\ana\Vault", r"D:\notas", r"C:\Users\ana\Programas"):
            self.assertFalse(plat.system_folder(p, "windows", WIN_ENV), p)

    def test_system_folders_macos(self):
        for p in ("/", "/System", "/System/Library", "/Library/Preferences", "/private",
                  "/private/etc", "/private/var/db", "/usr", "/usr/local", "/bin", "/sbin", "/etc"):
            self.assertTrue(plat.system_folder(p, "macos"), p)
        for p in ("/Users/ana/Vault", "/Users/ana/Library/Mobile Documents", "/Volumes/dados",
                  "/private/tmp/x", "/private/var/folders/ab/T/x"):
            self.assertFalse(plat.system_folder(p, "macos"), p)

    def test_system_folders_linux_unchanged(self):
        for p in ("/", "/proc", "/etc", "/etc/ssh", "/sys/x", "/run/user"):
            self.assertTrue(plat.system_folder(p, "linux"), p)
        for p in ("/home/ana/Vault", "/usr/share/doc", "/tmp/x", "/etcetera"):
            self.assertFalse(plat.system_folder(p, "linux"), p)


class ProfileRuleTest(unittest.TestCase):
    def check(self, path):
        return plat.private_location_error(path, WIN_ENV, resolve=same, mod=ntpath, windows=True)

    def test_inside_profile_is_accepted(self):
        for p in (r"C:\Users\ana\AppData\Roaming\hud\config.toml", r"C:\Users\ana\AppData\Local\hud\agenda.md",
                  r"c:\users\ANA\appdata\local\temp\claude-sl-usage-x.json", r"C:\Users\ana\.codex\sessions\r.jsonl"):
            self.assertIsNone(self.check(p), p)

    def test_outside_profile_is_refused(self):
        for p in (r"C:\Users\Public\hud\config.toml", r"D:\hud\config.toml", r"C:\Users\ana2\x.toml",
                  r"C:\ProgramData\hud\x", r"C:\Windows\Temp\claude-sl-usage-x.json", "C:\\"):
            self.assertIsNotNone(self.check(p), p)

    def test_empty_profile_refuses_everything(self):
        self.assertIsNotNone(plat.private_location_error(r"C:\Users\ana\x", {}, resolve=same,
                                                         mod=ntpath, windows=True))

    def test_posix_has_no_location_rule(self):
        self.assertIsNone(plat.private_location_error("/srv/qualquer", windows=False))

    def test_executables(self):
        ok = (r"C:\Windows\System32\ipconfig.exe", r"C:\Program Files\nodejs\node.exe",
              r"C:\Users\ana\AppData\Roaming\npm\claude.cmd", r"C:\Users\ana\.local\bin\claude.exe")
        bad = (r"C:\Users\Public\claude.exe", r"C:\ProgramData\x\codex.exe", r"D:\tools\claude.exe",
               r"C:\tools\x.exe")
        for p in ok:
            self.assertIsNone(plat.exe_location_error(p, WIN_ENV, resolve=same, mod=ntpath), p)
        for p in bad:
            self.assertIsNotNone(plat.exe_location_error(p, WIN_ENV, resolve=same, mod=ntpath), p)

    def test_check_private_file_on_simulated_windows(self):
        with tempfile.TemporaryDirectory() as home, tempfile.TemporaryDirectory() as outro:
            f = Path(home, "config.toml")
            f.write_text("x")
            env = {"USERPROFILE": home, "APPDATA": home, "LOCALAPPDATA": home}
            with mock.patch.object(plat, "WINDOWS", True), mock.patch.object(plat, "_win_abs", os.path.isabs), \
                    mock.patch.dict(os.environ, env):
                config.check_private_file(f)
                g = Path(outro, "config.toml")
                g.write_text("x")
                with self.assertRaises(config.ConfigError):
                    config.check_private_file(g)


class ReparseTest(unittest.TestCase):
    def test_is_reparse(self):
        self.assertTrue(plat.is_reparse(SimpleNamespace(st_file_attributes=plat.REPARSE_POINT | 0x20)))
        self.assertFalse(plat.is_reparse(SimpleNamespace(st_file_attributes=0x20)))
        self.assertFalse(plat.is_reparse(SimpleNamespace()))  # POSIX: não tem o atributo
        attr = plat.REPARSE_POINT
        for tag in (0xA000000C, 0xA0000003, 0x8000001B):  # symlink, junção, alias de app
            self.assertTrue(plat.is_reparse(SimpleNamespace(st_file_attributes=attr, st_reparse_tag=tag)))
        # Arquivo sob demanda do OneDrive: reparse point, mas não leva a outro lugar.
        self.assertFalse(plat.is_reparse(SimpleNamespace(st_file_attributes=attr, st_reparse_tag=0x9000601A)))

    def test_windows_open_refuses_links_and_reparse_points(self):
        with tempfile.TemporaryDirectory() as d:
            real = Path(d, "real.txt")
            real.write_text("ok")
            fd = plat._open_windows(real, os.O_RDONLY, 0o777)
            os.close(fd)
            link = Path(d, "link.txt")
            symlink(self, real, link)
            with self.assertRaises(OSError) as cm:
                plat._open_windows(link, os.O_RDONLY, 0o777)
            self.assertEqual(cm.exception.errno, errno.ELOOP)

    def test_windows_open_refuses_reparse_attribute(self):
        with tempfile.TemporaryDirectory() as d:
            real = Path(d, "real.txt")
            real.write_text("ok")
            st = os.lstat(real)
            fake = SimpleNamespace(st_mode=st.st_mode, st_ino=st.st_ino, st_dev=st.st_dev,
                                   st_file_attributes=plat.REPARSE_POINT)
            with mock.patch("os.lstat", return_value=fake):
                with self.assertRaises(OSError) as cm:
                    plat._open_windows(real, os.O_RDONLY, 0o777)
            self.assertEqual(cm.exception.errno, errno.ELOOP)

    @posix_only
    def test_posix_open_nofollow_unchanged(self):
        with tempfile.TemporaryDirectory() as d:
            real = Path(d, "real.txt")
            real.write_text("ok")
            os.symlink(real, Path(d, "link"))
            with self.assertRaises(OSError) as cm:
                plat.open_nofollow(Path(d, "link"), os.O_RDONLY)
            self.assertEqual(cm.exception.errno, errno.ELOOP)


class ProcessTest(unittest.TestCase):
    def test_group_kwargs(self):
        with mock.patch.object(plat, "WINDOWS", True):
            kw = plat.popen_group_kwargs()
        self.assertEqual(kw, {"creationflags": plat.CREATE_NEW_PROCESS_GROUP | plat.CREATE_NO_WINDOW})
        self.assertNotIn("start_new_session", kw)
        with mock.patch.object(plat, "WINDOWS", False):
            self.assertEqual(plat.popen_group_kwargs(), {"start_new_session": True})

    def test_windows_kill_uses_fixed_taskkill(self):
        proc = mock.Mock(pid=4321)
        proc.poll.side_effect = [None, None]
        with mock.patch.object(plat, "WINDOWS", True), mock.patch.dict(os.environ, WIN_ENV), \
                mock.patch.object(subprocess, "run") as run:
            plat.kill_tree(proc)
        argv = run.call_args.args[0]
        self.assertEqual(argv, [r"C:\Windows\System32\taskkill.exe", "/T", "/F", "/PID", "4321"])
        self.assertFalse(run.call_args.kwargs.get("shell"))
        proc.kill.assert_called_once()

    def test_windows_kill_skips_finished(self):
        proc = mock.Mock(pid=1)
        proc.poll.return_value = 0
        with mock.patch.object(plat, "WINDOWS", True), mock.patch.object(subprocess, "run") as run:
            plat.kill_tree(proc)
        run.assert_not_called()

    def test_decode_output(self):
        self.assertEqual(plat.decode_output("ação".encode()), "ação")
        with mock.patch.object(plat, "WINDOWS", True):
            self.assertEqual(plat.decode_output("ação".encode()), "ação")
            self.assertEqual(plat.decode_output("ação".encode("cp850")).replace("\ufffd", "?")[0], "a")

    def test_is_root(self):
        with mock.patch.object(os, "geteuid", create=True, return_value=0):
            self.assertTrue(plat.is_root())
        with mock.patch.object(os, "geteuid", None, create=True):
            self.assertFalse(plat.is_root())  # Windows: não bloqueia


NPM_SHIM = r"""@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0

IF EXIST "%dp0%\node.exe" (
  SET "_prog=%dp0%\node.exe"
) ELSE (
  SET "_prog=node"
  SET PATHEXT=%PATHEXT:;.JS;=;%
)

endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & "%_prog%"  "%dp0%\node_modules\@anthropic-ai\claude-code\cli.js" %*
"""
OLD_SHIM = '@IF EXIST "%~dp0\\node.exe" (\r\n  "%~dp0\\node.exe"  "%~dp0\\node_modules\\@openai\\codex\\bin\\codex.js" %*\r\n)'


class ShimTest(unittest.TestCase):
    def test_parses_npm_shims(self):
        self.assertEqual(config.parse_npm_shim(NPM_SHIM), r"node_modules\@anthropic-ai\claude-code\cli.js")
        self.assertEqual(config.parse_npm_shim(OLD_SHIM), r"node_modules\@openai\codex\bin\codex.js")

    def test_refuses_unknown_shims(self):
        for text in ("@echo off\r\npowershell -c evil\r\n",
                     '"%dp0%\\node_modules\\a.js" "%dp0%\\node_modules\\b.js"',
                     '"%dp0%\\..\\..\\evil.js"',
                     '"%dp0%\\node_modules\\..\\..\\evil.js"',
                     '"%dp0%\\outro\\x.js"',
                     '"%dp0%\\node_modules\\C:\\x.js"'):
            with self.assertRaises(config.ConfigError, msg=text):
                config.parse_npm_shim(text)

    def test_launch_is_empty_off_windows_or_for_exe(self):
        self.assertEqual(config.agent_launch("/usr/bin/claude"), ())
        with mock.patch.object(plat, "WINDOWS", True):
            self.assertEqual(config.agent_launch(r"C:\Users\ana\.local\bin\claude.exe"), ())

    def test_argv_uses_launch(self):
        from hud.claude import ClaudeConfig, build_argv
        from hud.codex import CodexConfig, build_argv as codex_argv
        launch = (r"C:\Program Files\nodejs\node.exe", r"C:\x\cli.js")
        argv = build_argv(ClaudeConfig(executable=r"C:\x\claude.cmd", cwd="C:\\", launch=launch))
        self.assertEqual(argv[:3], [*launch, "-p"])
        self.assertNotIn(r"C:\x\claude.cmd", argv)
        argv = codex_argv(CodexConfig(executable=r"C:\x\codex.cmd", cwd="C:\\", launch=launch))
        self.assertEqual(argv[:3], [*launch, "exec"])


class ClaudeRulesTest(unittest.TestCase):
    def test_windows_rule_paths_are_posix_form(self):
        home = r"C:\Users\ana"
        self.assertEqual(win_rule_path(home, home), "~")
        self.assertEqual(win_rule_path(r"C:\Users\ana\Vault\Notas", home), "~/Vault/Notas")
        self.assertEqual(win_rule_path(r"c:\users\ANA\Vault", home), "~/Vault")
        self.assertEqual(win_rule_path(r"D:\dados\proj", home), "//d/dados/proj")
        self.assertEqual(win_rule_path(r"C:\Users\ana2\x", home), "//c/Users/ana2/x")


class MainTest(unittest.TestCase):
    def run_main(self, *args, tty=True):
        import contextlib
        import io
        import sys
        from hud import __main__ as m
        class Term(io.StringIO):
            def isatty(self):
                return tty

        err, out = io.StringIO(), Term()
        old = os.umask(0o022)
        os.umask(old)
        try:
            with mock.patch.object(sys, "argv", ["hud", *args]), \
                    mock.patch.object(sys, "stdin", Term()), \
                    contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
                rc = m.main()
        finally:
            os.umask(old)
        return rc, out.getvalue(), err.getvalue()

    def test_check_runs_on_every_platform(self):
        with tempfile.TemporaryDirectory() as d:
            cfg = Path(d, "c.toml")
            cfg.write_text("[claude]\nenabled = false\n[codex]\nenabled = false\n")
            if not WINDOWS:
                os.chmod(cfg, 0o600)
            rc, out, _ = self.run_main("--check", "-c", str(cfg))
        self.assertIn(rc, (0, 1))
        self.assertIn("configuração:", out)
        self.assertIn("claude: desligado", out)

    def test_missing_curses_explains_windows_curses(self):
        from hud import __main__ as m
        with tempfile.TemporaryDirectory() as d, mock.patch.object(m, "curses_available", return_value=False):
            rc, _, err = self.run_main("-c", str(Path(d, "nao-existe.toml")))
        self.assertEqual(rc, 2)
        self.assertIn("windows-curses", err)


@unittest.skipUnless(WINDOWS, "só no Windows de verdade (CI windows-latest)")
class RealWindowsTest(unittest.TestCase):
    def test_safe_path_and_defaults(self):
        root = os.environ["SystemRoot"]
        self.assertTrue(config.SAFE_PATH.lower().startswith(os.path.join(root, "System32").lower()))
        self.assertEqual(config.DEFAULT_CONFIG_PATH, Path(os.environ["APPDATA"], "hud", "config.toml"))
        self.assertEqual([c["argv"][0] for c in config.DEFAULT_COMMANDS][:2], ["systeminfo", "tasklist"])

    def test_junction_is_link(self):
        with tempfile.TemporaryDirectory() as d:
            target = Path(d, "alvo")
            target.mkdir()
            j = Path(d, "juncao")
            r = subprocess.run([os.path.join(os.environ["SystemRoot"], "System32", "cmd.exe"),
                                "/c", "mklink", "/J", str(j), str(target)], capture_output=True)
            if r.returncode != 0:
                self.skipTest("mklink /J indisponível")
            self.assertTrue(plat.is_link(j))

    def test_check_private_file_in_temp(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d, "x.toml")
            f.write_text("x")
            config.check_private_file(f)  # %TEMP% fica no perfil

    def test_vault_skips_junctions(self):
        from hud.vault import VaultWatcher
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as fora:
            Path(fora, "segredo.md").write_text("segredo")
            Path(d, "nota.md").write_text("oi")
            r = subprocess.run([os.path.join(os.environ["SystemRoot"], "System32", "cmd.exe"),
                                "/c", "mklink", "/J", str(Path(d, "j")), fora], capture_output=True)
            if r.returncode != 0:
                self.skipTest("mklink /J indisponível")
            snap = VaultWatcher(Path(d)).scan()
            self.assertEqual(snap.notes, 1)


if __name__ == "__main__":
    unittest.main()
