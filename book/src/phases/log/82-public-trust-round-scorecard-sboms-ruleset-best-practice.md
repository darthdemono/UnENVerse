# Phase log 82: Public trust round: Scorecard, SBOMs, ruleset, Best Practices and Baseline badges

OpenSSF Scorecard workflow, release SBOMs and provenance, pinned Docker bases, coverage floor and Codecov upload, community and policy documents, OpenVEX, protect-main ruleset, docs-only CI skipping, security.txt move, Best Practices passing and Baseline 1 and 2 earned, Baseline 3 at 95 percent.

## public trust round

Date: 2026-10-17, after `81-ci-repairs-site-wrap-and-seo-pass-llms-txt-and-badges.md` (CI repairs, site wrap and SEO pass). Version
0.55.1 at the end; v0.55.1 is published and is the first release with SBOMs. Everything
below is committed and pushed by the maintainer (commits listed under Current Progress),
except the items in "Not mine and not committed" at the end of this file.

## Goal

The maintainer asked whether to adopt Codecov for the repo. That became: get public
approval for a secrets manager. We settled on public security evidence (Scorecard,
signed and attested releases with SBOMs, policy documents, branch protection) rather
than a coverage percentage, then earned the OpenSSF Best Practices "passing" badge and
the Baseline series. Mid-way the maintainer added: restyle the badges like their profile
README, remove the four Scorecard vulnerability findings, stop README or website pushes
from costing a full CI run, and (with "do everything for the full score") the Baseline
forms. The goal changed twice: Codecov first became a minor item (the token and badge
only), and "full score" turned out to be capped at Baseline 3 = 20 of 21 for a single
maintainer.

## Current Progress

Commits (all on main, all by the maintainer through the one-liners I handed over):

- `c4a6d35` Scorecard workflow, attest and list release assets (also deleted the tracked
  `unv-cli/examples/__pycache__/*.pyc`, which was already staged).
- `4c75cdd` Vitest coverage floor and per-run coverage artifact.
- `8bea3bf` CONTRIBUTING, CODE_OF_CONDUCT, issue forms, PR template, Scorecard badge,
  SECURITY.md links.
- `f61b5d3` Docker bases pinned by digest, CodeQL token narrowed, pyc ignored,
  `curl | python` removed from `exporters.yml`.
- `983d3cf` Updated to 0.55.1 (the maintainer's bump; first release with SBOMs).
- `7853b0b` Codecov upload step and `codecov.yml`.
- `5a50156` `osv-scanner.toml` and the first badge restyle.
- `ba20844` CHANGELOG rewritten with versioned sections 0.42.7 to 0.55.1, Dependabot
  ignore for `age` minor bumps.
- `8a1de20` Best Practices badge in the README.
- `be4598a` `changes` job, `CI result`, `paths-ignore` on build.yml and docs.yml,
  `tests/workflows.test.ts` guards.
- `1b4c6f8` GOVERNANCE, ARCHITECTURE, THREAT_MODEL, SECURITY and CONTRIBUTING policy text.
- `2e58535` `scripts/gen-vex.py`, `vex.openvex.json`, VEX shipped in releases, README
  Project section.
- `5f3e39a` security.txt moved to `website/security.txt`.
- `cc40636` and `061976a` Baseline badge, then restyled.

Files and why:

- `.github/workflows/scorecard.yml` (new): OpenSSF Scorecard (ossf/scorecard-action
  v2.5.0 pinned by SHA), weekly and on push to main, `permissions: read-all` at the top,
  `security-events: write` and `id-token: write` only on the job, SARIF upload with the
  already pinned codeql-action `upload-sarif` (v4.38.3 SHA).
- `.github/workflows/build.yml` release job: new "Generate SBOMs" step before assembling
  assets (cargo-cyclonedx 0.5.9 for `unv-cli`, `unv-server`, `src-tauri`; `npm sbom
--omit dev` for the frontend; each output checked with `jq -e '.bomFormat ==
"CycloneDX"'`), `cp sbom/*.json` and `cp vex.openvex.json` into `release-assets/`,
  `actions/attest-build-provenance` (v4.2.2 SHA) placed BEFORE cosign so `.sig`/`.pem` are
  not attested, `attestations: write` added, release body gained `gh attestation verify`
  and an SBOM and VEX paragraph. Also `paths-ignore` on push (README, policy files,
  licence, screenshots, issue templates, `website/**`), with the long header comment
  corrected: `paths-ignore` skips a push only when EVERY changed file matches, so a version
  bump still runs.
- `.github/workflows/ci.yml`: `changes` job (bash classifier, outputs `rust` and
  `frontend`), `needs: changes` and `if:` on `test`, `lint`, `frontend`, a new `ci-ok` job
  named `CI result` (`if: always()`, fails on any result other than success or skipped),
  weekly `schedule` so `cargo deny` still runs without code changes, coverage step
  (`npm run test:coverage`), coverage artifact upload, and a Codecov step that runs only
  when job-level `CODECOV_TOKEN` is non-empty (job-level because a step's own `env` is not
  visible to its own `if`).
- `.github/workflows/docs.yml`: `paths-ignore` for policy and community files; `website/`
  is deliberately NOT ignored because this workflow deploys the site.
- `.github/workflows/codeql.yml`: `security-events: write` moved from workflow to job.
- `.github/workflows/exporters.yml`: two `curl | python3` pipes became `curl -o file` plus
  `jq` (Scorecard "downloadThenRun").
- `Dockerfile`, `tools/viewer/Dockerfile`: `rust:1.90-bookworm@sha256:3914072c...` and
  `debian:bookworm-slim@sha256:7c7b2c96...` (index digests from `skopeo inspect --raw`).
- `.github/dependabot.yml`: docker ecosystem now `directories: ["/", "/tools/viewer"]`;
  `age` minor bumps ignored with the reason (see What Didn't Work).
- `vitest.config.ts`: reporters `text, html, lcov, json-summary`; thresholds statements 71,
  branches 60, functions 73, lines 74 (measured 71.85 / 61.53 / 73.63 / 74.24).
- `codecov.yml`: informational only; the enforced gate is the Vitest floor.
- `osv-scanner.toml` (new): four documented ignores (RUSTSEC-2023-0071 rsa,
  RUSTSEC-2024-0370 proc-macro-error, RUSTSEC-2026-0173 proc-macro-error2, RUSTSEC-2024-0429
  glib). Scorecard honours it. `deny.toml` stays the source of truth for cargo-deny.
- `scripts/gen-vex.py` and `vex.openvex.json`: OpenVEX document (11 statements,
  `not_affected`) generated from `deny.toml` plus `osv-scanner.toml`; not hand written.
- `GOVERNANCE.md`, `ARCHITECTURE.md`, `THREAT_MODEL.md`, `SECURITY.md`, `CONTRIBUTING.md`:
  see Decisions. `CHANGELOG.md`: versioned sections for 0.42.7, 0.43.0, 0.44.x, 0.54.0,
  0.54.1, 0.55.0, 0.55.1, plus Unreleased; the Added bullets that sat under Unreleased now
  sit under 0.54.0.
- `README.md`: centered, dark for-the-badge badge block like the profile README (CI, CodeQL,
  Scorecard, Best Practices, Baseline, Codecov, Release, Downloads, then platforms, Rust,
  Tauri, licence, website), and a Project links section.
- `website/security.txt` (moved from `website/.well-known/`).
- `tests/workflows.test.ts`: new describe "documentation-only changes do not run the
  build" (5 tests).
- `book/src/adr/0159-main-is-protected-by-a-ruleset-and-builds-follow-what-changed.md`
  (this session's ADR) and its row in `book/src/SUMMARY.md`.
- `~/Code/unenverse-ruleset.json` (outside the repo): the ruleset body, applied by the
  maintainer. `~/Code/bestpractices-baseline.ps1` (outside the repo, generated): opens
  three prefilled Baseline forms.

Verification (commands and summary lines):

- `npm run check` (format, lint, typecheck, Vitest): 92 test files, 1,567 tests passed
  at the start of the session.
- `npx vitest run --coverage`: All files 71.85 / 61.53 / 73.63 / 74.24. With
  `statements: 99` it exited 1 with "Coverage for statements (71.85%) does not meet global
  threshold (99%)"; reverted. Then exit 0.
- `npx vitest run tests/workflows.test.ts`: 19 passed before, 24 passed after the new
  tests. Mutations checked: aggregator accepting `cancelled`, `package.json` added to the
  ignore list, the `rust` gate removed from `test` each produced "1 failed | 23 passed";
  restored, 24 passed.
- Classifier (bash extracted from ci.yml, run in a scratch git repo with output outside the
  repo): README only -> rust=false frontend=false; README + screenshots + issue template
  -> false/false; policy docs only -> false/false; website only -> rust=false frontend=true;
  website + README -> false/true; `src/a.ts` -> true/true; README + source -> true/true;
  `package.json` bump -> true/true; workflow file, `Cargo.toml` -> true/true; release gate,
  dispatch, schedule, zero-sha new branch, unknown base commit -> true/true.
- Aggregator (`ci-ok` run block, `RESULTS` env): "success success success success" exit 0,
  "success skipped skipped success" exit 0, any `failure` or `cancelled` exit 1.
- `actionlint .github/workflows/*.yml`: clean except pre-existing SC2034 (unused retry
  loop variable `i`) in `exporters.yml`.
- `scorecard --repo=github.com/darthdemono/UnENVerse` (remote): 6.3 at the start of the
  session (Vulnerabilities 6, Pinned-Dependencies 8, Token-Permissions 9, Security-Policy
  4, Binary-Artifacts 9); 7.5 after the changes were pushed (Branch-Protection 3,
  Code-Review 0, Maintained 0, Contributors 0, CII-Best-Practices 5 then passing,
  Signed-Releases 8). Local `--local .` run: 7.9; Pinned-Dependencies 10,
  Token-Permissions 10, Security-Policy 10, Dangerous-Workflow 10; Vulnerabilities went
  6 -> 10 after `osv-scanner.toml` was added.
- `cargo check -p vault-core --locked` after the ssh-key feature experiment (reverted);
  `cargo test -p vault-core --lib generators`: 3 passed; `cargo check --workspace --locked
--all-targets` finished clean on the reverted tree.
- `npm run format:check`, `npm run lint`, `npm run typecheck` clean after each batch.
- GitHub state: ruleset `protect-main` active (id 25308031); `CI result: success` seen in
  the main CI run `39045900172` (all six jobs success); docs deploy succeeded and
  `https://unenverse.darthdemono.com/security.txt` returned 200 about 150 s after the
  merge; release v0.55.1 lists four `*.cdx.json` assets with `.sig` and `.pem`.
- OpenSSF: Best Practices "passing" earned (project 15623, all five sections full);
  project JSON at the end: `badge_percentage_baseline_1: 100`, `_2: 100`, `_3: 95`,
  `achieved_baseline_1_at 2026-10-17T15:59:12Z`, `achieved_baseline_2_at
2026-10-17T15:59:53Z`, `achieved_baseline_3_at: null`.

What did not run: the `attest-build-provenance` step and the Scorecard action have not been
seen to succeed in CI by me (the SBOM commands were run locally; v0.55.1 did publish SBOMs,
so the release job ran). I did not run `cargo fuzz`. The Codecov upload has not run (no
token). I did not re-run the full `npm run preflight` after the last docs-only commits.

## The bugs that mattered

1. `https://unenverse.darthdemono.com/.well-known/security.txt` was a 404 although the file
   was in the repo and the site build copied it. `actions/upload-pages-artifact` drops
   dot-folders from the artifact. Impact: the published security contact did not exist,
   and the OpenSSF controls cite it. Fixed by serving `/security.txt` (RFC 9116 allows the
   root), commit `5f3e39a`. Checked live.
2. Every push, including a README edit, ran the full CI (about 18 minutes on Windows), a
   Build & Release run and a full Doxygen, rustdoc and mdBook rebuild. Fixed by ADR-0159
   (`changes` job, `paths-ignore`). The reason it is not a trigger `paths:` filter is the
   deadlock described there.
3. Scorecard flagged four RUSTSEC findings as "existing vulnerabilities". Three are
   build-time or Tauri's GTK3 stack and cannot be removed, one (rsa) is reachable only with
   a decryption oracle UnENVerse never exposes. They are documented, not fixed.
4. The restyle PR (#36) conflicted: the branch had been created from the old badge branch
   after that branch had already been rebase-merged, so it carried a commit `main` already
   had under a new hash. Fixed by `git switch -C <branch> origin/main; git cherry-pick`.
5. A multi-line JSON body pasted into pwsh as a here-string with an indented `'@` fails.
   Fixed by writing `~/Code/unenverse-ruleset.json` and handing `gh api ... --input`.

## Decisions made

- **Public evidence over a coverage number.** Codecov alone is the weakest trust signal for
  a secrets manager. Done instead: Scorecard, signed assets with attestations and SBOMs,
  policy files, branch protection, Best Practices. Codecov is kept as an optional badge fed
  by the same lcov, with an enforced Vitest floor that needs no third party. Rejected:
  Codecov as the quality gate (adds a token and a third-party app to a secrets manager's
  CI for no safety gain).
- **Keep RSA.** The SFTP destination (`unv-app/src/destinations.rs`) authenticates with RSA
  keys via russh `best_supported_rsa_hash`; disabling russh's `rsa` feature would break RSA
  key SFTP and still leave three advisories. Rejected: dropping `ssh-key`'s `crypto`
  feature in `vault-core` (it only generates ed25519), which looked like the fix but does
  nothing because russh enables `ssh-key/rsa` itself.
- **Ignore with reasons rather than hide.** `osv-scanner.toml` carries one reason per
  advisory, mirrors `deny.toml`, and `vex.openvex.json` is generated from both so the three
  cannot drift. Rejected: patching a fork of `rsa` with a bumped version number (supply
  chain risk, dishonest), and editing Cargo.lock by hand (cargo re-adds the entry).
- **Ruleset, not classic branch protection**, with `bypass_mode: pull_request` for the
  admin, 0 required approvals, required check `CI result`, CodeQL gate medium or higher
  (ADR-0159). Rejected: requiring approvals (no second human), requiring code-owner review
  (GitHub forbids self approval, it would block every merge), strict up-to-date branches
  and no admin bypass (friction with no Scorecard gain: the next tier needs a reviewer).
- **`CI result` is the single required check** so a skipped matrix job cannot leave a
  required context pending.
- **security.txt at the site root.** Rejected: a custom Pages artifact tar to keep
  `.well-known` (more machinery for the same RFC-compliant result).
- **Baseline answers.** All Baseline 1 and 2 controls Met; Baseline 3 all Met except
  OSPS-QA-07.01 (Unmet with an honest note). Rejected: a bot or second personal account as
  the approver (the control says non-author human, it would be a false record).
  OSPS-QA-04.01 and QA-04.02 are N/A (single repository).
- **The Baseline form was filled with prefilled-link URLs**, built by a script from one
  answer table and opened with `xdg-open`, not typed by a browser agent.
- **The baseline README badge is a shields dynamic-JSON badge** reading
  `badge_percentage_baseline_2` from the project JSON, because the project's own SVG cannot
  be styled. The label says "level 2" by hand.
- **Policy thresholds are drafts for the maintainer** (medium or higher fixed or documented
  before the next release; latest release only is supported; 7 day acknowledgement). They
  are commitments in a public file, so the maintainer owns them.
- **Releases stay manual.** A release needs a version bump and an absent `v<version>` tag.
  No automatic version bump was built (CI committing to main fights the ruleset).
- ADR written: ADR-0159.

## What Worked / What Didn't Work

Worked: reading Scorecard's per-check details to find exactly which lines to fix;
`skopeo inspect --raw | sha256sum` for index digests without pulling images; the
`osv-scanner.toml` file for Scorecard (honoured, 6 -> 10); testing the bash classifier and
the aggregator in a scratch repo before committing; mutation-checking the new workflow
tests; prefilled-link URLs for the 64 Baseline controls (the maintainer confirmed the
badges were earned).

Did not work:

- `cargo update -p ssh-key` and dropping the `crypto` feature: no effect on the lockfile,
  russh keeps `ssh-key/rsa`. Reverted.
- Upgrading `age` to 0.12.1 to clear proc-macro-error2: age 0.12 pulls `ml-kem 0.2.1`, which
  does not compile against `kem 0.3.0` (required by russh 0.64.1, via ml-kem 0.3.2).
  Forcing `kem` to `0.3.0-pre.0` was refused by cargo. Reverted; Dependabot PR #28 would
  have failed, so `age` minor bumps are ignored with a reason. Revisit when age or ml-kem
  agree with russh.
- `scorecard --local .` counts `node_modules/` binaries as Binary-Artifacts 0; use the
  remote run.
- A test harness that wrote its output file inside the scratch repo got it committed by
  `git add -A` and corrupted the first classifier run; output now goes outside the repo.
- `actions/upload-artifact`/Pages: dot-folders dropped (bug 1).
- The first badge restyle branch (bug 4).

## Next Steps

Only the maintainer can do:

1. `gh secret set CODECOV_TOKEN --repo darthdemono/UnENVerse`, then rotate the token in
   Codecov (it was pasted into a chat), then confirm the "Send coverage to Codecov" step
   ran (not skipped) on a main run. Until then the coverage badge reads "unknown".
2. Turn on Dependabot security updates (the repo setting reads "disabled").
3. Triage the open Dependabot PRs (#33, #31, #30, #29, #25 vite 8, #20 cosign-installer 4,
   #17, #16, #15, #14, #13, #4 typescript 7); #28 (age) was closed. Do not merge vite 8,
   typescript 7 or cosign-installer 4 without a full `npm run preflight` and a look at the
   release job.
4. Baseline 3 stays at 95 percent until a second human has write access; then set the
   ruleset to 1 required approval, remove the admin bypass or accept the weaker claim,
   add the person to `GOVERNANCE.md`, and flip OSPS-QA-07.01.
5. Decide on dynamic analysis as a release gate (`cargo fuzz run` 60 s per target in
   `build.yml`); that would flip the Best Practices `dynamic_analysis` answer from Unmet
   to Met.

Doable by an agent next:

- After the next version bump, confirm the release has `vex.openvex.json`, the four SBOMs
  and a build provenance attestation (`gh attestation verify &lt;asset> --repo
darthdemono/UnENVerse`); the provenance step has not been seen working yet.
- When Baseline 3 is reached, change the README badge from `badge_percentage_baseline_2`
  and "level 2" to level 3.
- If the maintainer wants it, skip the Doxygen and rustdoc steps for `website/`-only
  pushes in `docs.yml`.
- Re-check `deny.toml` and `osv-scanner.toml` whenever Tauri, `age` or `russh` move; then
  `python3 -I scripts/gen-vex.py` to regenerate the VEX file.

## Notes for whoever picks this up

- Main is protected: no direct push, not even by the maintainer. Commit one-liners must use
  `git fetch; git switch -c &lt;topic> origin/main; git add -- &lt;paths>; git commit -m '&lt;one
sentence>'; git push -u origin HEAD; gh pr create --fill; gh pr merge --rebase --admin`
  (`--auto` waits for CI instead). Starting a branch from a stale local branch is what
  produced PR #36's conflict. Never run commit or push yourself.
- Never put `paths:` on ci.yml's `pull_request` trigger. The required check is the job named
  `CI result`; renaming that job silently blocks every PR.
- `website/` changes redeploy the site through docs.yml and never release; a release needs
  `npm run version <x.y.z>` and no existing tag.
- `website/.well-known/` must not be recreated: Pages drops dot-folders.
- The policy files (`SECURITY.md`, `GOVERNANCE.md`, thresholds, support window) are public
  commitments; edit them only on the maintainer's word.
- The Baseline and Best Practices answers cite file paths and URLs in this repository. If a
  cited file moves (`SECURITY.md`, `CONTRIBUTING.md`, `GOVERNANCE.md`, `THREAT_MODEL.md`,
  `ARCHITECTURE.md`, `vex.openvex.json`), update the form text too.
- The bypass is a pull-request bypass only; the ruleset can be removed with `gh api
repos/darthdemono/UnENVerse/rulesets/25308031 --method DELETE`.
- MCP memory (entity "UnENVerse (EnvVault) working rules and lessons") and the git and
  process-lesson entities were updated this session; they hold the ruleset id, the CI design
  and the open items. No token or secret was stored anywhere.

### Not mine and not committed

During the session the working tree gained edits I did not make: `src/ts/bundle-import.ts`,
`src/ts/cookies.ts`, `src/ts/copy-profile.ts`, `src/ts/stack.ts`, `src/ts/totp.ts`,
`tests/uid-pane.test.ts` and new `tests/coverage-*.test.ts` files (a coverage-raising effort
from another process or the maintainer). Treat them as that work's own; they are not part of
this handoff and nothing here depends on them. Earlier in the session
`unv-server/src/lib.rs` (the health-stall test) also changed under me; it has since been
committed as `80e3f5a`.
