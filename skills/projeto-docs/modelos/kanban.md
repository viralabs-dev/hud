---

kanban-plugin: board
title: Kanban ({{Projeto}})
aliases:
  - Kanban {{Projeto}}
  - Board {{Projeto}}
tags:
  - {{tag}}/backlog
  - kanban
atualizado: {{data}}

---

## Kanban ({{Projeto}})



## Como usar

- [ ] [[Como usar o Kanban ({{Projeto}})|Guia compartilhado para {{dono}} e agentes]] — uma atividade = um card + nota em `06-backlog/atividades/`, id `{{ID}}-NNN` fixo; estado vem do código, nunca da conversa; arquivo vivo (card, nota, Backlog e plano no mesmo passo, com data e evidência); card concluído não é apagado; gesto humano fora de lane; "A fazer" em P1 → P3.


## Plano de entrega

- [ ] **Atualizado em {{data}} (Rodada 1)** · paralelismo máximo: {{N}} subagentes + principal · caminho crítico: {{ID}}-NNN → {{ID}}-NNN → {{entrega}} · o plano: [[Plano de execução paralela ({{Projeto}})]]
- [ ] **Rodada 1 — {{planejada|disparada|fechada}} em {{dd/mm}}:** Lane A — {{ID}}-001 (`{{área A}}`); Lane B — {{ID}}-002 (`{{área B}}`). Áreas disjuntas. **Principal:** {{arquivos compartilhados e integração}}.
- [ ] **Sequencial / não paralelizar:** mesmo arquivo — {{arquivo e quem fica com ele}}; dependência real — {{X depois de Y}}; recurso único — {{VM, banco, ambiente}}; gesto humano — {{atividades}}; arquivo compartilhado — este board, as notas de atividade, o Backlog, o plano e o [[{{Projeto}}]] só com o agente principal.
- [ ] **Regras de alocação:** um subagente por lane, todas as lanes da rodada disparadas numa única mensagem; cada lane de código trabalha num worktree isolado e entrega um branch; o principal confere o diff, integra, roda a suíte e atualiza o board no mesmo passo; card de gesto humano ou com decisão pendente nunca vai para lane; replanejar a cada rodada fechada.


## A fazer

- [ ] [[{{ID}}-001 — {{título da atividade}} ({{Projeto}})]] — P1 · {{rodada ou "gesto humano"}} · {{próximo resultado verificável}}


## Em andamento



## Bloqueado



## Concluído



## Ligações

- [ ] [[Plano de execução paralela ({{Projeto}})|Plano de coordenação dos agentes]]
- [ ] [[{{Projeto}}|← {{Projeto}}]] · [[Backlog de Melhorias ({{Projeto}})]] · [[Decisões ({{Projeto}})]]
- [ ] [[Sincronização Pendente (Vault, {{Projeto}})]] · [[Docmap (Vault, {{Projeto}})]]




%% kanban:settings
```
{"kanban-plugin":"board","list-collapse":[true,true,true,false,false,false,false,true],"show-checkboxes":true}
```
%%
