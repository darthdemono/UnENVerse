# Phase log 74: R07 metadata and lifecycle core

## Goal

Complete roadmap R07, project phase 51: establish compatible credential
metadata, generation and lifecycle storage, stable references for rendered
configs, and an incremental config-history boundary for later rotation and
deployment phases.

## Current Progress

Complete, uncommitted work. `vault-core/src/lifecycle.rs` is new and migration
3, `metadata_lifecycle`, creates `credential_generations`,
`pending_credentials`, `lifecycle_events` and `deployment_state`. Existing
secret values seed generation 1 exactly once with `source = migration`,
`issued_at = NULL`, and `issued_at_precision = unknown`. New and changed values
create an append-only generation, supersede the prior active generation and
write created, imported, rotated or verified events.

Generation fingerprints are HMAC-SHA-256 over the field and value. A random
32-byte root is stored in SQLCipher `vault_meta` and expanded with HKDF, so the
same value has different fingerprints in different vaults. `deployment_state`
has an R12-facing query for records whose generation is no longer active.

`VaultEntry` now describes authority, provenance, issue time and precision,
attachment descriptors and plugin data. `formToEntry(base)` continues to
preserve them when a current form has no control for them. Redacted CLI output
keeps only documented, non-locating metadata and masks provenance locators,
attachment content and all plugin data.

`Project.bindings` maps a human reference name to an immutable entry id and an
optional field. Rust and TypeScript resolve one exact binding before legacy
provider-name resolution; absent bindings use the legacy path, while stale or
duplicate bindings are unresolved rather than guessed. Provider rename cascades
therefore leave a binding's `${NAME}` text alone while its rendered value stays
correct.

`unv entry timeline NAME`, the owner-only server endpoint, desktop IPC and a
card Timeline action surface only secret-free lifecycle facts. The capability
fixture declares the new CLI and IPC pair. ADR-0156, the README, changelog and
book summary document the model.

`unv-app/src/history.rs` persists non-secret project and entry row revisions in
encrypted `vault_meta`. After a save it renders only projects whose own row or
referenced entry rows changed. Bindings, direct references, bundles and chunk
references are analyzed conservatively. A render error does not advance the
revision cache, so the next save retries it.

## Verification

- `cargo test --workspace --locked`: passed. Relevant totals included 101
  `unv-app`, 467 `vault-core`, 92 `unv-server`, 18 node end-to-end, and the
  old-backup and old-vault golden suites.
- `cargo clippy --workspace --all-targets -- -D warnings` passed in preflight.
- `npm run check`: passed, 91 files and 1,563 Vitest tests.
- `npm run build:frontend`: passed. Existing Vite dynamic-import and chunk-size
  warnings remain non-fatal and predate this phase.
- `cargo test -p unv-cli --test capabilities`: passed.
- R07 focused tests cover cross-vault fingerprint separation, migration seeding
  with an unknown date, imported month precision, rotation supersession,
  deployment-generation mismatch, structured-metadata redaction, bound rename
  resolution, old vault migration and form preservation.
- The 50 project by 20 reference history fixture measured initial rendering at
  990.290318 ms and one-entry incremental rerendering at 180.610739 ms. The
  durable assertion is stronger than timing: after one changed entry it renders
  exactly one stream, not the other 49.

## The bugs that mattered

1. Calling recorded schema migration inside `load_vault` while
   `snapshot_vault` had already opened its read transaction caused SQLite to
   reject a nested `BEGIN IMMEDIATE`. `snapshot_vault` now ensures the schema
   before its snapshot, and `ensure_current_schema` does no transaction-opening
   migration inside an outer transaction.
2. A restored 0.42.x `.vaultarc` had no R07 tables when it first reached the
   public `load_vault` path. `ensure_current_schema` now applies ordered
   migrations on an autocommit connection. `golden_backups::old_backups_restore`
   proves the archive opens, migrates and imports again.
3. ESLint rejected the initial TypeScript binding fallback because a union value
   could have been stringified as `[object Object]`. It now accepts only a
   non-empty string and otherwise falls back to `api_key`.

## Decisions made

- Lifecycle metadata stays optional JSON on an entry. The side tables hold only
  relational lifecycle facts, preserving forward compatibility and old-form
  edits.
- Do not replace the HMAC root with a global hash key. The vault-local root is
  what prevents cross-vault secret correlation.
- Do not make a duplicate binding fall back to a provider lookup. That would
  silently retarget a deployed config after a rename.
- `snapshot_vault` must migrate before entering `storage::in_snapshot`; this is
  the transaction ordering that protects restored pre-R07 archives.

## Next Steps

R08 config parsers and migration core is next. R07 deliberately adds no
metadata or binding editor controls. Existing UI save paths preserve the
values; a future editing surface must also preserve unknown adapter data and
keep locator fields private. Keep all future lifecycle output on the
secret-free timeline path; a generation fingerprint is metadata, never a value
to use as an authentication token.
