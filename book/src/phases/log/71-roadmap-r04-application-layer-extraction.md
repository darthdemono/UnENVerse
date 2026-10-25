# Phase log 71: Roadmap R04, application-layer extraction

## Goal

Complete roadmap R04, project phase 48: extract the reusable Rust application engine from `unv-cli` into a new `unv-app` workspace library without changing CLI, desktop, server, IPC, HTTP, storage or data-format behaviour. Leave `unv-cli` as the `unv` binary and a compatibility library facade, establish empty future feature boundaries, and prove the public CLI contract is unchanged.

## Current Progress

R04 is complete locally from committed R03 head `d4ea10224386…` at version `0.43.0`. The starting working tree was clean. Nothing was staged, committed, pushed, tagged, published or deployed.

`unv-app/` is a new workspace package and library. Its `src/` now owns the exact former application modules: `access`, `agentio`, `authreq`, `backup`, `blast_cmd`, `bundle_cmd`, `catalogue_cmd`, `check_cmd`, `chunks`, `context`, `cookies`, `cxf_cmd`, `data`, `diff_cmd`, `doctor`, `emit_cmd`, `enrich`, `entries`, `envfile`, `error`, `exec`, `exporters`, `exposure`, `feed_cmd`, `filecred`, `fmt`, `gen`, `history`, `history_cmd`, `import_vaults`, `matlog`, `node_agent`, `node_cmd`, `node_install`, `node_listen`, `oauth_cmd`, `out`, `pool`, `profile`, `projects`, `ratelimit`, `refs`, `render`, `reset_cmd`, `scan`, `session`, `session_cmd`, `shield`, `starters`, `template_cmd`, `tls`, `totp_cmd`, `uid_cmd` and `users_cmd`.

`unv-cli/src/` now contains only `main.rs` and `lib.rs`. The binary imports `unv_app` directly. `lib.rs` is deliberately only `pub use unv_app::*;`, retaining `unv_cli` as a source-compatible library name for external callers. The 12 integration tests that run `CARGO_BIN_EXE_unv` remain in `unv-cli/tests/`; the eight direct-library tests (`agent`, `archive`, `golden_backups`, `import_vaults`, `parity`, `ratelimit`, `stack_parity`, `totp`) and their seven private fixtures moved to `unv-app/tests/`.

Cargo graph changes:

- Root `Cargo.toml` adds `unv-app` as a workspace member and `Cargo.lock` records it.
- `unv-app/Cargo.toml` receives the former library dependencies and declares `connectors`, `destinations` and `aws`. Defaults enable the first two, but all three are empty today.
- `unv-cli/Cargo.toml` retains only dependencies used by the binary and facade.
- `src-tauri` and `unv-server` now depend on `unv-app`; `unv-server` uses `default-features = false`.
- All desktop, server and moved-test imports now use `unv_app` directly. The public compatibility facade remains present but is no longer an internal dependency.

Documentation and records changed as part of the phase:

- `book/src/adr/0151-the-shared-application-engine-is-unv-app.md` records the package boundary, compatibility facade and future feature policy. `book/src/SUMMARY.md` now indexes it and also indexes existing ADR-0149/0150 files that were previously omitted from the summary.
- `AGENTS.md` and `.claude/rules/cli-app.md` now describe the completed `unv-app` extraction rather than a future move.
- `the session records/roadmap/STATUS.md`, `the session records/PHASES.md` and `the session records/INDEX.md` record R04 as complete and allocate the next handoff number.

The final CI and delivery sweep caught paths that an ordinary Rust build does not exercise:

- `scripts/version.mjs` now surveys and writes all eight version sources: Tauri, the root and VS Code package manifests, and all five Cargo packages. Its Cargo.lock updater verifies all five workspace package entries. The build workflow performs the same eight-way check.
- Docker's cached builder stage now copies, stubs and rebuilds `unv-app`, the direct server dependency. It retains the `unv-cli` manifest stub because Cargo still parses every workspace manifest, but it no longer copies the old CLI application source into the server image.
- The exporter-validation workflow filters now name the moved `unv-app` sources. Static workflow, VS Code reference and legacy-backup tests point at the new module and fixture locations. Parity-fixture comments and agent routing guides were made true to the new boundary.

## The Bugs That Mattered

1. A mechanical source-path update changed the clap documentation for `unv totp code`, which changed `unv describe` JSON. That is a public machine-readable contract change even though the command still behaved the same. The generated description in `unv-cli/src/main.rs` was restored byte-for-byte to its former text. Internal path comments moved where they are not public output.
2. Comparing `cargo test -- --list | wc -l` per package gained two lines after the split: a new crate adds its own lib and doc-test harness headings. The actual test files are unchanged: 20 before and 20 after, now split eight application tests plus twelve binary tests. Do not mistake Cargo's additional harness bookkeeping for new behavioural coverage or a lost test.
3. The initial complete frontend gate failed three static path checks: the VS Code resolver test still read `unv-cli/src/refs.rs`, the old-backup test still read the old fixture path, and the workflow test expected `unv-cli` exporter paths. These failures were the extraction's missed consumer references, not baseline test failures. Updating all three made the focused 44-test set and the full 1,561-test gate pass.

## Decisions Made

- Move every reusable module, including output/redaction and clap-derived command types, into `unv-app`. Several command argument types are shared with application code, so retaining a partial library in `unv-cli` would create a second artificial boundary without making the binary meaningfully thinner.
- Keep `unv_cli` as a direct re-export, not a forwarding wrapper. It preserves existing Rust import paths without duplicate code or runtime indirection.
- Give `unv-app` default `connectors` and `destinations` feature names now, while leaving their implementation empty. This lets CLI/desktop builds opt into future integrations and lets the server explicitly remain lean before any integration dependency is introduced.
- Preserve the old `unv-cli/src/totp_cmd.rs` wording inside the generated CLI description. It is contract text, not a Rustdoc link; changing it would violate R04's no-output-change rule.
- No data model, schema, endpoint, IPC command, feature behaviour or version bump occurred. ADR-0151 is the only new architectural decision record.
- Keep the version and delivery wiring comprehensive when adding a workspace package. A crate is not fully integrated until the version writer, release guard, Docker dependency stage, workflow filters, static source readers and test fixtures all reach its moved files.

## What Worked

- `cargo metadata --format-version 1 --no-deps` confirmed the final five-member workspace: `unenverse`, `unv-app`, `vault-core`, `unv-server` and `unv-cli`.
- `cargo check --workspace --offline` passed after Cargo added the new local package to the root lockfile.
- `cargo check -p unv-app --no-default-features --locked` passed, and `cargo tree -p unv-server -e features --locked` shows `unv-server -> unv-app` without a `unv-app default` feature edge.
- The pre-move captures and final captures match exactly: `unv describe` SHA-256 `495c42273c8f…`; `unv --help` SHA-256 `259b17bac2fa…`.
- `cargo doc --workspace --no-deps --locked` passed, proving the moved modules and doctext build cleanly.
- `npm run preflight` completed after format, Clippy, the full locked Rust workspace suite, the frontend gate and Vite build. A subsequent `npm run check` also completed after version, formatting, lint, typecheck and Vitest.
- `npm run version:check` reported all eight sources at `0.43.0`; running the writer at its current value exercised the lockfile branch and reported `Cargo.lock (5/5 workspace entries)`.
- `docker buildx build --check --file Dockerfile .` completed with no warnings. It verifies the Dockerfile syntax, base-image metadata and build graph without creating a Rust target directory.
- After the moved-path corrections, focused Vitest coverage passed 44 tests in `tests/vscode-refs.test.ts`, `tests/workflows.test.ts` and `tests/backup.test.ts`; the complete `npm run check` passed 91 files and 1,561 tests.
- `git diff --check` is clean. The full test-list baseline was core 484, CLI 245, server 125 and desktop 8 lines; post-move it is core 484, app 198, CLI 49, server 125 and desktop 8, with the explained two-harness difference in the split pair.

## What Did Not Work

- The first blanket replacement of `unv-cli/src/...` references touched generated clap help text. The byte comparison caught it before completion. Do not use a path-wide rename without re-running `unv describe` and `unv --help` comparisons.
- `cargo test -- --list | wc -l` is a coarse structural baseline, not a raw test-function count. Package extraction changes its harness lines even when every test remains.
- A direct `npx prettier --check Dockerfile` is not a valid Dockerfile check because Prettier has no Dockerfile parser. The project formatter passed over all supported files; Docker Buildx's `--check` validated the Dockerfile itself.

## Next Steps

- Next roadmap phase is R05, ops runtime and local recovery. Its entry condition is now satisfied by the `unv-app` crate and its feature boundary.
- The maintainer must push to run GitHub's Linux and Windows matrix. No local command can execute the hosted runners.
- Commit the tracked R04 changes only. Ignored local records (`AGENTS.md`, `the session records/`) remain outside the commit.

## Notes for Whoever Picks This Up

- New application behaviour belongs in `unv-app`, once. `unv-cli` is only parsing, output wiring and compatibility re-export.
- Keep `unv-server` on `unv-app` with `default-features = false`; do not add server connector dependencies by feature unification accidentally.
- The root `Cargo.lock` remains the only lockfile. Do not recreate `src-tauri/Cargo.lock`.
- The version script has five workspace Cargo names and eight version sources. If a sixth package is added, update both `CRATES` and `CRATE_NAMES`, then run the writer once at the current version to prove its lockfile branch reaches every package.

> 1 line(s) about the maintainer's own machines were left out of this page.
