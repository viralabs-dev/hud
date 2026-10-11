# HUD — dashboard de terminal Rust

[English](README.md)

A arquitetura-alvo e aplicação local atual do HUD são Rust. O comando principal é **`hud`**, selecionando **0.10.0-preview.12** após cutover local verificado. Métricas, agenda/notas, consultas Vault, customização e operações de agente revisadas compartilham o terminal. Instalação local Linux não equivale a release pública nem aceite multiplataforma completo.

```sh
hud --version
hud
```

Abra manualmente o terminal na área de trabalho escolhida. Abertura seleciona apenas paths explícitos de backend/dados registrados no launcher, sem login/prompt automático. `/agentes` explica gates por provider; auth/billing/política efetiva Unknown permanece bloqueando. [Abrir, conferir e voltar](rust/docs/local-preview.pt-BR.md).

## Arquitetura e uso

Runtime Rust/Ratatui/Crossterm, com crates Cargo de domínio, dados, recursos, métricas, custom, stores e adapters processos/protocolos. [Arquitetura-alvo](rust/docs/architecture.pt-BR.md) · [comandos/config](rust/docs/commands-config.pt-BR.md) · [workflows](rust/docs/workflows-local.pt-BR.md).

Texto normal edita notas; `/ajuda` lista comandos; Alt+1–4 escolhe notas/agentes; Alt+setas muda foco; Alt+Z enfatiza; Esc cancela revisão e Ctrl+C sai. Paths/settings são explícitos; escrita e execução exigem seus fluxos distintos. Salvar proposta não executa comando/feed.

## Instalação e verificação

Instalação verificada **$HOME/.local/opt/hud-rs/0.10.0-preview.12**, launcher principal **$HOME/.local/bin/hud**, recibo installation.json. Paths locais existentes e manifesto pinado pertencem ao recibo/wrapper. hud-rs permanece alias de compatibilidade para o mesmo wrapper.

Build/checks/dependências em [workspace Rust](rust/README.pt-BR.md); [pacote local](rust/scripts/PACKAGING.pt-BR.md) não publica nem troca launcher automaticamente. install.sh/install.ps1 da raiz são distribuição histórica, não atualização Rust. Sem instalador/release Rust público ou workflow remoto alegado.

Recibo instalado registra 392 testes e PTYs sintéticos. Terminal físico, macOS/Windows/ARM nativos, MSRV e providers/billing/isolamento reais permanecem na [matriz](rust/docs/support.pt-BR.md). Prompt real exige prova separada de auth/política/conta/isolamento; estimativa/quota não é fatura.

## Legado e comparação

Python0.18.0 preservado como **`hud-python`**, fallback/rollback explícito, sem ser runtime-alvo. [Guia EN histórico](docs/legacy/python-guide.en.md) · [manual PT original](docs/legacy/python-guide.pt-BR.md) · [inventário de referências](docs/python-reference-inventory.pt-BR.md). Fonte/testes/capturas/bridges históricos ficam separados da arquitetura ativa. Documentação não sobrescreve settings/credenciais/dados.

[Licença MIT](LICENSE)
