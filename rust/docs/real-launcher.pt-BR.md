# Launcher explícito, assinatura nativa e contratos de plataforma

[English](real-launcher.en.md) · [Comandos/configuração](commands-config.pt-BR.md) · [Instalação local](local-preview.pt-BR.md)
[Revisão readonly e roteiro condicionado](config-preflight.pt-BR.md)

## Estado vigente — 0.10.0-preview.12 instalada em 11/10/2026

`hud` local e o alias `hud-rs` apontam para `$HOME/.local/opt/hud-rs/0.10.0-preview.12/abrir-preview.sh` após o cutover verificado. Esse wrapper mantém o manifesto privado e a cwd sintética preparados na preview10. Não consulta login nem inicia providers ao abrir; descoberta em PATH lê somente metadados. A preparação do backend padrão ocorre em worker. Flags explícitas `--fake-agent`, `--real-agent-manifest` ou `--agent-dir` prevalecem. O binário direto sem manifesto não tem backend.

Claude2.1.296 e Codex 0.160.1 têm contratos de assinatura implementados. OpenCode assinatura nativa não é suportada. `/agentes` mostra inventário e todos os bloqueios; `monitor` mostra metadados. `agent detect` prepara consulta limitada de versão/login, `yes` confirma só a consulta. Depois `ask TEXTO` prepara envio separado; outro `yes` só transmite se todos os gates passarem. Erro de worker revoga readiness em cache. Não assumir que login, pins ou perfil leitura garantem isolamento ou ausência de cobrança.

Billing `unknown` e configuração/política `unknown` bloqueiam. O dono deve confirmar extra desabilitado e revisar configuração efetiva no diretório exato: Claude sem hooks executáveis; Codex rota nativa OpenAI/ChatGPT sem override de endpoint/autenticação/header/provedor/cobrança e sem hooks/MCP de projeto/sistema/gerenciada. Não alterar essas declarações por suposição. HOME e cwd não são sandbox. Full não está disponível na assinatura mínima. Valores/limites de tokens são estimativas, não fatura nem teto financeiro; quota da assinatura é separada.

392 testes aprovados no recibo local da 0.10.0-preview.12. Smoke Claude dedicado passou com um prompt/uma resposta; Codex real segue não validado. Consulta readonly anterior não prova billing/política atual. O rollback do cutover está em `$HOME/.local/state/hud-rust-cutover-preview12-active`; o fallback legado é `hud-python`. Recibo `$HOME/.local/opt/hud-rs/0.10.0-preview.12/installation.json`. MSRV1.88, Windows/macOS nativos, terminais físicos, Codex e demais perfis reais continuam pendentes. O candidato Codex separado tem 395 testes e contrato smoke-isolated; ver cobertura dedicada.

A prova do wrapper instalado confirma composição real do manifesto/pins e chegada aos gates preflight. Aquele teste de entrada usou fakes para versão/auth/prompts. Probes nativas isoladas e o smoke dedicado Claude posteriores estão registrados abaixo. Seleção funcional corrigida não comprova operação real. Cobertura completa das ferramentas do turno permanece pendente: arquivos locais/chaves não secretas permitem auditoria parcial, sem auth.json/keychain/.credentials.json; política gerenciada pode prevalecer e não há comando dry-run completo comprovado nesta tarefa. Não aprovar managed_policy por suposição.

## Histórico e contratos anteriores

As seções checkpoint9/10 abaixo preservam o escopo e evidências daquela etapa; não substituem a orientação ativa acima. Em particular, `--bare` e Codex0.162.1 são contratos legados, não o contrato de assinatura instalado.

## Entrada de produção AT-091 — histórico do checkpoint9

`--real-agent-manifest PRIVATE_JSON --agent-dir PRIVATE_WORKSPACE` seleciona o launcher de produção. É mutuamente exclusivo com `--fake-agent`. Nenhum executável, configuração pessoal, versão, login ou credential é descoberto/probado automaticamente. Apenas providers presentes no manifest ficam habilitados. Um arquivo `--agent-config` explícito pode ajustar settings; providers habilitados devem ter pin e o executable precisa ser o mesmo do manifest.

Manifest privado, schema 1, até três providers distintos, 32 KiB; executable absoluto, nativo ELF/Mach-O, arquivo regular privado e cadeia de pais confiável, digest SHA256 e identidade revalidados antes do spawn. Scripts, npm shims, symlinks, FIFO, versões fora da allowlist e alterações de inode/bytes são recusados. Não relaxar guard para lançar a partir de checkout com ancestrais graváveis por grupo. O manifesto declara a versão; seu hash é um pin fornecido pelo dono, não prova independente da release/versão real.

```json
{"version":1,"providers":[{"provider":"codex","executable":"/private/provider","sha256":"REPLACE_WITH_64_HEX_DIGITS","version_output":"codex-cli 0.162.1"}]}
```

`ask PROMPT` prepara o plano e mostra revisão REAL sem iniciar processo. O overlay começa no topo, quebra Unicode, mantém o aviso de transmissão/quota e permite PgUp/PgDn/roda. Mostra provider, release declarada, SHA256, executable/cwd, perfil/modelo, timeout, contrato, quantidade de argumentos e chaves do ambiente sem valores. `yes` confirma exclusivamente o prompt pendente; `cancel` recusa/interrompe. Cada novo envio exige aprovação. Settings/folder/profile/contrato invalidam sessão quando necessário; timeout/cancel/drop limpam o grupo de processos e a fila é limitada. Caps: 1 MiB de saída total, linhas/eventos limitados, validação do agregado argv/env.

Processo usa argv literal e ambiente limpo por allowlist; secrets sintéticos OPENAI_API_KEY, ANTHROPIC_API_KEY, CODEX_TOKEN e OPENCODE_API_KEY não são herdados. HUD não lê credenciais. Isso não confina acesso do próprio CLI ao filesystem, configuração global ou rede. Ataque pelo mesmo UID entre revalidação e spawn permanece risco residual.

## Contratos legados checkpoint9 e limites

| Provider | Release fixada | Política e gate restante |
| --- | --- | --- |
| Codex | [0.162.1](https://github.com/openai/codex/releases/tag/rust-v0.162.1) | Contrato exec JSON/streaming; configuração e hooks ambiente ainda exigem validação e isolamento explícitos. |
| Claude | [2.1.296](https://github.com/anthropics/claude-code/releases/tag/v2.1.296) | `--bare` ambos perfis; ReadOnly somente Read; Full Read/Edit/Bash com deny. Bare não usa OAuth/keychain: integração de autenticação explícita continua pendente. Bash não isola secrets. |
| OpenCode | [1.19.0](https://github.com/anomalyco/opencode/releases/tag/v1.19.0) | `--pure` e controles contra autoupdate/plugins/compartilhamento. Full mantém política ask/deny restrita até revisão de política Full própria; não declarar paridade Full. |

Documentação oficial: [Claude headless](https://code.claude.com/docs/en/headless), [OpenAI CLI](https://developers.openai.com/codex/cli/reference), [OpenCode CLI](https://opencode.ai/docs/cli/). `probe_request` apenas constrói um plano versionado; nenhum probe real foi rodado. Autorizar implementação não autoriza testes com contas, autenticação, rede ou quota.

## AT-092

`platform_contract.rs` oferece LocalParts/LocalCalendarBackend/CalendarAdapter e PrivateStorageBackend/PrivateStore: bounded read, nomes portáveis, capabilities obrigatórias e compare-and-replace com durabilidade explícita. Fixtures simulam calendário, CAS, limite, Unicode e nomes Windows. Capabilities são obrigações do implementador, não comprovação de ACL/handles.

Calendário Windows possui implementação source de note_stamp/clock_candidates usando FFI existente, com campos e conversões validados. Storage Windows ainda exige backend nativo seguro e integração; o contrato/Memory fixture não lê/grava disco Windows. Windows/macOS não foram compilados ou executados nativamente. AT-070 permanece aberta nesses critérios e na integração pessoal.

## Evidências históricas checkpoint9

367 testes workspace all-targets passed; fmt/check/build/clippy locked/offline passaram no Linux com toolchain já existente 1.99. Não comprova MSRV 1.88. `scripts/check_real_launcher_pty.py` exercita a entrada REAL somente com fake nativo, três protocolos, aprovação/cancel/resume, canários de ambiente e termios. Regressões settings/AT082/recovery passaram. Naquele checkpoint9 a prévia ainda não estava instalada; posteriormente instalada e supersedida por10/11. Fallback Python preservado.

Gates restantes: providers reais por versão/perfil/SO, autenticação explícita, política Full OpenCode, isolamento de configuração/segredos, backend Windows storage, matriz nativa/MSRV e terminais físicos. Não interpretar fixtures como encerramento AT-070/072/075/076.


## AT-095 — assinatura existente, candidato10 validado com fixtures

O caminho novo usa a autenticação que a CLI já possui, com HOME escolhido explicitamente. O HUD não abre auth.json, keychain, .credentials.json nem lê/copía tokens. Consulta de login é separada do envio de prompt, bounded e mediante confirmação, retornando somente assinatura/API/desconhecido. Modo API e indeterminado são recusados; nenhum login/configuração/billing é modificado.

Três conceitos distintos: custo estimado de tokens informado pela CLI; consumo da quota da assinatura; eventual cobrança extra da conta. A estimativa em USD não comprova cobrança e não garante teto financeiro. Claude pode passar a usage credits quando a quota acaba se estiver habilitado; a consulta auth status não informa isso. Verificação mínima do dono: Settings > Usage > Usage credits desabilitados, sem habilitar/comprar nada; confirmar estado atual antes de testar. Para Codex, confirmar assinatura existente e ausência de uso extra/créditos pagos para o teste. Não presumir uso ilimitado/gratuito.

Claude 2.1.296 usa --safe-mode --restricted e apenas Read, sem --bare, com MCP e caminhos secretos negados. A política gerenciada ainda pode executar hooks; exige revisão de ausência de comandos executáveis. Codex 0.160.1 tem contrato próprio, diferente0.162.1. --ignore-user-config e --ignore-rules não apagam configuração de projeto/sistema/gerenciada, e mcp_servers={} sofre merge: requer revisão explícita dessas camadas: rota nativa OpenAI/ChatGPT sem override de endpoint, autenticação, header, provedor ou cobrança, e sem hooks/MCP para o diretório escolhido. A flag model_provider=openai é defesa parcial; política gerenciada pode prevalecer. Mudar de diretório invalida a revisão de configuração. Full não é suportado no caminho de assinatura mínimo.

O worker precisa revalidar versão observada/pin/HOME e consultar novamente autenticação antes do prompt confirmado. Métodos/cobrança/política desconhecidos permanecem bloqueados. Declarações do dono no manifesto são attestations, não detecção independente de billing. Troca externa de conta/configuração entre consulta e uso é risco residual; não usar forced_login_method como probe porque pode encerrar login existente.

Fontes primárias: https://code.claude.com/docs/en/cli-reference ; https://support.claude.com/en/articles/12429409-manage-usage-credits-for-paid-claude-plans ; https://github.com/openai/codex/blob/rust-v0.160.1/codex-rs/exec/src/cli.rs ; https://github.com/openai/codex/blob/rust-v0.160.1/codex-rs/config/src/loader/mod.rs ; https://github.com/openai/codex/blob/rust-v0.160.1/codex-rs/config/src/merge.rs .


### Manifesto de assinatura (schema1)

Cada provider recebe um objeto subscription: mode=existing_subscription; home absoluto selecionado; extra_usage=unknown ou disabled_confirmed_by_user; managed_policy=unknown, reviewed_no_executable_hooks (Claude) ou reviewed_codex_subscription_configuration (Codex). Codex exige reviewed_cwd absoluto igual ao diretório da sessão, com cadeia/identidade pinada. Nenhum objeto declara login: esse estado vem da consulta nativa após confirmação. Ausência do objeto não habilita API; a TUI recusa prompts sem um caminho de assinatura revisado.

Fluxo: agent detect → revisão das consultas --version e auth status/login status → yes → método observado. Não envia prompt. Depois ask/PROMPT → revisão distinta do envio → yes. Uma consulta de login revoga prompt pendente; mudar de aba não transfere confirmação. O worker consulta novamente login antes do envio. API/indeterminado, billing desconhecido, configuração sem revisão, pasta alterada ou versão divergente bloqueiam envio.

Não preencher flags de confirmação por suposição. Na máquina local o login de assinatura foi observado por consultas readonly, mas billing e configuração efetiva continuam desconhecidos; nenhum prompt real faz parte deste aceite.

Validação final AT-095: 386 testes workspace all-targets, fmt/clippy sem avisos; PTY subscription/settings/recovery com fake. Revisão final sem novo bloqueante após corrigir configuração Codex, cadeia da cwd e confirmação exclusiva. Não prova cobrança/autenticação/configuração nativas; nenhum prompt real.

## AT-096 — diagnóstico da entrada padrão

O wrapper preview10 padrão não selecionava manifesto; /c, /x e /o chegavam a `Nenhum backend explícito selecionado`. A entrada alternativa selecionava Claude/Codex, mas billing/política permaneciam Unknown. Preview11 seleciona explicitamente esse mesmo manifesto local preparado ao abrir hud-rs, sem consulta de login nem processo de provider. Flags explícitas --fake-agent, --real-agent-manifest ou --agent-dir prevalecem sobre esse padrão. O binário direto sem manifesto continua sem backend.

`/agentes` mostra inventário e todos os bloqueios por provider; `monitor` mantém o monitor de metadados. A aba informa autenticação não verificada, cobrança extra desconhecida, revisão de configuração pendente e cwd exata. OpenCode não tem assinatura nativa suportada neste caminho. Detecção de login continua `agent detect` e confirmação separada; não preencher confirmações de cobrança/política por suposição. Essa correção não valida prompts reais.

## Smoke Claude opcional sem ferramentas

Um manifesto privado pode declarar explicitamente `subscription.smoke_no_tools = true` somente para Claude; o padrão é `false`. O smoke limita-se a contexto vazio, prompt com até 256 caracteres, sessão nova e um turno, sem persistência nem retomada. Os gates de billing e política continuam obrigatórios, assim como a aprovação separada do prompt. Um prompt real pode consumir quota da assinatura ou gerar uso extra; o estado de uso extra permanece Unknown até verificação pelo dono. Smoke Claude dedicado passou com um prompt/uma resposta; Codex real segue não validado.

A opção não integra isolamento de filesystem. O escopo separado validado com bubblewrap não é o launcher padrão do HUD; HOME/cwd e perfil readonly não provam confinamento de leitura. O smoke sem ferramentas não comprova política efetiva nem encerra aceite real do provider.

## Evidência local atual e limite do canário

Em 11/10/2026, um smoke Claude pelo core instalado passou em namespace filesystem sintético, contexto vazio e ferramentas desabilitadas: um prompt/uma resposta HUD-SMOKE-OK. Uso em tokens/estimativa USD não foi capturado; nenhum débito foi verificado. Esse escopo externo não é isolamento automático do launcher padrão.

Codex 0.160.1: canário sintético Landlock comprovou separação de processos, não cobertura do conjunto efetivo de ferramentas. A fonte pinada de view_image159–177 passa contexto sandbox às leituras; a hipótese anterior de leitura direta desprotegida não se confirmou. MCP/extensões/catálogo do modelo precisam de evidência própria. ToolPolicy permite teto interno vazio via embedding, sem interface exec/config comprovada. Nenhum prompt real Codex. Não presumir política corporativa nem converter canário genérico em aceite nativo.

[Codex — cobertura comprovada e pendente](codex-smoke-coverage.pt-BR.md)
