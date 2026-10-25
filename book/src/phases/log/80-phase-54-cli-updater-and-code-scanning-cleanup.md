# Phase log 80: Phase 54 CLI updater and code-scanning cleanup

Signed standalone-CLI update path (unv update), release feed and guard, ADR-0158; GitHub code-scanning alerts triaged and the form draft moved out of session storage.

## Phase 54 CLI updater, code-scanning cleanup

Date: 2026-10-17. Version unchanged (0.54.1). Start HEAD 1c2595d. All changes uncommitted.

## Code scanning (GitHub API)

106 open alerts on the scanned commit 9f911f5 (local main is 8 ahead). Triage: 76
hard-coded-crypto were test fixtures plus public salt labels (entropy.rs, lifecycle.rs)
and the timing dummy hash (users.rs:602); 19 cleartext-logging were `--reveal`-gated
or non-secret names; 7 path-injection are the local CLI taking the vault path; 2
storage alerts were the browser fallback store. 103 dismissed through
`PATCH code-scanning/alerts/N` with a reason and comment each. Fixed in code, closing
on the next scan after a push: `src/ts/modals.ts` form draft now lives in
`formDraft` (memory only, tests updated); `tests/helpers.ts` strips scripts through
the DOM. Alert 3 (Math.random in render.ts) no longer exists locally. Dependabot
alerts are disabled on the repo (403); 17 Dependabot PRs are open and untouched.

## Phase 54, CLI half

- `vault-core/src/update.rs`: signed manifest (catalogue shape, own key
  `UPDATE_KEY_HEX`, empty until pinned so an unpinned build refuses everything),
  validation, `is_newer_than`, `classify` (Standalone needs `.unv-standalone` beside
  the exe; package-manager paths win over the marker).
- `unv-app/src/update_cmd.rs`: `check`, `install`, `sign` (publisher). Order: origin,
  signature, strictly newer, size + SHA-256 before extraction, exact-name extraction
  (tar.gz/zip, 256 MiB cap), rename-aside replacement with `--version` probe and
  restore. Deviation from the spec text: bare `unv update` is `unv update install`
  because describe leaves must be leaf commands.
- Wiring: `unv-cli/src/main.rs` (no vault, early dispatch), capability entries
  `cli-self-update` and `update-publisher` (exempt), `flate2`/`tar`/`ed25519-dalek` in unv-app.
- Release: `.github/workflows/build.yml` writes `.unv-standalone` into the headless
  archives (`include-hidden-files: true`), signs `unv-update.json` when secret
  `UNV_UPDATE_KEY` exists, runs `scripts/check-update-manifest.sh` and a negative
  control (manifest minus windows must fail). Guard and control were run locally.
- Docs: ADR-0158, `book/src/cli.md` section.

## Verification

`unv-app/tests/update.rs` 10 tests, `vault-core` update unit tests 6; mutation
check: disabling the checksum comparison failed the wrong-checksum test, restored.
`cargo test -p unv-cli --test capabilities` passes. `npm run preflight` exit 0
(1565 Vitest). Not run: a real download over the network, the workflow itself
(needs GitHub), Windows replacement.

## Not done, and why

- Desktop Tauri updater (plugin, `createUpdaterArtifacts`, opt-in setting, Check
  button, static manifest): needs the maintainer's separate Tauri update key; no
  key generation in a development session (phase non-goal).
- To activate the CLI path: generate an Ed25519 seed, store it as secret
  `UNV_UPDATE_KEY`, put the public key hex in `UPDATE_KEY_HEX`, release.
- Website install/update text not yet written (waits for the feed to exist).
