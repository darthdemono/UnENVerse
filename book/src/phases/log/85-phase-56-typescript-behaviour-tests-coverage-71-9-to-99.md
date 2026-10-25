# Phase log 85: Phase 56, TypeScript behaviour tests, coverage 71.9 to 99.5 percent, nineteen bugs fixed

Date: 2026-10-17 to 2026-10-18 (overnight). Scope: `src/ts/**` (what Codecov measures, flag
`typescript`, vitest v8). The Rust side is phase log 83/84; this file does not touch it except the
one resolver fix noted below.

## Goal

Start: "Codecov says 61% of the code has tests, make it 100%." The maintainer then corrected the
intent: the tests exist to check that the code matches the behaviour they expect, functionally and
behaviourally, not to reach a number. So the work became: write tests that pin behaviour, let
coverage rise as a side effect, and fix what the tests expose. Source was never changed just to
raise a number, with one exception that is stated under Decisions (dead guards).

## Current Progress

Final measurement, `npm run test:coverage` (exit 0, thresholds enforced):

|            | start | end       | covered / total |
| ---------- | ----- | --------- | --------------- |
| statements | 71.85 | **99.52** | 13718 / 13784   |
| branches   | 61.53 | **95.35** | 9652 / 10122    |
| functions  | 73.63 | **99.13** | 2407 / 2428     |
| lines      | 74.24 | **99.90** | 11955 / 11966   |

- `Test Files 171 passed (171)`, `Tests 3264 passed (3264)`.
- `npm run lint` clean, `npx tsc --noEmit` clean, `npm run format:check` clean.
- `vitest.config.ts` floor raised from 90/82/89/92 to **99/95/99/99** (statements/branches/
  functions/lines), just under the measured figures.
- Not run: `npm run preflight` (cargo fmt/clippy/test), `ui-lab` (Playwright), the Rust suite. The
  Rust fix below was verified with `cargo test -p unv-app --lib` only.
- Nothing is committed. Commit commands were handed over separately.

### New test files (all under `tests/`)

Shell and grid: `vault-wiring`, `vault-bundles`, `vault-desktop`, `vault-edge`, `app-shell`,
`card-actions`, `card-display`, `card-interactions`, `grid-layouts`, `sidebar-trees`.
Config view: `config-view-add`, `config-view-export`, `config-view-layouts`,
`config-view-actions`, `chunk-cards`, `chunk-export-sparse`, `pick-file`.
Entry form: `entry-form`, `entry-form-tools`, `modals-tail`, `save-entry-rules`,
`cookie-session-form`, `form-generate`, `copy-menu` (extended), `dropdown-menu`.
Settings: `settings-desktop`, `settings-order`, `settings-tabs`, `settings-garbage`,
`settings-persistence`, `settings-backup`, `backup-restore-shapes`.
State and references: `state-edge`, `persist-paths`, `reference-resolution`, `field-references`,
`store-protocols`, `env-naming`, `env-link-modal` (extended), `project-slugs` (extended).
Tools and panes: `tools-panes`, `tools-workflows`, `health-scan` (extended), `dev-tools`,
`missing-markup`, `users-remote-contract`, `users-management`, `auth-panel-flows` (extended),
`coverage-nodes-pane` (extended), `remote-connect` (extended), `unlock-flow` (extended),
`permex-auth-more`, `small-tails`, `dialogs` (extended), `parsers` (extended),
`bundle-export` (extended), `config-check` (extended).
Helpers: `tests/app-boot.ts` (boots the real app against a small vault, tracks and removes the
document/window listeners it adds), `tests/desktop-boot.ts` (same, inside a faked Tauri shell).

## The bugs that mattered

Ranked by what they did to a person. All fixed, each with a regression test whose comment says
what the bug did.

1. **A `null` in the stored settings stopped the app starting** (`state.ts`, `mergeStoredSettings`).
   The first `.slice` on the replaced default threw in `init()`. Settings are plain JSON anyone can
   edit; values of the wrong kind are now dropped and the default kept.
2. **Saving the entry form after a reload overwrote a neighbouring entry and kept its id**
   (`modals.ts`). The form held only an index, so a peer edit or vault watch reload while it was
   open made the save land on whatever sat at that index. It now holds the entry by identity and
   refuses with "changed or deleted elsewhere" when it is gone (invariant 1).
3. **Restoring a backup or JSON file without projects left the vault with no default project**
   (`import-export.ts`).
4. **Cyclic `${chunk:…}` references returned their own `${…}` text as the value**, in TypeScript
   (`chunk-ops.ts`) and in Rust (`unv-app/src/refs.rs`). That literal reached exported configs.
5. **Cancelling "add member to bundle" pulled an unlabelled entry into the bundle** (`vault.ts`;
   `label === undefined` matched).
6. **A public value could not be hidden again** (`render.ts`): the public rule ran before the
   explicit reveal state, contradicting its own comment.
7. **Group-by-type silently dropped cookie, composite and other kinds** while the toolbar count
   still included them (`render.ts`).
8. **The live name-collision warning in the entry form never fired** (`modals.ts`): its draft held
   no values, so `namesGeneratedBy` returned nothing.
9. **The nginx "create certificate" stub never joined the project it was made from** (`render.ts`).
10. **Stack export "Download" threw** `Cannot access 'dlText' before initialization` (`render.ts`).
11. **The eye button did nothing on its first click when the value started out visible**
    (`modals.ts`, `toggleReveal`).
12. **"All Secrets" stayed highlighted beside an environment filter** (`render.ts`).
13. **Toggle buttons rendered `aria-pressed=""` for false** (`render.ts`); `html` prints `false`
    as nothing. Now `String(...)`.
14. **The recovery-package success message was overwritten by the summary at once**
    (`settings-panel.ts`).
15. **The lock-screen server picker showed the literal text `&nbsp;&nbsp;Local Vault`** (`lock.ts`).
16. **The docker services card never showed its "No fields" hint** (`chunk-ops.ts`, `!subCards`
    tested an array).
17. The add-2FA counter row stayed out of sync with the seed's kind (`auth-panel.ts`).
18. A closed dropdown or context menu still added its outside-click handler (`modals.ts`).
19. The Rust twin of bug 4 (counted once above as a file, listed here because it needs its own
    commit and a `cargo test`).

## Decisions made

- **Tests assert behaviour, not coverage.** Hollow tests written early (assertion-less, or driven
  by `/* v8 ignore */` hints) were deleted. Source changes made only for coverage were reverted.
- **Dead guards were removed, not tested.** Where an element comes from `index.html` (so the real
  DOM always has it) a guard such as `if (!el) return` or `el?.value ?? ''` is unreachable; those
  were converted to `!` / plain access (about 60 sites in `modals.ts`, `vault.ts`, `render.ts`,
  `lock.ts`, `settings-panel.ts` and others). This is the one place source changed without a bug.
  It sits in the same commit as the bug fixes because the files overlap.
- **Guards that tests rely on were kept**: `#custom-style` (a `<head>` element, absent after a DOM
  swap), `#search` in `resetViewState`, `#search-history`, and everything in `timeline.ts` init.
  `missing-markup.test.ts` pins that the init functions tolerate missing markup.
- **Removed dead code**: the `copy-chunk-raw` handler and `diffAndStashCopy` in `render.ts` (no
  markup ever emitted the action), the env_file branch of `chunkFieldCopyText`, two unreachable
  `yamlStr('')` branches, and the unreachable tail of `disambiguateNames`.
- **`invoke` in `users.ts` is exported** so the remote users REST contract can be asserted
  command by command (`users-remote-contract.test.ts`).
- No ADR written: nothing here changes a design.

## What Worked / What Didn't Work

Worked:

- Booting the real `vault.ts` (`bootApp`) and driving real buttons found most of the bugs; unit
  tests of helper functions found almost none.
- Table-driven tests for repetitive surfaces (21 add-chunk buttons, 7 exporters, 54 REST routes).
- Writing a "what would a person see" assertion first, then reading the code when it failed.
  Every one of bugs 1 to 16 came from a test whose first expectation was wrong in an interesting
  way.
- Reading coverage from `coverage-final.json` with a small script instead of the text table.

Did not work:

- `/* v8 ignore */` hints: abandoned, they hide rather than test.
- Subset coverage runs: they miss tests in other files that cover a line, so gaps looked bigger
  than they were. Use the full run for decisions.
- Running the suite while another heavy job (`cargo llvm-cov`) held the 4 cores: timing tests
  flaked. Use `--maxWorkers=2 --testTimeout=60000`.

## What is missing (coverage)

557 uncovered items remain (66 statements, 470 branches, 21 functions), in 30 files:

| file                                                                                                                                                                                                                                                                                              | stmts       | branches    | fns    |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ----------- | ------ |
| `modals.ts`                                                                                                                                                                                                                                                                                       | 0           | 80          | 0      |
| `settings-panel.ts`                                                                                                                                                                                                                                                                               | 13          | 51          | 3      |
| `render.ts`                                                                                                                                                                                                                                                                                       | 4           | 50          | 0      |
| `chunk-ops.ts`                                                                                                                                                                                                                                                                                    | 0           | 49          | 0      |
| `tools.ts`                                                                                                                                                                                                                                                                                        | 6           | 42          | 0      |
| `state.ts`                                                                                                                                                                                                                                                                                        | 4           | 28          | 8      |
| `users.ts`                                                                                                                                                                                                                                                                                        | 7           | 26          | 2      |
| `remote-panel.ts`                                                                                                                                                                                                                                                                                 | 6           | 20          | 4      |
| `lock.ts`                                                                                                                                                                                                                                                                                         | 0           | 20          | 3      |
| `auth-panel.ts`                                                                                                                                                                                                                                                                                   | 3           | 18          | 0      |
| `nodes-pane.ts`                                                                                                                                                                                                                                                                                   | 1           | 13          | 0      |
| `onboarding.ts`                                                                                                                                                                                                                                                                                   | 3           | 9           | 0      |
| `chunks/parsers.ts`, `chunks/env-link.ts`, `ui-qol.ts`, `timeline.ts`, `pools.ts`, `import-export.ts`, `copy-profile.ts`, `config-check.ts`, `projects.ts`, `icons.ts`, `history-pane.ts`, `totp-io.ts`, `html.ts`, `bundle-import.ts`, `utils.ts`, `stack.ts`, `session-presets.ts`, `permex.ts` | 1 to 4 each | 1 to 8 each | 0 to 1 |

What they are, by kind (so the next person does not re-derive it):

- **`??` / `||` fallbacks** on `dataset` strings, API answers and optional fields where the markup or
  the server always supplies the value (most of the branch count).
- **Tolerance for a missing DOM element** in functions that would throw a few lines later anyway
  (the form functions in `modals.ts`).
- **Document-teardown guards** (`typeof document === 'undefined'` in `ui-qol.ts`): unreachable in
  jsdom by design.
- **Unreachable `else` arms** in `settings-panel.ts` drag-and-drop and a few `from < 0` checks.
- **Real but small behaviour left**: `lock.ts` three anonymous functions (idle/visibility
  handlers), `remote-panel.ts` four (connect/test callbacks), `state.ts` eight (settings `_apply`
  callbacks, `Settings.init` migrations), `users.ts` two (permission save callbacks),
  `settings-panel.ts` three (drag handlers, archive picker).

## Next Steps

For the maintainer (needs their hands or authority):

- Commit. Nothing was committed or pushed. Suggested groups: Rust resolver fix; `src/ts` fixes and
  dead-guard removal; tests; coverage floor. Foreign changes in the tree (`Cargo.lock`, other
  `unv-app`/`unv-cli`/`unv-server`/`vault-core`/`src-tauri` files, `unv-cli/tests/*`,
  `src-tauri/tests/*`, ADR-0159, `SUMMARY.md`) are not part of this work.
- Decide whether `auth-panel.ts` `advance()` should save through `persist()` (see Notes). It is a
  behaviour change, so it was left alone.
- Run `npm run preflight` and the Codecov upload once the branch exists; the figures above are
  local.

For whoever continues the tests:

1. `lock.ts`: drive the idle-timer and visibility-change handlers with fake timers (the three
   remaining functions).
2. `remote-panel.ts`: the connect-time TLS pin callbacks and the status refresh error branches.
3. `state.ts` `Settings.init` migrations: run `init()` with each `unenverse-*-migrated` flag unset.
4. `users.ts`: save callbacks of the permission editors (`set_user_permissions`,
   `set_class_permissions`) through the real editor, not the dispatcher.
5. `settings-panel.ts`: the drag arms that need a `dataTransfer` shaped like WebKitGTK's.
6. Only then consider the `??` fallbacks. Each one is a decision: test with absent data, or delete
   the fallback if the invariant guarantees the value.

## Notes for whoever picks this up

- **Measuring**: `npx vitest run --coverage --coverage.reporter=json --coverage.reporter=json-summary
--coverage.reportsDirectory=/tmp/covfull --coverage.reportOnFailure=true --maxWorkers=3
--testTimeout=60000 --coverage.thresholds.lines=0 --coverage.thresholds.statements=0
--coverage.thresholds.branches=0 --coverage.thresholds.functions=0`, then read
  `/tmp/covfull/coverage-final.json` (statement/branch maps) rather than the text table. The full
  run takes about 10 to 15 minutes; run it in the background.
- **Flaky under load** (pass alone): `efficacy.test.ts`, `env-import-modal.test.ts` (fixed flush to
  120 ms), `coverage-audit`, `icon-picker`, `modals.test.ts` ("hashes on the hash tab"),
  `entry-form-tools` hash test (now `waitFor`). They are timing-sensitive, not wrong.
- **Pitfalls found** (candidates for `.claude/rules/frontend-tests.md`):
  - `html` renders `false` and `null` as an empty string, so a boolean ARIA attribute needs
    `String(...)`.
  - A single-line `<input>` strips newlines from `.value`, so a multi-line paste into `#f-key` is
    silently one line.
  - `resolveEditTarget` needs the object the form was opened on; an index alone is not identity.
  - `makeProject({ chunks: undefined })` really leaves the key undefined, which is how to test
    "project with no chunk list".
  - Dispatching `click` on a disabled button via `.click()` does nothing; use
    `dispatchEvent(new MouseEvent('click'))` to reach the handler's own guard.
- **Findings not fixed**:
  - `auth-panel.ts` `advance()` calls `st.store.save(st.vault)` directly instead of `persist()`, so
    it skips id normalisation, delta saves and conflict handling.
  - `initAuthPanel` throws when its markup is absent; the other panes tolerate it.
  - `entryHasPayload` counts a named variable with an empty value as a payload, so a typed entry
    with only suggested, empty variables saves as a hollow entry.
  - `toggleReveal` and `masked()` now agree, but the eye button's first click on a public field
    has no visible effect to explain to the user; consider a tooltip.
    coverage-push-2026-10.md` (goal, method, helper scripts, which guards must stay).

## Status at exit (2026-10-18)

- **Committed and pushed by the maintainer**: branch `raise-typescript-test-coverage`, 19 commits
  (`24af76b` Rust resolver fix through `9989e9b` coverage floor), cut from `main` at `061976a`
  (version 0.55.1). **PR #38** is open against `main`. Commit script:
  `~/Code/UnENVerse-commit-typescript-coverage.ps1`.
- **Parallel PR #37** (`rust-behaviour-tests`, phase log 83/84, release 0.56.0) is open too. The two
  branches do not touch the same files except possibly `vitest.config.ts` and the version number;
  `unv-app/src/refs.rs` differs only on this branch. Merge #37 first, then rebase #38 and rerun
  `npm run test:coverage` (the 0.56.0 bump and any Rust-side TypeScript-visible changes could move
  the figures).
- **Not run after the final commit**: CI. The figures in this file are from a local
  `npm run test:coverage` on the uncommitted tree that became these commits; the tree matched.
- **Next action for the next session**: check PR #38 CI, in particular the `CI result` job and the
  coverage thresholds (99/95/99/99), and rerun the load-sensitive tests listed in Notes if one
  fails on the runner.

> 1 line(s) about the maintainer's own machines were left out of this page.
