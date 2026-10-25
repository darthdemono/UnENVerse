# Phase log 61: CI fixes, dependency bumps, and making the phase numbering true again

Date: 2026-10-09. Version 0.39.0 (unchanged; agents never bump).

## Goal

User pasted three CI failure analyses and asked for all to be fixed, then to bump npm packages, fix the README, record phases 34-42 in memory, and remove stale claims.

## Current Progress (with evidence)

1. `.github/workflows/exporters.yml` line 418, job `grafana-probe`: apt install now `libsqlcipher-dev mold`. Cause: `.cargo/config.toml` passes `-fuse-ld=mold` for x86_64-unknown-linux-gnu and the job had no mold, so rustc failed `collect2: fatal error: cannot find 'ld'`. The pasted advice (`build-essential`) was wrong: gcc `ld` is not what is invoked. ci.yml, build.yml and docs.yml already install mold. Not run in CI yet.
2. `envv-server/tests/nodes_e2e.rs`: 10 sites changed from `path=\"{}\"` to the TOML literal string `path='{}'`. Windows temp paths contain `\U...`, an invalid TOML escape in a basic string. Rejected alternative from the paste: replace `\` with `/` (works but edits every call site's data; literal string is a one-character change). Linux: `cargo test -p envv-server --test nodes_e2e` 18 passed. Windows unverified until CI runs.
3. `.github/workflows/build.yml` artifact download: attempts 3 -> 5, backoff `i*20s` -> `i*30s`. The HTTP 500 "Request body length does not match content-length" is a transient GitHub error; the existing loop already retried. Rejected: the pasted rewrite (same loop plus `2>/dev/null`, which would hide the error text).
4. npm: bumped in-range via one `npm install --save-dev pkg@ver` per package: @playwright/test 1.64.0, @tauri-apps/cli 2.12.1, @types/node 26.6.4, eslint 10.12.0, globals 17.13.0, jsdom 30.1.2, prettier 3.9.9, typescript-eslint 8.71.1, simple-icons 16.34.0. `npm update` and any multi-package install crash with `Cannot read properties of null (reading 'edgesOut')`. vitest and @vitest/coverage-v8 stay 4.1.10 (their 4.1.11 pair fails the same way). Majors skipped: vite 5->8, typescript 5.9->7, vitest 4->5, js-yaml 4->5. `npm run check`: 88 files, 1497 tests passed after the bumps.
5. README line 13: "Phases 1 to 39" -> "1 to 40" (the table already had 40). Prettier check passes.
6. AGENTS.md: intro now says 0.39.0 / Phases 1-40 done and explains that `Phase N` in commit messages is a batch counter; next phase to allocate is 41 (was 40, wrong since 40 exists); test counts 1,497 TS in 88 files and 708 Rust (sum of `test result` lines of `cargo test --workspace --exclude envvault` = 704 plus 4 in envvault); project path is ~/Code/UnENVerse; Immediate Next Steps items 0.5 and 1 replaced (they described a 2026-09 branch and 0.20.0).
7. MCP memory entity "UnENVerse phases 34-42 (commit scripts vs planned phases)" created from the nine `~/Code/.Trash-1000/files/envvault-commit-phase34..42.ps1` scripts (they were moved to the trash by the user).

## The bugs that mattered

- The grafana-probe job could never link: every push to exporters.yml was red from that job.
- Windows `nodes_e2e` failed all node tests: TOML basic strings treat `\U` as a unicode escape.
- Stale build cache: after the directory rename EnvVault -> UnENVerse, `cargo test -p envvault` failed `failed to read plugin permissions: .../Code/EnvVault/target/...`. Build scripts of tauri, tauri-plugin-global-shortcut, -shell, -window-state (also ring, aws-lc-*, harmless) had the old absolute path in `target/debug/build/*/output`. Fix: `cargo clean -p tauri -p tauri-plugin-global-shortcut -p tauri-plugin-shell -p tauri-plugin-window-state` (2.7 GiB). Careful: my first try, `cargo clean -p envvault`, removed 33.2 GiB of the target dir (it did not fix anything) and the rebuild had to start over.

## Decisions

- "Phase 42" in git is a commit-batch label from the generated scripts; planned phases end at 40. Rejected: renumbering history. Going forward: plain-sentence commit messages for non-phase work.
- Did not bump major npm versions or the app version.

## Dead ends

- `npm update`, multi-package `npm install`: edgesOut crash (npm arborist bug).
- `cargo clean -p envvault`: wrong package, huge deletion.

## Not done

- No CI run was observed; all three CI fixes are unverified remotely.
- vitest 4.1.11 not applied. Major npm upgrades not attempted.
- Phase 39 live checklist, 31.3 Pages/secret/URL, 24 A4/A9/A10, 24.4 benchmark, `mdbook build` remain the maintainer's.

## Traps

- The `Phase N` commit prefix is not a phase number. phase log 62 is next.
- Scripts `envvault-commit-phaseN.ps1` are in `~/Code/.Trash-1000/files/`.
- `cargo test --workspace` fails in src-tauri if the target dir predates the rename; see above.
- Commit messages for these fixes were handed as a pwsh one-liner; the user commits.
