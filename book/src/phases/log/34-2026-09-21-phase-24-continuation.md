# Phase log 34: 2026-09-21 - Phase 24 continuation

## Goal

Continue Phase 24 from phase log 33, reconcile the actual source tree with the
stale phase table, and leave Phase 24 open where native-window or GitHub-side
verification is still required. No commit was created.

## Current Progress

- Phase 24 A4 now queries Caps Lock through a Tauri command (GDK on Linux,
  `GetKeyState` on Windows) on focus and after Caps Lock events. The typed-key
  fallback remains. X11 and Wayland still need a real native-window smoke test.
- A6/A7/A8/A10 were already implemented in the tree: labelled variable/2FA
  groups, a shared sidebar-row height, the in-form generator popover, and
  keyboard reorder. Added the A7 browser-layout assertion. The UI-lab server
  could not start in this sandbox, so the assertion was not measured here.
- Fixed A2's remaining migration defect: `Settings.init()` now filters the old
  `authenticator` sidebar key after merging settings sources, on every load.
- Fixed late Tauri bridge capture. `invokeTauri()` now resolves the current
  runtime at call time; TOFU, user actions, native export, generation and HTTPS
  proxy paths use that boundary. This repaired failures in the TOFU, users and
  vault-watch tests.
- Added an accepted ADR for the Caps Lock decision.
- Added first 24.1 bundle functionality: “Bundle Selected” creates the bundle
  and membership fields in one save; grid rendering groups valid members,
  orphan membership renders standalone, the card can be dissolved without
  deleting entries, and counts distinguish secrets from bundle cards. This is
  only the first slice of step 3; slot operations, scope resolution, dialects,
  import and the Discord acceptance fixture remain unfinished.
- Added server-handler tests for calendar feed metadata/revocation and for the
  opt-in UID registry register/check path.

## Verification

- `npm run format:check` - pass.
- `npm run lint` - pass, zero warnings.
- `npm run typecheck` - pass.
- `npm test` - 53 files, 1,189 tests passed.
- `cargo fmt --all -- --check` - pass.
- `cargo clippy -p envv-server --all-targets -- -D warnings` - pass.
- `cargo test --workspace -- --skip find_free_port_skips_a_taken_one` - pass,
  including the new feed and UID-registry tests and all four export-write tests.
  The one skipped existing test cannot bind a socket in this sandbox
  (`Operation not permitted`). Export tests now use unique temp directories;
  they no longer attempt writes under the user's Downloads/home directories.
- `npx playwright test --config=playwright.config.ts --grep "visible Secrets sidebar rows share one height"`
  could not start the configured Vite web server (exit 1); the layout assertion
  is present but unverified in a browser.
- `npm run docs` could not spawn `/usr/bin/node-22` (`EPERM`), so the docs build
  is unverified here.

## Still Open

- A9 emoji input: first reproduce GTK/WebKit input behavior with IBus/fcitx and
  paste in a real Tauri window; do not add an emoji picker based on guesswork.
- A4 and A10 native-window verification on X11 and Wayland.
- A10's actual drag behavior still needs native diagnosis; keyboard reorder is
  only the accessible fallback.
- C: enable Dependabot alerts/security updates, private vulnerability
  reporting, and CodeQL default setup in repository settings.
- D: seed the remaining accepted decision records from the Key Decisions Log
  and enable Pages if it is not already enabled.
- Phase 24.1 step 3 is partial; steps 4–6 remain. Do not mark 24.1 complete.
- No claim is made here that 24.2–24.5 are fully complete.

## Next Steps

1. Run the A4/A9/A10 diagnosis in a real Tauri window on X11 and Wayland.
2. Complete 24.1 bundle operations and scope before claiming it done.
3. Finish ADR extraction and verify the Pages workflow/repository settings.
4. Re-run the excluded socket-binding test and docs build in an unrestricted
   environment.
