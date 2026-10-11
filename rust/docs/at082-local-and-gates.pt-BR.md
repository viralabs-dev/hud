> Sequência histórica: instruções antigas hud-rs separado/Python padrão não descrevem entrada Rust hud atual; consulte README corrente.

> **Registro histórico.** Não usar os comandos de instalação/rollback deste checkpoint para a prévia atual. Consulte [guia local atual](local-preview.pt-BR.md) e recibo do artefato instalado.

# AT-082 local e gates concretos



## AT-082 local concluída e gates concretos — 2026-10-11

AT-082 Concluído nos critérios locais: `/doc salvar` revisa todo o lote, exibe existentes/propostas e exige segunda aprovação; `/doc descartar`/`limpar` descarta propostas e aprovação. Precheck compara caminho/root/cadeia/inode/bytes antes da primeira escrita, e cada apply revalida CAS. Lote não é uma transação global: corrida posterior pode gerar resultado parcial explicitamente comunicado.

Defaults Linux equivalentes ao Python carregam sem importação (arquivo/manifesto explícito, inclusive vazio, tem prioridade); `/r` aceita 1–9/0, F1–F10 e nome parcial único sem distinguir maiúsculas. Ambiguidade recusa; confirmações existentes permanecem. Nenhum comando default foi executado nos testes. Tab completa slash sem argumentos ou alterna notas→Claude→Codex→OpenCode habilitados, preservando draft/cursor; Alt+setas/Alt+Z durante edição mantêm foco/ênfase acessíveis.

**Evidência:** 350 testes Rust; fmt, Clippy all-targets -D warnings e build locked/offline passaram. PTYs AT082/render, recuperação, workflows e settings passaram apenas fixtures/fake, incluindo termios. Revisão independente concluída; old-content preview e CAS precheck de inode corrigidos. Python anterior 495 passed/19 skips; Python não alterado. Logs `/tmp/hud-at082-tests.log`, `/tmp/hud-at082-clippy.log`, `/tmp/hud-at082-build.log`, `/tmp/hud-at082-check_at082_pty.py.log`, `/tmp/hud-at082-check_review_pty.py.log`, `/tmp/hud-at082-check_workflows_pty.py.log`, `/tmp/hud-at082-check_settings_pty.py.log`.

Código: `/home/danielsouza/dev/hud/rust/crates/hud-tui/src/doc_app.rs:74`, `command_app.rs:240` (defaults), `:262` (selector), `application.rs:279` (Tab), `scripts/check_at082_pty.py`. Candidato checkout `0.8.0-preview.8`, não instalado nesta rodada; `hud-rs` continua preview7, core SHA256 `8bc35eb7f28b2104a4f440f1d9c9f1d1319eaa78fafa1d79d5b68a02acec2990`. Python/dados preservados, sem providers reais, novo MSRV, Actions, commit/push, GUI ou publicação.

### Todas as dez bloqueadas — mínimo para destravar

| Atividade | Natureza | Mínimo concreto |
| --- | --- | --- |
| [[AT-012 — a tela é testada no COSMIC Terminal real (Hud)\|AT-012]] | Terminal físico COSMIC | Daniel executar roteiro COSMIC final (teclas, cores, resize 80×24), registrar versões e resultados. Sem conta/pagamento. |
| [[AT-055 — a tela do Windows ganha validação específica (Hud)\|AT-055]] | Windows + terminal físico | Executar roteiro-windows.ps1 em Windows Terminal e conhost: Unicode, mouse, teclas, resize e restauração. Smoke não encerra visual. |
| [[AT-068 — a TUI Rust mostra arraste com prévia e alvo de drop (Hud)\|AT-068]] | Terminal/mouse físico | No Rust instalado: press/motion/release, alvo destacado, cancelamento, foco, resize e Unicode. Harness conectado não prova gesto físico. |
| [[AT-070 — o Rust lê Vault agenda e notas com paridade (Hud)\|AT-070]] | Código + filesystem/clock nativos | Implementar armazenamento seguro Windows e clock/stamps não-Unix; exercitar fixtures Windows/macOS. agenda_store.rs:627 e calendar.rs:106/137 ainda recusam alvos. Integração pessoal exige destino escolhido, sem adoção implícita. |
| [[AT-072 — os três agentes Rust preservam os perfis e protocolos (Hud)\|AT-072]] | Código + CLI/conta/rede/quota | Implementar launcher real separado do fake, validar executável/versão/ambiente e lifecycle. Depois testar leitura/completo, secrets sintéticos, resume, timeout e cancelamento por CLI/SO. agent_app.rs usa start_adapter_fixture; login sozinho não destrava. Chamadas reais podem consumir quota/dinheiro e não foram executadas. |
| [[AT-073 — skills documentação uso e monitor chegam ao Rust (Hud)\|AT-073]] | Código nativo + schemas/decisão de produto | Portar leitura privada e SQLite/monitor fora de Linux/POSIX, validar schemas reais/versionados. Decidir títulos/estado conservadores versus inferência Python. Leitura de metadados local não exige chamada paga. |
| [[AT-074 — as métricas Rust têm backends por plataforma (Hud)\|AT-074]] | SO/hardware nativo | Compilar e medir backends macOS/Windows/ARM para CPU/memória/rede/discos/uptime/sensores, confrontar unidades/erros com observações locais. Adapters existentes não comprovam FFI/hardware. |
| [[AT-075 — os pacotes Rust passam na matriz de paridade (Hud)\|AT-075]] | Código de distribuição + toolchain/ambientes | Criar/validar pacote e instalação por alvo, Linux ARM64/macOS Intel+ARM/Windows; executar MSRV 1.88 real, smoke e rollback. package_local.py aceita somente Linux x86_64; compilador 1.99 não comprova 1.88. Nenhuma instalação de toolchain realizada. |
| [[AT-076 — a troca para Rust é reversível e só ocorre após aceite (Hud)\|AT-076]] | Perdas essenciais + validações técnicas | Autorização LOCAL para adotar Rust no comando hud JÁ existe. Falta fechar launcher/providers reais AT-072 e validações/distribuição AT-075 antes de abandonar o comportamento Python; AT-082 local agora concluída. Inventário, importação e rollback locais existem. Aceite final de paridade é separado da autorização; não pedir novamente como ausente. |
| [[AT-080 — o SQLite Rust lê metadados sem cópia integral a disco (Hud)\|AT-080]] | Implementação extensa + decisão de produto | Decidir manter snapshot seguro ou construir VFS read-only sobre FD para leitura direta, com locks POSIX/journal/WAL/SHM, identidade/truncation/writer concorrente/cancelamento/ABI e revisão. sqlite_monitor.rs ainda copia DB/WAL; não remover NOFOLLOW. Sem conta/pagamento. |

Nenhuma atividade fica artificialmente Em andamento: o lote local autorizado termina aqui. Gates podem exigir código de outras frentes, além do ambiente/decisão listado; não são declarados como mera falta de login/autorização. Não foram realizadas ações pendentes de autorização.

**Contagem atual:** 90 IDs / 91 arquivos: 80 Concluído, 0 Em andamento, 0 A fazer, 10 Bloqueado. Por arquivo, 81 Concluído; colisão histórica AT-063 preservada. AT-090 concluiu o ajuste de oito seções, com histórico arquivado e cards preservados.
