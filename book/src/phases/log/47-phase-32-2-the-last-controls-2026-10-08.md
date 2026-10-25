# Phase log 47: Phase 32.2: the last controls (2026-10-08)

## Goal

Close the three items of the 32.2 row (phase log 46): (a) the 15 unreached `data-action` kinds, (b) selects/checkboxes/toggles, (c) `role="button"` divs and the lock screens' submit buttons.

## What was built

- `tests/probe.ts`: `probeKinds` accepts an async `reset` and an `afterReset` hook (lets a reset click "Run Scan" and then clear the toast it raised).
- `tests/efficacy-surfaces.test.ts` + `efficacy-surfaces-silent.json` (empty): surfaces `health` (vault with a weak entry and an empty bundle; click `#health-scan-btn`; probe `#health-results [data-action]`), `settings` (sidebar sections minus `tags`, so "+ Show" exists; `#settings-overlay [data-action]`), `icons` (`openIconPicker()`; `#icon-picker-overlay [data-action]`), `feeds` (a `RemoteVaultStore` instance with `listCalendarFeeds`/`revokeCalendarFeed` stubbed, `renderTimeline()`; `#tl-feed-list [data-action]`). Names are `<surface>:<action>`.
- `tests/efficacy-settings.test.ts` + `efficacy-settings-silent.json`: for each of the 19 inputs/selects in the Settings overlay plus 5 segmented/theme groups, change it, press Save, and require `Settings.getAll()` to differ. Only `settings-backup-pw` is silent (an action's password field), allowed with that reason.
- `tests/efficacy-cards.test.ts`: selector now includes `#bundle-suggest [data-action]`; reset clears `dismissedBundleSuggestions` (a Dismiss probe persisted into the next probe).
- `tests/efficacy-config.test.ts`: `extras(type)` adds an `env_file` chunk (docker), a docker service with a `${Alpha}` env field, a wg_peer with a vault-reference field, an nginx_key + nginx_server, and a certificate entry for `example.com`.
- `tests/honest-controls.test.ts` +7 tests: banner Accept creates a bundle, banner Dismiss stores `'twin'` (lower-cased), a `${ref}` badge from a config view shows the entry, a health-scan finding for a filtered-out entry shows it, unlock and re-lock screens refuse an empty password visibly, Enter on the `role=button` card icon opens the icon picker.

## Bugs found and fixed

1. **Accept/Dismiss on the "bundle them?" banner were dead in the real app.** `renderBundleSuggestion` inserts `#bundle-suggest` before `#card-grid`; every `data-action` handler is a delegated listener on `#card-grid`, so clicks on the banner never reached it. Fix (`src/ts/vault.ts`): the grid handler became `onGridClick`, still bound to `#card-grid`, plus a document-level delegate for clicks inside `#bundle-suggest`. The banner is created lazily, hence the document delegate rather than a direct listener.
2. **Jumping to an entry showed nothing when something hid it.** `jump-ref` (the `→ vault` badge in the config view) and `health-jump` expanded the card and repainted, but with a project selected the config view replaced the grid, and with a search/tag/project filter active the card was filtered out. New exported `revealEntry(entry)` in `render.ts` (switch panel, `clearAllFilters()` when the entry is not in `getFiltered()` or a project is selected, expand it and its pool, render, flash the card found by id after the repaint). Both call sites use it. `health-jump` previously used a `data-idx` captured before an 80 ms timeout; it now re-finds by id.
3. Probe artefact removed: a dismissed suggestion persisted in settings and silenced the next "accept" probe.

## Findings left (32.3 row in AGENTS.md)

- `copy-chunk-env` ("Copy (resolved)") is unreachable: it is rendered inside `renderChunkCard` only for `docker_service` chunks, which the docker project draws with `renderDockerServicesCard` (that card has `copy-env`). It is the one `data-action` kind still unreached (101/102). Evidence is the probe plus reading the call sites (render.ts 1878-1986); deletion not done because `diffAndStashCopy` and the handler need a check for other users first.
- Sampling is per kind (6 instances); per card type would catch a control broken for one type only.

## Verification

- `npm run check` green: prettier, eslint `--max-warnings 0`, tsc, vitest **71 files / 1364 tests**.
- Fault injection: with the document delegate renamed to a selector that matches nothing and the `clearAllFilters()` call in `revealEntry` removed, exactly four tests failed (banner Accept, banner Dismiss, config-view jump, health jump); restored from copies and the suite re-ran green.
- Not run: `cargo test`, ui-lab (no Rust touched; the banner/jump fixes are DOM behaviour that only a real window fully confirms: the ui-lab does not drive them).

## Traps

- Settings probe presses Save, which writes settings to `localStorage` in jsdom; `reset()` reopens Settings each time but the stored values persist across probes inside one test. Order-independent so far because each control is flipped once, from whatever state it has.
- `Settings.set('dismissedBundleSuggestions', ...)` stores providers lower-cased.
- `st.store` replaced by a `RemoteVaultStore` in the feeds surface; `fresh()` calls `resetState`, which does not restore `st.store`. The feeds surface is last in `surfaces`; if you add one after it, set `st.store` yourself.
- Pre-existing uncommitted changes in the tree (fixtures, `index.html`) are not from this step.
