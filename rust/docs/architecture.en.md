# Rust architecture and migration boundaries

[Português brasileiro](architecture.pt-BR.md) · [current status](../README.md)

Rust is the target architecture. `hud-domain` owns pure layout/focus/drag transitions; `hud-layout-codec` the Rust layout format; `hud-layout-store` private filesystem persistence. `hud-data` owns agenda, calendar, notes, folders and bounded Vault discovery. `hud-custom`/`hud-toml` parse configuration and reviewed proposals; `hud-feeds` refreshes text/files and explicitly trusted argv feeds. `hud-resources` reads skills, usage and session metadata; `hud-metrics` isolates OS sampling. These crates do not choose personal defaults.

`hud-agent-protocol` defines provider/version/profile adapters and subscription/platform contracts. `hud-agent-process` supervises workers, streaming, bounded events, process trees and version/auth gates. `hud-runner` executes configured literal argv with minimal environment and fixed PATH. `hud-doc-store` applies explicitly approved document proposals with opened-descriptor checks. `hud-tui` coordinates use cases, confirmation, tabs, Unicode input and Ratatui/Crossterm rendering.

Dependency boundaries use small traits where alternate storage/metrics/protocol implementations or fault tests exist. Domain state changes are deterministic; filesystem/process changes occur in adapters with explicit acceptance. Libraries and fixture parsers are not evidence of native OS support.

Data flow: selected explicit paths → bounded private parse → application state/proposal → displayed review → distinct approval → worker/store → bounded events or reconciled state → rendering. Config/custom/doc/command saves never authorize command execution; fake and production capabilities cannot be silently interchanged. Profile/folder/backend changes revoke relevant proposals and sessions.

POSIX descriptors pin directories and reject symlink traversal; atomic rename and sync provide explicit durability handling. A sync error after rename may follow a committed replacement: reconcile rather than blindly retry. Byte/inode compare-before-replace detects manual edits but is not a same-UID perfect compare-and-swap boundary. Native Windows storage is still a required adapter, not supplied by a policy fixture.

Migration order and acceptance remain in [parity](parity.en.md) and [support](support.en.md). Rust owns runtime and primary hud entry after verified cutover. Historical source/config/data stay separate with explicit fallback hud-python. Import is optional/reviewed and rollback receipt/CAS based; development helpers are not runtime.

Separate workers handle metadata-only PATH inventory and default backend preparation/hash, without startup login/model probes. Found/declared version/unknown auth is not process authorization. Async preparation keeps UI available; no forced cancellation of remote filesystem calls is promised.
