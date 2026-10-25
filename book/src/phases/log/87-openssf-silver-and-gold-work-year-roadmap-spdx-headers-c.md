# Phase log 87: OpenSSF silver and gold work, year roadmap, SPDX headers, CI speed-up, trust badges, 0.56.1

Date: 2026-10-18. Version 0.56.1 (bumped, nothing committed or pushed). Branch at the time of writing: `main`
(PRs #37 and #38 were merged by the maintainer earlier in the day; v0.56.0 was published at 15:57 UTC).
Everything below is uncommitted in the working tree. Handoffs are gitignored; never put this file in a commit line.

## Goal

The session started as "read past conversations for bestpractices.dev data and fill the Silver form" and grew, in
the maintainer's order, to:

1. Fill OpenSSF Best Practices Silver, then Gold, as truthfully as the repository allows, and fix what could be
   fixed in code or docs to move criteria to Met.
2. A one-year plan (to 1.0.0) naming every remaining phase, put on the README and the website.
3. Find the real Rust coverage number without a long local run.
4. Why v0.56.0 had no release for 45+ minutes, and make the pipeline faster: tests only for the part that changed,
   release not 4 to 5 times longer than needed.
5. Codecov-style reporting for Rust, the Codecov tree graph, and trust badges on the README and the website.
6. Bump to 0.56.1 because code changed.

## Current Progress

All numbers below were produced in this session unless marked "not run".

### OpenSSF forms (nothing is submitted; the maintainer submits)

- `~/Code/bestpractices-silver.ps1` (47 answers, 3 tabs) and `~/Code/bestpractices-gold.ps1` (16 answers, 1 tab) are
  prefilled-URL scripts (`/en/projects/15623/{silver,gold}/edit?<id>_status=..&<id>_justification=..`), the same method
  as `bestpractices-baseline.ps1`. Generators live in the session scratchpad only (`gen.py`, `gen_gold.py`); the scripts
  are the artefact. Regenerating needs the answer table, which is inside those two python files (lost with the session).
  Live percentages when checked: passing 100, silver 15, gold 13, baseline 1 and 2 at 100, baseline 3 at 95.
- Silver answers: 32 Met, 3 N/A (`sites_password_security`, `installation_standard_variables`,
  `vulnerability_report_credit`), 7 Unmet (`access_continuity`, `bus_factor`, `internationalization`,
  `build_repeatable`, `test_statement_coverage80`, `crypto_algorithm_agility`, `version_tags_signed`). `dco` became Met
  after the pull-request-template checkbox was added.
- Gold answers: Met `copyright_per_file`, `license_per_file`, `small_tasks`, `require_2FA`, `secure_2FA` (maintainer
  confirmed TOTP), `code_review_standards`, `test_invocation`, `crypto_used_network`, `crypto_tls12`, `security_review`,
  `hardening`. Unmet `bus_factor`, `contributors_unassociated`, `two_person_review`, `build_reproducible`,
  `hardened_site` (GitHub Pages cannot set CSP, HSTS, nosniff or X-Frame-Options headers; live `curl -I` shows none).
  Left unanswered on purpose until Rust coverage is measured: `test_statement_coverage90`, `test_branch_coverage80`,
  and `dynamic_analysis` (the criterion accepts an automated suite with 80 percent branch coverage).
- `small_tasks` is only true once `~/Code/unenverse-good-first-issues.ps1` has run: it creates the `good first issue`
  label and seven issues taken from the open findings of phase log 83/84.

### Files added or changed (repository)

- `ASSURANCE_CASE.md` (new): security requirements, trust boundaries, secure design principles, common weaknesses with
  the enforcing place. A path-traversal row was removed because it could not be verified.
- `scripts/spdx.mjs` (new) and `package.json` (`spdx:check`, wired into `check`): adds and verifies
  `SPDX-License-Identifier: Apache-2.0` plus `Copyright the UnENVerse contributors.` on every tracked .rs/.ts/.js/.mjs/
  .py/.sh outside `tests/fixtures`, `docs/`, `node_modules`, `dist`, `target`. 450 files after the merge (369 before it).
- `CONTRIBUTING.md`: DCO 1.1 via checkbox, "Good first issues", "How review works" (five checks).
  `.github/pull_request_template.md`: DCO checkbox.
- `book/src/roadmap.md` (full year plan), `website/roadmap/index.html` (generated from it by a one-off script, so the
  two can drift), README `## Roadmap` section plus contents entry, sidebar and footer links, `NAV` entry in
  `scripts/build-site.mjs`.
- Trust: README badge row (Silver %, Gold %, Baseline 3 %, TypeScript coverage, Rust coverage), the Codecov tree graph
  under "Continuous integration", `website/security/index.html` "Trust at a glance", the same `.badges` row on the home
  page and the download page, `.badges` rule in `website/styles.css`.
- CI speed: `Cargo.toml` (dev-profile optimisation of argon2, blake2, scrypt, pbkdf2, sha2, hmac, chacha20poly1305,
  curve25519-dalek), `.github/workflows/ci.yml` (per-crate test selection), `.github/workflows/build.yml` (desktop and
  headless packaging no longer wait for the gate), `tests/workflows.test.ts` (classifier behaviour tests and the changed
  gate test), `.github/workflows/coverage-rust.yml` (new), `codecov.yml` (flags with carryforward).
- Version 0.56.1 in every copy `npm run version` owns, and a `0.56.1` section in `CHANGELOG.md`.

### Verification that ran

- `npm run -s format:check`: clean. `npm run -s lint`: clean. `npm run -s spdx:check`: "All 450 source files have a header".
- `npx vitest run tests/workflows.test.ts`: 29 passed. `tests/site-build.test.ts`: 20 passed.
- Site: `node scripts/build-site.mjs --out <scratch>` then `node scripts/check-site.mjs`: "site checks ok: 10 pages x 2 viewports".
- `actionlint .github/workflows/ci.yml build.yml coverage-rust.yml`: clean apart from the SC2034 warnings in
  `exporters.yml` that were already there.
- `cargo test -p unv-cli --test contract_users --locked`: 111.11 s before the Cargo.toml change, 9.12 s after.
- Classifier proof (ran the real `ci.yml` step in bash on sample file lists): `unv-cli/..` -> `-p unv-cli`;
  `vault-core/..` and `Cargo.lock` -> `--workspace`; `unv-server/..` -> `-p unv-server -p unv-cli -p unenverse`;
  TypeScript only -> no Rust tests; README only -> nothing.
- Before the SPDX headers: `cargo fmt --all --check` clean, `npm run typecheck` clean, six source-reading test files
  passed (97 tests).

### Not run

- `cargo test --workspace`, `cargo llvm-cov`, `npm test` (full), `npm run preflight`, ui-lab, any GitHub Actions run of
  the changed `ci.yml`, `build.yml` or the new `coverage-rust.yml`. The expected effect of the Cargo.toml change on the
  whole Rust job (45 min to roughly 5 to 8 min) is an extrapolation from one test file.
- The silver and gold forms, the good-first-issues script.

## The bugs that mattered

1. **The Rust CI job took about 42 minutes of test time** (Test ubuntu 14:51 to 15:36). Cause: debug-build Argon2id
   (64 MiB, t=3) in every test that unlocks a vault; 35 contract files each ran 8 to 12 tests at 5 to 12 s. Fixed by
   optimising only the key-derivation crates in the dev profile (release unchanged, overflow checks and debug assertions
   kept). 12 times faster on the one file measured.
2. **A release took CI plus the slowest build.** `desktop` and `headless` had `needs: [meta, ci]` although they publish
   nothing, so packaging started after a 45 minute gate. Now `needs: [meta]`; `docker` and `release` still need `ci`.
   v0.56.0 timeline for reference: gate 14:49 to 15:36, Windows CLI finished 15:54, release 15:57 (68 minutes).
3. **Every change ran every Rust test**, including TypeScript-only and single-crate changes. Now selected per crate.
4. **My earlier work disappeared from the working tree**: the maintainer ran `git stash` ("before-main-pull", including
   untracked files) before pulling main. Recovered with `git stash push` of today's three edits, `git stash pop stash@{1}`,
   `git checkout -- tests/workflows.test.ts` (it only held a header and blocked the second pop), second pop, then
   `node scripts/spdx.mjs --write` for the 81 files that came in with the merge.

## Decisions made

- **DCO as a checkbox, not `Signed-off-by`**: the maintainer's commits are one sentence and he runs them himself; forcing
  `-s` would change that. Sign-off is welcome, the checkbox is the mechanism (CONTRIBUTING.md).
- **Honest Unmet over aspirational Met**: bus factor, unassociated contributors, two-person review, reproducible build,
  hardened site headers, signed tags, algorithm agility and i18n are all Unmet. The roadmap names the path for each.
- **`crypto_used_network` answered Met** because `unv-server` binds `127.0.0.1` by default and plain HTTP is only used
  on loopback (a node refuses a plain-HTTP hub unless loopback). Refusing a non-loopback bind without `--tls` was
  considered and rejected: it would break the maintainer's own container deployment.
- **Rust coverage figure**: the newest saved measurement is 78.86 percent lines (43,167 of 54,738) from
  phase log 86 (src-tauri went from 7.8 percent to tested, about 20 more contract files). No later figure exists; the
  `run4` attempt was killed (exit 143). Measuring now happens on a runner, not locally (maintainer's rule: no long local runs).
- **Per-crate selection rule**: vault-core -> whole workspace; unv-app -> unv-app, unv-server, unv-cli, unenverse;
  unv-server -> unv-server, unv-cli, unenverse; unv-cli alone; src-tauri -> `unenverse`; anything not clearly one crate
  or clearly TypeScript (manifests, lockfile, `.github`, `scripts`, `tests/fixtures`, Docker, catalogue) -> whole workspace.
  The package for `src-tauri` is `unenverse`.
- **Release publishing still needs the gate**; only the no-publish packaging jobs run beside it. Test in
  `tests/workflows.test.ts` rewritten accordingly (two tests: publishing needs `ci`; packaging must not wait for it).
- **Trust badges are live dynamic badges** (shields.io reading `bestpractices.dev/projects/15623.json` and Codecov), so no
  number is typed anywhere. The Codecov tree graph token in the README is a read-only graph token the maintainer supplied;
  the upload token (`CODECOV_TOKEN`) is separate and still needs setting and rotating.
- No ADR written: nothing here changes a design. `.claude/rules/ci-release.md` got the permanent CI notes.

## What Worked / What Didn't Work

prefilled-URL method; timing one slow test file before and after a profile change; extracting the real `ci.yml` step into
bash and into a vitest to prove the classifier; `actionlint` and `check-site.mjs` as fast gates.

Did not work / traps:

- The first `python3` edit of `coverage-rust.yml` ran after the stash had removed the file and aborted before touching
  `codecov.yml`; redone after the restore. Check files exist after any stash.
- `check-site.mjs` rejects an `<img>` without `width` and `height`; shields badges need `width="160" height="28"` plus CSS
  `width:auto`.
- `actionlint` prints SC2034 for `exporters.yml` (pre-existing); filter them when reading output.
- A dynamic shields badge shows "invalid" until its service has data (Rust coverage before the first upload).
- `git add` globs: `'*.ts'` etc. must exclude `tests/workflows.test.ts` from the headers group, which belongs to the CI
  group (it holds both).

## Next Steps

Only the maintainer can:

1. Run the commit command at the bottom of this section's companion message, merge the PR (rebase), and let CI cut v0.56.1
   (no `v0.56.1` tag exists and the version moved, so `build.yml` publishes it).
2. After merge: `gh workflow run coverage-rust.yml --repo darthdemono/UnENVerse`, read the job summary, and send the Rust
   numbers back so the silver `test_statement_coverage80` and gold `test_statement_coverage90`, `test_branch_coverage80`,
   `dynamic_analysis` answers can be set. TypeScript is 99.52 statements and 95.35 branches (phase log 85).
3. Submit the forms: `pwsh -File ~/Code/bestpractices-silver.ps1` and `...-gold.ps1` (review each tab, press Submit), then
   `pwsh -File ~/Code/unenverse-good-first-issues.ps1`.
4. Set and rotate `CODECOV_TOKEN` (pasted into chat earlier); close Dependabot PR #28 (age 0.12.1); enable Dependabot
   security updates; sign release tags (needs his GPG or SSH key) to move `version_tags_signed`.
5. Put the site behind a CDN that can set security headers if `hardened_site` matters (his account).

Doable by an agent next:

1. Confirm the faster pipeline on the first real PR: read the `Changes` job output (`crates=`), the Test job time, and the
   release run's overlap of gate and Windows builds. Tune the optimised-crate list if the Test job is still slow.
2. A fuzz job (`cargo fuzz` 60 s per target) as a release gate, to flip `dynamic_analysis`.
3. Reproducible build check (`SOURCE_DATE_EPOCH`, `--remap-path-prefix`, build twice and compare) for `build_reproducible`.
4. Make `website/roadmap/index.html` generated from `book/src/roadmap.md` so they cannot drift.
5. Roadmap phases themselves (R21 first), see `book/src/roadmap.md`.

## Notes for whoever picks this up

- The roadmap dates and quarter assignment of phases are my proposal (the dependency order comes from
  `the session records/roadmap/phases/*.md` "Depends on"); R18 to R21 and R24 to R26 have no stated dependency and were placed in Q4 2026.
- `package-lock.json` still says 0.42.7; `npm run version` does not own it and `version:check` does not flag it.
- `ASSURANCE_CASE.md` claims were checked against `tls.rs`, `tauri.conf.json`, `THREAT_MODEL.md`, `AGENTS.md`
  invariants and the README; re-verify before citing it elsewhere.
- Silver `documentation_current` and `regression_tests_added50` are attestations about practice, not checked by a tool.
- Do not commit this file. Unset `UNV_SERVER_URL` and `UNV_PASSWORD` before running `unv` by hand.

> 2 line(s) about the maintainer's own machines were left out of this page.
