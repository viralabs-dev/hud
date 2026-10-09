---
title: Decisões ({{Projeto}})
aliases: [ADRs {{Projeto}}, ADRs ({{Projeto}})]
tags: [{{tag}}/registro, tipo/adr]
total: {{total}}
proximo: ADR-{{SIGLA}}-{{NNN}}
atualizado: {{data}}
---

# Decisões ({{Projeto}})

As decisões moram nesta nota, com id `ADR-{{SIGLA}}-NNN` fixo e nunca reutilizado.

## A convenção

- uma tabela por data com ID, Decisão e Consequência, e uma seção
  `## ADR-{{SIGLA}}-NNN — <título>` por decisão nesta mesma nota;
- três subseções: **Contexto**, **Decisão**, **Consequências**;
- `proximo:` no frontmatter guarda o próximo id livre e `total:` conta as decisões.

> [!important] ADR aceito nunca é editado
> Decisão revista vira **ADR novo** que referencia o antigo. Os dois continuam
> legíveis.

## {{data}} — decisões aceitas

| ID | Decisão | Consequência |
|---|---|---|
| [[Decisões ({{Projeto}})#ADR-{{SIGLA}}-001 — {{título}}\|ADR-{{SIGLA}}-001]] | {{decisão em uma linha}} | {{consequência em uma linha}} |

## ADR-{{SIGLA}}-001 — {{título}}

Estado: **aceito** · data: {{data}} · aliases: [ADR-{{SIGLA}}-001] · tags: [{{tag}}/decisoes, tipo/adr]

### Contexto

{{O problema, as forças em jogo e as opções consideradas.}}

### Decisão

{{O que foi decidido, em frases afirmativas.}}

### Consequências

{{O que fica mais fácil, o que fica mais difícil e o que passa a ser proibido.}}

## Ligações

- [[{{Projeto}}|← {{Projeto}}]] · [[Kanban ({{Projeto}})]] · [[Backlog de Melhorias ({{Projeto}})]]
