> Sequência histórica: instruções antigas hud-rs separado/Python padrão não descrevem entrada Rust hud atual; consulte README corrente.

> **Registro histórico.** Não usar os comandos de instalação/rollback deste checkpoint para a prévia atual. Consulte [guia local atual](local-preview.pt-BR.md) e recibo do artefato instalado.

# Integração local comparada ao Python


## Avanço comparável ao Python — integração local final (2026-10-10)

Daniel informou após abrir a prévia instalada: “Verifiquei e como versao inicial ok mas sinto que esta bem distante ainda do python, imagino que isso esteja sendo trabalhado”. Isso é **aceite da prévia inicial somente**, sem aceite de paridade, drag físico específico, outros sistemas ou migração completa. AT-068 permanece bloqueada no roteiro físico específico. A instalação e os diretórios `hud-rs-preview` não foram atualizados nesta rodada.

Implementação viável desta rodada entregue no checkout `~/dev/hud/rust`: supervisor visual **exclusivamente fake** com seleção Codex/Claude/OpenCode, ask/yes, streaming/session/cancel; documentos com prévia, aprovação create/overwrite, revogação por Esc/foco/resize/input e reconciliação após erro; drag customizado com save-before-memory e `custom.json` restaurado sem perder larguras, alturas, feeds, títulos e Unicode. `--custom-state-dir` requer diretório privado já existente; `--doc-root`/`--doc-proposals` são explícitos, sem escrita automática de respostas. Command feeds permanecem recusados. O estado customizado selecionado explicitamente é autoritativo no restart. Inserção/remoção de colunas customizadas continua recusada quando o número de colunas muda, porque a política de largura não está definida.

Revisão independente concluiu sem novo achado bloqueante após três correções: P1, comando ativo sobrevivia a q (Running agora cancela+join); P2, fila final de eventos era perdida (Completion devolve pending_events após join); P2, doc indisponível deixava confirmação antiga válida (revogação antes do retorno). Regressões comprovam cada correção. Limites aceitos permanecem: concorrência residual mesmo UID, POSIX validado, durabilidade incerta se fsync falhar depois do rename.

Validação final: **162 testes Rust**, clippy all-targets warnings denied, fmt check e build locked/offline; **495 testes Python, 19 skipped** no `.venv` existente; PTY original, PTY de shutdown com filho ativo/termios e PTY ampliado de customização/documentos/três protocolos fake/restart passaram. Todos em fixtures/processos inofensivos. O build fake integrado foi validado numa cópia privada `/tmp/hud-rust-final-j5ayawjn/rust`; o verificador recusou corretamente ancestrais 775 do checkout e nenhuma permissão global foi alterada. Logs: `/tmp/hud-private-test.log`, `/tmp/hud-private-clippy.log`, `/tmp/hud-python-venv-final.log`, `/tmp/hud-private-shutdown.log`, `/tmp/hud-private-pty.log`, `/tmp/hud-extended-pty.log`. Instalação permanece SHA-256 `d3a8191e627d12d5da4f6ccf1ffa5329377e9b55ec14fe6592bdb061d1e868e8`.

### Matriz funcional e visual atual

| Área | Python v0.18.0 | Rust instalado 0.1.0 inicial | Rust checkout seguinte | Lacuna concreta / atividade |
|---|---|---|---|---|
| Tela, navegação e edição | Abas/notas/agentes, render Markdown, histórico/scroll e comandos de projeto | Quatro painéis genéricos; foco/ênfase/drag básico | Mantém quatro painéis; supervisor e prévias aparecem no output, conteúdo/títulos customizados | Abas paralelas, histórico/scroll/roda, editor e render Markdown ainda distintos. AT-068/072/073 |
| Customização | TOML, seleção lembrada, modelos, trust por digest, feeds periódicos | Ausente | JSON explícito estrito, geometria/texto/arquivo, persistência e restart de metadados | Não importa TOML completo; não lembra seleção; command feeds/trust, refresh dos feeds e novas colunas pendentes. AT-073/075 |
| Agentes | Contratos Claude/Codex/OpenCode, perfis e conversação integrados | Bibliotecas/fakes sem seleção visual | Seleção visual fake, prompt/yes, streaming, sessão e cancelamento seguros | Sem provedor real/versionamento validado; perfil Full indisponível; contexto/projeto, visual de custo/rate/tools e abas não equivalentes. AT-072 |
| Documentos | Propostas oriundas dos agentes, prévia e aprovação | Parser, sem escrita na UI | Propostas de arquivo privado; preview 1/save/overwrite e recusa de stale/manual/link | Fluxo automático de extração/revisão das respostas e atalhos ainda não equivalentes; nenhum autosave. AT-073 |
| Vault e agenda | Cache/watcher, filtros/notas/recent/today, ações e datas locais | Scan inicial; agenda com UTC | Worker refresh 5 s e busca literal Unicode; erro mantém snapshot; agenda continua UTC | Cache/recent/today, estatísticas/ações de notas e timezone local pendentes. AT-070 |
| Uso e monitor | Apresentação completa de sessões/subagentes e formatos/versionamento | Uso selecionado e primeira linha de metadata | Mesma fatia read-only; saída de supervisor fake não substitui preview de docs | Monitor completo/live, várias sessões e render detalhado pendentes. AT-073 |
| Métricas e SO | Backends Linux/macOS/Windows | Linux | Linux real + parsers macOS/conversões Windows fixtures | Backend macOS sem build nativo/CPU/discos; Windows Unsupported; seleção worker e testes nativos/ARM pendentes. AT-074/075 |
| Distribuição | Releases e instaladores Python existentes | Prévia Linux lateral aceita inicialmente | Fonte local e build privado testados, sem nova instalação | Pacote novo/matriz completa/rollback/cutover dependem AT-075/076 e autorização. |

Evidências Python: `hud/ui.py:134,2120`, `hud/custom.py:108,129,173,188,217,270,347`, `hud/vault.py:204`, `hud/monitor.py:105`. Evidências Rust: `rust/crates/hud-tui/src/main.rs:69,140`, `hud-tui/src/application.rs:93`, `hud-custom/src/lib.rs:438,468`, `hud-custom/src/store.rs:252,276`, `hud-agent-process/src/worker.rs:198`, `hud-runner/src/lib.rs:531`, módulos `hud-tui/src/doc_app.rs` e `agent_app.rs`, scripts `rust/scripts/check_extended.py` e `check_shutdown.py`. Referências Rust relativas a `rust/crates/` salvo scripts. SOLID: domínio/codec puros, controllers separados para comandos/docs/agentes, traits pequenas de stores e adapters de OS; duplicação interna dos stores de JSON/Markdown é dívida de manutenção delimitada, não motivo para uma abstração universal prematura.

### Todas as tarefas abertas e seus gates restantes

| ID | Status | Trabalho local restante | Dependência externa / quem desbloqueia |
|---|---|---|---|
| AT-012 | Bloqueado | Roteiro já preparado | Daniel: teste físico Python no COSMIC, teclas/cores/resize. |
| AT-055 | Bloqueado | Roteiro/harness preparado | Windows/terminal nativos: executar e registrar teclas, mouse, Unicode e resize. |
| AT-068 | Bloqueado | Automação Linux concluída; não inventar aceite físico | Daniel: roteiro específico drag/resize/Unicode/foco no terminal real. Aceite inicial não encerra esse gate. |
| AT-070 | Em andamento | Cache/recent/today, estatísticas/ações de notas, timezone local e fixtures de paridade | SOs adicionais para validação da segurança e caminhos nativos. |
| AT-072 | Em andamento | Abas/contexto/editor e visual custo/rate/tools; fixtures do contrato de perfis | Verificar CLIs reais por versão/SO requer autorização específica e ambiente; Full não habilitado nesta autorização. |
| AT-073 | Em andamento | Trust/preview de command feeds/custom proposals, TOML/seleção lembrada, refresh feeds, extração de doc proposals da conversa, monitor completo e ergonomia/atalhos | Política de largura para colunas custom novas/removidas deve ser definida com teste; publicação/instalação de skills fora desta rodada. |
| AT-074 | Em andamento | Seleção backend em worker, CPU/discos macOS e implementação de fonte Windows | Compilação/leitura em macOS/Windows/ARM e hardware reais antes de declarar suporte. |
| AT-075 | A fazer | Atualizar pacote/checksums/licenças e matriz de paridade completa após módulos | Builds/smokes/gestos nos alvos nativos; sem Actions/publicação nesta autorização. |
| AT-076 | A fazer | Inventário/backup/importação e ensaio de rollback após AT-075 | Daniel: aceite explícito de paridade/cutover; comando Python padrão permanece. |

Prioridade recomendada: **P1 AT-073 e AT-072**, começando pelos fluxos/visual que aproximem a experiência Python; **P2 AT-070 e AT-074**, com aceites nativos separados; AT-075/076 só depois dos gates. Existem etapas locais viáveis acima: não foram falsamente marcadas bloqueadas ou concluídas. Contagem por ID preservada: 77 atividades, 68 concluídas, 4 em andamento, 2 a fazer, 3 bloqueadas (78 arquivos por colisão histórica AT-063).

## Implementação local integrada — AT-070 e avanço AT-072/073/074 (2026-10-10)

AT-070 agora tem cache incremental por identidade/metadados, filtros de quadros/recent/today, estatísticas Obsidian/cache/read e agenda visual unindo compromissos locais e tarefas datadas do Vault. Data local usa libc e DST nativo em subprocessos isolados (23/25 h); há fonte Windows Kernel32, ainda sem build nativo. Ações `note read` são read-only; `note toggle done|todo LINE PATH` + `note apply` requerem `--note-fixture-dir`, diretório privado sob `/tmp` com marcador privado `.hud-note-fixture` contendo `HUD-NOTE-FIXTURE 1\n`. Nenhuma nota pessoal foi escrita. Preview preserva bytes/CRLF/Unicode/wikilinks; aprovação recusa stale/manual/link e revalida cadeia; erro tardio informa durabilidade incerta e reread. Esc/foco/resize/entrada alheia revogam preview.

Avanço adicional integrado, não apenas staging: abas fake independentes/contexto/reset/custo/tokens/rate/tools, editor Unicode/histórico/scroll e status por provedor (AT-072); `doc from-agent` importa propostas limitadas antes da truncação visual para revisão, sem auto-save; custom JSON/TOML/aliases/blocos, seleção lembrada/revalidada, drop com origem/target e largura Python; feeds periódicos com trust do digest exato, argv literal e cancelamento; monitor metadata-only multi-sessões/filho/estado registrado, refresh em worker (AT-073). Métricas agora selecionam backend host em worker; fontes macOS/Windows implementadas em código, com CPU/discos/rede/processos/cores e limites explícitos (AT-074). Nenhum provedor real foi descoberto/executado.

Revisão independente corrigiu startup de seleção lembrada + trust dir, revogação visual quando token some e espera de leitura de feed no encerramento. Jobs exclusivamente de leitura são desanexados após cancelamento; processos têm orçamento de limpeza de 2 s. Syscalls já bloqueadas podem continuar até retornar; não há promessa de interrupção nativa. Runner pré-cancelado retorna cancelled=true antes do spawn. Configuração editada revoga trust. Command feeds não autorizados continuam recusados.

Validação já concluída: **235 testes Rust**, fmt, clippy workspace/all-targets warnings denied e build locked/offline; PTYs novos de notas e seleção lembrada passaram; PTY de shutdown confirmou filho reaped e termios restaurado. Logs `/tmp/hud-complete-test.log`, `/tmp/hud-complete-clippy.log`, `/tmp/hud-complete-build.log`, `/tmp/hud-notes-final.log`, `/tmp/hud-selection-final.log`, `/tmp/hud-shutdown-final.log`. PTY ampliado final também passou: largura/altura, drag/persistência/restart de metadados, create/overwrite/revogação documental, três protocolos fake/stream/sessão/cancelamento, isolamento de saída e termios. Log `/tmp/hud-new-extended.log`. Nenhuma nova execução da suíte Python é alegada nesta rodada.

| Atividade | Estado após integração | Restante local / gate externo |
|---|---|---|
| AT-070 | Bloqueado | Checklist local desta fatia concluído em Linux/fixtures; validação de scanner/store/caminhos nativos em outros SOs permanece externa. Sem escrita real de notas. |
| AT-072 | Em andamento | Atalhos de provedores/ergonomia, regressões visuais ampliadas e diferenças de layout ainda viáveis. Contratos CLI reais/versionamento/Full permanecem externos e não autorizados. |
| AT-073 | Em andamento | Render Markdown, aplicação visual completa de show/days dos blocos, refresh de uso/skills e formatos OpenCode além do export limitado ainda precisam ser avaliados/implementados; não concluir paridade a partir de mocks. |
| AT-074 | Bloqueado | Worker e fontes locais integrados; compilação/execução Windows/macOS/ARM, hardware e desempenho nativos são gates externos. |

Contagens atuais por ID: **77 IDs: 68 Concluído, 2 Em andamento, 2 A fazer, 5 Bloqueado**. 78 arquivos devido às duas notas AT-063 históricas, preservadas. AT-075/076 continuam A fazer; não há aceite de paridade, novo pacote instalado ou cutover. A prévia instalada continua SHA256 `d3a8191e627d12d5da4f6ccf1ffa5329377e9b55ec14fe6592bdb061d1e868e8`, e seus diretórios de dados permanecem intactos.


## Segunda prévia instalada — 2026-10-10

0.2.0-preview.2 separada, SHA256 2c1aedfb82c0c83507ba6d1af2392de0dbe4e1a87229a33e5648b6986347bd72; instalação ~/.local/opt/hud-rs/0.2.0-preview.2, launcher hud-rs-preview2, estado hud-rs-preview2. 247 testes Rust, 495 Python (19 skips), 11 scripts; fmt/clippy -D warnings/build locked offline. Seis PTYs sobre o instalado mais smoke/demo em PTY; recusa de fake adulterado/runtime digest spoof. Primeira versão/launchers/estado preservados por hashes. Revisão corrigiu teto de expansão Markdown.

Atalhos/Tab/Fkeys/Unicode; Markdown bounded com tabelas Unicode/fallback estreito; filtros show/days e IDs TOML nativos; refresh 5s skills/uso mantendo último válido; OpenCode JSON legado metadata-only; pin SHA256 do fake no build e pacote com 79 licenças. Guias e diferenças em preview2.pt-BR.md. AT-072 bloqueado em CLI real/Full/versionamento/SO; AT-073 ainda local SQLite/descoberta/apresentação monitor. Windows scanner/store POSIX continua port dependente do ambiente nativo, não paridade concluída. Nada foi publicado; Python em ddce368 e git status só rust/ aditivo.
