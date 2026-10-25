# Phase log 14: 2026-08-08 - Vault-switch render bug, three fixes

Single reported bug: **"Switching from Remote to Local is buggy - all api
disappears but Projects/Chunks don't."** Traced, fixed, plus two adjacent
defects found on the way. **3 bugs fixed, 1 regression test added.**

phase log 2 left 563 tests green and Phase 11 uncommitted. Nothing in this session
is committed either - the working tree still holds all of Phase 11.

---

## Goal

Fix one user-reported bug. Scope did not grow beyond the switch paths: the
investigation touched `remote-panel.ts`, `render.ts`, `filters.ts`, `lock.ts`,
`state.ts` and `vault.ts`, but only three lines needed changing.

---

## Current Progress - complete and verified

```
npm test           564 passed, 0 failed   (22 test files)   [was 563]
npm run typecheck  clean
```

The new test was verified by reverting the fix and watching it fail:

```
❯ tests/remote-panel.test.ts:224
    expect(document.getElementById('project-list')!.textContent).not.toContain('Remote WG')
 Tests  1 failed | 25 passed (26)
```

---

## The bugs that mattered

### 1. Disconnecting from a remote left its whole UI in the DOM

`switchToLocalVault()` cleared `st.vault` correctly - phase log 2 already fixed
the state half of this - but **never touched the DOM**. The only thing that
rendered was `finishInit()`, called at the end. On the path the user was
actually on, it is never reached:

```ts
if (!unlocked) { showRelockScreen('switch'); return; }   // ← no render
```

That branch runs whenever the local vault is still locked, which is the normal
case for anyone who connected to a remote **from the startup unlock screen** -
they never entered a master password for the local vault at all.

What the user saw: the remote's project tree in the sidebar and a full config
view of chunk cards, for a vault they were no longer authenticated to. It also
came back intact if the unlock was abandoned. The relock overlay
(`z-index:999`, opaque `var(--bg)`, direct child of `<body>`) hides it while
open, which is why the symptom read as "projects stayed, entries went" rather
than "nothing changed".

**Fix:** `triggerRender()` immediately after the state clear, before the
early return - `remote-panel.ts`.

### 2. Connecting to a remote never reset view state (invariant #3)

`switchToLocalVault()` called `resetViewState()`; `connectRemote()` did not. So
the **local→remote** direction carried `currentSelectedProjectIds`, `expanded`,
`revealed` and `bulkSelected` - all keyed to the vault being left - into the
remote's data. A selected local project id the remote does not have matches
nothing in `getFiltered()`, so the remote loaded into an empty-looking grid with
its data present but invisible. Exactly the failure mode `resetViewState()`'s
own doc comment describes.

**Fix:** `resetViewState()` in `connectRemote()`. `st.vault` is deliberately
**not** cleared there - `finishInit()` reads `st.vault.api_keys.length` to decide
whether to offer the "remote vault is empty, push your N local entries?" prompt,
and clearing it would silently disable that feature.

### 3. `finishInit` had no `|| []` on `api_keys`

```ts
st.vault.api_keys        = data.api_keys;        // no fallback
st.vault.user_categories = data.user_categories || [];
st.vault.projects        = data.projects || [ … ];
```

A vault blob from an older build or an edited backup can be missing the key
entirely; undefined then threw on the very next loop over it. The two lines
below it already had the fallback.

**Fix:** `data.api_keys || []` - `vault.ts`.

---

## Decisions made

**Proved the state layer was already correct before changing anything.** Wrote a
throwaway DOM-level test that ran the real `render()` after
`switchToLocalVault()` and asserted `#project-list` and the chunk cards were
gone. It passed - which is what narrowed the bug to "render is never called on
this path" rather than "the clear is incomplete". The existing
`remote-panel.test.ts` suite only asserted `st.*`, never the document, so it was
green throughout.

**Asked which local-vault state reproduced it instead of guessing.** Two
readings of the report implied different fixes: stale remote UI (render bug) vs.
the local vault genuinely holding projects with zero `api_keys` (data loss). The
answer - "local locked → relock screen" - picked the first and matched the
missing-render branch exactly.

**Did not write a test for bug 2.** `connectRemote` is not exported and reaching
it requires stubbing `showPasswordPrompt`, the panel click wiring and
`RemoteVaultStore`'s network calls. The change is one line whose absence is
self-evident against `switchToLocalVault` two functions below it. Flagged rather
than skipped silently.

---

## What Worked

- **Reading the invariants section of `CLAUDE.md` first.** Bug 2 is invariant #3
  verbatim, applied to the one direction nobody had checked.
- **Grepping for every construction site of the local store**
  (`new TauriVaultStore|new LocalVaultStore` - three hits total) to prove there
  was only one switch path and the bug had to be inside it.
- **Checking the CSS stacking of `#relock-overlay`** to explain why only _part_
  of the stale UI was visible. Without that the report looked self-contradictory
  ("entries cleared but projects didn't" can't both be true if one render ran).

## What Didn't Work

- **Static reasoning about the data layer.** Spent a long stretch trying to
  construct a sequence where the _local_ vault ends up with projects and no
  `api_keys` (a `persist()` firing while `st.store` was Tauri but `st.vault`
  still held remote data). There is no such window - `switchToLocalVault` has no
  `await` between swapping the store and clearing the vault. Should have gone to
  the DOM test sooner.
- **Assuming the relock overlay made the stale DOM harmless.** It does hide it
  while open, but "hidden behind a modal" is not "cleared", and the state is one
  abandoned unlock away from being on screen.

---

## Next Steps

1. **Confirm the fix against the real app.** If the stale projects/chunks appear
   _after_ the master password is entered, this is not the whole fix - that
   would mean the local vault on disk actually holds those projects with zero
   `api_keys`, which is a different (data-loss) bug and needs a fresh trace.
2. **Commit.** Phase 11 is still entirely uncommitted; this session's three
   files sit on top of it. Commit line offered at the end of the session:
   `src/ts/remote-panel.ts`, `src/ts/vault.ts`, `tests/remote-panel.test.ts`.
3. Manual smoke tests from phase log 2 are still unrun - in particular
   _"connect to a remote from the startup screen, then switch to Local Vault"_,
   which is precisely the path bug 1 lived on. It was listed as a next step and
   never executed; that is why the bug shipped.

---

## Notes for whoever picks this up

**The existing suite tests `st.*` and not the document.** That gap is what let
bug 1 survive Phase 11 - `switchToLocalVault` had five passing tests asserting
the state was cleared, and the state _was_ cleared. Where a fix is "the DOM
should no longer show X", assert on `document`, and stub `finishInit` to a no-op
so the test exercises the path that does not render.

**`triggerRender()` vs `render()`:** `remote-panel.ts` cannot import `render.ts`
statically (cycle), so it uses `triggerRender()`, which calls whatever
`setRenderFn` registered. In tests that means importing `../src/ts/render` for
its module-scope `setRenderFn(render)` side effect - without that import
`triggerRender()` is a no-op and the test passes for the wrong reason.
