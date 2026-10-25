# Phase log 64: Windows CI portability, bundle display and Nodes loading

Date: 2026-10-09. Version unchanged: 0.42.3.

## Goal

Fix the five reported Windows installer-test failures, inspect the remaining
Windows CI surface for similar defects, and return a per-file PowerShell commit
line. The maintainer then supplied screenshots showing collapsed TMDB bundle
members squeezed into one column and reported nodes missing from the UI.
Both requests are handled together. Commits and pushes remain manual.

## Current Progress

- `unv-cli/src/node_install.rs`: the Linux installation planner now accepts
  rooted Linux paths when invoked on Windows, matching `NodeConfig` validation.
  A relative-path rejection assertion remains covered on every platform. Only
  the test executing the generated Unix `grep` validator is Unix-gated.
- `unv-server/tests/nodes_e2e.rs`: both successful validation fixtures use
  `exit 0`, a builtin accepted by `sh -c` and `cmd /C`, instead of Unix `true`.
- `vault-core/src/nodes_apply.rs`: the existing new-file validation failure
  and existing-file reload rollback tests now run on Windows too. The reload
  fixture also uses `exit 0`.
- `unv-server/src/lib.rs`: the sub-second Retry-After test leaves 500 ms before
  expiry instead of 1 ms, keeping the same assertion with scheduling headroom.
- Inspected CI and Build & Release Windows setup, Rust shell calls, rooted-path
  checks, permission guards, integration-test file cleanup, and short timing
  assertions. No dependency, workflow, or version changes were needed.
- Final `npm run preflight` passed: Rust formatting, workspace all-target clippy
  with warnings denied, 717 Rust tests (one pre-existing ignored benchmark),
  version synchronization, repository Prettier and ESLint, TypeScript, all
  1,501 frontend tests across 88 files, and Vite's production build. The build
  reports the existing mixed static/dynamic import and chunk-size warnings;
  Nodes now follows that existing dynamic-import pattern as well.
- Targeted verification also passed: CLI library 81 tests; node integration
  18 tests; file application 12 tests; targeted Retry-After test. The relative
  path guard mutation was caught by its regression test and restored.

### Bundle display

- `src/css/cards.css`: explicitly hide `.pool-card-members[hidden]`. The shared
  class covers pools and bundles. No grid sizing redesign was needed.
- `src/ts/render.ts`: wrap the bundle chevron with the existing rotating class;
  stringify expanded/selected booleans for bundle, pool and ordinary card ARIA
  attributes. The tagged HTML helper deliberately omits `false`, so interpolating
  a boolean directly produced an empty attribute rather than the string `false`.
- `tests/render.test.ts`: assert inactive slot-tab and collapsed card flags.
- `ui-lab/seed.ts`: add synthetic two-member TMDB and three-member OmniRoute
  bundles, reusable by the browser lab and actual desktop viewer. No values
  from the maintainer's vault were copied.
- `ui-lab/bundles.spec.ts`: new browser regression across all nine viewport and
  scaling profiles. It asserts hidden collapsed members, accessible toggle
  state, full-grid expanded width, member count and usable member geometry,
  then collapses again. It writes screenshot evidence into ignored artifacts.
- `ui-lab/layout.spec.ts`, `tools/viewer/shots.py`, `tools/demo/shots_demo.py`:
  select a visible/top-level normal card rather than a hidden nested member for
  the existing expanded-card scenarios. The native viewer also has explicit
  collapsed and expanded bundle assertions and screenshots.

### Nodes loading

- `src/ts/state.ts`: opening Nodes through `switchTool('nodes')` now triggers
  the existing refresh logic, using the surrounding dynamic-import pattern.
- `src/ts/nodes-pane.ts`: export that logic as `refreshNodes`, update every
  existing caller, clear old rows before requesting, and ignore a successful
  response from a store that is no longer current.
- `tests/nodes-pane.test.ts`: verify automatic load/reload, old-vault row
  clearing when opening against a local vault, and a deferred response after
  switching stores. Removing the store identity guard made that regression
  fail; it was restored and the tests passed again.
- `tests/tools.test.ts`: click the actual tool navigation button against a
  mocked remote and verify that the enrolled node appears without Refresh.
- Nodes remain in the existing Tools pane. A clarification asking whether the
  maintainer instead wants a dedicated sidebar section has no answer yet.
  No new navigation surface was invented. Local vaults correctly explain that
  nodes require a remote server; enrolled nodes are not local vault entries.
- A read-only `unv node ls --json` against the maintainer's existing local hub,
  using its env-file without exposing credentials, succeeds. It returns two
  The backend already has node data; this session did not enroll, revoke,
  accept, push, change node policy or restart the user's server.

## The bugs that mattered

1. Windows CI rejected `/etc/...` in the installer planner because Rust regards
   it as rooted but not absolute without a drive prefix. Four pure planning
   tests failed before reaching the behavior they were meant to check.
2. The installer validator test executed Unix `grep` with shell quoting through
   Windows `cmd`; this does not exercise the Linux installer's actual runtime.
3. Later node integration tests depended on Unix `true` being available to
   `cmd`. The current failing CI run stops before these tests execute, so this
   was found by inspection rather than by a failing Windows log.
4. The rate-limit test could expire its own window after a scheduler delay of
   only 1 ms. Widening the sub-second margin reduces that timing sensitivity;
   it does not make the test independent of wall-clock scheduling.
5. Author CSS `display: grid` overrode the browser's default hiding of an HTML
   `hidden` element. The wrapper was logically collapsed and did not span the
   outer grid, while its member body still displayed. This is exactly the
   narrow-column TMDB screenshot, not a failure of the full-row expanded rule.
6. The Nodes pane had refresh controls and rendering code but no refresh on
   opening. It stayed blank until a manual Refresh even on a connected hub.

## Decisions made

- Keep platform-independent planner and rollback coverage enabled on Windows.
  Gate only tests that actually require Unix executables or permissions.
- Reuse shell builtins and existing tests; do not add dependencies or introduce
  a clock abstraction for this small CI fix.
- Use the repository's numbered handoff convention rather than a root
  `HANDOFF.md`. Handoffs and their index are gitignored and not in the commit line.
- Keep the shared grid and existing Nodes pane. No dependencies, additional
  settings, sidebar sections, real credentials or new production API were added.
- Browser geometry and the native WebKitGTK window, not jsdom alone, validate
  the bundle CSS behavior. jsdom remains sufficient for Nodes request wiring.

## What Worked / What Didn't Work

- The current Windows job log confirms exactly the five supplied failures:
  &lt;https://github.com/darthdemono/UnENVerse/actions/runs/37983317042/job/113998989455>.
  Linux and frontend jobs passed for that revision.
- Cross-checking the validator's actual launcher (`cmd /C` versus `sh -c`)
  found the later `true` dependency; the CI step shell does not choose that
  subprocess shell.
- There is no installed Windows Rust target or cross-linker here. Linux checks
  cannot establish that the native Windows run is green.
- The new bundle regression initially failed because members with `hidden`
  were visible. After fixing CSS it exposed `aria-expanded=""`; stringifying
  the flags fixed that second defect. All nine bundle tests then passed.
- The new Nodes auto-load tests failed against the old code, then passed after
  wiring the open lifecycle. Nodes, tools and render suites passed 109 tests;
  focused ESLint and TypeScript checks passed too.
- The combined real Chromium run passed all 31 selected tests: nine bundle
  regressions, secrets-grid/expanded layout checks at nine sizes, and the four
  existing sidebar/boot/keyboard behavior checks.
- The first full preflight was interrupted by new user input, not a test
  failure. A fresh full run completed successfully with the final production
  code restored, including both mutation guards and the lint-compliant mock
  assertion in the real navigation regression.
- Native harness attempts found two harness defects, not app defects: the
  new scenario first used nonexistent `#search-input` (the real id is `#search`),
  then WebKit WebDriver refused an empty send-keys value when clearing search.
  Both were corrected. The final native Tauri/WebKitGTK run passed all three
  selected scenarios, with zero page errors. Collapsed and expanded bundle
  geometry passed, and Nodes against a local vault displayed the remote-server
  explanation automatically. Screenshots were visually inspected at
  `shots/bundle-regression/`; this does not test a live enrolled node on a hub.
- The Playwright browser cache needed the matching Chromium revision. Installing
  it changed only the local cache, not package files. Generated Python bytecode
  from syntax checks was removed; screenshot/test artifacts remain ignored.

## Next Steps

1. Give the maintainer the explicit per-file PowerShell add/commit line. Do not
   commit, push, or dispatch a remote workflow. Verification is complete,
   `git diff --check` is clean, and there are no staged files.
2. After the maintainer pushes, check the Windows CI job all the way through
   integration tests and clippy, not just the previously failing CLI library.
3. A live-hub desktop login and native Windows execution remain unverified.
   The full UI lab was not rerun, only the 31 relevant browser checks. If the
   maintainer meant a dedicated Nodes sidebar section, settle that navigation
   request separately; the existing Tools pane's missing auto-load is fixed.

## Notes for whoever picks this up

- Always read server-memory before every task, beginning with
  "How he wants AI agents to work", then the project/topic.
- Preserve `exit 0`/`exit 2`/`exit 3` fixture coverage on Windows. Tests using
  `grep`, `touch`, `sleep`, and Unix modes remain Unix-only.
- No files were staged, no commit or push was made, and the product version was
  not bumped. The global instruction preferences were also persisted earlier
  in this session; they are outside this repository.
- All 16 changed source/test/harness files belong to this task. `AGENTS.md` and
  this handoff directory are ignored deliberately; do not force-add them or
  generated screenshots to the commit command. Each source file gets its own
  explicit add/commit pair with one detailed message, all on one pwsh line.

> 1 line(s) about the maintainer's own machines were left out of this page.
