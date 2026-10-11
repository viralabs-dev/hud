# HUD Rust — aplicação-alvo local

[English](README.md)

Runtime Rust, instalado localmente em **0.10.0-preview.12**. Comando principal **`hud`** após cutover verificado; `hud-rs` é alias de compatibilidade para o mesmo wrapper. Python não é a arquitetura-alvo nem requisito do runtime. Fallback histórico explícito `hud-python`, sem reescrever seus dados/settings.

```sh
hud --version
hud
```

Wrapper usa dados privados preservados e manifesto preparado, sem login/prompt automático. Descoberta PATH roda em worker limitado apenas para inventariar candidatos por metadados; Found não prova versão/login nem habilita spawn. Preparação/hash de backend nativo ocorre assíncrona no caminho padrão; UI permanece disponível e prompts bloqueados enquanto preparação/gates faltam. Config/contexto explícitos podem exigir validação inicial própria.

Métricas Linux, Unicode/Markdown, foco/arraste/ênfase, agenda/notas/layout privados, Vault limitado somente leitura, proposals config/custom/docs/comandos revisadas e feed trust estão implementados localmente. `/agentes` separa instalação observada, versão/auth Unknown e gatesbilling/política. `agent detect` precisa confirmação própria; prompt precisa outra. Assinatura OpenCode e Full de assinatura não suportados. Descoberta não lê credenciais/config nem executa CLIs; filesystem remoto pode bloquear worker, sem prometer cancelamento kernel.

[Guia local](docs/local-preview.pt-BR.md) · [arquitetura-alvo](docs/architecture.pt-BR.md) · [comandos/config](docs/commands-config.pt-BR.md) · [workflows](docs/workflows-local.pt-BR.md) · [gates reais](docs/real-launcher.pt-BR.md)

## Build e validação

Na pasta Rust, com toolchain/cache existentes:

```sh
cargo fmt --check
cargo test --locked --offline --all-targets
cargo clippy --locked --offline --all-targets -- -D warnings
cargo build --locked --offline
./target/debug/hud-rs --version
```

MSRV1.88 declarado, toolchain1.99.0 validado. Recibo instalado: 392 testes e PTYs específicos do artefato. Candidato checkout: 395 testes. Um smoke isolado Claude passou; Codex tem zero prompts reais; física/SOs nativos/MSRV/billing/providers seguem [gates](docs/support.pt-BR.md). Build local não comprova release pública ou paridade integral.

[Dependências](docs/dependencies.pt-BR.md) · [pacote local](scripts/PACKAGING.pt-BR.md) · [comparação histórica](docs/parity.pt-BR.md) · [bridge legado opcional](docs/import.pt-BR.md). Scripts de desenvolvimento/migração podem usar Python sem torná-lo dependência do runtime. Checkpoints são [históricos](docs/historical-checkpoints.pt-BR.md); orientações antigas de hud-rs separado/Python padrão não governam a instalação atual.

Referências de crates/scripts/fonte/legado são exclusivas do checkout; archive não inclui código/toolchain. Entrega pública e mudanças dos installers/workflows legados exigem atividade/validação própria.
