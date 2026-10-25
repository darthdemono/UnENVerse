# Phase log 65: roadmap R00 (baseline and instruction split), 2026-10-10

## Goal

Run phase R00 from the session records/roadmap/phases/00_baseline_and_instruction_split.md after the research-bundle integration session (New/LOCAL_HANDOFF.md). No application code, no tracked file changes, no git add/commit/push.

## Starting and ending state

## Baseline (step 1), ran sequentially, logs in the session scratchpad

| Check                                                              | Result                                                      | Time  |
| ------------------------------------------------------------------ | ----------------------------------------------------------- | ----- |
| cargo fmt --all -- --check                                         | pass                                                        | 1 s   |
| cargo clippy --workspace --locked --all-targets -- -D warnings     | pass                                                        | 27 s  |
| cargo test --workspace --locked                                    | pass, 717 passed, 0 failed (sum of all `test result` lines) | 517 s |
| npm run check (version, format, lint, typecheck, vitest)           | pass; Vitest 88 files, 1501 tests                           | 201 s |
| npx vite build                                                     | pass                                                        | 3 s   |
| Not run: `npm run ui-lab` (not in R00's list), Windows/macOS jobs. |

## Steps 2-8 (done by the integration session, re-verified here)

- Backups: the session records/archive/AGENTS-20261010-full.md sha256 f716efe2c28d… (equals the attachment hash the research used, so nothing newer to carry); CLAUDE-20261010.md sha256 336cc4fb...0ab49.
- Loading mode found (step 4): the old CLAUDE.md was the single line `@AGENTS.md`, an import of the whole 398,274-byte file at every launch. Now `@AGENTS.md` plus a short Claude section; AGENTS.md is 8,137 bytes, 145 lines (budgets 32,768 / 200 pass).
- PHASES.md and the session records/archive/ (phase-designs/, goals, bug history, key decisions, pitfalls, invariants, architecture, testing, maintenance procedures) are verbatim line-range copies of the original.
- Not done: `/context` check. It needs a fresh interactive Claude Code session; this session cannot run it. Maintainer to run it in a new session: expect AGENTS.md, CLAUDE.md and routing.md loaded at launch, and e.g. desktop-platform.md after opening a file under src-tauri/. Exit criterion 3 is open until that output is recorded.

## The 13 stale claims, checked against code

1 version: AGENTS.md carries no version now (npm run version = 0.42.4). 2 Rust: Dockerfile `FROM rust:1.90-bookworm`, local rustc 1.99.0; no version written in AGENTS.md. 3 src-tauri/Cargo.lock: IS git-tracked (git ls-files lists it beside root Cargo.lock); the research called it stale and was wrong on tracked status; AGENTS.md says keep it. 4 settings: UI uses localStorage `unenverse-settings` (src/ts/state.ts:1211,1219); CLI reads `io.unenverse/settings.json` (unv-cli/src/refs.rs:201); nothing about a settings location is in AGENTS.md now. 5 SecretType: secret-types.json holds 26 entries; list removed from AGENTS.md. 6 24.1 "Not implemented" sub-header: still in the archived Phase-24.1.md (verbatim copy, left). 7 tracing: present in unv-cli, unv-server, vault-core Cargo.toml; goals text archived. 8 ENVV shim: vault-core/src has no compat module, but book/src/adr/0149 line 20 still describes `vault_core::compat::adopt_legacy_env()`: public doc drift, finding for a later phase (tracked file, not touched). 9 packaging: only unv-node.service exists. 10 ADR-0084: Status superseded by ADR-0076, the old log row was unmarked; the log is archived and replaced by the ADR index (151 files in book/src/adr). 11 lock local on switch: Settings has `keepLocalUnlocked` (state.ts:1189, remote-panel.ts:438; default false) so the open item is resolved. 12 test count: 1501 Vitest measured today; no count in AGENTS.md. 13 CLAUDE.md: was the import, see above.
Additional finding: draft rule globs and one routing row mention `unv-app/`, which does not exist yet (R04). Kept as forward-looking and marked in AGENTS.md.

## Memory MCP (step 9)

claude.ai config (user level ~/.claude.json) has one server `memory`: `npx -y @modelcontextprotocol/server-memory`, status connected; storage path is set by an environment value I did not print. Tools mcp__memory__*. Wrote entities `unenverse:research-bundle:2026-10-09` and `unenverse:phase:R00` (non-secret pointers) and read the latter back. Left in place; delete if unwanted.

## STATUS

R00 set to `complete` in the session records/roadmap/STATUS.md with the exception above noted. R->project number mapping: no project numbers are assigned yet; the next project phase number remains 43 (PHASES.md).

## Traps for the next agent

- The shell exports UNV_SERVER_URL; unset before any unv/cargo test run.
- cargo test --workspace takes about 9 minutes; npm run check about 3.5.
- research/, the session records/, .claude/, AGENTS.md, CLAUDE.md are gitignored; search tools may skip them; read by path.

## Next

R01 (security hotfixes) and R02 (CI release gating) can run in parallel.

> 2 line(s) about the maintainer's own machines were left out of this page.
