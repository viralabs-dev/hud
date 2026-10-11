# Migração Rust: paridade e critérios de segurança

Base verificada: Python `ddce3688548e6ebf1df62a554d0003b99f75060c`, v0.18.0, limpo durante a leitura. O contrato detalhado e referências completos estão em [parity.en.md](parity.en.md). Este documento não afirma que a implementação Rust já possui esses recursos. A revisão original do contrato não executou agentes externos; o [estado atual e evidência local](support.pt-BR.md) são registrados separadamente.

## Paridade necessária

- **Configuração e comandos:** manter TOML, pasta lembrada, avisos, resolução segura de executáveis, argv sem shell, confirmação, limite de saída/linhas e timeout. Preservar prévia de propostas antes de salvar. Evidências: `hud/config.py:150–215,273–395`; `hud/runner.py:13–108`; `hud/comandos.py:33–118`.
- **Layout:** manter formatos atuais, tamanhos mínimos, troca/encaixe/nova coluna, customização lembrada, confiança por digest e gravação privada/atômica. Evidências: `hud/layout.py:287–400,478–690,745–784`; `hud/custom.py:129–188,217–318`.
- **Interação:** foco por Alt+setas/clique, ênfase por Alt+Z, Esc, roda, edição/histórico e abas. Acrescentar arraste press→move→release com indicação visual da origem/alvo; cancelar não grava e soltar grava uma vez. O Python atual inicia no título e aplica somente ao soltar (`hud/ui.py:1486–1627`). Shift+arrastar depende da seleção do terminal.
- **Agentes:** manter disponibilidade, modelos, cores/abas, eventos JSONL, sessão retomada/nova, tokens/custo e mudança de perfil. Evidências: `hud/agent.py:20–190`, `codex.py:29–40`, `claude.py:107–136`, `opencode.py:44–66`.
- **Métricas:** manter CPU, memória, disco, rede, uptime e histórico por adaptador de SO; dado indisponível não vira zero inventado (`hud/metrics.py:43–159`, `metrics_linux.py`, `metrics_macos.py`, `metrics_windows.py`).
- **Vault e documentação:** varredura/busca limitadas, Kanban/tarefas/prazos, proteção contra links e corrida; varredura não escreve. Propostas `.md` têm prévia, destino validado e consentimento de sobrescrita (`hud/vault.py:88–200,252–380`; `hud/docs.py:116–174,241–486`).
- **Agenda, notas, skills e uso:** manter dados privados/atômicos, sem reescrever originais automaticamente; descoberta/precedência/recursos empacotados de skills; porcentagem restante e ausência de leitura de tokens OAuth/auth (`hud/agenda.py:46–179`; `skills.py:42–181`; `usage.py:62–125,216–337`).
- **Monitor:** manter processos/sessões/subagentes, limites e frescor; SQLite somente leitura; mostrar metadados, não conversas inteiras. No Windows a lista de processos ainda é ausente: não atribuir paridade inexistente (`hud/monitor.py:145–339,465–954`; `README.md:479–494`).
- **Texto e empacotamento:** retirar escapes/controles/bidi de conteúdo externo, medir células Unicode e manter Markdown estreito; binário isolado, templates/skills/licenças e comandos administrativos compatíveis. Evidências: `hud/text.py`, `markdown.py`, `__main__.py`, `pyproject.toml`.

## Contratos distintos de leitura

**Codex:** sandbox read-only + MCP desativado bloqueiam escrita/comandos de rede no sandbox, mas não confinam leitura à pasta. Arquivos que o usuário consegue ler podem virar contexto transmitido ao provedor. Não foi constatado vazamento; não prometer isolamento ausente (`hud/codex.py:29–40`).

**Claude:** Read/Grep/Glob restritos às pastas, segredos negados em ambos os perfis, MCP restrito/dontAsk em leitura, teto de gasto. O perfil completo deliberadamente usa outro modo (`hud/claude.py:29–55,95–136`).

**OpenCode:** ask por padrão, permissões nomeadas, segredos negados e diretórios externos controlados. A recusa automática no modo headless depende da CLI externa: testes de JSON não a provam. Registrar versão e testar com segredos sintéticos, sem credenciais reais (`hud/opencode.py:27–59`).

**Processos:** preservar stdin para prompts, sem shell, grupos próprios, limites e cancelamento. O ambiente de comandos usa PATH fixo; agentes usam PATH do usuário e variáveis permitidas por prefixo. Não fundir os dois ambientes (`hud/runner.py:16–38`; `hud/agent.py:33–56`). Testar filhos/netos inofensivos terminados por timeout/cancel em cada SO (`hud/plataforma.py:390–427`).

**Arquivos:** validar o descritor aberto, tipo/tamanho/identidade e cadeia de diretórios. POSIX usa dir_fd; Windows verifica reparse/identidade e tem contrato diferente. canonicalize seguido de open não substitui a proteção contra troca entre checagem e abertura (`hud/vault.py:155–200`; `docs.py:356–486`; `plataforma.py:338–370`).

## SOLID idiomático

Domínio: layout, geometria, foco, estado de arraste e regras puras. Aplicação: comandos/eventos, confirmação, cancelamento e persistência. Adaptadores: arquivos/TOML, métricas/SO, árvore de processos e protocolos dos agentes. UI: eventos Crossterm para comandos e widgets Ratatui. Dependências apontam para dentro.

Traits pequenas somente nos limites úteis: LayoutStore, ProcessSupervisor, MetricsSource, NoteReader e AgentProtocol. Strategy para agentes, adapters para SO, comandos tipados para casos de uso. Evitar trait por struct, herança simulada e camadas DTO repetidas.

## Aceite por plataforma

| Gate | Linux amd64/arm64 | macOS Intel/Apple Silicon | Windows amd64 |
|---|---|---|---|
| Build/unitários | alvo nativo, lockfile/licenças, libc mínima documentada | alvos nativos, SO mínimo | alvo nativo, runtime/MSVC documentado |
| Pacote/instalação | checksum, instalação de usuário atômica/rollback | idem; status real de assinatura/notarização | PowerShell 5.1/7, rollback e executável bloqueado |
| Smoke | fora do código; config ausente/inválida e CLI | idem | idem; espaços, drive/UNC, OEM/UTF-8 |
| Processos | grupo, timeout/cancel, filhos/netos e descritores | idem; métricas/comandos específicos | árvore, handles e shim npm sem shell cmd |
| UI automatizada | PTY real com teclas/mouse injetados, resize/persistência | idem com diferenças do decoder | harness ConPTY/console ou lacuna explicitamente registrada |
| UI física | terminal/versionado, drag com feedback, Shift, roda/Unicode | mesmo roteiro | Windows Terminal/versionado e mesmo roteiro |

Registrar SO/arquitetura, emulador, shell, dimensões e fonte. Shell não é emulador. WSL demonstra Linux; emulação x64 no Windows ARM não demonstra suporte ARM nativo. Outros SO/alvos seguem não verificados. Agentes externos continuam dependendo de instalação, autenticação, rede e versões próprias. Nenhuma linguagem garante suporte universal.

Os testes `test_ui.py` injetam eventos em PTY POSIX; CI pula tela do binário no Windows. Teste de geometria, build, instalação ou smoke não prova mouse físico. Reaproveitar os testes atuais como especificações e fixtures, sem copiá-los mecanicamente.

## Sequência histórica e gates de entrega pública

1. A sequência inicial preservou fonte/config/dados históricos e Rust começou com entrada separada. Adoção local atual sob hud em 0.10.0-preview.12 é autorizada; fallback hud-python é explícito, sem alterar originais.
2. Primeira fatia: layout real, resize, foco, feedback/drop e persistência privada, com domínio headless e restauração do terminal em erro/saída. Recursos ausentes ficam pendentes.
3. Migrar leitura antes de execução; habilitar comandos/agentes somente depois dos gates de falha/permissão. Perfil completo ausente deve ficar explicitamente indisponível.
4. Importação explícita: prévia, backup que preserve bytes, validação de schema e gravação atômica. Testar Unicode, campos desconhecidos e rollback. Confiança não passa silenciosamente para comandos alterados.
5. Entrega pública exige paridade/platform gates e autorização própria. Cutover local autorizado 0.10.0-preview.12 possui rollback verificado separado; não encerra gates nem autoriza publicação. Escopo diferencia voltar a artefato Rust anterior de fallback hud-python e dados históricos intactos.

Lacunas de aceite: gesto físico, CLIs reais, plataformas nativas/MSRV; builds/testes Linux locais têm recibos próprios. Este contrato compara comportamento histórico; arquitetura-alvo/runtime atual é Rust.
