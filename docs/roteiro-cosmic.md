# Roteiro físico HUD Rust no COSMIC Terminal

[English](roteiro-cosmic.en.md)

Este roteiro é para o comando Rust `hud`, 0.10.0-preview.12. Abra manualmente terminal na área de trabalho escolhida; não executar o condutor legado scripts/roteiro-cosmic.sh com o launcher principal novo. PTY injeta eventos reais mas não comprova terminal físico.

1. Registrar SO/arquitetura, versão do cosmic-term, shell, TERM, fonte/Unicode e dimensões; conferir `hud --version`/recibo.
2. Usar só dados sintéticos/privados explícitos como no [guia local](../rust/docs/local-preview.pt-BR.md). Sem providers reais, Vault/config pessoal ou prompts.
3. Conferir início/resize em80×24,120×40,160×50: painéis, cores, relógio/histórico e nenhuma sobreposição.
4. Conferir clique/Alt+setas, Alt+Z/Esc, press→motion→release no título, alvo visível e cancelamento sem gravação. Shift+arrastar pertence ao terminal; roda/scroll e seleção precisam ser registrados.
5. Editar Unicode com setas/Home/End, histórico↑/↓, Tab e Esc; agenda/notas sófixture, restart round-trip e contexto/gates por aba sem execução.
6. Sair Ctrl+C ou /sair; verificar terminal restaurado e ausência de filhos fake pendurados se teste fake estiver explicitamente configurado. Guardar resultados por gesto, erro e versão.

Falha física não é corrigida abrindo janela fora da área de trabalho escolhida nem executando provider. [Roteiro Python histórico42passos](legacy/roteiro-cosmic.pt-BR.md) conserva contrato/tests legado, sem ser o aceite visual Rust.
