# Phase log 86: Phase 56, Rust behaviour tests, final state and branch preparation

Phase 56 is the test round of both sessions (this file and phase log 85); version 0.56.0.

Supersedes the Next Steps of phase log 83 and phase log 84 (their content stays true as history).
The TypeScript half of the same push is phase log 85 (another session).

## Goal

Maintainer asked for Rust test coverage "100%, or as much as possible", then set the rule:
tests must check that the code does what HE expects, functionally and behaviourally; coverage
is a map, never the goal. Later rule: no long or full local runs unless asked; one targeted
test file at a time.

## Current Progress

New test files (all untracked, each run green ALONE; no workspace run was done):

- `unv-cli/tests/`: 35 `contract_*.rs` files plus `common/` (37 .rs files, about 291 tests). They
  run the real `unv`, an in-process `unv-server`, `unv node run --once` as the agent, a real
  KDBX built with the `keepass` crate, and a real OpenSSH container for SFTP (skips itself
  without Docker or the `atmoz/sftp` image).
- `src-tauri/tests/`: `ipc.rs`, `ipc_pure.rs`, `ipc_session.rs`, `common/mod.rs` (32 tests).
  Desktop commands are now generic over `tauri::Runtime`, `register()` holds state plus the
  handler list, and the tests call the shipped registration on `tauri::test` (no display).
- Every command in `unv describe` now appears in at least one test (31 had none this stretch).

Coverage: last MEASURED figure is still 78.86 percent lines (43,167 of 54,738) from before
phase log 83. It is stale: this stretch added hub node flows, drills, scan, enrich, KeePass,
SFTP, the odd commands and all of `src-tauri` (was 7.8 percent). The real number is higher
and unknown until the workspace is run.

Verification that did run: each new test file alone; `cargo test -p unenverse` (4 binaries,
32 plus 5 unit tests); `cargo test -p vault-core users` (59 passed); `cargo test -p unv-app
--lib enrich` (11 passed); `cargo test -p unv-server class` (2 passed); `cargo clippy -p
unenverse -p unv-cli -p unv-app -p vault-core --all-targets -- -D warnings` clean; `cargo fmt
--all` applied. NOT run: `cargo test --workspace`, `unv-server` full suite, `npm run
preflight`, the TypeScript suite, ui-lab.

## The bugs that mattered

1. Desktop never installed a rustls crypto provider (reqwest is `rustls-no-provider`): unpinned
   `remote_request`, `oauth_refresh`, `enrich_online`, `catalogue_update` panicked on client
   build. Fixed in `register()`; test fails without it (checked).
2. Deleting a class with members unassigned them, so a member with a broad rule of their own
   became unrestricted. Server already refused; `vault_core::delete_user_class` now refuses, so
   desktop and `unv class rm` match. Error maps to conflict (6); confirm texts reworded.
3. Failed restore drill exited 0 with `ok: true` (`finish_run` in `unv-cli/src/main.rs`: now
   exit 7, one document, run in `error.details`); unknown destination now not_found (3).
4. Taken user, class or node-token name was a 400 (CLI exit 10, raw SQL text); now 409 and a
   readable message on the server and in the desktop commands.
5. Chunk rename onto an existing name left two indistinguishable chunks.
6. `enrich --apply` proposed the same no-op `secretType=api_key` forever.
7. Unknown run id in `ops cancel`, "already exists", unknown destination, bad schedule time
   or zone exited with generic code 1.
8. Earlier in the push (phase log 83/84): secrets files not created 0600, `render --out`
   without matlog, wrong master password not denied, closed pipe panic, `gen ssh` key field,
   `doctor`/`uid` two JSON documents, `--user` flag collisions, cookie curl, TOTP import of an
   encrypted Aegis export.

## Decisions made

- Behaviour over coverage; a deviation found is fixed once at the shared point (class refusal in
  vault-core, taken-name mapping in `error.rs` and the server `rejected`).
- Commands generic over the runtime is a behaviour-preserving refactor needed for testability;
  it sits in the same desktop commit as the provider fix (one file), flagged in the commit text.
- Not decided here (maintainer): the findings list in phase log 83/84 (README `get --out`,
  `export --format json`, `user strict-write --server`, global `--json` clash, `entry add
--categories` not registering the category, `user passwd` non-interactive, and others).

## What Worked / What Didn't Work

Worked: contract tests against the real binary; mock-runtime IPC tests; HOTP for deterministic
codes; building a real KDBX in the test; one shared data dir per src-tauri test binary.
Didn't: first guesses of flags and JSON shapes (each fixed per test); a data dir per test in
src-tauri (`unv_app::access::set_paths` is a `OnceLock`, the second test used the first's
vault); `cargo fmt` is needed after bulk edits.

## Next Steps

Only the maintainer can:

1. Run `~/Code/UnENVerse-commit-rust-tests.ps1`, then `git switch main` is done by the script,
   then `~/Code/UnENVerse-commit-typescript-coverage.ps1`, then `~/Code/UnENVerse-push-branches.ps1`.
2. Run the whole workspace once (`cargo test --workspace`, optionally `cargo llvm-cov
--workspace`, 25 to 40 minutes) to confirm the shared edits (`vault_core` class delete,
   `error.rs`, server rejects) against existing tests and to get the real percentage.
3. Decide the findings list; set or rotate `CODECOV_TOKEN`; close Dependabot PR #28.

Still not covered: residue in `unv-cli/src/main.rs`, `caps_lock_state` (needs GDK),
`probe_cert_fingerprint` success path (needs a TLS server), Windows-only and defensive branches.

## Notes for whoever picks this up

- `unv-app/src/refs.rs` and `src/ts/users.ts` carry the TypeScript session's changes, plus one
  line of mine in `users.ts` (class delete confirm text); they ship in the TypeScript branch.
- Unset `UNV_SERVER_URL`/`UNV_PASSWORD` before running `unv` by hand.
- src-tauri tests: URL `tauri://localhost`, camelCase args, lock before `backup_archive_build`.
- `.claude/rules/desktop-platform.md` has the mock-runtime and provider notes.

## Branches pushed (2026-10-18)

- PR #37 `rust-behaviour-tests` (10 commits, last is "Release 0.56.0": version in all manifests,
  `Cargo.lock`, openapi fixture, CHANGELOG). PR #38 `raise-typescript-test-coverage` (19 commits,
  stays on 0.55.1 by design; the TypeScript session's phase log 85 covers it).
- Merge #37 first (it carries the release), then #38. Intermediate #37 commits are not each
  consistent with the lock file; the final one is. CI on both PRs has not been seen yet:
  check the `CI result` check, then the first `cargo test --workspace` result is the real
  confirmation of the shared edits.
- Phase 56 recorded in `the session records/PHASES.md` (next number 57) and `the session records/roadmap/STATUS.md`.
- After merge: tag/release is the maintainer's call (version already 0.56.0 on #37).

## CI fixes and merge (2026-10-18)

First full CI of both PRs was red. The whole workspace had never run, only single test files, and Windows had never run at all.

PR #38 (`raise-typescript-test-coverage`):

- Frontend failed on Prettier: `src/ts/users.ts` reformatted.
- Frontend then failed on an unhandled `document is not defined` from the vault poll timer firing after jsdom teardown: `closeDesktop()` in `tests/desktop-boot.ts` now calls `stopVaultWatch()`.
- CodeQL alert 183 (`tests/app-boot.ts`, clear text storage) is a fake credential in a test helper. Dismissed by the maintainer as "used in tests".

PR #37 (`rust-behaviour-tests`):

- Lint: `cargo fmt` diff in `contract_users_admin.rs`; unused `Path` import in `unv-cli/tests/common/mod.rs` on Windows (now imported with the helpers that use it).
- Windows desktop tests: `STATUS_ENTRYPOINT_NOT_FOUND` because the test binaries had no Common Controls manifest. `src-tauri/build.rs` now links `src-tauri/windows-test.manifest` into tests (msvc only).
- Windows desktop tests: `X not allowed. Plugin not found`, the ACL only admits `http://tauri.localhost` on Windows. `tests/common/mod.rs` has `APP_URL` per platform. Tests that unlock a vault (`ipc.rs` x3, all of `ipc_session.rs`) are `#[cfg(unix)]` because `app_data_dir` ignores `XDG_DATA_HOME` on Windows and would touch the real profile.
- `contract_doctor`: Windows reports the permissions check as a note, the test expects that now.
- `contract_generators`: `/dev/urandom` and `/dev/zero` cases are `#[cfg(unix)]`.
- `contract_node_agent`: Linux-only by design (`unv node install` is systemd), file gated to `target_os = "linux"`.
- `contract_materialise`: the two tests that read the materialisation log (and use `true`) are `#[cfg(unix)]`; the log path has no override and Windows ignores `XDG_STATE_HOME`.
- Product bug: `unv` panicked on a closed pipe on Windows (os error 109 "The pipe has been ended", also 232). `is_closed_pipe_panic` in `unv-cli/src/main.rs` now matches them; the existing pipeline test was right.
- Windows sessions: the harness shared the runner's real session file. `common::pin_state_files` sets `UNV_SESSION_FILE`/`UNV_POOL_FILE` per test on Windows, at the Unix default path; used by `cached()` and `login()` in `contract_sessions.rs`.
- Product bug: on Windows `rpassword` reads the console device, so `unv` with no password and a redirected stdin hung forever (the logout test hung for 18+ min, one run for 45). `unv-app/src/access.rs::get_password_for` now returns the empty password (a denial) on Windows when stdin is not a terminal. The other `rpassword` prompts (new user, backup) are interactive by design and untouched.
- Ubuntu: `contract_users` exited with SIGSEGV after all 12 tests passed (flaky). The test `Server` thread was never stopped and raced the process exit handlers. `common/server.rs` now shuts the server down gracefully in `Drop` and waits up to 10 s; `unv-cli` dev-dep tokio gained the `sync` feature.
- `ci.yml` Test step now runs `cargo test --workspace --locked --verbose --no-fail-fast` so one run lists every failure (a Windows run is about 14 min).
- CodeQL: 22 alerts on test files (hard-coded test passwords, test fixtures logged) dismissed by the maintainer as "used in tests".

Merge: #37 `9ee8b22b1101…`, then #38 `c49a1bb0fac4…` (no conflicts). `origin/main` package.json reads 0.56.0.

Traps for the next agent:

- Other sessions edit the main checkout at the same time (about 380 dirty files while this ran). Work in `git worktree add` copies; a worktree needs `node_modules` and `dist` symlinked from the main checkout.
- GitHub job logs stay unreadable until the run finishes; `gh run cancel` makes them readable, which is how a hung Windows test was found. Use `gh api --allow-escape-sequences repos/.../actions/jobs/<id>/logs`.
- `gh pr checks --required` reports nothing here; the ruleset's required check is the job named `CI result`.
- Windows resolves `app_data_dir`, `%LOCALAPPDATA%` state and similar from the profile and ignores XDG variables. Any new test that touches state must pin the file with env vars or be `#[cfg(unix)]`.
- Not done: a local `main` pull. The main checkout holds other sessions' uncommitted work, so `git stash push -u` first, then `git switch main; git pull`.

## Local main and branch inventory (2026-10-18, after the merges)

- Local `main` fast-forwarded to `c49a1bb` after `git stash push -u -m before-main-pull`; `npm run version` prints 0.56.0. The stash (`stash@{0}`) holds other sessions' uncommitted work (roadmap, SPDX lines, `ASSURANCE_CASE.md`, CHANGELOG edits) taken on the old branch base; `git stash pop` may conflict on `CHANGELOG.md`.
- Merged and safe to delete (checked with `git branch --merged` and `git cherry`): local and remote `rust-behaviour-tests` and `raise-typescript-test-coverage`; local `worktree-agent-a3723c0e15ae0afb5` (May); remote `add-baseline-badge`, `restyle-baseline-badge`, `fix-security-txt` (PRs #34, #35, #36 merged by rebase, so `--merged` misses them but every commit is in `main` by patch).
- Kept: local `backup/pre-secret-scrub` (2 commits not in `main`, a deliberate backup).
- 12 `dependabot/*` branches each back an open PR (#4, #13 to #17, #20, #25, #29, #31, #33, #39). Deleting a branch closes its PR. #4 (TypeScript 7.0.2) and #13 to #16 date from 8 Oct. Whether to close them is the maintainer's call; the major bumps (vite 8, codeql-action 4, typescript 7) were not evaluated here.
- Deletion commands were handed to the maintainer, not run: branch deletion was outside the authorised scope.
