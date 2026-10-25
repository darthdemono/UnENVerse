# Phase log 68: roadmap R02 (project phase 45), CI release gating and speed

Date 2026-10-10. Start commit `b989266` (0.42.5; the maintainer had pushed R01 and W1 meanwhile). Nothing committed, staged or pushed; no workflow has run on GitHub. Mapping R02 to 45 is in STATUS.md and PHASES.md (next free number is 46).

## Goal

Make it impossible to publish a release from a red commit, give WireGuard, nginx and Compose real validators, and shorten the Windows critical path without weakening a check. Scope items 1 to 9 of `the session records/roadmap/phases/02_ci_release_gating_and_speed.md`. Mid-session the maintainer pasted three failures from the first push (below); those were fixed first.

## The three CI failures from the push of b989266 (fixed, not R02 scope)

1. **Docs workflow, step "Assemble the site": `ENOENT ... website/assets/shots/manifest.json`.** And 2. **Frontend job, test `fills every placeholder, marks the current nav item...`: same error.** Cause: `.gitignore` line 69 is `shots/`, which also ignores `website/assets/shots/`, so the WebP images and the manifest were never committed (`git ls-files website | grep -c shots` was 0; `git check-ignore -v` named the rule). The pasted advice (create an empty manifest, make the build tolerate a missing one) would have hidden a site with no images. Fix: the folder is now `website/assets/screens/` (not ignored), `build-site.mjs`, `site-shots.sh`, `index.html` and the tests follow, and two new tests ("has every local asset a page or the stylesheet names", "has a screenshot manifest that matches the images on disk") fail on a clean checkout exactly as CI did (reproduced by hiding the folder: 3 failures; restored: green). The ignore rule itself is untouched (the maintainer's `.gitignore`; renaming the folder is the robust fix).
2. **CI, Rust (windows-latest): `history::tests::a_save_that_changes_a_config_is_snapshotted_in_the_background` panicked `called Result::unwrap() on an Err value: "database is locked"` at `unv-server/src/history.rs:125`.** The pasted analysis called it a timing flake in the background snapshot. It is deterministic setup: line 125 is `open_db(...)` inside the test helper `hub()`, which named its temp folder from `SystemTime::now()` nanoseconds with no per-test tag. Windows' clock ticks coarsely, so tests that start in the same instant (the log shows the failure at the very start of the run, 18:27:25) got the same folder and the same `vault.db`, and one hit a locked database. Fix: `hub_dir_name()` adds the pid and a process-wide counter; test `hubs_created_in_the_same_clock_tick_never_share_a_folder` feeds a frozen clock 200 times. Ran the history tests three times locally (6 passed each) and clippy on `unv-server`. I audited the other `as_nanos()` temp-dir helpers: all carry a per-test tag, a pid, or serve a single test function, so only `hub()` was exposed. Not reproduced on Windows (no Windows machine); the mechanism is argued from the log line and the code, so treat it as the probable cause until a Windows run is green.

## Re-verified claims (plan vs code)

- `ci.yml` was one `rust` job per OS running test, `cargo check --all-targets`, fmt, clippy, deny, bundled check in sequence: true. Last fully green Windows run (ffe3c2b, run 37992936583): job 32 min, Test 18, Check the whole workspace 10. Other recent Windows runs on main: 7 to 19 min, all failing or cancelled early (b989266 5 min Test then fail; 5ea7516 18; 0aa5609 19; 7250c56 13; b408fff 14).
- `build.yml`: `meta` decided `publish`; `desktop`/`headless`/`docker` needed only `meta`; `docker` pushed three tags with `push: publish` and ran its smoke test after the push: true.
- `exporters.yml` had ssh, apache, haproxy, ansible, postgres, traefik, kubernetes, prometheus, grafana, homepage, grafana-probe; no wireguard, nginx or compose: true. Its `pull_request` path list was a shorter copy of the `push` list (no `enrich.rs`, no workflow file): true.
- All 17 pinned action references were SHA-pinned; 13 ran on Node 20 (checkout v4, setup-node v4, upload/download-artifact v4, configure/deploy-pages, docker/*, release-action, rust-cache, codeql-action v3). Composite ones (rust-toolchain, cosign-installer, lychee, upload-pages-artifact) have no runtime of their own.
- `cargo-deny` was `cargo install`ed from source on every run: true.

## Changes, file by file

- `.github/workflows/ci.yml`: `workflow_call` with a `gate` input; concurrency group `ci-<ref>-gate|push` and `cancel-in-progress: !inputs.gate` (without the split the gate and the push-triggered CI on one ref would cancel each other); `rust` replaced by `test` and `lint` (both Linux and Windows); fmt, deny and the bundled check stay Linux-only; clippy on both; `cargo check --workspace` removed (clippy `--all-targets` compiles the same code and reports more); cargo-deny via `taiki-e/install-action` pinned to `cargo-deny@0.20.2` (the manifest lists 0.20.2); `frontend` job unchanged apart from pins.
- `.github/actions/rust-setup/action.yml` (new composite action): Linux packages, the Windows Strawberry Perl/NASM block and its verification, rust-toolchain, rust-cache (key per caller: `test` / `lint`, because they build different artefacts), Node, `npm ci`, `vite build`. One copy of the long Windows block instead of two.
- `.github/workflows/build.yml`: new `ci` job (`uses: ./.github/workflows/ci.yml`, `with: gate: true`, only when `build == 'true'`); `desktop`, `headless`, `docker` need `[meta, ci]`; `release` needs `[meta, ci, desktop, headless, docker]`. Docker: `push: false, load: true`, smoke test, then a "Push the tags" step (`if: publish == 'true'`) that `docker push`es version, minor and `latest`. No tag exists in the registry until the image started.
- `.github/workflows/exporters.yml`: jobs `wireguard`, `nginx`, `compose`, each with a control that must be refused and the repo's usual "proves nothing" message; one path list for both triggers, extended with `src/ts/chunks/**`, `vault-core/src/config_check.rs`, `unv-cli/src/{exporters,chunks,refs}.rs`, `tests/cli-parity.test.ts`, `tests/stack.test.ts`.
- `.github/workflows/ui-lab.yml` (new): Playwright on pull requests touching `src/**`, `index.html`, `ui-lab/**`, `playwright.config.ts`, `vite.config.ts`, `package*.json`; uploads artefacts on failure.
- `.github/workflows/docs.yml`: cron weekly (`17 4 * * 1`; it was nightly).
- `.github/workflows/codeql.yml`: codeql-action v3 to v4.38.3, checkout.
- `.github/dependabot.yml`: npm groups `vitest` (vitest, @vitest/_), `tauri` (@tauri-apps/_), `lint` (eslint family, prettier, globals); cargo groups `tauri`, `rustcrypto`; github-actions covers `/` and `/.github/actions/*` (`directories`; `directory` and `directories` cannot be combined); `rusqlite`, `libsqlite3-sys`, `argon2` ignore major AND minor (the plan said major; in 0.x a minor is the breaking one and these set the on-disk format; patches still arrive). Validated against the schemastore `dependabot-2.0.json`: 0 errors.
- Actions bumped by full SHA with version comment, each checked to run on node24 at its new tag: checkout v7.0.1, setup-node v7.1.0, upload-artifact v7.0.2, download-artifact v8.0.2, configure-pages v6.0.0, deploy-pages v5.0.1, upload-pages-artifact v5.0.0, docker/build-push v7.4.0, login v4.6.0, setup-buildx v4.4.1, release-action v1.21.0, rust-cache v2.9.2, codeql v4.38.3. Left alone on purpose: `sigstore/cosign-installer` v3.9.2 (composite; v4 changes the default cosign major and the signature format the release step and the website's verify recipe rely on), `dtolnay/rust-toolchain` (composite), lychee-action (new, composite).
- `tests/workflows.test.ts` (19 tests): the gate graph (release and every build job have `ci` as an ancestor; release's needs line is exact; `ci.yml` is callable; gate concurrency differs and is never cancelled; ci.yml asks for no write permission), Docker order (build `push: false`, smoke test before `docker push`, push step conditional on publish), the Windows split (test and lint independent, no `cargo check --workspace`, no `cargo install cargo-deny`), validators (three jobs exist, each with a control, push and PR path lists identical, every non-glob path exists), pinning (every third-party `uses:` is a 40-hex SHA with a version comment; none of the old Node 20 SHAs remain), weekly docs cron, Dependabot families, ignore rules and no `directory` plus `directories`.
- `.claude/rules/ci-release.md`: gate, Docker order, composite action, ui-lab, Dependabot conventions.

## Validators, proven locally before they were written into the workflow

Each ran in a container against the repo's real fixtures, then again by extracting the `run:` blocks from the finished YAML and executing them verbatim:

- **compose:** `docker compose -f compose.yaml --env-file compose.env config -q` ok; a file with `ports: ["not-a-port"]` and `bogus_key` fails (`services.web.bogus_key false schema`). Docker Compose v5.6.0 here; the runner ships v2.
- **nginx** (`nginx:1.27-alpine` pinned by digest `sha256:65645c7b...`): the fixture included from a minimal `http {}` main config with `ssl_certificate_key` at http level and a throwaway cert generated in the container: `nginx -t` successful; a server block with a missing semicolon and an unknown directive fails. **Finding:** the exporter writes `listen 443 ssl http2;`, which nginx 1.27 reports as deprecated (`[warn] the "listen ... http2" directive is deprecated, use the "http2" directive instead`). It still works and is accepted by every nginx, so it is a warning, not a failure; changing it means changing both the TypeScript and Rust exporters and the parity fixture, so it is left as a decision (see Next steps).
- **wireguard:** the fixture's key text is placeholder, and `wg` accepts only 32-byte base64 keys, so the job substitutes generated keys and leaves every other line as written, then `wg-quick strip ./good.conf | wg setconf wgparse /dev/stdin`. No kernel interface or root is needed: `wg` parses the whole file before it touches a device, so a syntax error always prints `Configuration parsing error` and a clean parse fails only at the device. Controls: an impossible prefix (`AllowedIP is not in the correct format: <ip>/99`) and an unknown section (`Line unrecognized: [Peerz]`) are both refused. Running the step for real caught a bug: `wg-quick strip good.conf` treats a bare name as an interface under `/etc/wireguard`; it needs `./good.conf`. A first attempt that created a real interface (`ip link add wg0 type wireguard`, privileged container) also worked but was rejected because it depends on the runner kernel.
- `actionlint` 1.7.12 with `shellcheck` 0.11.0 over every workflow: clean except seven pre-existing SC2034 notes about unused loop variables in older exporters jobs (not touched). It found one issue in my new step (SC2069, redirect order), fixed.

## Verification summary

`npm run check`: eslint, tsc, prettier, Vitest 90 files / 1537 tests (was 1515). `cargo fmt --all -- --check` clean. `cargo clippy -p unv-server --all-targets` clean. Mutation checks of the workflow tests (each reverted, files restored byte for byte): releasing without `ci` in `needs` (2 failures), Docker `push: true` (1), nginx control message removed (1), an action pinned by `@v7` (1), nightly docs cron (1). All five failed as they should; restored run 19/19.

## Timing

Before (measured, ffe3c2b, the last fully green run): Windows job 32 min = Test 18 + Check the whole workspace 10 + about 4 for setup, clippy and the rest, all sequential. After: the Windows `test` job is setup plus the same 18-minute test run; `lint` runs beside it; the 10-minute check step no longer exists. Expected critical path about 20 min (minus roughly 12 min, 37 percent). This is an estimate: the new jobs have not run on a runner, and the first run of `lint` has a cold cache (its own key) so it is slower once. To record the real after, run in pwsh after pushing, three times over three pushes:

```powershell
gh run list --workflow CI --branch main --limit 3 --json databaseId --jq '.[].databaseId' | ForEach-Object { gh run view $_ --json jobs --jq '.jobs[] | select(.name|test("windows")) | "\(.name): \(((.completedAt|fromdateiso8601)-(.startedAt|fromdateiso8601))/60|floor) min"' }
```

## What did not run, and why

- **No workflow ran on GitHub** (a push is the maintainer's). So the gate has not been seen blocking a release, only proven structurally (`tests/workflows.test.ts` plus the mutation checks). The manual demonstration, as the phase file asks when no push is allowed: on a throwaway branch, add `assert!(false)` to any test, run Build & Release by `workflow_dispatch` with `publish: true` on that branch, and confirm `Desktop`, `CLI + server`, `Server image` and `Publish release` show as skipped with `CI gate` red, and that no `:latest` appears in GHCR (`gh api /user/packages/container/unenverse%2Funv-server/versions`).
- The composite action, `taiki-e/install-action` for cargo-deny, the Windows split and `ui-lab.yml` have only been linted and read, not executed on a runner. The Windows history-test fix is not confirmed on Windows.
- Windows-specific timing numbers after the change (above).

## Next steps

Only the maintainer can: push (watch the first CI and Docs runs; the first Docs run also deploys the site, whose images are now tracked); optionally run the dispatch demonstration above; enable required status checks on `main` if wanted. Decisions for the maintainer: whether the nginx exporter should emit the `http2 on;` form for new nginx (the deprecation warning above); whether `.gitignore`'s `shots/` should be narrowed. Then R03.

## Traps for the next agent

- `sed` with `#` as the delimiter over text containing `#` silently rewrites every line (it corrupted `exporters.yml` once this session; recovered by reversing the exact prefix and verified with a diff that only additions remained). Use Python for multi-line workflow edits.
- A job-level `uses:` (reusable workflow) cannot also have `steps`, `runs-on` or `strategy`; permissions of the caller cap the callee, so `ci.yml` must never ask for a write permission (the test checks).
- `wg-quick strip name` reads a name as an interface; give it `./name`.
- `directory` and `directories` cannot both appear in one Dependabot ecosystem entry.
- mdBook-style silent creation is not the only silent failure: a `.gitignore` rule that matches a directory name anywhere (`shots/`) can drop whole folders from a commit without a warning.
