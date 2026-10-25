# Phase log 35: 2026-09-21 - Phase 24 continuation

## What changed

- Bundle cards now expose member add/remove, primary selection, ordering, local
  variable add/remove and bundle rename through the existing edit modal. Secret
  local values use the masked prompt and default to masked display.
- Key pools are grouped inside a bundle rather than flattened into individual
  member cards. Tests cover bundle controls, pool grouping and dangling
  membership rendering.
- `merge_user_vault_write` now requires write permission on both the member and
  the source/destination bundle whenever membership changes. A regression test
  covers a writable member moved into a bundle outside the user's write scope.
- Updated the Phase 24 rows in `AGENTS.md` to reflect source status: 24.2 is
  implemented; 24.3/24.4 have handler tests; 24.1 and 24.5 remain partial.

## Verification

- `npm run format:check` - pass.
- `npm run typecheck` - pass.
- `npm run lint` - pass, zero warnings.
- `npm test -- --run` - 53 files, 1,190 tests pass.
- `cargo fmt --all -- --check` - pass.
- `cargo test -p vault-core changing_bundle_membership_requires_write_access_to_both_entries` - pass.
- `cargo test -p vault-core` - 236 unit tests + composite parity test pass.
- `cargo clippy -p vault-core --all-targets -- -D warnings` - pass.

## Still open - do not mark Phase 24 complete

- 24.1 step 3 still lacks full bundle semantics; steps 4–6 (tainted/cycle-safe
  scope resolver, export/import dialects, Discord parity fixture) remain.
- 24.5's session recipes/presets and per-type workflows remain incomplete.
- 24.3/24.4 still need complete API-route and production acceptance, beyond the
  direct handler tests.
- Phase 24 A9 needs a real GTK/WebKit reproduction with IBus/fcitx and paste;
  A4/A10 need X11 and Wayland native smoke tests. Do not guess at emoji input.
- Repository security settings and Pages enablement require GitHub-side access;
  seed remaining accepted ADRs from the Key Decisions Log.
- Browser layout test and docs-site build could not run in the restricted
  environment during phase log 34; recheck outside it.

No commit was created. The worktree contains substantial earlier Phase 24
changes; preserve them.
