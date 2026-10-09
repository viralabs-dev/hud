"""Claude Code na entrada do HUD (`claude -p`, stream JSON).

Dois perfis:
- `leitura` (padrão): só Read/Grep/Glob presos às pastas de `read_dirs`, nenhum
  servidor MCP, `dontAsk` (o que não está liberado é negado sem perguntar).
  Uma nota do Vault com instruções escondidas não consegue mandar e-mail,
  escrever arquivo, rodar comando nem ler ~/.ssh.
- `completo`: o mesmo Claude Code do seu terminal — suas ferramentas, MCP,
  skills e comandos `/`, com o modo de permissão de `full_permission_mode`
  (padrão `auto`). Escolha consciente: `/perfil completo` ou `profile` na config.
Nos dois, a lista de segredos é negada e há teto de gasto por pergunta.
"""

import os
from dataclasses import dataclass

from .agent import Agent, describe_tool
from .text import clean, clean_line

SYSTEM_PROMPT = (
    "Você está respondendo dentro de um HUD de terminal, num painel estreito "
    "(cerca de 80 colunas). Responda em português do Brasil, direto e curto, "
    "sem tabelas largas."
)
# Liberar Read sem caminho libera o disco inteiro (testado: lê fora do cwd).
SCOPED_TOOLS = ("Read", "Grep", "Glob")
# Negados sempre, nos dois perfis, mesmo que read_dirs inclua a home.
SECRETS = ("~/.ssh/**", "~/.gnupg/**", "~/.claude/.credentials.json", "~/.claude.json",
           "~/.codex/auth.json", "~/.aws/**", "~/.azure/**", "~/.config/gcloud/**",
           "~/.config/gh/**", "~/.kube/**", "~/.docker/config.json", "~/.netrc", "~/.pgpass",
           "~/.password-store/**", "~/.local/share/keyrings/**", "~/.mozilla/**",
           "~/.config/google-chrome/**", "**/.env", "**/.env.*", "**/*.pem", "**/*.key",
           "**/id_rsa*", "**/id_ed25519*")
PERMISSION_MODES = ("default", "acceptEdits", "auto", "dontAsk", "plan")


@dataclass(frozen=True)
class ClaudeConfig:
    executable: str
    cwd: str
    tools: tuple[str, ...] = ("Read", "Grep", "Glob")
    read_dirs: tuple[str, ...] = ()
    model: str = ""
    max_budget_usd: float = 1.0
    timeout: float = 600.0
    profile: str = "leitura"
    full_permission_mode: str = "auto"
    follow_folder: bool = True  # cwd/leitura acompanham a pasta do HUD


def rule_path(d: str) -> str:
    """Caminho no formato das regras de permissão: ~/x ou //absoluto."""
    home = os.path.expanduser("~")
    d = os.path.realpath(os.path.expanduser(d))
    if d == home:
        return "~"
    if d.startswith(home + os.sep):
        return "~" + d[len(home):]
    return "/" + d


def allow_rules(cfg: ClaudeConfig) -> list[str]:
    rules = []
    for t in cfg.tools:
        if t in SCOPED_TOOLS:
            rules += [f"{t}({rule_path(d)}/**)" for d in cfg.read_dirs]
        else:
            rules.append(t)
    return rules


def build_argv(cfg: ClaudeConfig, session_id: str = "", profile: str | None = None,
               context: str = "") -> list[str]:
    profile = profile or cfg.profile
    deny = [f"{t}({p})" for t in SCOPED_TOOLS for p in SECRETS]
    argv = [cfg.executable, "-p", "--output-format", "stream-json", "--verbose"]
    if profile == "completo":
        argv += ["--permission-mode", cfg.full_permission_mode]
    else:
        names = ",".join(dict.fromkeys(t.split("(")[0] for t in cfg.tools))
        argv += ["--permission-mode", "dontAsk", "--strict-mcp-config", "--tools", names,
                 "--allowedTools", *allow_rules(cfg)]
        for d in cfg.read_dirs:
            argv += ["--add-dir", d]
    argv += ["--disallowedTools", *deny,
             "--max-budget-usd", f"{cfg.max_budget_usd:.2f}",
             "--append-system-prompt", f"{SYSTEM_PROMPT} {context}".strip()]
    if cfg.model:
        argv += ["--model", cfg.model]
    if session_id:
        argv += ["--resume", session_id]
    return argv


class Claude(Agent):
    name = "claude"
    env_prefixes = ("ANTHROPIC_", "CLAUDE_CODE_", "CLAUDE_CONFIG_DIR")

    def argv(self) -> list[str]:
        return build_argv(self.cfg, self.session.id, self.profile, self.context)

    def handle(self, ev: dict) -> dict | None:
        kind = ev.get("type")
        if kind == "system" and ev.get("subtype") == "init":
            self.session.id = str(ev.get("session_id") or self.session.id)
            self.session.model = clean_line(str(ev.get("model") or ""))
            cmds = ev.get("slash_commands")
            if isinstance(cmds, list):
                self.session.slash_commands = sorted(clean_line(str(c)) for c in cmds if c)
        elif kind == "rate_limit_event":
            self.events.put(("usage_event", ev.get("rate_limit_info")))
        elif kind == "assistant":
            content = (ev.get("message") or {}).get("content") or []
            for block in content if isinstance(content, list) else []:
                if not isinstance(block, dict):
                    continue
                if block.get("type") == "text" and block.get("text"):
                    self.emit("agent_text", clean(str(block["text"])).strip())
                elif block.get("type") == "tool_use":
                    self.emit("agent_tool", describe_tool(str(block.get("name")), block.get("input")))
        elif kind == "result":
            if ev.get("session_id"):
                self.session.id = str(ev["session_id"])
            cost = ev.get("total_cost_usd")
            if isinstance(cost, (int, float)):
                self.session.cost += cost
            self.session.turns += 1
            for d in ev.get("permission_denials") or []:
                if isinstance(d, dict):
                    self.emit("agent_denied", clean_line(str(d.get("tool_name", "?"))))
            return ev
        return None

    def finish(self, result: dict) -> tuple[bool, str, str]:
        ok = not result.get("is_error") and result.get("subtype") == "success"
        msg = "" if ok else clean_line(str(result.get("result") or result.get("subtype") or "erro"))
        return ok, msg, f"conversa US$ {self.session.cost:.2f}"
