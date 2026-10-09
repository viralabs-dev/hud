"""Codex CLI local na entrada do HUD (`codex exec --json`).

Perfis:
- `leitura` (padrão): sandbox `read-only` — os comandos que o Codex roda não
  escrevem em disco nem acessam a rede. Atenção: o sandbox read-only deixa ler
  qualquer arquivo que você possa ler; não há como prender a leitura a uma pasta.
- `completo`: o Codex como no seu terminal, com o sandbox e as aprovações do
  seu ~/.codex/config.toml.
A conversa continua com `codex exec resume <thread_id>` até /novo.
"""

from dataclasses import dataclass

from .agent import Agent
from .text import clean, clean_line


@dataclass(frozen=True)
class CodexConfig:
    executable: str
    cwd: str
    model: str = ""
    timeout: float = 600.0
    profile: str = "leitura"
    follow_folder: bool = True  # cwd/leitura acompanham a pasta do HUD


def build_argv(cfg: CodexConfig, thread_id: str = "", profile: str | None = None) -> list[str]:
    profile = profile or cfg.profile
    argv = [cfg.executable, "exec"]
    if thread_id:
        argv += ["resume", thread_id]
    argv += ["--json", "--skip-git-repo-check"]
    if profile != "completo":
        # Vale para exec e para resume (resume não aceita -s).
        argv += ["-c", 'sandbox_mode="read-only"']
    if cfg.model:
        argv += ["-m", cfg.model]
    return argv + ["-"]  # prompt pela entrada padrão


class Codex(Agent):
    name = "codex"
    env_prefixes = ("OPENAI_", "CODEX_")

    def argv(self) -> list[str]:
        return build_argv(self.cfg, self.session.id, self.profile)

    def prepare(self, prompt: str) -> str:
        if self.context and not self.session.id:
            return f"[Contexto do HUD: {self.context}]\n\n{prompt}"
        return prompt

    def handle(self, ev: dict) -> dict | None:
        kind = ev.get("type")
        if kind == "thread.started" and ev.get("thread_id"):
            self.session.id = str(ev["thread_id"])
        elif kind == "item.started":
            item = ev.get("item") or {}
            if item.get("type") == "command_execution":
                self.emit("agent_tool", f"$ {clean_line(str(item.get('command', '')))}")
        elif kind == "item.completed":
            item = ev.get("item") or {}
            t = item.get("type")
            if t == "agent_message" and item.get("text"):
                self.emit("agent_text", clean(str(item["text"])).strip())
            elif t == "command_execution" and item.get("exit_code") not in (0, None):
                self.emit("agent_denied", f"comando saiu com {item.get('exit_code')}")
            elif t == "file_change":
                self.emit("agent_tool", f"✎ {clean_line(str(item.get('changes', '')))[:120]}")
            elif t == "error" and item.get("message"):
                self.emit("agent_denied", clean_line(str(item["message"])))
        elif kind == "turn.completed":
            usage = ev.get("usage") or {}
            n = usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
            if isinstance(n, int):
                self.session.tokens += n
            self.session.turns += 1
            return ev
        elif kind in ("turn.failed", "error"):
            return ev
        return None

    def finish(self, result: dict) -> tuple[bool, str, str]:
        if result.get("type") == "turn.completed":
            return True, "", f"conversa {self.session.tokens / 1000:.0f}k tokens"
        err = result.get("error") or result.get("message") or "falhou"
        if isinstance(err, dict):
            err = err.get("message", "falhou")
        return False, clean_line(str(err)), ""
