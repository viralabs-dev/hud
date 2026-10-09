---
name: projeto-docs
description: Cria e mantém a documentação de projetos numa pasta de notas em Markdown (um Vault do Obsidian ou outra pasta do usuário), no modelo de pasta de projeto com nota-raiz, arquitetura, CI, distribuição, operação, decisões (ADRs), Kanban vivo com Plano de entrega, atividades e referência. Use quando o usuário quiser criar um projeto novo (/project), planejar ou criar documentação de um projeto (/doc), atualizar Kanban, atividades, backlog ou ADRs, ou listar e ler projetos e documentos (quais projetos existem, o que está bloqueado, qual o próximo id). Serve ao Claude Code e ao Codex, dentro e fora do HUD.
---

# projeto-docs: projetos e documentação no modelo da pasta de projetos

Esta skill ensina a criar, planejar, atualizar e consultar a documentação de um
projeto numa pasta de notas Markdown do usuário — normalmente um Vault do
Obsidian, ou a pasta que o HUD está lendo. O modelo é o de uma pasta de projeto
com uma nota-raiz, seis áreas numeradas e uma área de referência, com um Kanban
vivo (plugin Obsidian Kanban) e ids fixos.

Serve igual ao **Claude Code** e ao **Codex**: os dois leem este `SKILL.md` e a
pasta `modelos/` ao lado dele. O que muda é só **como entregar os arquivos**
(veja *Como entregar*): dentro do HUD, em blocos `hud-doc`; fora dele, gravando
direto na pasta.

## Primeiro: qual é o modo?

Decida pelo pedido, antes de qualquer coisa:

| o pedido | modo | entrega |
|---|---|---|
| veio de `/project <pedido>`, ou pede "crie um projeto", "comece a documentar o projeto X" (que ainda não existe) | **criar projeto** | o esqueleto completo do projeto |
| veio de `/doc <pedido>`, ou pede "documente", "planeje a documentação", "crie uma atividade", "registre a decisão", "mova o card" | **planejar documentação** | um plano curto e depois os arquivos |
| pergunta: "liste os projetos", "o que está bloqueado no X", "qual o próximo ADR", "o que diz a atividade AT-012", "resuma o Kanban" | **listar / ler** | só a resposta, em texto; **nenhum** arquivo, **nenhum** bloco `hud-doc` |

`/project` e `/doc` também podem trazer uma consulta (`/doc o que falta no X?`,
`/project liste`): se o pedido é para **ler ou listar**, é o modo listar/ler,
mesmo vindo desses comandos. Na dúvida entre ler e escrever, leia, responda e
ofereça a escrita numa frase.

## O modelo

### A árvore de uma pasta de projeto

`<Projeto>` é o nome do projeto como aparece nos títulos (ex.: `Hud`,
`Mudarro`). Todo arquivo de nota tem o nome igual ao `title`, com o projeto
entre parênteses.

| caminho dentro da pasta do projeto | modelo | papel |
|---|---|---|
| `<Projeto>.md` | `modelos/projeto.md` | nota-raiz (MOC): resumo, mapa em mermaid, estado datado, navegação, fronteira |
| `01-arquitetura/Visão Geral (<Projeto>).md` | `modelos/visao-geral.md` | o produto, componentes e o que cada um não faz, fluxo, limites |
| `02-cicd/Workflows (<Projeto>).md` | `modelos/workflows.md` | CI e release: gatilhos, jobs, permissões, execuções com resultado |
| `03-deploy/Distribuição (<Projeto>).md` | `modelos/distribuicao.md` | canal, instalação, alternativas, evidência local e publicada |
| `04-operacao/Operação (<Projeto>).md` | `modelos/operacao.md` | runbook: rodar, configurar, testes, problemas comuns |
| `04-operacao/Segurança (<Projeto>).md` | `modelos/seguranca.md` | tabela de controles (área, controle, onde) |
| `04-operacao/Threat Model (<Projeto>).md` | `modelos/threat-model.md` | superfície em mermaid e STRIDE (ameaça, mitigação, lacuna) |
| `05-registro/Decisões (<Projeto>).md` | `modelos/decisoes.md` | todos os ADRs numa nota só: tabela por data + uma seção por ADR |
| `06-backlog/Kanban (<Projeto>).md` | `modelos/kanban.md` | o quadro (estado canônico das atividades) com o Plano de entrega |
| `06-backlog/Como usar o Kanban (<Projeto>).md` | `modelos/como-usar-o-kanban.md` | procedimento do quadro para o dono e os agentes |
| `06-backlog/Backlog de Melhorias (<Projeto>).md` | `modelos/backlog-de-melhorias.md` | atividades por id, épicos `EP-NNN` e histórias `HS-NNN` |
| `06-backlog/Plano de execução paralela (<Projeto>).md` | `modelos/plano-de-execucao-paralela.md` | rodadas, lanes de subagentes, caminho crítico, sequenciais, regras de alocação |
| `06-backlog/Sincronização Pendente (Vault, <Projeto>).md` | `modelos/sincronizacao-pendente.md` | medição datada do que está no código e ainda não está nas notas |
| `06-backlog/atividades/AT-NNN — <título> (<Projeto>).md` | `modelos/atividade.md` | uma nota por atividade: pedido e origem, escopo e aceite, registro |
| `99-referencia/Comandos (<Projeto>).md` | `modelos/comandos.md` | tabela de comandos da CLI e de dentro do programa |
| `99-referencia/Docmap (Vault, <Projeto>).md` | `modelos/docmap.md` | caminho do repositório → nota a revisar |

Projeto **sem código** (pesquisa, evento, estudo): pode omitir `02-cicd/`,
`03-deploy/`, o Threat Model, o Docmap e a Sincronização Pendente, e tirar os
links deles da nota-raiz. O resto fica.

### Placeholders dos modelos

Os modelos usam `{{...}}`. **Nenhum `{{` pode sobrar** num arquivo entregue:
troque cada um pelo valor real ou, se não houver informação, por "a definir"
(nunca invente commit, versão, data, resultado de teste nem link).

| placeholder | o que é | exemplo |
|---|---|---|
| `{{Projeto}}` | nome nos títulos e wikilinks | `Hud` |
| `{{SIGLA}}` | sigla maiúscula dos ADRs | `HUD` → `ADR-HUD-001` |
| `{{ID}}` | prefixo das atividades: `AT` por padrão, ou a sigla do projeto se o usuário preferir | `AT` → `AT-001`; `MUD` → `MUD-001` |
| `{{tag}}` | prefixo das tags, minúsculo e sem espaço | `hud` → `hud/backlog` |
| `{{data}}` | data ISO de hoje | `2026-01-31` |
| `{{dd/mm}}` | data curta, usada nos cards | `31/01` |
| `{{dono}}` | como o usuário quer ser chamado nas notas (pergunte ou use "o dono") | `o dono` |
| `{{area-pai}}` | nota-índice da área onde ficam os projetos (sem ela, use a pasta pai) | `Projetos` |
| `{{repositorio}}` | URL pública ou "sem repositório" — nunca um caminho absoluto da máquina | `https://exemplo.org/projeto` |
| `{{descricao}}`, `{{versao}}`, `{{NNN}}`, `{{título}}` e textos entre chaves | o conteúdo daquela posição | — |

### Convenções

- **Títulos:** `Nome (Projeto)` em toda nota que não é a raiz; o nome do arquivo
  é o título + `.md`. Sem `/` nem `:` no título (são nomes de arquivo).
- **Frontmatter** em toda nota: `title`, `aliases` (lista), `tags` (lista,
  `{{tag}}/<área>`), `atualizado` (ISO). Atividades têm ainda `status`
  (`A fazer`, `Em andamento`, `Bloqueado`, `Concluído`), `prioridade` (`P1`–`P3`),
  `epico` (`EP-NNN`) e `historia` (`HS-NNN`). Decisões têm `total` e
  `proximo: ADR-<SIGLA>-NNN`. O Kanban tem `kanban-plugin: board`.
- **Ids fixos, nunca renumerados nem reutilizados:** atividades `AT-NNN` (ou
  `<SIGLA>-NNN`), épicos `EP-NNN`, histórias `HS-NNN`, decisões
  `ADR-<SIGLA>-NNN`. Três dígitos. O próximo id livre é o maior existente + 1
  (procure nos nomes de `06-backlog/atividades/` e no `proximo:` das Decisões).
- **Wikilinks** `[[Nome da nota]]` para tudo que é nota; seção com
  `[[Nota#Seção|rótulo]]`. **Dentro de tabela**, o `|` do rótulo é escapado:
  `[[Nota#Seção\|rótulo]]`. Toda nota termina com `## Ligações` voltando à raiz
  (`[[<Projeto>|← <Projeto>]]`).
- **Atividade:** o título diz o resultado, em frase ("o painel mostra o uso",
  não "painel de uso"); a nota tem **Pedido e origem**, **Escopo e aceite**
  (com aceite verificável) e **Registro** (entradas datadas, a mais nova em
  cima). Linha do topo: `**Área:** ... · **Épico:** [[...]] · **História:** [[...]]`.
- **ADR:** linha na tabela da data (`ID | Decisão | Consequência`) e uma seção
  `## ADR-<SIGLA>-NNN — <título>` com `Estado: **aceito** · data: ...` e as
  subseções **Contexto**, **Decisão**, **Consequências**. ADR aceito nunca é
  editado: decisão revista vira ADR novo que cita o antigo. Atualize `proximo:`
  e `total:`.
- **Arquivo vivo:** toda mudança de estado (iniciar, concluir, bloquear) muda,
  no mesmo passo, o card, a nota da atividade, a linha do Backlog e o Plano de
  entrega, com data e evidência (arquivo, teste, commit, link). **Card
  concluído não é apagado**: vai para Concluído com `✅ dd/mm` e a evidência.
  O estado é do que existe de fato, nunca da conversa.
- Português, frases curtas, nada de segredo nem dado pessoal (veja *Limites*).

### O Kanban

O quadro é um arquivo do plugin Obsidian Kanban. O formato tem de ser exato,
senão o plugin não abre. Copie `modelos/kanban.md`:

- frontmatter entre `---` com uma linha em branco depois da primeira `---` e
  antes da última, e `kanban-plugin: board`;
- cada coluna é um `## Título`; cada card é `- [ ] texto` (concluído: `- [x]`);
  linha de continuação de card é indentada com tabulação;
- colunas, nesta ordem: `## Kanban (<Projeto>)` (título, vazia), `## Como usar`,
  `## Plano de entrega`, `## A fazer`, `## Em andamento`, `## Bloqueado`,
  `## Concluído`, `## Ligações`;
- no fim, o bloco de configuração, com a cerca **sem identificador de
  linguagem** e `list-collapse` com **uma posição por coluna `##`**, na mesma
  ordem (`true` = recolhida):

````
%% kanban:settings
```
{"kanban-plugin":"board","list-collapse":[true,true,true,false,false,false,false,true],"show-checkboxes":true}
```
%%
````

**Coluna nova no board?** Acrescente uma posição em `list-collapse` na posição
dela.

Card de atividade: `- [ ] [[AT-NNN — <título> (<Projeto>)]] — P1 · <rodada ou
"gesto humano"> · <próximo resultado verificável>`. "A fazer" fica ordenado
P1 → P3. Card bloqueado diz a causa, o próximo passo e quem desbloqueia.

**`## Plano de entrega`** (logo depois de `## Como usar`) tem sempre:

1. um card de **cabeçalho**: data da atualização, paralelismo máximo (quantos
   subagentes), caminho crítico e o link para a nota do plano;
2. **um card por rodada**, com as lanes paralelas (Lane A — AT-NNN (`área`) …)
   e o que fica com o agente principal;
3. um card **"Sequencial / não paralelizar"** com os motivos;
4. um card de **regras de alocação**.

O detalhe fica na nota `Plano de execução paralela (<Projeto>)` (alias
*Plano de entrega e subagentes (<Projeto>)*). Regras do plano:

- **Divida o máximo possível.** Cada lane é um subagente, com arquivos ou área
  disjuntos das outras lanes da mesma rodada.
- Só é sequencial o que tem **dependência real**, **arquivo compartilhado**,
  **recurso único** (VM, banco, ambiente) ou **decisão ou gesto humano**.
- Arquivos compartilhados (índices, listas, o próprio board) ficam com o agente
  principal; o subagente relata e o principal escreve.
- Todas as lanes de uma rodada são disparadas numa única mensagem; lane de
  código trabalha num worktree isolado; o principal integra, valida e atualiza
  o board. Card bloqueado por decisão humana não vai para lane.
- **Criou o Kanban ou mudou o backlog? Crie ou replaneje o Plano de entrega no
  mesmo passo.**

## Modo criar projeto (`/project`)

1. **Descubra onde ficam os projetos.** Procure, na pasta do usuário, arquivos
   `*/06-backlog/Kanban (*).md` e notas-raiz `X/X.md` com a tag `projeto` ou
   `moc`. A pasta do projeto é a que contém `06-backlog/`; a **área de
   projetos** é a pasta pai dela (ex.: `projetos/`, ou `projetos/<grupo>/`).
   Ignore `.obsidian/`, `.trash/` e pastas ocultas.
2. **Use os projetos que já existem como modelo vivo.** Leia a nota-raiz e o
   Kanban de um ou dois deles e siga as convenções que encontrar (estilo do
   nome da pasta, tags, prefixo das atividades, nota-índice da área). Os
   arquivos de `modelos/` são a base quando não houver projeto, ou para o que
   faltar. Se não puder ler `modelos/` (por exemplo, dentro do HUD, onde a
   leitura fica presa à pasta do usuário), siga as tabelas e regras desta skill.
3. **Proponha a pasta nova ao lado dos projetos existentes**:
   `<área de projetos>/<Projeto>/`. Se houver mais de uma área, escolha a que
   combina com o pedido e diga por quê. Se não houver nenhum projeto, use
   `projetos/<Projeto>/` na raiz e avise. Se a pasta já existir, **pare** e
   pergunte (não sobrescreva um projeto).
4. **Gere o esqueleto completo** a partir dos modelos: a nota-raiz e todas as
   notas da árvore (menos as que não se aplicam a projeto sem código), com
   todos os placeholders preenchidos.
5. **Se o pedido descreve trabalho**, transforme-o em atividades: `AT-001`,
   `AT-002`… (uma nota cada em `06-backlog/atividades/`, cards em A fazer
   ordenados por prioridade), um épico `EP-001` com histórias `HS-NNN` no
   Backlog, e a **Rodada 1** no Plano de entrega e na nota do plano, com as
   lanes em áreas disjuntas. Se o pedido traz uma decisão (pilha, licença,
   escopo), registre `ADR-<SIGLA>-001`; senão, Decisões sai sem linhas, com
   `total: 0` e `proximo: ADR-<SIGLA>-001`.
6. **Nota-índice da área** (`{{area-pai}}`): se ela existir e você leu o
   arquivo inteiro, inclua-a atualizada com o link para o projeto novo; se não
   leu, diga ao usuário a linha a acrescentar.
7. Feche com o resumo: a pasta criada, quantas notas, as atividades e a rodada.

## Modo planejar documentação (`/doc`)

1. **Leia antes.** Localize o projeto (pelo nome no pedido, ou pergunte se
   houver mais de um candidato) e leia a nota-raiz, o Kanban e as notas que o
   pedido toca. Leia o arquivo **inteiro** que for atualizar.
2. **Plano curto primeiro**, em lista: cada arquivo a **criar** ou
   **atualizar**, com uma linha de porquê. Ex.: "criar
   `06-backlog/atividades/AT-014 — … (X).md` — o pedido novo; atualizar
   `Kanban (X).md` — card em A fazer e Plano de entrega replanejado; atualizar
   `Backlog de Melhorias (X).md` — linha da AT-014".
3. **Depois, os arquivos**, seguindo os modelos e as convenções, mantendo o
   arquivo vivo: atividade nova mexe em nota + card + Backlog + Plano; decisão
   nova mexe em Decisões (`proximo:`, `total:`) e, se mudar escopo, nas
   atividades afetadas; mudança no código mexe nas notas que o Docmap aponta.
4. Ao **atualizar** um arquivo existente, preserve tudo o que não muda (cards
   concluídos, registros antigos, ADRs, aliases); mude só o necessário e
   atualize `atualizado:`.
5. Se o pedido for só planejar ("só o plano"), pare no passo 2.

## Modo listar / ler

Responda só com a informação lida, em texto curto ou tabela. **Não** produza
blocos `hud-doc` nem grave nada. Exemplos:

- *liste os projetos*: um por linha, com a pasta e, do Kanban, quantos cards em
  Em andamento, A fazer e Bloqueado;
- *o que está bloqueado no X*: os cards da coluna Bloqueado, com a causa;
- *próximo id / próximo ADR*: o maior id existente + 1, e o `proximo:` das
  Decisões;
- *o que diz a AT-NNN*: status, aceite e o último registro.

Cite as notas pelo nome (`Kanban (X)`), sem inventar o que não leu. Se a
informação não existe nas notas, diga isso.

## Como entregar os arquivos

### Dentro do HUD

Você está dentro do HUD quando a conversa traz um contexto que começa com
`[Contexto do HUD`, ou quando o pedido chegou por `/project`, `/doc` ou
`/skill`. Lá, **não grave arquivos**, mesmo que tenha ferramenta para isso. O
HUD grava os blocos da sua última resposta quando o usuário pede.

Responda com **um bloco por arquivo**, com cerca de **quatro crases** (assim o
conteúdo pode ter blocos de três crases dentro, como o mermaid e o
`kanban:settings`):

- a linha de abertura é exatamente ````` ````hud-doc arquivo="<caminho>" `````;
- a linha de fechamento é ````` ```` ````` sozinha;
- `<caminho>` é **relativo à pasta que o HUD está lendo** (o Vault ou a pasta
  escolhida com `/pasta`), com `/` como separador, terminando em `.md`;
  **sem** `..`, sem começar por `/` nem por `.`, sem pasta oculta (nada de
  `.obsidian/`);
- o conteúdo é o **arquivo inteiro**, não um trecho nem um diff;
- para **atualizar** um arquivo que já existe, mande o conteúdo completo novo
  (o HUD pede confirmação antes de sobrescrever);
- só arquivos `.md`.

Antes dos blocos, o plano curto (modo `/doc`) ou o resumo do projeto (modo
`/project`). No fim, diga ao usuário: **"digite `/doc salvar` para gravar"**, e
que ele pode revisar os blocos antes; se algum arquivo já existe, avise que o
HUD vai pedir confirmação para sobrescrever.

Exemplo literal de resposta (projeto `Exemplo`, área `projetos/`):

`````markdown
Vou criar o projeto Exemplo em `projetos/Exemplo/`, ao lado dos outros
projetos, com o Kanban já planejado e duas atividades.

````hud-doc arquivo="projetos/Exemplo/06-backlog/atividades/AT-001 — a página inicial mostra o catálogo (Exemplo).md"
---
title: AT-001 — a página inicial mostra o catálogo (Exemplo)
aliases: [AT-001]
tags: [exemplo/backlog, tipo/atividade]
status: A fazer
prioridade: P1
epico: EP-001
historia: HS-001
atualizado: 2026-01-31
---

# AT-001 — a página inicial mostra o catálogo (Exemplo)

**Área:** `site/` · **Épico:** [[Backlog de Melhorias (Exemplo)#EP-001 — O catálogo no ar|EP-001 — O catálogo no ar]]

## Pedido e origem

Pedido do dono em 2026-01-31: "quero um site com o catálogo".

## Escopo e aceite

Aceite: a página inicial lista os itens do catálogo; teste da página verde.

## Registro

2026-01-31: criada em A fazer.

[[Kanban (Exemplo)]] · [[Backlog de Melhorias (Exemplo)]]
````

(… um bloco para cada uma das outras notas do projeto …)

Revise os blocos e digite `/doc salvar` para gravar.
`````

### Fora do HUD

Num terminal comum (Claude Code ou Codex fora do HUD), **grave os arquivos
direto** na pasta do usuário, com as ferramentas de arquivo, seguindo o mesmo
modelo e os mesmos caminhos:

- a pasta é a que o usuário indicar; sem indicação, o Vault do Obsidian dele
  (a pasta que tem `.obsidian/`) ou o diretório atual, se for uma pasta de
  notas — na dúvida, pergunte antes de gravar;
- crie as pastas que faltarem; não sobrescreva arquivo que você não leu;
- depois de gravar, releia os arquivos e confira: nenhum `{{` sobrando,
  wikilinks apontando para notas que existem, Kanban com uma posição de
  `list-collapse` por coluna;
- no fim, liste o que gravou (caminhos relativos à pasta).

## Limites

- **Nunca escreva segredo**: senha, token, chave, cookie, conteúdo de `.env`,
  credencial de nuvem. Dado pessoal (e-mail, telefone, endereço, documento) só
  se o usuário pedir e for dele.
- **Não toque em `.obsidian/`** nem em pasta oculta.
- **Não apague notas.** Card, atividade, ADR e registro antigos ficam; o que
  acabou muda de estado.
- **Não mova nem renomeie arquivos** sem pedido explícito.
- Nada de caminho absoluto da máquina nas notas: use caminhos relativos à
  pasta do usuário ou ao repositório.
- Não invente evidência. Sem commit, teste ou link, escreva "a definir" ou
  "não verificado".
