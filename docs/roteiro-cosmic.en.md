# Physical Rust HUD checklist for COSMIC Terminal

[Português brasileiro](roteiro-cosmic.md)

For Rust command `hud`, 0.10.0-preview.12. Open the terminal manually in your chosen workspace; do not run legacy scripts/roteiro-cosmic.sh against the new primary launcher. Injected PTY events do not prove physical terminal behavior.

Record OS/arch, cosmic-term/shell/TERM/font versions, Unicode and dimensions. Use explicit private synthetic data from [local guide](../rust/docs/local-preview.en.md), with no real provider/Vault/personal context. Check80×24/120×40/160×50 resize, borders/colors/clock/history; click/Alt+arrows, emphasis/Escape; title press-motion-release target/drop/cancel; terminal Shift-selection and wheel; Unicode editing/history/Tab, fixture agenda/notes restart, per-tab readiness with no execution; Ctrl+C/exit restoration and fake child cleanup only if explicitly configured. Attach per-gesture result/version/errors.

[Historical42-step guide](legacy/roteiro-cosmic.pt-BR.md) preserves legacy test contracts, not Rust acceptance. Never open outside the chosen workspace or launch a real provider to resolve a physical test failure.
