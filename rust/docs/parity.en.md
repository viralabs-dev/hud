# Rust migration: parity and security acceptance

> **Link scope:** references to crates/scripts/source/Python are checkout-only. The local archive includes docs/licenses, not source/build tools; those source links are not present inside the archive.

Reference inspected: Python `ddce3688548e6ebf1df62a554d0003b99f75060c` (v0.18.0), clean at inspection. Source references below are checkout-relative. This is a migration contract, not a claim that Rust implements these features. The target runtime is Rust; historical source is the behavioral comparison baseline, not an active implementation guide. Local adoption under hud at 0.10.0-preview.12 is explicitly authorized, without claiming full parity. The original baseline contract review did not execute external agents. Current local implementation/test evidence is maintained separately in [support](support.en.md).

## Functional parity inventory

| Capability | Existing implementation and test anchors | Rust acceptance |
|---|---|---|
| Configuration / folder | `hud/config.py:150,187–215,273–395,408–499`; `tests/test_hud.py`, `test_plataforma.py` | Preserve TOML semantics, rejected commands, warnings, safe executable resolution, OS-specific private paths and remembered folder. No automatic rewrite of Python files. |
| Layout / customization | `hud/layout.py:287–400,478–690,745–784`; `hud/custom.py:129–188,217–318,347–546`; `test_layout_ops.py`, `test_custom.py` | Read existing layouts; match geometry/minimum-size errors, swap/insert/new-column operations, digest-based trust, private atomic persistence and remembered selection. Untrusted custom command feeds require explicit trust. |
| Focus / drag / keyboard | `hud/ui.py:1359–1478,1486–1627`; `test_ui.py` | Alt+arrows/click focus, Alt+Z emphasis, Esc cancellation, wheel/input/history/tab parity. Add press→move→release state, visible dragged title and valid drop target; release commits once, cancellation never writes. Shift text selection remains terminal-controlled. |
| Command execution / editing | `hud/runner.py:13–108`; `hud/comandos.py:33–118`; `hud/ui.py:1135–1164,1646–1660`; `test_comandos.py` | Configured argv only; confirmation before marked command; one run per key; live completion events, timeout/truncation and failure display. Proposal preview precedes private atomic save; discard does not write. |
| Agent conversation | `hud/agent.py:20–190`; `codex.py:29–40`; `claude.py:107–136`; `opencode.py:44–66`; `test_opencode.py`, `test_hud.py`, `test_verificar_leitura.py` | Preserve availability detection, modes/colors/tabs, JSONL event mapping, session resume/reset, model/cost/token data and profile changes starting fresh conversation. Version-specific real CLI contracts remain separate acceptance gates. |
| Metrics | `hud/metrics.py:43–159`, `metrics_linux.py`, `metrics_macos.py`, `metrics_windows.py`; `test_metrics.py` | OS adapters provide bounded sampling of CPU/memory/disks/network/uptime; unsupported readings degrade to unavailable rather than invented zero; histories and units match. |
| Vault | `hud/vault.py:88–200,252–380`; `test_vault_links.py` | Bounded read-only scan/search, Kanban/task/due parsing, newline normalization, file ownership and link/race protections. No writes from scanning. |
| Documentation | `hud/docs.py:116–174,241–486`; `hud/ui.py:1881–1954`; `test_docs.py`, `test_docs_dirfd.py` | Discover projects, parse restricted Markdown proposals, preview exact destinations, refuse traversal/link escape, explicit overwrite consent; preserve folder/wikilink conventions. |
| Agenda / notes | `hud/agenda.py:46–179`; `hud/ui.py:1999–2066`; `test_hud.py` | Date/time parsing, add/remove/done/prune/upcoming and notes keep compatible storage; atomic private saves and restart round-trip. |
| Skills | `hud/skills.py:42–181`; `hud/custom.py:574–671`; `test_skills.py`, `test_skill_files.py`, `test_skill_hud_comandos.py`, `test_skill_projeto_docs.py` | Discover correct precedence, bounded safe reads, frontmatter parsing, prompt/context composition and packaged resources. Installation remains an explicit command. |
| Usage | `hud/usage.py:62–125,216–337`; `test_usage_codex.py` | Claude owned/private cache + stream events; Codex owned bounded session tail. Never open OAuth/auth files. Stale/missing values display unavailable; preserve remaining-percent semantics. |
| Monitor | `hud/monitor.py:145–339,465–954`; `test_monitor.py` | Processes + bounded local session metadata, parent/subagent grouping, freshness and budgets. SQLite is read-only. Preserve current Windows limitation (session data available, process list absent) until separately delivered and tested. Never display full conversation content. |
| Text / Markdown | `hud/text.py`, `hud/markdown.py`; `test_markdown.py`, `test_ui.py` | Strip escape/control/bidi sequences from all external sources; Unicode cell width, wrapping, narrow tables and proposal summaries retain intent. |
| Packaging / CLI | `pyproject.toml:1–51`, `hud/__main__.py`, `scripts/hud`; `test_installers.py`, `test_workflows.py`, `packaging/smoke.sh`, `smoke.ps1` | Isolated standalone executable, version/check/custom-check/install-skill behavior and bundled templates/licenses; no dependency on a shell to execute commands. Python continues as separately selectable fallback. |

## Security contracts that cannot be collapsed into one “read-only” promise

- **Codex:** read-only sandbox plus MCP disabled (`hud/codex.py:29–40`). This blocks sandboxed writes/network commands but permits reading any user-readable file. Model-provider transmission still occurs. Rust must disclose this scope; do not claim folder isolation or credential filtering that this adapter does not implement.
- **Claude:** scoped Read/Grep/Glob, explicit deny patterns for secrets in both profiles, strict MCP config and `dontAsk` in leitura, budget in both profiles (`hud/claude.py:29–55,95–136`). Path rules have Windows drive/UNC normalization. Complete mode intentionally uses configured permission mode; it is not read-only.
- **OpenCode:** permission default `ask`, named read allowances, secret deny globs, external-directory rules, grep in ask (`hud/opencode.py:27–59`). Headless auto-rejection is an external CLI behavior, not guaranteed by generated JSON alone. Pin and record tested CLI version; real synthetic fixtures must prove rejection without interactive approval. Complete mode uses user configuration.
- **Process boundary:** prompts through stdin, shell=False, isolated process groups, bounded prompt/JSONL/stderr and timeout/stop (`hud/agent.py:20–43,114–190`). Ordinary commands use a fixed safe PATH and no inherited tokens; agent environment deliberately includes provider-prefixed variables and user PATH (`runner.py:16–38` vs `agent.py:33–56`). Preserve this distinction without printing environment values.
- **Tree termination:** POSIX group signals; Windows absolute taskkill /T /F plus fallback (`hud/plataforma.py:390–427`). A unit assertion on arguments is insufficient: spawn harmless child+grandchild fixtures and prove they exit on timeout/cancel on each OS. Test for races/deadlocks and terminal restoration.
- **File boundary:** validate opened descriptor, regular file, size cap and identity; POSIX relative directory descriptors protect intermediate path components (`hud/vault.py:155–200`; `hud/docs.py:356–486`). Windows reparse/identity checks differ (`hud/plataforma.py:338–370`); retain explicit platform limitations until handle-based containment is validated. Do not replace these protections with canonicalize-then-open.

## Architecture boundaries (idiomatic SOLID)

Domain owns immutable layout/panel IDs, focus, drag state, geometry, agenda and parsing rules; no terminal/files/processes. Application owns commands/events and explicit confirmation, cancellation and persistence use cases. Adapters own TOML/files/OS metrics/process trees/agent protocols. UI converts Crossterm events into application commands and draws Ratatui widgets. Dependency direction points inward.

Use small traits only where alternative implementations or test seams exist: LayoutStore, ProcessSupervisor, MetricsSource, NoteReader and AgentProtocol. Agent adapters are strategies; OS filesystem/process adapters isolate platform policy. Typed application commands justify the command pattern. Avoid a trait for every struct, inheritance emulation, generic repositories and duplicate DTO layers. Pure deterministic state transitions and fake adapters should test failure paths without launching real providers.

## Platform acceptance matrix

The Python [packaging guide](../../docs/legacy/python-guide.pt-BR.md#instalar) lists Linux amd64/arm64; macOS amd64/arm64 (11+); Windows amd64 (10/11, ARM only through x64 emulation). This is the Python target, not Rust support evidence.

| Gate | Linux amd64 / arm64 | macOS Intel / Apple Silicon | Windows amd64 |
|---|---|---|---|
| Build / unit | Native target, lockfile, licenses, minimum libc documented | Native targets, minimum OS documented | Native target, MSVC/runtime requirements documented |
| Package / install | Archive checksums, atomic user install/uninstall, no PATH/global credential edits | Same plus honest signing/notarization status | PowerShell 5.1/7 archive install/rollback and locked executable behavior |
| Smoke | Run outside source with empty/malformed config, check/version/custom-check | Same | Same with spaces, drive/UNC paths and OEM/UTF-8 output |
| Process integration | Group timeout/cancel child tree, cleanup, inherited-FD safety | Same; OS metrics adapter and command availability | Process-tree timeout/cancel, npm shim launch without cmd shell, inherited handle safety |
| Automated interaction | Real PTY injected key/SGR mouse events + resize, snapshots and persisted layout | Same incl terminal decoder differences | ConPTY/console interaction harness or explicitly tracked missing coverage |
| Physical interaction | Named terminal/version; title drag with visible move/drop feedback, shift selection, wheel | Named terminal/version and identical checklist | Windows Terminal/version and identical checklist |

For every physical gate record OS/arch, terminal emulator, shell, dimensions, Unicode font/width behavior and results. Shell is not terminal emulator. Ratatui/Crossterm do not supply external agents, their authentication, network, model availability or version compatibility. WSL is Linux coverage; Windows ARM emulation is not native ARM acceptance. Other OS/architectures remain unverified.

Existing `tests/test_ui.py` injects events in a POSIX PTY; `.github/workflows/ci.yml` skips binary screen tests on Windows. Passing layout unit tests, build, installer or smoke does not prove mouse interaction. Existing tests should become golden fixtures/behavior specifications; avoid mechanically reproducing implementation details.

## Historical rollout sequence and public-delivery gates

1. Initial sequence preserved historical source/config/data and used a separate Rust entry. Current authorized local entry is hud; hud-python is the explicit preserved fallback. Never auto-mutate originals.
2. First executable vertical slice: render real layout, resize, focus, drag feedback/drop + persisted layout through a private store; headless domain tests plus terminal restoration on normal exit/error. Mark every absent capability above as pending.
3. Migrate read-only modules before process execution; add guarded agent/command adapters only after failure and permission gates pass. Keep destructive/full-profile capabilities explicitly disabled until implemented.
4. Migration/import is explicit, dry-run preview + byte-preserving backup + schema validation + atomic save. Test malformed/unknown fields, Unicode and rollback round-trip. Trust digests cannot silently transfer to altered command content.
5. Public rollout/release requires capability/platform gates and explicit authorization. The owner-approved local Rust cutover 0.10.0-preview.12 has a separate checked rollback receipt; it does not close those gates or authorize publication. Rollback scope distinguishes previous Rust artifacts from hud-python and untouched historical data.

Contract-review limitations: no physical terminal session or real CLI permission probes. Linux integration builds/tests now have separate receipts; cross-platform/native/MSRV acceptance remains pending. Read [current support](support.en.md) rather than inferring delivery from this required-parity contract.
