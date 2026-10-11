# HUD Rust: plataformas e limites atuais

[English](windows-macos.en.md)

Linux x86_64 é o alvo local validado. Runtime Rust usa Ratatui/Crossterm; não usa curses/windows-curses nem paths/config Python automaticamente. Native Windows storage/processos/terminal e macOS/ARM ainda exigem validação; parser/calendário/política com fixtures não fecha gate nativo.

[Suporte atual](../rust/docs/support.pt-BR.md) · [arquitetura-alvo](../rust/docs/architecture.pt-BR.md). Não instalar pacotes históricos Windows/macOS como se fossem Rust, nem chamar smoke Python de aceite Rust. [Diferenças históricas Python](legacy/windows-macos.pt-BR.md) conservam seus contratos sem orientar o runtime atual.
