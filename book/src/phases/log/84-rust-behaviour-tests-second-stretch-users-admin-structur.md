# Phase log 84: Rust behaviour tests, second stretch: users admin, structure, totp io, node agent, destinations

Five more contract test files (each run green on its own), four more bug fixes, and what is deliberately not measured or not covered.

## Rust behaviour tests, second stretch

After `83-rust-behaviour-tests-readme-driven-cli-contract-suite-an.md` (line coverage 78.86 percent at that point). Nothing is committed.
The maintainer then said not to run local test runs and later allowed single targeted
runs, so **no full coverage run was made after phase log 83**: the 78.86 figure is the last
measured one and is now out of date (higher). Each new file below was run once, alone,
and passed; the whole workspace was NOT re-run after the last code edits.

## Current Progress

New files in `unv-cli/tests/` (all run green individually):

- `contract_users_admin.rs` (11): classes (add, ls, set, rm, duplicates, unknown, removal
  refused while members exist), users (add, rename, rm, list never shows a hash, removal
  ends tokens and login), `perm show`, and the two-phase user second factor (enroll does
  not enable, confirm enables, wrong code denied, a used code cannot be replayed, a token
  skips it, disable returns to password-only, the owner cannot enrol). The test computes
  codes with the CLI itself (the seed is also stored as an entry; `totp code --next` skips
  the window `confirm` used).
- `contract_structure.rs` (10): categories (unique ignoring case, rename carries entries,
  delete removes it from entries and deletes no entry), `tags` with counts, chunks (ls,
  show masks an env_file whole, rename rewrites `${chunk:Name/Field}` references, rename
  onto an existing name refused, unset, disable and enable, unknown project or chunk).
- `contract_totp_io.rs` (7): otpauth import (kinds, parameters, HOTP counter), rerun is a
  no-op, a different seed is a conflict shown as fingerprints unless `--force`, export is
  refused to the terminal and 0600, `--only`, Aegis and 2FAS round trips, an encrypted
  export refused by name, junk refused.
- `contract_node_agent.rs` (7): `node install --plan` (numbered steps on stderr, closing
  line on stdout, dedicated account, private node.toml, an applying target gets rw on its
  file plus a reload unit for one service, an observing target gets read only and no reload
  unit, relay service only with `--relay`, malformed targets refused) and `node check`.
- `contract_destinations.rs` (9): local destinations (unique id, list, remove keeps the
  packages), schedules (interval, daily converted from its IANA zone to UTC, bad time or
  zone or destination refused with their codes, interval and daily exclusive, monthly
  drill), a package needs a recovery identity and writes nothing without one.

Code changes (each covered by a test above): 11. `unv-server/src/lib.rs` `rejected()` (also used by `nodes.rs`): a taken name (SQLite
UNIQUE, "already open", "already exists") is 409 conflict instead of 400, with the
message "That name is already taken" instead of raw SQL. Applied to create user,
rename user, create class, update class and the node enrollment token. The CLI now
exits 6 for them (it exited 10). 12. `unv-app/src/chunks.rs` `rename`: renaming a chunk onto a name already used in its
project created two chunks with one name (every `${chunk:Name/Field}` ambiguous);
now a conflict, the rule `chunk add` already applied. 13. `vault-core/src/totp_import.rs` `parse_as`: an explicit `--format` skipped the
encrypted-export refusal and gave "no db.entries array"; the refusal by name now also
applies when the format is chosen. 14. `unv-app/src/error.rs`: `From<String>` classifies "... already exists" as conflict (6),
"No recovery destination" as not_found (3), and "Scheduled time must be" and "Unknown
IANA time zone" as invalid (10); they were all the generic code 1.

I read (did not run) the existing `unv-server` tests that mention 400 and duplicates: none
asserts the old status for a duplicate user, class or token.

## Decisions made

- Targeted single-file runs only, after the maintainer's instruction; no full run.
- A taken name is a conflict everywhere (README table), fixed once at the server boundary
  and once in the CLI's string classification rather than per command.

## Findings for the maintainer (new, not fixed)

- `entry add --categories infra` does not register `infra`: `category ls` omits it and
  `category rename` and `category rm` answer not_found, so such a category cannot be
  managed from the CLI (register it first with `category add`). Decide whether adding an
  entry should register its categories.
- A class with members cannot be removed (409, a sensible refusal); the earlier findings
  in phase log 83 are unchanged.

## Next Steps

Only the maintainer can: run the whole workspace once (`cargo test --workspace` and, if
wanted, `cargo llvm-cov --workspace`, 25 to 40 minutes) to see the new coverage and to
confirm the four code edits did not disturb an existing test; commit (branch from
`origin/main`, pull request).

Still not covered (largest first, from the last measured run): `src-tauri/src/lib.rs`,
`unv-cli/src/main.rs`, `unv-app/node_cmd.rs` (hub pull/accept/approve/reject, `node run`),
`recovery_ops.rs` (drill execution), `destinations.rs` (SFTP, needs a server),
`password_sources.rs` (KeePass and attachments), `enrich.rs`, `scan.rs`,
`catalogue_cmd.rs`, `update_cmd.rs`. Literal 100 percent is not reachable: some code is
Windows-only, some is defensive error handling that cannot be triggered from outside, and
`src-tauri` needs a GUI runtime.

## Notes for whoever picks this up

- The plan steps of `node install --plan` go to stderr; read both streams.
- `user passwd` cannot set a password non-interactively; `--clear` is the scriptable way to
  end a user's sessions.
- Do not start long background runs without being asked: the maintainer objected to the
  waiting and to progress reports about a run that was still going.

## Addendum

`unv-cli/tests/contract_node_hub.rs` (5 tests, green alone): held push, approve by prefix, reject, decided request conflict (6), pull to 0600 file never printed, accept. No code bugs found there.

`contract_drill` (3): a failed restore drill exited 0 with `ok: true`; now exit 7, one document, run in `error.details`; unknown destination is not_found (3) (`finish_run` in `unv-cli/src/main.rs`, destination check in `recovery_ops::run_now/run_drill_now`).
`contract_scan_enrich` (9): enrich proposed the same no-op `secretType=api_key` forever (`propose!` in `unv-app/src/enrich.rs` now skips a value equal to the current one).
`contract_keepass` (4): real KDBX built in the test (`keepass` dev-dependency in `unv-cli/Cargo.toml`), key-file only.

`contract_codes_misc` (11): recovery codes, file write, project show, chunk rm, template show, entry verify/timeline, totp uri/rm, ops. Bug: `ops cancel <unknown>` exited 1; now not_found (3) (`error.rs` "No such operation").
`contract_publisher_pool_history` (6): pool show/reset, history snapshot/where/diff-file, catalogue export/sign, update sign.
`contract_watch_uid_update` (5): watch (real process, edit reaches vault), uid prune, catalogue update/update install refusal.
Every command in `unv describe` now appears in at least one test.

## Desktop (src-tauri) now tested without a window

Commands made generic over the runtime, `register()` extracted from `run()`, `tauri` dev-dep with `test`. `src-tauri/tests/ipc.rs` (5), `ipc_pure.rs` (16), `ipc_session.rs` (11), all green alone.
Bugs fixed: (1) desktop never installed a rustls crypto provider, so unpinned `remote_request`, `oauth_refresh`, `enrich_online`, `catalogue_update` panicked (install in `register()`); (2) `create_user`/`rename_user`/`create_user_class`/`update_user_class` showed `UNIQUE constraint failed: users.username` (now "That name is already taken"); (3) deleting a class with members unassigned them (a member with a broad own expression became unrestricted); the server already refused, `vault_core::delete_user_class` now refuses for desktop and `unv class rm` too (message and UI confirm text updated). Checked fail-first for (1).
Also `contract_sftp` (2, real OpenSSH container, skipped without Docker/image).
`cargo clippy -p unenverse -p unv-cli -p unv-app -p vault-core --all-targets -- -D warnings` clean.
