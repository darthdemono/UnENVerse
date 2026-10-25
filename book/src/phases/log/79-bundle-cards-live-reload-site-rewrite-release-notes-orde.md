# Phase log 79: Bundle cards, live reload, site rewrite, release-notes order

Bundle card redesign and filtered collapse fix, faster vault reload, wider audit pane, site rewrite with in-site guides and fresh screenshots, release notes list changes first.

## bundle cards, live reload, site rewrite, release-notes order

Date: 2026-10-17. Version unchanged (0.54.1). All changes are uncommitted.

## Goal

Reconcile MCP memory with the handoffs, then fix: slow auto-reload of the window
when something else writes the vault; ugly bundle cards whose shorten/expand stopped
working under a filter; crammed audit and thin project screenshots; a website that
sent readers to the README and led with one function; and release notes that listed
the commit list last.

## Current Progress

Memory vs handoffs: Handoffs 77 and 78 were still described as uncommitted; their work
is committed (a3af187, 40868ee, f91637a). No handoff covered c572b7f/ef45bab (SQLCipher 3
vaults migrate in place with a backup), b89c2c5 (0.54.1), 9f911f5 (Windows lock wait in
the chain test) or the 16-10 vault cleanup. Memory entity "UnENVerse (EnvVault) working
rules and lessons" got the reconcile and this session's requests; this file covers the rest.

Files and why:

- `src/ts/render.ts` `buildBundleCard`: header is now name, count badge, collision badge,
  one accent Copy button, Edit, then a chip row (Projects/Categories/Tags as pills). The
  four rare actions (Select visible members, Export all, Dissolve, Delete bundle and
  members) moved to a `.bundle-manage` row at the foot of the open body. `expanded` is now
  `collapsedBundles.has(id) ? false : forceExpanded || expandedBundles.has(id)`.
- `src/ts/state.ts`: `st.collapsedBundles` (session only), cleared in `resetViewState`.
- `src/ts/vault.ts` `bundle-toggle`: flips what is on screen (reads `aria-expanded`), not
  the stored set.
- `src/css/cards.css`: bundle card styling (accent left border, pills, manage row).
- `src/css/activity-tools.css`: `#tool-audit .tool-body` no longer capped at 600px.
- `src/ts/vault-watch.ts`: poll 3000 ms to 750 ms, skipped while `document.hidden`, plus an
  immediate check on window focus (removed in `stopVaultWatch`).
- `.github/workflows/build.yml`: release body now starts with "What changed" (the commit
  list), then Desktop, CLI, Verifying. Only the next release uses it.
- `website/`: new home hero and two strength sections, a six-figure tour, new pages
  `guides/importing`, `guides/cli`, `guides/nodes` (replace the three README links),
  security page owns its reporting text (`#report`), footer "Changelog" now points at the
  GitHub releases list. `styles.css` gained prose, pill and wide-figure rules.
- `Screenshots/*.png` retaken with `OUT=shots/demo tools/demo/run.sh`; site WebPs made with
  `scripts/site-shots.sh shots/demo ...` (main, card-expanded, config-history, audit-log,
  project-compose, project-wireguard). `website/assets/screens/manifest.json` updated.
- Tests: `tests/render.test.ts` (filtered bundle collapses), `ui-lab/bundles.spec.ts`
  (same in a real browser).

Verification: `npm run check` (92 files, 1565 tests, typecheck, lint, format) green on two
clean runs; one earlier run showed a single unidentified failing test that did not repeat
on two reruns. `npx playwright test ui-lab/bundles.spec.ts` 10 passed. `node
scripts/build-site.mjs` against a fake release plus `node scripts/check-site.mjs`: 9 pages
x 2 viewports ok. `tests/site-build.test.ts` and `tests/workflows.test.ts` pass. Regression
checked by mutation: disabling the `collapsedBundles` term fails both the Vitest and the
Playwright test.

Not run: the new release body on a runner (it only exists on the next version bump);
`cargo` tests (no Rust changed).

## The bugs that mattered

1. Filtered bundles could not be collapsed. A search matching part of a bundle forced it
   open (`forceExpanded`), and the toggle only edited a set the forced path ignored, so
   the chevron looked dead for as long as any filter was on.
2. Audit log was a third-width table: every tool pane shares a 600px `.tool-body`, which
   clipped the hash and details columns and made README and site screenshots look crammed.
3. Window reload lag: 3 s poll, no look on focus.

## Decisions made

- Overflow menu for bundle actions was built first and dropped: an absolutely positioned
  list inside the card rendered clipped and transparent in the lab. Actions at the foot of
  the open card need no popover, and keep destructive buttons away from Copy.
- Tests still click `bundle-dissolve` and `bundle-bulk-toggle` without expanding; hidden
  elements stay in the DOM, so that works. Do not render the manage row lazily.
- The Nodes and Health screenshots were not used on the site: the demo vault has no nodes
  and both panes are 600px wide. Authenticator is mostly empty. README keeps them.
- Existing release v0.54.1 notes were not edited (public, outward facing).

## What Worked / What Didn't Work

Worked: viewer pipeline (`tools/demo/run.sh`) produced all shots in one run. Did not:
`pgrep -f tools/demo/run.sh` in a monitor matches the monitor's own command line; poll a
file or a pid instead. `page.evaluate` results from a Playwright test are swallowed by
the runner; write to a file.

## Next Steps

- Human: commit (groups in the final message) and push; the site deploys from the latest
  release, so the new hero and guides appear after the next version bump.
- Human: optionally edit the v0.54.1 release body by hand to the new order.
- Health and Tools panes still cap at 600px; widen them like the audit pane if they appear
  in screenshots.
- Phase 54 (R26 updater) is still the next planned phase.

## Notes for whoever picks this up

`shots/` is git-ignored. `Screenshots/` is committed and feeds the README. The demo run
needs the `unv-viewer` image and `ghcr.io/darthdemono/unenverse/unv-server:latest`.
Bundle state: `expandedBundles` persists via `lastView`; `collapsedBundles` does not.

## Addendum (same day, after the first commit round): site look and message

- Headline went through three rounds; final: "Ditch the .env files. Let your team and your
  AI agents use secrets they never get to see." plus the "$0.00" lead, a real terminal block
  (`unv get` fingerprint, `unv exec`, `unv render`, commands taken from the README), then a
  "what it is" paragraph. Licence is not a feature: removed from pills, hero and trust list
  (footer still states it as legal text).
- Home now has three alternating text-and-screenshot rows (self-hosted with the connect
  dialog, agents with the expanded card, teams with the alice permissions panel, all cropped
  from the retaken demo shots), then the cards, then the tour.
- Site shell now mimics the vault window (his words: "i want the website to look like the
  vault", not a demo vault): sticky top bar with search-style link and a purple "+ Download"
  button, activity rail with four icons, 232px sidebar (Explore, Guides, Reference with the
  version as a count badge), app palette tokens (#0e0e0e, #161616, accent #7364c9), light
  variant kept. Under 900px the rail disappears and the sidebar becomes a scrolling tab row.
- `scripts/build-site.mjs`: `fill(html, vars, partials, page, sub)`; new `CUR_*` and
  `RAIL_*` placeholders; front page key is now `home` (was empty). Two tests updated.
- Docs stay linked: sidebar "Documentation", "API reference", "Release notes" on every page.
- Dropped idea: embedding the real app build with a seeded vault as a live demo (he refused a
  demo vault). A demo server was started and removed; nothing from it was kept.

## Addendum 2: all-pages review (desktop and 390px)

Eyeballed every page. Fixed: headings directly under a lead paragraph had no spacing
(`main .wrap > h2:not(:first-child)`); quickstart showed "1. 1. Install" (auto-numbered list
plus numbered headings, numbers removed from the headings); home figures were unreadable
whole-window shots (now cropped to the content area, project shots full width); copy
claims softened ("nothing to breach" and "Other vaults" removed). The config history pane
had no CSS for `.history-table` and crammed its control rows: added table styling, a wider
pane, wrapped control rows with a divider (`src/css/activity-tools.css`), then retook all
demo shots and recropped (`history-crop`, `audit-crop`, `compose-crop`, `wg-crop`, `card-crop`,
`teams`, `selfhost`). Number inputs in that pane got `flex: none` after the retake, so the
committed screenshot shows them slightly wider than the CSS now renders.
Trap: `pkill -f tools/demo/run.sh` inside a Bash command kills the shell running it (exit 144).
