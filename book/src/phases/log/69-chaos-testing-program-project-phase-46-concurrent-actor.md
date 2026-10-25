# Phase log 69: chaos testing program (project phase 46), concurrent-actor bug hunt and fixes

Date 2026-10-10. Start commit 17886a8 (0.42.6). Nothing committed, staged or pushed. Also contains the Compose/nginx/WireGuard enrichment (part 1 of the maintainer's request). Catalog with per-item status: `the session records/chaos-candidates.md` (31 items fixed, rest listed below with a reason).

## Request

(1) Compose, nginx and WireGuard exports and demo projects needed much richer content, like the maintainer's own server. (2) A bigger test method: several actors act on one vault at the same time at different levels (storage, server RBAC/sessions, nodes/hub, clients) to find edge-case bugs: edit vs delete, revoke-all vs add, etc., then fix them properly.

## Method

Four read-only reviewers (one per level) produced the catalog (S, R, N, C). Every item that was fixed has: a failing test first, the fix, and a mutation check (revert the fix alone, watch the test fail; with `warnings = deny` unused-variable mutations are written as `if false && cond`). Tests live in `vault-core/tests/chaos_storage.rs`, `unv-server/src/chaos_tests.rs`, `unv-server/tests/chaos_nodes.rs`, `unv-server/tests/chaos_clients.rs`, `tests/chaos-client.test.ts`, plus unit tests next to fixes (matlog, import, parity).

## Bugs fixed (what the code did -> fix -> proof)

Storage (vault-core):

- S1 audit chain forked: `append_audit` was two autocommit statements (read last hash, insert), so a node audit racing `save_vault` produced two rows with one `prev_hash`. Now `BEGIN IMMEDIATE` when the connection is in autocommit (`append_audit_in_txn` split out). Test: concurrent appenders, chain verifies.
- S2/R6 PATCH without If-Match read then wrote with `expect=None` and reverted rows others changed. `begin_row_patch(conn, expect)` reads and writes in one transaction; used by server `patch_vault` and Tauri `save_vault_rows`.
- S3 stale writer's `version_history` replaced stored history for an unchanged row, losing the rotation record (the old secret). `storage::merge_history` (longer list is base, tie to stored, insert the other side's records before the first strictly older `saved_at`, cap 50), used in the changed-row arm of `save_vault_txn` and in `write()`.
- S4/S7 read skew and false tamper reports: rows and history were read in separate autocommit statements. `storage::in_snapshot` wraps `load_ents_opts`, `verify`, `snapshot_vault`.
- S5 uid registry minted key and pepper twice on first use: `get_or_create_secret` (INSERT OR IGNORE then re-read).
- S6/C13 `pools.json` read-modify-write was unlocked and non-atomic: atomic save (tmp + rename) and `pool::update(f)` under an exclusive `<file>.lock`; `unv pool select/report/reset` use it. Also `matlog` got a lock, `saturating_sub`, pid-named tmp.
- Dangling project references after a concurrent project delete: `storage::repair_references` right after merge.

Server (unv-server, users.rs):

- R1 widening a user's permissions then a stale PUT deleted entries they were never served: `Served` per session, `perm_epoch`; non-owner PUT answers 409 when the epoch changed; merge base is If-Match, else the served version, else merge_base.
- R2 deleted user / revoked token / changed password / disabled TOTP / permission change left the live session working: `purge_sessions` and `bump_perm_epoch` in every such handler (session records `via_token`).
- R3 deleting a class widened its members (fail-open): `delete_class` refuses while it has members (`class_member_count`); permission_expressions rows are deleted with the user or class.
- R4 owner lock raced a login: `lock_gen`; `auth_user` checks it in the same lock scope as the session insert.
- R5 TOTP code accepted twice, replay mark could move backwards: CAS `UPDATE ... WHERE totp_last_step IS NULL OR totp_last_step < ?1` in `verify_user_totp` and `totp_confirm`.
- R7 burst of bad logins bypassed the limiter while Argon2 ran: in-flight guard (`RateGuard`) in unlock and `auth_user`.
- R8 default admin reseeded after deletion: seeded once (marker `default_admin_seeded`; existing users count as seeded).
- R9 read-only users could not save an unchanged readable entry: `merge_user_vault_write` allows `unchanged = prior == entry && readable(entry)`.
- R11 token `expires_at` unvalidated: RFC 3339 parsed with `pool::parse_rfc3339`.
- R13 (deleted-user class counts) covered by the class tests.

Nodes/hub:

- N1/N4 a revoked node's long-poll beat kept being served: beat re-reads the node after the wait (401 if revoked); `revoke_node` notifies waiters.
- N2a stale pull upload survived a new `request_pull`: removed. N2b `accept_pull` could apply bytes that changed after the human looked: `accept(id, target, expected)` with `ACCEPT_CHANGED` mapped to 409; `unv node accept NODE TARGET --sha256`.
- N13 see chaos_nodes tests.

Clients:

- C1-C6, C10 (src/ts/state.ts, vault.ts, modals.ts): undo stack cleared on view reset; `markStale` after a merged save whose reload failed; keep-mine conflict resolution re-applies the user's patch instead of a blind `forceSave` (removed); `dropPreviousVault`; modal save resolves its edit target by id (`resolveEditTarget`) so a concurrent delete cannot write into another entry.
- CLI remote: `RemoteClient.base_version` from the GET ETag, sent as If-Match, "stale-copy" on `x-vault-merged`; local load uses `snapshot_vault`.
- C11 `unv import-vault` replaced the stored secret of the first same-named entry without `--force` and picked an arbitrary entry among duplicates. Now: prefer the entry that already holds the exact secret; a differing secret is a conflict (untouched, listed in `conflicts`); `--apply` with conflicts refuses and writes nothing; `--force` replaces. Desktop pane shows the conflict names. Test `import_never_replaces_a_different_secret_without_force`, mutation checked (guard disabled -> test fails).

## Enrichment (request part 1)

- `nginx_ident` (Rust `exporters.rs`, TS `chunk-ops.ts`) makes upstream names valid identifiers; pinned by `tests/fixtures/parity/nginx-ident.json` asserted from both sides (`tests/cli-parity.test.ts`, `unv-cli/tests/parity.rs`).
- `tools/demo/seed.py` now builds rich Compose/nginx/WireGuard projects with typed fields; `tools/demo/validate-exports.sh` runs the real validators over them. Screenshots were NOT regenerated in this session (needs the docker viewer rebuild; command in the website phase handoff 67: `OUT=shots/website tools/demo/run.sh`, then `scripts/site-shots.sh`).

## Wave 2 (2026-10-11): every remaining catalog item closed

Same loop for each: failing test, fix at the shared function, mutation check (revert the fix alone, test fails).

Nodes and hub:

- N3 hub-made approvals re-pushed after a local edit. Two causes: the hub signed a fresh nonce on every beat (the node's "already used" memory never matched), and an approval was consumed only when the node REPORTED the approved hash, so a file edited between the write and the next beat kept the approval "approved". Fix: nonce = approval id (one approval, one nonce); `record_beat` also consumes on an ok `ApplyResult` for the approved sha; the node's `approvals-seen.json` is written tmp+rename. Tests: `one_approval_carries_the_same_nonce_on_every_beat` (server), `a_reported_write_spends_the_approval_even_if_the_file_was_edited_since` (core).
- N5 `unv-reload-<id>.path` watched the target file itself, so systemd reloaded the service on the agent's rename, before validation, and again on a failed validation's restore. The agent now writes `<state>/reload/<id>` (content: the sha) only after validate and reload succeeded (`nodes_apply::reload_signal_path`); `reload_units` watches that. Existing installs keep the old unit until `unv node install` is run again. Tests: `the_reload_signal_is_written_only_after_a_file_validated`, install unit text test.
- N6 grants are project names; a deleted project's name stayed on the node and a later project of that name was pushed to it. `NodeStore::prune_projects` runs on every beat that has the vault open (audit row `node.project.pruned`). A rename therefore drops the grant (re-grant); that is deliberate, fail-closed. Tests: `a_grant_for_a_deleted_project_is_dropped`, `a_project_recreated_under_a_deleted_name_is_not_inherited_by_the_node`.
- N7 `Limited` socket: the listen-mode total deadline is enforced on every read/write including inside the TLS handshake (it used to reset per read). Test with a trickling peer.
- N8 `poll_all`: listening nodes are polled side by side (8 at a time), not one by one behind a silent node.
- N9 `process` runs on the blocking pool behind a 4-permit gate (`process_off_thread`), not on an async worker; a save no longer runs every held beat at once on the runtime.
- N10 `write_atomic` uses a unique temp name with `create_new`; a rollback after a failed validate/reload refuses to overwrite a file someone else edited meanwhile (it says so).
- N11 a 401 whose body is the clock-skew or replay message is retried (`classify_rejection`), not treated as a revocation that ends the agent.
- N12 one open enrolment token per name; enrol rechecks the name and rolls back memory if the save fails (token stays usable).
- N14 the approval's review link is verified to hold the approved bytes (`reviewable_seq`), otherwise the newest snapshot that does, otherwise none.
- N15 `vault_core::record_event_once`: a result resent after a lost acknowledgement is one audit row.

Storage and server:

- S8 `uid_registry::register`: only a primary-key conflict is "already registered"; any other error is an error. Savepoint per batch, and a batch that registered nothing leaves no provenance row.
- S9 `vault_core::commit()` rolls back when COMMIT fails (used by save_vault, audit append, migration).
- S10 the v1 backup is replaced through tmp+rename on every attempt (a half-written leftover used to count as "the backup exists").
- S12 `verify()` checks every `vault_history` row against its `rev`.
- S13 `config_history::prune` removes only a prefix: it stops at the first row that is recent or inside the kept window (a stepped clock can put a recent row between two old ones).
- S14 a v1 blob next to converted rows is refused with an explanation instead of deleting every row; the older copy and `vault.db.v1.bak` are named in the message.
- R10 `permex::covers` (sound, not complete: unsure means no) and `guard_grant_within_own`: a non-owner may set user/class permissions and assign a class only if what they confer is within what they hold. Narrower is fine (`type:api_key AND env:prod` under `type:api_key`); anything the structural check cannot prove is refused and the owner grants it instead.
- R12 `Universal` cannot be renamed or deleted by a non-owner.
- R15 status/enrol/confirm of one's own second factor are open to the user (`guard_own_totp`); replacing an enabled factor (enrol resets it to disabled) and disabling stay with a manager.

Clients:

- C8 `rename-refs.json` golden table asserted by `rename_provider_refs` (Rust, also used by `unv entry rename`) and `renameProviderRefs` (TS). Rules: an entry with a key_id is referenced by its compound name, so a bare ref becomes the compound; `${bundle:...}` is skipped; a bare `${Old}` is left alone while another entry still carries that provider; `ref_name` follows.
- C9 `persist()` is a queue (one save at a time) and records "what the store holds" from the document as it was when the request left (`captureSnapshot` before the await, discarded if `forgetSynced` ran meanwhile); before, an overlapping save's snapshot included an edit that a failing second save never delivered.
- C12 `removeBundleLive` and `spendRecoveryCodeLive` look the entries up again after the confirm dialog.
- C15 locking closes the edit form (and its draft); `saveModal` refuses when the vault is locked.

By design (reasons in `the session records/chaos-candidates.md`): S11 and C7 (deletion wins over a concurrent chunk add; order is not contradicted data), R14 (the epoch check makes an in-flight request ordered before the revocation).

## Verification (2026-10-10)

## For the next agent

Always `env -u UNV_SERVER_URL -u UNV_PASSWORD` before running `unv`. Never `pkill -f`. A new concurrency bug follows the same loop: failing test in the chaos files, fix at the shared function, mutation check.

> 1 line(s) about the maintainer's own machines were left out of this page.
