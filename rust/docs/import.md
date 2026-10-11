# Explicit Python layout bridge

> **Link scope:** references to crates/scripts/source/Python are checkout-only. The local archive includes docs/licenses, not source/build tools; those source links are not present inside the archive.

[Português brasileiro](import.pt-BR.md) · [reviewed config import](review-workflows.en.md)

[import_python_layout.py](../scripts/import_python_layout.py) is a one-shot Python3.11+ bridge using the existing Python layout validator, not a Rust runtime dependency. It reads only an explicit absolute source, never personal defaults, and follows no symlink components. Linux/POSIX tested. This bridge preserves built-in panel order but intentionally loses sizing; it is distinct from the newer TUI config/custom proposal workflows.

Fixture-only dry-run from the Rust workspace:

```sh
fixture_root=$(mktemp -d /tmp/hud-layout-doc.XXXXXX)
chmod 700 "$fixture_root"
printf 'nome="fixture"
[[coluna]]
paineis=["sistema","agenda"]
[[coluna]]
paineis=["pasta","saida"]
' > "$fixture_root/layout.toml"
chmod 600 "$fixture_root/layout.toml"
python3 scripts/import_python_layout.py --source "$fixture_root/layout.toml"
```

Dry-run prints JSON without creating state. Only `--target NEW_ABSOLUTE_DIR --apply --accept-geometry-loss` writes new Rust state. Existing targets/symlinks always fail. Source bytes are copied exactly to `python-layout.original.toml`, synced before `layout.state`; directories700/files600, temporary exclusive writes/rename/sync. Errors can leave a backup or complete published state: inspect rather than retry blindly, use a fresh target. Original Python file is never rewritten/restored automatically.

Mappings: sistema→system, pasta→vault, saida→output; supported built-ins retain ordering. Widths/heights/weights/metadata cannot be represented by this bridge format; explicit loss approval required. Custom painel/bloco definitions fail before target creation, not silently discarded. A valid panel ID is not a promise of feature parity.

Nine import fixtures cover schema/Unicode/errors/no-write/backup/private modes/symlinks/traversal/manual-source permissions and injected write/rename failures. No personal state is used. Same-UID concurrent directory relocation remains a residual limit.
