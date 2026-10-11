# Support and acceptance matrix

> **Link scope:** references to crates/scripts/source/Python are checkout-only. The local archive includes docs/licenses, not source/build tools; those source links are not present inside the archive.

[Português brasileiro](support.pt-BR.md)

| Scope | Evidence and remaining gate |
| --- | --- |
| Historical Python 0.18.0 | Existing Python application, packaging targets and tests; [Python guide](../../docs/legacy/python-guide.pt-BR.md) |
| Installed Rust 0.10.0-preview.12 | Linux x86_64 receipt: 392 workspace tests, smoke/fixture/readiness PTYs, recorded verified rollback and dedicated Claude smoke: one prompt/one response; no real Codex prompt; prepared backend selected without startup execution |
| Checkout 0.10.0-preview.12 | Default-wrapper/per-provider readiness correction is installed and locally verified; real readiness gates still open |
| Linux Rust application | Local build/unit/fault/PTY evidence; this is not a distribution/glibc compatibility matrix or physical-terminal acceptance |
| macOS / Windows / ARM | Parsers, source adapters and policy/calendar fixtures exist; native build/process/storage/terminal tests remain pending |
| Rust MSRV 1.88 | Declared; local toolchain 1.99.0, MSRV not exercised |
| FAKE provider path | Synthetic protocol/worker/approval/timeout/cancel tests; no real account/model/billing proof |
| REAL provider path | One isolated Claude smoke passed; Codex has zero real prompts. Remaining versions/profiles/OS permissions are unvalidated |
| Subscription billing | User attestations and login detection are different facts; unknown billing/configuration blocks prompts; token cost estimate is not an invoice |
| Physical terminal | Title drag/drop/cancel, wheel/selection, focus, Unicode/font widths and resize require named terminal/OS/shell/dimensions results |

No complete Rust parity, public release, native cross-platform acceptance or default replacement follows from a local checkpoint. WSL evidence belongs to Linux; x64 emulation is not Windows ARM native support. The installed artifact and active wrapper are distinct: report their paths/version/receipt together.
