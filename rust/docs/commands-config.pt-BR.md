# Comandos e configuração explícita

> **Escopo dos links:** referências a crates/scripts/código/Python são do checkout. O pacote local inclui docs/licenças, sem código/toolchain; esses links de fonte não existem dentro do archive.

Diferença documental somente no checkout: textos de `--help` sobre confiança de feeds/readiness corrigidos após o aceite da 0.10.0-preview.12 instalada. O binário instalado mantém o pin do recibo (388 testes); suas legendas antigas de help não descrevem corretamente a confiança dos feeds. Use este guia para esse comportamento. Help corrigido compilado e conferido localmente; não é novo artefato instalado.

[English](commands-config.en.md) · [CLI](../crates/hud-tui/src/main.rs) · [dispatcher](../crates/hud-tui/src/application.rs)

`hud --help` lista opções. Cada flag de caminho recebe um argumento; repetidas/desconhecidas falham. Help/version/build-info saem antes de abrir terminal. Caminhos são explícitos, salvo defaults do wrapper instalado; sem importação automática Python/credenciais.

| Opções | Comportamento |
| --- | --- |
| `--data-dir`, `--state-dir` | Agenda/notas append-only e layout privados; pais precisam existir |
| `--vault-dir`, `--vault-default-dir` | Vault limitado somente leitura; pasta lembrada usa data-dir privado |
| `--custom-file`, `--custom-root`, `--custom-selection-dir`, `--custom-state-dir` | TOML/JSON custom, catálogo revisado, seleção lembrada e geometria privada |
| `--feed-trust-dir` | Pasta privada existente para confiança SHA256 do config exato; salvar custom não concede confiança |
| `--command-file`, `--commands-output`, `--command-proposals` | Declarações argv literal/propostas revisadas; carregar/gravar não executa |
| `--doc-root`, `--doc-proposals` | Destino/propostas explícitos; criar/sobrescrever com aprovações distintas |
| `--import-config`, `--import-output`, `--reviewed-config` | Export config Python revisado/aplicação explícita; original intacto |
| `--agent-config`, `--agent-context`, `--agent-dir` | Settings/contexto privados e pasta preparada explícitos |
| `--real-agent-manifest` / `--fake-agent` | Produção pinada ou fake verificado no build, mutuamente exclusivos |
| `--skills-dir`, `--claude-cache`, `--codex-rollout` | Skills/uso limitados, sem ler auth files |
| `--monitor-file`, `--monitor-dir`, `--monitor-root`, `--monitor-opencode-db`, `--monitor-process-root` | Metadados explícitos/DB/processos UID próprio; sem conversa/args/env |
| `--note-fixture-dir` | Escrita checkbox só em fixture /tmp privada; Vault real somente leitura |

Entradas principais: `/ajuda`; `/c`, `/x`, `/o` e `/agentes`; `agent detect` → `yes` consulta versão/auth sem login/prompt; `ask TEXTO` → `yes` revisa/aprova prompt; `cancel`/`/parar` cancela; `/agenda DATA [HORA] TEXTO`, `/ok N`, `/rm N` usam índices visíveis desde 1 e calendário local; `/b`, `/vault`, `/notas`, `/conflitos` consultam Vault. Texto normal na aba notas grava notas locais.

`/pasta CAMINHO|vault`, `/pasta fixo|seguir AGENTE` e `/perfil leitura|completo` fazem transições revisadas; Full de assinatura não suportado. `/skills`, `/skill NOME PEDIDO`, `/project` compõem pedido para aprovação. `/doc from-agent` → `/doc preview N` → `/doc save|overwrite` separa análise de escrita. `/custom`, `/cmd`, `/config` seguem [revisão/rollback](review-workflows.pt-BR.md). `feeds trust` → `feeds approve SHA256` confirma config exato; novo/alterado permanece negado. `run NOME|ÍNDICE`, F1–F10, `yes`, `cancel` executam argv declarado, com confirmação quando marcada.

Parsers aceitam subconjunto limitado, recusando dados desconhecidos/duplicados; não descartar chaves para aparentar compatibilidade. Feeds: intervalos5..3600s, timeout1..60s com frações, máximo2 workers, saída limitada e último snapshot conservado em falha.

`yes` vale só para operação atualmente revisada. Esc, ações pertinentes e mudanças backend/pasta/perfil revogam confirmação. Salvar não executa; ask não inicia processo. Entrada hud é Rust; descoberta PATH só inventaria candidatos e backend padrão é preparado em worker, sem execução. Seleção não elimina gates assinatura.
