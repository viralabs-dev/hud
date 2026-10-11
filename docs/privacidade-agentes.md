# Privacidade e gates dos agentes no HUD Rust

[English](privacidade-agentes.en.md)

HUD Rust separa revisão e autorização do backend pinado. Abertura não inicia provider; prompt aprovado pode transmitir contexto e consumir quota. Política CLI não substitui isolamento de filesystem. Codex read-only não confina leitura à cwd; assinatura OpenCode e Full de assinatura não têm aceite nesta entrada.

[Contrato atual](../rust/docs/real-launcher.pt-BR.md) · [preflight readonly e roteiro condicionado](../rust/docs/config-preflight.pt-BR.md). Billing/política gerenciada/cloud/auth Unknown permanece bloqueando. HUD não lê arquivos de credenciais; não copiar tokens, consultar CLI ou mudar conta para substituir prova ausente. Hooks/MCP/plugins pessoais observados não provam execução no plano atual; ausência local não prova política efetiva remota.

[Contrato Python histórico](legacy/privacidade-agentes.pt-BR.md) preserva antigos argumentos/perfis/env, sem descrever o target Rust nem autorizar testes reais. Fallback é seleção explícita; não herdar permissões/dados silenciosamente.
