> Legacy sequence only: old separate hud-rs/Python-primary instructions are not the current Rust hud entry; see current README.

# Historical Rust checkpoints

Archived during the 2026-10-11 documentation reconciliation. These are past receipts, not current installation or launch instructions. Use [the current README](../README.md) and installation.json for current status.

# HUD Rust — checkout 0.10.0-preview.10

AT-091: launcher explícito `--real-agent-manifest PRIVATE_JSON --agent-dir DIR`, com versão declarada/pin SHA, ambiente filtrado, revisão REAL e confirmação antes do spawn. Validado exclusivamente com executáveis simulados; providers reais/autenticação/quota não exercitados. AT-092: contratos portáveis de storage/calendário e source Windows clock; backend Windows storage e validação nativa continuam pendentes.

A preview9 está instalada em `hud-rs`; rollback para8 validado. Este é o candidato10; installation.json registra o estado da instalação. Preview9 preservada como rollback. AT-093 adiciona política pura Windows com fixtures; integração nativa continua pendente. AT-094 corrige monitoramento assíncrono e recusa tags citadas fora de envelope explícito. 378 testes workspace, fmt e clippy passaram; notificação de CLI real permanece sem validação. AT-095 integra autenticação nativa da assinatura, detecção confirmada de versão/login, gates independentes de billing/configuração e avisos de estimativa/quota. 386 testes e PTYs simulados passaram. Não houve prompts reais; login não prova ausência de cobrança extra. Custo estimado de tokens não comprova cobrança da assinatura nem extra usage desativado. Veja [launcher, contratos e gates](real-launcher.pt-BR.md). Os checkpoints seguintes são históricos.

# HUD Rust checkpoint6 integrado

Configuração explícita privada `--agent-config SETTINGS.toml|json` é consumida pelas três abas, sempre com `--fake-agent` verificado. `[claude]`, `[codex]`, `[opencode]` são obrigatórios neste formato independente; não importa silenciosamente o config.toml pessoal do Python. Executáveis reais indicados não são resolvidos nem executados.

Settings suportados: enabled, executable absoluto, cwd, model, timeout10–3600, profileleitura/completo, follow_folder, read_dirs/read_rules, max_budget_usd em centavos e full_permission_mode permitido. ToolsClaude aceita apenas subset seguro Read/Grep/Glob; outros tools falham explicitamente. Valores LANG/OPENCODE_CONFIG_CONTENT são as únicas entradas do ambiente de fixture; credenciais são recusadas. Cwd/read_dirs existentes e root / recusado. Estado follow/fixo é da sessão.

`--agent-context PRIVATE_TEXT` acrescenta texto completo a todas as abas, junto de instruções HUD/custom/doc e caminhos context.custom_root/custom_skill/templates_root. Limites64KiB/20000scalars recusam excesso sem truncar. `/pasta` prepara adapter/context/scopes antes de persistir e aplica apenas seguidores. `/novo` recompõe contexto inicial; perfil mantém destino/contexto. Documento usa doc from-agent → doc preview INDEX → doc save/overwrite. Salvar/importar propostas custom pela conversa segue pendente; hudPython é preservado.

Este pacote não codifica o estado de instalação da máquina: consulte installation.json criado pelo instalador. Prévia mantém providers reais e migração pessoal desligados. Os checkpoints abaixo são históricos.

# Checkout seguinte 0.6.0-preview.6 — ainda não instalado

A preview5 validada foi instalada em `hud-rs`, com rollback real conferido. `hud` permanece Python. O checkout seguinte adiciona histórico CPU colorido por nível, LOAD com tendência e processos alinhados, relógio local adaptativo e consumo de adapters preparados por workers restritos a fake com SHA verificado. Não resolve nem executa providers reais configurados. Loader pessoal e integração de settings/contexto na TUI ainda faltam. Veja `docs/workflows-local.pt-BR.md`; os checkpoints abaixo são históricos.

# HUD Rust 0.5.0-preview.5 candidate

Local Linux x86_64 candidate, not installed. Installed `hud-rs` remains preview4; `hud` remains Python. Explicit TOML/JSON commands preserve declaration order and F1–F10. Skill/project/folder/profile workflows use private fixtures and verified fake providers only. No automatic personal configuration import. TOML is a deliberately bounded subset, with unsupported syntax rejected.

Visual refinement follows Python padding, borders, colors, hierarchy, Unicode composer and metric priority at 80×24, 120×40 and 160×50. Actual PTY captures complement render tests; physical terminals and other operating systems remain unvalidated. Normal keys enter text; `:` starts a command; Ctrl+C exits. Agenda uses displayed 1-based indices and local calendar dates.

See [current workflow evidence and limitations](workflows-local.pt-BR.md). The following sections are historical checkpoints.

## Checkpoint history

# HUD Rust 0.1.0 local preview

Experimental Linux x86_64 development build, separate `hud-rs` command. Python HUD0.18.0 and its launcher remain untouched. No release, workflow or cutover.

Implemented: live Linux metrics; visible drag/drop target, keyboard focus, emphasis, resize/cancel; private Rust-only layout/agenda storage; explicit read-only Vault selection; Portuguese agenda operations; configured literal-argv commands with confirmation/cancellation; explicit skills/usage/session-metadata views. Press : to enter a command, Enter submits, Esc cancels, q exits outside input. Relative agenda dates use UTC in this preview. Vault scan is initial, without live watcher.

Options: --state-dir, --data-dir, --vault-dir, --command-file PRIVATE_JSON, --skills-dir, --claude-cache, --codex-rollout, --monitor-file. No automatic Python configuration reads/imports. Agenda requires selected private data directory. Commands are run NAME, yes (pending approval only), cancel. Parent state directories must exist.

Linux validation:109 Rust tests,9 import tests,2 packaging tests; fmt, clippy warnings denied and locked/offline build. Headless PTY covers Unicode agenda/restart, drag, keyboard focus, resize, terminal restoration, command approval/cancel and read-only Vault. Physical behavior remains untested. Toolchain1.99.0; declared MSRV1.88 not separately validated.

Agent protocol/process crates use fake fixtures only; real agents are not exposed in the TUI. Streaming, custom geometry/panels, document writes, skill installation, full monitor/watchers and macOS/Windows/ARM remain parity gates. Explicit offline Python layout import preserves backup bytes and requires accepting geometry loss; custom definitions fail closed. After-rename errors can expose new state: the application reconciles and reports uncertain durability. This dynamically linked local build is not a portable static distribution.

See [parity](parity.en.md), [dependencies](dependencies.md), [import](import.md), [local installation](local-preview.en.md). Python remains fallback. No real providers should run without separate confirmation.


## Next checkout checkpoint (installed preview unchanged)

144 workspace tests, clippy, locked offline build and fixture PTY passed on Linux. Explicit `--custom-file PRIVATE_JSON` selects custom geometry/text/file feeds; command feeds and custom drag are refused pending trust/metadata persistence. Vault refresh/search run on a worker (`vault refresh`, `vault search TEXT`). Streaming agent registry and reviewed document store are integrated libraries tested with fakes/fixtures; their UI integration remains pending. macOS/Windows parsers are fixture-tested, not native validation; Windows source is Unsupported. Installed 0.1.0-preview remains the previous 109-test binary.


## Integrated checkout checkpoint (not installed)

162 Rust tests, clippy/fmt/locked offline build, extended and shutdown PTY passed on Linux; existing Python venv suite 495 tests passed (19 skipped). `--custom-file PRIVATE_JSON --custom-state-dir EXISTING_PRIVATE_DIR` enables atomic persistent drag with full metadata (column-count changes refused). `--doc-root ROOT --doc-proposals PRIVATE_FILE` enables `doc preview 1`, `doc save` (new), `doc overwrite` (existing); Escape/focus/resize/unrelated commands revoke review. `--fake-agent PATH --agent-dir DIR` permits only byte-verified local fake build: `agent codex|claude|opencode`, `ask PROMPT`, `yes`, `cancel`. Fake build and executable ancestors must be private/non-writable by others; use an isolated private build when checkout ancestors are group writable. Actual providers remain disabled. Installed preview is unchanged. See [comparison and remaining gates](checkpoint-local.pt-BR.md).
