# hud

HUD de terminal numa tela só, sem abas: indicadores do sistema, painel de
comandos permitidos, agenda, uso do plano Claude e do Codex, entrada/saída de texto ligada ao
Claude Code e ao Codex local, e o Vault do Obsidian ao vivo. O layout é
customizável: cada painel vai onde você quiser, e dá para criar painéis próprios.

Instala sem Python (binário para Linux amd64/arm64):

```bash
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
```

O código é Python 3.11+ só com a biblioteca padrão; o binário já traz o Python dentro.

## Telas

Execução real do HUD num pseudo-terminal (pty) de 120×40, com `TERM=xterm-256color` e teclas de verdade. Os PNGs e o GIF são quadros renderizados da gravação, não capturas de tela do desktop. Todos os dados são de demonstração: HOME, Vault, agenda, comandos e cache de uso sintéticos, e o Claude e o Codex são scripts falsos que respondem no formato real. Só os números do painel SISTEMA (CPU, memória, disco) são da máquina que gravou.

**Revisão gravada:** `289d790`, em 2026-10-08. Para gravar de novo: `python3 scripts/record-screens.py` (o HUD continua sem dependências; só a renderização usa Pillow, num ambiente virtual temporário).

![Tela inicial no modo notas: sistema, comandos, agenda, uso do Claude, Vault ao vivo e a saída.](docs/telas/01-notas.png)

Tela inicial no modo notas: sistema, comandos, agenda, uso do Claude, Vault ao vivo e a saída.

![Modo Claude (laranja): uma pergunta e a resposta do Claude falso.](docs/telas/02-claude.png)

Modo Claude (laranja): uma pergunta e a resposta do Claude falso.

![Modo Codex (cinza): a pergunta e a resposta chegando do Codex falso.](docs/telas/03-codex.png)

Modo Codex (cinza): a pergunta e a resposta chegando do Codex falso.

![/ajuda na saída.](docs/telas/04-ajuda.png)

/ajuda na saída.

![Saída rolada para cima com a roda do mouse.](docs/telas/05-rolagem.png)

Saída rolada para cima com a roda do mouse.

![/pasta com um projeto de exemplo no lugar do Vault.](docs/telas/06-pasta.png)

/pasta com um projeto de exemplo no lugar do Vault.

![Sessão inteira, animada: notas, Claude, Codex, ajuda, rolagem e /pasta.](docs/telas/hud-demo.gif)

Sessão inteira, animada: notas, Claude, Codex, ajuda, rolagem e /pasta.

A gravação crua fica em `docs/telas/hud-demo.cast` (asciinema v2), `hud-demo.pty.txt` e `hud-demo.metadata.json` (revisão, duração, teclas enviadas e sha256 do código).

## Instalar

O HUD é distribuído como um binário autocontido: o Python e o `curses` vão
dentro dele, então **não é preciso ter Python instalado**.

- **Sistemas:** Linux amd64 e arm64, com glibc 2.35 ou mais nova (Ubuntu 22.04+,
  Debian 12+, Fedora 36+ e equivalentes). No Windows, use o WSL.
- **macOS:** ainda não. O HUD lê `/proc` e `/sys`, que o macOS não tem.
- Distribuições com musl (Alpine) não rodam o binário; use o `pipx` (abaixo).

### Comando

```bash
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
export PATH="$HOME/.local/bin:$PATH"   # se ~/.local/bin ainda não estiver no PATH
hud --version
```

O instalador baixa `hud_linux_<arch>.tar.gz` da release, confere o SHA-256 com
o `checksums.txt` da mesma release (sem checksum válido, não instala), grava o
binário de forma atômica em `~/.local/bin/hud` e os avisos de licença de
terceiros em `~/.local/bin/hud-licenses/`. Ele só usa HTTPS (TLS 1.2+), não lê
nada do teclado e se recusa a escrever através de um symlink.

### Variáveis

| Variável | Padrão | Para quê |
|---|---|---|
| `HUD_VERSION` | `latest` | uma tag `vX.Y.Z` fixa uma versão |
| `HUD_INSTALL_DIR` | `~/.local/bin` | outra pasta de destino |
| `HUD_REPOSITORY` | `viralabs-dev/hud` | outro repositório (um fork) |

```bash
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh |
  HUD_VERSION=v0.5.0 bash
```

Para ler o script antes de rodar:

```bash
curl -fsSLO https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh
less install.sh
bash install.sh
```

### Atualizar

Rode o instalador de novo. Ele troca o binário e os avisos; a configuração
(`~/.config/hud/`) e os dados (`~/.local/share/hud/`) não são tocados.

### Desinstalar

```bash
rm ~/.local/bin/hud
rm -r ~/.local/bin/hud-licenses
```

A configuração e os dados ficam. Para apagar tudo: `rm -r ~/.config/hud ~/.local/share/hud`.

### Vindo da instalação antiga (symlink ou pipx)

Se `~/.local/bin/hud` é um symlink, por exemplo para o `bin/hud` do repositório,
o instalador para e diz o que fazer:

```text
hud: /home/voce/.local/bin/hud é um symlink (aponta para /home/voce/dev/hud/bin/hud).
     O instalador não escreve através de symlink. É a instalação antiga;
     remova o link e rode o instalador de novo:
       rm /home/voce/.local/bin/hud
```

Remova o link (o repositório não é afetado) e rode o instalador de novo:

```bash
ls -l ~/.local/bin/hud      # confira para onde aponta
rm ~/.local/bin/hud
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
```

Se a instalação antiga veio do `pipx`, use `pipx uninstall hud` no lugar do `rm`.

### Alternativa: pipx (para quem já tem Python 3.11+)

```bash
pipx install git+https://github.com/viralabs-dev/hud
```

Funciona também em distribuições sem glibc (musl). O comando `hud` instalado
roda o Python do venv em modo isolado (`-I`).

### Desenvolvimento

Do clone, sem instalar nada:

```bash
git clone https://github.com/viralabs-dev/hud
cd hud
bin/hud               # roda o código do repositório com python3 -I
python3 -m unittest discover -s tests -t .
```

Para montar o binário localmente (Linux, Python 3.11+ com `_curses` e
`objdump`):

```bash
python3 -m venv /tmp/hud-build
/tmp/hud-build/bin/pip install --require-hashes -r packaging/requirements-build.txt
PYTHON=/tmp/hud-build/bin/python scripts/release.sh v$(sed -n 's/^__version__ = "\(.*\)"$/\1/p' hud/__init__.py)
HUD_BIN=$PWD/build/pyinstaller/dist/hud python3 -m unittest tests.test_ui   # tela contra o binário
scripts/test-installer.sh dist/hud_linux_amd64.tar.gz                        # instalador contra o pacote
```

O glibc mínimo do binário é o do sistema onde ele é montado; por isso a release
é montada no Ubuntu 22.04. O binário extrai o interpretador numa pasta
temporária (`$TMPDIR`, ou `/tmp`) a cada execução e apaga ao sair; se o `/tmp`
for montado com `noexec`, aponte `TMPDIR` para uma pasta que permita execução.

## Rodar

```bash
hud                        # precisa de 80×24 ou mais
hud --check                # mostra a configuração e os comandos resolvidos
hud --pasta ~/dev/projeto  # lê outra pasta no lugar do Vault, só nesta execução
hud --custom-check foco    # valida uma customização
```

O binário, o `bin/hud` e o pipx ignoram `PYTHONPATH`, o site do usuário e o
diretório atual: um `json.py` ou `curses.py` plantado não é carregado
(`packaging/smoke.sh` prova isso no CI).

## Entrada

| Digite | Faz |
|---|---|
| texto livre | vira nota com data e hora em `notas.md` |
| `/ag amanhã 14h dentista` | agenda (`hoje`, `amanhã`, `+3`, `sex`, `12/10`, `2026-10-12`; hora `14h`, `9:30`) |
| `/ok N` · `/rm N` | conclui · apaga o item N da agenda |
| `/r N` · `/r nome` | roda um comando do painel (também F1–F10) |
| Alt+1 · Alt+2 · Alt+3 | entrada em notas · Claude · Codex (Tab alterna entre os três) |
| `/pasta caminho` · `/pasta vault` | lê outra pasta local no lugar do Vault (lembrada) · volta ao Vault |
| `/b termo` | busca no Vault, nome e conteúdo |
| `/notas [N]` · `/conflitos` · `/vault` | últimas notas · conflitos de sync · relê o Vault |
| `/c pergunta` · `/x pergunta` | pergunta ao Claude Code · ao Codex; a conversa continua |
| Tab | alterna notas → Claude (laranja) → Codex (cinza); no modo de um agente, texto livre vai para ele |
| `/perfil leitura\|completo` | troca o perfil do agente do modo atual (abre conversa nova) |
| `/comandos` | lista os comandos `/` do Claude Code; no modo Claude, `/qualquer` que o HUD não conhece vai para o Claude e `//nome` força |
| `/novo` · `/parar` | nova conversa · interrompe a resposta |
| `/custom lista` · `/custom nome` | lista as customizações de `custom/` · usa uma (lembrada) |
| `/custom padrao` · `/custom salvar` | volta ao layout embutido · grava a proposta que um agente fez |
| `/limpar` · `/ajuda` · `/sair` | |

Os comandos `/` do HUD valem em qualquer modo da entrada (notas, Claude ou
Codex); só os comandos `/` próprios do Claude Code são repassados ao Claude.

A roda do mouse (ou PgUp/PgDn) rola a saída, ↑/↓ percorrem o histórico, Esc limpa a linha.
Com o mouse ligado, selecione texto com Shift+arrastar.

## Vault ou outra pasta

Por padrão o painel lê o Vault do Obsidian. Para ler outra pasta local (um
projeto, uma pasta de anotações), use `/pasta caminho`: a troca é na hora e fica
lembrada para a próxima vez (`~/.local/share/hud/pasta`, 600). `/pasta vault`
volta ao Vault e esquece a escolha; `hud --pasta caminho` vale só para aquela
execução. Prioridade: `--pasta` > a escolhida com `/pasta` > `vault` da config.

Numa pasta que não é Vault, o painel se chama **PASTA · nome**, conta
`.md`, `.markdown` e `.txt` e pula `node_modules`, `venv`, `build`, `dist`,
`target`, `vendor` e pastas ocultas (até 50 000 arquivos). Pastas do sistema
(`/`, `/etc`, `/proc`, `/sys`, `/dev`, `/run`, `/boot`) são recusadas; a home
inteira é aceita com aviso.

O Claude e o Codex acompanham a pasta (é nela que trabalham e, no perfil
leitura, a única que o Claude lê), a não ser que `[claude]` tenha `read_dirs` ou
`cwd`, ou `[codex]` tenha `cwd`. A troca abre conversa nova.

### Vault

Lido a cada 5 s, relendo só as notas que mudaram. Mostra os quadros com
`kanban-plugin: board` (colunas *A fazer*, *Em andamento*, *Bloqueado*,
*Concluído*), os cards em andamento, as notas mexidas por último e os arquivos
`sync-conflict` do Syncthing. Tarefas abertas com data no formato do Obsidian
Tasks (`📅 2026-10-10`) ou do Kanban (`@{2026-10-10}`, hora `@@{15:00}`)
aparecem na agenda com ◆ e são somente leitura.

## Customização

Cada customização é uma pasta `custom/<nome>/` na raiz do repositório, para ser
compartilhada por commit/PR. O `layout.toml` diz em que coluna e em que ordem
fica cada painel (`sistema`, `comandos`, `agenda`, `uso_claude`, `uso_codex`,
`vault`, `saida`) e a altura de cada um; `[[painel]]` cria painéis próprios:

| tipo | mostra |
|---|---|
| `texto` | um `.md`/`.txt` da própria pasta da customização |
| `arquivo` | as últimas linhas de um arquivo (ex.: um log); segredos (`~/.ssh`, `.env`, chaves…) são recusados |
| `comando` | a saída de um `argv` a cada intervalo, sem shell e com as regras dos comandos do painel |

Vêm três modelos: `padrao` (a tela de sempre), `foco` (saída grande e lembretes)
e `monitor` (três colunas, syslog e portas). `/custom lista` mostra todos;
`/custom foco` usa; `/custom padrao` volta. Fora do HUD, `hud --custom-check
<nome>` valida uma pasta.

**Painel de comando de terceiros não roda sem você ver:** ao escolher uma
customização com `comando`, o HUD mostra os argv e pede `s`; a confiança fica
registrada pelo sha256 do `layout.toml` e volta a ser pedida se o arquivo mudar.
Sem confiar, o layout é usado com esses painéis desligados.

**Com o Claude ou o Codex:** a skill `hud-custom` (`skills/hud-custom/SKILL.md`)
ensina os dois a montar customizações. Para instalar nos dois:

```bash
ln -s ~/dev/hud/skills/hud-custom ~/.claude/skills/hud-custom
ln -s ~/dev/hud/skills/hud-custom ~/.codex/skills/hud-custom
```

Dentro do HUD peça, por exemplo, "deixe a saída maior e ponha o uso embaixo": o
agente responde com a proposta (blocos `hud-custom`), sem gravar nada; você
grava com `/custom salvar` e usa com `/custom <nome>`. Fora do HUD, o agente
grava direto em `custom/<nome>/`. A pasta `custom/` vai para um repositório
público: nada de credencial, token ou caminho pessoal num `layout.toml`.

## Claude Code e Codex

A entrada conversa com dois agentes, cada um com a sua cor: **Claude Code em
laranja** e **Codex em cinza** (borda e prompt da entrada, e as linhas dele na
SAÍDA). Os dois têm dois perfis:

| perfil | Claude | Codex |
|---|---|---|
| `leitura` (padrão) | só `Read`/`Grep`/`Glob` nas pastas de `read_dirs`, sem MCP, `dontAsk` | sandbox `read-only`: não escreve nem usa a rede, mas **lê o disco todo** |
| `completo` | o seu Claude Code: ferramentas, MCP, skills e comandos `/`, `--permission-mode` de `full_permission_mode` (padrão `auto`) | o seu Codex: sandbox e aprovações do `~/.codex/config.toml` |

O perfil completo é escolha sua (`/perfil completo` na sessão ou `profile` na
config); a entrada mostra **⚠ ferramentas completas** enquanto ele vale. Nos dois
perfis do Claude, os segredos continuam negados e o teto por pergunta vale. Uma
resposta curta custou US$ 0,03 no perfil leitura e US$ 0,26 no completo, que
carrega os MCP.

O Codex roda `codex exec --json` e retoma a conversa com `codex exec resume
<thread_id>`; o fim de cada resposta mostra os tokens da conversa.

### Claude

A entrada conversa com o Claude Code em modo headless (`claude -p`, stream JSON).
A resposta aparece na SAÍDA conforme chega, com as ferramentas usadas (⚙) e o
custo acumulado da conversa. Cada pergunta retoma a mesma sessão (`--resume`)
até `/novo`. Configuração em `[claude]` (ver `config.example.toml`).

Padrão seguro: só `Read`, `Grep` e `Glob`, **presos às pastas de `read_dirs`**
(padrão: o Vault; liberar `Read` sem caminho deixa ler o disco inteiro, testado),
`~/.ssh`, credenciais, `.env`, `*.pem` e afins negados sempre, nenhum servidor MCP
(`--strict-mcp-config`), `--permission-mode dontAsk` (o que não está na lista é
negado, sem perguntar), teto de US$ 1 por pergunta e o prompt passado pela entrada
padrão (nunca como argumento). Sem MCP a pergunta também sai mais barata: num
teste, um turno custou US$ 0,61 com MCP e US$ 0,12 sem.

## Uso do plano

O painel USO CODEX, abaixo, faz o mesmo com o Codex: as janelas que ele informa
(neste plano, só a semanal) vêm do `rate_limits` mais recente dos arquivos de
sessão em `~/.codex/sessions`, aceitos só se forem seus; o HUD não lê
`~/.codex/auth.json`.

O painel USO CLAUDE mostra o que **resta** da janela de 5 horas e da semanal:
percentual na frente e uma barra de 5 segmentos de 20% (▒ = segmento pela
metade), verde acima de 75%, amarelo de 25 a 75% e vermelho de 25% para baixo,
com o tempo até zerar. Os números vêm do cache do plugin `claude-usage-monitor`
do statusline (`/tmp/claude-sl-usage-*.json`, aceito só se for seu e ninguém mais
puder escrevê-lo) e dos eventos `rate_limit_event` das conversas do HUD. O HUD
nunca lê seu token OAuth.

## Segurança

- **Sem shell, só lista.** Os comandos vêm da configuração como `argv`. O
  executável é resolvido para caminho absoluto num PATH fixo; `sudo`, `su`,
  `pkexec`, shells, `env` e `xargs` são recusados, e também executáveis ou
  pastas que outro usuário possa alterar. A entrada de texto nunca vira comando.
- **Ambiente mínimo.** O processo filho recebe só PATH, HOME, USER, LANG e
  variáveis que desligam cor e pager — nada de `LD_PRELOAD`, `PYTHONPATH` ou
  tokens do seu shell. Stdin fechado, sessão própria, tempo-limite que mata o
  grupo de processos, saída limitada a 256 KB e `max_lines`.
- **Configuração protegida.** `~/.config/hud/config.toml` é ignorado se não for
  seu ou se o grupo/outros puderem escrevê-lo.
- **Texto de fora é limpo.** Saída de comando, notas do Vault e o que você digita
  perdem sequências de escape (título, clipboard OSC 52, cursor), controles C0/C1
  e caracteres bidi antes de chegar à tela.
- **O Vault é só leitura.** Links simbólicos não são seguidos; notas acima de 2 MB
  são puladas.
- **Seus dados ficam privados.** `agenda.md` e `notas.md` são gravados com
  permissão 600 numa pasta 700; a agenda é gravada de forma atômica.
- **Não roda como root**, e o lançador usa `python3 -I` (ignora `PYTHONPATH`,
  site do usuário e o diretório atual).

## Testes

```bash
python3 -m unittest discover -s tests -t .
```
