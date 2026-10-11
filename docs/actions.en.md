# HUD Rust delivery and workflows

[Português brasileiro](actions.md)

Rust is the delivery target. Current validation is locked/offline Cargo build/tests/clippy/fmt, local Linux archive with notices/checksums and verified rollback. No publication/Actions runs here. Existing setup-python workflows belong to historical distribution, not a validated Rust pipeline.

[Packaging](../rust/scripts/PACKAGING.en.md) · [support/gates](../rust/docs/support.en.md). A public Rust pipeline needs an explicit decision, pinned-action/permission/credential review, native matrix and artifact acceptance. The [historical action inventory](legacy/actions.pt-BR.md) is preserved for legacy test contracts, not current release instructions.
