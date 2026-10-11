# Customization persistence staging

Integrate src/lib.rs and src/store.rs into hud-custom; existing APIs retained.
Add rustix =1.1.5 with fs/process under cfg(unix) if missing. Staging manifest
uses absolute paths; integrated dependencies must remain ../hud-domain/resources.

`Custom::to_json()` serializes validated bounded JSON v1 without losing feed,
title, line limit, width, height/auto or weight metadata. It reparses and compares
the entire value before returning bytes. `Custom::reordered(&Layout)` preserves
slots by panel ID and widths at column positions; rejects differing panel sets,
changed column count or invalid >8 slot columns. Column insertion/removal requires
an explicit new-width policy and remains blocked rather than silently discarding
geometry. Command feeds remain denied and are never executed by serialization.

`store::FileCustomStore::new(existing_private_directory)` selects custom.json.
`CustomStore::load()` returns CustomSnapshot { custom: Option<Custom> }.
`save(&next_custom,&previous_snapshot)` verifies prior file bytes and identity,
directory identity chain and private permissions before atomic replacement;
returns the fresh loaded snapshot. Store creates no directories, selects no
personal defaults, imports no Python files, and validates new data before writes.
Directory must exist with 0700; files require 0600, regular, owned by caller.
Readers/writers traverse directory descriptors with NOFOLLOW, bounded reads,
exclusive private temporary file, file fsync, rename and directory fsync. Native
Linux only tested. Non-Unix fails closed; exclusive initial creation requires
Linux. Snapshot detects ordinary concurrent/stale/manual edits but is not a
transactional lock against malicious same-UID changes between final check/rename.
A directory fsync/load error after rename may mean data committed despite returned
error; UI should reload/report, never claim that error proves no disk mutation.

UI integration sequence:
1. Load snapshot; if corrupt/unsafe report error and retain explicitly selected
   original custom config without overwriting damaged state automatically.
2. On release calculate domain Layout and `custom.reordered(&layout)`.
3. Call store.save(new,&snapshot) before changing in-memory layout/custom.
4. Success: update custom/layout/snapshot together. Error: retain old memory,
   report actionable error; safe reload can reconcile ambiguous postcommit errors.

Tests: 14 including restart/Unicode, inert command serialization, geometry and
feed/title preservation, corrupt manual edits, stale inode replacement, moved
root, symlink refusal, private permissions/bounds and atomic old-reader semantics.
No installed preview or user state was altered.
