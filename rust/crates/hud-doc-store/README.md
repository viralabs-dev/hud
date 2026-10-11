# Explicit document writes (Linux-validated preview)

`FileDocumentStore::new(explicit_root)` -> `preview(Proposal)` -> show `Preview::proposal()` and `existing()` to user -> `apply(&preview, Approval::CreateNew | Approval::OverwriteExisting)`.

No default paths, directories created, personal files read, or writes during preview. Paths use hud-resources portable Markdown validation; proposal/existing cap 256 KiB. UTF-8 proposed text bytes are preserved exactly, including CRLF, trailing blank lines, wikilinks and escaped aliases. Existing bytes are arbitrary bounded bytes, never normalized. Roots and parent directories must already exist. Parent creation/batched transactions are deliberately absent.

Each component is opened NOFOLLOW relative to directory descriptors. Root and note parents must be current-user-owned and not group/other writable; non-sticky writable ancestors refused. Final existing files must be owned regular files not group/other writable. Preview captures ancestor device/inode identities and target bytes/device/inode/timestamps. Apply reopens/rechecks chain and target before committing, refusing manual edits, inode replacement, symlinks and moved directories since preview.

Temporary 0600 file is exclusive, written and fsynced, then renamed atomically and directory fsynced. Linux creation uses NOREPLACE; existing overwrite requires explicit distinct Approval. No backup is created; callers should retain preview existing bytes or implement approved backup before destructive overwrite. Overwrite permissions become 0600. A post-rename directory fsync failure returns error although replacement may already be committed: caller must re-read, not blindly retry.

Same-UID attacker/manual writer can still change target or relocate a directory between final checks and rename; POSIX rename has no byte/inode compare-and-swap. This is best-effort stale-preview detection, not perfect CAS or a same-UID security boundary. Symlinks are not followed, including when a final symlink appears during rename (the entry itself would be replaced). No claim of native macOS/Windows validation. Non-Unix unsupported; exclusive creation outside Linux unsupported.

8 fixture tests passed; cargo fmt and clippy -D warnings passed. Tests cover zero-mutation preview, byte preservation, approval kinds, stale bytes/inodes, symlink replacement, moved ancestors/root, unsafe/missing parents, size cap, nonregular/world-writable files, old-open-reader atomic behavior. No real Vault write performed.

Integration: change hud-resources dependency path from staging absolute path to ../hud-resources; add workspace member and TUI dependency. CLI proposal-file reading must be bounded and explicit, with separate review/confirmation before apply.
