# Phase log 45: Phase 32: "Does it actually do anything?" (2026-10-08)

## Goal

Phase 32 (design: `the session records/23-2026-08-31-review-01-defect-fixes-the-key-pools-sidebar.md`, "Phase 26 - Does it actually do anything?", two deliverables). 32a: behavioural tests that drive the CONTROL and assert what the user sees, including that an unmet precondition refuses honestly. 32b: walk every control and give a verdict (works / silent / lies / decoration), one bug per non-works. Also in this session, before Phase 32: 31.1 (catalogue UI row) and 31.2 (catalogue axes/console_url), written up in the addendum of `the session records/44-phase-31-the-provider-catalogue-2026-10-08.md`.

## What was built

- `tests/efficacy.test.ts` (new): boots the real shell exactly like `tests/vault-shell.test.ts` (real `index.html`, `mountToolsPanes()`, `import('../src/ts/vault')`, keep the bound nodes, re-append per probe), seeds a 2-entry vault, and for each of the 145 `button[id]` (143 clicked; 2 disabled) resets the toast and every `.open` dialog, clicks, waits 60 ms, and records "silent" when neither a `MutationObserver` record nor a snapshot diff appeared. Snapshot = body HTML (inline style reduced to `display`/`visibility`) + every input/textarea/select `.value`/`.checked`/`.disabled` (innerHTML omits properties). Fails on: an unexplained silent id, a thrown error / unhandled rejection, or a STALE allow-list entry (listed but no longer silent). `EFFICACY_REPORT=<file>` dumps the raw result; `EFFICACY_ONLY=id,id` probes a subset (used to debug order effects).
- `tests/efficacy-silent.json` (new): the allow-list, 40 entries, each with a reason (closers with no dialog open, controls hidden until a precondition, handlers bound only when a lock screen is shown, empty-input converters, native file picker).
- `tests/honest-controls.test.ts` (new, 26 tests): 12 Copy buttons x (empty -> "Nothing to copy yet" err toast; full -> "Copied ✓" ok toast), bulk delete with nothing ticked, import confirm with nothing staged.
- `src/ts/tools.ts`: `copyOrExplain(v)` replaces 11 `if (v) void clipboardWrite(v);` sites and the fmt-copy one. Bulk delete and import confirm toast when their precondition is unmet. `src/ts/modals.ts`: cookie split with no parseable cookies and the session-preset header copy with no preset now toast.

## Verdicts (32b, for the 145 buttons)

- **works (visible effect): 103.**
- **silent, bugs, FIXED: 16** - 12 generator/converter Copy buttons (sg, pg, uu, ak, hg, pc-cert, pc-key, sk-pub, sk-priv, st, b64, fmt; silent with an empty output AND with a full one, because `clipboardWrite` does not toast), plus bulk-delete-btn, import-confirm-btn, f-cookie-split-btn and f-session-preset-copy-header. (143 clicked - 40 allowed = 103 works; 56 were silent before the fixes.)
- **silent, allowed with a reason: 40** (see `tests/efficacy-silent.json`). Three deserve a second look in 32.1: `audit-verify` (silent only after `audit-refresh` because both leave the identical status line; alone it writes "Could not load audit log"), `icon-manual-apply` (empty slug ignored without a word), `unlock-submit-btn`/`relock-submit-btn` (not bound until the screen is shown, so the probe cannot reach them).
- **lies: none new.** The known one (`injectIntoForm`) was fixed in an earlier session and has its own test.
- **decoration: none found among `button[id]`.** Static pre-scan: all 305 control ids in `index.html` + `tools-markup.ts` are referenced somewhere in `src/ts` (the ten that are not are the activity tabs, bound through `data-panel`).

## Decisions / rejected alternatives

- Probe instead of hand-walking: the only way the audit stays true is a failing test, per AGENTS.md "Keeping this file true". Rejected a one-off markdown checklist as the primary artefact; this handoff is the human-readable copy.
- Allow-list with reasons rather than excluding ids from the probe: the stale-entry check means a silent control that gets fixed must be removed from the list.
- Reset `.open` dialogs between probes. Side effect: every closer is silent, so they are allow-listed rather than verified. 32.1 probes each with its dialog open.
- Detection by MutationObserver AND snapshot: the first version (innerHTML only) reported 64 silent controls, mostly generators (their output is a `.value`); adding values dropped it to 56; resetting the toast/dialog leakage and keeping `display` from inline style dropped it to 40.

## Verification

- `npm run check` green: version, prettier, eslint `--max-warnings 0`, tsc, vitest **66 files / 1330 tests**. `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo check -p envvault` ok.
- Fault injection: removed the "Nothing to copy yet" toast from `copyOrExplain`; `efficacy.test.ts` failed with 7 unexplained ids; restored.
- NOT done: reverting each of the other fixes one at a time to watch `honest-controls.test.ts` fail (the bulk-delete, import-confirm, cookie-split and preset toasts; only the cookie-split and preset ones are not asserted by `honest-controls.test.ts` at all, they are covered only by the efficacy probe).

## Not done / traps

- 32.1 (new row in AGENTS.md) holds the rest: `[data-action]` card controls, menu items, selects/toggles, `role="button"` divs, config-view handlers, closers with their dialog open, the `audit-verify` probe artefact.
- The probe waits a fixed 60 ms per click; a control whose effect lands later (a slow IPC) is falsely silent. The 17 s runtime is mostly that wait.
- Probes leak state across each other (entries deleted, settings changed) since only the vault and a few `st` fields are reset; order effects like the audit one are possible. Add to the reset rather than reordering.
- Vitest here swallows `console.log`; debug by writing a file.
- Pre-existing uncommitted work in the tree (many files in `git status`) is not from this session; stage explicitly.
