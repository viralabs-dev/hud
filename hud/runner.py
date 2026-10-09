"""Execução dos comandos permitidos, fora da thread da tela."""

import os
import queue
import signal
import subprocess
import threading
import time

from .config import SAFE_PATH, Command
from .text import clean

OUTPUT_CAP = 256 * 1024


def safe_env() -> dict[str, str]:
    """Ambiente mínimo: nada de LD_PRELOAD, PYTHONPATH ou PATH do usuário."""
    lang = os.environ.get("LANG", "C.UTF-8")
    return {
        "PATH": SAFE_PATH,
        "HOME": os.path.expanduser("~"),
        "USER": os.environ.get("USER", ""),
        "LANG": lang,
        "LC_ALL": os.environ.get("LC_ALL", lang),
        "TERM": "dumb",
        "NO_COLOR": "1",
        "SYSTEMD_COLORS": "0",
        "SYSTEMD_PAGER": "",
        "PAGER": "cat",
        "COLUMNS": "200",
    }


class Runner:
    """Roda um comando por vez de cada tecla e publica eventos numa fila."""

    def __init__(self, events: queue.Queue):
        self.events = events
        self.running: dict[str, float] = {}
        self._lock = threading.Lock()

    def is_running(self, cmd: Command) -> bool:
        with self._lock:
            return cmd.key in self.running

    def start(self, cmd: Command) -> bool:
        with self._lock:
            if cmd.key in self.running:
                return False
            self.running[cmd.key] = time.monotonic()
        threading.Thread(target=self._work, args=(cmd,), daemon=True).start()
        return True

    def _work(self, cmd: Command) -> None:
        t0 = time.monotonic()
        rc, timed_out, truncated = None, False, False
        buf = bytearray()
        try:
            proc = subprocess.Popen(
                cmd.argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, env=safe_env(),
                cwd=cmd.cwd or os.path.expanduser("~"),
                start_new_session=True, close_fds=True, shell=False,
            )

            def kill() -> None:
                nonlocal timed_out
                timed_out = True
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

            timer = threading.Timer(cmd.timeout, kill)
            timer.start()
            try:
                while chunk := proc.stdout.read1(65536):
                    room = OUTPUT_CAP - len(buf)
                    if room > 0:
                        buf += chunk[:room]
                    if len(chunk) > room:
                        truncated = True
                rc = proc.wait()
            finally:
                timer.cancel()
                proc.stdout.close()
            text = clean(buf.decode("utf-8", "replace")).rstrip("\n")
            lines = text.split("\n") if text else []
            if len(lines) > cmd.max_lines:
                lines = lines[: cmd.max_lines]
                truncated = True
            self.events.put(("cmd_out", cmd, lines))
        except OSError as e:
            self.events.put(("cmd_out", cmd, [f"falha ao iniciar: {e.strerror}"]))
        finally:
            with self._lock:
                self.running.pop(cmd.key, None)
            self.events.put(("cmd_end", cmd, rc, time.monotonic() - t0, timed_out, truncated))
