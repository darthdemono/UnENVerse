# Phase log 18: 2026-08-24 - Card sizing, repo cleanup, and the CLAUDE.md drift audit

**Date:** 2026-08-23 → 2026-08-24
**Branch:** `main`, at `0088c90 QOL Improvements`. Nothing committed this session.

---

## Goal

Started narrow: make the card-size setting change card _height_ as well as grid
column width, stop text clipping without an ellipsis, and fix the copy-button
footer changing size and position from card to card.

Then widened twice, at the user's direction: first to a critique of that change,
then - after a clarification - to an all-time whole-repository diagnosis. The
diagnosis was interrupted twice (once by the usage limit, once by the user), and
only one of its eight reports survived. That report is the reason the second half
of this session became a documentation-accuracy fix.

---

## Current Progress

### 1. Card sizing - done and verified

`src/css/cards.css` now derives every card dimension from `--cs-*` tokens
selected by `#card-grid[data-card-size]`, which `applyGridSettings()` stamps in
`src/ts/state.ts`. One setting moves width and height together.

| token             | compact | medium | large |
| ----------------- | ------- | ------ | ----- |
| `--cs-card-h`     | 200px   | 236px  | 280px |
| `--cs-icon`       | 32      | 40     | 48    |
| `--cs-desc-lines` | 2       | 3      | 4     |
| `--cs-foot-h`     | 34      | 40     | 44    |

Collapsed cards have a fixed height; `.card-head` takes the slack (`flex: 1 1
auto`) so the footer sits on the bottom edge. Expanded cards are `height: auto`
and every clamp is lifted. `#card-grid` gained `align-items: start` so an
expanded neighbour cannot stretch a collapsed card.

Verification: `npx vitest run` → **671 passed / 27 files**; `npm run typecheck`
clean; `npx vite build` clean. Nine new tests in `tests/render.test.ts`, each
verified by reverting its fix and watching it fail.

### 2. Abandoned worktree deleted - 1.9 GB reclaimed

`.claude/worktrees/agent-a3723c0e15ae0afb5` was registered under the pre-rename
path `~/Code/APIV/...`, so its gitlink was already broken (`git status` inside it
returned "not a git repository"). phase log 1 §Next Steps had prescribed this
cleanup and recorded it as blocked by the sandbox classifier.

Verified safe before deleting: its commit `6284b35` ("Initial Commit at second
phase", May 2026) is an ancestor of `main`, branch
`worktree-agent-a3723c0e15ae0afb5` still resolves, and its six locally-modified
files were Phase-3-era edits long superseded (its `lib.rs` was 654 lines against
main's 958). Those six are archived at
`scratchpad/orphan-worktree-modified-files.tgz` regardless.

Repo is now 9.4 MB excluding `node_modules`/`target`.

**Not done from phase log 1's cleanup list:** `git branch -D
worktree-agent-a3723c0e15ae0afb5` (kept deliberately as deletion insurance) and
`rm -f architecture.dot`. Both still pending a decision.

### 3. CLAUDE.md drift fixed

See "The bugs that mattered" below for what was wrong. Nine corrections applied,
plus a new **"Keeping this file true"** section stating the rules that would have
prevented them.

---

## The bugs that mattered

Ranked by consequence. The first is the worst thing found this session, and it is
not in the code.

### 1. Five consecutive documents asserted work was undone that was already committed

CLAUDE.md's Immediate Next Steps and Handoffs 2, 3, 4, 5 and 6 all said "Commit
Phase 11 - still uncommitted, now underneath `0088c90`". It was committed in
`de212a3 "Fixed Bugs"` at 14:37:47, **thirty minutes before** `0088c90` at
15:07:37. `de212a3` contains all 24 Phase-11 test files.

Nobody checked `git log`. Each session inherited the line and copied it forward,
and it became more credible with each repetition. What that cost: every session
since Phase 11 opened with a false top-priority item, and the _real_ uncommitted
work - 8,036 lines across 38 untracked files, the whole of Phases 13–15 - was
never described accurately in its place.

This is the same failure mode the project already documented in Phase 6, where a
wrong claim about `danger_accept_invalid_certs` "hid a real bug". The
documentation is load-bearing here because it is injected into every session.

### 2. The Architecture section described a codebase shape three phases dead

`### File Layout` listed `src/vault.ts (~1800 lines)` and `src/vault.css`.
Neither has existed since Phase 3 split them into 24 modules under `src/ts/` and
15 stylesheets under `src/css/`. A reader trusting it would look for files that
are not there and miss `vault-core/`, `envv-server/` and `envv-cli/` entirely.

### 3. Type unions documented at a third of their real size

`ProjectType` was listed as 3 variants; there are 11. `ChunkType` as 7; there are
**29**. Both grew across Phases 12–15 without the doc moving.

Worse, the `ProjectType` line **contradicted the Phase 12 section ninety lines
above it**, which correctly lists all 11. Two sections of one file disagreeing is
a live bug in the docs, and the nearer one was wrong.

### 4. The Rust command list documented 9 of 36

Missing: the entire RBAC surface (users, classes, tokens - 17 commands), all
three LAN commands, `get_expiring` and `get_audit_log`. Anyone reading CLAUDE.md
to learn the Tauri API would conclude multi-user and Open-to-LAN do not exist.

### 5. The `vault_audit` schema omitted the hash chain

Documented as five columns. The real table gains `entry_hash`, `prev_hash` and
`actor` via idempotent `ALTER TABLE` in `init_schema()`. Those three columns
_are_ the tamper-evidence that `envv scan --verify` checks - the doc omitted the
feature, not just the columns. A third table, `vault_meta`, went unmentioned.

### 6. A Platform note contradicted by its own Phase 15 section

`## Platform` said `GDK_BACKEND=x11` is set unconditionally in `run()`. Phase 15,
700 lines earlier in the same file, correctly says it is set only when the
session is not Wayland - and `configure_linux_webkit()` in
`src-tauri/src/lib.rs:835` confirms Phase 15 is right. Following the Platform
note would mean forcing XWayland and reintroducing the fractional-scaling bug
Phase 15 fixed.

Also in that section: the project path was `~/Code/ENVV`; the repo is
`~/Code/EnvVault`.

### 7. Three different test counts in one file

563 (Phase 11 row), 651 (Phase 12 row), 662 (Testing header and the `npm test`
comment). The real number is 671. Recording a number in four places is four
chances to disagree; it now lives only in the Testing section.

---

## Decisions made

- **Fixed collapsed card height, not a min-height.** "Change the height" could
  mean either, but only a fixed height puts the footer at a constant offset -
  which was the third requirement. Cards with little content carry slack in the
  head rather than shrinking.
- **Clamp to whole rows, never a fraction.** Badge rows, tags and the project row
  clamp to an exact multiple of their row height, so a clamp never slices a badge
  in half. Ellipsis is used wherever the overflowing thing is text.
- **Test counts recorded once.** The Testing section is the only place. Phase
  rows describe what a phase _did_, not how many tests existed when it ended.
- **Lists that mirror enumerable things now carry the enumerating command.**
  `grep -c '#\[tauri::command\]'` sits beside the command list; `src/ts/types.ts`
  beside the unions. A reader can check staleness in one step.
- **Kept the orphan worktree's branch.** Deleting 1.9 GB of working tree is
  reversible from the branch; deleting the branch is not. phase log 1 asked for
  both; I did the expensive half only.
- **Did not restructure the three-document contract.** CLAUDE.md and
  `CLAUDE.original.md` both violate it (see Next Steps). Splitting them is a
  larger, riskier edit than the factual corrections asked for, and it should be a
  deliberate decision rather than a side effect.

---

## What Worked

- **Reverting each fix to verify its test.** Four of nine new tests failed on the
  first revert pass; a fifth only failed after I reworded a CSS comment, because
  the regex was matching the literal `min-width:0` **inside the comment** rather
  than the declaration. That test had been passing for the wrong reason. This
  technique keeps earning its cost.
- **Verifying doc claims against `git log` instead of reading more docs.** The
  Phase 11 finding took one command. Five sessions of reading had not found it.
- **Checking a destructive delete three ways first** - commit reachability,
  branch existence, and a file-by-file hash comparison against the commit - then
  archiving the six differing files anyway.

## What Didn't Work

- **Eight parallel Opus subagents over a 40k-line repo.** Exhausted the session
  limit before any of them finished. Every report was lost because they were told
  to write at the end.
- **Relaunching all five at once after the reset.** Better, but still too wide;
  the user stopped it. Of five, only the docs/tests agent produced anything
  substantive - and only because that run wrote incrementally.
- **Waiting after the usage-limit reset instead of resuming.** The user had to
  say "you could have restarted". The reset notice ends the checkpoint
  constraint; it is not a standing stop.
- **Regexing CSS source as a test.** Five of the nine new tests do this because
  jsdom applies no stylesheets. They are a brittle proxy: they match comments,
  they anchor to the first matching rule and miss later cascade overrides, and
  one (`--cs-card-h` ordering) depends on declaration order in the file. They are
  better than nothing and worse than a real layout check.

---

## Next Steps

1. **Commit the tree.** 28 modified files (+2,335/−646) and 38 untracked files
   (8,036 lines) - Phases 13–15. This is the largest unversioned surface the
   project has ever carried. A commit plan was requested from the diagnosis agent
   but that section was never written; it needs redoing. **The user commits, not
   Claude** - hand over a pwsh one-liner.
2. **Finish the diagnosis.** Eight areas were scoped; one produced a partial
   report (`scratchpad/diag-8-docs-tests-ci.md`, Sections A and B only - C/D/E
   were never written). Not started: Rust core/Tauri, envv-server, envv-cli
   redaction, frontend invariants 1–9, chunks/exporters TS↔Rust drift,
   authz/remote/LAN, CSS/UX/a11y. Run these in **waves of two or three**, on
   Sonnet for the inventory work, and require incremental writes to disk.
3. **`CLAUDE.md` is not tracked by git.** `.gitignore:30` ignores `.claude/`, and
   CLAUDE.md, `CLAUDE.original.md` and every `Handoff-N.md` are untracked. The
   project's only memory has no history, no diff, and no backup - which is part
   of why drift went unnoticed for five sessions. Decide whether to track them.
4. **The three-document contract is not being followed.** CLAUDE.md carries a
   `## Phase History` and a `## Handoff - Post-Phase 6 Session` (both session-log
   material it says it must not hold). ~74% of `CLAUDE.original.md`'s 953 lines
   are a dated chronological Session History rather than pitfalls grouped by
   area, and it has **no Phase 15 entry at all**. Three overlapping session logs
   exist and none is authoritative.
5. **Still-open items confirmed by the audit** (all pre-existing):
   - `chunks/env-link.ts` and `chunks/edit-modal.ts` at 0% coverage.
   - The local vault key stays resident in `VaultState` for a whole remote
     session - no decision since Phase 12.
   - The CLI cannot pin a certificate (no `--ca-cert`, no custom verifier in
     `access.rs`), so a self-signed `envv-server` is unreachable from `envv`.
   - Experimental exporters (apache/haproxy/ansible/postgres) are UI-only.
   - No tests for the Phase 15 icon validator, `readIconField`/`setIconField`
     round-trip, the enrich signature table, or the `is_blank` secretType case.
6. **Five manual smoke tests, still never run** - listed in CLAUDE.md §Immediate
   Next Steps #2 since Phase 11. Only a human can do these.
7. **Decide on `architecture.dot` and the orphan branch** (phase log 1's remaining
   cleanup).

---

## Notes for whoever picks this up

The card work is finished and green; do not re-litigate it. The interesting
material this session is documentary, not functional.

The lesson worth carrying: **this project's docs are injected into every session,
so a false line in them is not an inaccuracy, it is a recurring instruction.**
"Commit Phase 11" survived five sessions and cost each of them a wrong first
priority. Before repeating any claim you inherited from CLAUDE.md or a handoff -
especially one about the state of the repo - spend the one command it takes to
check. CLAUDE.md now has a "Keeping this file true" section saying so.

If you resume the diagnosis, keep the surviving report
(`scratchpad/diag-8-docs-tests-ci.md`) - Sections A and B are verified work and
should not be redone. Its promise ledger is the single most useful artifact
produced this session.
