# Phase log 15: 2026-08-08 - UX persistence, window state, LAN fix (phase 12)

Feature session, not a bug hunt. Started as "add UX persistence", grew three
more asks mid-session. **4 features shipped, 3 bugs found and fixed on the way,
87 tests added (564 → 651).**

phase log 3 left 564 tests green with Phase 11 entirely uncommitted. **That changed
this session:** the user committed the first chunk as `0088c90 QOL Improvements`
(18 files, +1578). Phase 11 is _still_ uncommitted underneath it - see Next Steps.

---

## Goal

Four asks, delivered in order, each one arriving after the previous was verified:

1. "Add persistence on user end - dragging sidebar, login screen dropdown of
   successfully connected servers, research and add many more UX QOL UIs."
2. "Add Tauri window size/position persistence." (follow-up to a gap I flagged)
3. "When a server has been added to Remote Vaults, the open to LAN is shown even
   if local [vault] isn't there, fix this bug."
4. "Put [untested project types] behind an experimental option, add the toggle
   to settings, and remove Remote from settings."

---

## Current Progress - complete and verified

```
npm test                 651 passed, 0 failed   (26 test files)  [was 564]
npm run typecheck        clean
npx vite build           clean
cargo check --workspace  clean
```

Test count by stage: 564 → 626 (QOL persistence) → 633 (LAN fix) → 651
(experimental types).

**Committed:** `0088c90 QOL Improvements` - items 1 and 2.
**Uncommitted:** items 3 and 4 -
`src/ts/lan.ts`, `src/ts/projects.ts`, `src/ts/remote-panel.ts`,
`src/ts/settings-panel.ts`, `src/ts/state.ts`, `src/ts/types.ts`,
`index.html`, `src/css/config-view.css`, `src/css/settings.css`,
`tests/settings-panel.test.ts`, and new `tests/lan-availability.test.ts` +
`tests/experimental-ptypes.test.ts`.

---

## The bugs that mattered

### 1. "Open to LAN" published the wrong vault

Reported as cosmetic - _"open to lan is shown even if local isn't there"_. It is
a display bug, but the button was **live and did the wrong thing**.

`lan_start` (`src-tauri/src/lib.rs:512`) reads the key out of Rust's
`VaultState` and opens **this machine's** `vault.db`. It has no concept of a
remote vault being on screen. And nothing locks the local vault when you connect
to a remote - `lockVault` only calls `.lock()` on the _current_ store
(`src/ts/lock.ts:33`), which by then is the `RemoteVaultStore`. So:

```
unlock local            → local key resident in VaultState
connect to a remote     → st.store swapped; VaultState untouched
#lan-card               → lives inside #remote-workspace, the panel you're in
press "Open to LAN"     → succeeds, serves the LOCAL vault over TLS
```

The user is looking at the remote's data and would reasonably believe they had
just shared it. They shared something else entirely, to anyone who can reach the
port.

**Mechanism of the stale button:** `initLanPanel()` runs once from
`finishInit()`, and `render()` otherwise fires only on start/stop/poll. A vault
switch triggers none of those, so the card kept whatever it was showing when the
panel was first built.

**Fix, three parts:**

- `lanAvailable()` - `inTauri && !st.store.isRemote`. (Also covers non-Tauri,
  where `invoke` is undefined and the button was simply inert.)
- `refreshLanPanel()`, called from `connectRemote()` and `switchToLocalVault()`.
- A guard inside `startLan()` itself. Not redundant: the card is painted only on
  events we remembered to hook, so any future switch path that misses a repaint
  leaves a live button. The consequence is publishing the wrong vault, so it is
  checked at the point of action, not only at paint time.

A **running** server stays visible and stoppable regardless of the gate - hiding
the only "Stop serving" control while the vault is still being published would
be strictly worse than showing it in the wrong panel.

### 2. `recentServers()` comparator returned NaN

The unlock screen's new server picker sorts by `lastConnectedAt`. First version:

```ts
.sort((a, b) => Date.parse(b.lastConnectedAt ?? '') - Date.parse(a.lastConnectedAt ?? ''))
```

`Date.parse(undefined ?? '')` is `NaN`, and **any** arithmetic with NaN is NaN. A
comparator returning NaN leaves the array in its original order, so a
never-connected server did not sort last - it stayed wherever it happened to be
added, potentially above the server used every day. Fixed with an explicit `0`
fallback. Caught by a test, not by reading.

### 3. Dead code in `relativeTime`

`if (secs < 0) return 'just now'` was unreachable - the `secs < 60` line below
already catches negatives. Found by reverting it and watching the test _pass_
anyway. Removed; the clock-skew rationale moved onto the live line.

---

## Decisions made

**Persisted view state is validated on read, not trusted.** `restoreViewState()`
checks every id against the loaded vault and drops what no longer resolves. A
persisted filter is a reference held across a _restart_ - invariant 3 with a
longer fuse. Unvalidated, a deleted project id matches nothing in `getFiltered()`
and the app opens to an empty grid with every secret present but invisible, under
a filter the user has no memory of setting. `resetViewState()` also nulls the
persisted copy so a replaced vault cannot leave its selection on disk.

**Added "Clear filters" (Shift+Esc) as insurance for the above.** Restoring
filters is only safe if there is an obvious way out. Plain Escape clears the
search box only, which is not enough once a category/project/tag selection can
survive a restart.

**Excluded `StateFlags::VISIBLE` from window-state persistence.** The default
flag set includes it, and the tray handler (`src-tauri/src/lib.rs:878`) hides the
window on click. With VISIBLE saved, hide-to-tray → quit → relaunch restores an
_invisible_ window: the app appears to start and do nothing, with only the tray
icon as a way back.

**Experimental project types gate creation only.** Turning the toggle off does
not touch stored `project_type`, and an existing Kubernetes project keeps its
config view and chunks. Gating the render path would strand that data in the
vault with no way to reach it - the same failure mode as the persisted-filter
case above. There is a test asserting the toggle never rewrites stored data.

**Asked before gating the project types.** The request named three types
(Docker, nginx, WireGuard) as "experimental", but there are eleven, and the
other seven were never tested at all. Two readings, materially different
outcomes: gate 3 or gate 7. The answer inverted my assumption - the tested
_three plus generic_ stay available, the untested seven are gated. Guessing
would have hidden the exact features in daily use.

**Retargeted an obsolete test instead of deleting it.** Removing the Remote
settings tab broke `tests/settings-panel.test.ts > the remote-panel link`. The
button was incidental; the bug it guards is handler accumulation across repeated
`openSettings()` calls, which still applies to `s-clear-recent` (same `onclick`
pattern). Moved it there and added an assertion on the observable effect.

---

## What Worked

- **Reverting every fix to watch its test fail.** Five separate reverts. It
  caught three tests that passed for the wrong reason:
  - one asserted `textContent` contained `"this machine"` - but the _normal_ LAN
    card also says "the master password never leaves this machine", so it passed
    with the gate removed;
  - one was a negative assertion that gate-removal cannot exercise, verified
    separately by stubbing `refreshLanPanel()` to a no-op;
  - one had its assertions satisfied by the modal's own pre-fill, so the pick
    handler was never exercised - fixed by blanking both fields first.
- **Splitting a gate into paint-time and write-time, then reverting each
  independently.** Confirms neither is resting on the other. Both reverts
  produced disjoint failure sets.
- **`tsc --noEmit` catching a bad test fixture.** `environment: 'prod'` is not in
  the union (`'production' | 'staging' | …`). `tests/**` being in the tsconfig
  `include` earned its keep.
- **Reading the Rust side before believing the UI report.** The LAN bug looked
  cosmetic until `lan_start` showed it opens `db_path(&app)` - the local file -
  regardless of what the frontend is displaying.
- **Reading the plugin's source rather than its README.** Confirmed
  `tauri-plugin-window-state` skips a position no connected monitor intersects
  (`available_monitors()` + `intersects()`), so unplugging a display cannot
  strand the window. That would otherwise have been a real risk worth building
  around.

## What Didn't Work

- **Hooking view persistence into `render()` without checking who calls it.**
  `debouncedSearch()` calls `renderGrid()` directly, not `render()`, so search
  never reaches `saveViewState()`. Harmless here - search is deliberately not
  persisted - but the single-hook assumption was wrong and I only noticed while
  tracing something else.
- **Pre-lowercasing the search query in the history pick handler.** Diverged from
  the existing `input` handler, which stores it as typed; `parseSearch()`
  lowercases internally. Would have shown a mangled query in the filter tooltip.
- **Assuming "experimental" meant the three types the user named.** It meant the
  opposite. Cost one clarifying round trip, would have cost a wrong feature.

---

## Next Steps

1. **Commit items 3 and 4.** Command was offered at the end of the session; the
   file list is under _Current Progress_.
2. **Phase 11 is still uncommitted**, underneath `0088c90`. Handoffs 2 and 3
   both listed this and it has still not happened. `git status` also carries the
   unrelated `apiv-*` → `envv-*` rename, CSS and `Dockerfile` changes.
3. **Manual smoke tests only a human can run** (jsdom has no Tauri):
   - **Window state.** Resize/move → quit → relaunch. Then the tray path
     specifically: hide to tray → quit → relaunch must give a **visible**
     window. This is the flag decision above and it is unverified at runtime.
   - **LAN gate.** Unlock local → connect to a remote → confirm the Remote panel
     shows the muted "switch to the local vault" notice and no Start button.
     Then start a LAN server, connect to a remote _while it runs_, and confirm
     the Stop control is still reachable.
   - **TOFU + startup-screen remote → Local Vault** - both still unrun from
     phase log 2/3.
   - k8s Secret export with a non-ASCII value (phase log 2).
4. **Untested surface unchanged:** `chunks/env-link.ts` and
   `chunks/edit-modal.ts` still at 0%; `render.ts` and `tools.ts` still have
   large untested regions.

---

## Notes for whoever picks this up

**Connecting to a remote leaves the local vault's key resident in Rust.** The
LAN fix blocks the one path that exploited this, but the key still sits in
`VaultState` for the whole session with nothing on screen indicating it. Locking
local on switch-to-remote is the deeper fix; it was deliberately _not_ done here
because it changes behaviour (switching back would prompt for the master password
every time) and that is a product decision, not a bug fix. Flagged to the user,
no decision yet.

**The starter-chunk factories for all seven gated project types are still
compiled in** (`makeK8sStarterChunks`, `makeTraefikStarterChunks`, …). The
toggle controls reachability only - it does not strip the code. Correct while
they are behind a flag; a separate and larger change if they should be cut.

**New module: `src/ts/ui-qol.ts`.** Password reveal, Caps Lock hints, password
strength, recent-search dropdown, `relativeTime`. Every `wire*` helper is
**idempotent by assignment** (`el.onclick = …`, not `addEventListener`) because
the unlock and relock screens are shown repeatedly across a lock cycle - the
project has now hit this exact bug at least three times (`showUnlockModal`'s
server field, `s-open-remote-panel`, and these). If you add a handler to a screen
that can be re-shown, assign it.

**`resetReveal()` must be called on every open of a password screen.** Without
it, revealing once leaves _every later_ visit showing plaintext - including the
auto-lock screen, which fires precisely when the user has walked away.

**Two settings now gate UI that can hide data**: `rememberFilters` and
`experimentalProjectTypes`. Both are validated/creation-only for the same
reason. If you add a third, the question to answer first is _"what does a user
see if the thing this points at no longer exists?"_

---

## Also recorded in the rolling log

`CLAUDE.original.md` carried a condensed retelling of this session, kept
here so nothing is lost when that file stops being a log.

### 2026-08-08 - Phase 12: UX persistence, window state, LAN fix, experimental types

Four asks in one session, each arriving after the previous was verified. See
`the session records/15-2026-08-08-ux-persistence-window-state-lan-fix-phase-12.md` for the full account. 564 → 651 tests.

- **Persistence.** Sidebar width (clamped on read, not only on drag) + collapsed
  state, sort mode, grid filters, recent searches - all in the existing
  `localStorage` settings blob. `restoreViewState()` validates every id against
  the loaded vault; `resetViewState()` nulls the persisted copy.
- **New `src/ts/ui-qol.ts`**: password reveal + Caps Lock hints on both password
  screens, strength meter on create-vault, recent-search dropdown,
  `relativeTime`, `beforeunload` guard for pending undos.
- **Unlock-screen server picker** sorted by `lastConnectedAt`, filling URL and
  its paired username together.
- **`tauri-plugin-window-state`** with explicit flags (no `VISIBLE`).
- **"Open to LAN" published the wrong vault** - the headline bug of the session.
  Reported as cosmetic; the button was live and served the local vault while the
  remote's data was on screen.
- **Experimental project types.** Only generic/wireguard/docker/nginx are tested;
  the other seven are behind a Settings toggle, creation-only. Remote settings
  tab removed.
- Three bugs found by the test suite rather than by reading: the `Date.parse`
  NaN comparator, dead code in `relativeTime`, and three tests that passed for
  the wrong reason.
