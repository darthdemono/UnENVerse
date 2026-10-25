# Phase log 66: roadmap R01 (project phase 43), security and storage-engine hotfixes

Date 2026-10-10. Start commit `ffe3c2b` (0.42.4), tree had only a pre-existing `.gitignore` change. Toolchain: rustc 1.99.0, node v22.22.2. Nothing committed or staged (maintainer rule); every change below is in the working tree. Version unchanged (0.42.4): R01 is not a version bump.

## Goal

Execute `the session records/roadmap/phases/01_security_hotfixes.md`: seven items (rusqlite 0.31 to 0.40, three advisory pins, explicit pragmas, Argon2 off the async runtime plus username timing, vendor TOTP import, Docker on bundled SQLCipher, vite/vitest majors). Mapping R01 to 43 written in STATUS.md and PHASES.md (next free phase is now 44).

## Re-verified research claims (code vs plan)

- `rusqlite =0.31.0` in `vault-core/Cargo.toml` twice (Unix `sqlcipher` feature, Windows `bundled-sqlcipher-vendored-openssl`): true. **New fact the plan did not state:** on Unix dev builds the `sqlcipher` feature links the SYSTEM SQLCipher (here `libsqlcipher-3.39.2`), so the rusqlite upgrade alone does not change the engine on a developer's Linux box. Only `--features bundled` (releases, Windows, now Docker) carries SQLite 3.51.3 / SQLCipher 4.14.0. The version-floor test is therefore conditional (see below).
- `rand =0.8.5`, `time =0.3.37` (vault-core, unv-cli, unv-server), `rustls-pemfile` (vault-core optional, unv-server): true. pemfile call sites: `vault-core/src/tls.rs` (2) and `unv-server/src/lib.rs` `fingerprint_of_cert_pem` (1).
- `open_db` set no busy timeout or journal limit: true (rusqlite's implicit 5 s).
- `derive_key` inline in `unlock` handler and `verify_user_password` inline in `auth_user`: true. Unknown username returned before any Argon2 (`row` None), and a password-less user (`hash_opt` None) also returned early: **two** oracle branches, the plan named one.
- `import_vaults.rs` wrote TOTP into notes ("there is no TOTP support here"): true and stale, since Phase 22.
- Dockerfile used `libsqlcipher-dev` / `libsqlcipher0`: true.
- `vite ^5.4`, `vitest/coverage-v8 ^4.1.10`: true; the lock already had a second vite 8.2.1 under vitest. `npm audit` before: 7 vulnerabilities (4 moderate, 3 high).
- `glib` RUSTSEC-2024-0429: `cargo deny check` did NOT report it, so no ignore added (plan said add only if reported).

## Changes, file by file

- `vault-core/Cargo.toml`: rusqlite `=0.40.2` (both entries; resolves libsqlite3-sys 0.38.2), `rand =0.8.6`, `time 0.3.55` (caret, features parsing), `tls` feature no longer lists `dep:rustls-pemfile`, dependency line removed.
- `unv-cli/Cargo.toml`: `rand =0.8.6`, `time 0.3.55`. `unv-server/Cargo.toml`: `time 0.3.55`, `rustls-pemfile` replaced by `rustls-pki-types = "1"`.
- `Cargo.lock` updated (rand 0.8.6, time 0.3.55, time-core 0.1.9, time-macros 0.2.32, rusqlite 0.40.2). `src-tauri/Cargo.lock` was stale; see the follow-up pass below.
- `vault-core/src/uid_registry.rs`: rusqlite 0.40 has no `FromSql for u64`; the COUNT is read as `i64` and cast. Only compile break in the whole upgrade.
- `vault-core/src/lib.rs` `open_db`: `conn.busy_timeout(5 s)` and `PRAGMA journal_size_limit=67108864` after WAL setup.
- `vault-core/src/tls.rs`: `PrivateKeyDer::from_pem_slice` / `CertificateDer::pem_slice_iter` (`rustls_pki_types::pem::PemObject`). Error strings kept: "holds no private key" for `NoItemsFound`, "Cannot read CA file" for bad certs. New test `tls_identity_load_refuses_bad_keys_and_certificates` (empty key, cert-as-key, corrupt key body, corrupt cert body, mismatched key).
- `unv-server/src/lib.rs`: `fingerprint_of_cert_pem` reads the file and uses `pem_slice_iter`; `KDF_PERMITS = 2`; `AppState.kdf_gate: Arc<tokio::sync::Semaphore>`; `async fn run_kdf(state, f)` = acquire owned permit, `spawn_blocking`, re-raise panics. `unlock` derives via `run_kdf`; `auth_user` moves the `Connection` into the closure for `verify_user_password` and gets it back (`(conn, result)`); token auth stays inline (SHA-256 only). Test `wrong_password_logins_do_not_stall_health`.
- `vault-core/src/users.rs`: `spend_verify_time(password)` verifies against a lazily hashed dummy (made with `hash_password`, so cost params match real users) on both early-return branches; test hook is a `thread_local!` counter under `cfg(test)`. Tests `unknown_and_passwordless_usernames_pay_for_an_argon2_run` and ignored `unknown_username_timing`.
- `unv-cli/src/import_vaults.rs` `merge`: `vault_core::totp::parse_seed(t)` then `vault_core::totp_import::write_fields(&mut e, &stored)`; unparsable value goes to notes ("TOTP secret imported (not a usable seed): ..."). Only the create branch; the update branch never carried TOTP (unchanged, see Not done).
- `Dockerfile`: builder drops `libsqlcipher-dev`; both `cargo build` lines add `--features vault-core/bundled`; runtime drops `libsqlcipher0`.
- `deny.toml`: removed the pemfile, rand and time ignores and their comments. Remaining ignores untouched.
- `package.json` / `package-lock.json`: vite `^7.3.7`, vitest `^5.0.3`, `@vitest/coverage-v8 ^5.0.3`; then `npm audit fix` (non-breaking: brace-expansion, source-map-js). Lock version lines 3 and 9 still 0.42.4.
- New tests/fixtures: `vault-core/tests/golden_vault.rs` + `vault-core/tests/fixtures/vault-0.42.x/{vault.db,vault.salt}`; `unv-cli/tests/golden_backups.rs` + `unv-cli/tests/fixtures/backups-0.42.x/{golden.vaultbak,golden.vaultarc}`. `cargo fmt --all` was run.

## Verification (what ran)

- Golden tests written and run on the OLD code (rusqlite 0.31) first: 5 passed. Fixtures generated by ignored `generate` tests on that code (`cargo test -p vault-core --test golden_vault -- --ignored generate`, same for unv-cli). Argon2 vector for password `golden-fixture-password` + salt `unv-golden-salt!`: `5b214207fcc8…`.
- After the upgrade, system SQLCipher: golden_vault 7 passed, golden_backups 1 passed.
- `cargo test -p vault-core --features bundled --test golden_vault -- --nocapture`: `sqlite 3.51.3, sqlcipher 4.14.0 community`, all passed. The 4.5-era fixture opened with no `cipher_migrate` (plan step 3: no migration needed). The wrong-password test logs SQLCipher "hmac check failed" lines: expected.
- Controls: wrong password gives "Wrong master password"; a copy with 64 bytes flipped is noticed by `cipher_integrity_check`/`integrity_check`/open.
- Mutation checks (each reverted afterwards, diffed clean): dummy verify removed from the unknown-user branch gives `unknown username skipped the dummy verify`; `run_kdf` forced inline gives `/api/health stalled for 9.5s`; TOTP import forced to the old path fails 2 of 3 new import tests.
- Health latency (debug build, 2 worker threads, 10 concurrent wrong-password logins each from its own IP): `/api/unlock` batch 12.1 s total, worst health 13 ms; `/api/auth` batch 3.9 s, worst health 5 ms; bound 200 ms. Inline mutant: 9.5 s.
- Timing (release, 50 attempts each): known 59.6 ms, unknown 61.3 ms, ratio 1.028 (within the plan's 10%). Recorded, not gated (wall clock); the gate is the call-count test.
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok, with the three ignores removed.
- `npm run preflight` exit 0: fmt, clippy, `cargo test --workspace` (all result lines ok; vault-core lib 442 passed 2 ignored), `npm run check` (tsc, eslint, prettier, Vitest 88 files / 1501 tests, same count as the R00 baseline), vite build. `npm audit`: 0 vulnerabilities.
- Docker: `docker build` of the changed Dockerfile succeeded under a throwaway tag; a container with a scratch volume (own name, port 18743, then removed, image removed) answered `/api/health`, created a vault on `/api/unlock`, and had no system libsqlcipher. Image 147 MB. The maintainer's running containers were not touched.

## Decisions

- Version floor test conditional on `cfg(any(feature = "bundled", windows))`, always recording the version; rejected asserting unconditionally because Unix dev builds link the OS library and the test would fail for a reason unrelated to the code.
- `time` and `rand`: followed the plan (`time` caret, `rand` exact pin per the "exact pin at the new version" spirit for format-adjacent crates).
- Dummy verification lives in vault-core (`verify_user_password`), not the server, so every caller (server, CLI) closes the oracle; cost: both early branches now spend an Argon2 run.
- Unparsable vendor TOTP kept in notes rather than dropped (data loss over tidiness).
- Health test spawns the probe onto the runtime: the first version called the handler from the test body (main thread, outside the 2 workers) and the inline mutant still passed. That test could not fail; found only by running the mutant.
- Vite 7 not 8, vitest 5 (plan). Vitest 5 has peers vite `^6.4 || ^7 || ^8`, node `^22.12`; install vitest and coverage-v8 in ONE npm command (separately npm reports ERESOLVE).

## Follow-up pass (same day, after "always finish")

Everything first listed as "not done" that I could do was done:

- `create_user_handler` and `set_password_handler` now hash through `run_kdf` too. New test `creating_users_does_not_stall_health` (10 concurrent creations, worst health 48 ms in a debug build); mutation (`run_kdf` inline) fails it at 3.99 s.
- Import into an EXISTING entry now adds a missing TOTP seed (`parse_seed` + `write_fields`), never replaces a different one (integrations rule), and an unchanged re-import still changes nothing. Test `reimporting_adds_a_missing_seed_but_never_replaces_a_different_one`.
- `src-tauri/Cargo.lock`: confirmed dead (cargo metadata from `src-tauri` reports the repo root as workspace root, so the root lock governs). Overwritten with the root `Cargo.lock` so the tracked file no longer disagrees (rusqlite 0.40.2 / rand 0.8.6 / time 0.3.55); `cargo metadata --locked` passes. Not deleted.
- `npm run ui-lab`: 194 passed (4.1 m) on vite 7.
- `cargo test --workspace --features vault-core/bundled`: exit 0, 34 result lines all ok, so the whole suite passes on SQLCipher 4.14.0 / SQLite 3.51.3, not only the golden tests.
- After the last edits: `cargo fmt --check` clean, `npm run lint:rust` (clippy, deny warnings) clean, `cargo deny check` ok.

## What did not run, and why it cannot

Only GitHub CI (Linux and Windows runners): it needs a push, which is the maintainer's. The Windows branch of the bundled-version test is therefore unproven on Windows itself. The `unknown_username_timing` test is a manual measurement by design.

## Next

R02 (CI release gating and speed) per STATUS, or the website lane W1. Exit criteria 1 to 6 of R01 are met locally; commit one commit per item as the phase file asks (pwsh one-liner handed to the maintainer separately).
