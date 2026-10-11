# Python reference inventory and target architecture

[Português brasileiro](python-reference-inventory.pt-BR.md)

After verified cutover, runtime/primary hud are Rust; hud-python is explicit historical fallback. Literal case-insensitive Python count excludes this inventory to avoid counting itself. Before: 42 documents and 177 matches. After includes preserved legacy copies, so total is not a measure of active-guide cleanup.

| Category | Before files | Before Python matches | After files | After Python matches |
| --- | --- | --- | --- | --- |
| active | 29 | 88 | 34 | 55 |
| comparison-contract | 2 | 12 | 2 | 13 |
| development-migration-tool | 4 | 29 | 4 | 29 |
| historical | 6 | 47 | 6 | 52 |
| legacy | 1 | 1 | 8 | 78 |

Remaining references are classified: baseline/comparison, history/ADR/receipts, fallback/rollback, explicit import and development tools. Optional helpers use an interpreter at development/migration time, never the shipped runtime. Legacy workflows/installers/tests/source are preserved code and separate migration gates; this documentation audit does not modify them.

| Path | Classification | Python matches after |
| --- | --- | --- |
| `README.md` | active | 5 |
| `README.pt-BR.md` | active | 5 |
| `docs/actions.en.md` | active | 1 |
| `docs/actions.md` | active | 1 |
| `docs/legacy/actions.pt-BR.md` | legacy | 7 |
| `docs/legacy/privacidade-agentes.pt-BR.md` | legacy | 7 |
| `docs/legacy/python-guide.en.md` | legacy | 25 |
| `docs/legacy/python-guide.pt-BR.md` | legacy | 29 |
| `docs/legacy/roteiro-cosmic.pt-BR.md` | legacy | 2 |
| `docs/legacy/roteiro-windows.pt-BR.md` | legacy | 2 |
| `docs/legacy/windows-macos.pt-BR.md` | legacy | 5 |
| `docs/privacidade-agentes.en.md` | active | 1 |
| `docs/privacidade-agentes.md` | active | 1 |
| `docs/roteiro-cosmic.md` | active | 1 |
| `docs/roteiro-windows.md` | active | 1 |
| `docs/telas/README-telas.md` | legacy | 1 |
| `docs/windows-macos.md` | active | 3 |
| `rust/README.md` | active | 4 |
| `rust/README.pt-BR.md` | active | 4 |
| `rust/docs/architecture.en.md` | active | 1 |
| `rust/docs/architecture.pt-BR.md` | active | 1 |
| `rust/docs/at082-local-and-gates.pt-BR.md` | historical | 7 |
| `rust/docs/checkpoint-local.pt-BR.md` | historical | 16 |
| `rust/docs/commands-config.en.md` | active | 3 |
| `rust/docs/commands-config.pt-BR.md` | active | 3 |
| `rust/docs/historical-checkpoints.en.md` | historical | 11 |
| `rust/docs/historical-checkpoints.pt-BR.md` | historical | 12 |
| `rust/docs/import.md` | development-migration-tool | 10 |
| `rust/docs/import.pt-BR.md` | development-migration-tool | 11 |
| `rust/docs/local-preview.en.md` | active | 1 |
| `rust/docs/local-preview.pt-BR.md` | active | 2 |
| `rust/docs/monitor-sqlite.pt-BR.md` | active | 1 |
| `rust/docs/parity.en.md` | comparison-contract | 9 |
| `rust/docs/parity.pt-BR.md` | comparison-contract | 4 |
| `rust/docs/preview2.pt-BR.md` | historical | 6 |
| `rust/docs/real-launcher.en.md` | active | 1 |
| `rust/docs/real-launcher.pt-BR.md` | active | 2 |
| `rust/docs/review-workflows.pt-BR.md` | active | 2 |
| `rust/docs/support.en.md` | active | 5 |
| `rust/docs/support.pt-BR.md` | active | 4 |
| `rust/docs/workflows-local.en.md` | active | 1 |
| `rust/docs/workflows-local.pt-BR.md` | active | 1 |
| `rust/scripts/PACKAGING.en.md` | development-migration-tool | 4 |
| `rust/scripts/PACKAGING.pt-BR.md` | development-migration-tool | 4 |

Public preflight sanitized in EN/PT: personal candidate/file observations, hook/plugin counts, MCP/plugin names, model/reasoning preferences, private HOME/cwd, config line references and report path removed. Private evidence stays in the Vault; no public values/credentials copied.
