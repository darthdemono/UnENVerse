# Phase log 70: Roadmap R03, dependency modernisation

## Goal

Complete roadmap R03 (project phase 47): replace the exact-pin-everything policy with caret requirements plus the root lockfile, modernise the listed Rust and npm families together, keep binary and cryptographic compatibility guarded by fixtures, and raise the cost of newly written encrypted backups without losing the ability to read existing ones.

## Current Progress

R03 is complete locally from base `3710d7170f8e…` at version `0.42.7`; nothing was staged, committed, pushed, tagged, published, or deployed.

- Added workspace Rust MSRV `1.90` and inherited it in all four member manifests. `1.90` is Tauri 2.12's published MSRV, rather than the roadmap's minimum suggestion of 1.89.
- Direct constraints are caret requirements except the two data-format-sensitive pins: `rusqlite = 0.40.2` and `argon2 = 0.6.0`. The root `Cargo.lock` records the resolved graph.
- Removed the obsolete tracked `src-tauri/Cargo.lock`. `cargo metadata --format-version 1 --no-deps` had SHA-256 `3dfb2718fd55…` before and after removal, so it never selected that member-local lockfile.
- Updated the requested compatible crates, including zeroize 1.9.1, webpki-roots 1.0.9, Tauri 2.12.2 with build 2.7.2 and shell/window-state plugins, RustCrypto's current API family, Argon2 0.6.0, rand 0.10.3, rcgen 0.14.10, reqwest 0.13.5, toml 1.1.8, dirs 7.0.0, notify 8.2.0, tower-http 0.7.1 and utoipa 6.0.0. Transitive older versions retained by upstream dependencies remain normal Cargo resolution, not direct project requirements.
- Removed the unused global-shortcut plugin registration and its dependency. There was no matching capability entry. There was also no direct `@tauri-apps/api` import, so no unused npm API package was added.
- Moved the frontend from `js-yaml` plus its typings to `yaml` 2.9.1 and TypeScript 6.0.3. The current import/export and formatting paths parse/stringify data and do not need comment preservation; R08's future template mode can make a separate preservation decision.
- Repaired the family API migrations: Rand 0.10 uses `rand::rng`, HMAC imports `KeyInit`, SHA digest formatting uses `hex::encode`, Argon2 0.6 uses its password-hashing API, AES-GCM nonce construction validates length with `try_from`, and the `ssh-key` 0.6 bridge implements that crate's older rand-core trait against the project's entropy source.
- Changed newly generated `.vaultbak` and `.vaultarc` headers to PBKDF2-SHA256 with 600,000 iterations. Import still uses the iteration count in the envelope, so the 0.42.x fixtures at 210,000 iterations remain readable by both Rust and Web Crypto.
- Added a byte-for-byte reviewed OpenAPI snapshot at `unv-server/tests/fixtures/openapi.json`. Its SHA-256 matches the pre-upgrade canonical document: `6658b21cc080…`.
- Added tests for the old desktop backup fixture, fresh CLI backup header cost, Utoipa document snapshot, and `dirs` platform data-directory semantics. Existing Argon2, node-signature, audit/history chain and tampered-request tests remain the compatibility coverage.

Verification completed:

- `cargo test --workspace --locked` passed after lock generation. It includes 459 vault-core unit tests, 92 server unit tests, CLI backup restoration, node signature/tamper rejection, the Argon2 golden vector, and all integration/doc-test targets.
- `npm run check` passed: 91 test files and 1,561 tests; format, ESLint, TypeScript 6 typecheck and version consistency were clean.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo deny check` passed advisories, bans, licences and sources. It printed only existing duplicate-transitive-crate and stale-ignore warnings while returning zero.
- `npm audit --omit=dev` and `npm audit` both reported `found 0 vulnerabilities`.
- `npm run preflight` passed end to end, including a second full Rust suite, the frontend check, and Vite build. Vite retained its existing dynamic-import and chunk-size warnings.

GitHub's Linux/Windows matrix cannot be run locally without a push, and an interactive native Tauri window was not opened. The upgraded desktop crate compiled and linked during both workspace test and preflight.

## The bugs that mattered

1. TypeScript 6 strengthened Web Crypto's typed-array declaration. The generic entropy helper previously allowed a `SharedArrayBuffer`-backed view that `crypto.getRandomValues` does not accept. `fillRandom` now admits only `ArrayBuffer`-backed `Uint8Array` or `Uint32Array`, preserving its actual browser contract instead of casting away the error.
2. `yaml.parse` returns `any`. Two formatter paths inherited that unsafe value and ESLint correctly refused the migration. They now contain it as `unknown` before serialisation.
3. The generated OpenAPI JSON must not be reformatted. Prettier initially rejected the new canonical snapshot, so `unv-server/tests/fixtures/` now follows the existing golden-fixture ignore policy. Formatting it would make the review assertion noisy rather than test a real document.
4. The RustCrypto and Rand upgrades changed APIs at compilation boundaries. The source updates preserve validation: malformed AES-GCM IVs are rejected before decryption, entropy remains the source for generated SSH keys, and digest output stays lowercase hex.

## Decisions made

- No new ADR: R03 implements already-recorded roadmap policy and does not change the backup envelope's fields or the public API. The only user-visible policy choice, 600,000 PBKDF2 iterations for new backups, was prescribed by the phase and pinned with compatibility tests.
- `rust-version = 1.90`, because this is the Tauri 2.12 MSRV verified during research. A lower declaration would promise a build the desktop dependency no longer supports.
- Keep `ssh-key` on 0.6: 0.7 was only a release candidate at verification time. Its older rand-core traits require the tiny local bridge; jumping to an RC would create a larger compatibility risk than the necessary adapter.
- Do not add `@tauri-apps/api`: package parity means matching versions for packages in use, not creating an unused runtime dependency.
- Keep the old `kdf.iters` reader behaviour exactly. New files harden, old headers declare their historical cost, and guessing a current default for an old envelope would make valid backups unreadable.

## What Worked / What Didn't Work

- Cargo's metadata output gave a definitive, cheap proof that deleting `src-tauri/Cargo.lock` cannot alter workspace resolution.
- The pre-upgrade OpenAPI server snapshot and Utoipa 6 test showed no document drift, so the fixture remains the exact pre-upgrade JSON rather than an accepted schema rewrite.
- `cargo generate-lockfile --offline` was needed after removing an unnecessary Argon2 feature because `--locked` correctly refuses any lockfile change.
- The first frontend check caught fixture formatting and the second caught the two unsafe YAML assignments; both were fixed before the final preflight.

## Next Steps

- The next roadmap item is R04, `unv-app` extraction.
- The maintainer must push the completed changes for the GitHub Linux and Windows matrix. No local action can reproduce the hosted runner matrix exactly.
- Commit the changed tracked files using the explicit per-file command provided with the completion report. `the session records/70-roadmap-r03-dependency-modernisation.md` itself is ignored local session history.

## Notes for whoever picks this up

- Do not reintroduce `src-tauri/Cargo.lock`; the root lockfile is the sole workspace lockfile.
- Keep exact pins only for `rusqlite` and `argon2`. Any new exact pin needs an on-disk/derived-key rationale and a golden test.
- The OpenAPI fixture is canonical generated output. Do not run a formatter over it; update it only after reviewing an intentional semantic specification diff.
- The 600,000 iteration setting applies to new backups only. Both Rust and TypeScript importers must continue to honour `kdf.iters` from existing envelopes.

> 1 line(s) about the maintainer's own machines were left out of this page.
