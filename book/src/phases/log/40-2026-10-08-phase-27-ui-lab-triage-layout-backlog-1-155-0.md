# Phase log 40: 2026-10-08 - Phase 27, UI-lab triage (layout backlog 1,155 → 0)

> Numbering: this file uses the **2026-10-08 (current)** phase numbers. Phase 27 was
> "Phase 30" in phase log 36/37 (before that 28, 29, 33). See `AGENTS.md → Which
numbering does this document use?`. Phase 25 = route tests + `permex`
> (`38-2026-10-08-phase-25-route-coverage-and-permission-expres.md`, done by Codex), Phase 26 = redaction engine outward
> (`39-2026-10-08-phase-26-outward-redaction-engine.md`, done by Codex), Phase 27 = this one. Lane A-3: footprint is
> `src/css/` and `ui-lab/` **only**; markup/aria findings were deliberately left for
> Phase 28 (escape by construction owns the TypeScript renderers).

## Goal

The user said: "25 and 26 done by Codex, read 2 latest Handoff and do 27." Phase 27
is the UI lab: a real-browser layout audit over a viewport matrix. phase log 37 had
just got it to run to completion for the first time (166 tests green) and recorded
**1,223 layout findings, mostly `target-too-small`** as the backlog. Phase 27 = triage
that backlog: for every finding decide _real defect → fix the CSS_ or _audit false
positive → fix the audit and say why_, ending at a lab that is honest and green.
It is also a recurring phase: re-run `npm run ui-lab` after every major release.

Two follow-up requests came in the same session and are folded in:

1. "remove the risk, and find a better route" - my first overlap rule hid real
   defects (see Dead ends). Rebuilt and proven.
2. "Write Phase 27 handoff" + "handoffs need to be super detailed, LLM for LLM"
   (saved to memory, file `handoffs-llm-detail.md` and the MCP graph).

## Final state (what is true now)

| Check                                 | Result                                                                                                                                                         |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm run ui-lab`                      | **167 passed, 0 layout findings** (2.5 min, 2 workers). phase log 37 recorded 166; I did not trace the extra test, the count is simply what Playwright printed |
| `npx vitest run`                      | 58 files, **1,257 tests pass** (82 s)                                                                                                                          |
| `npm run typecheck`                   | clean                                                                                                                                                          |
| `npx eslint ui-lab`                   | clean                                                                                                                                                          |
| `npx prettier --check ui-lab src/css` | clean (after `prettier --write src/css/a11y.css`: I had written `[role='button']`, prettier wants double quotes)                                               |
| Rust                                  | **not run** - no Rust touched. `cargo test`/clippy last recorded in phase log 37/38/39                                                                         |
| Nothing committed                     | the user commits; see Commit section                                                                                                                           |

(If the count changes, read `SCREENS` x `VIEWPORTS` in `ui-lab/layout.spec.ts` plus the single tests in `layout.spec.ts` and `behaviour.spec.ts`.)

## Baseline measured this session (before any change)

First `npm run ui-lab` this session: 167 passed, **1,155 findings** (phase log 37 said
1,223; the difference is phase log 37's own later row-height / audit fixes). Grouped
by rule, with "distinct elements" (the `.md` groups by selector):

| Rule                | Distinct elements | Notes                                                                                                                                                                                                                                                                                                                     |
| ------------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `target-too-small`  | 39                | `#vault-switcher`, `#add-btn` (81 places each), `.section-collapse-btn`, `#new-category-root-btn`, `#new-project-btn`, `#expand-all-btn`, `#bulk-select-btn`, `.pool-card-expand` (43-48), `#settings-save` (25), settings segment buttons, `#f-compromised` and `#s-env-include-prefix` (checkboxes), tool buttons, etc. |
| `controls-overlap`  | 19                | header buttons "overlapping" settings/modal controls; form-internal `#env-btn-N` vs `#f-role`/`#f-label` by 6-7 px; `.card-chevron` vs a custom select; `#f-totp-period`, `#f-details`, `#settings-cxf-export-btn`, `.btn-xs` vs the modal footer at 125 %                                                                |
| `content-clipped-y` | 4                 | `#search-syntax-hint`, `button > span.sidebar-label` (3 px), `div.card-provider` (up to 49 px), `div.card-tags`                                                                                                                                                                                                           |
| `content-clipped-x` | 1-2               | `#search-syntax-hint`, `#s-grid-cols > button.active` (5 px)                                                                                                                                                                                                                                                              |

Artifacts: `ui-lab/.artifacts/layout-findings.{json,md}` (the `.md` is grouped by
rule/selector, the `.json` has one row per finding with `viewport` and `screen`).
`ui-lab/.artifacts/` is regenerated every run; do not commit it. `npm run
ui-lab:report` cats the `.md`.

## Triage decisions, finding by finding

### A. `target-too-small` (39 elements) → real, fixed once in CSS

WCAG 2.2 SC 2.5.8 sets 24x24 CSS px. The audit enforces exactly that
(`ui-lab/audit.ts` rule 5). Nearly every control was small, each with its own
sizing rule, so the lazy-correct fix is **one floor**, not 39 edits.

`src/css/a11y.css` (the stylesheet imported **last**, per `AGENTS.md` file layout, so
it outranks component rules at equal specificity) gained:

```css
button,
[role="button"],
select,
a[href]:not(.skip-link) {
  min-width: 24px;
  min-height: 24px;
}
```

Result: 1,155 → **496** findings in one change. Remaining small targets: 2
(`#f-compromised`, `#s-env-include-prefix`, both checkboxes).

Decisions inside this fix:

- **Why `min-*` and not `width/height`:** components that are already larger must not
  shrink, and `min-*` on a flex child interacts correctly with `flex: 1` in a group.
- **Why checkboxes/radios are not in the selector:** forcing 24 px on a native
  checkbox glyph distorts form rows; the _label_ is the hit target. The two remaining
  checkbox findings were then resolved in the **audit** (rule 3 below), not by growing
  the glyph.
- **Why `.skip-link` is excluded:** it is an off-screen link that appears on focus at a
  fixed place; a min-height there changed nothing useful and it overlapped the sidebar
  in one 125 % screen. Excluded to keep its geometry exactly as designed.
- **Specificity:** the rule is element-level (0,0,1). It only wins where nothing
  larger is declared, which is the intent: it is a floor. If a future component sets
  `height: 18px` on a button, _height_ wins over `min-height`? No: `min-height` always
  wins over `height` when height < min-height. That is the point. If a component sets
  `min-height: 0` deliberately (flex overflow trick), the floor loses to a class
  selector; the lab will then flag it, which is correct.
- **Side effect to watch:** `select` now has a 24 px floor. All existing selects were
  already taller; the lab screenshots in `ui-lab/screenshots/` show no change.

### B. `controls-overlap` (19 elements) → mostly false positives; audit rebuilt; some were real-looking and are retired with evidence

This is the part that went wrong first and then right. Read the Dead ends section
before changing the rule again.

What the 19 actually were (`detail` strings from `layout-findings.json`):

- `#add-btn`, `#copy-all-btn`, `#copy-all-arrow` "overlaps `#settings-close` /
  `button.settings-tab`" on **settings-*** screens: header buttons sitting _under_ the
  open settings modal. Not clickable by a user; the audit measured rectangles only.
- `button.card-chevron`, `button.pin-btn`, `button.icon-btn.sm`, `.danger`,
  `.env-copy-caret`, `.provider-icon-wrap` "overlaps `button.custom-select-btn`" on
  **modal-add-entry**: cards in the grid behind the add-entry modal.
- `#env-btn-N` vs `#f-role` / `#f-label` by 6-7 px inside the add-entry form, at fhd and
  others. These looked _real_ (a preview line touching a field) and are the reason the
  user asked me to "remove the risk".
- `#f-totp-period`, `#f-details`, `#settings-cxf-export-btn`, `button.btn-xs`
  "overlaps `#modal-cancel` / `#settings-save`" at **fhd-scaled-125** only: scrolling
  form content passing under the sticky modal footer.
- `a.skip-link`, `#vault-switcher` at 125 %: skip link overlapping the first sidebar
  item (the skip link is `position: absolute` and only shown on focus).

Final rule (rule 7 in `ui-lab/audit.ts`, inside the `AUDIT_FN` template string).
For each pair `(a, b)` of interactive controls whose rects overlap by more than
4x4 px and neither contains the other:

1. `clickable(el, other)`: `el.scrollIntoView({block:'center', inline:'center'})`, then
   `document.elementFromPoint` at `el`'s own centre. The pair survives only if what is
   on top there is `el`, a descendant of `el`, or contains/is `other`. Anything else on
   top (an overlay, a modal backdrop) means that control cannot be clicked, so it
   cannot clash with anything → skip. The "or `other`" allowance matters: when the
   overlap _is_ `b` sitting over `a`'s centre, the top element is `b`, and that is
   exactly the defect we want reported.
2. `scroller(a) !== scroller(b)` → skip. `scroller` = nearest ancestor with
   `overflow-y: auto|scroll` _and_ `scrollHeight > clientHeight`. Content in a
   scrolling body passing under something outside that scroller is scrolling, not a
   clash.
3. `pinned(a) !== pinned(b)` → skip. `pinned` = nearest ancestor (or self) with
   `position: sticky|fixed`. A sticky footer in the same scroll context as the form
   body (`.modal-footer`-like block at `modals.css:81`, `position: sticky; bottom: 0`) is
   passed under by the body by design. Pinned-vs-unpinned is not a clash.
4. Otherwise report `controls-overlap`.

Outcome of the full run with this rule: **0 overlap findings**. The `#env-btn-N` vs
`#f-role`/`#f-label` overlaps do **not** reappear, so they were artefacts of the old
viewport-clamping behaviour of the audit's first hit-test (the controls were mid-scroll
inside the modal when measured, not actually overlapping at rest). I treat that as
established, not assumed - see Verification, fault injection.

### C. `content-clipped-y` / `content-clipped-x` (5 elements)

| Element                                                              | Verdict                                                                                                                                                                                                   | Action                                                                                                                                                    |
| -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `#search-syntax-hint` (81 places)                                    | false positive: it is a `.sr-only` span referenced by `aria-describedby`; clipped on purpose so screen readers read it and eyes do not                                                                    | audit skips `el.closest('.sr-only')` for **all** rules in the per-element loop                                                                            |
| `div.card-provider` (up to 49 px)                                    | by design: collapsed card clamps the badge row to whole rows (`max-height: var(--cs-clamp-rows); overflow: hidden`, `cards.css:~216`) and `.card.expanded` lifts it (`max-height:none; overflow:visible`) | audit: `clampedByDesign = el.matches('.card:not(.expanded) .card-provider, .card:not(.expanded) .card-tags')` suppresses both clipped rules for these two |
| `div.card-tags` (21 px at qhd)                                       | same: `max-height` computed from `--cs-tag-rows`, `overflow:hidden`, lifted on `.card.expanded` (`activity-tools.css:~1925-1952`)                                                                         | same selector                                                                                                                                             |
| `button > span.sidebar-label` (3 px)                                 | **real**                                                                                                                                                                                                  | CSS, below                                                                                                                                                |
| `#s-grid-cols > button.active` ("Auto", 5 px, `overflow-x: visible`) | **real**                                                                                                                                                                                                  | CSS, below                                                                                                                                                |

CSS fixes:

- `src/css/sidebar.css`: appended `.sidebar-item .sidebar-label { line-height: 1.4; }`.
  The label is `overflow:hidden; text-overflow:ellipsis; white-space:nowrap` in a row
  whose height is capped by the `--sidebar-row-h` token (30 px, A7 assertion). The line
  box was shorter than the glyphs, so descenders were sliced by 3 px. 1.4 x ~11-12 px
  = ~16-17 px, still far below the 30 px cap, so no row height changes (the A7
  `visible Secrets sidebar rows share one height` test still passes).
- `src/css/settings.css`: `.seg-control button { flex: 1 0 auto; }` (was `flex: 1`).
  `flex: 1` means `flex-basis: 0`, so a segment could shrink below its label and the
  active (heavier) label overflowed by 5 px. `1 0 auto` keeps equal growth but never
  narrower than content.

### D. `target-too-small` leftovers (2 checkboxes) → audit rule

`#f-compromised` (add-entry form) and `#s-env-include-prefix` (settings) are native
checkboxes inside/named by a `<label>`. Audit rule 5 now computes
`viaLabel = el instanceof HTMLInputElement && (type==='checkbox'||'radio') && el.labels.length>0`
and does not apply the 24 px test to those. This is the standard reading of 2.5.8's
"inline / adequate via label" allowance. A checkbox with **no** label is still flagged.

## Complete change list

| File                                                                                                                                                                            | Change                                                                                                          | Why           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ------------- |
| `src/css/a11y.css`                                                                                                                                                              | +12 lines: the 24 px floor rule with a comment citing the lab count (39)                                        | rule A        |
| `src/css/sidebar.css`                                                                                                                                                           | +5 lines: `.sidebar-label { line-height: 1.4 }`                                                                 | rule C        |
| `src/css/settings.css`                                                                                                                                                          | `.seg-control button { flex: 1 0 auto }`                                                                        | rule C        |
| `ui-lab/audit.ts`                                                                                                                                                               | `.sr-only` skip; `clampedByDesign`; checkbox/radio `viaLabel`; overlap rule rebuilt (clickable/scroller/pinned) | rules B, C, D |
| `the session records/40-2026-10-08-phase-27-ui-lab-triage-layout-backlog-1-155-0.md`, `the session records/INDEX.md` (row 40), `AGENTS.md` (Phase 27 row ✅, next handoff = 41) | docs                                                                                                            | housekeeping  |

Pre-existing, **not mine**, appear in `git diff` of the same files: `ui-lab/audit.ts`
also carries phase log 37's accessible-name resolution (`byIds`, `forLabel`, `wrapping`);
`src/css/sidebar.css` carries phase log 37's `max-height: var(--sidebar-row-h)` row cap
and `.tag-chip-sidebar` truncation; `src/css/settings.css` etc. may carry other Phase 24
edits. `git diff` on those files shows both sets. `src/ts/audit.ts` (the _vault audit
log UI_, unrelated to `ui-lab/audit.ts`) is a Phase 24 file; do not confuse the two.

## Dead ends and corrections (read before touching the overlap rule)

1. **First overlap rule (wrong).** I added `reachable(el)`: hit-test each control's
   _centre_, clamped into the viewport (`Math.min(Math.max(x,0), clientWidth-1)`), and
   dropped controls that were not the top element. Findings fell 496 → "5" and I
   reported 0 overlaps. The user asked me to remove the risk. Why it was wrong:
   - clamping an off-screen control's centre to the edge hit-tests _some other
     element_, so any control below the fold of the modal was silently excluded; real
     overlaps in the add-entry form could never be seen;
   - it excluded controls "covered" by the _other control of the pair_, which is the
     defect itself.
     Lesson: **an audit that reports 0 must be shown to report >0 on a planted defect.**
2. **TypeScript syntax inside the audit string.** `AUDIT_FN` in `ui-lab/audit.ts` is a
   _template-string of plain JS_ evaluated in the page. I wrote
   `const reachable = (el: Element): boolean =>`. Every screen died with `SyntaxError:
Unexpected token ':'` (166+ failures, `layout-findings.md` said "0 total" because no
   test reached `afterAll` data). No TS annotations, no `as`, no generics inside it.
   Also escape backslashes (`\\s+`) because it sits in a template literal.
3. **Fault injection that missed.** My first planted overlap
   (`#copy-all-btn{left:-40px}`) did nothing: on the `secrets-grid` screen
   `#copy-all-btn` has zero size (hidden). Check the element's rect before trusting a
   negative. A second injection (`#add-btn{position:relative;left:-90px}`) correctly
   produced `controls-overlap` for `#shortcuts-btn`, `#lock-btn`, `#settings-btn`.
4. **`clickable()` initially required the top element to be the control itself.**
   That skipped the planted overlap because the _other_ button covered the centre.
   Fixed by accepting `other.contains(t)`.
5. **Prettier:** `[role='button']` in CSS failed `prettier --check`; the repo uses double
   quotes. Ran `prettier --write src/css/a11y.css` (diff stayed 12 added lines).

## Verification

Run order and exact observations:

1. `npm run ui-lab` baseline: 167 pass, 1,155 findings.
2. +24 px floor: 496 findings (overlap 18, clip-y 4, clip-x 2, small 2).
3. - sidebar/seg/audit tweaks: 5 → then 0 on the _first (wrong)_ overlap rule.
4. Rebuilt overlap rule (centre+other): full run **0**.
5. **Fault injection (the proof):** appended `#add-btn{position:relative;left:-90px}`
   to `src/css/a11y.css`, ran only `-g "fhd \(1920.*secrets-grid"`, got 3
   `controls-overlap` findings (`#shortcuts-btn`, `#lock-btn`, `#settings-btn`),
   restored `src/css/a11y.css` from a backup (`/tmp/a11y.bak`, byte-identical;
   `git diff --stat` shows exactly the 12-line floor). Then a clean full run: 0.
6. `npx vitest run` 1,257 pass. `npm run typecheck`, `eslint ui-lab`, `prettier
--check ui-lab` clean.

Not verified, on purpose or for lack of means:

- **WebKitGTK.** `playwright install webkit` fails on host requirements here. The lab
  is Chromium only. The WebKit-specific defects in `AGENTS.md` (ghost inputs, A9/A10)
  are still human-window checks.
- **A real Tauri window.** Unchanged from phase log 37 (A4/A9/A10).
- The 24 px floor was checked via the lab's own rule; I did not eyeball every
  screenshot. If the user notices a visibly fatter control, look at `ui-lab/screenshots/`
  for that screen first.

## Decisions (with rejected alternatives)

- **One global floor vs per-component fixes.** Rejected 39 edits: they would drift,
  and a new button would regress silently. The lab now guards the invariant.
- **Fix the audit when the rule is wrong, but only with evidence.** Four audit-rule
  changes, each justified above and each narrow: `.sr-only`, collapsed-card clamp,
  labelled checkbox, overlap context. Rejected "raise thresholds" or "ignore a
  selector list" (would hide the next real defect). Rejected deleting the overlap rule.
- **No `!important`, no markup changes.** Lane footprint is CSS + `ui-lab/`. Anything
  needing markup/aria (e.g. giving `#search-syntax-hint` a different mechanism) waits
  for Phase 28.
- **Chromium-only accepted for this phase.** WebKit remains a human check.
- **No change to the viewport matrix or screens.** Adding screens is future work (below).

## Traps for the next agent

- `AUDIT_FN` is a **string of JS**. No TS types, escape `\\` and backticks.
- `ui-lab/.artifacts/layout-findings.md` says "0 total" both for a clean run **and** for
  a run where no test finished; check the Playwright summary line too.
- A `-g` filtered run **overwrites** the artifacts with only that subset. Run the full
  suite before reading the findings.
- The lab tests set `onboardingCompleted: true` in `localStorage` (the Phase 19 wizard
  otherwise covers everything) and seed the vault through `sessionStorage`.
- `ui-lab` uses `--host 127.0.0.1` for Vite (the `::1` default breaks `baseURL`).
- The overlap rule calls `scrollIntoView` inside the page, so it **changes scroll
  positions during the audit**; it runs last, after screenshots data is gathered per
  screen (the screenshot is taken after `evaluate`, so screenshots may show a scrolled
  modal). Fine for now; if screenshots must be pristine, move the screenshot before the
  evaluate in `layout.spec.ts`.
- If you add a sticky/fixed bar that content should _not_ pass under, the pinned-vs-
  unpinned exemption will hide a real clash there. Add a behaviour test instead.
- Do not run `npm run check` as a gate: its watchdog stops the vitest run after 30 s
  (recorded in phase log 38). Run suites individually.
- `ENVV_SERVER_URL` is exported in the user's shell (memory
  `envv-server-url-in-shell`); irrelevant here, relevant to any CLI smoke test.

## Not done / next

1. Add screens the lab never visits: the Authenticator panel, bundle card, web-session
   form, composite form, calendar feed dialog, uid registry pane, context menus and the
   copy-as dropdowns. Findings there will surface only once screens exist.
2. Markup/aria findings (control names, landmark roles) → Phase 28.
3. Re-run `npm run ui-lab` after every major release (the phase's recurring clause)
   and after any CSS-touching phase; Phase 28 and 29 are the next two.
4. WebKitGTK real-window checks (A4, A9, A10) remain human tasks.
5. Merge order reminder: Phases merge in number order; this phase is CSS/ui-lab only so
   it cannot conflict with 28/29 (TS) beyond `index.html`.

## Commit (for the user; I never commit)

Phase 27 only: `src/css/a11y.css`, `src/css/sidebar.css`, `src/css/settings.css`,
`ui-lab/audit.ts`. **Caveat:** those four files also hold earlier uncommitted Phase 24
edits (see "Pre-existing" above), so staging them commits both. Left out:
`AGENTS.md`, `the session records/`, `HANDOFF.md` (gitignored), `src/ts/audit.ts` (Phase 24).

```powershell
git add src/css/a11y.css src/css/sidebar.css src/css/settings.css ui-lab/audit.ts; git commit -m "Clear the UI-lab layout backlog from 1,155 findings to zero with a 24px target floor, two CSS fixes and four reviewed audit-rule changes."
```
