# Commands and explicit configuration

> **Link scope:** references to crates/scripts/source/Python are checkout-only. The local archive includes docs/licenses, not source/build tools; those source links are not present inside the archive.


[Português brasileiro](commands-config.pt-BR.md) · [CLI implementation](../crates/hud-tui/src/main.rs) · [command dispatcher](../crates/hud-tui/src/application.rs)

`hud --help` lists options. Each path option takes exactly one argument; duplicate/unknown options fail. Help/version/build-info return before opening the terminal. Except the installed wrapper's selected data/backend defaults, paths are explicit. No automatic Python configuration or credential import.

| Options | Behavior |
| --- | --- |
| `--data-dir`, `--state-dir` | Private agenda/append-only notes and Rust layout; parent directories must exist |
| `--vault-dir`, `--vault-default-dir` | Bounded read-only Vault; remembered folder uses selected private data directory |
| `--custom-file`, `--custom-root`, `--custom-selection-dir`, `--custom-state-dir` | TOML/JSON customization, reviewed proposal catalog, remembered selection and private geometry persistence |
| `--feed-trust-dir` | Existing private directory for exact-config SHA256 command-feed trust; saving custom never implies trust |
| `--command-file`, `--commands-output`, `--command-proposals` | Literal argv declarations in TOML/JSON, reviewed command proposals/save; no execution on load/save |
| `--doc-root`, `--doc-proposals` | Explicit document destination/proposals with separate create/overwrite review |
| `--import-config`, `--import-output`, `--reviewed-config` | Reviewed Python config export and explicit application; original configuration remains intact |
| `--agent-config`, `--agent-context`, `--agent-dir` | Explicit private provider settings/context and prepared session directory |
| `--real-agent-manifest` / `--fake-agent` | Mutually exclusive pinned production or compile-time verified synthetic backend |
| `--skills-dir`, `--claude-cache`, `--codex-rollout` | Bounded explicit skills/usage sources, no auth file reads |
| `--monitor-file`, `--monitor-dir`, `--monitor-root`, `--monitor-opencode-db`, `--monitor-process-root` | Explicit bounded metadata roots/DB/own-UID process names; no conversation/args/environment discovery |
| `--note-fixture-dir` | Task-checkbox writing confined to private /tmp fixture roots; real Vault is read-only |

| Input | Effect |
| --- | --- |
| `/ajuda`, `/sair`, `/limpar` | Help, exit, clear display only |
| `/c`, `/x`, `/o`; `/agentes` | Select tab and prepare request; inventory backend/auth/billing/configuration gates |
| `agent detect` → `yes` | Separate bounded version/auth-status checks, no login or model prompt |
| `ask TEXT` → `yes`; `cancel`, `/parar` | Review a prompt, then distinct approval; refuse/stop worker |
| `/agenda DATE [TIME] TEXT`, `/ok N`, `/rm N` | Private agenda; displayed indices start at 1, calendar is local |
| `/b TEXT`, `/vault`, `/notas [N]`, `/conflitos` | Read-only Vault queries; ordinary text in notes tab appends local notes |
| `/pasta PATH|vault`, `/pasta fixo|seguir AGENT`, `/perfil leitura|completo` | Reviewed folder/profile/session transitions; subscription Full remains unsupported |
| `/skills`, `/skill NAME REQUEST`, `/project` | Explicit skills/projects and complete request composition for approval |
| `/doc from-agent`, `/doc preview N`, `/doc save` or `/doc overwrite` | Parse proposals, review, distinct new-file/overwrite approval |
| `/custom`, `/cmd`, `/config` | Reviewed preview/save/apply/rollback workflows; see [details](review-workflows.en.md) |
| `feeds trust` → `feeds approve SHA256` | Review configured command feeds and approve exact byte digest; unknown/changed config denied |
| `run NAME|INDEX`, F1–F10, `yes`, `cancel` | Declared literal command; confirmation when marked; timeout/cancel/failure visible |

Config parsers are deliberately bounded subsets; unsupported/duplicate/unknown data fail closed. Custom feed intervals are 5..3600s and command timeouts 1..60s, including fractions; at most two workers, bounded output and stale snapshots retained on error. TOML/JSON settings/schema examples should be verified against the current parser, not adapted by silently dropping unknown keys.

`yes` approves only the current reviewed operation. Escape, relevant unrelated actions, backend/folder/profile changes revoke pending approval. Saving does not execute; ask does not spawn. The primary hud entry is Rust; background metadata discovery/default backend preparation executes no CLI and leaves subscription gates in place.
