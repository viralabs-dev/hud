---
title: Plano de execução paralela ({{Projeto}})
aliases: [Plano de entrega e subagentes ({{Projeto}}), Plano de entrega {{Projeto}}, Rodadas de subagentes ({{Projeto}})]
tags: [{{tag}}/operacao, {{tag}}/backlog, agentes, planejamento]
estado: vivo
atualizado: {{data}}
---

# Plano de execução paralela ({{Projeto}})

Esta é a nota de plano de entrega e subagentes do {{Projeto}} (alias: *Plano de entrega e subagentes ({{Projeto}})*).

Quadro = estado; plano = coordenação. Diz **em que ordem e com quantos subagentes** as atividades abertas do [[Kanban ({{Projeto}})]] andam, dividindo o máximo possível. Divergiu do board? **Vale o board**, e este plano é replanejado.

## Rodada 1 — {{tema}} — {{planejada|disparada|fechada}} em {{data}}

Pedido do dono de {{data}}: "{{pedido}}". Base: {{branch e commit de partida}}.

**Paralelismo máximo: {{N}} subagentes.** **Caminho crítico:** {{ID}}-NNN → {{ID}}-NNN → {{entrega}}.

| Lane / responsável | Atividade | Escopo e isolamento | Dependência / aceite | Estado |
|---|---|---|---|---|
| A | [[{{ID}}-001 — {{título}} ({{Projeto}})\|{{ID}}-001]] | `{{área A}}`, worktree próprio | {{aceite}} | {{A fazer, Em andamento desde dd/mm ou Concluído dd/mm · commit}} |
| B | [[{{ID}}-002 — {{título}} ({{Projeto}})\|{{ID}}-002]] | `{{área B}}`, worktree próprio | {{aceite}} | {{estado}} |
| Principal | integração | único escritor dos arquivos compartilhados, do board e dos índices | suíte inteira verde | {{estado}} |

### Sequencial — não paralelizar

- **Mesmo arquivo:** {{arquivo}} só com {{quem}}.
- **Dependência real:** {{X depois de Y}}.
- **Recurso único:** {{VM, banco, tenant, ambiente}}.
- **Gesto humano ou decisão:** {{atividades}} — fora de lane.
- **Arquivo compartilhado:** board, notas de atividade, [[Backlog de Melhorias ({{Projeto}})]], este plano e [[{{Projeto}}]] ficam com o principal.

### Regras de alocação

1. **Um subagente por lane**, e as lanes de uma rodada são **disparadas juntas, numa única mensagem**.
2. **Worktree isolado** em toda lane que toca código; cada lane entrega um branch.
3. **Arquivos compartilhados ficam só com o agente principal.** O subagente **relata**; o principal escreve.
4. **O principal integra**: confere o diff, roda a suíte, atualiza board, nota e Backlog no mesmo passo.
5. **Card de gesto humano ou com decisão pendente nunca vai para lane.**
6. **Nenhuma lane instala dependência** no ambiente do dono sem pedido.

## Ligações

- [[Kanban ({{Projeto}})|← Kanban]] · [[{{Projeto}}]] · [[Como usar o Kanban ({{Projeto}})]]
