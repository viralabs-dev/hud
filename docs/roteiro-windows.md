# Roteiro de validação da tela no Windows

Os testes de tela (`tests/test_ui.py`) rodam num pseudo-terminal e só existem
no Linux e no macOS. No Windows o CI instala o binário e roda os testes de
unidade, mas ninguém olha a tela: o PDCurses (o `windows-curses` que vai dentro
do `hud.exe`) lê teclado e mouse pela API do console, desenha de outro jeito e
tem outras constantes. Este roteiro é a validação visual que falta, feita por
uma pessoa, no **binário final** de uma release.

O smoke de instalação (`packaging/smoke.ps1`, `install.ps1`) não conta como
teste visual: ele só prova que o `hud.exe` abre, lê a configuração e não carrega
módulos plantados.

## Como usar

1. Num Windows 10 ou 11, sem "Executar como administrador", instale a versão a
   testar (README, seção Instalar → Windows) ou tenha o `hud.exe` do pacote
   `hud_windows_amd64.zip` da release.
2. Baixe o código da mesma versão (o `.zip` do código-fonte da release, ou
   `git clone`), que traz este arquivo e o `scripts\roteiro-windows.ps1`.
3. Abra um PowerShell comum e rode o condutor:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\roteiro-windows.ps1 -Terminal wt
   powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\roteiro-windows.ps1 -Terminal conhost
   ```

   O `-ExecutionPolicy Bypass` vale só para esse processo. O condutor procura o
   `hud.exe` no PATH; para outro binário, use `-Hud C:\caminho\hud.exe`.
4. O condutor prepara a pasta de teste, coleta o ambiente, mostra cada passo
   abaixo, abre o HUD na janela do terminal escolhido quando o passo pede e
   pergunta o resultado: `o` ok, `f` falhou, `p` pulado, `a` abre o HUD de
   novo, `s` para e grava o que já foi feito. Cada resposta aceita uma
   observação. No fim ele grava `roteiro-windows-<data>-<terminal>.md`.
5. Rode o roteiro inteiro uma vez em cada terminal (abaixo). São dois
   relatórios por release.

Sem o condutor dá para seguir este arquivo à mão: monte a pasta como em
"Dados sintéticos", abra o HUD com `hud.exe -c <config>` e anote cada passo no
campo **Resultado**.

O condutor não instala nada, não muda configuração do sistema, não precisa de
administrador e não toca no seu `%APPDATA%\hud` nem no seu Vault: o HUD de teste
roda com `USERPROFILE`, `APPDATA` e `LOCALAPPDATA` apontando para a pasta de
teste (só no processo dele). Opções: `-Pasta` (onde criar a pasta de teste,
padrão `%LOCALAPPDATA%\hud-roteiro`), `-Ensaio` (não pergunta nem abre o HUD;
serve para testar o próprio script, inclusive no `pwsh` do Linux) e
`-SoPreparar` (só cria a pasta e mostra os comandos para abrir o HUD).

## Terminais

| | T1 · Windows Terminal | T2 · console clássico (conhost) |
|---|---|---|
| Shell | PowerShell 7 (`pwsh`) | Windows PowerShell 5.1 (`powershell`) |
| Como o condutor abre | `wt.exe -w new pwsh ...` (janela nova) | `conhost.exe powershell ...` (janela nova) |
| Versão a registrar | Configurações → Sobre, ou `Get-AppxPackage Microsoft.WindowsTerminal` | a do Windows (`conhost.exe` vem com ele) |
| Cores | em geral 256 cores: Claude laranja, Codex cinza, OpenCode lilás | pode ter só 16: Claude amarelo, Codex branco, OpenCode magenta (aceito) |
| Fundo | preto se o PDCurses não aceitar o fundo padrão (aceito) | preto |
| Unicode | acentos, `─│╭`, `█░`, `●▶■` e emoji (🚀) | acentos, `─│╭`, `█░`, `●▶■` com Consolas ou Cascadia; emoji costuma virar `?` ou dois quadrados (anote, não é falha do HUD se a moldura não quebrar) |
| Mouse | clique e roda vão para o HUD | o PDCurses desliga a Edição Rápida enquanto o HUD roda; se o clique selecionar texto em vez de ir para o HUD, anote |
| Atalhos do terminal que competem | Alt+setas movem o foco entre painéis do próprio Windows Terminal (com um painel só, a tecla deve chegar ao HUD); Alt+Enter e F11 tela cheia; Alt+F4 fecha | Alt+Enter tela cheia; Alt+Espaço abre o menu da janela; Alt+F4 fecha |
| Colar | Ctrl+V ou clique direito | clique direito (com Edição Rápida) ou menu da janela → Editar → Colar; Ctrl+V pode chegar como tecla |

No Windows 11 o terminal padrão costuma ser o Windows Terminal. O condutor abre
o conhost com `conhost.exe powershell.exe`, sem mudar o terminal padrão do
sistema.

## O que o código sugere que pode falhar

Lido em `hud/ui.py`, para quem executa saber onde olhar com cuidado (não é
resultado; o resultado é o que a tela mostrar):

- **Alt+5, Alt+Z e Alt+setas.** O PDCurses manda Alt+tecla como um código só
  (`ALT_5`, `ALT_Z`, `ALT_UP`...). O HUD traduz assim só Alt+1 a Alt+4
  (`ALT_KEYS`); Alt+setas é reconhecido pelos nomes do ncurses (`kUP3`...) ou
  por Esc + seta, e Alt+5/Alt+Z por Esc + tecla. É provável que essas três
  combinações não façam nada no Windows. Os caminhos alternativos (`/agentes`,
  clique numa caixa, Esc) estão nos passos.
- **Ctrl+C.** No Linux vira `KeyboardInterrupt`; no PDCurses depende do modo do
  console. Se não fechar, saia com `/sair` e marque falhou.
- **Roda do mouse.** Sem `BUTTON4_PRESSED`/`BUTTON5_PRESSED` no módulo, o HUD
  fica sem roda; PgUp/PgDn continuam valendo.

## Dados sintéticos

O condutor cria, dentro de `-Pasta` (padrão `%LOCALAPPDATA%\hud-roteiro`):

```
hud-roteiro\
  home\                              USERPROFILE do HUD de teste
    AppData\Roaming\hud\config.toml  config base: Claude, Codex e OpenCode desligados
    AppData\Roaming\hud\config-abas.toml
                                     config abas: os três agentes apontam para whoami.exe
    AppData\Local\hud\agenda.md      agenda com itens de ontem, hoje, amanhã e +3 dias
    Vault\                           Vault de exemplo (.obsidian, notas, um projeto com Kanban)
    Outra pasta\                     pasta para o /pasta
  abrir-hud-base.ps1                 abre o HUD de teste (config base) nesta janela
  abrir-hud-abas.ps1                 o mesmo, com a config abas
  roteiro-windows-<data>-<terminal>.md   o relatório
```

- **Comandos do painel (F1–F10):** só programas do Windows que leem e não
  mudam nada, sem shell:

  | Tecla | Nome | Programa |
  |---|---|---|
  | F1 | `Nome da máquina` | `hostname` |
  | F2 | `Usuário` | `whoami` |
  | F3 | `Fuso horário` | `tzutil /g` |
  | F4 | `Fusos (lista longa)` | `tzutil /l` (nomes com acento) |
  | F5 | `Drivers (lista longa)` | `driverquery /FO TABLE` |
  | F6 | `Processos do HUD` | `tasklist /FI "IMAGENAME eq hud.exe"` |
  | F7 | `Ping local` | `ping -n 4 127.0.0.1` |
  | F8 | `Tempo-limite` | `ping -n 30 127.0.0.1`, cortado em 5 s |
  | F9 | `Confirmação` | `whoami`, pede `s` antes |
  | F10 | `Falha de propósito` | `whoami /opcao-invalida`, sai com código 1 |

- **Agentes falsos (config abas):** `executable = "whoami"` nos três. Uma
  pergunta "responde" com o erro do `whoami` (ele recusa os argumentos do
  Claude/Codex/OpenCode), o que basta para testar abas, cores e a marca `●` sem
  conta, rede ou custo.
- **Vault:** `Bem-vindo.md` (com uma tarefa datada para hoje), cinco notas em
  `Notas\` e o projeto `Projetos\Roteiro\` com `Roteiro.md` e
  `05-backlog\Kanban (Roteiro).md`: 2 cartões a fazer, 2 em andamento, 1
  bloqueado e 1 concluído, com acentos, `─│╭╮╰╯`, `█░`, `●▶■` e 🚀 nos títulos.

Nada disso é dado real. Mesmo assim, F1 e F2 mostram o nome da máquina e o seu
usuário, e o rodapé do Vault mostra o caminho da pasta de teste (com o seu nome
de usuário): confira as capturas antes de publicá-las.

## Evidência

O condutor coleta e grava no relatório: `hud.exe --version`; a versão do
Windows (`[Environment]::OSVersion`, e `DisplayVersion`/`CurrentBuild`/`UBR` do
registro, só leitura); a edição; a versão do PowerShell; o terminal e a versão
dele; a página de código do console. Pergunta o que não dá para ler: a fonte do
terminal e o resultado do `winver` (confira que bate com o que ele achou).

Capturas de tela: Win+Shift+S, salvas na pasta do relatório com o nome do
passo e do terminal (`P07-wt.png`, `P07-conhost.png`). No mínimo P04, P07,
P15, P22, P24 e P31, e todo passo que falhar.

## Passos

Cada passo diz em que estado o HUD deve estar (**Sessão**), o que fazer, o que
conferir na tela (texto exato entre crases) e tem o campo de resultado. `hh:mm`
é a hora do momento; `<pasta>` é a pasta de teste.

### P01 · Versão do binário
- **Sessão:** fora do HUD
- **Faça:** na janela do condutor, confira a linha `hud --version` que ele mostrou (ou rode `hud.exe --version`).
- **Confira:** `hud X.Y.Z`, com a versão da release em teste; o caminho do `hud.exe` é o binário instalado ou o do `.zip` da release, não um `bin\hud` do código-fonte.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P02 · Ambiente
- **Sessão:** fora do HUD
- **Faça:** rode `winver` e compare com o que o condutor coletou; informe a fonte do terminal quando ele perguntar.
- **Confira:** versão, compilação e edição do Windows iguais às do `winver`; terminal e versão preenchidos; janela com 80×24 ou mais.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P03 · Configuração de teste
- **Sessão:** fora do HUD
- **Faça:** confira a saída do `hud.exe --check -c <pasta>\home\AppData\Roaming\hud\config.toml` que o condutor mostrou.
- **Confira:** `claude: desligado`, `codex: desligado`, `opencode: desligado`, `comandos:` seguido de `[1] Nome da máquina` até `[0] Falha de propósito` (10 linhas, acentos certos) e nenhuma linha `aviso:` (numa sessão elevada sai `aviso: rodando como administrador`: feche e use uma sessão comum).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P04 · Abertura e caixas
- **Sessão:** abre o HUD (base)
- **Faça:** deixe o condutor abrir o HUD na janela nova do terminal em teste e espere 3 s.
- **Confira:** as caixas `SISTEMA`, `COMANDOS`, `AGENDA`, `USO CLAUDE`, `USO CODEX`, `VAULT · ao vivo`, `SAÍDA` e, no rodapé, `ENTRADA`; na SAÍDA, `HUD pronto. Digite /ajuda para ver tudo o que a entrada aceita.` e `configuração: ...config.toml · dados: ...`; nenhuma linha com `⚠`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P05 · Moldura e acentos
- **Sessão:** HUD aberto (base)
- **Faça:** olhe as bordas e os títulos de todas as caixas.
- **Confira:** cantos `╭ ╮ ╰ ╯` e linhas `─ │` contínuas, sem `?`, sem quadrado vazio e sem caractere sobrando além da borda direita; `SAÍDA` e `Amanhã` com acento.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P06 · Barras e indicadores
- **Sessão:** HUD aberto (base)
- **Faça:** olhe a caixa SISTEMA por 10 s.
- **Confira:** barras de CPU, memória e disco com `█` cheio e `░` vazio, que mudam com o uso; histórico de CPU com `▁▂▃▄▅▆▇█`; números que se atualizam a cada segundo.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P07 · Vault e Kanban
- **Sessão:** HUD aberto (base)
- **Faça:** olhe a caixa `VAULT · ao vivo`.
- **Confira:** `1 quadro(s)`, `▶ 2 em andamento` e `■ 1 bloq.` no resumo; a tabela `QUADRO` com a linha `Roteiro` e a barra `█░`; a lista de atenção com `▶ ... Barras █░ e marcas ●▶■`, `▶ ... Lançar foguete 🚀` e `■ ... Aguardando Windows de teste`; a borda direita alinhada mesmo na linha do emoji.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P08 · Agenda
- **Sessão:** HUD aberto (base)
- **Faça:** olhe a caixa AGENDA.
- **Confira:** `Atrasado · <dia>` com `Item atrasado de teste`; `Hoje · <dia dd/mm>` com `09:00 Café da manhã: ação, pão` e `16:00 Tarefa do Vault ◆` (o `◆` marca o que vem do Vault); `Amanhã · <dia>` com `14:00 Reunião ─│╭ caixas`; e o item de daqui a 3 dias com 🚀.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P09 · Nota com Unicode
- **Sessão:** HUD aberto (base)
- **Faça:** cole na ENTRADA a linha de teste que o condutor mostrou (`Teste ação maçã coração ─│╭ █░ ●▶■ 🚀`) e tecle Enter.
- **Confira:** na SAÍDA, `hh:mm › Teste ação maçã coração ─│╭ █░ ●▶■ 🚀`, igual ao colado (no conhost o emoji pode virar `?` na tela; anote); a ENTRADA fica vazia.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P10 · Notas gravadas
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/notas` e Enter.
- **Confira:** `últimas notas (...notas.md)` e a linha `- aaaa-mm-dd hh:mm Teste ação maçã coração ...` com todos os caracteres (inclusive o 🚀, que vai para o arquivo em UTF-8).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P11 · Edição, histórico e Esc
- **Sessão:** HUD aberto (base)
- **Faça:** tecle ↑ (histórico); depois ↓; digite `abcdef`, use ←, →, Home, End, Delete e Backspace no meio do texto; tecle Esc; digite `xyz` e Ctrl+U.
- **Confira:** com ↑/↓, a linha volta e a borda de baixo mostra ` histórico 2/2 · ↑/↓ `; o cursor anda e apaga onde deve; Esc limpa a linha; Ctrl+U também.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P12 · Sugestões de / e Tab
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/a` (sem Enter); depois `j`; tecle Tab; tecle Enter.
- **Confira:** com `/a`, a borda de baixo da ENTRADA mostra ` /ag `, ` /agentes `, ` /ajuda ` e ` Tab completa `; com `/aj`, só ` /ajuda ` e `tudo o que a entrada aceita · Tab completa `; o Tab completa para `/ajuda `; o Enter mostra a ajuda com os títulos `Teclas`, `Caixas`, `Entrada`, `Segurança` e a linha `F1–F10 ou /r N · roda o comando do painel`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P13 · Alt+1 a Alt+4 e Tab sem agentes
- **Sessão:** HUD aberto (base)
- **Faça:** com a ENTRADA vazia, tecle Tab; depois Alt+2, Alt+3, Alt+4 e Alt+1.
- **Confira:** Tab: `Claude e Codex desligados (veja os avisos no início ou hud --check)`; Alt+2: `claude desligado (veja os avisos no início ou hud --check)`; Alt+3: `codex desligado (...)`; Alt+4: `opencode desligado (...)`; Alt+1 não mostra erro e a ENTRADA continua `ENTRADA` (as teclas chegaram ao HUD).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P14 · F1 a F5
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F1, F2, F3, F4 e F5, esperando cada um terminar.
- **Confira:** para cada um, `hh:mm:ss  ▶ [N] <nome>`, a linha `$ <programa> ...`, a saída e `✓ <nome> · 0.0s`; F1 mostra o nome da máquina, F2 `maquina\usuario`, F3 o fuso (ex.: `E. South America Standard Time`), F4 nomes com acento (ex.: `Brasília`) sem `�` nem letras trocadas; F5 a lista de drivers; na caixa COMANDOS, `✓` ao lado de cada um.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P15 · F6 a F10, confirmação e tempo-limite
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F6 e F7; F8 e espere 5 s; F9 e responda `n`; F9 de novo e responda `s`; F10.
- **Confira:** F6 lista `hud.exe` (o binário PyInstaller costuma aparecer duas vezes); F7 mostra 4 respostas de `127.0.0.1` e, enquanto roda, a caixa COMANDOS mostra a animação e os segundos; F8: `✗ Tempo-limite: interrompido após 5s`; F9: a ENTRADA vira `confirmar: Confirmação?` com `s = sim · qualquer outra tecla cancela`, e `n` dá `Confirmação: cancelado`; `s` roda o `whoami`; F10: `✗ Falha de propósito · código 1 · ...`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P16 · /r por número e por nome
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/r 3`, Enter; `/r fuso`, Enter; `/r xyz`, Enter.
- **Confira:** `/r 3` roda `Fuso horário`; `/r fuso` responde `mais de um: [3] Fuso horário, [4] Fusos (lista longa)`; `/r xyz` responde `nenhum comando com “xyz”`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P17 · Rolagem com a roda e PgUp/PgDn
- **Sessão:** HUD aberto (base)
- **Faça:** tecle F5 (lista longa); com o mouse sobre a SAÍDA, gire a roda para cima três vezes e depois para baixo até voltar; tecle PgUp e PgDn.
- **Confira:** girando para cima, o canto direito da borda da SAÍDA mostra `↑ 9 linhas · PgDn volta` (3 linhas por clique da roda) e o texto sobe; para baixo, volta a `roda · PgUp/PgDn`; PgUp/PgDn rolam um terço da tela. Se a roda rolar a janela do console em vez da SAÍDA, anote.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P18 · Clique: foco numa caixa
- **Sessão:** HUD aberto (base)
- **Faça:** clique dentro da caixa AGENDA; depois dentro da SAÍDA; depois na aba `1 NOTAS` na borda da SAÍDA.
- **Confira:** a borda da caixa clicada fica em destaque (cor e negrito) e a anterior volta ao normal; clicar na aba não dá erro e a aba continua aberta. No conhost, se o clique selecionar texto, anote.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P19 · Alt+setas: foco entre caixas
- **Sessão:** HUD aberto (base)
- **Faça:** com o foco na SAÍDA (P18), tecle Alt+↑, Alt+←, Alt+↓ e Alt+→ (Alt+setas).
- **Confira:** o destaque vai da SAÍDA para o VAULT (Alt+↑), para uma caixa da coluna da esquerda (Alt+←), para a de baixo (Alt+↓) e volta à direita (Alt+→). Ver "O que o código sugere que pode falhar": se nada mudar, marque falhou e diga se o terminal reagiu (no Windows Terminal, Alt+setas trocam de painel).
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P20 · Alt+Z: ênfase
- **Sessão:** HUD aberto (base)
- **Faça:** clique na SAÍDA; tecle Alt+Z; tecle Alt+Z de novo; tecle Alt+Z e depois Esc com a ENTRADA vazia.
- **Confira:** com Alt+Z, a SAÍDA ocupa a área toda e a borda mostra `Alt+Z volta · roda · PgUp/PgDn`; Alt+Z de novo volta ao layout; Esc com a entrada vazia também volta. Se Alt+Z não fizer nada, marque falhou (ver "O que o código sugere que pode falhar").
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P21 · Alt+5 e /agentes
- **Sessão:** HUD aberto (base)
- **Faça:** tecle Alt+5; volte com Esc; digite `/agentes` e Enter; volte com Esc.
- **Confira:** a caixa `AGENTES` na área toda, com `PROCESSOS`, `sem leitura de processos no Windows` e `SESSÕES` com `nenhuma sessão na última hora` (a pasta de teste não tem sessões), e a borda `0 processo(s) · 0 sessão(ões) ativa(s)`; Esc volta. Anote separadamente se Alt+5 falhar e `/agentes` funcionar.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P22 · Arraste de caixa pelo título
- **Sessão:** HUD aberto (base)
- **Faça:** pressione o botão esquerdo sobre o título `AGENDA`, arraste até o meio da caixa COMANDOS e solte; depois digite `/custom lista` e `/custom padrao`.
- **Confira:** AGENDA e COMANDOS trocam de lugar; a SAÍDA mostra `caixa agenda movida · layout “pessoal” gravado em <pasta>\home\AppData\Local\hud\custom\pessoal · /custom padrao volta ao original`; `/custom lista` mostra `customizações em ...` com `pessoal` marcado com `●`; `/custom padrao` responde `customização: padrão embutido` e o layout volta.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P23 · Redimensionar a janela
- **Sessão:** HUD aberto (base)
- **Faça:** diminua a janela até menos de 80 colunas ou 24 linhas; aumente de novo; maximize; restaure; no Windows Terminal, aumente e diminua a fonte (Ctrl+roda ou Ctrl+= / Ctrl+-).
- **Confira:** pequena: `Terminal pequeno (LxA). O HUD precisa de 80×24.` e `Aumente a janela ou Ctrl+C para sair.`; ao crescer, o HUD redesenha no tamanho novo em até 1 s, sem restos da tela anterior, sem caixa cortada e sem barra de rolagem do console aparecendo.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P24 · Ctrl+L redesenha
- **Sessão:** HUD aberto (base)
- **Faça:** selecione um trecho de texto com o mouse (Shift+arrastar no Windows Terminal) para sujar a tela; tecle Ctrl+L.
- **Confira:** a tela é redesenhada inteira, sem a seleção nem restos.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P25 · Agenda pela entrada
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/ag amanhã 10h Pão de queijo ●`, Enter; `/ok 1`, Enter; `/rm 2`, Enter (os números são os da caixa AGENDA).
- **Confira:** `＋ agenda: <dia> 10:00 — Pão de queijo ●` e o item novo na AGENDA, com acento; `/ok 1` conclui o primeiro item (some da lista); `/rm 2` apaga o segundo. Um item marcado com `◆` responde `esse item vem do Vault (...); o HUD não escreve no Vault`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P26 · Busca e leitura do Vault
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/b acentuação`; `/b x`; `/conflitos`; `/vault`; `/project`.
- **Confira:** `busca no Vault: “acentuação”`, os arquivos achados com o trecho e `N resultado(s)`; `/b x`: `uso: /b termo (mínimo 2 letras)`; `/conflitos`: `conflitos de sync (0)` e `nenhum`; `/vault`: `relendo o Vault…`; `/project`: `projetos em ...Vault (1)` com `Roteiro`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P27 · Outra pasta
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/pasta`; depois `/pasta <pasta>\home\Outra pasta`; depois `/pasta vault`.
- **Confira:** `/pasta` sozinho mostra a pasta atual e o uso; com o caminho, a caixa vira `PASTA · Outra pasta · ao vivo`; `/pasta vault` volta a `VAULT · ao vivo`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P28 · Comandos sem agente
- **Sessão:** HUD aberto (base)
- **Faça:** digite, um por vez: `/cmd`, `/proposta`, `/doc`, `/skills`, `/skill`, `/perfil`, `/novo`, `/parar`, `/comandos`, `/c oi`, `/x oi`, `/o oi`, `/limpar`.
- **Confira:** `/cmd`: `comandos (10) · fonte: ...` e o arquivo; `/proposta`: `nenhuma proposta pendente · ...`; `/doc`: `uso: /doc pedido · /doc salvar · /doc descartar`; `/skills`: `skills (N)` com as skills do HUD; `/skill`: `uso: /skill nome [pedido] · /skills lista as skills`; `/perfil`: `nenhum agente ligado`; `/novo`: nada; `/parar`: `nenhum agente está respondendo agora`; `/comandos`: `comandos / do Claude Code` e `a lista chega com a primeira resposta do Claude (pergunte algo com /c)`; `/c oi`, `/x oi`, `/o oi`: `claude desligado (...)`, `codex desligado (...)`, `opencode desligado (...)`; `/limpar` esvazia a SAÍDA.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P29 · Saída limpa com /sair
- **Sessão:** HUD aberto (base)
- **Faça:** digite `/sair` e Enter.
- **Confira:** o HUD fecha na hora; a janela volta ao prompt do PowerShell com `hud saiu com codigo 0`, cursor visível, cores normais, sem restos das caixas e com o texto que você digitar aparecendo normalmente.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P30 · Abas da SAÍDA
- **Sessão:** abre o HUD (abas)
- **Faça:** deixe o condutor abrir o HUD com a config abas (agentes falsos).
- **Confira:** na borda de cima da SAÍDA, `1 NOTAS`, `2 CLAUDE`, `3 CODEX` e `4 OPENCODE`, a primeira em destaque; a ENTRADA com `texto = nota · Alt+2 Claude · Alt+3 Codex · Alt+4 OpenCode · /ajuda`.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P31 · Modos e cores
- **Sessão:** HUD aberto (abas)
- **Faça:** tecle Alt+2, Alt+3, Alt+4 e Alt+1; depois Tab quatro vezes.
- **Confira:** Alt+2: ENTRADA `CLAUDE · LEITURA` e aba `2 CLAUDE` em destaque, em laranja (256 cores) ou amarelo (16); Alt+3: `CODEX · LEITURA` em cinza ou branco; Alt+4: `OPENCODE · LEITURA` em lilás ou magenta; Alt+1: `ENTRADA`; Tab percorre notas → Claude → Codex → OpenCode → notas. Anote as cores que viu.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P32 · Clique nas abas
- **Sessão:** HUD aberto (abas)
- **Faça:** clique em `3 CODEX`, em `4 OPENCODE`, em `2 CLAUDE` e em `1 NOTAS`.
- **Confira:** cada clique abre a aba e troca o modo da ENTRADA, como Alt+N.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P33 · Resposta em outra aba e a marca ●
- **Sessão:** HUD aberto (abas)
- **Faça:** no modo notas, digite `/c teste`, Enter; espere 2 s; tecle Alt+2; volte com Alt+1 e repita com `/x teste` (Alt+3) e `/o teste` (Alt+4).
- **Confira:** em NOTAS, `→ pergunta enviada ao claude: a resposta sai na aba CLAUDE (Alt+2)`; a aba vira `2 CLAUDE ●`; na aba CLAUDE, `✦ claude · leitura · conversa nova`, `você › teste` e `✗ claude: ...` (o erro do whoami, sem texto solto fora das caixas); ao abrir a aba o `●` some. O mesmo para codex e opencode.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P34 · Comandos no modo de um agente
- **Sessão:** HUD aberto (abas)
- **Faça:** no modo Claude (Alt+2), digite `/perfil`, `/novo`, `/parar`; digite `/ajuda` e role com a roda; tecle Alt+1 e confira que a aba NOTAS tem a rolagem dela.
- **Confira:** `/perfil`: `claude: perfil leitura · uso: /perfil leitura|completo [claude|codex|opencode]`; `/novo`: `próxima pergunta abre uma conversa nova com o claude`; `/parar`: `nenhum agente está respondendo agora`; a ajuda sai na aba CLAUDE e a rolagem de uma aba não mexe na outra.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P35 · Saída limpa com Ctrl+C
- **Sessão:** HUD aberto (abas)
- **Faça:** tecle Ctrl+C.
- **Confira:** o HUD fecha; a janela volta ao prompt com `hud saiu com codigo 0` (ou o código que o Windows der para a interrupção: anote), cursor visível e sem restos. Se o Ctrl+C não fechar, saia com `/sair` e marque falhou.
- **Resultado:** ☐ ok · ☐ falhou · observação:

### P36 · Nada ficou para trás
- **Sessão:** fora do HUD
- **Faça:** feche as janelas do HUD de teste e deixe o condutor conferir os processos e a pasta temporária.
- **Confira:** nenhum `hud.exe` rodando (`Get-Process hud` vazio) e nenhuma pasta `_MEI*` nova em `%TEMP%` (o binário PyInstaller apaga a dele ao sair).
- **Resultado:** ☐ ok · ☐ falhou · observação:

## Depois de executar

- Anexe os dois relatórios (`-wt` e `-conhost`) e as capturas à atividade
  AT-055 do Kanban do HUD, com a versão testada.
- Cada `falhou` vira um card próprio, com o passo, o terminal, a versão do
  Windows e a captura.
- Quando mudar uma tecla, um comando `/` ou uma aba no HUD, atualize este
  roteiro no mesmo commit: `tests/test_roteiro_windows.py` falha se o roteiro
  deixar de citar algo que a ajuda (`HELP`), os modos (`MODE_KEYS`) ou as
  sugestões de `/` (`SLASH`) oferecem.
