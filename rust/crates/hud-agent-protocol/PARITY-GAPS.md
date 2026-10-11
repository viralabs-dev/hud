# Agent protocol milestone / limites

Contracts only, no CLI launched, no credentials accessed, no live/provider authentication claims.

Implemented: Codex read-only + MCP disabled; Claude scoped Read/Grep/Glob, dontAsk/strict MCP, complete Python POSIX deny patterns and budget; OpenCode wildcard ask with explicit permitted tools and secret read denies; stdin prompt, model/resume vectors; minimal POSIX environment; typed bounded JSONL mapping, counters, errors, rate-limit events, profile/provider/folder changes reset sessions.

Not complete AT-072: actual runner glue/process lifetime, UI queue/cancellation/timeout; full profile deliberately returns UnsupportedFullExecution until explicit UI authorization + audited runner; Windows path rules/extra secret list/environment/shims; filesystem canonicalization belongs verified path adapter (caller must supply resolved literal POSIX Claude rules); richer Codex file-change summary; Python context prompt exact wording; full terminal text sanitizer equivalence (current removes control chars but does not remove printable escape-sequence residue); provider auth/discovery/model inventory; config defaults/profile switching in UI. JSON parsing rejects malformed/scalar/oversized lines with explicit errors; integration must choose stream skip/report policy. Line framing/total stream budget must be implemented by runner; parser bounds only each supplied line.

Dependency: official crates.io serde_json =1.0.152, preserve_order essential for last-matching permission rule ordering. Cargo fetched serde_json/indexmap/hashbrown/zmij normal project dependencies, no system packages. Stage tests: 6 pass + clippy -D warnings. Tested Linux Rust/Cargo1.99 only.
