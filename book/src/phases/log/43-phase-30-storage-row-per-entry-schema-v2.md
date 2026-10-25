# Phase log 43: Phase 30: storage, row-per-entry, schema v2

Date: 2026-10-08. Phase 30 (queue position; was 25 → 23 → 24 → 25 → 30 across renumberings; design in review-01 section 2.1 and phase log 23 "Phase 23 - Storage"). Lane A1, after 25. Version untouched (integrator's). Nothing committed. This session also finished Phase 29 at its original scope first (phase log 42 was rewritten for that: eight rule ids, `secretEnv`/`secretMounts` on the Kubernetes Deployment); read it for that change.

## Goal

review-01 2.1: the vault was one JSON string in one row. Every write was a full read-modify-write; compare-and-swap was whole-blob, so two writers editing _different_ entries conflicted and both branches of the conflict prompt were destructive; `version_history` lived inside the blob; nothing could be indexed. Do it before 1.0 and before Phases 35+ (config history, nodes) which would otherwise snapshot into the blob.

## Result

- `vault-core/src/storage.rs` (new, ~1,500 lines with tests): schema v2. Tables `vault_rows(kind,key,pos,data,rev)`, `vault_history(key,data,rev)`, `vault_saves(seq,state_hash,at)`, `vault_changes(seq,kind,key,prev_rev,new_rev)`; meta `schema_version=2`, `data_hash="<seq>.<state hash>"`, `state_acc`. Kinds: `entry`, `project`, `categories` (one row), `doc` (one row holding every other top-level key, and the marker that a vault exists).
- The document API did not change: `load_vault`/`save_vault`/`vault_version`/`verify_vault_integrity`/`migrate_legacy_json` keep their signatures. New: `load_vault_lite` (no history), `ensure_current_schema`, `MERGED_SUFFIX`.
- `vault-core/src/lib.rs`: `VAULT_SCHEMA_VERSION = 2`; `save_vault_txn` rewritten around split, merge, per-changed-entry history/audit, write; audit and history logic is the old code, only run for entries whose content changed. Audit indexes added (`entry_provider`, `timestamp`). The sha2 import moved out of lib.rs.
- Server (`envv-server/src/lib.rs`): PUT returns `204` with `ETag` (the version to send as the next `If-Match`) and `X-Vault-Merged: 1` when it merged; CORS now exposes both headers (without it a cross-origin page could never read the ETag, so the remote store's conditional writes were silently unconditional); the 409 text names the entries both writers changed; `/api/vault/entries` reads without `version_history`; the non-owner PUT path calls `ensure_current_schema` before reading its merge base.
- Tauri (`src-tauri/src/lib.rs`): `load_vault` calls `ensure_current_schema` before reading the version (see "the bug that mattered").
- CLI: `access.rs` keeps the version it loaded in a thread-local and saves conditionally on it (it used to save unconditionally, "single process"); `envv doctor` has a `storage` check (rows verify, row counts, and a Note when `vault.db.v1.bak` still exists).
- App (`src/ts/state.ts`): both stores understand the `+merged` marker (`takeMerged()`); `persist()` reloads after a merged save; `VaultConflictError` carries the entry names; the conflict dialog says what is and is not in conflict; `RemoteVaultStore` adopts the returned ETag and reloads on `X-Vault-Merged`.
- ADR-0138, SUMMARY entry, decisions-log row, a storage section and four pitfalls in AGENTS.md, phase row 30 done.

Verification (last run, after the final edit): `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` all `ok` (vault-core lib 305 + 1 ignored benchmark, envv-server 34, the new `envv-cli/tests/storage.rs` and `check.rs` each 1, the rest as before); `tsc`, `eslint --max-warnings 0`, `prettier --check` clean; `vitest run` 63 files / 1300 tests. Mutation checks on `storage::merge` and migration: disabling the "writer left it alone, keep theirs" branch, the "writer deleted an untouched row" branch, the "created after you read it" branch and the blob deletion each failed a test.

## The merge, precisely

A save sends the whole document plus the version it read. If that version is the current one, the document wins as written. Otherwise the token is `"<seq>.<hash>"`; the hash must match `vault_saves[seq]` (so a forged or pruned token is a whole-vault conflict) and `vault_changes` after `seq` give, per row, `prev_rev` of the earliest later change = what the writer saw. Per incoming row with such a change: writer's rev == that `prev_rev` means untouched, the stored row (or its absence) stands; writer's rev == stored rev means both arrived at the same content; otherwise conflict. A stored row the writer lacks: untouched since the base means the writer deleted it, created after the base means keep it, changed since the base means conflict. The result keeps the writer's order, then the rows the writer did not have.

## The bug that mattered

Found by reading, before anyone hit it: the Tauri `load_vault` reads the version _before_ the data (a deliberate, documented ordering). On an unconverted v1 vault the conversion happens inside `load_vault`, so the version it handed the app was the old blob's hash, and the app's first save after upgrading would have been refused as a whole-vault conflict. `ensure_current_schema` before the version read fixes it; `the_version_read_before_the_data_is_valid_after_a_conversion` pins it; the server's non-owner PUT path got the same call.

## Decisions (with rejected alternatives)

- **Keep the document API, change only what a save does.** Rejected: a row-level API for clients (deltas, per-entry endpoints). It would remove the remaining O(vault) CPU but touches the app, server, CLI and every test at once; it is a separate phase.
- **Merge by content hash and a change log, not by per-row revision counters shipped to the client.** A counter in each entry would round-trip through the frontend and every `formToEntry`, and the client would have to send a base map. Rejected.
- **A merged save returns `<token>+merged`.** Rejected: return the new token (the writer's stale copy would overwrite the merge next save); return the old base (the next save conflicts with the writer's own previous one, which a first version of this phase did and a test caught).
- **XOR accumulator for the state hash.** Rejected: hashing every row on every save (my first version; the one-edit save was _slower_ than v1 until this was removed).
- **History in its own table, one row per entry.** Rejected: one row per revision (more rows, no query that needs it yet).
- **Backup, not dual-write.** The first open of a v1 vault copies the file to `vault.db.v1.bak` and deletes the blob; a v1 build refuses the file (`VAULT_SCHEMA_TOO_NEW`). Rejected: keeping the blob updated for downgrade (a second copy of every secret that drifts).
- **CLI saves conditionally.** Behaviour change: a long-running `enrich --online` that races an edit of the _same_ entry now exits 6 naming it instead of silently overwriting. A run that saves twice after a merged first save is refused at the second (sentinel base), not allowed to clobber.
- **Chunks, templates and the rest of `Project` stay one row per project.** A project row is rewritten when any of its chunks change; chunk-level rows would be the next step if projects grow large.

## Measurements (release build, 5,000 entries, 3 history records each, 3.6 MB document)

`storage::tests::measure_one_edit_against_the_blob`, run with `--ignored --nocapture`:

|                                       | v1                                                            | v2                                                                     |
| ------------------------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------------- |
| save one edit                         | 166 ms (read blob, parse, index, serialise, hash, rewrite)    | 90 ms (of which ~36 ms is splitting and hashing the incoming document) |
| bytes written for that edit           | 3.6 MB                                                        | one row                                                                |
| full load                             | ~35 ms (one parse, from memory of the shape; not re-measured) | 88 ms (10,000 rows parsed)                                             |
| load without history (selective read) | n/a                                                           | 22 ms                                                                  |

Two honest notes. The first v2 number I measured was 159 ms against a naive "v1" of 56 ms, which only looked slower because the naive v1 skipped the parse and index; the faithful sequence is the 166 ms above. And a full load got _slower_; unlock and the 3-second poll are not affected (the poll reads one meta row), so I left it, but it is the first thing to look at if unlock feels slow on a large vault.

## Dead ends

- First merge returned the writer's old base after a merge: its own next save conflicted with its previous one (test caught it). Replaced by `+merged` and a reload.
- A per-save full `ORDER BY` state hash and a per-entry `to_string` + SHA made the new save slower than the old one. Replaced by the XOR accumulator and `to_writer` into the hasher.
- The ignored benchmark first compared against a v1 that skipped the parse; fixed (above).

## What was not done

- A delta API from clients (the remaining O(vault) CPU), chunk-level rows, history-per-revision rows, and indexes beyond the audit ones: nothing queries by them yet. `load_entries_where` exists in `storage.rs` and has no caller.
- No visual check of the conflict dialog text in a real window.
- `time`-based pruning of `vault_saves`: retention is "newest 2000 saves", which a vault saved thousands of times a day (a polling script) would burn through in hours, making an hour-old token a whole-vault conflict. If that bites, raise `KEEP_SAVES` (it is one constant) or prune by age.
- The remote store over the pinned-HTTPS proxy surfaces no headers, so it still has no ETag there and writes unconditionally, as before.

## Traps for the next agent

- `version_history` is **not** in `entry` rows. Code that reads rows directly (SQL) and wants history must read `vault_history`.
- Never `UPDATE vault_rows` without updating `rev` and the accumulator; `verify_vault_integrity` will (correctly) report tampering. Go through `save_vault`.
- A new top-level key in the document needs nothing; a new per-entity table needs a kind in `storage.rs` and a case in `merge`.
- `split` consumes the document; clone first if you still need it.
- The thread-local base in `envv-cli/src/access.rs` is per thread; a CLI command that saves from another thread would save unconditionally. None does.
- Test vaults that plant a v1 blob must also set `schema_version` to `1`; `plant_v1` in `storage.rs` tests shows the shape.
- `ENVV_SERVER_URL` in the user's shell is irrelevant here (tests remove it).

## Commit (for the user; I never commit)

Phase 30 only; several of these files also carry unrelated earlier uncommitted edits (`state.ts`, `lib.rs` files especially), so staging them commits those too. Phase 29's follow-up (its exporter/model change) is in the same tree: `envv-cli/src/exporters.rs`, `src/ts/chunk-ops.ts`, `src/ts/chunks/starters.ts`, `tests/fixtures/parity/k8s.yaml`, `vault.json` belong to it (command in phase log 42). Left out: `AGENTS.md`, `the session records/`, `HANDOFF.md`, `book/` (untracked; holds ADR-0138).

```powershell
git add vault-core/src/storage.rs vault-core/src/lib.rs envv-server/src/lib.rs src-tauri/src/lib.rs envv-cli/src/access.rs envv-cli/src/doctor.rs envv-cli/tests/storage.rs src/ts/state.ts tests/vault-merge.test.ts; git commit -m "Store the vault row-per-entry in schema v2, merging a stale writer per row instead of refusing it, with a backed-up one-way conversion from the single-blob format."
```

## Follow-ups scheduled

The items under "What was not done" are scheduled as sub-phase 30.1 and 30.2 in `AGENTS.md` (Phase table and the Wave-E lanes); the rows there are the source of truth for their scope, footprint and edges.
