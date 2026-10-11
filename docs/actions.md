# Entrega e workflows do HUD Rust

[English](actions.en.md)

O alvo de entrega é Rust. Validação atual é build Cargo locked/offline, tests/clippy/fmt, pacote Linux local com notices/checksums e rollback verificado. Nenhuma publicação/Actions é executada nesta etapa. Workflows existentes com setup-python pertencem à distribuição histórica, não comprovam pipeline Rust.

[Empacotamento Rust](../rust/scripts/PACKAGING.pt-BR.md) · [suporte/gates](../rust/docs/support.pt-BR.md). Pipeline Rust público exige decisão explícita, revisão de actions pinadas/permissões/credenciais, matriz nativa e aceite do artefato. Fixar SHA é propriedade necessária da futura entrega, não afirmação de que o workflow Rust já foi migrado.

A [tabela histórica de actions](legacy/actions.pt-BR.md) permanece como registro e contrato dos testes legados; não usar seus comandos para publicar o alvo atual.
