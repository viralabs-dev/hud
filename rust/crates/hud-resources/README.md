# Read-only resource slice

Staged `hud-resources` crate, inspected against Python `hud/skills.py`, `docs.py`, `usage.py`, `monitor.py`, and `text.py`. No installation, provider calls, process inspection, automatic personal-directory discovery, or document writes.

Implemented:

- Explicit ordered HUD/Claude/Codex skills roots; case-insensitive deduplication preserving first-source precedence and recording all origins. Frontmatter supports first scalar value and simple folded/literal blocks. Up to 5,000 entries/root and 500 skills; 16 KiB listing header and 64 KiB exact UTF-8 context. Symlinked skill installs remain supported; regular-file check and nonblocking descriptor prevent special-file reads.
- ANSI CSI/OSC/DCS removal and control/bidi sanitization for display/context; Unicode retained. This sanitization is not a trust boundary for instructions in skill content.
- Explicit Claude statusline cache and Codex rollout readers. Filenames restricted to supported cache/rollout families, denying `auth.json`; no credential lookup. Unix reads pin ancestors through `openat` with NOFOLLOW, reject symlink components/final file, nonregular files, foreign ownership or group/world writable files. Cache limit 4 KiB; rollout tail limit 256 KiB, partial first record discarded. This does not claim protection against an adversary with the same user identity or a pinned directory moved by that identity.
- Strict finite numeric usage parsing; invalid present Codex windows reject the record; latest valid tail record wins. Source timestamps supplied by caller for Codex, no fabricated polling freshness.
- Pure Codex session_meta parser retaining only sanitized id/cwd/parent/nickname/role, never prompts/responses/tool output.
- Bounded hud-doc proposal parser (4 MiB input/total, 256 KiB/document, 200 documents) with nested fences, last duplicate wins, portable Markdown paths; returns proposals only, no save API. Attribute grammar is deliberately narrower than Python: one `arquivo=` attribute.

Validation: Linux Rust/Cargo 1.99.0; `cargo test --locked --offline` 16 fixture tests, `cargo clippy --locked --offline --all-targets -- -D warnings`, and fmt pass. Fixtures explicitly set private mode because the sandbox default umask permits group writing. No personal paths were read in tests.

Partial gaps: no live TUI integration; no skill install or write confirmation flow; no project scan/document saves; no Claude in-process rate-limit event parser; no default SHA1 statusline-path derivation, dated session discovery, tracker freshness, live watcher, process monitoring, full session/subagent monitor, Claude/OpenCode monitor adapters. Non-Unix safe filesystem reads fail closed with Unsupported; pure parsers remain usable. macOS/Windows/ARM/MSRV 1.88 were not natively validated.

Public `read_owned_file(path, max_bytes)` returns bounded bytes (1 byte–4 MiB cap), with pinned NOFOLLOW Unix chain, regular current-owner checks and strict mode 0600/0400 (no group/world permissions). Known auth/credential/private-key filenames are denied case-insensitively; arbitrary filenames may still contain secrets, so callers must select appropriate files explicitly. No output/logging of contents. Non-Unix reads fail closed. Tests cover exact/over limits, private/group-readable permissions, binary content, traversal, symlink file/parent and credential denylist. Foreign UID enforcement exists in shared descriptor reader; no foreign-owner file was created because no privilege escalation was requested.
