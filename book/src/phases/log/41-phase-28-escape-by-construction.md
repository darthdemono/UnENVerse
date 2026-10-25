# Phase log 41: Phase 28: escape by construction

Date: 2026-10-08. Phase 28 (queue position 28; was 22 → 25 → 26 → 27 before the 2026-10-08 renumbering; review-01 §2.3). Lane A4 in the parallel-lane table. Version untouched (integrator's job). Nothing committed.

## Goal

review-01 §2.3: `render.ts` had 51 `esc()` against 299 interpolations, `chunk-ops.ts` 89 against 485, and nothing proved which of the rest were safe; invariant 4 was enforced by memory. Required: an `html` tagged template that escapes by default with an explicit `raw()` opt-out, migrated mechanically across the renderers, so the bug class cannot be written by accident. Scope grew from "render.ts and chunk-ops.ts" to every file that builds markup, because leaving a second path (`innerHTML = string`) would have kept the class alive.

## Result

- `src/ts/html.ts` (new, ~60 lines): `SafeHtml` class (private markup string, `toString`), `HtmlValue` type, `raw(string)`, `html` tag, `setHtml(el, SafeHtml | '')`. Self-contained escape (`& < > " '`); it does NOT import `utils.ts` because `utils.ts` now imports `raw` for the SVG constants (a cycle otherwise).
- 353 `html` templates across 20 files. `esc()`/`escAttr()` no longer appear in any renderer; they remain only in `utils.ts` (definitions) and the exporter functions in `chunk-ops.ts` lines ~40-260 that emit config text, not markup.
- `raw()` call sites: 9, all constants or an encoder: `utils.ts` x5 (eye/copy/edit/del/dup SVG), `ui-qol.ts` x2 (eye paths), `render.ts` x1 (`ARIA_CURRENT`), `wifi-qr.ts` x1 (`qrSvg` output, generated geometry only). `tests/html.test.ts` pins this list exactly.
- ESLint `no-restricted-syntax` bans assigning `innerHTML`/`outerHTML` and calling `insertAdjacentHTML` in `src/**` except `src/ts/html.ts`. One exemption with a written reason: `tools-markup.ts` injects the constant `TOOLS_PANES_HTML`.
- `.prettierrc.json`: `embeddedLanguageFormatting: "off"` (see Dead ends).
- ADR-0136 + `book/src/SUMMARY.md` entry + Key Decisions Log row + six pitfalls in AGENTS.md + phase table row 28 marked done.

Verification (all run after the last edit): `tsc --noEmit` 0 errors; `eslint src tests --max-warnings 0` clean; `prettier --check src tests eslint.config.js .prettierrc.json` clean; `vitest run` 59 files / 1268 tests green (baseline before the phase was 1257 in 58 files; +11 in `tests/html.test.ts`); `npm run ui-lab` 167 passed (2.5 min). Fault injection on the guards: appending `raw('x')` and `document.body.innerHTML = ''` to `lan.ts` failed both source-guard tests; reverted.

## Pipeline used (so it can be repeated or audited)

Scripts live in the session scratchpad and are NOT in the repo (they are throwaway). Shape, in case a later phase wants to repeat it on new files:

1. `codemod.mjs` - TypeScript compiler API with the real type checker. For every template literal whose static text matches `/<[a-zA-Z\/!]/` and that is not already tagged: prefix `html`. Each `${e}`: `esc(x)`/`escAttr(x)` unwrapped to `x`; nested HTML template converted; ternary/`&&`/`||`/`??` recursed; `arr.map(cb).join('')` unwrapped to the array when `cb` returns a template; otherwise if the checker says the type is number/boolean/null/undefined/enum/an escape-identity string literal union it is left alone, and anything else is wrapped `raw(...)` so behaviour is preserved and the site is greppable. Functions named `export*`, `chunkToString`, `serviceChunkToYaml`, `b64Utf8` are skipped (they emit config text; Apache's `<VirtualHost>` was caught converted on the first run). A second mode rewrote `X.innerHTML = RHS` to `setHtml(X, <html expr>)`.
2. `pass2.mjs` `innerHTML = html\`..\``→`setHtml`. `pass3.mjs`drops`raw(e)`where`e`is already`SafeHtml`. `pass4.mjs`loops on`tsc`diagnostics (TS2322/TS2345 mentioning`SafeHtml`) and retypes the enclosing function's `: string`return to`SafeHtml`or a`string`parameter to`HtmlValue`. `acc.py`rewrote`let x = ''; x += html...`accumulators into`SafeHtml[]`with`.push`, and `raw(x) || y`into`x.length ? x : y`.
3. Then a manual audit of every remaining `raw(` (the codemod left ~270 `raw(` wrappers: 52 in render.ts, 137 in chunk-ops.ts, 92 elsewhere).

## Bugs found by the migration (what they did to the user)

Ranked by consequence. Each was a latent defect in the old code or a defect the codemod itself would have introduced; the second kind is listed because it is the trap for the next person.

1. **Project and chunk ids were interpolated raw** (`data-project-id="${pid}"`, `data-chunk-id="${cid}"`, 53 + 12 sites in `chunk-ops.ts`). Vault data is untrusted (invariant 4); a project id containing `"` broke out of the attribute on every config-view header button (Import/Export/Add chunk). Fixed by construction; regression test in `tests/html.test.ts` loops five project types. Verified the old behaviour would fail it by reasoning (the old code wrote the id unescaped), not by revert; I did not re-introduce it.
2. **Hand-rolled escaping in three places** (`import-export.ts` preview, `tools.ts` diff table `esc2`, `vault.ts` context-menu label) - three more copies of "escape by discipline". Replaced with plain values. The context-menu one would have double-escaped (`Copy Provider: A&amp;amp;B`) once the dropdown started escaping labels.
3. **Dropdown labels were "already escaped or static, render as-is"** (a comment in `modals.ts`). That was a convention, not a guarantee. `DropdownItem.label` is now `HtmlValue`: text is escaped, markup must be `html\`...\``. The one markup label (`lock.ts` "No servers connected yet") was converted.
4. **Codemod trap - `raw(x) || fallback` is always truthy.** `raw()` returns an object, so the "empty" fallbacks in the chunk-card renderers (`empty`, `No fields`) and the diff table (`-`) would have stopped showing. Caught by reading the raw list, not by a test. Fixed with arrays and `.length`.
5. **Codemod trap - local names shadowing the imports.** `tools.ts` had `let html = ''` (calendar) and `const raw = ...` (cron parser); `chunk-ops.ts` had a `raw: string` parameter. They compile to "This expression is not callable" in a different place from the cause. Renamed (`expr`, `pemText`); calendar rewritten to `SafeHtml[]`.
6. **Codemod trap - non-HTML templates containing `<`.** `icons.ts` builds an SVG data URI for `encodeURIComponent`; run through `html` it became `&lt;svg…` and the icon would have vanished. Reverted by hand (and the codemod re-converted it once when re-run on the file; do not re-run the codemod on already-migrated files). Same family: Apache `<VirtualHost>` lines in `exportApache`.
7. **Codemod trap - JSDoc duplication.** The first run on `render.ts` produced a 3,967-line file from 2,805 because `node.getChildren()` returns `JSDoc` nodes and the text rebuild emitted every doc comment twice. Noticed from the line count; redone from a backup with `ts.isJSDoc` filtered.

## Decisions (with rejected alternatives)

- **`SafeHtml` is a runtime class, not a branded string.** A branded `string & {__safe}` cannot be told from a normal string inside the tag function, so `html` could not choose between "escape" and "pass through". Rejected: brand-only (unsound), marker prefix in the string (spoofable by vault data).
- **One escape strength (`& < > " '`) everywhere.** Rejected: separate text/attribute escapes - that is the `esc`/`escAttr` split that let the two drift.
- **`html` flattens arrays; `.join('')` is removed rather than wrapped in `raw`.** Joining SafeHtml items produces a plain string that would then need `raw` or be double-escaped. Rejected: `htmlJoin` helper (one more name; the tag already does it).
- **`setHtml` accepts only `SafeHtml | ''`.** `''` stays allowed so "clear" does not need `html\`\``; elsewhere `replaceChildren()`would also do. Rejected: overloading`setHtml` to accept strings (that is the hole).
- **Config text stays out.** Exporters, ICS, SVG data URIs, clipboard text keep plain templates. Rejected: a second tag (`text`) - no consumer needs it.
- **Prettier embedded formatting off.** Keeping it on was my first state: it re-indented `html` templates and, worse, moved `${value}` to its own line inside `.key-value`, so `textContent` gained leading/trailing whitespace (7 tests failed, and `#f-categories`-style `textContent` readers exist). Rejected: leaving it on and fixing the tests (hides a real behaviour change in anything `pre`-like; the `<pre>` elements in this app are real `<pre>` and Prettier preserves them, which is why nothing visible broke).
- **No ADR-style change to `esc`/`escAttr`.** Left exported (exporters and any future non-HTML use). Rejected: deleting them now - `chunk-ops.ts` exporters still call them.
- **Version not bumped.** Integrator rule.

## Dead ends

- Running `prettier --write` before turning embedded formatting off: it rewrote template whitespace across ~35 files and I could not undo it (the pre-prettier state was not saved). Recovered by (a) a script that removes whitespace between `>` and `${` and between `}` and `</` inside `html` templates (54 templates), (b) turning embedded formatting off, (c) three hand edits of sentences Prettier had wrapped mid-phrase (`timeline.ts` summary, `pools.ts` footer, `render.ts` union chip). Net effect on rendering: none observed; the remaining difference is line wrapping inside source templates. **Save a copy before running a formatter over generated code.**
- `imp.py`/`imp2.py` added an `import { raw }` to files that merely contained an identifier called `raw` (`state.ts`, `totp.ts`, `secret-types.ts`); ESLint `no-unused-vars` flagged them and a script removed the unused names. Files only touched by that cycle are byte-identical to their pre-phase state (verified by `diff` against the backup).

## What was not done

- Exporter functions and other string-building code that is not markup were not touched (by design).
- `esc`/`escAttr` still exist and are exported; `render.ts` and `tools.ts` no longer import them.
- The `innerHTML = ''` sites became `setHtml(x, '')`, not `replaceChildren()`; either is fine.
- Static markup in `index.html` and `tools-markup.ts` is not templated (it has no interpolation).
- Hostile-value coverage was added after the first pass: `tests/escape-renderers.test.ts` (timeline, key pools, authenticator, remote panel, 11 chunk types, Docker services card, nginx cert card, config header buttons for 11 project types), `tests/escape-users.test.ts` (users, user detail, classes) and a diff-tool case in `tests/tools.test.ts`. Mutation check: putting `raw()` back on `data-project-id` and on the saved-remote name failed the remote-panel and `nginx_key` cases. Not covered by a hostile-value test: health scan rows, tool templates grid, lan panel, onboarding, settings panel rows (their values are constants or settings the user typed).
- The ui-lab does not visit the Authenticator panel, bundle card, composite form and others (phase log 40 item 1); those renderers were migrated and unit-tested but not visually checked.
- A real WebKitGTK window was not run (A4/A9/A10 remain human tasks).

## Traps for the next agent

- **Never interpolate a pre-escaped string into `html`.** It is escaped twice. Hold plain values.
- **Do not wrap a joined string in `raw()`** - keep the array of `SafeHtml` and interpolate it.
- A function that returns markup should say `SafeHtml` (or `SafeHtml | ''`); a parameter that accepts a fragment should say `HtmlValue`. If `tsc` says `SafeHtml` is not assignable to `string`, the answer is almost always to retype the receiver, not to cast.
- `html` templates that go anywhere but the DOM (a URI, an exported file) will be escaped. Use a plain template there.
- Do not re-run the codemod on migrated files (it converts plain `<`-containing templates such as the SVG data URI).
- `tests/html.test.ts` has an exact per-file `raw(` count; a new `raw(` is a deliberate edit to that test and a line in review.
- `npm run check`'s 30 s vitest watchdog (phase log 38) still applies; I ran suites individually.
- `ENVV_SERVER_URL` in the user's shell is irrelevant here.
- Handoff directory is `the session records/` (lowercase) in this repo.

## Commit (for the user; I never commit)

Phase 28 only, as one coherent change. Many files in the tree already carry unrelated uncommitted Phase 24 and 25-27 edits, so staging the same paths commits those too (phase log 40 had the same caveat). Left out on purpose: `AGENTS.md`, `the session records/` (gitignored), `HANDOFF.md`, `book/` (untracked, belongs to Phase 24 D; the ADR and SUMMARY edit are in it).

```powershell
git add src/ts/html.ts tests/html.test.ts tests/icon-bundle.test.ts tests/perm-editor.test.ts eslint.config.js .prettierrc.json src/ts/audit.ts src/ts/auth-panel.ts src/ts/chunk-ops.ts src/ts/chunks/edit-modal.ts src/ts/icons.ts src/ts/import-export.ts src/ts/lan.ts src/ts/lock.ts src/ts/modals.ts src/ts/onboarding.ts src/ts/perm-editor.ts src/ts/pools.ts src/ts/remote-panel.ts src/ts/render.ts src/ts/settings-panel.ts src/ts/timeline.ts src/ts/tools-markup.ts src/ts/tools.ts src/ts/ui-qol.ts src/ts/users.ts src/ts/utils.ts src/ts/vault.ts src/ts/wifi-qr.ts; git commit -m "Make escaping the default for every rendered fragment with an html tagged template, setHtml as the only innerHTML write, and a test that pins the nine places allowed to vouch for markup."
```

## Follow-ups scheduled

The items under "What was not done" are scheduled as sub-phase 28.1 in `AGENTS.md` (Phase table and the Wave-E lanes); the rows there are the source of truth for their scope, footprint and edges.
