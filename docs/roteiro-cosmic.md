# Roteiro de validação da tela no COSMIC Terminal

Os testes de tela (`tests/test_ui.py`) rodam o HUD num pseudo-terminal e leem a
tela com um emulador VT feito para o teste. Eles provam o que o HUD manda, não o
que um terminal de verdade mostra. Este roteiro é a conferência feita por uma
pessoa no **COSMIC Terminal** (`cosmic-term`), o terminal do desktop COSMIC
(Wayland, Pop!_OS), com `TERM=xterm-256color`.

No Wayland um agente não consegue mandar teclas para outra janela nem capturar
a tela, então a conferência é um gesto humano. O condutor
`scripts/roteiro-cosmic.sh` deixa esse gesto curto: prepara dados sintéticos,
abre a janela, mostra cada passo e grava o relatório.

## Como usar

1. Num desktop COSMIC, com o HUD instalado (README, seção Instalar) ou com o
   código do repositório (`bin/hud`), abra um `cosmic-term` comum.
2. Rode o condutor a partir do repositório:

   ```sh
   scripts/roteiro-cosmic.sh
   scripts/roteiro-cosmic.sh --hud ~/.local/bin/hud      # outro binário
   scripts/roteiro-cosmic.sh --so-preparar               # só a pasta de teste
   ```

   Sem `--hud`, ele usa o `hud` do PATH e, se não houver, o `bin/hud` do
   repositório.
3. O condutor prepara a pasta de teste, coleta o ambiente, mostra cada passo
   abaixo, abre uma janela nova do `cosmic-term` quando o passo pede e pergunta
   o resultado: `o` ok, `f` falhou, `p` pulado, `a` abre a janela de novo, `s`
   para e grava o que já foi feito. Cada resposta aceita uma observação. No fim
   ele grava `roteiro-cosmic-<data>.md` na pasta de teste.
4. O `cosmic-term` 1.x só aceita `-w` (pasta de trabalho) na linha de comando:
   não há opção documentada para rodar um comando na janela nova. Por isso o
   condutor abre a janela já na pasta de teste com `cosmic-term -w <pasta>` e
   mostra o comando curto para colar nela (`./abrir-hud-base.sh` ou
   `./abrir-hud-abas.sh`). Colar no `cosmic-term` é Ctrl+Shift+V. Se a janela
   não abrir, abra uma aba (Ctrl+Shift+T) e rode `cd <pasta>` antes.

Opções: `--pasta DIR` (onde criar a pasta de teste, padrão
`$XDG_STATE_HOME/hud-roteiro-cosmic`, ou `~/.local/state/hud-roteiro-cosmic`),
`--hud CAMINHO`, `--so-preparar` (só cria a pasta e mostra como abrir o HUD) e
`--ensaio` (não pergunta nada nem abre janela; serve para testar o próprio
script e não exige `cosmic-term`).

Sem o condutor dá para seguir este arquivo à mão: monte a pasta como em "Dados
sintéticos", abra o HUD com os lançadores e anote cada passo no campo
**Resultado**.

O condutor não instala nada, não muda a configuração do `cosmic-term` nem do
sistema e não toca no seu `~/.config/hud`, no seu `~/.local/share/hud` nem no
seu Vault: o HUD de teste roda com `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
`XDG_STATE_HOME`, `XDG_CACHE_HOME`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` e `TMPDIR`
apontando para a pasta de teste, só no processo dele. Antes e depois ele tira
uma impressão (só leitura) das suas pastas reais do HUD e confere, em P42, que
nada mudou.

## O terminal

| | COSMIC Terminal |
|---|---|
| `TERM` | `xterm-256color` |
| Cores | 256: Claude laranja (208), Codex cinza (248), OpenCode lilás (141); fundo é o do tema do terminal |
| Unicode | acentos, `─│╭╮╰╯`, `█░`, `▁▂▃▄▅▆▇█`, `●▶■◆` e emoji (🚀, que ocupa duas colunas) |
| Teclas livres para o HUD | F1–F10, Alt+1 a Alt+0, Alt+setas, Alt+Z |
| Teclas do terminal | F11 tela cheia; Ctrl+Shift+número troca de aba; Ctrl+Shift+T aba nova; Ctrl+Shift+C/V copiar/colar; Ctrl+= e Ctrl+- zoom |
| Mouse | o HUD liga o relato de mouse: clique, roda e arraste vão para o HUD; selecionar texto do terminal pede Shift+arrastar |
| Versão a registrar | `cosmic-term --version` (o condutor coleta) |

## Se uma tecla não chegar

Não mude a configuração do terminal. Primeiro descubra quem ficou com a tecla:
abra uma aba nova (Ctrl+Shift+T), rode `cat -v`, tecle a combinação e saia com
Ctrl+D. Se aparecer algo (`^[2` para Alt+2, `^[OP` para F1, `^[[1;3A` para
Alt+↑), a tecla chega ao programa e a falha é do HUD: marque `falhou`. Se nada
aparecer, o `cosmic-term` (ou o COSMIC) capturou a tecla: anote qual e siga pela
alternativa documentada, que é o caminho que o usuário teria:

| Tecla | Alternativa no HUD |
|---|---|
| F1–F10 | `/r N` ou `/r nome` |
| Alt+1 a Alt+4 | Tab alterna o modo; clique na aba da SAÍDA |
| Alt+5 | `/agentes` |
| Alt+Z | clique na caixa e Esc volta; não há comando `/` para a ênfase (anote) |
| Alt+setas | clique na caixa |
| roda do mouse | PgUp/PgDn |
| Ctrl+L | redimensionar a janela também redesenha |
| Ctrl+C | `/sair` |
| F11 | o menu do `cosmic-term`, ou maximizar a janela (Super+M) |

## Dados sintéticos

O condutor cria, dentro de `--pasta`:

```
hud-roteiro-cosmic/
  home/                            HOME do HUD de teste
    .config/hud/config.toml        config base: Claude, Codex e OpenCode desligados
    .config/hud/config-abas.toml   config abas: os três agentes apontam para `false`
    .local/share/hud/agenda.md     agenda com itens de ontem, hoje, amanhã e +3 dias
    Vault/                         Vault de exemplo (.obsidian, notas, um projeto com Kanban)
    Outra pasta/                   pasta para o /pasta
    custom/                        customizações do teste (o arraste grava aqui)
    texto-longo.txt                300 linhas com acentos, molduras e emoji (F4)
  abrir-hud-base.sh                abre o HUD de teste (config base) na janela atual
  abrir-hud-abas.sh                o mesmo, com a config abas
  roteiro-cosmic-<data>.md         o relatório
```

- **Comandos do painel (F1–F10):** só programas do sistema que leem e não
  mudam nada:

  | Tecla | Nome | Programa |
  |---|---|---|
  | F1 | `Data e hora` | `date` |
  | F2 | `Núcleo do sistema` | `uname -sr` |
  | F3 | `Fuso horário` | `date +%Z %z` |
  | F4 | `Texto longo` | `cat texto-longo.txt` (acentos e emoji) |
  | F5 | `Números (lista longa)` | `seq 1 500` |
  | F6 | `Processos do HUD` | `pgrep -a -f <pasta>/home/.config/hud/config` |
  | F7 | `Espera 3 s` | `sleep 3` |
  | F8 | `Tempo-limite` | `sleep 30`, cortado em 5 s |
  | F9 | `Confirmação` | `echo confirmado`, pede `s` antes |
  | F10 | `Falha de propósito` | `false`, sai com código 1 |

- **Agentes falsos (config abas):** `executable = "false"` nos três. Uma
  pergunta "responde" com erro, o que basta para testar abas, cores e a marca
  `●` sem conta, rede ou custo.
- **Vault:** `Bem-vindo.md` (com uma tarefa datada para hoje), cinco notas em
  `Notas/` e o projeto `Projetos/Roteiro/` com `Roteiro.md` e
  `05-backlog/Kanban (Roteiro).md`: 2 cartões a fazer, 2 em andamento, 1
  bloqueado e 1 concluído, com acentos, `─│╭╮╰╯`, `█░`, `●▶■` e 🚀 nos títulos.

Nada disso é dado real. A caixa SISTEMA mostra o nome da máquina, a caixa
AGENTES pode listar agentes que você tenha rodando e o rodapé do Vault mostra o
caminho da pasta de teste (com o seu nome de usuário): confira as capturas antes
de publicá-las.

## Evidência

O condutor coleta e grava no relatório: `hud --version`; `cosmic-term
--version`; o `$TERM` e o tamanho (`stty size`) da janela do condutor; o
sistema (`/etc/os-release`), a sessão (`XDG_SESSION_TYPE`,
`XDG_CURRENT_DESKTOP`) e o locale. Pergunta o que não dá para ler: a fonte do
terminal. O lançador do HUD mostra o `TERM` e o tamanho da janela do HUD antes
de abrir.

Capturas de tela: Print Screen (a ferramenta de captura do COSMIC), salvas na
pasta do relatório com o nome do passo (`P08-cosmic.png`). No mínimo P04, P06,
P08, P10, P25, P36 e todo passo que falhar.

## Passos

Cada passo diz em que estado o HUD deve estar (**Sessão**), o que fazer, o que
conferir na tela (texto exato entre crases) e tem o campo de resultado. `hh:mm`
é a hora do momento; `<pasta>` é a pasta de teste.

### P01 · Versão do HUD
- **Sessão:** fora do HUD
- **Faça:** na janela do condutor, confira a linha `hud --version` que ele mostrou.
- **Confira:** `hud X.Y.Z`, com a versão em teste; o caminho do `hud` é o que você quer testar (instalado ou `bin/hud` do repositório).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P02 · Ambiente
- **Sessão:** fora do HUD
- **Faça:** confira o ambiente que o condutor mostrou e informe a fonte do terminal quando ele perguntar.
- **Confira:** `cosmic-term X.Y.Z` preenchido, `TERM` igual a `xterm-256color`, sessão `wayland` e desktop `COSMIC`; janela do condutor com 80×24 ou mais.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P03 · Configuração de teste
- **Sessão:** fora do HUD
- **Faça:** confira a saída do `hud --check -c <pasta>/home/.config/hud/config.toml` que o condutor mostrou.
- **Confira:** `claude: desligado`, `codex: desligado`, `opencode: desligado`, `comandos:` seguido de `[1] Data e hora` até `[0] Falha de propósito` (10 linhas, acentos certos) e nenhuma linha `aviso:`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P04 · Abertura e caixas
- **Sessão:** abre o HUD (base)
- **Faça:** na janela nova do `cosmic-term`, cole `./abrir-hud-base.sh` (Ctrl+Shift+V) e tecle Enter; espere 3 s.
- **Confira:** antes do HUD, o lançador mostra `TERM=xterm-256color`; depois, as caixas `SISTEMA`, `COMANDOS`, `AGENDA`, `USO CLAUDE`, `USO CODEX`, `VAULT · ao vivo`, `SAÍDA` e, no rodapé, `ENTRADA`; na SAÍDA, `HUD pronto. Digite /ajuda para ver tudo o que a entrada aceita.` e `configuração: ...config.toml · dados: ...`; nenhuma linha com `⚠`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P05 · Moldura e acentos
- **Sessão:** HUD aberto (base)
- **Faça:** olhe as bordas e os títulos de todas as caixas.
- **Confira:** cantos `╭ ╮ ╰ ╯` e linhas `─ │` contínuas, sem `?`, sem quadrado vazio, sem falha entre as linhas e sem caractere sobrando além da borda direita; `SAÍDA` e `Amanhã` com acento.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P06 · Cores (256)
- **Sessão:** HUD aberto (base)
- **Faça:** olhe as cores de bordas, títulos e barras.
- **Confira:** bordas em azul, títulos em ciano e negrito, barras em verde, amarelo ou vermelho conforme o uso; o fundo é o do tema do `cosmic-term` (não um preto forçado) e o texto `dim` aparece mais apagado, mas legível. As cores de agente (laranja, cinza, lilás) ficam para P36.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P07 · Barras e indicadores
- **Sessão:** HUD aberto (base)
- **Faça:** olhe a caixa SISTEMA por 10 s.
- **Confira:** barras de CPU, memória e disco com `█` cheio e `░` vazio, que mudam com o uso; histórico de CPU com `▁▂▃▄▅▆▇█`; números que se atualizam a cada segundo.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P08 · Vault e Kanban
- **Sessão:** HUD aberto (base)
- **Faça:** olhe a caixa `VAULT · ao vivo`.
- **Confira:** `1 quadro(s)`, `▶ 2 em andamento` e `■ 1 bloq.` no resumo; a tabela `QUADRO` com a linha `Roteiro` e a barra `█░`; a lista de atenção com `▶ ... Barras █░ e marcas ●▶■`, `▶ ... Lançar foguete 🚀` e `■ ... Aguardando COSMIC de teste`; a borda direita alinhada mesmo na linha do emoji.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P09 · Agenda
- **Sessão:** HUD aberto (base)
- **Faça:** olhe a caixa AGENDA.
- **Confira:** `Atrasado · <dia>` com `Item atrasado de teste`; `Hoje · <dia dd/mm>` com `09:00 Café da manhã: ação, pão` e `16:00 Tarefa do Vault ◆` (o `◆` marca o que vem do Vault); `Amanhã · <dia>` com `14:00 Reunião ─│╭ caixas`; e o item de daqui a 3 dias com 🚀 e a borda direita no lugar.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P10 · Nota com Unicode e emoji
- **Sessão:** HUD aberto (base)
- **Faça:** cole na ENTRADA (Ctrl+Shift+V) a linha de teste que o condutor mostrou (`Teste ação maçã coração ─│╭ █░ ●▶■ 🚀`) e tecle Enter.
- **Confira:** enquanto digita, o cursor fica logo depois do 🚀 (o emoji ocupa duas colunas); na SAÍDA, `hh:mm › Teste ação maçã coração ─│╭ █░ ●▶■ 🚀`, igual ao colado, com a borda direita no lugar; a ENTRADA fica vazia.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P11 · Notas gravadas
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/notas` e Enter.
- **Confira:** `últimas notas (...notas.md)` com o caminho dentro de `<pasta>/home/.local/share/hud` e a linha `- aaaa-mm-dd hh:mm Teste ação maçã coração ...` com todos os caracteres.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P12 · Edição, histórico e Esc
- **Sessão:** HUD aberto (base)
- **Faça:** tecle ↑ (histórico); depois ↓; digite `abcdef`, use ←, →, Home, End, Delete e Backspace no meio do texto; tecle Esc; digite `xyz` e Ctrl+U.
- **Confira:** com ↑/↓, a linha volta e a borda de baixo mostra ` histórico 2/2 · ↑/↓ `; o cursor anda e apaga onde deve; Esc limpa a linha; Ctrl+U também.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P13 · Sugestões de / e Tab
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/a` (sem Enter); depois `j`; tecle Tab; tecle Enter.
- **Confira:** com `/a`, a borda de baixo da ENTRADA mostra ` /ag `, ` /agentes `, ` /ajuda ` e ` Tab completa `; com `/aj`, só ` /ajuda ` e `tudo o que a entrada aceita · Tab completa `; o Tab completa para `/ajuda `; o Enter mostra a ajuda com os títulos `Teclas`, `Caixas`, `Entrada`, `Segurança` e a linha `F1–F10 ou /r N · roda o comando do painel`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P14 · Alt+1 a Alt+4 e Tab sem agentes
- **Sessão:** HUD aberto (base)
- **Faça:** com a ENTRADA vazia, tecle Tab; depois Alt+2, Alt+3, Alt+4 e Alt+1.
- **Confira:** Tab: `Claude e Codex desligados (veja os avisos no início ou hud --check)`; Alt+2: `claude desligado (veja os avisos no início ou hud --check)`; Alt+3: `codex desligado (...)`; Alt+4: `opencode desligado (...)`; Alt+1 não mostra erro e a ENTRADA continua `ENTRADA`; nenhum `1`, `2`, `3` ou `4` aparece digitado na ENTRADA (as teclas chegaram ao HUD como Alt).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P15 · Alt+6 a Alt+0 livres
- **Sessão:** HUD aberto (base)
- **Faça:** com a ENTRADA vazia, tecle Alt+6, Alt+7, Alt+8, Alt+9 e Alt+0.
- **Confira:** nada acontece: nenhuma mensagem na SAÍDA, nenhum número digitado na ENTRADA e o `cosmic-term` não troca de aba nem de janela (anote se alguma for capturada pelo terminal ou pelo COSMIC).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P16 · F1 a F5
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F1, F2, F3, F4 e F5, esperando cada um terminar.
- **Confira:** para cada um, `hh:mm:ss  ▶ [N] <nome>`, a linha `$ <programa> ...`, a saída e `✓ <nome> · 0.0s`; F1 a data, F2 `Linux` e a versão do núcleo, F3 o fuso (ex.: `-03 -0300`), F4 as linhas `linha 001 · ação, coração, pão ─│╭ █░ ●▶■ 🚀` em diante, sem `�` e com a borda no lugar, F5 os números; na caixa COMANDOS, `✓` ao lado de cada um. Nenhuma tecla F abriu menu ou ajuda do `cosmic-term`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P17 · F6 a F10, confirmação e tempo-limite
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F6 e F7; F8 e espere 5 s; F9 e responda `n`; F9 de novo e responda `s`; F10.
- **Confira:** F6 lista o processo do HUD de teste; F7, enquanto roda, a caixa COMANDOS mostra a animação e os segundos, e termina com `✓ Espera 3 s · 3.0s`; F8: `✗ Tempo-limite: interrompido após 5s`; F9: a ENTRADA vira `confirmar: Confirmação?` com `s = sim · qualquer outra tecla cancela`, e `n` dá `Confirmação: cancelado`; `s` mostra `confirmado`; F10: `✗ Falha de propósito · código 1 · ...`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P18 · /r por número e por nome
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/r 3`, Enter; `/r espera`, Enter; `/r xyz`, Enter.
- **Confira:** `/r 3` roda `Fuso horário`; `/r espera` roda `Espera 3 s`; `/r xyz` responde `nenhum comando com “xyz”`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P19 · Rolagem com a roda e PgUp/PgDn
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F5 (lista longa); com o mouse sobre a SAÍDA, gire a roda para cima três vezes e depois para baixo até voltar; tecle PgUp e PgDn.
- **Confira:** girando para cima, o canto direito da borda da SAÍDA mostra `↑ 9 linhas · PgDn volta` (3 linhas por clique da roda) e o texto sobe; para baixo, volta a `roda · PgUp/PgDn`; PgUp/PgDn rolam um terço da tela. Se a roda rolar o histórico do `cosmic-term` em vez da SAÍDA, anote.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P20 · Clique: foco numa caixa
- **Sessão:** HUD aberto (base)
- **Faça:** clique dentro da caixa AGENDA; depois dentro da SAÍDA; depois na aba `1 NOTAS` na borda da SAÍDA.
- **Confira:** a borda da caixa clicada fica em destaque (cor e negrito) e a anterior volta ao normal; clicar na aba não dá erro e a aba continua aberta; o clique não seleciona texto do terminal.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P21 · Alt+setas: foco entre caixas
- **Sessão:** HUD aberto (base)
- **Faça:** com o foco na SAÍDA (P20), tecle Alt+↑, Alt+←, Alt+↓ e Alt+→ (Alt+setas).
- **Confira:** o destaque vai da SAÍDA para o VAULT (Alt+↑), para uma caixa da coluna da esquerda (Alt+←), para a de baixo (Alt+↓) e volta à direita (Alt+→); o `cosmic-term` não troca de painel nem de aba.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P22 · Alt+Z: ênfase
- **Sessão:** HUD aberto (base)
- **Faça:** clique na SAÍDA; tecle Alt+Z; tecle Alt+Z de novo; tecle Alt+Z e depois Esc com a ENTRADA vazia.
- **Confira:** com Alt+Z, a SAÍDA ocupa a área toda e a borda mostra `Alt+Z volta · roda · PgUp/PgDn`; Alt+Z de novo volta ao layout; Esc com a entrada vazia também volta; nenhum `z` aparece digitado.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P23 · Alt+5 e /agentes
- **Sessão:** HUD aberto (base)
- **Faça:** tecle Alt+5; volte com Esc; digite `/agentes` e Enter; volte com Alt+Z.
- **Confira:** a caixa `AGENTES` na área toda, com `PROCESSOS` (os agentes que estiverem rodando na máquina, ou `nenhum agente rodando`) e `SESSÕES` com `nenhuma sessão na última hora` (o HOME de teste não tem sessões), e a borda `N processo(s) · 0 sessão(ões) ativa(s)`; Esc e Alt+Z voltam. Anote separadamente se Alt+5 falhar e `/agentes` funcionar.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P24 · Arraste de caixa pelo título
- **Sessão:** HUD aberto (base)
- **Faça:** pressione o botão esquerdo sobre o título `AGENDA`, arraste até o meio da caixa COMANDOS e solte; depois digite `/custom lista` e `/custom padrao`.
- **Confira:** AGENDA e COMANDOS trocam de lugar; a SAÍDA mostra `caixa agenda movida · layout “pessoal” gravado em <pasta>/home/custom/pessoal · /custom padrao volta ao original`; `/custom lista` mostra `customizações em ...` com `pessoal` marcado com `●`; `/custom padrao` responde `customização: padrão embutido` e o layout volta.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P25 · Redimensionar até 80×24
- **Sessão:** HUD aberto (base)
- **Faça:** saia da tela cheia e diminua a janela pelo canto até aparecer o aviso; aumente devagar até o HUD voltar (esse é o tamanho mínimo); depois maximize e restaure.
- **Confira:** pequena: `Terminal pequeno (LxA). O HUD precisa de 80×24.` e `Aumente a janela ou Ctrl+C para sair.`; no tamanho mínimo todas as caixas aparecem inteiras, sem texto cortado no meio da borda; ao crescer, o HUD redesenha no tamanho novo em até 1 s, sem restos da tela anterior.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P26 · F11 tela cheia e zoom
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F11; espere 1 s; tecle F11 de novo; depois Ctrl+= duas vezes e Ctrl+- duas vezes (zoom do `cosmic-term`).
- **Confira:** F11 é do `cosmic-term`: a janela vai para tela cheia e o HUD redesenha no tamanho novo, sem restos; o HUD não reage ao F11 (nada na SAÍDA); de volta, o layout se ajusta; com o zoom, o HUD redesenha a cada mudança e mostra o aviso de terminal pequeno se passar do limite.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P27 · Ctrl+L redesenha
- **Sessão:** HUD aberto (base)
- **Faça:** selecione um trecho com Shift+arrastar (seleção do `cosmic-term`) para sujar a tela; tecle Ctrl+L.
- **Confira:** Shift+arrastar seleciona texto em vez de mover caixas; Ctrl+L redesenha a tela inteira, sem a seleção nem restos.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P28 · Agenda pela entrada
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/ag amanhã 10h Pão de queijo ●`, Enter; `/ok 1`, Enter; `/rm 2`, Enter (os números são os da caixa AGENDA).
- **Confira:** `＋ agenda: <dia> 10:00 — Pão de queijo ●` e o item novo na AGENDA, com acento; `/ok 1` conclui o primeiro item (some da lista); `/rm 2` apaga o segundo. Um item marcado com `◆` responde `esse item vem do Vault (...); o HUD não escreve no Vault`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P29 · Busca e leitura do Vault
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/b acentuação`; `/b x`; `/conflitos`; `/vault`; `/project`.
- **Confira:** `busca no Vault: “acentuação”`, os arquivos achados com o trecho e `N resultado(s)`; `/b x`: `uso: /b termo (mínimo 2 letras)`; `/conflitos`: `conflitos de sync (0)` e `nenhum`; `/vault`: `relendo o Vault…`; `/project`: `projetos em ...Vault (1)` com `Roteiro`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P30 · Outra pasta
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/pasta`; depois `/pasta <pasta>/home/Outra pasta`; depois `/pasta vault`.
- **Confira:** `/pasta` sozinho mostra a pasta atual e o uso; com o caminho, a caixa vira `PASTA · Outra pasta · ao vivo`; `/pasta vault` volta a `VAULT · ao vivo`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P31 · Comandos sem agente
- **Sessão:** HUD aberto (base)
- **Faça:** digite, um por vez: `/cmd`, `/proposta`, `/doc`, `/skills`, `/skill`, `/perfil`, `/novo`, `/parar`, `/comandos`, `/c oi`, `/x oi`, `/o oi`, `/limpar`.
- **Confira:** `/cmd`: `comandos (10) · fonte: ...` e o arquivo; `/proposta`: `nenhuma proposta pendente · ...`; `/doc`: `uso: /doc pedido · /doc salvar · /doc descartar`; `/skills`: `skills (N)` com as skills do HUD; `/skill`: `uso: /skill nome [pedido] · /skills lista as skills`; `/perfil`: `nenhum agente ligado`; `/novo`: nada; `/parar`: `nenhum agente está respondendo agora`; `/comandos`: `comandos / do Claude Code`; `/c oi`, `/x oi`, `/o oi`: `claude desligado (...)`, `codex desligado (...)`, `opencode desligado (...)`; `/limpar` esvazia a SAÍDA.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P32 · Saída limpa com /sair
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/sair` e Enter.
- **Confira:** o HUD fecha na hora; a janela volta ao prompt com `hud saiu com código 0`, cursor visível, cores normais, sem restos das caixas, o texto que você digitar aparece normalmente e a roda do mouse volta a rolar o histórico do terminal.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P33 · Entrada de novo
- **Sessão:** abre o HUD (base)
- **Faça:** na mesma janela, rode `./abrir-hud-base.sh` de novo; digite `/notas` e Enter.
- **Confira:** o HUD abre igual a P04, com o layout padrão (P24 voltou ao padrão) e a caixa `VAULT · ao vivo`; `/notas` ainda mostra a nota de P10.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P34 · Saída limpa com Ctrl+C
- **Sessão:** HUD aberto (base)
- **Faça:** tecle Ctrl+C.
- **Confira:** o HUD fecha; a janela volta ao prompt com `hud saiu com código 0` (ou 130: anote), cursor visível, eco do teclado normal e sem restos. Se o Ctrl+C não fechar, saia com `/sair` e marque falhou.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P35 · Abas da SAÍDA
- **Sessão:** abre o HUD (abas)
- **Faça:** na janela do HUD, rode `./abrir-hud-abas.sh` (agentes falsos).
- **Confira:** na borda de cima da SAÍDA, `1 NOTAS`, `2 CLAUDE`, `3 CODEX` e `4 OPENCODE`, a primeira em destaque; a ENTRADA com `texto = nota · Alt+2 Claude · Alt+3 Codex · Alt+4 OpenCode · /ajuda`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P36 · Modos e cores de agente
- **Sessão:** HUD aberto (abas)
- **Faça:** tecle Alt+2, Alt+3, Alt+4 e Alt+1; depois Tab quatro vezes.
- **Confira:** Alt+2: ENTRADA `CLAUDE · LEITURA` e aba `2 CLAUDE` em destaque, em laranja; Alt+3: `CODEX · LEITURA` em cinza; Alt+4: `OPENCODE · LEITURA` em lilás; Alt+1: `ENTRADA`; Tab percorre notas → Claude → Codex → OpenCode → notas. Laranja, cinza e lilás distintos provam as 256 cores; amarelo, branco e magenta indicariam só 16 (falhou).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P37 · Clique nas abas
- **Sessão:** HUD aberto (abas)
- **Faça:** clique em `3 CODEX`, em `4 OPENCODE`, em `2 CLAUDE` e em `1 NOTAS`.
- **Confira:** cada clique abre a aba e troca o modo da ENTRADA, como Alt+N.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P38 · Resposta em outra aba e a marca ●
- **Sessão:** HUD aberto (abas)
- **Faça:** no modo notas, digite `/c teste`, Enter; espere 2 s; tecle Alt+2; volte com Alt+1 e repita com `/x teste` (Alt+3) e `/o teste` (Alt+4).
- **Confira:** em NOTAS, `→ pergunta enviada ao claude: a resposta sai na aba CLAUDE (Alt+2)`; a aba vira `2 CLAUDE ●`; na aba CLAUDE, `✦ claude · leitura · conversa nova`, `você › teste` e `✗ claude: ...` (o erro do agente falso, sem texto solto fora das caixas); ao abrir a aba o `●` some. O mesmo para codex e opencode.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P39 · Comandos no modo de um agente
- **Sessão:** HUD aberto (abas)
- **Faça:** no modo Claude (Alt+2), digite `/perfil`, `/novo`, `/parar`; digite `/ajuda` e role com a roda; tecle Alt+1 e confira que a aba NOTAS tem a rolagem dela.
- **Confira:** `/perfil`: `claude: perfil leitura · uso: /perfil leitura|completo [claude|codex|opencode]`; `/novo`: `próxima pergunta abre uma conversa nova com o claude`; `/parar`: `nenhum agente está respondendo agora`; a ajuda sai na aba CLAUDE e a rolagem de uma aba não mexe na outra.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P40 · Abas do cosmic-term com o HUD aberto
- **Sessão:** HUD aberto (abas)
- **Faça:** abra uma aba do terminal (Ctrl+Shift+T); volte à aba do HUD com Ctrl+Shift+1; feche a aba nova pelo X dela.
- **Confira:** as teclas de aba são do `cosmic-term` e não chegam ao HUD (nada digitado na ENTRADA); de volta à aba do HUD, a tela está inteira e os números da caixa SISTEMA continuam mudando.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P41 · Saída limpa das abas
- **Sessão:** HUD aberto (abas)
- **Faça:** digite `/sair` e Enter.
- **Confira:** o HUD fecha com `hud saiu com código 0`, prompt normal, sem restos e sem processo de agente falso pendurado.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P42 · Nada ficou para trás
- **Sessão:** fora do HUD
- **Faça:** feche as janelas do HUD de teste e deixe o condutor conferir os processos e as suas pastas reais.
- **Confira:** o condutor mostra `nenhum hud de teste rodando` e `pastas reais do HUD sem mudança` (a impressão de `~/.config/hud` e `~/.local/share/hud` é a mesma do início).
- **Resultado:** ☐ ok · ☐ falhou · observação:

## Depois de executar

- Anexe o relatório e as capturas à atividade AT-012 do Kanban do HUD, com a
  versão testada e a do `cosmic-term`.
- Cada `falhou` vira um card próprio, com o passo, a versão do `cosmic-term` e
  a captura.
- Quando mudar uma tecla, um comando `/` ou uma aba no HUD, atualize este
  roteiro no mesmo commit: `tests/test_roteiro_cosmic.py` falha se o roteiro
  deixar de citar algo que a ajuda (`HELP`), os modos (`MODE_KEYS`) ou as
  sugestões de `/` (`SLASH`) oferecem.
