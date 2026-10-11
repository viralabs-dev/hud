# Explicit config and proposal review

[Português brasileiro](review-workflows.pt-BR.md) · [current local preview](local-preview.en.md)

Config: select `--import-config PRIVATE_TOML --import-output NEW_PRIVATE_JSON`, then `/config preview` and `/config apply`. Restart with `--reviewed-config PRIVATE_JSON`. Existing export targets require another destination. Original data-dir remains a reference, not silent adoption. Provider binaries are described without execution. `/config rollback-preview` then `/config rollback` removes only this reviewed export under snapshot checks; original/backup remain intact.

Custom: select `--custom-root PRIVATE_CATALOG --custom-selection-dir PRIVATE_SELECTION`. Parse complete successful response proposals with `/custom from-agent`, review `/custom preview 1`, save `/custom save` (new) or `/custom overwrite` (existing). Escape restores the prior visual state. `/custom lista`, `/custom NAME`, `/custom padrao` select/remember layouts. Rollback has its own preview/confirmation and compare checks. Preview disables feeds; save/selection never grants command trust.

Commands: `--commands-output PRIVATE_COMANDOS_TOML`, `/cmd from-agent` or explicit `--command-proposals`; preview declared argv/cwd/timeout/limits/confirmation, then `/cmd save|overwrite`. Rollback previews backup or creation removal; the latter requires `/cmd delete-created`. Saving/reloading never executes.

Documents: explicit root/proposals, `/doc from-agent` → `/doc preview INDEX` → `/doc save|overwrite`. `/doc salvar` reviews a batch and requires a second approval; `/doc descartar` cancels. All preconditions are checked before batch writing and per item. The batch is not one global transaction: a later race/error can produce a reported partial result.

Private bounded files, NOFOLLOW descriptors and byte/inode snapshots reject traversal/links/manual edits. Proposal bytes preserve CRLF/Unicode/wikilinks; unsafe controls fail write validation while display is sanitized. Sync failure after rename can mean publication succeeded with uncertain durability. Restart recovery reads private receipts only and requires fresh rollback review/approval; incompatible/incomplete/tampered/stale receipts fail closed. Same-UID coherent tampering or interruption before receipt persistence is not a guaranteed recovery boundary. Custom rollback after restart restores selected bundle/default, not transient previous geometry.

Fixture/PTY validation covers these flows without real prompts or proposed-command execution. Current artifact evidence is in its installation receipt; preview7/preview11 counts are historical. Native Windows, physical terminals and real provider gates remain open.
