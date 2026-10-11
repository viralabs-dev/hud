# Explicit production launcher and subscription readiness

> **Link scope:** references to crates/scripts/source/Python are checkout-only. The local archive includes docs/licenses, not source/build tools; those source links are not present inside the archive.
[Safe preflight and conditional smoke](config-preflight.en.md)

[Português brasileiro](real-launcher.pt-BR.md) · [commands](commands-config.en.md) · [support](support.en.md)

0.10.0-preview.12 is locally installed under the primary hud command. Its default wrapper selects the prepared manifest/session directory; startup does not run version/auth probes, login or prompts. Direct core binaries have no backend unless `--real-agent-manifest PRIVATE_JSON --agent-dir PRIVATE_WORKSPACE` or mutually exclusive `--fake-agent` is provided. `/agentes` shows each provider and every remaining blocker; `monitor` remains metadata monitoring.

Manifest schema1 holds at most three distinct pinned providers,32KiB. Absolute private native ELF/Mach-O executable, trusted directory chain, SHA256 and file identity are revalidated before launch. Scripts/npm shims/symlinks/FIFOs/version mismatch fail. Declared version_output is an owner-provided pin, not independent version detection. Explicit settings may adjust supported fields; enabled executable must agree with manifest. HUD itself does not read auth/keychain/token files or mutate login/billing/configuration.

Subscription mode requires explicit HOME and separate auth, billing and managed-config gates. `agent detect` prepares bounded --version/auth-status requests → distinct review → `yes`; no model prompt. Observed API/unknown auth refuses prompts. The owner must actually review extra-usage/account settings before recording disabled_confirmed_by_user; HUD cannot infer this from login or token USD. Managed policy remains unknown until effective configuration is reviewed; Codex additionally pins reviewed_cwd identity. Changing folder invalidates that scope. Never populate confirmations by assumption.

Claude minimum subscription path uses safe/restricted Read policies and secret/MCP denial; effective managed hooks still require review. Codex user-ignore flags do not remove project/system/managed configuration, nor does an empty MCP table necessarily remove merged entries: native subscription routing, endpoint/auth/billing overrides and hooks/MCP need explicit review for chosen cwd. OpenCode native subscription is unsupported here. Full subscription profile is unsupported. See version-specific source contracts in [subscription.rs](../crates/hud-agent-protocol/src/subscription.rs) and [contract.rs](../crates/hud-agent-protocol/src/launcher.rs); allowlists are compatibility gates, not independently verified provider releases.

`ask PROMPT` prepares a REAL review; `yes` approves only that current request. Provider/profile/model/cwd/executable/hash/contract/env-key names and quota/transmission warning are visible; env values remain hidden. Worker rechecks observed version/auth/HOME and gates before prompt. A detection review revokes prompt approval. Context/provider/folder/profile changes revoke relevant pending state; cancel/drop kills process trees. Bound argv/env/stream/event limits apply.

Minimal environment strips API secrets; CLI itself can access user-readable configuration, filesystem and network. Read permissions are not OS isolation. Same-UID checks-to-spawn and external account/config changes between check/use remain residual risks. Token price estimates, subscription quota and account extra billing are distinct facts; no financial guarantee follows from a CLI estimate.

Installed wrapper evidence proves actual manifest/pin composition and arrival at subscription preflight gates. That entry check used fake executables for version/auth/prompts. Subsequent isolated native auth probes and the dedicated Claude smoke are recorded below. The functional startup correction does not prove real-provider operation. Complete turn-tool coverage remains pending: non-secret local settings can provide a partial audit, excluding auth/keychain/credential files; managed policy may override them and this task has not verified a safe complete effective-config dry-run. Never approve managed_policy by assumption.

Local evidence: installed receipt records 392 tests; checkout candidate passed 395 tests and a dedicated Codex fake PTY. One isolated Claude real smoke passed; Codex has zero real prompts. Local read-only native login observations do not close billing/policy gates. Physical terminal, real CLI/version/profile permissions, native OS storage/process behavior and billing remain pending. Portable policy/calendar/storage fixtures are obligations, not implemented native Windows storage.

## Optional Claude smoke without tools

A private manifest may explicitly set `subscription.smoke_no_tools = true` for Claude only; the default is `false`. This limits the smoke to empty context, a prompt of at most 256 characters, a fresh session and one turn, with no persistence or resume. Billing and policy gates still apply, as does the separate prompt approval. A real prompt may consume subscription quota or incur extra usage; extra-usage status remains Unknown until verified by the owner. Current isolated smoke evidence: Claude one successful prompt; Codex zero real prompts.

This option does not integrate filesystem isolation. The separate bubblewrap validation scope is not the default HUD launcher; HOME/cwd and a read-only profile do not establish read confinement. Do not treat the no-tools smoke as effective-policy or real-provider acceptance.

## Current local evidence and canary limits

On 2026-10-11, a dedicated Claude smoke through the installed core passed in a synthetic filesystem namespace with empty context and disabled tools: one prompt/one HUD-SMOKE-OK response. Tokens/USD estimate were not captured; no charge was verified. This external scope does not provide automatic isolation for the default launcher.

Codex 0.160.1: a synthetic Landlock canary demonstrated process separation, not coverage of the effective tool set. Pinned view_image159–177 passes sandbox context to reads; the earlier hypothesis of unprotected direct reading was not confirmed. MCP/extensions/model tool catalog need separate evidence. ToolPolicy supports an internal empty ceiling via embedding; no exec/config interface was proven. No real Codex prompt. Do not infer enterprise policy or turn a generic canary into native acceptance.

[Codex — cobertura comprovada e pendente](codex-smoke-coverage.en.md)
