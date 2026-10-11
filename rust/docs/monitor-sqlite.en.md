# SQLite and local metadata monitoring

[Português brasileiro](monitor-sqlite.pt-BR.md)

OpenCode DB input is explicit (`--monitor-opencode-db ABS_PRIVATE_DB`) or inside the explicitly selected monitor root. Linux optionally loads existing system SQLite; no database tool is installed/bundled. Missing/unsupported ABI is a visible unavailable state, not a TUI crash. Other native platforms remain pending.

Private descriptor-opened DB and optional WAL are copied into a private temporary snapshot with bounded size/identity checks. Original SHM is not copied; SQLite may create SHM only in the private snapshot. Direct descriptor/VFS access is a separate blocked AT-080 prototype, not a delivered backend. SQLite opens read-only with trusted_schema OFF/query_only ON. Fixed bounded read-only queries accept only normal session schema, id,parent_id,directory,time_updated; no title/message/prompt/tool body. Session metadata is not proof of liveness. VM/query budgets do not guarantee hard deadline for stalled filesystem/dlopen/prepare; worker shutdown limitations are explicit.

Known roots are codex/sessions, claude/projects and opencode/storage/session under a selected root. Scans reject links, cap entries/depth/session groups, deduplicate by provider/id and retain orphan/cycle visibility. Optional own-UID Linux process sampling reads comm/stat only, never args/environment. It is a separate process sample, not complete discovery of npm launchers or session state.

See the Portuguese detail for numeric caps and source references; fixture tests validate metadata/privacy contracts, not native databases across every release/OS or physical monitoring behavior.
