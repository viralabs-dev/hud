"""Configuração e a lista de comandos permitidos.

O painel de comandos só executa o que está nesta lista: argv fixo, executável
resolvido para caminho absoluto num PATH fixo, sem shell. O arquivo de
configuração define o que roda na máquina, então ele é recusado se outro
usuário puder escrevê-lo (no Windows: se estiver fora do seu perfil).
"""

import os
import re
import shutil
import stat
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

from . import plataforma as plat
from .agent import PROFILES
from .claude import PERMISSION_MODES, ClaudeConfig
from .codex import CodexConfig
from .opencode import DEFAULT_MODEL as OPENCODE_MODEL, OpencodeConfig

if plat.WINDOWS:  # grp e pwd não existem no Windows
    grp = pwd = None
else:
    import grp
    import pwd

# Linux e macOS: o PATH de sempre. Windows: System32, o Windows, Wbem e o
# PowerShell do sistema (o powershell em si é proibido; ele só está no PATH).
SAFE_PATH = plat.safe_path()

# Nada de elevar privilégio nem abrir um interpretador de shell: com eles a
# lista deixaria de ser uma lista.
FORBIDDEN = {
    "sudo", "su", "doas", "pkexec", "run0", "sh", "bash", "dash", "zsh",
    "fish", "ksh", "csh", "tcsh", "env", "xargs", "nohup", "setsid", "script",
}
KEYS = "1234567890"
MAX_COMMANDS = len(KEYS)

DEFAULT_CONFIG_PATH = plat.default_config_path()

LINUX_COMMANDS = [
    {"name": "Uptime e usuários", "argv": ["w", "-s"]},
    {"name": "Discos", "argv": ["df", "-h", "-x", "tmpfs", "-x", "devtmpfs",
                                "-x", "squashfs", "-x", "overlay", "-x", "efivarfs"]},
    {"name": "Memória", "argv": ["free", "-h"]},
    {"name": "Endereços de rede", "argv": ["ip", "-br", "address"]},
    {"name": "Portas escutando", "argv": ["ss", "-tulnH"]},
    {"name": "Serviços com falha", "argv": ["systemctl", "--failed", "--no-pager", "--plain"]},
    {"name": "Processos por CPU", "argv": ["ps", "-eo", "pid,user,%cpu,%mem,comm",
                                           "--sort=-%cpu"], "max_lines": 15},
    {"name": "Erros do boot", "argv": ["journalctl", "-p", "err", "-b", "-n", "25",
                                       "--no-pager", "-q"], "timeout": 15},
    {"name": "Atualizações (apt)", "argv": ["apt", "list", "--upgradable"], "timeout": 30},
]

MACOS_COMMANDS = [
    {"name": "Uptime e carga", "argv": ["uptime"]},
    {"name": "Discos", "argv": ["df", "-h"]},
    {"name": "Memória", "argv": ["vm_stat"]},
    {"name": "Interfaces de rede", "argv": ["ifconfig"], "max_lines": 120},
    {"name": "Conexões e portas", "argv": ["netstat", "-an"], "max_lines": 120},
    {"name": "Processos por CPU", "argv": ["ps", "-Ao", "pid,user,%cpu,%mem,comm", "-r"],
     "max_lines": 15},
    {"name": "Bateria", "argv": ["pmset", "-g", "batt"]},
]

# argv fixo, sem shell. O wmic está obsoleto; tasklist não ordena por CPU.
WINDOWS_COMMANDS = [
    {"name": "Sistema", "argv": ["systeminfo"], "max_lines": 60, "timeout": 30},
    {"name": "Processos", "argv": ["tasklist", "/FO", "TABLE"], "max_lines": 60},
    {"name": "Conexões e portas", "argv": ["netstat", "-ano"], "max_lines": 120, "timeout": 30},
    {"name": "Endereços de rede", "argv": ["ipconfig", "/all"], "max_lines": 120},
    {"name": "Disco C:", "argv": ["fsutil", "volume", "diskfree", "C:"]},
    {"name": "Serviços", "argv": ["sc", "query", "state=", "all"], "max_lines": 120},
    {"name": "Usuário", "argv": ["whoami"]},
    {"name": "Nome da máquina", "argv": ["hostname"]},
]

PLATFORM_COMMANDS = {"linux": LINUX_COMMANDS, "macos": MACOS_COMMANDS, "windows": WINDOWS_COMMANDS}
DEFAULT_COMMANDS = PLATFORM_COMMANDS[plat.system()]


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class Command:
    key: str
    name: str
    argv: tuple[str, ...]
    timeout: float = 20.0
    confirm: bool = False
    max_lines: int = 200
    cwd: str | None = None


@dataclass
class Config:
    vault: Path
    data_dir: Path
    refresh: float = 1.0
    vault_scan: float = 5.0
    commands: list[Command] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    source: str = "padrão"
    claude: ClaudeConfig | None = None
    default_vault: Path = Path("~/Vault")
    custom_dir: Path | None = None  # None = custom/ na raiz do repositório
    folder_source: str = "config"
    codex: CodexConfig | None = None
    opencode: OpencodeConfig | None = None


def _private_group(gid: int) -> bool:
    """Grupo privado do usuário (padrão do Ubuntu): mesmo nome, sem outros membros."""
    try:
        g = grp.getgrgid(gid)
        return g.gr_name == pwd.getpwuid(os.getuid()).pw_name and not g.gr_mem
    except KeyError:
        return False


def _writable_by_others(st: os.stat_result) -> bool:
    if st.st_uid not in (0, os.getuid()) or st.st_mode & stat.S_IWOTH:
        return True
    return bool(st.st_mode & stat.S_IWGRP) and not (
        st.st_uid == os.getuid() and _private_group(st.st_gid))


def _untrusted(p: str) -> bool:
    """Executável ou pasta que outro usuário pode alterar.

    POSIX: dono e modo. Windows: o caminho resolvido precisa ficar no seu
    perfil, no Windows ou em Program Files (a ACL não é lida)."""
    if plat.WINDOWS:
        return plat.exe_location_error(p) is not None
    return _writable_by_others(os.stat(p))


def check_private_file(path: Path) -> None:
    if plat.WINDOWS:
        st = os.lstat(path)
        if stat.S_ISLNK(st.st_mode) or plat.is_reparse(st):
            raise ConfigError(f"{path} é link simbólico ou junção")
        why = plat.private_location_error(path)
        if why:
            raise ConfigError(why)
        return
    st = path.stat()
    if st.st_uid != os.getuid():
        raise ConfigError(f"{path} não pertence a você")
    if st.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise ConfigError(f"{path} pode ser escrito por outros (rode chmod 600)")


def resolve_executable(name: str) -> str:
    if plat.is_forbidden(name, FORBIDDEN):
        raise ConfigError(f"'{name}' não é permitido no painel")
    if plat.WINDOWS:
        # Só .exe (e só se o PATHEXT aceitar): .cmd/.bat passariam pelo cmd.exe.
        exe = plat.find_windows(name, SAFE_PATH.split(";"), plat.allowed_exts(plat.COMMAND_EXTS))
    else:
        exe = name if os.path.isabs(name) else shutil.which(name, path=SAFE_PATH)
    if not exe or not os.path.isfile(exe) or not os.access(exe, os.X_OK):
        raise ConfigError(f"'{name}' não encontrado ou não executável")
    exe = os.path.realpath(exe)
    if plat.is_forbidden(exe, FORBIDDEN):
        raise ConfigError(f"'{name}' aponta para '{exe}', que não é permitido")
    if plat.WINDOWS and os.path.splitext(exe)[1].lower() not in plat.COMMAND_EXTS:
        raise ConfigError(f"'{name}' aponta para '{exe}': no Windows só .exe roda no painel")
    for p in (exe, os.path.dirname(exe)):
        if _untrusted(p):
            raise ConfigError(f"'{p}' pode ser alterado por outros usuários")
    return exe


def build_command(i: int, raw: dict) -> Command:
    if not isinstance(raw, dict):
        raise ConfigError(f"comando {i + 1}: precisa ser uma tabela")
    name, argv = raw.get("name"), raw.get("argv")
    if not isinstance(name, str) or not name.strip():
        raise ConfigError(f"comando {i + 1}: falta 'name'")
    if (not isinstance(argv, list) or not argv
            or not all(isinstance(a, str) and "\x00" not in a for a in argv)):
        raise ConfigError(f"'{name}': 'argv' precisa ser lista de textos")
    timeout = raw.get("timeout", 20)
    max_lines = raw.get("max_lines", 200)
    if not isinstance(timeout, (int, float)) or not 0 < timeout <= 600:
        raise ConfigError(f"'{name}': 'timeout' entre 1 e 600 segundos")
    if not isinstance(max_lines, int) or not 1 <= max_lines <= 5000:
        raise ConfigError(f"'{name}': 'max_lines' entre 1 e 5000")
    cwd = raw.get("cwd")
    if cwd is not None:
        cwd = os.path.expanduser(str(cwd))
        if not os.path.isdir(cwd):
            raise ConfigError(f"'{name}': cwd '{cwd}' não existe")
    exe = resolve_executable(argv[0])
    return Command(
        key=KEYS[i], name=name.strip(), argv=(exe, *argv[1:]),
        timeout=float(timeout), confirm=bool(raw.get("confirm", False)),
        max_lines=max_lines, cwd=cwd,
    )


FORBIDDEN_ROOTS = plat.LINUX_FORBIDDEN_ROOTS  # macOS e Windows: plat.system_folder
FOLDER_FILE = "pasta"


def validate_folder(raw: str | Path) -> Path:
    """Pasta que o HUD pode ler ao vivo (e os agentes, quando a seguem)."""
    p = Path(os.path.realpath(os.path.expanduser(str(raw).strip())))
    if not str(raw).strip():
        raise ConfigError("informe uma pasta")
    if not p.is_dir():
        raise ConfigError(f"'{p}' não é uma pasta")
    if plat.system_folder(str(p)):
        raise ConfigError(f"'{p}' é pasta do sistema; escolha uma pasta sua")
    if not os.access(p, os.R_OK | os.X_OK):
        raise ConfigError(f"sem permissão para ler '{p}'")
    return p


def remembered_folder(data_dir: Path) -> Path | None:
    f = data_dir / FOLDER_FILE
    if not f.exists():
        return None
    check_private_file(f)
    return validate_folder(f.read_text(encoding="utf-8").strip())


def remember_folder(data_dir: Path, folder: Path | None) -> None:
    """Grava (600) a pasta escolhida com /pasta, ou esquece com None."""
    f = data_dir / FOLDER_FILE
    if folder is None:
        f.unlink(missing_ok=True)
        return
    data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    tmp = data_dir / f".{FOLDER_FILE}.tmp"
    fd = plat.open_nofollow(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(str(folder) + "\n")
    os.replace(tmp, f)


def load(path: Path | None = None, folder: str | None = None) -> Config:
    path = path or DEFAULT_CONFIG_PATH
    data: dict = {}
    warnings: list[str] = []
    source = "padrão"
    if path.exists():
        try:
            check_private_file(path)
            with path.open("rb") as f:
                data = tomllib.load(f)
            source = str(path)
        except (ConfigError, tomllib.TOMLDecodeError, OSError) as e:
            warnings.append(f"configuração ignorada: {e}")
            data = {}

    cfg = Config(
        vault=Path(os.path.expanduser(data.get("vault", "~/Vault"))),
        data_dir=Path(os.path.expanduser(data.get("data_dir", plat.default_data_dir_text()))),
        refresh=min(max(float(data.get("refresh_seconds", 1.0)), 0.5), 10.0),
        vault_scan=min(max(float(data.get("vault_scan_seconds", 5.0)), 2.0), 300.0),
        warnings=warnings,
        source=source,
    )
    cfg.default_vault = cfg.vault
    if data.get("custom_dir"):
        cfg.custom_dir = Path(os.path.realpath(os.path.expanduser(str(data["custom_dir"]))))
    # Prioridade: --pasta > a lembrada pelo /pasta > `vault` da config.
    try:
        if folder:
            cfg.vault, cfg.folder_source = validate_folder(folder), "linha de comando"
        elif (mem := remembered_folder(cfg.data_dir)) is not None:
            cfg.vault, cfg.folder_source = mem, "escolhida com /pasta"
    except (ConfigError, OSError) as e:
        warnings.append(f"pasta ignorada: {e}")
    raw_cmds = data.get("command", DEFAULT_COMMANDS)
    if len(raw_cmds) > MAX_COMMANDS:
        warnings.append(f"só os {MAX_COMMANDS} primeiros comandos entram no painel")
    slot = 0
    for raw in raw_cmds[:MAX_COMMANDS]:
        try:
            cfg.commands.append(build_command(slot, raw))
            slot += 1
        except ConfigError as e:
            warnings.append(f"comando ignorado: {e}")
    if data.get("claude", {}).get("enabled", True) is not False:
        try:
            cfg.claude = build_claude(data.get("claude", {}), cfg.vault)
            risky = [t for t in cfg.claude.tools if t.split("(")[0] in RISKY_TOOLS]
            if risky:
                warnings.append("Claude com ferramentas que alteram ou enviam dados: " + ", ".join(risky))
            if cfg.claude.profile == "completo":
                warnings.append("Claude abre no perfil completo (ferramentas, MCP e skills do seu Claude Code)")
        except ConfigError as e:
            warnings.append(f"Claude desligado: {e}")
    if data.get("codex", {}).get("enabled", True) is not False:
        try:
            cfg.codex = build_codex(data.get("codex", {}), cfg.vault)
            if cfg.codex.profile == "completo":
                warnings.append("Codex abre no perfil completo (sandbox e aprovações do seu ~/.codex)")
        except ConfigError as e:
            warnings.append(f"Codex desligado: {e}")
    if data.get("opencode", {}).get("enabled", True) is not False:
        try:
            cfg.opencode = build_opencode(data.get("opencode", {}), cfg.vault)
            if cfg.opencode.profile == "completo":
                warnings.append("OpenCode abre no perfil completo (permissões do seu opencode.json)")
        except ConfigError as e:
            if data.get("opencode"):  # só avisa quem configurou; o OpenCode é opcional
                warnings.append(f"OpenCode desligado: {e}")
    if not cfg.vault.is_dir():
        warnings.append(f"pasta não encontrada: {cfg.vault}")
    elif cfg.vault == Path.home():
        warnings.append("a pasta é a sua home inteira: varredura pesada e, se os agentes a seguirem, eles leem tudo fora da lista de segredos")
    return cfg


TOOL_NAME = re.compile(r"^[A-Za-z][A-Za-z0-9_]*(\([^\x00-\x1f]*\))?$")
# Ferramentas que escrevem, executam ou mandam dados para fora: só entram se
# você listar em [claude].tools, e o HUD avisa ao abrir.
RISKY_TOOLS = {"Bash", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch", "Agent", "Task"}


def agent_dirs() -> list[str]:
    """Windows: onde procurar os agentes — o PATH fixo, o npm global
    (%APPDATA%\\npm), ~\\.local\\bin (instalador nativo do Claude Code) e
    ~\\.opencode\\bin (instalador do OpenCode)."""
    dirs = SAFE_PATH.split(";")
    appdata = os.environ.get("APPDATA", "")
    if appdata:
        dirs.append(os.path.join(appdata, "npm"))
    dirs.append(os.path.join(os.path.expanduser("~"), ".local", "bin"))
    dirs.append(os.path.join(os.path.expanduser("~"), ".opencode", "bin"))
    return dirs


def resolve_agent(name) -> str:
    """Executável de um agente: PATH fixo, ~/.local/bin e ~/.opencode/bin, nada gravável por outros.

    No Windows: também %APPDATA%\\npm; aceita .exe e o shim .cmd do npm, que
    `agent_launch` troca por node.exe + script (nunca cmd.exe)."""
    if not isinstance(name, str) or not name:
        raise ConfigError("'executable' inválido")
    if plat.WINDOWS:
        if plat.is_forbidden(name, FORBIDDEN):
            raise ConfigError(f"'{name}' não é permitido como agente")
        exe = plat.find_windows(os.path.expanduser(name), agent_dirs(),
                                plat.allowed_exts(plat.AGENT_EXTS))
    elif os.path.isabs(os.path.expanduser(name)):
        exe = os.path.expanduser(name)
    else:
        # ~/.opencode/bin é onde o instalador oficial do OpenCode põe o binário.
        user_bins = (os.path.expanduser("~/.local/bin"), os.path.expanduser("~/.opencode/bin"))
        exe = shutil.which(name, path=":".join((SAFE_PATH, *user_bins)))
    if not exe or not os.access(exe, os.X_OK):
        raise ConfigError(f"'{name}' não encontrado")
    real = os.path.realpath(exe)
    if plat.WINDOWS and (plat.is_forbidden(real, FORBIDDEN)
                         or os.path.splitext(real)[1].lower() not in plat.AGENT_EXTS):
        raise ConfigError(f"'{name}' aponta para '{real}', que não é permitido como agente")
    for p in (real, os.path.dirname(real), os.path.dirname(exe)):
        if _untrusted(p):
            raise ConfigError(f"'{p}' pode ser alterado por outros usuários")
    return exe


# O shim .cmd que o npm gera para um pacote global termina com uma linha como
#   "%_prog%"  "%dp0%\node_modules\@anthropic-ai\claude-code\cli.js" %*
# (versões antigas usam %~dp0). Só esse formato é aceito.
NPM_SHIM_SCRIPT = re.compile(r'"%(?:dp0%|~dp0)\\?([^"%<>|&^\r\n]+?\.(?:js|cjs|mjs))"', re.I)
MAX_SHIM = 16 * 1024


def parse_npm_shim(text: str) -> str:
    """Caminho relativo (à pasta do shim) do script JS que o shim do npm roda.

    Exatamente um script, dentro de node_modules, sem `..`, `:` nem partes vazias."""
    found = {m.group(1) for m in NPM_SHIM_SCRIPT.finditer(text)}
    if len(found) != 1:
        raise ConfigError("shim .cmd não reconhecido (esperava um shim do npm)")
    rel = found.pop().replace("/", "\\")
    parts = rel.split("\\")
    if (":" in rel or ".." in parts or any(not p for p in parts)
            or parts[0].lower() != "node_modules"):
        raise ConfigError(f"shim .cmd aponta para '{rel}', fora do node_modules")
    return rel


def agent_launch(exe: str) -> tuple[str, ...]:
    """argv inicial do agente quando o executável não roda direto; () quando roda.

    Um `.cmd` no Windows exigiria `cmd.exe /c`: shell de volta, com expansão de
    %VAR% e metacaracteres nos argumentos (o prompt de sistema e as regras vão
    na linha de comando). Em vez disso, o HUD lê o shim do npm, acha o script
    JS e roda o mesmo node.exe com ele — o que o shim faria, sem cmd.exe."""
    if not plat.WINDOWS or os.path.splitext(exe)[1].lower() != ".cmd":
        return ()
    shim_dir = os.path.dirname(os.path.realpath(exe))
    try:
        fd = plat.open_nofollow(exe, os.O_RDONLY)
        with os.fdopen(fd, "rb") as fh:
            raw = fh.read(MAX_SHIM + 1)
    except OSError as e:
        raise ConfigError(f"não consegui ler o shim '{exe}': {e.strerror}") from None
    if len(raw) > MAX_SHIM:
        raise ConfigError(f"shim '{exe}' grande demais")
    rel = parse_npm_shim(raw.decode("utf-8", "replace"))
    script = os.path.normpath(os.path.join(shim_dir, rel))
    real_script = os.path.realpath(script)
    if os.path.normcase(real_script) != os.path.normcase(script):
        raise ConfigError(f"'{script}' passa por link ou junção")
    if not os.path.isfile(real_script) or _untrusted(real_script):
        raise ConfigError(f"script do shim '{script}' não encontrado ou fora do seu perfil")
    # O shim usa o node.exe ao lado dele, senão o do PATH: o mesmo aqui.
    node = plat.find_windows("node", [shim_dir] + os.environ.get("PATH", "").split(";"),
                             plat.allowed_exts((".exe",)))
    if not node:
        raise ConfigError("node.exe não encontrado (o shim .cmd do npm precisa dele)")
    node = os.path.realpath(node)
    if plat.command_name(node) != "node":
        raise ConfigError(f"'{node}' não é o node.exe")
    for p in (node, os.path.dirname(node)):
        if _untrusted(p):
            raise ConfigError(f"'{p}' pode ser alterado por outros usuários")
    return (node, real_script)


def _common(raw: dict, default_cwd: str) -> tuple[str, str, float, str]:
    cwd = os.path.expanduser(str(raw.get("cwd", default_cwd)))
    if not os.path.isdir(cwd):
        raise ConfigError(f"cwd '{cwd}' não existe")
    timeout = raw.get("timeout", 600)
    if not isinstance(timeout, (int, float)) or not 10 <= timeout <= 3600:
        raise ConfigError("'timeout' entre 10 e 3600 segundos")
    model = raw.get("model", "")
    # provedor/modelo:variante (OpenCode) cabe; começar com "-" viraria opção do agente.
    if (not isinstance(model, str) or not re.fullmatch(r"[A-Za-z0-9._:/\[\]-]*", model)
            or model.startswith("-")):
        raise ConfigError("'model' inválido")
    profile = raw.get("profile", "leitura")
    if profile not in PROFILES:
        raise ConfigError(f"'profile' precisa ser {' ou '.join(PROFILES)}")
    return cwd, model, float(timeout), profile


def build_codex(raw: dict, vault: Path | None = None) -> CodexConfig:
    if not isinstance(raw, dict):
        raise ConfigError("[codex] precisa ser uma tabela")
    exe = resolve_agent(raw.get("executable", "codex"))
    launch = agent_launch(exe)
    cwd, model, timeout, profile = _common(raw, str(vault) if vault and vault.is_dir() else "~")
    return CodexConfig(executable=exe, cwd=cwd, model=model, timeout=timeout, profile=profile,
                       follow_folder="cwd" not in raw, launch=launch)


def build_opencode(raw: dict, vault: Path | None = None) -> OpencodeConfig:
    if not isinstance(raw, dict):
        raise ConfigError("[opencode] precisa ser uma tabela")
    exe = resolve_agent(raw.get("executable", "opencode"))
    launch = agent_launch(exe)
    folder = str(vault) if vault and vault.is_dir() else "~"
    cwd, model, timeout, profile = _common({"model": OPENCODE_MODEL, **raw}, folder)
    return OpencodeConfig(executable=exe, cwd=cwd, model=model, timeout=timeout, profile=profile,
                          follow_folder="cwd" not in raw, launch=launch)


def build_claude(raw: dict, vault: Path | None = None) -> ClaudeConfig:
    if not isinstance(raw, dict):
        raise ConfigError("[claude] precisa ser uma tabela")
    exe = resolve_agent(raw.get("executable", "claude"))
    launch = agent_launch(exe)
    tools = raw.get("tools", ["Read", "Grep", "Glob"])
    if (not isinstance(tools, list) or not tools
            or not all(isinstance(t, str) and TOOL_NAME.match(t) for t in tools)):
        raise ConfigError("'tools' precisa ser lista de nomes de ferramenta")
    read_dirs = raw.get("read_dirs", [str(vault)] if vault else [])
    if not isinstance(read_dirs, list) or not all(isinstance(d, str) for d in read_dirs):
        raise ConfigError("'read_dirs' precisa ser lista de pastas")
    read_dirs = [os.path.realpath(os.path.expanduser(d)) for d in read_dirs]
    for d in read_dirs:
        if not os.path.isdir(d):
            raise ConfigError(f"read_dirs: '{d}' não existe")
        if d == "/" or (plat.WINDOWS and not os.path.splitdrive(d)[1].strip("\\/")):
            raise ConfigError("read_dirs não pode ser a raiz /")
    cwd, model, timeout, profile = _common(raw, read_dirs[0] if read_dirs else "~")
    budget = raw.get("max_budget_usd", 1.0)
    if not isinstance(budget, (int, float)) or not 0 < budget <= 50:
        raise ConfigError("'max_budget_usd' entre 0 e 50")
    mode = raw.get("full_permission_mode", "auto")
    if mode not in PERMISSION_MODES:
        raise ConfigError(f"'full_permission_mode' precisa ser um de {', '.join(PERMISSION_MODES)}"
                          " (bypassPermissions não é aceito)")
    return ClaudeConfig(executable=exe, cwd=cwd, tools=tuple(tools), model=model,
                        max_budget_usd=float(budget), timeout=timeout,
                        read_dirs=tuple(read_dirs), profile=profile, full_permission_mode=mode,
                        follow_folder="read_dirs" not in raw and "cwd" not in raw,
                        launch=launch)
