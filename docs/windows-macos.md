# HUD no Windows e no macOS: diferenças de comportamento e de segurança

O Linux é a referência e continua igual. Toda a lógica que muda por sistema
fica em `hud/plataforma.py`; os outros módulos só a chamam. Este texto é para
integrar ao README (seções Segurança e Configuração).

## Resumo por sistema

| Tema | Linux | macOS | Windows |
|---|---|---|---|
| Tela | curses (ncurses) | curses (ncurses do sistema) | `windows-curses` (PDCurses), embutido no binário; do código-fonte, `pip install windows-curses` |
| Configuração | `~/.config/hud/config.toml` | `~/.config/hud/config.toml` | `%APPDATA%\hud\config.toml` |
| Dados (agenda, notas, `pasta`, `custom`, confiança) | `~/.local/share/hud` | `~/.local/share/hud` | `%LOCALAPPDATA%\hud` |
| Customizações fora do repositório | `~/.local/share/hud/custom` | `~/.local/share/hud/custom` | `%LOCALAPPDATA%\hud\custom` |
| Modelos e skills embutidos (binário e pipx, só leitura) | `hud/_modelos`, `hud/_skills` dentro do pacote | o mesmo | o mesmo |
| Destino de `hud --instalar-skill` | `~/.claude/skills`, `~/.codex/skills` | o mesmo | `%USERPROFILE%\.claude\skills`, `%USERPROFILE%\.codex\skills` |
| PATH fixo dos comandos | `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin` | o mesmo | `%SystemRoot%\System32;%SystemRoot%;%SystemRoot%\System32\Wbem;%SystemRoot%\System32\WindowsPowerShell\v1.0` |
| Quem pode ter escrito o arquivo | dono e modo (`st_uid`, grupo/outros) | o mesmo | o lugar: só dentro do seu perfil (sem ler ACL) |
| Link no último componente | `O_NOFOLLOW` | `O_NOFOLLOW` | `lstat` recusa reparse point (link e junção) antes de abrir e confere o arquivo aberto depois |
| Grupo de processos | `start_new_session` + `killpg` | o mesmo | `CREATE_NEW_PROCESS_GROUP` + `taskkill /T /F` |
| Root/administrador | recusa root | recusa root | administrador só gera aviso |
| Vault padrão | `~/Vault` | `~/Vault` | `~\Vault` (`%USERPROFILE%\Vault`) |

### Por que XDG no macOS

O macOS usa os mesmos caminhos do Linux (`~/.config/hud` e
`~/.local/share/hud`), e não `~/Library/Application Support/hud`. Os motivos:
o `install.sh`, o README e a skill `hud-custom` já documentam esses caminhos;
ferramentas de terminal (gh, git, a própria CLI do Claude) seguem essa convenção
no macOS; e um caminho só para os dois sistemas POSIX evita um ramo a mais, com
seus próprios erros. `XDG_CONFIG_HOME`/`XDG_DATA_HOME` não são lidos, como no
Linux.

## Comandos padrão (painel de comandos)

- **Linux:** os de sempre (`w`, `df`, `free`, `ip`, `ss`, `systemctl`, `ps`,
  `journalctl`, `apt`).
- **macOS:** `uptime`, `df -h`, `vm_stat`, `ifconfig`, `netstat -an`,
  `ps -Ao pid,user,%cpu,%mem,comm -r` e `pmset -g batt`.
- **Windows** (argv fixo, sem shell): `systeminfo` (60 linhas, 30 s),
  `tasklist /FO TABLE` (não dá para ordenar por CPU), `netstat -ano`,
  `ipconfig /all`, `fsutil volume diskfree C:`, `sc query state= all`,
  `whoami` e `hostname`. O `wmic` está obsoleto e ficou de fora. Em algumas
  versões o `fsutil` pede administrador; se pedir, o painel mostra o erro e
  os outros comandos seguem.

No Windows a saída dos programas de console chega na página de código OEM
quando vai para um pipe: o HUD tenta UTF-8 e cai para a OEM.

## Segurança no Windows

### Lista de proibidos

Além dos nomes POSIX (`sudo`, `sh`, `bash`, `env`, `xargs`...), o Windows recusa
`cmd`, `powershell`, `powershell_ise`, `pwsh`, `wsl`, `bash`, `wscript`,
`cscript`, `mshta`, `rundll32`, `regsvr32`, `runas`, `start`, `explorer`,
`conhost`, `wt`, `msiexec`, `schtasks`, `at`, `forfiles`, `certutil` e
`bitsadmin` (interpretadores, hosts de script e programas que executam outro
programa). A comparação ignora maiúsculas, a extensão, pontos e espaços no fim
(`cmd.exe.` abre o cmd) e o sufixo de fluxo NTFS (`cmd.exe::$DATA`); vale para o
nome escrito e para o caminho resolvido.

### Só `.exe` no painel

No Windows, `CreateProcess` roda `.cmd` e `.bat` pelo `cmd.exe` mesmo com
`shell=False`, e o `cmd.exe` expande `%VAR%` e metacaracteres nos argumentos
(a classe de falha "BatBadBut"). Por isso o painel só aceita `.exe`. A busca
respeita o `PATHEXT` (uma extensão que o `PATHEXT` não tem não entra), procura
só nas pastas do PATH fixo e nunca na pasta atual (o `shutil.which` do Python
3.11 no Windows põe a pasta atual na frente, mesmo com `path=`; o HUD usa uma
busca própria). Caminho UNC (`\\servidor\...`) e de dispositivo são recusados.

### Agentes (`claude`, `codex`) e o shim `.cmd` do npm

Além do PATH fixo, o HUD procura em `%APPDATA%\npm` (npm global) e em
`%USERPROFILE%\.local\bin` (instalador nativo do Claude Code). Um `.exe` roda
direto. Um `.cmd` é quase sempre o shim que o npm gera, e rodá-lo exigiria
`cmd.exe /c` — o shell que o resto do HUD evita, e o prompt de sistema e as
regras de permissão vão como argumentos.

**Escolha:** o HUD nunca roda o `.cmd`. Ele lê o shim (até 16 KB, sem seguir
link), aceita só o formato do npm (exatamente um `"%dp0%\node_modules\...\x.js"`,
sem `..`, `:` nem partes vazias), confere que o script não passa por link ou
junção e fica no seu perfil, e roda `node.exe <script>` — o mesmo que o shim
faria. O `node.exe` é o da pasta do shim ou o do seu PATH, e precisa estar no
seu perfil, no Windows ou em Program Files. Shim de outro formato é recusado
com a explicação; aponte `executable` para um `.exe`. O `hud --check` mostra o
`node.exe` e o script usados.

O PATH passado aos agentes é o seu (como no Linux, por causa de hooks como o
rtk), mais `SystemRoot`, `TEMP`/`TMP`, `USERPROFILE`, `APPDATA`,
`LOCALAPPDATA`, `PATHEXT`, `COMSPEC` e afins, sem os quais o node não inicia.

### Permissões: "fora do seu perfil, recusado"

No Windows `st_uid` vem zerado e o `chmod` só liga e desliga somente-leitura,
então "recusar se outro usuário puder escrever" vira uma regra de lugar:

- **Configuração e dados** (`config.toml`, `pasta`, `custom`,
  `custom_confianca.json`, `agenda.md`, `notas.md`, o cache de uso do Claude em
  `%TEMP%`, os rollouts do Codex em `~\.codex`): aceitos só dentro de
  `%USERPROFILE%`, `%APPDATA%` ou `%LOCALAPPDATA%`, com o caminho resolvido
  (8.3, links). `C:\Users\Public`, `C:\ProgramData`, a raiz de outro drive e
  `C:\Windows\Temp` são recusados. Sem `USERPROFILE`, nada é aceito.
- **Executáveis** (comandos do painel, agentes, `node.exe`, script do shim):
  aceitos dentro do seu perfil, de `%SystemRoot%` e de `Program Files` /
  `Program Files (x86)`, onde só administradores escrevem na instalação
  padrão. Fora disso (`C:\tools`, `D:\`, `C:\ProgramData`), recusados.

Aplicado em `check_private_file`, `resolve_executable`, `resolve_agent`,
`UsageTracker.poll`, `read_codex_rollout`, `Trust`, `remembered`,
`remembered_folder`, na agenda e nas notas.

### Links e junções (reparse points)

`O_NOFOLLOW` não existe no Windows. Antes de abrir, o HUD faz `lstat` e recusa
link simbólico e qualquer reparse point (`FILE_ATTRIBUTE_REPARSE_POINT`, o que
inclui junções); depois de abrir, confere que o arquivo aberto é o mesmo
(volume e índice do `fstat` iguais aos do `lstat`). A varredura da pasta não
entra em junções (o `os.walk` entraria) e pula arquivos que sejam reparse
points.

Conta como link o reparse point que leva a outro lugar: etiqueta "name
surrogate" (link simbólico, junção) e o alias de app da Microsoft Store
(AppExecLink). Os arquivos sob demanda do OneDrive também são reparse points,
mas não apontam para outro lugar, e são lidos — senão um Vault no OneDrive
ficaria vazio. Sem a etiqueta (o `fstat` não a traz), o HUD trata como link.

### Processos

O filho nasce com `CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW` (sem console
próprio, então não desenha por cima da tela do HUD), `close_fds`, stdin fechado
e o ambiente mínimo (`PATH` fixo + `SystemRoot`, `SYSTEMDRIVE`, `WINDIR`,
`TEMP`, `TMP`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `PATHEXT`, `COMSPEC`,
`USERNAME`; nada de `LD_PRELOAD`, `PYTHONPATH`, tokens). O tempo-limite e a
interrupção de um agente rodam `%SystemRoot%\System32\taskkill.exe /T /F /PID <pid>` (argv
fixo) e, se o processo ainda estiver vivo, `TerminateProcess`. O `Popen` segura
o handle do processo, então o PID não é reaproveitado no meio do caminho. Saída
limitada a 256 KB e a `max_lines`, como no Linux.

### Pastas que o HUD não lê como "sua pasta"

- **Linux:** `/`, `/proc`, `/sys`, `/dev`, `/run`, `/boot`, `/etc`.
- **macOS:** `/`, `/System`, `/Library`, `/private`, `/usr`, `/bin`, `/sbin`,
  `/etc`, `/dev`, `/cores` (depois de resolver: `/etc` e `/var` viram
  `/private/...`). Exceções: `/private/tmp` e `/private/var/folders` (o
  `$TMPDIR` de cada usuário), como `/tmp` no Linux.
- **Windows:** a raiz de qualquer drive ou compartilhamento, `%SystemRoot%`,
  `Program Files*` em qualquer drive e `ProgramData`.

### Segredos negados ao Claude e aos painéis `arquivo`

Os mesmos do Linux (`~/.ssh`, `~/.aws`, `.env`, chaves...), e no Windows também
o cofre de credenciais (`AppData\...\Microsoft\Credentials`), as chaves DPAPI
(`AppData\Roaming\Microsoft\Protect`), o GitHub CLI e os perfis do Chrome e do
Firefox. As regras de leitura do Claude Code no Windows usam a forma POSIX
(`~/Vault/**` e `//d/dados/**`), que é como o Claude Code compara caminhos lá.
A comparação com os caminhos negados ignora maiúsculas no Windows.

### Tela (PDCurses)

`curs_set`, `mousemask`, `mouseinterval` e `use_default_colors` ficam em
`try` (sem fundo padrão, o fundo é preto); sem as constantes da roda, não há
roda. No `KEY_RESIZE` o HUD chama `resize_term(0, 0)`, que o PDCurses exige.
Alt+1/2/3 chega como uma tecla só (`ALT_1`...) e é tratado; Tab também alterna.
`ESCDELAY` não tem efeito lá (inofensivo).

## Lacunas conhecidas (Windows)

- **A ACL não é lida.** A regra é de lugar: um arquivo no seu perfil com ACL
  afrouxada de propósito, ou uma pasta em Program Files que um instalador
  deixou gravável por todos, passa. Ler a ACL exigiria `GetNamedSecurityInfo`
  via ctypes; ficou de fora.
- **Janela entre `lstat` e `open`.** A conferência do índice depois de abrir
  fecha a troca do arquivo por um link, mas não a troca de uma pasta acima por
  uma junção no meio do caminho.
- **Gravação do `/doc salvar` pelo caminho.** No Linux e no macOS cada pasta
  é criada e aberta relativa à anterior (`mkdirat`/`openat` com `O_NOFOLLOW`) e
  o temporário e o `os.replace` são relativos à pasta aberta, então trocar uma
  pasta por link no meio não leva a escrita para fora da raiz. O Python no
  Windows não tem `dir_fd`: lá a gravação continua pelo caminho, conferindo que
  a pasta resolvida fica dentro da raiz antes de abrir o temporário, logo antes
  e logo depois do `os.replace` (se depois estiver fora, a nota é apagada e o
  HUD dá erro). Isso estreita a janela, mas uma junção trocada entre essas
  conferências ainda passa; não há garantia no Windows.
- **`taskkill /T` segue a árvore por PID pai.** Um neto cujo pai já saiu não é
  encontrado. Um Job Object com `KILL_ON_JOB_CLOSE` resolveria; ficou de fora
  para não depender de ctypes no caminho de todo comando.
- **Administrador não é bloqueado**, só avisado.
- **`fsutil volume diskfree`** pode pedir administrador em algumas versões.
- **Variáveis do perfil vêm do ambiente.** `USERPROFILE`, `APPDATA` e
  `SystemRoot` são do seu processo; quem já controla seu ambiente já é você.

## O que só o CI prova

Não há Windows nem macOS na máquina em que isto foi escrito. A suíte foi
escrita para rodar nos três e o CI (`windows-latest`, `macos-latest`) é quem
prova:

- no Windows: o `taskkill` matando a árvore no tempo-limite (`ping -n 30`),
  a resolução real de `hostname`/`tasklist` em System32, o ambiente mínimo, a
  regra de perfil com `%TEMP%` do runner, a recusa de `cmd`/`.bat`, junções na
  varredura (`mklink /J`), links simbólicos (pulados se o runner não puder
  criá-los) e o `--check`;
- no macOS: a tela no pty (`tests/test_ui.py`), os comandos padrão do macOS,
  `/private/var/folders` aceito como pasta;
- só à mão: a tela no Windows Terminal/conhost com o PDCurses (cores, roda,
  redimensionar, Alt+N) e o shim do npm com o Claude Code e o Codex reais.
