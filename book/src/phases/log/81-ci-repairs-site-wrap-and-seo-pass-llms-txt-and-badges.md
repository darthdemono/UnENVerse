# Phase log 81: CI repairs, site wrap and SEO pass, llms.txt and badges

zip 9 advisory fix, Windows update-test fix, health stall tests judged by waiting share, site wrapping and SEO head tags, llms.txt, agents.txt, security.txt, README badges, alert 3 dismissed.

## CI repairs, site wrap and SEO pass

Date: 2026-10-17, after `80-phase-54-cli-updater-and-code-scanning-cleanup.md` (Phase 54 CLI half). Version 0.55.1 at the
end (the maintainer bumped it). Everything here is committed by the maintainer;
`git status` was clean at the end.

## Goal

The maintainer reported CI failures one after another ("already failed", "2 became
3", "now 4", "1 more failed task"), then asked for: the website's odd line breaks
fixed, `agents.txt` / `llms.txt`, SEO improvements, README badges, the footer's
"More from DarthDemono" line replaced by website, GitHub and LinkedIn links, and
the one remaining code-scanning alert closed.

## Current Progress

CI failures on main, in the order they appeared (each fixed and re-checked):

1. **cargo-deny, Lint (ubuntu).** `zip` 7.2.0 had RUSTSEC-2026-0367/0368 and a
   third advisory (extract symlink and permission issues), fixed in >= 9.0.0.
   Bumped `unv-app` to `zip = "9"` (locks 9.0.4). zip 9 makes `ZipFile::name()`
   fallible: `unv-app/src/password_sources.rs` now maps it with a named error.
   Verified: `cargo deny check advisories` ok, clippy clean, `cargo test -p unv-app` ok.
2. **Test (windows), update tests.** My Phase 54 tests built archives holding
   `unv`, but the extractor looks for `unv.exe` on Windows. Tests now use a `bin()`
   helper (`unv-app/tests/update.rs`).
3. **Test (windows), `creating_users_does_not_stall_health`.** 244 ms against a
   200 ms worst-probe bound. First fix (a 750 ms constant) failed again: 2.85 s on
   the next run, while one hash takes about 1.8 s on that runner, so no worst-case
   time bound separates "hashing inline" from "starved two-core runner". Final fix:
   `stall_share` = sum of sequential probe latencies / run time, bound
   `HEALTH_STALL_SHARE = 0.4` (`unv-server/src/lib.rs`). Measured: offloaded 0.09 to
   0.13; Windows starvation spike about 0.16; `run_kdf` forced inline 0.82 (both
   tests fail). Mutation run and restore confirmed; 92 `unv-server` lib tests pass.
   A median bound was tried first and rejected: inline hashing gives one huge probe
   among many fast ones, so the median stays tiny (the unlock path passed the mutant).
4. **Not fixed, by design:** the Dependabot PR for `age` 0.12.1 fails with `kem`
   trait errors (a real API break). The maintainer held that upgrade back in the
   changelog commit. Needs a port of our `age` usage.

Code scanning: alert 3 (`js/insecure-randomness`, `render.ts:1312`) is a false
positive (no `Math.random` in `src/`; only test id helpers), dismissed through the
API with a comment. 0 open alerts. Earlier dismissals are in `80-phase-54-cli-updater-and-code-scanning-cleanup.md`.

Website (`website/`, `scripts/build-site.mjs`):

- `styles.css`: `text-wrap: balance` on headings, `pretty` on p/li/figcaption,
  `white-space: nowrap` for inline code in p/li (normal under 640 px), the hero
  terminal no longer wraps (`white-space: pre; overflow-x: auto`) and its lines were
  shortened in `index.html` so they fit. Checked by screenshots at 1440 and 390 px.
- `seo(html, urlPath)` in the build script adds, idempotently: robots, author,
  `rel="me"` links (website, GitHub, LinkedIn), og site name/locale/image size/alt,
  Twitter title/description/image derived from the page itself, an `llms.txt`
  alternate link and a BreadcrumbList. noindex stubs are untouched. `jsonLd` now has
  author (Person with `sameAs`), image, description and `featureList`. Tests in
  `tests/site-build.test.ts`.
- New files: `website/llms.txt`, `website/agents.txt` (no settled standard; the file
  says so), `website/.well-known/security.txt` (the maintainer later also serves it
  from the site root because Pages drops dot folders, commit 5f3e39a), `robots.txt`.
- Footer: "By Jubair Hasan / DarthDemono: Website, GitHub, LinkedIn"
  (darthdemono.com, github.com/darthdemono, linkedin.com/in/darthdemono; found in
  the GitHub profile's social accounts).
- README: badge row (CI, CodeQL, release, downloads, licence, platforms, Rust and
  Tauri, website); the maintainer restyled and extended them afterwards.

Verification: `npm run check` exit 0, 1567 Vitest tests; `site:build` and `site:check`
9 pages x 2 viewports; `cargo test -p unv-server --lib` 92 passed. Not run: any
GitHub workflow (only the maintainer's pushes run them), the Windows jobs locally.

## The bugs that mattered

1. A vulnerable `zip` 7.2 on an extraction path (release gate red).
2. A worst-case latency assertion that could not distinguish a healthy build from a
   starved runner (release gate red twice).
3. Platform-specific file name in the update tests (Windows red).

## Decisions made

- Assert on the share of a run spent waiting, not on one probe (rationale above).
- Dismiss alert 3 as a false positive rather than add code to satisfy CodeQL.
- Put SEO tags in the build step (`seo`) instead of editing every page.
- `agents.txt` is a plain statement of what agents may do and where the machine
  contract is (`unv describe`); it makes no claim to be a standard.

## What Worked / What Didn't Work

Worked: reading the failing job log with `gh api --allow-escape-sequences .../logs`
and stripping colour codes; mutation-checking the fix by inlining `run_kdf`.
Did not: a fixed time bound (twice); median instead of share; `gh run view --log-failed`
while the run was still in progress (it refuses).

## Next Steps

- Maintainer: pin the update public key and set the `UNV_UPDATE_KEY` secret to
  activate the CLI update feed (see `80-phase-54-cli-updater-and-code-scanning-cleanup.md`); hand over the Tauri update key
  so the desktop updater can be built; push and watch the Windows job once more.
- Port our `age` usage to 0.12 and close the Dependabot PR (#28).
- Website install and update text once the update feed exists.

## Notes for whoever picks this up

- Build & Release re-runs the whole CI gate; a red gate skips every publish job.
- The site stamps the version of the latest published release, not the repo version,
  so `site:build` showing 0.54.1 while the repo says 0.55.x is expected.
- Never put `the session records/` files in a commit line; the directory is gitignored.
