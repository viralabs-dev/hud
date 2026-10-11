# Workflows locais e verificação

[English](workflows-local.en.md) · [comandos/configuração](commands-config.pt-BR.md)

Versão0.10.0-preview.12 instalada está em [guia local](local-preview.pt-BR.md). Fonte/settings legados são referência histórica, com fallback explícito hud-python. Contagens registram artefato específico, sem afirmar paridade completa.

Skills/projetos preparam prompts completos limitados para aprovação distinta. Settings/contexto explícitos atendem abas habilitadas; troca provider/perfil/pasta prepara adapters/scopes antes de persistir pasta lembrada. Seguidores mudam de pasta, fixos conservam a sua; escolha da sessão. Mudanças pertinentes revogam confirmação/resume quando necessário. Campos desconhecidos/excesso falham sem truncamento silencioso.

Custom/comandos/documentos/config seguem prévia → aprovação distinta criar/sobrescrever/aplicar. Esc/descarte não grava. Gravar/recarregar não lança feeds/comandos; feed comando exige confiança separada do config exato. Rollback com recibo compara bytes/identidade e recusa edições manuais/recuperação incompleta. Veja [revisão](review-workflows.pt-BR.md).

Backends produção/fake têm capabilities distintas. Worker observa candidatos PATH apenas por metadados; wrapper seleciona manifesto pinado preparado em worker sem chamar CLIs na abertura. `/agentes` mostra prontidão por provider; `agent detect` exige revisão/aprovação própria, distinta do prompt. Auth/billing/config desconhecidos e Full de assinatura não suportado permanecem bloqueados. [Gates reais](real-launcher.pt-BR.md) governam aceite.

Camadas de testes: domínio/parsers; falhas filesystem privado; protocolo/processos fake; PTY do binário candidato/instalado; terminal físico; plataforma/provider nativos. PTY usa sessão real de protocolo terminal com eventos injetados, sem desktop físico/provider real. Capturas históricas usam fixtures e conservam revisão fonte. MSRV/suporte nativo são gates separados.

A cada pedido, conferir projeto HUD no Vault e documentação do repositório, identificar IDs/dependências/impactos e preservar alterações concorrentes. Implementar só escopo autorizado; registrar evidência/status do trabalho real. AT-097 acompanha esta reconciliação repo/Vault. Recibos novos substituem orientações ativas; checkpoints antigos ficam claramente arquivados.
