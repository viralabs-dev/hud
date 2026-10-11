# hud

> **Historical Python 0.18.0 reference.** This is not the Rust target architecture or current `hud` installation guide. Old installers/commands are preserved for history and isolated legacy tests; do not run them to update the current Rust installation. The separately preserved fallback is `hud-python`; consult the current [HUD guide](../../README.md).

[Português brasileiro](python-guide.pt-BR.md) · [Rust migration preview](../../rust/README.md)

HUD is a terminal dashboard for system metrics, an agenda, configured commands, an Obsidian Vault and local Claude Code, Codex and OpenCode conversations. Panels support custom layouts, focus and emphasis. The published application is **Python 0.18.0**; packaged executables bundle Python and curses. The additive Rust migration has a separate `hud-rs` launcher, explicit data paths and its own acceptance gates. Installing or testing Rust does not replace `hud` or import Python settings automatically.

## Install the Python application

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
hud --version
```

Windows: follow the [PowerShell installation instructions](python-guide.pt-BR.md#windows). Installers verify release archive checksums; local Rust preview archives are a separate workflow.

| Python release target | Requirements |
| --- | --- |
| Linux amd64 / arm64 | glibc 2.35+; musl distributions use the Python source option |
| macOS Intel / Apple Silicon | macOS 11+; unsigned/not notarized binaries require the documented Gatekeeper procedure |
| Windows amd64 | Windows 10/11, PowerShell 5.1/7; Windows ARM uses x64 emulation |
| WSL | Linux package and requirements |

This describes the Python packaging target, not Rust platform validation. See [installation, update, uninstall and pipx alternatives](python-guide.pt-BR.md#instalar).

## Run and configure

```sh
hud
hud --help
```

The Python [development workflow](python-guide.pt-BR.md#desenvolvimento), [input commands](python-guide.pt-BR.md#entrada), [configuration and customization](python-guide.pt-BR.md#customização), [allowed command panel](python-guide.pt-BR.md#comandos-do-painel) and [agent setup](python-guide.pt-BR.md#claude-code-codex-e-opencode) document the existing application. Configuration examples are in [config.example.toml](../../config.example.toml). Do not copy Python configuration into Rust state directories without the explicit reviewed import workflow.

Use `/c`, `/x` and `/o` to select the corresponding agent; `/ajuda` displays commands. Agent CLIs are separate installations and use their own authentication and permissions. Reading profiles do not constitute operating-system filesystem isolation. Full profiles can modify files; review the provider and effective permissions before submitting work. Usage information is an estimate or local quota metadata, not proof of billing or unlimited use.

## Screens and evidence

[Python terminal recordings](python-guide.pt-BR.md#telas) use a real pseudo-terminal and synthetic Vault/agenda/agent fixtures. PNG/GIF assets render that recording; they are not screenshots of a physical desktop. Only the recorded system metrics come from the host. Keep media provenance and revision details with the recording.

## Develop and verify

Python uses Python 3.11+ and the standard library. Follow the repository's existing test/development instructions; screen rendering tools are development-only. [Python tests](../../tests) and [packaging helpers](../../packaging) are independent from the Rust workspace.

Rust uses [Cargo workspace crates](../../rust/crates), a pinned lockfile and Linux fixture/PTY validation. Start with its [README](../../rust/README.md), [local preview guide](../../rust/docs/local-preview.en.md) and [support matrix](../../rust/docs/support.en.md). A fake-provider test does not validate a real subscription, CLI release, filesystem permissions or billing.

[Security details](python-guide.pt-BR.md#segurança) · [MIT license](../../LICENSE)
