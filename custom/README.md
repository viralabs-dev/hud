# Customizações do HUD

Cada pasta aqui é um layout do HUD: `custom/<nome>/layout.toml` diz quais
painéis aparecem, em quantas colunas e com que tamanho. Painéis próprios do tipo
`texto` leem arquivos `.md`/`.txt` da mesma pasta.

| pasta | o que é |
|---|---|
| `padrao/` | o layout embutido, comentado: ponto de partida para copiar |
| `foco/` | saída grande, sistema/uso/lembretes à esquerda, Vault pequeno |
| `monitor/` | três colunas, com syslog e um painel de portas (`ss`) |

O esquema completo e as regras estão na skill `skills/hud-custom/SKILL.md`.

## Usar

Na entrada do HUD (em qualquer modo: notas, Claude ou Codex):

| comando | faz |
|---|---|
| `/custom lista` | lista as customizações |
| `/custom <nome>` | aplica (fica lembrada) |
| `/custom padrao` | volta ao layout embutido |
| `/custom salvar` | grava a customização que o Claude ou o Codex acabou de propor |

Peça ao agente dentro do HUD algo como "faça um layout com a saída maior" e,
depois da resposta, digite `/custom salvar` e `/custom <nome>`.

Fora do HUD, crie a pasta à mão e valide:

```bash
cp -r custom/padrao custom/meu-layout   # troque o `nome` no layout.toml
hud --custom-check meu-layout           # ou bin/hud --custom-check meu-layout
```

Painéis `comando` só rodam depois que você confia na customização (o HUD mostra
o `argv` e pede "s"); se o arquivo mudar, ele pede de novo.

## Compartilhar

1. Valide com `hud --custom-check <nome>`.
2. Faça commit só da pasta `custom/<nome>/` e abra um pull request.

> **Atenção: este repositório é PÚBLICO.** Nunca ponha credencial, token, senha,
> e-mail ou caminho pessoal (`/home/<usuário>/…`) num `layout.toml` ou nos
> arquivos de texto. Use `~` nos caminhos.
