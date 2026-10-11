# Preflight readonly e roteiro real condicionado

[English](config-preflight.en.md) · [gates de assinatura](real-launcher.pt-BR.md)

Esta página é um roteiro público genérico. Observações de configuração pessoal, contagens, nomes de plugins/MCP, preferências modelo/reasoning, caminhos HOME e linhas de arquivos privados ficam somente no Vault pessoal. Revisão readonly não atesta política efetiva nem fecha handshake/provider real. Sem execução de CLI/hook/MCP/plugin/modelo, login/config/billing alterados ou leitura/cópia de credenciais nesta etapa.

Fontes permitidas devem ser explícitas, limitadas, NOFOLLOW e redigidas por whitelist de fatos não secretos; arquivos mistos auth/config e conversas ficam excluídos. Presença de hooks/MCP/plugins pessoais não prova que o plano HUD os executa; ausência de arquivos locais não prova política gerenciada/cloud. Manter Unknown quando faltam fontes aplicáveis.

Claude restricted documentado desde2.1.248 limita file tools às working directories e carrega managed/--settings; safe-mode mantém hooks gerenciados. Docs rolling não provam toda inicialização/authstatus de uma release pinada. [CLI](https://code.claude.com/docs/en/cli-reference) · [managed](https://code.claude.com/docs/en/managed-settings).

Codex 0.160.1 exclui codex_home da descoberta project ancestor; user config ignorado não retorna por essa via. Exec carrega cloud separado; isso não fecha policy. Versão é despachada antes de Config após inicialização; login status carrega Config/auth e pode consultar remoto ou imprimir fragmentos API. Conservar só classificação, descartar raw streams; nenhuma consulta presume dump efetivo seguro. [Loader pinado](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/config/src/loader/mod.rs) · [exec](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/exec/src/lib.rs) · [login](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/cli/src/login.rs).

Checklist do dono, sem modificar conta: assinatura pessoal/modelo/quota incluída; extra usage/créditos não usados no teste; policy managed/cloud sem hooks/MCP/overrides rota/auth/billing para cwd exata; isolamento filesystem sem acesso Vault/repo/conversas/segredos de projeto. Codex read-only não confina leituras à cwd; instrução “não leia arquivos” não é controle de acesso. Billing/login/custo tokens são fatos distintos. [Claude usage credits](https://support.claude.com/en/articles/12429409-manage-usage-credits-for-paid-claude-plans) · [Codex plano](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan).

Roteiro condicionado: ambiente isolado já aprovado com CLIs pinadas/contexto/cwd/data sintéticos somente, autenticação nativa autorizada sem copiar/expor tokens. Falta de isolamento bloqueia. Revisar config efetiva ali; só depois preencher attestations provadas em manifesto dedicado. Nenhum setup/install está autorizado por esta página.

Core explícito com esse manifesto, perfil leitura e sem Vault/skills/contexto pessoal/feeds/hooks/MCP/plugins. Consulta versão/auth bounded somente quando rota segura comprovada, com revisão/yes distinta; API/Unknown para. Um único prompt por provider: “Responda exatamente HUD-SMOKE-OK, sem ferramentas e sem ler arquivos.” Confirmação separada; acompanhar stream/final, encerramento/timeout/cancel/quota; qualquer rota/billing/hook/MCP inesperado cancela sem retry. Registrar somente versão/hash/escopo/modelo/auth/policy/billing/isolamento/resultado sintético não secretos. Resume/cancel adicionais são casos posteriores; estimativa USD não é fatura/teto.

AT-098 encerrou diagnóstico/checklist local, sem encerrar AT-072. Unknown auth/billing/cloud/isolamento e assinatura OpenCode não suportada continuam bloqueios. Testes anteriores são fake/local, não evidência de modelo real.

## Smoke Claude opcional sem ferramentas

Um manifesto privado pode declarar explicitamente `subscription.smoke_no_tools = true` somente para Claude; o padrão é `false`. O smoke limita-se a contexto vazio, prompt com até 256 caracteres, sessão nova e um turno, sem persistência nem retomada. Os gates de billing e política continuam obrigatórios, assim como a aprovação separada do prompt. Um prompt real pode consumir quota da assinatura ou gerar uso extra; o estado de uso extra permanece Unknown até verificação pelo dono. Smoke Claude dedicado passou com um prompt/uma resposta; Codex real segue não validado.

A opção não integra isolamento de filesystem. O escopo separado validado com bubblewrap não é o launcher padrão do HUD; HOME/cwd e perfil readonly não provam confinamento de leitura. O smoke sem ferramentas não comprova política efetiva nem encerra aceite real do provider.

## Evidência local atual e limite do canário

Em 11/10/2026, um smoke Claude pelo core instalado passou em namespace filesystem sintético, contexto vazio e ferramentas desabilitadas: um prompt/uma resposta HUD-SMOKE-OK. Uso em tokens/estimativa USD não foi capturado; nenhum débito foi verificado. Esse escopo externo não é isolamento automático do launcher padrão.

Codex 0.160.1: canário sintético Landlock comprovou separação de processos, não cobertura do conjunto efetivo de ferramentas. A fonte pinada de view_image159–177 passa contexto sandbox às leituras; a hipótese anterior de leitura direta desprotegida não se confirmou. MCP/extensões/catálogo do modelo precisam de evidência própria. ToolPolicy permite teto interno vazio via embedding, sem interface exec/config comprovada. Nenhum prompt real Codex. Não presumir política corporativa nem converter canário genérico em aceite nativo.

[Codex — cobertura comprovada e pendente](codex-smoke-coverage.pt-BR.md)
