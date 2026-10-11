> Sequência histórica: instruções antigas hud-rs separado/Python padrão não descrevem entrada Rust hud atual; consulte README corrente.

# Checkpoints Rust históricos

Arquivo preservado na reconciliação de 2026-10-11. Recibos passados, sem orientar instalação ou abertura atual. Consulte [README atual](../README.pt-BR.md) e installation.json.

# HUD Rust checkpoint6 integrado

Configuração explícita privada `--agent-config SETTINGS.toml|json` é consumida pelas três abas, sempre com `--fake-agent` verificado. `[claude]`, `[codex]`, `[opencode]` são obrigatórios neste formato independente; não importa silenciosamente o config.toml pessoal do Python. Executáveis reais indicados não são resolvidos nem executados.

Settings suportados: enabled, executable absoluto, cwd, model, timeout10–3600, profileleitura/completo, follow_folder, read_dirs/read_rules, max_budget_usd em centavos e full_permission_mode permitido. ToolsClaude aceita apenas subset seguro Read/Grep/Glob; outros tools falham explicitamente. Valores LANG/OPENCODE_CONFIG_CONTENT são as únicas entradas do ambiente de fixture; credenciais são recusadas. Cwd/read_dirs existentes e root / recusado. Estado follow/fixo é da sessão.

`--agent-context PRIVATE_TEXT` acrescenta texto completo a todas as abas, junto de instruções HUD/custom/doc e caminhos context.custom_root/custom_skill/templates_root. Limites64KiB/20000scalars recusam excesso sem truncar. `/pasta` prepara adapter/context/scopes antes de persistir e aplica apenas seguidores. `/novo` recompõe contexto inicial; perfil mantém destino/contexto. Documento usa doc from-agent → doc preview INDEX → doc save/overwrite. Salvar/importar propostas custom pela conversa segue pendente; hudPython é preservado.

Este pacote não codifica o estado de instalação da máquina: consulte installation.json criado pelo instalador. Prévia mantém providers reais e migração pessoal desligados. Os checkpoints abaixo são históricos.

# Checkout seguinte 0.6.0-preview.6 — ainda não instalado

A preview5 validada foi instalada em `hud-rs`, com rollback real conferido. `hud` permanece Python. O checkout seguinte adiciona histórico CPU colorido por nível, LOAD com tendência e processos alinhados, relógio local adaptativo e consumo de adapters preparados por workers restritos a fake com SHA verificado. Não resolve nem executa providers reais configurados. Loader pessoal e integração de settings/contexto na TUI ainda faltam. Veja `docs/workflows-local.pt-BR.md`; os checkpoints abaixo são históricos.

# Candidato HUD Rust 0.5.0-preview.5

Candidato local Linux x86_64, ainda sem instalação. O launcher `hud-rs` instalado permanece em 0.4.0-preview.4 e `hud` permanece Python. Sem publicação, Actions ou provedores reais.

Esta rodada integra TOML/JSON explícito e ordem F1–F10, `/skill` com corpo completo e intenção, `/project` com descoberta e contagem do Kanban já carregado, `/pasta` com restauração/CAS e abas que seguem ou fixam pasta, e `/perfil` com contratos Python e execução apenas fake verificada. O parser TOML é um subconjunto: formatos não suportados são recusados explicitamente. Não importa configuração pessoal automaticamente.

O visual usa referência Python em 80×24, 120×40 e 160×50: padding, título, bordas, cores, foco, compositor Unicode, métricas e escolha por prioridade quando a altura diminui. Capturas são de processos reais em PTY; não substituem validação física. Histórico colorido, tendência LOAD e relógio mais rico são refinamentos pendentes.

Digite normalmente no compositor; `:` inicia comando. `Ctrl+C` encerra. Agenda exibida com índices a partir de 1 e calendário local. Dados Rust ficam em diretório privado explícito; não há migração de agenda/notas Python.

Veja [workflows e limites](workflows-local.pt-BR.md). As seções abaixo registram checkpoints históricos e não descrevem o candidato atual.

## Histórico de checkpoints

# Prévia HUD Rust 0.1.0

Prévia experimental local para Linux x86_64, instalada com nome separado `hud-rs`. Python HUD 0.18.0 e seu lançador continuam intactos. Não há publicação, Actions, commit ou corte para Rust. Build de desenvolvimento; não é pacote estático portátil.

Já funciona: métricas Linux reais (CPU/memória/rede/disco/uptime), layout com prévia/alvo de arraste, foco Tab/Shift-Tab/Alt+setas, Alt+z, Esc e resize; persistência Rust isolada; Vault read-only por caminho explícito; agenda pt-BR com gestos de salvar; comandos de argv fixo com confirmação/cancelamento; visualizações explícitas de skills/uso/metadados.

Abra `hud-rs` em um terminal; `:` abre entrada, Enter envia, Esc cancela, q sai fora da entrada. Agenda: `agenda add 2026-10-11 14:30 revisar HUD`, `agenda done 0`, `agenda remove 0`. Índices são zero-based; datas relativas usam UTC nesta prévia. `--data-dir` é necessário para salvar; nada escreve no Vault. Comandos: `run NAME`, `yes` somente confirma o pendente, `cancel`. Só existe execução quando um arquivo JSON privado foi explicitamente selecionado por `--command-file`.

Opções: `--state-dir` para layout, `--data-dir` para agenda, `--vault-dir` para leitura de notas; `--skills-dir`, `--claude-cache`, `--codex-rollout`, `--monitor-file` para recursos escolhidos. Diretórios pessoais Python nunca são importados automaticamente. Pais devem existir; diretórios finais de estado são privados. Scan Vault é inicial nesta fatia, sem watcher ao vivo.

Validação Linux: 109 testes Rust, 9 de importação e 2 de empacotamento; fmt, clippy warnings deny, build locked/offline. PTY sintético cobre arraste, foco, resize, restauração, agenda Unicode/reinício, confirmação/cancelamento de comandos e Vault sem escrita. Não substitui teste físico. Rust/Cargo1.99.0; MSRV declarado1.88 ainda não testado.

Os protocolos e o supervisor Codex/Claude/OpenCode estão em crates com fixtures; agentes reais ainda não estão expostos na TUI e não foram executados/autenticados. Faltam streaming, paridade de customização/geometria, documentos/instalação de skills, monitor completo, watchers, testes físicos e macOS/Windows/ARM. Importação é ponte offline explícita, guarda backup byte-exato e exige aceite da perda de geometria; layouts customizados são recusados. Falha após rename pode deixar nova versão visível: aplicação reconcilia e informa durabilidade incerta.

Veja [paridade](parity.pt-BR.md), [dependências](dependencies.pt-BR.md), [importação](import.md) e [instalação](local-preview.pt-BR.md). Manter Python como fallback. Provedores reais exigem confirmação separada. Não abra janelas fora da área de trabalho escolhida para teste físico.


## Checkout seguinte (prévia instalada preservada)

144 testes do workspace, clippy, build locked/offline e PTY com fixtures passaram no Linux. `--custom-file PRIVATE_JSON` seleciona geometria/texto/arquivo; command feeds e drag customizado recusados até trust/persistência dos metadados. `vault refresh` e `vault search TEXTO` usam worker. Supervisor streaming e store de documentos integrados como bibliotecas, validados com fakes/fixtures; conexão visual ainda pendente. Parsers macOS/Windows não equivalem a validação nativa; fonte Windows Unsupported. A instalação 0.1.0-preview permanece com o binário anterior de 109 testes.


## Integração seguinte no checkout (não instalada)

162 testes Rust, clippy/fmt/build locked offline e PTYs ampliado/shutdown passaram no Linux; suíte Python .venv 495 passou (19 skipped). `--custom-file PRIVATE_JSON --custom-state-dir DIRETORIO_PRIVADO_EXISTENTE` permite drag persistente com metadados (mudança de quantidade de colunas recusada). `--doc-root ROOT --doc-proposals ARQUIVO_PRIVADO` expõe `doc preview 1`, `doc save` (novo), `doc overwrite` (existente); Esc/foco/resize/comando alheio revogam a prévia. `--fake-agent PATH --agent-dir DIR` aceita somente build fake local verificado por bytes: `agent codex|claude|opencode`, `ask PROMPT`, `yes`, `cancel`. Cadeia de diretórios do build/executável precisa ser privada e não gravável por terceiros; se checkout tem ancestrais group-writable, usar build privado isolado. Provedores reais continuam desabilitados. Instalação preservada. Veja [matriz e gates](checkpoint-local.pt-BR.md).
