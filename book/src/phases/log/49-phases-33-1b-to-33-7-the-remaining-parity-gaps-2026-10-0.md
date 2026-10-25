# Phase log 49: Phases 33.1b to 33.7: the remaining parity gaps (2026-10-08)

## Goal

Close what phase log 48 left in the capability map: online enrich, doctor's file checks, vendor import and archive UI, strict-write and entropy sources in the app, `envv diff` and the template library in the CLI, and the dead Tauri commands.

## 33.7 - dead commands

Deleted `get_expiring`, `parse_totp_seed`, `totp_import_parse`, `load_settings`, `save_settings` (and the now-unused `settings_path`), and their `generate_handler!` lines (`get_expiring` was registered twice). `tests/ipc-contract.test.ts` no longer lists the two TOTP ones. `entropy_sources` stayed because 33.4 wires it. The capability `unused-ipc` is gone; the gate's reverse rule (a command in `unused-ipc` that becomes used must move) has nothing left to guard until a command goes dead again, at which point re-add the capability.

## 33.6 - diff and templates in the CLI

- `secret-templates.json` (repo root) generated from the TypeScript array with esbuild + node (not retyped); `src/ts/templates.ts` now imports it; `vault-core/src/templates.rs` (`list`, `find`) includes it. One copy, so no parity fixture.
- `envv template ls|show ID` (`template_cmd.rs`, early-dispatched, no password) and `envv entry add NAME --preset ID`: defaults are merged into the new entry, the entry keeps the name given, explicit flags override. The flag is `--preset`, not `--template`: `--template` already is a composite's `{part}` template (clap panics at startup on the duplicate id; found by running it).
- `envv diff A B` (`diff_cmd.rs`): same 14 fields and order as the Diff pane; `api_key`/`api_secret` are fingerprints unless `--reveal`; two different entries required. Test: `envv-cli/tests/diff_template.rs`.

## 33.4 - strict-write and entropy

- **strict-write**: `PUT /api/users/{id}/strict-write` and `/api/classes/{id}/strict-write`, owner only in both directions (turning it off widens writes), 404 for an unknown subject; Tauri `set_strict_write`; `users.ts` dispatcher maps it to REST for remote; checkbox in user and class detail (`strictToggleHtml`, `wireStrictToggle`). The CLI's `user strict-write` is still local-database only (it cannot target a remote); the app can now. Tests: the existing `user_and_class_routes_over_http_preserve_rbac_boundaries` gained four assertions; `tests/users.test.ts` +1.
- **entropy**: Tauri `entropy_fill(source, length)` (hex, mixed with OS entropy exactly as the CLI via `entropy::fill`, 1..8192 bytes); `generators.ts` has `setEntropySource`, an 8 KiB pool refilled in the background, `fillRandom` (OS CSPRNG when source is `os`), `EntropyPoolEmpty` and `guardEntropy`. An empty pool is a toast; it never falls back to the OS because that would claim a hardware source that was not used. New setting `entropySource` (default `os`), Settings -> Randomness populated from `entropy_sources`; `Settings._apply` calls `setEntropySource` only when the value changed. Call sites wrapped: Tools secret/password/API-key panes and the form popover. `tests/entropy-source.test.ts`. Not done: bulk `uid register` in the unique-ID pane.

## 33.3 - import-vault and archive in the app

- `import_vaults::plan_import` extracted from `run` (pure; returns entries, counts, fingerprints); Tauri `import_vault_plan`; `src/ts/vendor-import-pane.ts` (markup under the Import pane): choose vendor and file, preview with counts, Import replaces `api_keys` with the plan's entries and saves once. `tests/vendor-import-pane.test.ts` (faked file picker by spying `HTMLInputElement.prototype.click`).
- `backup::build_archive` / `open_archive` extracted (pure, shared); Tauri `backup_archive_build` (returns text; the pane saves it with `saveFile`, 0600) and `backup_archive_restore` (verifies first, `lan_stop`, zeroizes the key, writes the salt before the database, removes `-wal`/`-shm`, then the pane reloads the page). Round-trip/tamper/wrong-password unit test `archive_tests`. `tests/archive-rows.test.ts`.
- **Caveat for the next agent:** an archive is a byte copy of `vault.db`. The vault is WAL; connections are opened and closed per command so the file is normally checkpointed, but an archive taken while another process holds a connection (LAN server, a running `envv`) can miss pages. The CLI has always had this; the Settings hint tells the user to lock other tools first. A proper fix is a `VACUUM INTO` copy under the key, which would also drop the "needs the key" distinction between CLI and app.
- A rescue slip: my first splice of `backup.rs` duplicated functions (a `str.index` for the write call matched `export()` before `archive()`); it was restored from the index and redone. If a `git status` shows `backup.rs` modified by someone else, check `git diff` before assuming.

## 33.2b and 33.1b

- `doctor::file_findings(&Access)` (integrity, storage hashes, salt pairing, permissions, audit); Tauri `doctor_file` builds `Access::Local(key)` from `VaultState` after `set_paths(db, salt)` (a OnceLock: the first call wins for the process). The Diagnose button runs it only when the store is not remote; remote says the rest is the server's. `--fix` is not in the app.
- `enrich::issuer_for(entry)` (cookies excluded), Tauri `enrich_online_targets` (no request) and `enrich_online` (blocking probes in `spawn_blocking`). The pane: checkbox -> `showConfirm` listing `Issuer (count)` and what the issuer will see -> only then `enrich_online` on exactly the targeted entries. Live answers replace inferred proposals for the same field; a rejected key shows `Issuer: rejected` with its detail. Declining sends nothing (tested), but the final status line is overwritten by the "N entries with proposals" text; cosmetic.

## Verification

`npm run check` green (76 files / 1388 tests); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` all ok; `cargo test -p envv-cli --test capabilities` ok after every map change. Not run: ui-lab; any of the new panes or the archive/restore in a real Tauri window; fault injection on the new pane tests (only the capability gate and the earlier phases were fault-injected).

## Not done

Bulk `uid register` in the pane; `--fix` in the app's doctor; strict-write on the CLI against a remote; the archive/WAL caveat above.
