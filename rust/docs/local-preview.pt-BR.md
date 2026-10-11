# Instalação Rust local: abrir, conferir e voltar

[English](local-preview.en.md)

Entrada principal `hud`, alias compatível `hud-rs`, ambos selecionando **0.10.0-preview.12** em `$HOME/.local/opt/hud-rs/0.10.0-preview.12/abrir-preview.sh`. Recibo `$HOME/.local/opt/hud-rs/0.10.0-preview.12/installation.json` confirma hashes/testes/cutover/rollback; não deduzir instalação pelo Cargo.toml.

```sh
readlink -f "$HOME/.local/bin/hud"
hud --version
hud
```

Abrir manualmente terminal na área de trabalho escolhida. Wrapper conserva data-dir `.local/state/hud-rs-preview5/data` salvo explícito, e manifesto/cwd preparados anteriores. Binário core continua chamado hud-rs no artefato; comando principal do usuário é hud. Descoberta PATH metadata/prepare pin em workers não inicia provider/login/prompt; gates Unknown e OpenCode assinatura sem suporte permanecem visíveis. `agent detect` exige confirmação distinta do prompt.

Python legado não é atualizado/importado: `hud-python --version` confere fallback preservado. Installers públicos antigos são legado, não mecanismo Rust. Qualquer teste que escreve usa diretórios temporários privados; para não selecionar manifesto padrão do wrapper, usar core direto:

```sh
preview_root=$(mktemp -d /tmp/hud-doc-test.XXXXXX)
chmod 700 "$preview_root"
mkdir "$preview_root/data" "$preview_root/layout"
chmod 700 "$preview_root/data" "$preview_root/layout"
"$HOME/.local/opt/hud-rs/0.10.0-preview.12/hud-rs" --data-dir "$preview_root/data" --state-dir "$preview_root/layout"
```

Nenhum Vault/provider real selecionado nesse exemplo; PATH metadata não executa. Conferir fisicamente arraste/alvo/cancel, foco/ênfase, Unicode/resize e notas/agenda restart. PTY não substitui aceite físico. Preserve/inspecione fixture antes de remover.

Rollback do cutover: recibo **$HOME/.local/state/hud-rust-cutover-preview12-active**, expectativas before/after, links/data digests; troca só links capturados salvo restauração de dados opt-in. Inspecione recibo e use helper com único positional BACKUP; CAS recusa edição concorrente. Não reinstalar legado por cima. A preview anterior e fallback histórico têm escopos distintos; retornar a launcher não implica restaurar dados. Este guia não executa rollback.
