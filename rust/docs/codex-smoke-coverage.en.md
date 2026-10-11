# Codex 0.160.1 smoke coverage

[Português](codex-smoke-coverage.pt-BR.md) · [launcher contract](real-launcher.en.md)

As of 2026-10-11, a dedicated Claude smoke passed with one prompt/one HUD-SMOKE-OK response; tokens/cost were not captured. Codex has zero real prompts. Synthetic Landlock and native sandbox canaries passed without real credentials, hooks or network; neither proves coverage of the entire session.

| Route | Pinned source | Evidence and limit |
|---|---|---|
| Shell/exec/stdin | spec_plan.rs:1079–1107 | shell_tool=false prevents registration; native profile denied synthetic subprocess reads |
| Image | view_image.rs:159–177 | metadata/read receive sandbox context; unprotected reading was an unconfirmed hypothesis; handler not executed |
| Filesystem helper | local_file_system.rs:95–108; fs_sandbox.rs:111–142 | restrictive context reaches helper; sandbox command test is not this helper execution |
| Patch | apply_patch.rs:358–366 | filesystem/sandbox checks; not executed; registration depends on model catalog |
| MCP/apps/extensions | spec_plan.rs:146–174; handlers/mcp.rs:196–258 | separate routes outside the canary; config/read confirmed reduced features and no configured MCP/hooks/plugins; turn catalog remains unattested |
| Model tools | spec_plan.rs:1181–1286 | catalog may add utilities; flags do not prove an empty catalog |

The pinned tree has no read_file/list_dir/grep_files handlers. Sources: [tool plan](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/spec_plan.rs), [image](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/handlers/view_image.rs), [filesystem](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/exec-server/src/local_file_system.rs), [helper](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/exec-server/src/fs_sandbox.rs), [patch](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/handlers/apply_patch.rs). Lines refer to preserved HTTP source.

## Validated synthetic native profile

```sh
codex -c 'permissions={hud-smoke={extends=":read-only",filesystem={"/home/hud"="deny"}}}' \
  sandbox --permission-profile hud-smoke --cd /work -- /bin/cat SYNTHETIC_PATH
```

Real HOME/config/auth were absent and network disabled. Reader outside the profile read an invalid synthetic marker; public control was readable; auth.json, symlink, ancestor and proc/self/root and proc/1/root paths were denied. An initial dotted-key form failed parsing before command execution; the complete table passed.

Explicit-profile debug defaults omit managed/auth/cloud bootstrap. This was used only in an empty environment; **do not ignore policy in a real session**. [Routing](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/cli/src/debug_sandbox.rs), [bootstrap](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/cli/src/debug_sandbox/cloud_config.rs).

## Remaining technical acceptance

The checkout integrates `hud.codex.subscription.smoke-isolated.v1@0.160.1`. Private manifests accept boolean `smoke_isolated: true` only for Codex, separately from Claude smoke_no_tools. It requires a fresh session, empty context and a nonempty prompt of at most 256 characters; resume, mutated plans and cwd inside denied HOME are rejected. Existing subscription, billing and effective-configuration review gates remain mandatory. The previously installed preview is preserved; the new contract is in the checkout candidate.

Fixed argv selects `default_permissions="hud-smoke"`, a single HOME deny inherited by `.codex`, never approvals, chatgpt login/openai provider, ignored user configuration/rules, ephemeral execution and disabled web search. Twenty-nine shell/image/apps/plugins/alternate-MCP/extension/remote-control features are disabled. This reduces capabilities without promising an empty catalog. Overlapping HOME/.codex deny rules caused helper mount failure before public control execution; one parent deny corrected it. Final canaries permitted public control and denied direct credential markers, symlink, ancestor and proc aliases.

A native readonly inspection in synthetic HOME/work mounted only native auth readonly without credential copying. RPCs were initialize, config/read with `/work` cwd and configRequirements/read; no thread/start or turn/start. It returned the expected deny profile, all 29 features false, openai provider, no configured MCP/hooks/plugins and no required hooks in the requirements response. Only allowlisted metadata was saved, without raw payloads/env/headers. No cloud-load warning was observed; this does not attest the entire tool catalog or universal policy absence. App-server and exec have separate loading paths; a readonly response cannot prove a future turn is identical.

Additional pinned sources: [configuration RPC](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/app-server/src/request_processors/config_processor.rs), [plugin startup](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core-plugins/src/manager.rs), [remote state](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/app-server-transport/src/transport/remote_control/desired_state.rs). Plugin startup is guarded by plugins_enabled; synthetic HOME with no persisted enrollment prevents remote activation. mcp list was not treated as turn-tool evidence.

Local validation: 395 workspace tests, fmt and Clippy passed; synthetic CLI/auth PTY passed with explicit review and restored terminal. Integration spans protocol subscription.rs, process subscription.rs/worker.rs and TUI agent_manifest.rs/agent_app.rs.

**Remaining technical gate:** attest the actual turn catalog and MCP/extension runtimes, and exercise filesystem handlers with the profile. A subprocess canary and config/read alone do not meet this acceptance. Codex remains at zero real prompts; do not expand permissions to bypass the gate. Included quota/no extra billing were already confirmed; do not ask again. Do not repeat the successful Claude real smoke. AT-072 remains blocked on this gate and the provider/profile/OS matrix.

Internal ToolPolicy.allowed_tools=Some([]) is supplied through ExtensionDataInit before the thread and captured without later relaxation. No exec/config CLI interface was proven. [Policy](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/ext/extension-api/src/tool_policy.rs), [registry](https://raw.githubusercontent.com/openai/codex/rust-v0.160.1/codex-rs/core/src/tools/registry.rs). Do not modify the native binary or claim flags apply that ceiling. No personal security change, new login or credential copy occurred.
