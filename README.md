# hud

HUD de terminal numa tela só, sem abas: indicadores do sistema, painel de
comandos permitidos, agenda, uso do plano Claude, entrada/saída de texto ligada ao
Claude Code e ao Codex local, e o Vault do Obsidian ao vivo.

Python 3.11+ e só a biblioteca padrão (curses, tomllib). Nada para instalar.

```
╭ SISTEMA ─────────╮╭ VAULT · ao vivo ───────────────────────╮
│ CPU MEM SWAP / │││ notas, quadros Kanban, em andamento,   │
│ LOAD REDE TEMP   ││ recentes, conflitos de sync            │
├ COMANDOS ────────┤├ SAÍDA ─────────────────────────────────┤
│ F1–F10 / Alt+N   ││ resultado dos comandos, buscas, notas  │
├ AGENDA ──────────┤│ e as respostas do Claude               │
│ hoje, próximos   ││                                        │
├ USO CLAUDE ──────┤│                                        │
│ 5h  82% ██████▒▒ ││                                        │
╰──────────────────╯╰────────────────────────────────────────╯
╭ ENTRADA ─────────────────────────────────────────────────────╮
```

## Rodar

```bash
bin/hud              # precisa de 80×24 ou mais
bin/hud --check      # mostra a configuração e os comandos resolvidos
bin/hud --pasta ~/dev/projeto   # lê outra pasta no lugar do Vault, só nesta execução
ln -s ~/dev/hud/bin/hud ~/.local/bin/hud   # opcional, para chamar de qualquer lugar
```

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
| `/limpar` · `/ajuda` · `/sair` | |

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
