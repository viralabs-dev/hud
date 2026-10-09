---
title: Threat Model ({{Projeto}})
aliases: [STRIDE {{Projeto}}, Modelo de Ameaças ({{Projeto}})]
tags: [{{tag}}/seguranca, tipo/threat-model, seguranca]
metodo: STRIDE
atualizado: {{data}}
---

# Threat Model ({{Projeto}})

> {{Uma frase: onde está a superfície de ataque (rede, arquivos, texto que entra, terceiros).}}

## Superfície

```mermaid
graph LR
  F["{{fonte de entrada}}"] -->|{{o que entra}}| P["{{componente}}"]
  P -->|{{o que sai}}| D["{{destino}}"]
```

## STRIDE

### S · Spoofing

| ameaça | mitigação | lacuna |
|---|---|---|
| {{ameaça}} | {{mitigação}} | {{lacuna, ou —}} |

### T · Tampering

| ameaça | mitigação | lacuna |
|---|---|---|
| {{ameaça}} | {{mitigação}} | {{lacuna, ou —}} |

### R · Repudiation

| ameaça | mitigação | lacuna |
|---|---|---|
| {{ameaça}} | {{mitigação}} | {{lacuna, ou —}} |

### I · Information disclosure

| ameaça | mitigação | lacuna |
|---|---|---|
| {{ameaça}} | {{mitigação}} | {{lacuna, ou —}} |

### D · Denial of service

| ameaça | mitigação | lacuna |
|---|---|---|
| {{ameaça}} | {{mitigação}} | {{lacuna, ou —}} |

### E · Elevation of privilege

| ameaça | mitigação | lacuna |
|---|---|---|
| {{ameaça}} | {{mitigação}} | {{lacuna, ou —}} |

## Ligações

- [[{{Projeto}}|← {{Projeto}}]] · [[Segurança ({{Projeto}})]]
