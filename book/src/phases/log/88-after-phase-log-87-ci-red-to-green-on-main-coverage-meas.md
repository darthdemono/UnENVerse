# Phase log 88: after phase log 87, CI red-to-green on main, coverage measured, badges, forms ready

Date: 2026-10-18 (evening). Continues phase log 87 (which described the uncommitted state). Everything here is merged to
`main` (head `4e545b1`) unless listed under Next Steps. Version 0.56.1 is released and the website shows it.
Handoffs are gitignored; never put this file in a commit line.

## Goal

Land the phase log 87 work, keep CI honest while it got 5 times faster, get real Rust coverage numbers without a local
run, fix the README badges and the website release lag, and get the OpenSSF Silver and Gold forms ready to submit.

## Current Progress

Merged PRs (all by rebase, after `CI result` was green): #37, #38 (the maintainer, tests), #40 (speed-up, headers, roadmap,
assurance case, 0.56.1), #43 (rules-link fix, badge regrouping, nightly branch-coverage job, stall-test bound),
#44 (IPC test isolation), #45 (README header fix).

### Measured, from CI (not local)

- Rust lines 85.07 percent (46,637 of 54,825), regions 83.38, functions 74.79 (stable `cargo llvm-cov`, run 39150314288).
- Rust nightly `--branch` (run 39160284093): branches **66.06 percent (4,956 of 7,502)**, lines 83.91, regions 81.61.
- TypeScript (phase log 85): statements 99.52, branches 95.35. Codecov shows its own blends: all 89, TypeScript 96, Rust 87.
- CI timings on PRs after the speed-up: Test ubuntu 6m14s to 7m06s (was about 45 min), Test windows 10m37s to 11m02s
  (was 33 to 45), Frontend 5.5 to 7 min, Lint 2 to 8 min.
- v0.56.0 release took 68 minutes before the changes (gate 47 min, then Windows builds).

### What changed since phase log 87 (beyond what it listed)

- `build.yml`: new last job `site` dispatches `docs.yml` once the release is public. Reason: the site stamps its version
  from the latest release at docs run time, which ran at the merge push, before the release existed, so the site showed the
  previous version until the Monday schedule. A release made with GITHUB_TOKEN triggers no other workflow, but
  `workflow_dispatch` is allowed. Needs `actions: write` on that job only. Confirmed working: the site showed 0.56.1.
- `coverage-rust.yml`: job `branches` (nightly, `--branch`) added; runs on dispatch and weekly only. Artifacts:
  `rust-coverage` and `rust-branch-coverage`.
- README header: three badge rows (health; coverage all, TypeScript, Rust; OpenSSF trust), the caption was removed at the
  maintainer's request, and a blank line after the last `</p>` is required or GitHub does not render the bold tagline.
  The website uses the same labels and a `.badges` CSS rule.
- `CONTRIBUTING.md` now holds the ten rules inline ("Rules every change must respect") because `AGENTS.md` is gitignored
  (`.gitignore:69 AGENTS*.md`) and every GitHub link to it was a 404. `ASSURANCE_CASE.md` points at those rules.
  The already-submitted Passing and Baseline justifications still cite AGENTS.md; optional to edit.
- `~/Code/unenverse-good-first-issues.ps1` rewritten with single-quoted here-strings. The first version failed in
  PowerShell: a backtick followed by `u` inside double quotes is a Unicode escape (`` `unv ``).
- `~/Code/bestpractices-silver.ps1` (47 answers, 4 tabs) and `bestpractices-gold.ps1` (18 answers, 1 tab) regenerated with
  measured coverage. Silver: 38 Met, 3 N/A, 6 Unmet. Gold: 11 Met, 7 Unmet.

## The bugs that mattered

1. **Windows health-stall test failed twice on main** (`creating_users_does_not_stall_health`). After the crypto crates
   became optimised, ten user creations ran in a fraction of a second, so one 115 ms scheduling hiccup was 0.53 of the run;
   with 60 creations a two-core Windows runner still saw worst probe 725 ms, share 0.557. It is CPU contention (two KDF
   threads, two workers, the test thread on two cores), not inline hashing. Fixes: 60 creations, and a separate
   `CREATE_STALL_SHARE = 0.8` for this test only (the login test keeps 0.4 and passes). Mutation check with `run_kdf`
   forced inline: 0.943 to 0.970, so the test still catches the bug. Measured offloaded: 0.13 to 0.20 on four cores.
2. **Test binaries crashed on exit with SIGSEGV after all tests passed** (`contract_users`, Ubuntu, once). The in-process
   server harness (`unv-cli/tests/common/server.rs`) signalled "stopped" before its runtime was dropped, so the process
   could exit mid-teardown. Now `drop(rt)` before `done_tx.send`. Intermittent, so not provable; 12 local runs of the
   binary had 0 failures both before and after.
3. **src-tauri IPC test "Wrong master password"** (`ipc_session`, Ubuntu, once). All desktop tests share one data
   directory that each test wipes; most tests end unlocked, leaving the operations executor running with the old key, which
   can recreate the database in the wiped directory. `Desk::drop` now calls `lock_vault` (stops and joins the executor and
   the LAN server) before releasing the mutex guard. Three local runs of the three IPC binaries were green.
4. **Website lagged the release** (see above; fixed by the `site` job).
5. **README tagline not bold**: an HTML block (`</p>`) followed directly by a markdown line is swallowed by the HTML block.
6. **Broken links to the ignored AGENTS.md** in CONTRIBUTING and my assurance case.

## Decisions made

- Rust branch coverage is Unmet for gold `test_branch_coverage80` (66.06 percent), reported honestly even though the two
  languages combined are about 82.9 percent; the criterion is per language.
- `CREATE_STALL_SHARE` 0.8 rather than restoring slow hashing for one test: slow hashing cannot be selected per test
  because the optimisation is a Cargo profile setting.
- Merged PRs myself only after the maintainer reported green and, earlier, explicitly asked ("handle the merging, and
  pulling"); used `gh pr merge --rebase`, never `--admin`.
- A rebase merge changes SHAs, so a branch that already landed shows its old commits again in the next PR; GitHub drops
  the already-applied patches. Not a problem, just noisy.

## What Worked / What Didn't Work

Worked: reading failures from the run log zip (`gh api repos/R/actions/runs/ID/logs` > zip; job logs are empty or cancelled
while a run is in progress, and `gh run view --log` can die with a stream CANCEL on big logs); mutation checks on timing
tests; downloading `summary.json` artifacts instead of reading logs; `check-runs/<job>/annotations` for the exit code.
Did not work: assuming a larger sample fixes a timing test on a 2-core runner; `gh run view --job ... --log` on large jobs.

## Next Steps

Only the maintainer can:

1. Run `pwsh -File ~/Code/unenverse-good-first-issues.ps1` (0 issues carry the label so far; GitHub's default label exists),
   then `bestpractices-silver.ps1` and `bestpractices-gold.ps1` and press Submit on every tab.
2. Silver cannot be awarded until two MUST criteria are Met: `access_continuity` (a second person with release keys and
   admin access) and `build_repeatable`. Gold also needs people (`bus_factor`, `contributors_unassociated`,
   `two_person_review`).
3. Set and rotate `CODECOV_TOKEN`; close Dependabot PR #28 and review the open Dependabot PRs #20, #25, #31, #33, #39, #41;
   sign release tags (your key); consider a CDN for security headers (`hardened_site`).

Doable by an agent next:

1. `build_repeatable` / gold `build_reproducible`: CI job that builds `unv` twice with `SOURCE_DATE_EPOCH` and
   `--remap-path-prefix` and compares hashes (the only Silver blocker that needs no other person).
2. Raise Rust branch coverage (66 percent) and lines (85 percent) toward the gold coverage criteria; the per-file gaps are
   in the `rust-branch-coverage` artifact and on Codecov.
3. Fuzz as a release gate for `dynamic_analysis`; generate `website/roadmap/index.html` from `book/src/roadmap.md`.
4. The roadmap phases themselves, R21 first (`book/src/roadmap.md`).

## Notes for whoever picks this up

- A "success" on one job is not a finished release: the GitHub Release is created only after every build job, and the
  Windows builds are the long pole (about 17 to 19 minutes).
- Do not put docs links to gitignored files (`AGENTS.md`, `CLAUDE.md`, `the session records/`, `review/`) in public files or forms.
- Silver and gold generators lived in the session scratchpad; the `.ps1` files are the artefact. Rebuilding them means
  re-deriving the answer table from phase log 87 and this file.
- The `ubuntu-latest` label migrates to Ubuntu 26 on 2026-10-19 (runner notice); watch the first CI run after that date.
