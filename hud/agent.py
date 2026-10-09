"""Base dos agentes de linha de comando (Claude Code, Codex) usados pela entrada.

Cada pergunta é um processo novo em modo headless que retoma a mesma sessão.
O prompt vai pela entrada padrão, nunca como argumento, então não pode ser
lido como opção. O processo roda em sessão própria, com tempo-limite que mata
o grupo inteiro, e o stdout é lido como JSONL.
"""

import json
import os
import signal
import subprocess
import threading
import time
from dataclasses import dataclass, field

from .text import clean, clean_line

MAX_PROMPT = 20_000
MAX_LINE = 8 * 1024 * 1024
PROFILES = ("leitura", "completo")


@dataclass
class Session:
    id: str = ""
    cost: float = 0.0
    tokens: int = 0
    turns: int = 0
    model: str = ""
    slash_commands: list[str] = field(default_factory=list)


def agent_env(prefixes: tuple[str, ...]) -> dict[str, str]:
    """O seu PATH (hooks como o rtk precisam dele) e as variáveis do agente;
    nada de LD_PRELOAD, PYTHONPATH e afins."""
    env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": os.path.expanduser("~"),
        "USER": os.environ.get("USER", ""),
        "LANG": os.environ.get("LANG", "C.UTF-8"),
        "TERM": "dumb",
        "NO_COLOR": "1",
    }
    common = ("XDG_", "HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY", "https_proxy", "http_proxy", "no_proxy")
    for k, v in os.environ.items():
        if k.startswith(prefixes + common):
            env[k] = v
    return env


def describe_tool(name: str, inp) -> str:
    for key in ("file_path", "pattern", "path", "query", "url", "command", "skill", "prompt"):
        v = inp.get(key) if isinstance(inp, dict) else None
        if isinstance(v, str) and v:
            return f"{name} {clean_line(v)}"
    return name


class Agent:
    """Uma pergunta por vez; eventos vão para a fila da tela com o nome do agente."""

    name = "agente"
    env_prefixes: tuple[str, ...] = ()

    def __init__(self, cfg, events):
        self.cfg = cfg
        self.events = events
        self.profile = cfg.profile
        self.session = Session()
        self.proc: subprocess.Popen | None = None
        self.started = 0.0
        self.stopped = False
        self._lock = threading.Lock()

    # Cada agente diz como montar a linha de comando e ler os eventos.
    def argv(self) -> list[str]:
        raise NotImplementedError

    def handle(self, ev: dict) -> dict | None:
        """Trata um evento; devolve o evento final quando a resposta termina."""
        raise NotImplementedError

    def finish(self, result: dict) -> tuple[bool, str, str]:
        """(ok, mensagem de erro, resumo) a partir do evento final."""
        raise NotImplementedError

    def emit(self, kind: str, *args) -> None:
        self.events.put((kind, self.name, *args))

    @property
    def busy(self) -> bool:
        return self.proc is not None

    def new_session(self) -> None:
        self.session = Session(slash_commands=self.session.slash_commands)

    def stop(self) -> bool:
        with self._lock:
            p = self.proc
            self.stopped = p is not None
        if p:
            try:
                os.killpg(p.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        return p is not None

    def ask(self, prompt: str) -> bool:
        if self.busy:
            return False
        try:
            proc = subprocess.Popen(
                self.argv(), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, env=agent_env(self.env_prefixes), cwd=self.cfg.cwd,
                start_new_session=True, close_fds=True, shell=False,
            )
        except OSError as e:
            self.emit("agent_end", False, f"não consegui iniciar: {e.strerror}", None, "")
            return False
        self.proc, self.started, self.stopped = proc, time.monotonic(), False
        threading.Thread(target=self._work, args=(proc, prompt[:MAX_PROMPT]), daemon=True).start()
        return True

    def _work(self, proc: subprocess.Popen, prompt: str) -> None:
        err_buf: list[bytes] = []
        drain = threading.Thread(target=lambda: err_buf.append(proc.stderr.read(64 * 1024)), daemon=True)
        drain.start()

        def kill() -> None:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(self.cfg.timeout, kill)
        timer.start()
        result = None
        try:
            try:
                proc.stdin.write(prompt.encode())
                proc.stdin.close()
            except BrokenPipeError:
                pass
            for raw in proc.stdout:
                if len(raw) > MAX_LINE:
                    continue
                try:
                    ev = json.loads(raw)
                except ValueError:
                    continue
                if isinstance(ev, dict):
                    result = self.handle(ev) or result
            proc.wait()
        finally:
            timer.cancel()
            drain.join(timeout=2)
            proc.stdout.close()
            with self._lock:
                self.proc = None
        dur = time.monotonic() - self.started
        if result is not None:
            ok, msg, summary = self.finish(result)
            self.emit("agent_end", ok, msg, dur, summary)
        elif self.stopped:
            self.emit("agent_end", False, "interrompido", dur, "")
        else:
            tail = clean(b"".join(err_buf).decode("utf-8", "replace")).strip().splitlines()[-3:]
            msg = " · ".join(tail) or f"saiu com código {proc.returncode}"
            self.emit("agent_end", False, msg, dur, "")
