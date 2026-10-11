# HUD Rust — local target application

[Português brasileiro](README.pt-BR.md)

Rust runtime, locally installed **0.10.0-preview.12**. Primary command **`hud`** after verified cutover; `hud-rs` is a compatibility alias to the same wrapper. Python is not target architecture or a runtime requirement. Explicit historical fallback `hud-python` preserves its data/settings.

```sh
hud --version
hud
```

Wrapper uses retained private data and a prepared manifest; no automatic login/model prompt. Bounded background PATH discovery inventories executable candidates by metadata only; Found proves neither version/auth nor spawn authorization. Default native backend preparation/hash runs asynchronously; UI stays available and prompts blocked until preparation/readiness completes. Explicit settings/context may require their own startup validation.

Linux metrics, Unicode/Markdown, focus/drag/emphasis, private agenda/notes/layout, bounded read-only Vault, reviewed config/custom/doc/command proposals and feed trust are locally implemented. `/agentes` separates observed installation, unknown version/auth and billing/policy gates. `agent detect` needs separate approval; prompts need another. OpenCode native subscription and Full subscription are unsupported. Discovery reads no credentials/config or executes CLIs; remote filesystem calls can stall a worker, without forced kernel cancellation claims.

[Local guide](docs/local-preview.en.md) · [target architecture](docs/architecture.en.md) · [commands/config](docs/commands-config.en.md) · [workflows](docs/workflows-local.en.md) · [real gates](docs/real-launcher.en.md)

## Build and validate

From Rust workspace with existing toolchain/cache:

```sh
cargo fmt --check
cargo test --locked --offline --all-targets
cargo clippy --locked --offline --all-targets -- -D warnings
cargo build --locked --offline
./target/debug/hud-rs --version
```

MSRV1.88 declared, toolchain1.99.0 locally validated. Installed receipt records 392 tests and artifact-specific PTYs. Candidate workspace: 395 tests. One isolated Claude smoke passed; Codex has zero real prompts; physical/native OS/MSRV/billing/provider [gates](docs/support.en.md) remain. Local build proves neither public release nor full parity.

[Dependencies](docs/dependencies.md) · [local packaging](scripts/PACKAGING.en.md) · [historical comparison](docs/parity.en.md) · [optional legacy bridge](docs/import.md). Development/migration helpers may use Python without making it a runtime dependency. [Historical checkpoints](docs/historical-checkpoints.en.md) preserve evidence; old separate-hud-rs/Python-primary instructions do not govern current installation.

Crates/scripts/source/legacy references are checkout-only; archives omit source/toolchain. Public delivery and legacy installer/workflow migration require their own activity/validation.
