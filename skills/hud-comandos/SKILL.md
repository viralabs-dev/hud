---
name: hud-comandos
description: Muda os comandos do painel COMANDOS do HUD de terminal (repositório viralabs-dev/hud), os que rodam com as teclas F1 a F10. Use quando o usuário quiser trocar, acrescentar, tirar, renomear ou reordenar esses comandos, mudar o tempo-limite, pedir confirmação antes de rodar, ou ver onde fica e como validar o comandos.toml. Serve ao Claude Code e ao Codex, dentro e fora do HUD.
---

# HUD: comandos do painel COMANDOS

## O que é

O painel COMANDOS do HUD lista até **10 comandos**, um por tecla: o primeiro
roda com F1, o segundo com F2… o décimo com F10 (as teclas `1`–`0` também
servem). Só roda o que está nessa lista: argv fixo, sem shell, executável
resolvido num PATH fixo e ambiente mínimo.

A lista mora no arquivo **`comandos.toml`**, ao lado do `config.toml`:

| sistema | caminho |
|---|---|
| Linux e macOS | `~/.config/hud/comandos.toml` |
| Windows | `%APPDATA%\hud\comandos.toml` |
| HUD aberto com `-c outro/config.toml` | `outro/comandos.toml` (mesma pasta) |

Ordem de prioridade:

1. `comandos.toml`, se existir e for privado (seu, sem escrita para outros:
   `chmod 600`; no Windows, dentro do seu perfil; nunca link simbólico);
2. senão, os `[[command]]` do `config.toml` (o jeito antigo, ainda vale);
3. senão, os comandos padrão do sistema.

Quando o `comandos.toml` existe, ele **substitui** todos os `[[command]]` do
`config.toml`. `hud --check` mostra qual fonte está valendo, o caminho do
arquivo e cada comando com a sua tecla.

## Esquema

O arquivo só tem tabelas `[[command]]`, na ordem das teclas (qualquer outra
chave no topo faz o HUD ignorar o arquivo inteiro):

```toml
[[command]]
name = "Discos"                  # obrigatório: o nome no painel
argv = ["df", "-h"]              # obrigatório: programa e argumentos, sem shell
timeout = 20                     # opcional: segundos, de 1 a 600 (padrão 20)
confirm = false                  # opcional: pede "s" antes de rodar (padrão false)
max_lines = 200                  # opcional: linhas guardadas da saída, 1 a 5000 (padrão 200)
cwd = "~/projetos"               # opcional: pasta onde roda (precisa existir)
```

## Regras de segurança (o HUD recusa o comando que não cumpre)

- **argv sem shell.** Cada argumento é um texto da lista; nada de `sh -c`,
  pipes (`|`), `;`, `&&`, `>`, `$(...)` ou curingas: não há shell para
  interpretá-los, então eles chegariam literais ao programa.
- **Proibidos:** `sudo`, `su`, `doas`, `pkexec`, `run0`, shells (`sh`, `bash`,
  `dash`, `zsh`, `fish`, `ksh`, `csh`, `tcsh`), `env`, `xargs`, `nohup`,
  `setsid`, `script`. No Windows também `cmd`, `powershell`, `wsl` e afins, e
  só `.exe` roda (`.bat`/`.cmd` passariam pelo `cmd.exe`). Não use
  interpretadores (`python3 -c`, `node -e`, `perl -e`) para contornar a regra.
- **PATH fixo.** O executável é procurado só nas pastas do sistema
  (`/usr/local/sbin`, `/usr/local/bin`, `/usr/sbin`, `/usr/bin`, `/sbin`,
  `/bin` no Linux e no macOS; `System32` e afins no Windows) ou dado por caminho absoluto. O
  executável e a pasta dele não podem ser graváveis por outros usuários.
- **`confirm = true`** para tudo que muda o sistema (reiniciar serviço, limpar
  cache, atualizar pacotes, `docker system prune`…): o HUD mostra o argv e só
  roda com "s".
- **Tempo-limite.** Comando que demora (atualização de pacotes, varredura)
  merece `timeout` maior; o HUD mata o processo quando ele estoura.
- **No máximo 10.** Do 11º em diante nada entra no painel.
- **Nada pessoal.** Use `~` em `cwd`; não ponha token, senha nem URL com chave
  em argumentos (a saída e o argv aparecem na tela).

## Exemplos

Linux, com um comando novo e outro que pede confirmação:

```toml
[[command]]
name = "Uptime e usuários"
argv = ["w", "-s"]

[[command]]
name = "Discos"
argv = ["df", "-h", "-x", "tmpfs", "-x", "devtmpfs"]

[[command]]
name = "Contêineres"
argv = ["docker", "ps", "--all"]
timeout = 10

[[command]]
name = "Limpar o Docker"
argv = ["docker", "system", "prune", "--force"]
confirm = true
timeout = 120

[[command]]
name = "Git do projeto"
argv = ["git", "status", "--short", "--branch"]
cwd = "~/projetos/meu-app"
```

macOS: `["uptime"]`, `["df", "-h"]`, `["vm_stat"]`, `["pmset", "-g", "batt"]`.
Windows: `["tasklist", "/FO", "TABLE"]`, `["ipconfig", "/all"]`, `["whoami"]`.

## Dentro do HUD

Quando a conversa acontece **dentro do HUD** (o HUD avisa isso no contexto do
agente), **não grave arquivos**, mesmo que tenha ferramentas para isso.
Responda com **um** bloco `hud-comandos` contendo o `comandos.toml` **inteiro**
(não um trecho: o que ficar de fora sai do painel):

- a linha de abertura é exatamente ```` ```hud-comandos ````;
- a cerca fecha com o mesmo caractere e pelo menos o mesmo comprimento (use
  quatro crases ou `~~~` se o conteúdo tiver uma linha só de crases);
- se a resposta tiver mais de um bloco, vale o último; acima de 64 KB é recusado.

Parta dos comandos atuais (o usuário pode colar a saída de `hud --check`, ou
você recebe a lista no contexto) e mude só o que foi pedido. Termine dizendo ao
usuário que o HUD vai mostrar uma **prévia**: o painel COMANDOS como vai ficar
e o que muda (`+` entra, `-` sai, `~` muda ou troca de tecla, `=` igual), com
os comandos recusados e o motivo. Só com `s` o HUD grava o `comandos.toml`
(privado, 600) e passa a usá-lo; qualquer outra tecla descarta. Se algum
comando for recusado, nada é gravado: corrija e mande o arquivo inteiro de novo.

Exemplo literal de resposta:

````markdown
Deixei só três comandos: "Contêineres" no F2 e pus a limpeza do
Docker, com confirmação, no F3.

```hud-comandos
[[command]]
name = "Uptime e usuários"
argv = ["w", "-s"]

[[command]]
name = "Contêineres"
argv = ["docker", "ps"]
timeout = 10

[[command]]
name = "Limpar o Docker"
argv = ["docker", "system", "prune", "--force"]
confirm = true
timeout = 120
```

O HUD vai mostrar a prévia do painel; aperte `s` para gravar.
````

## Fora do HUD

Num terminal comum (Claude Code ou Codex fora do HUD), edite o arquivo direto:

1. Rode `hud --check` para ver a fonte atual e o caminho do `comandos.toml`.
2. Se ele ainda não existe, crie-o a partir dos comandos que `hud --check`
   lista (ou dos `[[command]]` do `config.toml`), com permissão 600:
   `install -m 600 /dev/null ~/.config/hud/comandos.toml` (Linux e macOS).
   No Windows, crie em `%APPDATA%\hud\`.
3. Escreva o arquivo inteiro seguindo o esquema e as regras acima.
4. Valide com `hud --check`: a linha `comandos:` deve dizer `comandos.toml`, e
   cada comando recusado aparece como aviso com o motivo. Corrija até não haver
   aviso.
5. Diga ao usuário para reabrir o HUD (a lista é lida ao abrir).

Para voltar ao que era, apague o `comandos.toml`: valem de novo os
`[[command]]` do `config.toml` ou os comandos padrão.
