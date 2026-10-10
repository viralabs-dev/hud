"""OpenCode local na entrada do HUD (`opencode run --format json`).

Perfis:
- `leitura` (padrão): as ferramentas que escrevem ou rodam coisas (edição, bash,
  web, pastas fora da pasta do HUD) ficam em `ask`. No `opencode run` não há quem
  aprove, então todo pedido de permissão é recusado na hora ("auto-rejecting").
  Não usamos `deny`: ele tira a ferramenta da lista, e o plano gratuito do
  OpenCode recusa a chamada quando a lista muda. Arquivos de segredo ficam em
  `deny` na leitura.
- `completo`: o OpenCode como no seu terminal, com as permissões do seu
  opencode.json (o que lá estiver em `ask` continua recusado, pela mesma razão).
O modelo vem de `model` (provedor/modelo); sem ele o `opencode run` pode ficar
esperando um modelo padrão que não existe. A conversa continua com
`--session <id>` até /novo.
"""

import json
from dataclasses import dataclass

from .agent import Agent, describe_tool
from .text import clean, clean_line

DEFAULT_MODEL = "opencode/big-pickle"  # gratuito, sem credencial

# Leitura: nada escreve, roda comando nem sai para a rede. `read` nega segredos
# pelo nome; o resto da leitura vale só dentro da pasta (external_directory).
SECRET_GLOBS = ("*.env", "*.env.*", ".env*", "*.pem", "*.key", "id_rsa*", "id_ed25519*",
                "*credentials*", "*secret*", ".netrc", ".npmrc", ".pypirc")


@dataclass(frozen=True)
class OpencodeConfig:
    executable: str
    cwd: str
    model: str = DEFAULT_MODEL
    timeout: float = 600.0
    profile: str = "leitura"
    follow_folder: bool = True  # cwd acompanha a pasta do HUD
    launch: tuple[str, ...] = ()  # Windows, shim .cmd do npm: (node.exe, script.js)
    read_dirs: tuple[str, ...] = ()  # pastas fora do cwd que o perfil leitura pode ler


def read_only_config(read_dirs: tuple[str, ...] = ()) -> str:
    """O `OPENCODE_CONFIG_CONTENT` do perfil leitura."""
    outside = {f"{d.rstrip('/')}/**": "allow" for d in read_dirs}
    outside["*"] = "ask"
    read = {"*": "allow", **{g: "deny" for g in SECRET_GLOBS}}
    return json.dumps({"permission": {
        "edit": "ask", "bash": "ask", "webfetch": "ask", "websearch": "ask", "task": "ask",
        "external_directory": outside, "read": read,
    }}, sort_keys=True)


def build_argv(cfg: OpencodeConfig, session_id: str = "") -> list[str]:
    argv = [*(cfg.launch or (cfg.executable,)), "run", "--format", "json"]
    if cfg.model:
        argv += ["-m", cfg.model]
    if session_id:
        argv += ["--session", session_id]
    return argv  # prompt pela entrada padrão


class Opencode(Agent):
    name = "opencode"
    env_prefixes = ("OPENCODE_",)

    def argv(self) -> list[str]:
        return build_argv(self.cfg, self.session.id)

    def extra_env(self) -> dict[str, str]:
        if self.profile == "completo":
            return {}
        return {"OPENCODE_CONFIG_CONTENT": read_only_config(self.cfg.read_dirs)}

    def prepare(self, prompt: str) -> str:
        if self.context and not self.session.id:
            return f"[Contexto do HUD: {self.context}]\n\n{prompt}"
        return prompt

    def handle(self, ev: dict) -> dict | None:
        kind = ev.get("type")
        if ev.get("sessionID") and not self.session.id:
            self.session.id = str(ev["sessionID"])
        part = ev.get("part") or {}
        if kind == "text" and part.get("text"):
            text = clean(str(part["text"])).strip()
            if text:
                self.emit("agent_text", text)
        elif kind == "tool_use":
            state = part.get("state") or {}
            what = describe_tool(str(part.get("tool", "?")), state.get("input") or {})
            if state.get("status") == "error":
                self.emit("agent_denied", f"{what}: {clean_line(str(state.get('error', '')))[:120]}")
            else:
                self.emit("agent_tool", what)
        elif kind == "step_finish":
            tokens = (part.get("tokens") or {}).get("total")
            if isinstance(tokens, int):
                self.session.tokens += tokens
            if part.get("reason") == "stop":  # "tool-calls" = passo intermediário
                self.session.turns += 1
                return ev
        elif kind == "error":
            return ev
        return None

    def finish(self, result: dict) -> tuple[bool, str, str]:
        if result.get("type") == "step_finish":
            return True, "", f"conversa {self.session.tokens / 1000:.0f}k tokens"
        err = result.get("error") or "falhou"
        if isinstance(err, dict):
            err = (err.get("data") or {}).get("message") or err.get("name") or "falhou"
        return False, clean_line(str(err)), ""
