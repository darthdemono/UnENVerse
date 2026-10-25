# Phase log 83: Rust behaviour tests: README-driven CLI contract suite and the ten bugs it found

20 contract test files that run the real unv binary (and a real unv-server) against the behaviour the README and AGENTS.md promise; Rust line coverage 69.97 to 78.86 percent; ten real bugs fixed, nine findings left for the maintainer.

## Rust behaviour tests

Date: 2026-10-17 to 2026-10-18, after `82-public-trust-round-scorecard-sboms-ruleset-best-practice.md`. Version 0.55.1. Nothing here is
committed. Untracked `tests/*.test.ts`, `tests/app-boot.ts`, `vitest.config.ts` and
`src/ts/*` edits in the tree belong to a separate TypeScript coverage effort, not to this
work.

## Goal

The maintainer asked for "100% Rust tests", then corrected the meaning: the tests exist to
check that the code does what HE wants and expects, functionally and behaviourally, and
coverage is only a map. So expectations come from the README, AGENTS.md, `--help` and the
ADRs, never from reading the implementation back; a deviation is a finding (fixed with a
regression test when the right behaviour is unambiguous, reported when it is a decision).
100 percent was never reachable and was not the aim; the honest figure is below.

## Current Progress

Coverage (cargo-llvm-cov 0.9.1, `cargo llvm-cov --workspace --locked
--ignore-filename-regex '(^|/)(tests|fuzz)/'`, whole workspace, all tests green):

| Crate      | Baseline                                     | Now                                        |
| ---------- | -------------------------------------------- | ------------------------------------------ |
| all        | 69.97% lines (38252/54667), functions 61.59% | **78.86%** (43167/54738), functions 69.35% |
| vault-core | 91.3%                                        | 91.9%                                      |
| unv-server | 86.4%                                        | 88.3%                                      |
| unv-app    | 52.2%                                        | 69.4%                                      |
| unv-cli    | 19.4%                                        | 64.7%                                      |
| src-tauri  | 7.8%                                         | 7.8%                                       |

A full run takes 25 to 40 minutes on 4 cores (load 13 to 18): every CLI call does an
Argon2 derivation and the instrumented build is slower. A flaky test makes the whole run
exit 101 without writing the JSON (it happened once, see Bugs).

Files (all new, `unv-cli/tests/`):

- `common/mod.rs`: harness. Each `Vault` is isolated (unique temp dir, own HOME,
  XDG_DATA_HOME, XDG_CONFIG_HOME, XDG_STATE_HOME; `UNV_*` variables that would hit a real
  server or password are removed). `run` (envelope), `run_text`, `run_no_yes`, `run_with`
  (globals), `run_stdin`, `ok`, `reveal_field`, `Out::json/error_code`, `mode_of`.
- `common/server.rs`: starts a real `unv-server` in-process on an ephemeral port
  (`Server::start`, `start_with(.., uid_registry)`), `Vault::on/as_user/with_token`,
  `http_get`. Included with `#[path = "common/server.rs"] mod server;` so test files that
  do not need it do not link it. New dev-dependencies in `unv-cli/Cargo.toml`:
  `unv-server`, `axum`, `tokio`; `Cargo.lock` changed accordingly.
- `contract_secrets`, `contract_exit_codes`, `contract_orchestrator`,
  `contract_projects_refs`, `contract_users`, `contract_entries`, `contract_totp_pools`,
  `contract_backup`, `contract_recovery`, `contract_materialise`, `contract_sessions`,
  `contract_import_vault`, `contract_bundles`, `contract_generators`, `contract_doctor`,
  `contract_context`, `contract_pipelines`, `contract_nodes`, `contract_health`,
  `contract_formats`, `contract_feeds_uid`, `contract_migrate`.

Code changes (each has a regression test that failed first):

1. `unv-app/src/fmt.rs`: `emit` wrote every `--out` file with the default umask (0644)
   although each `--out` help says 0600. New `write_owner_only` (create 0600, tighten an
   existing file before writing); `write_secret_file` reuses it.
2. `unv-app/src/render.rs`: `render --out` was 0644 and absent from the materialisation
   log (`matlog.rs` says every `--out` is logged). Now uses `write_secret_file`.
3. `unv-app/src/error.rs`: a wrong master password on a local vault exited 1 `error`;
   `From<String>` now maps "Wrong master password" to `denied` (5).
4. `unv-cli/src/main.rs`: a closed pipe (`unv describe | head -1`) panicked with exit 101.
   A panic hook treats "failed printing to std... Broken pipe" as a quiet exit 0.
5. `unv-app/src/entries.rs` `cmd_rename`: renaming onto an existing name created two
   indistinguishable entries; now a conflict (6), the same rule `entry add` uses.
6. `unv-cli/src/main.rs`: `gen ssh` read `private_key_openssh` but vault-core returns
   `private_key`, so the private key printed empty and `--save-as` stored an empty
   `ssh_key` entry (silent data loss). Fixed in both places.
7. `unv-app/src/out.rs` new `ok_or_fail`; `doctor.rs` and `uid_cmd.rs` use it: a command
   that computes a result and then fails printed a success envelope AND an error envelope
   (two JSON documents). Now one error envelope carrying the result in `error.details`.
8. `unv-cli/src/main.rs`: `backup dest add sftp --user` collided with the global `--user`;
   clap's debug assertion panicked on `unv completions <shell>` and on that command's
   `--help`. Renamed to `--username` (CLI-visible).
9. `unv-app/src/node_cmd.rs` `token_new`: minted the enrollment token, then refused to
   print it, leaving a live unreadable token that blocked the node name. The output policy
   is now checked before minting. Also `node approver rm`: the positional named
   `fingerprint` shared its id with the global TLS-pin option and a prefix was rejected
   ("must be 64 hex"); the field is now `device` with `value_name = "FINGERPRINT"`.
10. `unv-app/src/entries.rs` `cmd_cookie`: `cookie curl` printed `curl '<url>'` with no
    cookie because it used the generic api-key curl builder; it now presents the jar as the
    Cookie header (and keeps the User-Agent).

Verification: every contract file passed on its own run, and the last full
`cargo llvm-cov --workspace` run exited 0 with no failing test. `actionlint` was not
relevant. `npm run check` was not re-run for the TypeScript side.

What did not run: no Windows or release-profile run (release builds have debug assertions
off, which is how the `--user` collision hid); no `cargo fuzz`; `src-tauri` is untested
beyond its five unit tests (the Tauri runtime is not reachable from a plain test).

## The bugs that mattered

Ranked by consequence: (6) `gen ssh` silently lost the private key; (1) and (2) secrets
written to world-readable files; (9) an unreadable live enrollment token; (4) a panic in
every pipeline that stops reading; (7) and (3) machine-readable output and exit codes
that misled a script; (5) duplicate entries; (8) `completions` and one `--help` crashing in
debug builds; (10) a session curl that could not authenticate.

## Decisions made

- Expectations come from documented behaviour (README, AGENTS.md, `--help`), checked by
  running the binary first when unsure, never from the implementation. Where a README
  claim and the code disagree and the right answer is ambiguous, it is a finding, not a
  test bent to pass.
- Black-box CLI contract tests (the real binary, plus a real server) over unit tests: they
  cover the CLI layer and the `unv-app` command modules together and pin the documented
  contract (exit codes, redaction, modes, atomicity).
- `emit` now makes EVERY `--out` owner-only, not only the ones whose help says 0600. A
  service that must read a rendered config as another user will need `chmod`; the
  alternative (a per-command policy) was rejected as the exact inconsistency that caused
  the bug. Revisit if group-readable output is wanted.
- HOTP (RFC 4226 vectors) instead of TOTP wherever a test compares two codes (a TOTP
  window rolling between two commands flaked the instrumented run).
- Rejected: asserting on the exit code of dead entropy devices and several other
  generic-1 errors (not specified), and asserting that audit gets a row for non-secret
  edits (it does not, and the README does not claim it).

## What Worked / What Didn't Work

Worked: reading `--help` and running a command by hand before writing each assertion;
scanning `unv describe` for global-id collisions; a test that runs `--help` on every
subcommand (about 200) to catch clap definition errors; asserting on parsed JSON fields
instead of substrings (`"issued": false` contains "issued").

Did not work: several first guesses at flags (`--user-password` on `user passwd`,
`--namespace` on `uid lookup`, `--config` on `node enroll`); `rm -rf` inside an
exploration command was blocked by the safety check (use a fresh directory instead);
a harness that wrote its output file inside the scratch repo contaminated its own diff.

## Findings for the maintainer (not fixed)

- README `unv get GitHub --out token.txt` does not work: `--out` requires `--profile`,
  and there is no way to write a raw value with `get`.
- README says a vault-wide export to stdout is refused, but `export --format json` is
  allowed (a masked structure).
- `user strict-write` refuses `--server` (exit 7 `unavailable`, a wrong code for an
  unsupported operation) although the server has a strict-write route.
- Global `--json` collides with `import --json`: a `.env` cannot be imported in envelope
  mode ("Not valid JSON"). `entry add/set --totp` (seed) also reuses the global `--totp`
  (second-factor code) id. `uid prune`, `calendar feed revoke`, `totp advance` and
  `node enroll --tofu` redefine global flags with the same meaning.
- README says all four importers are preview-first; `unv import FILE` writes at once.
- README says pool state is in the data directory (`io.unenverse`); it is
  `$XDG_STATE_HOME/unv/pools.json` (the app and CLI agree).
- `backup recovery-key` run twice silently replaces the retained recovery identity.
- `user passwd` can only prompt (or `--clear`); an orchestrator cannot set a password.
- `node token new --project nope` mints a token for a project that does not exist.
- Several user-input errors use the generic code 1 (invalid recovery identity, non-empty
  restore directory, dead entropy device) instead of `invalid` (10).

## Next Steps

Only the maintainer can: commit (group `unv-app/src`, `unv-cli/src`, `unv-cli/Cargo.toml`
with `Cargo.lock`, `unv-cli/tests/`, ADR-0159 with `book/src/SUMMARY.md`; main is
protected, use a branch from `origin/main` and a pull request); decide each finding above;
decide whether CI should run these tests (they add several minutes: each CLI call
derives an Argon2 key) and whether to add `cargo llvm-cov` to CI.

Doable by an agent next: more contract tests for the remaining gaps (largest first):
`src-tauri/src/lib.rs` (1508 lines missed, needs a Tauri-side approach), `unv-cli/src/main.rs`
(587 missed), `unv-app/node_cmd.rs` (501: the node agent side, `node run/check/install`),
`users_cmd.rs` (478: classes, totp enrollment, remote strict-write), `entries.rs` (449),
`recovery_ops.rs` (441: schedules, drills), `destinations.rs` (405: SFTP needs a server),
`password_sources.rs` (364: KeePass and attachment paths), `totp_cmd.rs` (345: import and
export formats), `enrich.rs`, `projects.rs`, `scan.rs`, `catalogue_cmd.rs`, `update_cmd.rs`.

## Notes for whoever picks this up

- Always `env -u UNV_SERVER_URL -u UNV_PASSWORD cargo test ...`; a set `UNV_SERVER_URL`
  sends commands to a real server (one exploration run once did, with a wrong password).
- The harness must keep isolating `XDG_STATE_HOME`: sessions, `pools.json` and
  `materialisations.jsonl` live there and a developer with it set would otherwise have
  tests touch real state.
- Never compare two TOTP codes across two commands; use HOTP.
- After a llvm-cov run delete `target/llvm-cov-target` (14 GB) with
- A command that computes a result and then fails must use `out::ok_or_fail`; printing
  `out::ok` and then returning `Err` makes stdout two JSON documents.
- Exploration scratch directories and two scratch `unv-server` processes were stopped;
  nothing of mine is left running.

> 1 line(s) about the maintainer's own machines were left out of this page.
