# Empacotamento local Linux e rollback

[English](PACKAGING.en.md) · [prévia instalada](../docs/local-preview.pt-BR.md)

`package_local.py` empacota executável Linux ELF64 x86_64 existente: não compila, instala nem publica. Exige caminho de arquivo novo; inclui executável, licença MIT, README/docs, Cargo.lock, licenças/notices integrais de dependências ativas normal/build, inventário e SHA256SUMS. Fake opcional deve corresponder ao pin SHA do core; sem providers reais/Python/config pessoal. SPDX não é auditoria jurídica.

Consultar argumentos sem empacotar:

```sh
python3 scripts/package_local.py --help
```

Se empacotamento estiver autorizado, na pasta Rust, com binário testado existente e output novo:

```sh
python3 scripts/package_local.py --repo "$PWD" --binary "$PWD/target/debug/hud-rs" --output /tmp/hud-rs-reviewed-new.tar.gz --cargo "$HOME/.cargo/bin/cargo"
```

Usa Cargo metadata/tree locked/offline, verifica ELF/alvo e smoke fora do checkout com HOME isolado: help/version/opções/caminho/state corrompido. Recusa arquivo existente. Smoke não valida gesto físico/conta/billing/plataforma nativa. Registrar distribuição/ldd; build glibc dinâmico não é release estática portátil.

Instalar é operação separada no diretório do usuário, com hashes archive/binário, alvo symlink, versão, escopo dados, ensaio rollback e testes em installation.json. `local_cutover.py` captura digests, verifica antes da troca, persiste expectativa rollback antes de publicar e recusa edições concorrentes. Restaurar dados é opt-in; trocar launcher não restaura estado. Não reutilizar recibo antigo para versão nova.

Recibo atual 0.10.0-preview.12: 391 testes workspace, rollback efetivo registrado pelo cutover, PTYs core/prontidão padrão/fixtures, zero chamadas de modelo por assinatura. [Matriz de suporte](../docs/support.pt-BR.md) mantém gates nativos/MSRV/provider/físicos. Pacote local não autoriza release/Actions nem efetuar nova troca do launcher principal.

Links de crates/scripts/fonte referem-se ao checkout; pacote não inclui código/toolchain. Notices incluídos e recibo validam o artefato.

Scripts de empacotamento usam interpretador Python em desenvolvimento. Runtime/core distribuído e comando principal hud são Rust; não dependem da aplicação legada.
