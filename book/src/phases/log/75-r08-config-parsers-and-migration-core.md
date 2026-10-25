# Phase log 75: R08 config parsers and migration core

## Goal

Complete roadmap R08, project phase 52: add a lossless, local config-migration
pipeline for `.env`, Compose, WireGuard and nginx sources, with durable source
identity, encrypted manifests and archives, template mode, CLI and desktop
entry points, and parser fuzz targets. The desktop work expanded from the
initial preview-only screen to an apply path when the capability check exposed
that the advertised two-surface feature was not yet true.

## Current Progress

Complete, uncommitted work. Schema migration 4 (`migration_core`) creates the
encrypted `migrations`, `migration_items`, `vault_blobs` and `source_archives`
tables. `save_vault_with_migration` writes the vault document, audit data,
migration manifest and archive rows in one SQLCipher transaction.

`unv-app/src/migrate.rs` owns the reader dispatch, redacted plan, three-way
reconciliation and apply flow. A source identity combines the source kind, a
hash of the source label, semantic field name and ordinal. Re-importing the
same source/value is a real no-op; a changed source updates only when the
mapped vault entry is unchanged; concurrent source and vault changes return a
named conflict. Counts remain disjoint, with conflicts included in `skipped`.

Template mode replaces each imported representation with a `${UNV_MIG_...}`
binding and converts existing literal `${` to `$${`. Its encrypted archive and
binding representation reproduce the exact imported bytes, including BOM,
CRLF, tabs, trailing spaces and comments. Non-template migrations retain the
complete original source in an encrypted archive, so comments and unsupported
constructs are never discarded. `unv migrate
archive ls|export` lists and writes those bytes; export is owner-only.

The CLI now provides `unv migrate plan|apply|verify|report|archive`. The
desktop Migrate Config tool plans and applies pasted local text through the
same Rust code. It returns only names, statuses and counts to the renderer,
disables Apply until a conflict-free plan is reviewed, and re-plans immediately
before writing. The capability map and IPC contract cover the two commands.

R08 adds a shared sanitized TypeScript/Rust reader fixture and four
`cargo-fuzz` targets: `migrate_env`, `migrate_compose`, `migrate_wireguard` and
`migrate_nginx`. ADR-0157, README, changelog and book summary document the
source-identity and archive decision.

## Verification

- `cargo test --workspace --locked` passed: the workspace, integration suites
  and doctests are green; the final `vault-core` unit run was 468 passed and 2
  intentionally ignored, and the golden-vault suite was 7 passed and 1 ignored.
- `npm run check` passed: 92 Vitest files and 1,564 tests. jsdom emitted its
  existing navigation notices only.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `npm run build:frontend` and `npm run docs` passed. Vite retained its existing
  dynamic-import and large-chunk warnings.
- `cargo check --manifest-path fuzz/Cargo.toml --bins` passed for all four fuzz
  targets.
- Focused migration checks passed: `cargo test -p unv-app migrate --lib`,
  `cargo test -p vault-core --test golden_vault`,
  `cargo test -p unv-cli --test capabilities`, and
  `npx vitest run tests/migrate-parsers.test.ts tests/ipc-contract.test.ts`.
- A throwaway local SQLCipher CLI vault proved apply, report, verify, exact
  repeat no-op, source-only update and dual-change conflict. JSON output was
  checked not to contain fixture values.
- `docker compose config` accepted a representative rendered Compose shape.
  `wg-quick strip` is installed but invokes `sudo`, so it could not run
  unattended; nginx is not installed. Both are recorded rather than claimed.
- A 60-second `cargo fuzz run migrate_env` was attempted after a local
  `cargo-fuzz` installation. Stable Rust rejected `-Zsanitizer=address`; this
  machine has no nightly toolchain. Targets compile, but runtime fuzz campaigns
  await a CI or local nightly environment.

## The bugs that mattered

1. A repeated unchanged apply originally wrote another migration manifest and
   archive, so it looked idempotent while growing encrypted state indefinitely.
   `apply` now returns before every write when no item is imported or updated;
   the CLI repeat/report check proves the migration count stays one.
2. The first `.env` line-ending trim could restore a newline into the value on
   an LF-only source. Exact template verification would then fail or import a
   different secret. The reader now trims CR and LF together and the byte-equality
   fixture covers BOM, CRLF, tabs, spaces and `$${`.
3. After the desktop gained a plan/apply path, its old standalone parse command
   was still claimed as UI evidence. The mechanical capability check caught the
   stale claim before release; the unused command was removed and the capability
   declaration now names only invoked bridges.
4. The first independent fuzz manifest accidentally joined the root workspace.
   Adding its own `[workspace]` lets `cargo check --manifest-path fuzz/Cargo.toml
--bins` build the targets without changing the product workspace.

## Decisions made

- Persist the document, manifest and archive in one SQLCipher transaction. A
  migration ledger that can outlive its corresponding secret update cannot
  reconcile safely on the next run.
- Identity is source kind plus a source-label hash and a field ordinal, not a
  display name alone. Two files with `TOKEN` therefore cannot overwrite each
  other. ADR-0157 records this.
- A template stores the exact source representation in the encrypted migration
  manifest so initial verification can prove byte equality. Binding metadata
  records the entry id. A future source exporter that needs to render rotated
  values must preserve each format's quoting and escaping rules; it must not
  replace this exactness proof with a lossy JSON value.
- The app accepts pasted local content rather than choosing arbitrary paths from
  the renderer. The CLI remains the path-reading and archive-export surface;
  both use the same Rust planner and writer.
- Keep the established TypeScript chunk parsers for existing config editing.
  R08 migration parsing is Rust-only and pinned against a shared sanitized
  fixture; do not retire the TypeScript path until all existing chunk behavior
  has equivalent Rust coverage.

## What Worked / What Didn't Work

The migration transaction, redacted IPC boundary, capability-map check and
throwaway SQLCipher exercise worked as intended. The capability map immediately
identified the stale desktop command claim, which prevented a false parity
record.

Runtime libFuzzer was not possible on the available stable-only toolchain. Do
not install or switch the maintainer's global Rust toolchain just to satisfy
this local smoke test; run each 60-second target under an isolated nightly CI
job or an explicitly approved local nightly environment.

## Next Steps

1. Start R09, project phase 53: KeePassXC, Bitwarden and 1Password readers on
   the R08 pipeline. Preserve original attachment and unsupported-item bytes in
   archives; do not silently flatten unsupported item types.
2. R10 may run in parallel after R08. Its external-tool adapters must reuse the
   R08 identity, authority and conflict protocol rather than defining a second
   reconciliation model.
3. Add nightly CI fuzz smoke runs for all four R08 targets when toolchain policy
   permits it. This needs maintainer or CI configuration authority.
4. Expand reader parity before retiring TypeScript config editing parsers. The
   current shared fixture covers the migration field boundary, not every chunk
   construct or exporter rule.

## Notes for whoever picks this up

- `migrations` and every archived source byte live inside SQLCipher. Never add
  a plaintext source cache or put a field value in CLI, IPC, UI status or report
  output.
- The current reader is intentionally conservative. Every non-template source
  is archived before parsing is considered complete; Compose, WireGuard and
  nginx content outside recognized migration fields must not be transformed or
  guessed.
- The working tree intentionally also contains uncommitted R06 and R07 work.
  Preserve it. Handoff records are gitignored local files.
- Generated `target/`, `fuzz/target/`, `fuzz/Cargo.lock` and the temporary
  `fuzz/.tools/` installation are removed at the end of this session. Rebuild
  targets locally or in CI when needed.
