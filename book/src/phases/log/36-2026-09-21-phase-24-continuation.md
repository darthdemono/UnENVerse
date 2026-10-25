# Phase log 36: 2026-09-21 - Phase 24 continuation

## What changed

- Added bundle-scope resolution in TypeScript and Rust, pinned to one parity
  fixture. Bundle locals and composites can resolve local templates, sibling
  fields and global references while carrying taint and refusing unresolved
  references, cycles and over-deep templates.
- Preserved value-kind metadata through normal form edits; reject cyclic derived
  bundle variables before saving.
- Hardened sub-user bundle reads and writes: hidden parent membership is removed
  from reads and preserved on unrelated writes, while changing membership
  requires permission on the affected bundle.
- Wired bundle bulk selection to the existing selection path, deduplicating pool
  members, and made adding a pool member offer to add the entire pool while
  refusing a split across bundles.
- Added Playwright `storageState` export for cookies and localStorage. Export
  refuses sessionStorage rather than silently losing it.
- `envv doctor` now reports dangling bundle memberships; `doctor --fix` clears
  only stale membership fields and saves alongside entry-ID repairs.

## Verification

- `npm run format` - pass.
- `npm run typecheck` - pass.
- `npm run lint` - pass, zero warnings.
- Full `npm test -- --run` - 54 files, 1,206 tests pass.
- Full `cargo test -p vault-core` - 241 unit tests plus composite parity pass.
- Full `cargo test -p envv-cli` - 35 unit, 20 agent, 1 archive, 6 import, 25
  parity, 11 rate-limit and 8 TOTP tests pass.
- `cargo clippy` for `vault-core` and `envv-cli`, with `-D warnings` - pass.
- `cargo fmt --all -- --check` - pass.
- `npm run build:frontend` - pass (Vite reports existing mixed static/dynamic
  import and large-chunk warnings).
- `npm run docs` - could not run: the sandbox denies the script's child-process
  spawn (`EPERM` when it invokes the configured Node binary).

## Still open - Phase 24 is not complete

- 24.1 still lacks export/import dialects and the Discord config round-trip
  acceptance fixture; B1–B16 are not all implemented (including reference
  cascades, empty/dangling-bundle health actions, complete collision handling,
  and one-save create/dissolve coverage).
- 24.5 has an editable, validated JSON-array field for local/session storage
  tokens; it still lacks the remaining cookie/cURL/HAR/Firefox import workflows
  and usable workflows for the new types. CXF coverage needs
  continued acceptance testing against the 24.1 bundle model.
- 24.3/24.4 still need full API-route and production acceptance beyond handler
  tests; 24.4's limits need benchmark evidence.
- Native A4/A9/A10 checks need a real GTK/WebKit window on X11 and Wayland.
- GitHub repository settings and Pages enablement require repository access;
  finish any accepted ADR publication work when that access is available.
- Browser layout audit and docs-site build remain unverified because this
  sandbox denies local network listen and child-process spawn.

No commit was created. Preserve the existing worktree changes.
