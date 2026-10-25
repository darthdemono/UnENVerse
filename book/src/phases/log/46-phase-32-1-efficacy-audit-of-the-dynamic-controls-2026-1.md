# Phase log 46: Phase 32.1: efficacy audit of the dynamic controls (2026-10-08)

## Goal

Phase 32.1 (AGENTS.md row, written in phase log 45): extend the "does this control do anything visible" probe past static `button[id]` to the controls rendered at runtime, the closers the first probe could only mark "dialog not open", and the `audit-verify` probe artefact.

## What was built

- `tests/probe.ts` (new, shared): `snapshot()` (body HTML with inline style reduced to display/visibility, plus every input/textarea/select value/checked/disabled), `clearTransient()` (toast, every `.open`, and now every `.tool-status` line, so two probes ending in the same status text cannot hide each other), `hadEffect(act, waitMs)` (MutationObserver records OR snapshot diff), `probeKinds({reset, selector, sample, errors})` (groups by `data-action`, samples up to 6 instances spread across the grid, skips disabled buttons, re-prepares the page before every click, reports `targets, kinds, reached, silent, partial, threw`), `judge(report, allowed)`.
- `tests/efficacy.test.ts`: now uses the shared helpers (behaviour identical; 145 buttons, 143 clicked).
- `tests/efficacy-cards.test.ts` + `efficacy-cards-silent.json` (empty): one entry per registered secret type (from `registry()`), plus TOTP seed, 2-key pool, 4-member bundle, same-provider twins, long descriptions; `allExpanded`; probes `#card-grid [data-action]`. 582 controls, 34 kinds, 0 silent, partial: `bundle-order` (first member cannot move up), `bulk-toggle`, `bundle-slot-tab` (active tab).
- `tests/efficacy-config.test.ts` + `efficacy-config-silent.json` (empty): one project per type using the real starter factories (wireguard, docker, nginx, kubernetes, ssh_config, traefik, apache, haproxy, ansible, postgres), selected via `st.currentSelectedProjectIds` and `render()`. 346 controls, 122 kinds (type-prefixed), 0 silent after the fix below.
- `tests/efficacy-closers.test.ts`: every allow-listed "Closes a dialog" id is pressed with its dialog open (`.open` on the nearest `[role=dialog]`/`.modal-overlay`) and must close it; `settings-cancel` needs `openSettings()` because its handler is assigned there (invariant 9), so an `openers` map exists for such cases. 19 tests.
- `tests/honest-controls.test.ts`: +4 tests (chunk move refused with a toast, chunk move among peers works, pool card with a bundle present, icon apply with empty name).

## Bugs found (ranked by consequence)

1. **Chunk up/down changed data without changing the screen** (`src/ts/render.ts`, the `chunk-up`/`chunk-down` branch). It swapped with the neighbour in the stored array; the config view groups by type, so moving the WireGuard Peer "up" put it before the Interface in the stored array (and therefore any export that follows array order) while the view looked unchanged. Verified with a throwaway test (stored order `Interface,Peer` -> `Peer,Interface`, DOM order identical). Fixed: swap with the nearest chunk of the SAME `chunk_type`; at the end of a group toast "Already first of its kind"/"Already last of its kind". Affects wireguard, docker, ssh_config (all chunks of the starter set had distinct types or a single group), and is harmless but now honest elsewhere.
2. **A vault containing any bundle never collapsed pools** (`render.ts`, `renderGrid`). The bundle branch iterates `items`, calls `buildCard` for non-bundle entries, and `return`s before the pool branch. Phase 24.2's "a pool is one card" therefore silently stopped working as soon as 24.1's first bundle existed. Fixed by giving the bundle branch the same pool grouping (`plainPools` excludes bundle members and bundles; `consumedPools`). Caught only because the probe never saw `pool-card-toggle` in a seed that also held a bundle.
3. `icon-manual-apply` ignored an empty name silently. Now toasts "Type an icon name first".
4. Probe artefact: `audit-verify` looked silent only after `audit-refresh` (identical status line). `clearTransient` now clears `.tool-status`; its allow-list entry and `icon-manual-apply`'s were removed and the stale-entry check confirms they are no longer silent.

## Verification

- `npm run check` green: version, prettier, eslint `--max-warnings 0`, tsc, vitest **69 files / 1355 tests**.
- Fault injection on the two real fixes: disabled the same-type check and the pool-in-bundle branch; `honest-controls.test.ts` failed exactly those two tests (28 passed); restored from a copy.
- Earlier (phase log 45): removing the `copyOrExplain` empty toast fails `efficacy.test.ts` with 7 ids.
- Not run: `cargo test` (no Rust touched this step), ui-lab.

## Decisions

- A kind is silent only when NO sampled instance had an effect; "partial" is reported, not failed, because inert instances are normal (first member cannot move up).
- Sampling (6 per kind) instead of all 582: the first run clicked everything and took 193 s; sampled it is ~40 s for each of the two grid probes. The cost is that a control broken only for a rare card type can slip through; 32.2 could sample per card type.
- Closers are verified by pressing, not by allow-listing forever; the allow-list reason text is the filter the closers test reads (`startsWith('Closes a dialog')`). Do not reword it.

## Not done / traps

- 15 `data-action` kinds unreached; listed in the 32.2 row with what each needs.
- `<select>`/checkbox/toggle controls not probed: most apply on Save. 32.2(b).
- The probe waits 40-60 ms; a control whose effect lands later reads as silent.
- State leaks between probes through module state not reset by `resetState`. Add to the reset rather than reordering.
- Pre-existing uncommitted changes in the tree (including `tests/fixtures/*`, `index.html`) are not from this step.
