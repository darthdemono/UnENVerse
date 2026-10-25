# Phase log 91: (2026-10-22): roadmap R10 to R21 first slices, release 0.58.0 published

## Goal

After release 0.57.0 the standing order was "keep implementing until all phases are done". This
session worked R10 to R21 in order, each as a tested slice, committing and pushing on branches cut
from `origin/main` (session grant), and prepared release 0.58.0.

## Current Progress

Merged to main: PR 50 (R10/R11/R12 slices, CI fixes), PR 51 (rotation engine, Cloudflare, `unv rotate`),
PR 52 (PostgreSQL connector). Open: PR 53 branch `phase-61-plugins` (R14, R15, R16, R19, R21, tests,
version bump to 0.58.0, CHANGELOG heading `0.58.0 (2026-10-22)`). Merging PR 53 to main releases 0.58.0
automatically (version differs from the last tag), so it was NOT merged: that is the maintainer's call.

Verified locally (lowest-resource runs only after the user said it used too much RAM and CPU):

- vault-core lib: rotation 29, identity 13, platform 9, plugin tests green; golden_vault green
- unv-app: issuer_postgres 10 (1 needs a server, run against a real postgres:16 container earlier),
  issuer_cloudflare 11, plugin_host 17, consent_prompts, rotate_flows
- unv-cli: contract_rotate 7, contract_plugin 7, contract_vault_instance 6, contract_surface,
  contract_recipes 3, capabilities
- unv-server nodes_e2e epoch tests; vitest palette 13, health-scan 34, vault-wiring
  Not run since the last edits: the full workspace test suites and the TypeScript coverage thresholds
  (CI does both). The last full Rust llvm-cov run (before the final test additions) gave per-file lines:
  rotation 93.5, issuer_cloudflare 90.3, issuer_postgres 64.1, rotate_cmd 64.2, plugin_cmd 55.3,
  plugin_host 85.2, vault_cmd 63.6, identity 98.2, platform 97.8, plugin 95.3. Tests were added for
  almost every missed line afterwards (fake psql, misbehaving token API, plugin failure modes, prompts,
  rotate flows) but the re-measure was killed to save resources, so those numbers are stale.

## The bugs that mattered

1. Rolled credentials were verified, deployed and then revoked by id, which deleted the credential just
   rotated (found by the Cloudflare tests; fixed, regression test in rotation.rs).
2. A lost revoke could not be reconciled because the old token cannot answer once deleted (successor
   token now used).
3. `unv rotate run --resume` demanded `--token-id` although the run knew it (fixed).
4. The recipes first drafted for the contract page were wrong (`file write` is for blobs, `get` is a
   search, not a lookup); the recipe test caught them.
5. macOS export test directories collided (microsecond clock): counter added.
6. Windows: rooted plugin path `/bin/sh` was not "absolute"; open SQLite handle blocked directory removal.

## Decisions made

ADR-0160 to 0163 (earlier), 0164 rotation revokes last and never retries destructive calls, 0165
plugins are supervised programs with a frozen line protocol (no sandbox), 0166 Windows owner-only ACLs and
local device key, 0167 one live instance per vault with a node-remembered epoch.

## What Worked / What Didn't

Worked: injecting faults at every step (property test), fake `psql`, real Postgres container for the
SCRAM verifier, conformance suite plus faulty plugin copies. Didn't: full-workspace llvm-cov on this
machine (hours, high RAM); CodeQL flags literal passwords and "log" formatting in test code, so tests
build secrets at run time and avoid formatting them into assertion messages.

## Next Steps

Human only: merge PR 53 (releases 0.58.0) once `CI result` is green and CodeQL has no medium alert;
enable the repo Dependency graph (dependency-review stays red); (done 2026-10-22, secret set, public key pinned in PR 54) add the `CATALOGUE_SIGNING_KEY`
Actions secret (the signed catalogue is served only then); decide about publishing a macOS build.
Agent work, in order: finish `the session records/wip/rekey.rs` (R20 master-password change: the export must not
run inside a transaction, a second guard connection holds the write lock; the leftover failure was
"database table is locked" from ATTACH/DETACH inside BEGIN EXCLUSIVE, the guard-connection fix was
written but not re-run), then `unv vault rekey`, `unv entry forget` (config snapshots need a
`forgotten` column so the chain still verifies), R17 gitleaks/lego experiments, R18 desktop bridge,
R22 to R29, R30 to R34. Also: rotation consumers from node targets, a Settings, Plugins screen, the
desktop activate action.

## Notes for whoever picks this up

- Final CI on PR 53 (0.58.0): everything green except dependency-review (repo setting). Two flakes seen and cleared by a re-run: macOS `blast` test ("file is not a database" once) and the npm production audit ("Invalid package tree"). The macOS one may be a real race on first creation of an encrypted database; watch it.
- Commit rules: groups, one plain sentence, no attribution, no mention of tooling; permission is per
  session. Never run a whole-workspace `cargo llvm-cov` here; target one crate with `-p` and `nice`.
- `tests/fixtures/contract/surface.txt` must be regenerated (UNV_UPDATE_CONTRACT=1) with any new
  command or flag and the diff read.
- `winacl.rs` and the macOS job are checked only by their CI runs.

## Addendum: release, key, audit plan, cleanup (end of session)

- **Released v0.58.0** (63 assets, all jobs green). Merged PR 54 (catalogue key pin) then PR 53. Rule from
  the user: the next release is `0.{completed phases}.0` (see memory `feedback-release-version-rule`);
  phases 59 to 63 and 66 are partial, so the next number is not automatic.
- **Catalogue signing key replaced.** The old seed was lost (it lived in a deleted scratchpad). The user
  generated a new Ed25519 key with `openssl` and stored the seed as the `CATALOGUE_SIGNING_KEY` Actions
  secret; the public half `b3df5624...ad19d` is pinned in `vault-core/src/catalogue.rs`. Builds up to
  0.57.0 keep the old pin and use the built-in catalogue. To check: after a green `docs.yml` run,
  `https://unenverse.darthdemono.com/catalogue/catalogue.json` should answer 200.
- **Dependency graph enabled** by the user; `dependency-review` is now green.
- **Docs workflow failed after the release** because a doc link pointed at a private constant
  (`platform.rs`); fixed in PR 55 (a plain code span). Merge it so the site and references deploy.
- **Flakes found and fixed:** test directories named from the clock collided on macOS/Windows (counter
  added in history, blast, shield, stack_cli, nodes_e2e); the Windows owner-only access list was being
  set on every database open (now once per path per process), which stalled concurrent opens.
- **Security audit plan** (from an external review pasted by the user). Finding 1 is CONFIRMED:
  `unv-app/src/backup.rs` reads `kdf.iters` from an untrusted backup with no cap and `as u32`
  truncation, and the second import path also accepts 0 and ignores `kdf.name` and `kdf.hash`.
  Plan: A) one `validate_kdf` (name PBKDF2, hash SHA-256, iters 100k to 5M, reject non-integers and
  overflow) with regression tests, fast-fail; B) trace key and plaintext lifetimes and zeroize, test
  backup file handling failures, route-by-route authorization matrix test; C) audit/deny/OSV on the tag,
  fuzz the backup envelope and import parsers, config-history `forget` (R20); D) keep "no independent
  audit" prominent and write a reviewer scope document. Open question for the user: refuse backups with
  fewer than 100,000 iterations, or warn behind an `--allow-weak-kdf` flag.
- **Cleanup done:** local and remote branches of merged PRs deleted (kept `backup/pre-secret-scrub` and the
  dependabot branches), `target/llvm-cov-target` (23 GB) and incremental caches removed, scratch coverage
  files removed. The parked R20 draft is `the session records/wip/rekey.rs` (not in the tree).
- **Resource note:** never run whole-workspace `cargo llvm-cov` here (the user said it used too much RAM
  and CPU); use `-p <crate>`, `nice -n 10` and `CARGO_BUILD_JOBS=2`.
