---
name: hud-custom
description: Cria, altera, valida e compartilha customizações de layout do HUD de terminal (repositório viralabs-dev/hud). Use quando o usuário quiser mudar o layout do HUD, mover ou redimensionar painéis, tirar ou acrescentar painéis, criar um painel próprio (texto, arquivo de log ou comando), listar as customizações existentes ou compartilhar uma customização com outras pessoas.
---

# HUD: customizações de layout

## O que é o HUD

O HUD é um painel de terminal numa tela só (Python + curses), no repositório
`viralabs-dev/hud`, normalmente clonado em `~/dev/hud`. A tela é dividida em
**colunas**, e cada coluna empilha **painéis**. Uma customização é uma pasta
`custom/<nome>/` com um `layout.toml` (obrigatório) e, se precisar, arquivos
`.md`/`.txt` para painéis de texto.

A **ENTRADA** (a linha onde o usuário digita) não faz parte do layout: fica
sempre fixa no rodapé, com 3 linhas. O layout descreve só o que fica acima dela.

### Painéis embutidos (ids reservados)

| id | o que mostra | altura padrão |
|---|---|---|
| `sistema` | CPU, memória, swap, disco, load, rede, temperatura | 11 linhas |
| `comandos` | comandos permitidos (F1–F10) | `auto`: número de comandos + 2 |
| `agenda` | agenda de hoje e próximos itens | peso 1 |
| `uso_claude` | o que resta das janelas do plano Claude | 4 linhas |
| `uso_codex` | o que resta das janelas do Codex | `auto`: janelas + 2 (mínimo 3) |
| `vault` (ou `pasta`) | o Vault do Obsidian ao vivo, ou a pasta escolhida com `/pasta` | peso 1 |
| `saida` | resultado de comandos, buscas e respostas do Claude/Codex | peso 1 |

`vault` e `pasta` são o mesmo painel: use um ou outro, nunca os dois.

## Esquema do `layout.toml`

```toml
nome = "foco"            # igual ao nome da pasta (se divergir, vale o da pasta)
descricao = "Saída grande à direita"   # até 200 caracteres
autor = "fulano"         # opcional, até 60 caracteres (um apelido, nada pessoal)

[[coluna]]               # de 1 a 4 colunas, da esquerda para a direita
largura = 34             # % da largura do terminal (opcional, de 15 a 85)
paineis = [              # de 1 a 8 painéis, de cima para baixo
  { id = "sistema", altura = 11 },   # altura fixa em linhas (3 a 60)
  "comandos",                        # só o id: usa a altura padrão da tabela
  { id = "agenda", peso = 1 },       # peso: divide a sobra da coluna
]

[[coluna]]               # sem largura: divide o resto com as outras sem largura
paineis = [
  { id = "saida", peso = 3 },
  { id = "notas", peso = 1 },
]

[[painel]]               # painel próprio, usado acima pelo id
id = "notas"
titulo = "NOTAS"         # até 30 caracteres; padrão: o id em maiúsculas
tipo = "texto"           # texto | arquivo | comando
arquivo = "notas.md"     # arquivo desta pasta
```

### Tipos de painel próprio

- **`texto`**: mostra um arquivo da própria pasta da customização.
  `arquivo = "lembretes.md"`: só o nome, sem `/`, terminando em `.md` ou `.txt`.
- **`arquivo`**: mostra as últimas linhas de um arquivo da máquina (um log).
  `caminho = "/var/log/syslog"` (absoluto ou começando com `~`) e
  `linhas = 200` (opcional; padrão 200, máximo 2000).
- **`comando`**: roda um programa de tempos em tempos e mostra a saída.
  `argv = ["docker", "ps"]` (lista, sem shell), `intervalo = 30` (segundos,
  de 5 a 3600, padrão 30) e `timeout = 10` (de 1 a 60, padrão 10).

### Regras de validação

O HUD recusa o layout (com a mensagem do erro) quando:

- tem menos de 1 ou mais de 4 colunas, ou uma coluna com 0 ou mais de 8 painéis;
- falta o painel `saida` (ele é obrigatório);
- um id aparece mais de uma vez;
- um id usado não é embutido nem está declarado em `[[painel]]`
  (um `[[painel]]` declarado e não usado só gera aviso);
- `altura` não é inteiro de 3 a 60, `peso` não é número maior que 0 e até 100,
  ou o painel tem `altura` e `peso` ao mesmo tempo;
- `largura` fica fora de 15 a 85; a soma das larguras declaradas passa de 90
  quando alguma coluna não tem largura, ou passa de 100 no total;
- num `[[painel]]`:
  - o `id` não segue `^[a-z0-9][a-z0-9_-]{0,39}$` ou é um id embutido;
  - o `titulo` tem mais de 30 caracteres;
  - o `tipo` não é `texto`, `arquivo` ou `comando`;
  - `texto` sem `arquivo` válido; `arquivo` sem `caminho`; `comando` sem `argv`;
  - o `caminho` aponta para segredo (ver *Segurança*);
  - o `argv` não passa nas regras dos comandos do HUD;
- o arquivo passa de 64 KB ou não é TOML válido.

O **nome** da customização (e da pasta) segue a mesma regra dos ids:
`^[a-z0-9][a-z0-9_-]{0,39}$` (minúsculas, números, `_` e `-`). A pasta só pode
ter `layout.toml` e arquivos `.md`/`.txt`, sem subpastas nem links simbólicos,
cada um com até 64 KB.

### Como as alturas e larguras são calculadas

- Colunas com `largura` recebem essa porcentagem; as outras dividem o resto
  igualmente. Toda coluna precisa de pelo menos 30 colunas de terminal, senão o
  HUD avisa que a tela é pequena.
- Em cada coluna, os painéis com `altura` (ou altura `auto`) recebem a sua
  altura; o resto é dividido entre os painéis com `peso`, na proporção dos pesos
  (mínimo de 3 linhas cada).
- Se a coluna não tem nenhum painel com peso, o último painel cresce até o fim.
- Em tela apertada, as alturas fixas encolhem (do último para o primeiro) até 3.

## Exemplos completos

### 1. `foco`: saída grande e lembretes

`custom/foco/layout.toml`:

```toml
nome = "foco"
descricao = "Saída grande; sistema, uso e lembretes à esquerda; Vault pequeno embaixo"

[[coluna]]
largura = 30
paineis = [
  { id = "sistema", altura = 11 },
  "uso_claude",
  "uso_codex",
  { id = "lembretes", peso = 1 },
]

[[coluna]]
paineis = [
  { id = "saida", peso = 75 },
  { id = "vault", peso = 25 },
]

[[painel]]
id = "lembretes"
titulo = "LEMBRETES"
tipo = "texto"
arquivo = "lembretes.md"
```

`custom/foco/lembretes.md`:

```markdown
# Lembretes

- Uma coisa de cada vez.
- Pausa a cada 50 minutos.
```

### 2. `monitor`: três colunas, log e portas

`custom/monitor/layout.toml`:

```toml
nome = "monitor"
descricao = "Três colunas: sistema e portas, Vault e syslog, saída e uso"

[[coluna]]
paineis = [
  { id = "sistema", altura = 11 },
  "comandos",
  { id = "portas", peso = 1 },
]

[[coluna]]
paineis = [
  { id = "vault", peso = 40 },
  { id = "agenda", peso = 20 },
  { id = "syslog", peso = 40 },
]

[[coluna]]
paineis = [
  { id = "saida", peso = 1 },
  "uso_claude",
  "uso_codex",
]

[[painel]]
id = "syslog"
titulo = "SYSLOG"
tipo = "arquivo"
caminho = "/var/log/syslog"
linhas = 100

[[painel]]
id = "portas"
titulo = "PORTAS"
tipo = "comando"
argv = ["ss", "-tulnH"]
intervalo = 30
```

O layout padrão completo, comentado, está em `custom/padrao/layout.toml` no
repositório: é o melhor ponto de partida para copiar.

## Dentro do HUD

Quando a conversa acontece **dentro do HUD** (o HUD avisa isso no contexto do
agente), **não grave arquivos**, mesmo que tenha ferramentas para isso. Responda
com um bloco de código `hud-custom` por arquivo, com o nome da customização e o
nome do arquivo na linha de abertura:

- a linha de abertura é exatamente ```` ```hud-custom nome=<nome> arquivo=<arquivo> ````;
- `arquivo` é `layout.toml` ou um `.md`/`.txt`, sem `/`;
- o conteúdo do bloco é o arquivo inteiro, não um trecho.

O HUD guarda os blocos da última resposta. Termine dizendo ao usuário para
digitar `/custom salvar` (valida e grava; se a pasta já existir, o HUD pede
confirmação) e depois `/custom <nome>` para aplicar.

Exemplo literal de resposta:

````markdown
Montei a customização `foco`: a saída fica grande à direita e, à esquerda,
ficam o sistema, o uso dos planos e um painel de lembretes.

```hud-custom nome=foco arquivo=layout.toml
nome = "foco"
descricao = "Saída grande; sistema, uso e lembretes à esquerda"

[[coluna]]
largura = 30
paineis = [
  { id = "sistema", altura = 11 },
  "uso_claude",
  "uso_codex",
  { id = "lembretes", peso = 1 },
]

[[coluna]]
paineis = [
  { id = "saida", peso = 75 },
  { id = "vault", peso = 25 },
]

[[painel]]
id = "lembretes"
titulo = "LEMBRETES"
tipo = "texto"
arquivo = "lembretes.md"
```

```hud-custom nome=foco arquivo=lembretes.md
# Lembretes

- Uma coisa de cada vez.
```

Para usar, digite `/custom salvar` e depois `/custom foco`.
````

## Fora do HUD

Num terminal comum (Claude Code ou Codex fora do HUD), grave os arquivos
direto em `<raiz do repositório>/custom/<nome>/`. A raiz é a pasta do
repositório `viralabs-dev/hud`, normalmente `~/dev/hud` (é a pasta que tem o
`pyproject.toml` e a pasta `hud/`).

1. Crie `custom/<nome>/layout.toml` (e os `.md`/`.txt` dos painéis `texto`).
2. Valide: `hud --custom-check <nome>` (ou, de dentro do repositório,
   `bin/hud --custom-check <nome>`). Corrija até não haver erro.
3. Diga ao usuário para aplicar no HUD com `/custom <nome>`.

Comandos do HUD (funcionam em qualquer modo da entrada: notas, Claude ou Codex):

| comando | faz |
|---|---|
| `/custom lista` | lista as customizações disponíveis |
| `/custom <nome>` | aplica a customização (fica lembrada) |
| `/custom padrao` | volta ao layout embutido |
| `/custom salvar` | grava os blocos `hud-custom` da última resposta do agente |

## Segurança

- **Painel `comando` só roda com confiança.** Ao aplicar uma customização com
  painel `comando`, o HUD mostra cada `argv` e pede "s". Até lá, o painel mostra
  "comando não confiado" e nada roda. Se o `layout.toml` mudar, a confiança cai
  e o HUD pede de novo. Revise o argv de customizações de terceiros antes de
  confiar.
- **`argv` sem shell.** Nada de `sh -c`, pipes, `;`, `&&` ou redirecionamento.
  Valem as mesmas proibições dos comandos do HUD: `sudo`, `su`, `doas`,
  `pkexec`, shells (`bash`, `sh`, `zsh`…), `env` e `xargs` são recusados; o
  executável é resolvido num PATH fixo e roda com ambiente mínimo.
- **`arquivo` não lê segredos.** São recusados (depois de resolver links com
  `realpath`) caminhos dentro de `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.config/gh`,
  `~/.kube`, `~/.password-store`, `~/.local/share/keyrings`, além de
  `~/.claude/.credentials.json`, `~/.claude.json`, `~/.codex/auth.json`,
  `~/.netrc`, `~/.pgpass` e arquivos `.env*`, `*.pem`, `*.key`, `id_rsa*`,
  `id_ed25519*`. Não tente contornar.
- **A pasta `custom/` vai para um repositório PÚBLICO.** Nunca ponha num
  `layout.toml` (nem nos `.md`/`.txt`) credencial, token, senha, URL com chave,
  e-mail ou caminho pessoal (`/home/<usuário>/…`). Use `~` nos caminhos e um
  apelido em `autor`, ou deixe `autor` de fora.

## Passo a passo

### Criar uma customização

1. Escolha um nome válido (ex.: `foco`, `monitor-2`).
2. Parta do padrão: copie `custom/padrao/layout.toml` e troque `nome` e
   `descricao`.
3. Ajuste colunas e painéis; garanta que `saida` está em alguma coluna.
4. Dentro do HUD: responda com blocos `hud-custom`. Fora: grave os arquivos e
   rode `hud --custom-check <nome>`.

### Mover um painel

Tire o id da lista `paineis` de uma coluna e ponha na posição desejada de outra
(a ordem da lista é a ordem de cima para baixo). Cada id aparece uma vez só.
Para tirar um painel da tela, apague o id da lista (menos `saida`).

### Acrescentar um painel próprio

1. Declare um `[[painel]]` com `id` novo (não embutido), `titulo` e `tipo`.
2. Preencha o campo do tipo: `arquivo` (texto), `caminho` (arquivo) ou `argv`
   (comando).
3. Ponha o id na lista `paineis` de alguma coluna, com `altura` ou `peso`.
4. Para `texto`, crie o `.md`/`.txt` na mesma pasta.

### Compartilhar

1. Confira que não há credencial, token nem caminho pessoal nos arquivos.
2. Valide com `hud --custom-check <nome>`.
3. No repositório, faça commit só da pasta `custom/<nome>/`
   (`git add custom/<nome>` e `git commit`) e abra um pull request.
4. Quem receber aplica com `/custom <nome>`; se houver painel `comando`, essa
   pessoa revisa e confia no HUD dela.
