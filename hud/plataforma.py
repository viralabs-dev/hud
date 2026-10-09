"""O que muda entre Linux, macOS e Windows, num lugar só.

Linux é a referência: em Linux cada função aqui faz exatamente o que o HUD já
fazia. macOS é POSIX e quase tudo é igual (muda a lista de pastas do sistema e
os comandos padrão). Windows é o caso diferente:

- não há grupo de processos POSIX: o filho nasce num grupo próprio
  (`CREATE_NEW_PROCESS_GROUP`) e a árvore é encerrada com `taskkill /T /F`;
- `st_uid` e o `chmod` não dizem quem pode escrever: a regra "recusar se outro
  usuário puder escrever" vira "aceitar só dentro do seu perfil" (dados e
  configuração) ou "só no perfil, no Windows e em Program Files" (executáveis).
  A ACL não é lida (lacuna documentada em docs/windows-macos.md);
- não há `O_NOFOLLOW`: links simbólicos e junções (reparse points) são
  recusados com `lstat` antes de abrir e conferidos de novo depois;
- `.cmd`/`.bat` passam pelo `cmd.exe` mesmo com `shell=False`, então nunca
  rodam como comando do painel.

As funções puras recebem `env`/`system` para os testes simularem o Windows
em qualquer máquina.
"""

import errno
import ntpath
import os
import posixpath
import signal
import stat
import subprocess
import sys
from pathlib import Path

WINDOWS = sys.platform == "win32"
MACOS = sys.platform == "darwin"

O_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
O_NONBLOCK = getattr(os, "O_NONBLOCK", 0)
O_CLOEXEC = getattr(os, "O_CLOEXEC", 0)
O_BINARY = getattr(os, "O_BINARY", 0)
O_NOINHERIT = getattr(os, "O_NOINHERIT", 0)
REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
CREATE_NEW_PROCESS_GROUP = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0x200)
CREATE_NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000)

POSIX_SAFE_PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

# Windows: além dos nomes POSIX (sh, bash, env...), interpretadores, hosts de
# script e o que abre outro programa. Comparados sem maiúsculas e sem extensão.
FORBIDDEN_WINDOWS = {
    "cmd", "powershell", "powershell_ise", "pwsh", "wsl", "bash", "wscript", "cscript",
    "mshta", "rundll32", "runas", "start", "explorer", "conhost", "wt", "regsvr32",
    "msiexec", "schtasks", "at", "forfiles", "certutil", "bitsadmin",
}
# Só `.exe` roda como comando do painel. `.cmd`/`.bat` passam pelo cmd.exe mesmo
# com shell=False (e com ele a expansão de %VAR% e metacaracteres nos argumentos).
COMMAND_EXTS = (".exe",)
# Agentes: `.exe` direto; `.cmd` só como shim do npm, rodado como node.exe + script.
AGENT_EXTS = (".exe", ".cmd")

# Variáveis sem as quais muitos programas do Windows nem iniciam.
WINDOWS_BASE_ENV = ("SystemRoot", "SYSTEMDRIVE", "WINDIR", "TEMP", "TMP", "USERPROFILE",
                    "APPDATA", "LOCALAPPDATA", "PATHEXT", "COMSPEC", "USERNAME")

LINUX_FORBIDDEN_ROOTS = ("/", "/proc", "/sys", "/dev", "/run", "/boot", "/etc")
MACOS_FORBIDDEN_ROOTS = ("/", "/System", "/Library", "/private", "/usr", "/bin", "/sbin",
                         "/etc", "/dev", "/cores")
# /private/tmp e /private/var/folders (o $TMPDIR de cada usuário) são do usuário,
# como /tmp no Linux.
MACOS_ALLOWED_UNDER = ("/private/tmp", "/private/var/folders")


def system() -> str:
    return "windows" if WINDOWS else "macos" if MACOS else "linux"


def _get(env, name: str, default: str = "") -> str:
    """Leitura sem diferenciar maiúsculas (como o ambiente do Windows)."""
    env = os.environ if env is None else env
    if name in env:
        return env[name]
    low = name.lower()
    for k, v in env.items():
        if k.lower() == low:
            return v
    return default


def _win_abs(p: str) -> bool:
    """Caminho absoluto local (C:\\...), sem UNC nem caminho de dispositivo."""
    if not p or p.startswith(("\\\\", "//")):
        return False
    drive, rest = ntpath.splitdrive(p)
    return len(drive) == 2 and drive[1] == ":" and rest[:1] in ("\\", "/")


def system_root(env=None) -> str:
    for name in ("SystemRoot", "WINDIR"):
        v = _get(env, name)
        if _win_abs(v):
            return ntpath.normpath(v)
    return "C:\\Windows"


def windows_safe_path(env=None) -> str:
    root = system_root(env)
    return ";".join([
        ntpath.join(root, "System32"),
        root,
        ntpath.join(root, "System32", "Wbem"),
        ntpath.join(root, "System32", "WindowsPowerShell", "v1.0"),
    ])


def safe_path() -> str:
    return windows_safe_path() if WINDOWS else POSIX_SAFE_PATH


def windows_base_env(environ=None) -> dict[str, str]:
    out = {}
    for name in WINDOWS_BASE_ENV:
        v = _get(environ, name)
        if v:
            out[name] = v
    return out


# ------------------------------------------------------------------ nomes

def command_name(name: str, windows: bool | None = None) -> str:
    """Nome para comparar com a lista de proibidos.

    POSIX: o basename, como sempre. Windows: sem maiúsculas, sem fluxo NTFS
    (`x:$DATA`), sem pontos e espaços no fim (o Windows os ignora: `cmd.exe.`
    abre o cmd) e sem a extensão.
    """
    windows = WINDOWS if windows is None else windows
    if not windows:
        return os.path.basename(name)
    base = ntpath.basename(name.replace("/", "\\"))
    base = base.split(":", 1)[0].rstrip(". ").lower()
    stem, ext = ntpath.splitext(base)
    return (stem if ext else base).rstrip(". ")


def is_forbidden(name: str, forbidden, windows: bool | None = None) -> bool:
    windows = WINDOWS if windows is None else windows
    n = command_name(name, windows)
    return n in forbidden or (windows and n in FORBIDDEN_WINDOWS)


def pathext(env=None) -> list[str]:
    raw = _get(env, "PATHEXT") or ".COM;.EXE;.BAT;.CMD"
    return [e.strip().lower() for e in raw.split(";") if e.strip().startswith(".")]


def allowed_exts(allowed, env=None) -> list[str]:
    """As extensões permitidas que o PATHEXT do usuário também aceita, na ordem do PATHEXT."""
    return [e for e in pathext(env) if e in allowed]


def find_windows(name: str, dirs, exts) -> str | None:
    """`shutil.which` do Windows, sem a pasta atual e só com as extensões dadas.

    (No Python 3.11 o `shutil.which` do Windows põe a pasta atual na frente do
    PATH mesmo com `path=`; aqui ela nunca entra.)
    """
    if not name or "\x00" in name:
        return None
    exts = [e.lower() for e in exts]
    ext = ntpath.splitext(name)[1].lower()
    names = [name] if ext in exts else [name + e for e in exts]
    if name.startswith(("\\\\", "//")):
        return None  # UNC ou caminho de dispositivo: nada de executável da rede
    if _win_abs(name) or os.path.isabs(name):
        return next((n for n in names if os.path.isfile(n)), None)
    if "\\" in name or "/" in name:
        return None  # relativo com pasta: nunca
    for d in dirs:
        if not d:
            continue
        for n in names:
            cand = os.path.join(d, n)
            if os.path.isfile(cand):
                return cand
    return None


# ------------------------------------------------------------------ pastas

def default_config_path(system_name: str | None = None, env=None) -> Path:
    system_name = system_name or system()
    if system_name == "windows":
        base = _get(env, "APPDATA") or ntpath.join(os.path.expanduser("~"), "AppData", "Roaming")
        return Path(ntpath.join(base, "hud", "config.toml"))
    # Linux e macOS: o mesmo caminho XDG (veja docs/windows-macos.md).
    return Path("~/.config/hud/config.toml").expanduser()


def default_data_dir(system_name: str | None = None, env=None) -> Path:
    system_name = system_name or system()
    if system_name == "windows":
        base = _get(env, "LOCALAPPDATA") or ntpath.join(os.path.expanduser("~"), "AppData", "Local")
        return Path(ntpath.join(base, "hud"))
    return Path("~/.local/share/hud").expanduser()


def default_data_dir_text() -> str:
    """Valor padrão de `data_dir` (expandido por quem lê, como antes)."""
    return str(default_data_dir()) if WINDOWS else "~/.local/share/hud"


def _norm_win(p: str) -> str:
    return ntpath.normcase(ntpath.normpath(p))


def inside(path: str, roots, mod=None) -> bool:
    """`path` é uma das raízes ou fica dentro de uma delas (Windows: sem maiúsculas)."""
    mod = mod or (ntpath if WINDOWS else posixpath)
    norm = _norm_win if mod is ntpath else posixpath.normpath
    p = norm(path)
    for r in roots:
        if not r:
            continue
        r = norm(r)
        if p == r or p.startswith(r.rstrip(mod.sep) + mod.sep):
            return True
    return False


def profile_roots(env=None) -> list[str]:
    return [v for v in (_get(env, "USERPROFILE"), _get(env, "APPDATA"), _get(env, "LOCALAPPDATA"))
            if _win_abs(v)]


def exe_roots(env=None) -> list[str]:
    """Onde um executável pode morar no Windows: seu perfil, o Windows e Program Files
    (só administradores escrevem nos dois últimos, na instalação padrão)."""
    roots = profile_roots(env) + [system_root(env)]
    for name in ("ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"):
        v = _get(env, name)
        if _win_abs(v):
            roots.append(v)
    return roots


def _real(p: str) -> str:
    return os.path.realpath(p)


def location_error(path, roots, what: str, resolve=_real, mod=None) -> str | None:
    """Motivo da recusa se `path` (resolvido) cai fora de `roots` (também resolvidas)."""
    real = resolve(str(path))
    rr = [resolve(r) for r in roots]
    if not rr:
        return f"{path}: não sei onde fica o seu perfil (USERPROFILE vazio)"
    if inside(real, rr, mod):
        return None
    return f"{path} fica fora {what}"


def private_location_error(path, env=None, resolve=_real, mod=None,
                           windows: bool | None = None) -> str | None:
    """Windows: arquivo de dados/configuração precisa ficar no seu perfil.
    POSIX: None (lá valem dono e modo, checados por quem chama)."""
    windows = WINDOWS if windows is None else windows
    if not windows:
        return None
    return location_error(path, profile_roots(env), "do seu perfil (USERPROFILE, APPDATA, "
                          "LOCALAPPDATA); outros usuários podem escrever ali", resolve, mod)


def exe_location_error(path, env=None, resolve=_real, mod=None) -> str | None:
    return location_error(path, exe_roots(env), "do seu perfil, do Windows e de Program Files; "
                          "outros usuários podem alterá-lo", resolve, mod)


# Etiquetas de reparse point: "name surrogate" (link simbólico, junção) aponta
# para outro lugar; AppExecLink é o alias de app da Microsoft Store. Arquivos sob
# demanda do OneDrive também são reparse points, mas não apontam para outro
# lugar: esses são lidos (senão um Vault no OneDrive ficaria vazio).
NAME_SURROGATE = 0x20000000
IO_REPARSE_TAG_APPEXECLINK = 0x8000001B


def is_reparse(st) -> bool:
    """Reparse point que leva a outro lugar (link, junção, alias de app)."""
    if not getattr(st, "st_file_attributes", 0) & REPARSE_POINT:
        return False
    tag = getattr(st, "st_reparse_tag", 0)
    if not tag:
        return True  # sem a etiqueta (fstat): na dúvida, é link
    return bool(tag & NAME_SURROGATE) or tag == IO_REPARSE_TAG_APPEXECLINK


def is_link(path) -> bool:
    """Link simbólico ou, no Windows, qualquer reparse point (junção incluída)."""
    try:
        st = os.lstat(path)
    except OSError:
        return False
    return stat.S_ISLNK(st.st_mode) or is_reparse(st)


def foreign(st, path) -> bool:
    """Arquivo que outro usuário pode ter escrito (ou trocado por um link)."""
    if not WINDOWS:
        return st.st_uid != os.getuid() or bool(st.st_mode & stat.S_IWOTH)
    return is_reparse(st) or private_location_error(path) is not None


def system_folder(path: str, system_name: str | None = None, env=None) -> bool:
    """Pasta do sistema que o HUD não lê como "sua pasta" (caminho já resolvido)."""
    system_name = system_name or system()
    if system_name == "linux":
        return path in LINUX_FORBIDDEN_ROOTS or any(
            path.startswith(r + "/") for r in LINUX_FORBIDDEN_ROOTS[1:])
    if system_name == "macos":
        if inside(path, MACOS_ALLOWED_UNDER, posixpath):
            return False
        return path == "/" or inside(path, MACOS_FORBIDDEN_ROOTS[1:], posixpath)
    drive, rest = ntpath.splitdrive(path)
    if not rest.strip("\\/"):
        return True  # raiz de um drive (C:\, D:\) ou de um compartilhamento
    roots = [system_root(env)]
    for name in ("ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"):
        v = _get(env, name)
        if _win_abs(v):
            roots.append(v)
    sysdrive = _get(env, "SYSTEMDRIVE") or "C:"
    roots += [sysdrive + "\\ProgramData", sysdrive + "\\Windows"]
    if inside(path, roots, ntpath):
        return True
    first = rest.strip("\\/").split("\\")[0].split("/")[0].lower()
    return bool(drive) and first.startswith("program files")


# ------------------------------------------------------------------ arquivos

def open_nofollow(path, flags: int, mode: int = 0o777) -> int:
    """`os.open` sem seguir link no último componente.

    POSIX: `O_NOFOLLOW` (ELOOP se for link), como sempre. Windows: recusa
    reparse point antes de abrir e confere depois que o que abriu é o mesmo
    arquivo (mesmo volume e índice), também com ELOOP.
    """
    if not WINDOWS:
        return os.open(path, flags | O_NOFOLLOW, mode)
    return _open_windows(path, flags, mode)


def _open_windows(path, flags: int, mode: int) -> int:
    def refuse():
        raise OSError(errno.ELOOP, "é link simbólico ou junção", str(path))

    try:
        before = os.lstat(path)
    except FileNotFoundError:
        before = None
    if before is not None and (stat.S_ISLNK(before.st_mode) or is_reparse(before)):
        refuse()
    fd = os.open(path, flags | O_BINARY | O_NOINHERIT, mode)
    try:
        opened = os.fstat(fd)
        after = os.lstat(path)
        same = (after.st_ino, after.st_dev) == (opened.st_ino, opened.st_dev) or not opened.st_ino
        if stat.S_ISLNK(after.st_mode) or is_reparse(after) or not same:
            refuse()
    except BaseException:
        os.close(fd)
        raise
    return fd


def decode_output(data: bytes) -> str:
    """Saída de comando em texto. No Windows, programas de console escrevem na
    página de código OEM quando a saída é um pipe: tenta UTF-8 e cai para ela."""
    if not WINDOWS:
        return data.decode("utf-8", "replace")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        pass
    try:
        return data.decode("oem", "replace")
    except LookupError:
        return data.decode("cp850", "replace")


# ------------------------------------------------------------------ processos

def popen_group_kwargs() -> dict:
    """Processo filho em grupo próprio, para encerrar a árvore inteira depois."""
    if WINDOWS:
        return {"creationflags": CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW}
    return {"start_new_session": True}


def taskkill_argv(pid: int, env=None) -> list[str]:
    return [ntpath.join(system_root(env), "System32", "taskkill.exe"), "/T", "/F", "/PID", str(int(pid))]


def kill_tree(proc, force: bool = True) -> None:
    """Encerra o processo e os filhos dele.

    POSIX: sinal no grupo (SIGKILL, ou SIGTERM com force=False). Windows:
    `taskkill /T /F` (argv fixo, caminho absoluto) e, se ainda estiver vivo,
    TerminateProcess. O Popen segura o handle do processo, então o PID não é
    reaproveitado enquanto isso.
    """
    if not WINDOWS:
        try:
            os.killpg(proc.pid, signal.SIGKILL if force else signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            pass
        return
    if proc.poll() is not None:
        return
    try:
        subprocess.run(taskkill_argv(proc.pid), stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, env=windows_base_env(), close_fds=True,
                       shell=False, timeout=10, check=False, creationflags=CREATE_NO_WINDOW)
    except (OSError, subprocess.SubprocessError):
        pass
    try:
        if proc.poll() is None:
            proc.kill()
    except OSError:
        pass


def is_root() -> bool:
    """POSIX: uid efetivo 0. Windows: não bloqueia (veja is_admin)."""
    geteuid = getattr(os, "geteuid", None)
    return bool(geteuid) and geteuid() == 0


def is_admin() -> bool:
    """Windows: processo elevado (só para avisar)."""
    if not WINDOWS:
        return False
    try:
        import ctypes
        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except (AttributeError, OSError):
        return False
