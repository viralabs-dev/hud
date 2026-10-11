# Suporte e matriz de aceite

> **Escopo dos links:** referências a crates/scripts/código/Python são do checkout. O pacote local inclui docs/licenças, sem código/toolchain; esses links de fonte não existem dentro do archive.

[English](support.en.md)

| Escopo | Evidência e gate restante |
| --- | --- |
| Python 0.18.0 legado | Aplicação, targets de pacote e testes existentes; [guia Python](../../docs/legacy/python-guide.pt-BR.md) |
| Rust 0.10.0-preview.12 instalado | Recibo Linux x86_64: 392 testes, smoke/PTY fake/prontidão, rollback efetivo registrado e smoke Claude dedicado: um prompt/uma resposta; Codex sem prompt real; backend preparado sem execução na abertura |
| Checkout 0.10.0-preview.12 | Correção wrapper/prontidão instalada e validada localmente; gates reais continuam abertos |
| Rust Linux | Build/unit/falhas/PTY locais; não equivalem a matriz distribuição/glibc ou terminal físico |
| macOS / Windows / ARM | Parsers, source adapters e fixtures política/calendário; faltam build/processos/storage/terminal nativos |
| MSRV Rust 1.88 | Declarado; toolchain local 1.99.0, MSRV não exercitado |
| Caminho FAKE | Protocolos/workers/aprovação/timeout/cancel sintéticos; não provam conta/modelo/cobrança reais |
| Caminho REAL | Manifesto/pin/auth/gates implementados; prompts e permissões reais por versão/perfil ainda sem aceite |
| Cobrança da assinatura | Declaração do dono e detecção login são fatos distintos; billing/config desconhecidos bloqueiam; estimativa tokens não é fatura |
| Terminal físico | Arraste/drop/cancel, roda/seleção, foco, Unicode/fontes e resize exigem resultado com terminal/SO/shell/dimensões identificados |

Checkpoint local não comprova paridade completa, release pública, aceite nativo ou substituição padrão. WSL conta como Linux; emulação x64 não valida Windows ARM nativo. Artefato e wrapper ativo são distintos: informar caminhos/versão/recibo juntos.
