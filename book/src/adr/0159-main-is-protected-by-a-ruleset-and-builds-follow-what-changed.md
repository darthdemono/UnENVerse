# ADR-0159: Main is protected by a ruleset and builds follow what changed

Status: accepted

## Context

Public trust checks (OpenSSF Scorecard, Best Practices baseline) ask whether direct commits to the primary branch are prevented, whether status checks gate merges and whether code scanning blocks new findings. With one maintainer, a ruleset that does all of that would normally mean every documentation edit waits for an 18 minute Windows test run. A required check that never starts (because the workflow was path-filtered away) leaves a pull request waiting forever, so the usual fix of putting `paths:` on the CI trigger cannot be used.

## Decision

The `protect-main` repository ruleset (id 25308031) blocks deletion and force pushes, requires a pull request with zero approvals, requires one check named `CI result`, and applies a CodeQL code-scanning rule at medium or higher. The repository admin may bypass only through a pull request (`bypass_mode: pull_request`), so a direct push is rejected for the maintainer too, but a pull request can still be merged when CI is broken.

`ci.yml` decides what to run inside a `changes` job, not with trigger filters. It compares the pushed or proposed commits with their base: documentation-only changes (policy files, README, licence, screenshots, issue templates) run nothing, `website/`-only changes run the frontend job but not the Rust jobs, and everything else runs everything. The release gate (`inputs.gate`), manual runs and a weekly schedule always run everything. A final `CI result` job runs with `if: always()` and fails on any failed or cancelled needed job while accepting skipped ones; it is the only required check.

`build.yml` and `docs.yml` use `paths-ignore` on push. GitHub skips a push only when every changed file matches, so a version bump (which touches version files) still starts the release workflow. `website/` is ignored by `build.yml` (a site change is never a release) and not by `docs.yml` (which deploys it). Releases remain manual: they happen only when the version is bumped and no `v<version>` tag exists.

## Consequences

Every change reaches `main` through a branch and a pull request; the one-line commit helpers must create the branch from `origin/main`. The two checks a human cannot satisfy alone stay open: `OSPS-QA-07.01` (non-author approval) and the Scorecard review items. Baseline level 3 therefore stops at 20 of 21 controls until a second person with write access exists. `tests/workflows.test.ts` pins the `changes` and `CI result` jobs, the release-gate behaviour and the ignore lists, and each guard was checked by removing it. Adding `paths:` to `ci.yml`'s `pull_request` trigger would reintroduce the pending-check deadlock and must not be done.
