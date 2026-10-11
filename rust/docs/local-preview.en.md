# Local Rust install: open, inspect and roll back

[Português brasileiro](local-preview.pt-BR.md)

Primary entry `hud`, compatibility alias `hud-rs`, both select **0.10.0-preview.12** at `$HOME/.local/opt/hud-rs/0.10.0-preview.12/abrir-preview.sh`. Receipt `$HOME/.local/opt/hud-rs/0.10.0-preview.12/installation.json` confirms hashes/tests/cutover/rollback; Cargo.toml alone never proves installation.

```sh
readlink -f "$HOME/.local/bin/hud"
hud --version
hud
```

Open terminal manually in your chosen workspace. Wrapper retains `.local/state/hud-rs-preview5/data` unless explicit and the previously prepared manifest/cwd. The artifact core remains named hud-rs; primary user command is hud. Background PATH metadata discovery/pin preparation runs no provider/login/model prompt; Unknown gates and unsupported OpenCode subscription stay visible. `agent detect` has approval separate from a prompt.

Legacy source/config/data is not updated/imported: `hud-python --version` checks the preserved fallback. Old public installers are legacy, not Rust update mechanisms. Write tests use private temporary roots; direct core avoids default manifest selection:

```sh
preview_root=$(mktemp -d /tmp/hud-doc-test.XXXXXX)
chmod 700 "$preview_root"
mkdir "$preview_root/data" "$preview_root/layout"
chmod 700 "$preview_root/data" "$preview_root/layout"
"$HOME/.local/opt/hud-rs/0.10.0-preview.12/hud-rs" --data-dir "$preview_root/data" --state-dir "$preview_root/layout"
```

Example selects no Vault/real provider; PATH observation executes nothing. Physical checklist: title drag/target/cancel, focus/emphasis, Unicode/resize, fixture agenda/notes restart. PTY is not physical acceptance. Inspect retained fixtures before cleanup.

Cutover rollback receipt: **$HOME/.local/state/hud-rust-cutover-preview12-active**, before/after expectations, link/data digests. Only captured links change unless explicit data restoration requested. Inspect receipt, then invoke helper with one positional BACKUP; compare checks refuse concurrent edits. Do not reinstall legacy over Rust. Prior preview and historical fallback have distinct scopes; selecting a launcher does not restore data. This guide executes no rollback.
