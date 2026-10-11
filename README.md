# HUD — Rust terminal dashboard

[Português brasileiro](README.pt-BR.md)

HUD's target architecture and current local application are Rust. The primary user command is **`hud`**, selecting **0.10.0-preview.12** after the verified local cutover. System metrics, agenda/notes, Vault queries, customization and explicitly reviewed agent operations share one terminal interface. This local Linux installation is not a public release or proof of complete cross-platform acceptance.

```sh
hud --version
hud
```

Open the terminal manually in your chosen workspace. Startup selects only the prepared explicit backend/data paths recorded by the launcher; no provider login/model prompt runs automatically. `/agentes` explains every provider gate; unknown authentication/billing/effective policy remains blocked. [Open, inspect and rollback](rust/docs/local-preview.en.md).

## Architecture and usage

The runtime is Rust/Ratatui/Crossterm with Cargo workspace boundaries for domain, data, resources, metrics, customization, filesystem stores and process/protocol adapters. [Target architecture](rust/docs/architecture.en.md) · [commands/configuration](rust/docs/commands-config.en.md) · [workflows](rust/docs/workflows-local.en.md).

Normal text edits notes; `/ajuda` lists commands; Alt+1–4 select notes/agents; Alt+arrows focus; Alt+Z emphasizes; Escape cancels review and Ctrl+C exits. Paths/configuration are explicit and writes/execution require their distinct review/approval flows. Saving proposals never launches a command/feed.

## Installation and verification

The current verified user install is **$HOME/.local/opt/hud-rs/0.10.0-preview.12**, with primary launcher **$HOME/.local/bin/hud** and receipt `installation.json`. Existing local data paths and a pinned backend manifest are described by that receipt/wrapper. hud-rs remains a compatibility alias to the same wrapper.

Build/check instructions and dependency inventory are in [Rust workspace](rust/README.md); [local packaging](rust/scripts/PACKAGING.en.md) does not publish or replace a launcher automatically. The old root install.sh/install.ps1 belong to the historical distribution and are not a Rust update mechanism. No public Rust installer/release or remote workflow is claimed.

392 tests in the installed receipt and synthetic PTYs are local evidence. Physical terminals, native macOS/Windows/ARM, Rust MSRV and real provider/billing/filesystem-isolation gates remain in [support](rust/docs/support.en.md). Real prompts require separately reviewed auth/policy/account/isolation evidence; quota/token estimates are not invoices.

## Legacy and comparison references

Python0.18.0 remains preserved as **`hud-python`**, an explicit rollback/fallback rather than the target runtime. [Historical English guide](docs/legacy/python-guide.en.md) · [original Portuguese manual](docs/legacy/python-guide.pt-BR.md) · [reference inventory](docs/python-reference-inventory.en.md). Historical source, tests, screenshots and migration bridges remain distinguishable from active architecture. No original settings/credentials/data are overwritten by documentation.

[MIT license](LICENSE)
