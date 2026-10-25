# Phase log 78: CI repair for Rustls advisories and Windows recovery tests

Cargo deny now accepts only an unavoidable build-time advisory, Rustls is updated to its security fix, and recovery-operation tests close SQLite before Windows cleanup.

## CI repair for Rustls advisories and Windows recovery tests

## Goal

Find the failed UnENVerse GitHub Actions task and repair every concrete failure without widening the change beyond CI health.

## Current Progress

Complete, uncommitted work based on the current working tree.

`Cargo.lock` upgrades Rustls from 0.23.45 to 0.23.46. This is the smallest compatible release that fixes four newly published RustSec advisories reported by CI.

`deny.toml` documents and ignores `RUSTSEC-2026-0173` for `proc-macro-error2`, pulled in only at build time by `age -> i18n-embed-fl`. The advisory has no safe upgrade. It is not linked into the shipped program.

`unv-app/src/recovery_ops.rs` gives the six affected recovery-operation tests one `cleanup` helper. It explicitly drops the SQLite connection before removing the temporary vault directory. Unix permits unlinking the open database and hid the error; Windows preserves the open file lock and made those tests fail.

## Verification

- `cargo deny check advisories bans licenses sources` passed: advisories, bans, licenses and sources are all OK. Existing duplicate-package and stale-ignore warnings remain non-failing.
- `cargo test -p unv-app --lib --locked` passed: 117 passed, 0 failed.
- `cargo test --workspace --locked` passed, including 117 unv-app, 92 unv-server and 468 vault-core unit tests, plus integration and golden tests.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings`, and `git diff --check` passed.
- The hosted Windows runner and GitHub Actions rerun were not invoked because committing and pushing remain maintainer actions.

## The bugs that mattered

1. **High: the dependency-policy task rejected four fixed Rustls security advisories.** The locked 0.23.45 release was vulnerable even though its direct constraints allowed the patched version. The lockfile now selects 0.23.46.
2. **Medium: CI rejected an unmaintained macro crate with no safe upstream upgrade.** `proc-macro-error2` arrives through age's build-time localization dependency. The exception is explicit, limited and records why an upgrade cannot close it.
3. **Medium: Windows recovery tests deleted live SQLite vault files.** Six tests left their `rusqlite::Connection` open when deleting their scratch directory. The Windows runner correctly refused the locked files; every test now closes its connection before cleanup.

## Decisions made

- Update Rustls rather than ignore its advisories. The patch release is compatible and directly addresses all four findings.
- Add the narrow `proc-macro-error2` exception because the advisory reports no safe upgrade and the affected macro executes only while building. Do not treat this as permission to ignore runtime or fixable dependencies.
- Centralize the test cleanup step so later recovery-operation tests cannot accidentally reproduce the Windows-only file-lock failure.

## What Worked / What Didn't Work

The GitHub run logs isolated both failures: Linux dependency policy and Windows test cleanup. `cargo deny` gave the advisory chain and safe Rustls version, while the Windows logs named the open-file error and test lines.

The local Linux environment cannot reproduce Windows locking semantics, so correctness is enforced structurally by consuming the connection before cleanup and by the existing affected tests. No workflow change was needed.

## Next Steps

1. The maintainer should commit the three tracked files and push the branch. GitHub will then rerun the Linux dependency-policy and Windows workspace-test tasks.
2. If the hosted Windows task reports another lock, inspect the named test before adding a retry. Do not mask file-handle mistakes with timing loops.

## Notes for whoever picks this up

- The intended commit files are `Cargo.lock`, `deny.toml`, and `unv-app/src/recovery_ops.rs`.
- `.agents/` and `.serena/` are pre-existing untracked local directories and must remain unstaged.
- `the session records/` is ignored local session documentation. It is intentionally not part of the commit.

> 1 line(s) about the maintainer's own machines were left out of this page.
