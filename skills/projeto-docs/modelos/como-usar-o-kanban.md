---
title: Como usar o Kanban ({{Projeto}})
aliases: [Guia do Kanban {{Projeto}}, Procedimento de atividades {{Projeto}}]
tags: [{{tag}}/backlog, {{tag}}/operacao]
atualizado: {{data}}
---

# Como usar o Kanban ({{Projeto}})

Procedimento para {{dono}} e qualquer agente que crie, assuma, modifique, mova ou entregue atividades do {{Projeto}}. O quadro é vivo; esta nota não amplia autorização.

## Regras do quadro

- **Uma atividade = um card.** O id é fixo (`{{ID}}-NNN`) e nunca renumera. O agrupamento épico → história está no [[Backlog de Melhorias ({{Projeto}})]] e nos campos `epico:`/`historia:` de cada nota.
- **O estado é do código, nunca da conversa.** Um card só é "Concluído" se o que ele promete existe, com o arquivo e o teste que provam.
- Card concluído **não é apagado**: ganha a data e a evidência (arquivo, teste, commit).
- **Arquivo vivo:** card, nota da atividade, Backlog e plano mudam no mesmo passo de cada mudança de estado (iniciar, concluir, bloquear), com data e evidência.
- **Gesto humano** (algo que só o dono pode fazer) fica fora de lane e diz isso no card.
- **Ordem da coluna "A fazer":** P1 → P3. A prioridade mora no frontmatter da nota (`prioridade:`); o card copia.

## Antes de agir

1. Ler [[{{Projeto}}]], [[Kanban ({{Projeto}})]], a nota da atividade e [[Sincronização Pendente (Vault, {{Projeto}})]]. Conferir o estado do repositório (branch, último commit, mudanças locais). Consultar [[Docmap (Vault, {{Projeto}})]].
2. Não escrever credencial, token nem dado pessoal em nota.
3. Verificar dependências, bloqueios e trabalho já em execução no [[Plano de execução paralela ({{Projeto}})]]. Não duplicar atividade.
4. Escolher atividade desbloqueada: defeitos e caminho crítico primeiro, depois lacunas de validação, depois higiene. Publicar (push, release) exige pedido explícito do dono.

## Criar e assumir

**Pasta única de atividades:** todas as notas `{{ID}}-*` ficam em `06-backlog/atividades/`, inclusive as concluídas. Quadro, guia, plano e índices sem ID ficam na raiz de `06-backlog/`.

1. Procurar pedido equivalente no quadro, no Backlog e nos nomes de notas; reusar a atividade se o escopo já existir.
2. Reservar o próximo `{{ID}}-NNN` livre (em {{data}}, o próximo é **{{ID}}-{{NNN}}**).
3. Criar a nota em `06-backlog/atividades/` com o nome `{{ID}}-NNN — <título> ({{Projeto}}).md` e o frontmatter `title`, `aliases: [{{ID}}-NNN]`, `tags: [{{tag}}/backlog, tipo/atividade]`, `status`, `prioridade`, `epico`, `historia`, `atualizado`. Seções: **Pedido e origem**, **Escopo e aceite**, **Registro** (entradas datadas, a mais nova em cima). Título sem `/` nem `:` (são nomes de arquivo).
4. Todo card novo entra em **A fazer**; mover para **Em andamento antes de executar**, registrando responsável, data e lane. Depois Bloqueado ou Concluído conforme a evidência. Registro tardio declara reconciliação tardia, sem inventar transição.

## Editar, trocar ou mover

- Reler quadro **e** nota antes de escrever; substituições pontuais.
- Bloqueado exige causa concreta (técnica, decisão ou gesto humano), próximo passo e quem desbloqueia.
- Mudança de escopo muda o aceite na nota, com motivo e data, sem apagar histórico.
- Concluído exige critérios cumpridos, testes sobre o código final e documentação conciliada pelo Docmap. Publicado (push, release) é estado distinto, nunca inferido de teste local.

## Fechar uma rodada

1. Rodar a suíte de testes no código integrado e registrar o resultado.
2. Registrar commits, comandos, retorno e limitações na nota e no card.
3. Atualizar o [[Plano de execução paralela ({{Projeto}})]] (rodada fechada, próxima rodada) e a [[Sincronização Pendente (Vault, {{Projeto}})]].
4. Reler e conferir os wikilinks.

## Ligações

- [[Kanban ({{Projeto}})|← Kanban]] · [[{{Projeto}}]]
- [[Backlog de Melhorias ({{Projeto}})]] · [[Docmap (Vault, {{Projeto}})]]
