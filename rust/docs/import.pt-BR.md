# Bridge explícito de layout Python

> **Escopo dos links:** referências a crates/scripts/código/Python são do checkout. O pacote local inclui docs/licenças, sem código/toolchain; esses links de fonte não existem dentro do archive.

[English](import.md) · [importação config revisada](review-workflows.pt-BR.md)

[import_python_layout.py](../scripts/import_python_layout.py) usa Python3.11+ e validador Python existente uma única vez, sem dependência Python no runtime Rust. Lê fonte absoluta explícita, sem defaults pessoais/links. Testado Linux/POSIX. Conserva ordem de painéis built-in com perda intencional de dimensões; distinto dos workflows novos de config/custom na TUI.

Dry-run só com fixture, na pasta Rust:

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

Imprime JSON sem criar state. Só `--target NOVO_DIR_ABSOLUTO --apply --accept-geometry-loss` grava. Destino existente/link sempre falha. Backup `python-layout.original.toml` preserva bytes e é sincronizado antes de layout.state; pastas700/arquivos600/temp exclusivo/rename/sync. Falha pode deixar backup ou state completo: inspecionar antes de retry, escolher destino novo. Original Python não é reescrito/restaurado automaticamente.

Aliases sistema→system, pasta→vault, saida→output; built-ins mantêm ordem. Dimensões/pesos/metadados não cabem neste bridge: aceite explícito de perda. Definições painel/bloco custom falham antes de criar destino, sem descarte silencioso. ID válido não comprova paridade.

Nove fixtures cobrem schema/Unicode/falhas/preview sem escrita/backup/modos/links/traversal/permissões da fonte e erros write/rename injetados. Nenhum dado pessoal. Relocação concorrente pelo mesmo UID permanece limite residual.
