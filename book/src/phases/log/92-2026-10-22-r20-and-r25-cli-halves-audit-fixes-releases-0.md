# Phase log 92: (2026-10-22): R20 and R25 CLI halves, audit fixes, releases 0.59.0 and 0.60.0

## Goal

Read the last two handoffs and keep the cycle going: add phases, commit, push, watch CI, release.
It changed twice: the maintainer ordered "release 0.59.0" and then "release 0.60.0" although the
`0.{completed phases}.0` rule did not allow it (R12 to R16, R19, R20, R25 are partial), and an
external audit (pasted summary, then `UnENVerse-review.zip`) turned the second release into a
security release.

## Current Progress

Merged to `main` (all by rebase merge after `CI result` was green, CodeQL clean):

- PR 58: backup key-derivation validation (`validate_kdf`: PBKDF2, SHA-256, 100,000 to 5,000,000 iterations; TS twin `backupIters`).
- PR 59: R20 core. `vault_core::rekey` (crash-safe master password change, recovery at the next unlock, ADR-0168), `unv vault rekey`, `vault_core::forget` and `unv entry forget` (forgotten config snapshots keep hash and chain), delete prompt wording, book pages `compromise.md` and `format-policy.md`. Also a TOTP ticker guard (`tickTotp` without a document).
- PR 60: R25 CLI half and extras. `unv support bundle`, `unv demo-vault` (password in `demo-password.txt`, never printed), `SUPPORT.md`, bug template, tested Compose guide (`book/src/compose-secrets.md`), generated `book/src/cli-reference.md` (stale-page test in `contract_surface.rs`), version 0.59.0.
- PR 61: audit fixes, version 0.60.0 (list in CHANGELOG `0.60.0`).
- PR 62: macOS flake (test directories named from the clock, now a counter in ten more files) and a retry of `releases/latest` in `scripts/build-site.mjs`.

Releases: v0.59.0 and v0.60.0 published (63 assets each, every job green). Docs site redeployed by `workflow_dispatch` after one transient 404.

Verification run locally, per crate with `CARGO_BUILD_JOBS=2 nice -n 10`: vault-core 553 lib tests, unv-app 173, unv-server 96 plus `nodes_e2e` 22 and the new `delegation`, `salt_pairing`; unv-cli contract tests for forget, support, compose, surface, capabilities, recipes; vitest for backup, totp, workflows. Not run locally: the full workspace, Windows, macOS (CI only), the bundled SQLCipher.

## The bugs that mattered

1. Node grant redirection (audit N1): a writer could add a project whose id equalled a granted name and the node got its content. Fixed by `resolve_grant` (ambiguous grant refused) in `unv-server/src/nodes.rs`.
2. Delegate takeover and widening (C1, C2): tier says who outranks whom, not what they can read. `guard_no_takeover` and `apply_within_own` (savepoint, undo if the target ends wider) in `unv-server/src/lib.rs`.
3. `unv exec` and `shield` handed `UNV_PASSWORD` to the child (F5). `exec::drop_credentials`.
4. Recovery package member names escaped the staging directory (D2). `is_safe_member_name`.
5. 5,000 nested parentheses crashed the server (C3). `permex::MAX_TOKENS` = 256.
6. `restore-archive` ignored a "no" (A2). `restore_archive_with` takes the question as a closure.
7. The server minted a salt for an existing vault (G2), and my own PR 59 read the salt before `recover_interrupted` in the unlock handler (fixed in PR 61).
8. TLS key and credential files were created world-readable then chmodded. `vault_core::write_private` now used at every site.
9. Test directories named from the clock collided on macOS ("database is locked").

## Decisions made

- Rekey and forget ship CLI-first, with `exempt` rows in `tests/fixtures/parity/capabilities.json`; desktop actions are a later slice.
- Forget keeps the current value (rotate or delete a live credential); forgotten snapshots keep `sha256` and chain so `verify` still works.
- Named-user passwords need 12 characters at the server (not in `vault_core::create_user`, which tests call with short ones).
- Left alone on purpose: unauthenticated `/api/stats` counters, first caller initialising an empty server, unsalted display fingerprints (F1) and URL userinfo in "public" fields (F2), non-recursive masking (F3), key residue after lock (B1), default `admin` seeding (S7). Recorded in CHANGELOG 0.60.0.
- The remote agent's branch `claude/gifted-davinci-xdi62e` was deleted from origin at the maintainer's order; its audit tests were not integrated. They are kept under `research/external-review-2026-10-22/audit-tests-from-deleted-branch/` (they fail on purpose for unfixed findings and are not rustfmt-clean).
- Releases 0.59.0 and 0.60.0 were made at the maintainer's explicit instruction, against the version rule.

## What Worked / What Didn't Work

Worked: one branch per change set from `origin/main`, stacking a second branch while the first CI ran (about 40 minutes), `git stash` of the fix to prove a regression test fails, runtime-built test passwords.
Didn't: CodeQL flags literal passwords, hard-coded salts and printed passwords even in tests (four rounds on PR 59 and 60); clippy `ptr_arg` after `write_private` made `&PathBuf` pointless; `Vault::sh` is Unix-only in the test harness (Windows compile failure on PR 60); whole-workspace test runs are slow here, so per crate.

## Next Steps

Agent work, in order:

1. Take `research/external-review-2026-10-22/extracted/UnENVerse-review/improvement-review/improvements.patch` on its own branch (use `git apply --3way`, exclude `SUPPORT.md`, which already exists). It touches workflows, so test with a dispatch build before relying on the image changes. First item: the README cosign identity pattern accepts any repository named `UnENVerse` (T1).
2. R20 desktop: Change master password (needs the app to re-derive and replace its key in `VaultState`), Forget action, revoke wording. R25: Settings, Help bundle button.
3. R12 remainder (embedded hub, deployment operation kind, leases), R18 desktop bridge, R17, then R22+ as in `the session records/roadmap/STATUS.md`.
4. The deferred audit lows if the maintainer wants them (S1 cache for `/api/stats`, F1, F2, F3, S7, B1 with a non-`Copy` zeroizing key type).

Human only: decide on the public `AGENTS.md`, moving signing secrets into environments, repository settings in the review's Appendix A, and whether the next release number follows the phase rule again.

## Notes for whoever picks this up

- `UNV_SERVER_URL` is set in the maintainer's shell: unset it before running `unv` by hand.
- Release fires on merge when the version differs from the last tag. Merge one release PR at a time and wait for `Build & Release` (about 35 minutes).
- `releases/latest` can answer 404 for a minute after a release; the site build now retries.
- `tests/fixtures/contract/surface.txt` and `book/src/cli-reference.md` regenerate with `UNV_UPDATE_CONTRACT=1 cargo test -p unv-cli --test contract_surface`.
- The review package lives in `research/` (gitignored); never commit it.
