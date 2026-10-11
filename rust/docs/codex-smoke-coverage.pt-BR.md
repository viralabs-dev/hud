# Cobertura do smoke Codex 0.160.1

[English](codex-smoke-coverage.en.md) · [contrato launcher](real-launcher.pt-BR.md)

Estado em 2026-10-11: Claude dedicado passou com um prompt/uma resposta HUD-SMOKE-OK; uso/custo não capturados. Codex: zero prompts reais. Canários sintéticos Landlock e sandbox nativo passaram, sem credenciais reais, hooks ou rede. Não são prova de cobertura de toda sessão.

| Via | Fonte fixa | Evidência e limite |
|---|---|---|
| Shell/exec/stdin | spec_plan.rs:1079–1107 | shell_tool=false impede registro; perfil nativo negou leitura em subprocessos sintéticos |
| Imagem | view_image.rs:159–177 | metadata/read recebem sandbox; hipótese de leitura desprotegida não confirmada; handler não executado no canário |
| Filesystem helper | local_file_system.rs:95–108; fs_sandbox.rs:111–142 | encaminha perfil restrito ao helper; não confundir teste do comando sandbox com execução desse helper |
| Patch | apply_patch.rs:358–366 | valida com filesystem/sandbox; não executado; registro depende do catálogo |
| MCP/apps/extensões | spec_plan.rs:146–174; handlers/mcp.rs:196–258 | vias próprias não cobertas pelos canários; config/read confirmou recursos reduzidos e ausência de MCP/hooks/plugins configurados; catálogo do turno ainda não atestado |
| Ferramentas do modelo | spec_plan.rs:1181–1286 | catálogo pode adicionar utilitários; flags não provam catálogo vazio |

Não existem handlers read_file/list_dir/grep_files nessa árvore: não presumir ferramentas de outras versões. Fontes: [tool plan](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/spec_plan.rs), [imagem](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/handlers/view_image.rs), [filesystem](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/exec-server/src/local_file_system.rs), [helper](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/exec-server/src/fs_sandbox.rs), [patch](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/handlers/apply_patch.rs). Linhas correspondem ao texto HTTP preservado.

## Perfil sintético nativo validado

```sh
codex -c 'permissions={hud-smoke={extends=":read-only",filesystem={"/home/hud"="deny"}}}' \
  sandbox --permission-profile hud-smoke --cd /work -- /bin/cat CAMINHO_SINTETICO
```

HOME/config/auth reais ausentes, rede desabilitada. Leitor fora do perfil leu marcador inválido sintético; controle público permitido; auth.json, symlink, ancestral e caminhos proc/self/root e proc/1/root negados. Primeira sintaxe de chave pontuada foi recusada antes de executar; tabela inteira passou.

Esse comando debug com perfil explícito não carrega managed/auth/cloud por padrão. Usado apenas no ambiente vazio; **não ignorar política numa sessão real**. [Roteamento](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/cli/src/debug_sandbox.rs), [bootstrap](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/cli/src/debug_sandbox/cloud_config.rs).

## Próximo aceite técnico

O checkout integra `hud.codex.subscription.smoke-isolated.v1@0.160.1`. O manifesto privado aceita `smoke_isolated: true` somente para Codex, independente de `smoke_no_tools` Claude. Requer sessão nova, contexto vazio e prompt não vazio de até 256 caracteres; recusa resume, plano alterado e cwd dentro do HOME negado. Os gates de assinatura, cobrança e revisão de configuração continuam obrigatórios. A prévia instalada anterior permanece preservada; o contrato novo está no candidato do checkout.

O argv fixa `default_permissions="hud-smoke"`, um único deny sobre HOME herdado por `.codex`, aprovação never, login chatgpt/provider openai, configuração do usuário e rules ignoradas, execução ephemeral e web desabilitada. Também desativa 29 recursos de shell/imagem/apps/plugins/MCP alternativo/extensões/controle remoto. Isso reduz capacidades; não promete catálogo vazio. Duas regras deny sobrepostas causaram falha de montagem antes do controle público; a regra única corrigiu o problema. O canário final permitiu o controle público e negou credenciais sintéticas diretas, symlink, ancestral e aliases proc.

Inspeção nativa readonly em HOME/work sintéticos, com somente auth nativa montada readonly e sem copiar credenciais: `initialize`, `config/read` com cwd `/work` e `configRequirements/read`. Não enviou `thread/start` nem `turn/start`. Retornou perfil deny correto, os 29 recursos false, provider openai, zero MCP/hooks/plugins configurados e nenhum hook obrigatório na resposta de requisitos. Payloads completos e env/headers não foram persistidos; apenas metadados permitidos. Nenhum aviso de falha cloud foi observado; isso não é uma atestação de todo catálogo ou de ausência universal de política. App-server e exec carregam configuração por caminhos próprios; a resposta readonly não prova identidade de um futuro turno.

Fontes fixas adicionais: [configuração RPC](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/app-server/src/request_processors/config_processor.rs), [startup de plugins](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core-plugins/src/manager.rs), [estado remoto](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/app-server-transport/src/transport/remote_control/desired_state.rs). Startup de plugins é protegido por plugins_enabled; HOME sintético sem matrícula persistida impede ativação remota. `mcp list` não foi usado como prova de ferramentas do turno.

Validação local: 395 testes no workspace, fmt e Clippy; TUI com CLI/auth fictícias passou, com revisão explícita e terminal restaurado. Integração em `hud-agent-protocol/src/subscription.rs`, `hud-agent-process/src/subscription.rs` e `worker.rs`, `hud-tui/src/agent_manifest.rs` e `agent_app.rs`.

**Gate técnico restante:** atestar os runtimes MCP/extensões e catálogo efetivo do turno, além de exercitar os handlers de filesystem com o perfil. Canário de subprocesso e config/read não cumprem sozinhos esse aceite. Codex continua com zero prompts reais; não ampliar ferramentas/permissões para vencer o gate. Quota incluída/ausência de extra já confirmadas pelo dono; não é necessário perguntar novamente. Claude real já concluído não deve ser repetido. AT-072 permanece bloqueada pelo aceite restante e pela matriz de provedores/perfis/SO.

Existe teto interno ToolPolicy.allowed_tools=Some([]), fornecido antes da thread por ExtensionDataInit e capturado sem relaxamento posterior; não foi comprovada interface CLI exec/config para selecioná-lo. [Política](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/ext/extension-api/src/tool_policy.rs), [registro](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/registry.rs). Não modificar binário nativo nem declarar esse teto aplicado por flags. Sem alterações de segurança pessoal, novos logins ou cópia de credenciais.
