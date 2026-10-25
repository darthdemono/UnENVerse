# Phase log 76: Roadmap R09, password-manager sources

Phase 53 adds durable Bitwarden, 1Password and KeePass source migrations, attachment archives, a CLI-only desktop boundary, fuzz coverage, and a desktop reload regression repair.

## R09 password-manager sources

## Goal

Complete roadmap R09, project phase 53: migrate Bitwarden, 1Password and KeePass/KeePassXC exports through the R08 encrypted source-migration pipeline. The result must preserve stable source identity, attachment bytes, source history and unsupported records; refuse encrypted Bitwarden exports; keep the desktop surface honest where the renderer cannot make the required atomic vault-plus-migration write; and make every format-derived claim visibly unverified until a lawful sanitized real export proves it.

## Current Progress

Complete, uncommitted work from `801f09b`.

`unv-app/src/password_sources.rs` is the new shared reader and reconciliation module. It plans and writes migrations through `Access::save_migration`, so the vault document, encrypted manifest and encrypted archives remain one SQLCipher transaction. Source hashes cover the raw source record plus archived attachment content, not merely the fields that map to a vault entry. An attachment-only change therefore reaches reconciliation rather than appearing unchanged. Updates retain their original `created_at` value.

The Bitwarden reader accepts plaintext JSON and individual-vault export ZIP files without extracting them to disk. It limits each ZIP entry to 64 MiB and the archive to 256 MiB, imports item types 1 through 8, preserves URLs, folders, TOTP, history and custom fields, and archives unknown item shapes or source-only information with a reason. The error for an encrypted Bitwarden export names the ADR-0093 refusal rather than falling back to a lossy parse. Organization scope, trash and Sends remain reports because Bitwarden omits them from this export route.

The 1Password reader accepts JSON and `.1pux` ZIP exports, walks account and vault structures, retains each raw item for its source hash, maps supported fields and OTP values, and archives documents and file bytes. Passkeys are reported as missing at source instead of being represented as ordinary passwords. The implementation is format-derived and says so in the CLI, desktop copy, README and changelog because no sanitized production export was available.

The KeePass reader uses `keepass` 0.15 with in-memory password prompting and an optional key file. It recognizes the KeePass, KeePassXC and KDBX aliases, uses UUIDs as source identity, maps group structure into categories and optional folder tags, retains history as `version_history`, archives attachments as vault blobs, and preserves legacy TOTP custom fields. The password is never accepted as a command-line argument. A bad unlock attempt reports a secret-free failure.

`unv-cli/src/main.rs` routes all three durable formats to the shared source migration. `--force` is rejected for these sources because bypassing reconciliation would defeat durable identity and conflict handling. `--key-file` and `--no-password` are accepted for KeePass only and fail clearly for other source kinds. The historic Proton and Nextcloud paths retain their existing importer.

`src/ts/tools-markup.ts` and `src/ts/vendor-import-pane.ts` replace the old lossy password-manager quick importer with clear disabled controls and the exact CLI route. `tests/vendor-import-pane.test.ts` proves that boundary in browser and Tauri render paths. `tests/fixtures/parity/capabilities.json` records the temporary desktop exemption: password-manager migration requires an atomic native transaction and a native password prompt, so a renderer-only import would be dishonest.

The required real desktop viewer found a pre-existing reload failure while rendering that disabled UI. When a renderer reloaded after unlock, it derived the same vault key and called `unlock_vault` a second time. Rust then tried to create a second operations executor and failed on `.unenverse-ops.lock`. `src-tauri/src/lib.rs` now holds the operation-state lock before vault state, detects the existing unlocked executor, zeroizes the newly supplied key and returns success. The regression test `a_renderer_reload_reuses_the_existing_operations_executor` proves the repaired path. This is adjacent production work, not cosmetic viewer handling.

`fuzz/fuzz_targets/password_source_json_zip.rs` adds the `password_source_json_zip` libFuzzer target. Its deterministic 512-case JSON/ZIP smoke corpus uses the same entry point in the `unv-app` unit suite. The machine has stable Rust only and no `cargo-fuzz` command, so the libFuzzer runtime campaign could not start locally; the target is compile-checked and the deterministic malformed-input path ran in the normal suite.

`README.md`, `CHANGELOG.md` and ADR-0093 now describe the supported source formats, encrypted-export boundary and format-derived verification limit. `unv-app/Cargo.toml` and the root lock file add `keepass`, `zip` and `chrono` for the reader implementation.

## Verification

- `cargo clippy --workspace --all-targets -- -D warnings` passed after the reader's ZIP return type was named and the needless 1Password walker lifetime was removed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo test --workspace --quiet` passed after the final reader and desktop changes. The all-in-one final gate moved from the Rust suite into the frontend check, which confirms its `&&` predecessor succeeded.
- `cargo test -p unv-app password_sources --lib` passed 9 tests, including exact attachment bytes, attachment-only source change detection, source timestamp retention, corrupt ZIP handling, wrong KeePass password handling, item-count conservation and the deterministic JSON/ZIP malformed-input smoke cases.
- `cargo test -p unenverse --lib` passed 5 tests, including the renderer-reload executor regression.
- `cargo test -p unv-cli --test capabilities` passed.
- `cargo check --manifest-path fuzz/Cargo.toml --bin password_source_json_zip` passed after the final change.
- Final `npm run check` passed with Prettier, ESLint and TypeScript clean, followed by 92 Vitest files and 1,563 tests. jsdom printed its existing navigation notices only.
- The real Docker Tauri viewer completed after the executor repair. `shots/r09-viewer-fixed-A3ytkm/report.json` reported `1 screens, 0 with page errors`; the inspected Tools import/export image showed the clear CLI-only guidance and disabled password-manager controls.
- The viewer initially failed twice with `Operations executor is already held by another UnENVerse process`. The first run also had a malformed temporary screenshot path. Both failed runs were stopped, the root cause was fixed and the final run passed. The three screenshot directories created for these attempts were removed after verification; `shots/native` is deliberately untouched because it was not proven to be this session's artifact.

## The bugs that mattered

1. **High: renderer reload locked a user out of the already unlocked desktop app.** A page reload could call `unlock_vault` with an already-live executor, acquire a competing `.unenverse-ops.lock` and leave the user at an error screen. The new locked helper reuses the existing executor and zeroizes the redundant derived key. The real viewer reproduced the fault and the lib regression test now fixes its behavior.
2. **High: an attachment-only source edit was invisible.** Hashing just mapped JSON meant a changed file attachment could be skipped, leaving stale encrypted archive bytes while the user believed a refresh had occurred. The hash now includes raw source and archive content, with a regression test.
3. **Medium: re-importing an updated record changed its apparent creation time.** Users would lose the original chronology of a migrated secret even though the source identity was stable. Reconciliation now preserves the existing `created_at` on updates, with a regression test.
4. **Medium: the old desktop quick importer invited a false full-import claim.** It could not atomically preserve source identity, attachments, archive evidence or key-prompt semantics. The controls are now disabled with the exact CLI instruction, and the capability map records the exemption.
5. **Medium: malformed ZIPs and incorrect KeePass unlocks could have leaked parser detail or left partial migration state.** ZIP parsing is bounded and returns clean errors before planning; KeePass failures return a generic secret-free error. Both cases are covered by focused tests.

## Decisions made

- Use the R08 migration ledger and encrypted archive storage rather than extending the historic quick importer. Stable reconciliation, encrypted provenance and atomic writes are requirements for a password-manager migration.
- Treat raw source records and attachment metadata as hash inputs. Mapping only known credentials would make unsupported-field and attachment changes invisible.
- Keep desktop password-manager migration CLI-only until a native IPC command can receive the source path, prompt for KeePass credentials and save the vault and migration data atomically. The disabled renderer is an honest boundary, not a missing button.
- Refuse Bitwarden encrypted exports under ADR-0093. Accepting a partially understood encrypted container would create an unverifiable migration path.
- Label 1Password and unsupported export surfaces as format-derived and unverified. No export was invented or claimed as observed without a sanitized real fixture.
- Add a compile-checked libFuzzer target and deterministic smoke fuzzing now. Do not install or switch the maintainer's global toolchain merely to obtain nightly sanitizer support.

## What Worked / What Didn't Work

The R08 migration transaction and source reconciliation gave the new readers one place to save identities, conflicts and archive bytes. The capability map and the required real viewer prevented a misleading desktop claim. The viewer also exposed an unrelated native reload defect before it could become a release regression.

The first viewer execution had a malformed temporary directory setup, then the retry hit the executor bug. Neither failure was called a pass. The final run used a dedicated temporary screenshot directory and completed with zero page errors.

No lawful sanitized real export was available for KeePass, Bitwarden or 1Password. That limits interoperability evidence, not parser safety coverage. Documentation and UI copy state the boundary plainly. Runtime libFuzzer could not run because `cargo-fuzz` is absent and only stable Rust is installed; the target compiles and deterministic malformed JSON/ZIP cases run under `cargo test`.

`cargo clippy -D warnings` found a complex ZIP return type and a needless lifetime after the first implementation pass. Both were corrected before final workspace and frontend gates. The workspace continues to emit Cargo's pre-existing future-incompatibility warning for `proc-macro-error2 v2.0.1`; it is unrelated to R09 and does not fail the build.

## Next Steps

1. Begin R10, project phase 54, Git sources and cutover. Reuse the R08/R09 migration identity, conflict and encrypted archive protocol rather than inventing another import ledger.
2. If a maintainer can provide lawful sanitized exports, add fixture-backed reader coverage for current Bitwarden JSON/ZIP, 1Password `.1pux` and KeePass KDBX3/KDBX4 variants. Keep every resulting test fixture secret-free and do not weaken the unverified labels until the evidence exists.
3. Run the R08 and R09 libFuzzer targets in an isolated nightly-capable CI environment after toolchain policy permits it. This needs CI or maintainer authority; no local toolchain change is required.
4. Replace the desktop exemption only with a native command that proves one atomic write across the vault, manifest and archives while handling KeePass secrets outside the renderer.

## Notes for whoever picks this up

- `password_sources.rs` is intentionally the only source-reader surface. Do not recreate format handling in TypeScript or add a lossy importer beside it.
- Source archives, attachment bytes and migration manifests are encrypted in the vault database. Do not add plaintext scratch copies, source caches or log source values.
- The `source` value on planned items exists to make unknown source-field changes observable. It is not a renderer payload and must stay out of CLI, IPC and report output.
- `--force` is deliberately unavailable for R09 source migration. A future conflict-resolution UI or CLI path must name the conflict and retain the source archive; it must not silently replace a locally changed secret.
- Keep the special `docker_service` `${VAR}` exception in the regular config migration path. Password-manager readers do not change that R08 rule.

> 1 line(s) about the maintainer's own machines were left out of this page.
