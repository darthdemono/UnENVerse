# Phase log 13: 2026-08-08 - Frontend test suite and module audit (phase 11)

Frontend test suite built from zero, then a module-by-module bug hunt across
every TypeScript file in `src/ts/`. **66 bugs found and fixed, 563 tests added.**

phase log 1 left the Rust side at 83 tests and CI green. The TypeScript side had
**no tests at all** - that is what this session addressed.

---

## Goal

Started as "add UI tests and run them". The first two suites immediately
surfaced real defects, so the session became: build the test infrastructure,
then walk every module looking for more of what the tests had just found.

---

## Current Progress - all complete and verified

| Pass | Area                                                              | Bugs |
| ---- | ----------------------------------------------------------------- | ---- |
| 0    | Test infra + `filters` / `permex` / `utils` / `modals` / `render` | 2    |
| 1    | Two bugs you reported (remote connect, switch-to-local)           | 2    |
| 2    | `projects.ts`, `import-export.ts`, `modals.ts` - same bug class   | 9    |
| 3    | `tools.ts`, `chunk-ops.ts` + the three previously flagged         | 11   |
| 4    | `users.ts`, `vault.ts`                                            | 8    |
| 5    | `render.ts` escaping                                              | 8    |
| 6    | `audit.ts`, `lock.ts`, `icons.ts`                                 | 6    |
| 7    | `settings-panel.ts`, `filters.ts`, `perm-editor.ts`               | 5    |
| 8    | `import-export.ts`, `remote-panel.ts` (+ Rust)                    | 6    |
| 9    | `chunks/parsers.ts`, `chunks/starters.ts`                         | 3    |
| 10   | `chunk-ops.ts` export functions                                   | 4    |
| 11   | `chunk-ops.ts` remaining render paths                             | 2    |

**Verification at handoff:**

```
npm test              563 passed, 0 failed   (22 test files)
npx tsc --noEmit      clean
npx vite build        ✓ built, no warnings
cd src-tauri && cargo check   clean
```

Every fix has a regression test, and for each non-obvious one I reverted the fix
and confirmed the test fails. Where a fix was cosmetic-looking but the test
passed both before and after, I said so rather than claiming a catch.

---

## Test infrastructure (new)

**Stack:** Vitest 4 + jsdom. `npm test`, `npm run test:watch`,
`npm run test:coverage`, `npm run typecheck`.

**The load-bearing decision:** tests load the **real `index.html`** as their
fixture (`tests/helpers.ts` → `loadRealIndexHtml()`), not a hand-written mock.
This project has a documented history of a formatter silently dropping an
element that JS still queries (Phase 3, `#new-category-form`). A mock fixture
would keep passing after that happens; the real markup fails loudly. It caught a
stale `#grid` reference in my own test on the first run.

**Three fixture patterns, use the right one:**

| Situation                                                           | Pattern                                                                                                                                                                                                                                   |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pure functions                                                      | Import and call. No DOM.                                                                                                                                                                                                                  |
| Renderers                                                           | `loadRealIndexHtml()` in `beforeEach`, then call the render fn.                                                                                                                                                                           |
| Modules that bind listeners once (`initTools`, `vault.ts` `init()`) | `beforeAll`: load HTML → `mountToolsPanes()` → init once → snapshot `document.body.childNodes`. `beforeEach`: re-**append** those nodes. Re-parsing the HTML gives every test a fresh, unbound DOM and every click silently does nothing. |

**Gotchas already handled in `tests/setup.ts`:** jsdom has no `matchMedia`
(read at module scope by `state.ts`), no clipboard, no `execCommand`. The tool
panes are injected at runtime by `mountToolsPanes()` and are **not** in
`index.html` - suites touching `#audit-*`, `#sg-*`, `#bulk-*` must mount them
first.

**`tsconfig` targets ES2020**, so `Array.prototype.at` and other ES2022 methods
typecheck-fail in tests even though they run. Tests are in the `include` now, so
`tsc --noEmit` covers them.

---

## The one bug class behind ~40 of the 66

**Something holds a reference; the thing it points at changes; nobody tells the
holder.** Three shapes:

### 1. Identity by array index

`api_keys` positions were used as identity in five places. Any splice between
capturing the index and using it silently retargets the operation.

- **Bulk delete destroyed the wrong secrets.** `_bulkSelected` was a
  `Set<number>` of positions. Tick a card, delete a different one, press Delete.
- **Bulk export wrote the wrong secrets** to a plaintext file - same cause.
- **The card context menu** captured `idx` at open time and acted on it at click
  time.
- **The diff tool's `<option value>`** was an index, and the selects only
  rebuild when the tool is opened.
- **Undo restored to a captured index**, so a second delete misplaced the first.

All now key on entry `id`. `st.bulkSelected` moved into `st` so
`resetViewState()` clears it and `buildCard` can re-apply the tick across a
re-render.

### 2. Identity by name, with no cascade

- **`renameProject` / `renameCategory` / `deleteCategory` ignored the slash
  sub-tree.** Renaming `Acme` orphaned `Acme/Web`, and `buildProjectTree` then
  invented a phantom virtual parent.
- **`deleteProject` detached entries from surviving sub-projects.** Promoting a
  child changed its id; the prune pass three lines later saw every entry's
  reference as dangling and stripped it. Deleting a parent silently emptied
  every sub-project under it. **Worst data-loss bug of the session.**
- **Renaming an entry broke every `${Provider/field}` chunk reference.** The
  security audit could already _detect_ the resulting stale refs; nothing
  prevented them. `renameProviderRefs()` now cascades.

### 3. View state outliving its data

- **Switching vaults / importing a backup left the old project selection, tag
  filter and search query in place** - so a successful import rendered an
  **empty grid**. Data present, invisible.
- **`st.revealed` is keyed by entry id**, so an imported entry reusing an id
  rendered its secret **unmasked** without anyone asking.
- **Audit rows, user ids and class ids outlived a vault switch** - the panel
  reported on, and exported, the _previous_ vault's data.
- **Pending undos survived `lockVault`**, holding deleted secrets in memory with
  a live Undo button on a locked vault.

Fixed by one shared `resetViewState()` in `state.ts`, plus
`resetUsersPanelState()` and `resetAuditPanel()`, called from every path that
replaces the vault: import, backup restore, vault switch, lock.

---

## The bugs that mattered most

Ranked by consequence, not by how hard they were to find.

1. **`deleteProject` silently emptied surviving sub-projects** (data loss, above).
2. **HTTPS to a self-signed server was impossible.** `remote_request` pins when
   given a fingerprint and applies normal CA validation when not - with no
   accept-invalid path anywhere. So connecting needed a fingerprint obtainable
   only by connecting. **Phase 6's headline feature could never work.** Fixed
   with a `probe_cert_fingerprint` Tauri command (handshake + unauthenticated
   `GET /api/status`, no credentials) and an SSH-style confirm prompt.
3. **Audit log truncation verified as intact.** Nothing required the oldest
   hashed row to anchor to `genesis`, and the prev-hash link check is skipped on
   the first iteration for want of a predecessor. Deleting rows from the _start_
   of an append-only log - the most useful thing to erase - was the one edit the
   chain could not see.
4. **Encrypted backup silently failed on real vaults.** `b64` spread every byte
   as a call argument; past ~100 KB of ciphertext it threw an uncaught
   `RangeError`. No file, no error. It failed for exactly the vaults with the
   most to lose.
5. **`exportK8s` mis-encoded Kubernetes Secrets two ways.** `btoa` threw above
   U+00FF (killing the whole export), and encoded U+0080–U+00FF as Latin-1
   rather than UTF-8 - so the cluster silently decoded a _different_ secret.
   The quiet corruption is the worse half.
6. **Sub-user passwords were typed in a visible cleartext field.**
   `changePassword` used `showPrompt`, not `showPasswordPrompt` - which exists
   precisely because the plain one echoes in clear.
7. **Searching for a URL returned the entire vault.** `parseSearch` made a
   filter of any colon-bearing token, so `postgres://user` became the unread
   filter `postgres="//user"` and emptied the text query - and an empty query
   matches everything.
8. **A blank auto-lock box silently disabled the inactivity lock.**
   `parseInt(...) || 0`, and `0` means never. The field is `type="number"`, so a
   mistyped letter produces exactly that blank.
9. **Six vault-controlled fields interpolated raw into `innerHTML`** -
   `environment`, `secretType`, `price_type`, `expires_at`, `rotation_days`,
   `project_type`. `key_id` on the adjacent line was already escaped, so this
   was oversight, not a trust decision.
10. **WireGuard private keys imported as non-secrets.** `parseWgConf` compared
    key names with `===` while every neighbouring test in the same function is
    case-insensitive - and `wg-quick` accepts any casing.

---

## Decisions made this session

| Decision                                                           | Rationale                                                                                                  |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| Tests load the real `index.html`                                   | A mock fixture cannot catch element-id drift, which has bitten this project before                         |
| Bulk selection lives in `st`, keyed by id                          | Lets `resetViewState()` clear it and `buildCard` re-apply the tick; mirrors the existing `st.expanded` fix |
| One `resetViewState()` shared by every vault-replacing path        | These paths had drifted apart three separate times                                                         |
| `switchToLocalVault()` shared by Disconnect and the vault switcher | They had already diverged; one had a bug the other didn't                                                  |
| Exports tested by round trip through the matching parser           | A copied config a parser cannot read back is one the server will not read either                           |
| TOFU probe is a separate Rust command                              | Keeps the unauthenticated, unverified handshake confined to one call that carries no credentials           |
| Fingerprint changes warn and refuse, never re-pin                  | Silent re-trust made the pin hold only until the first mismatch - the one moment it matters                |
| `docker_service` keeps `${VAR}` placeholders on copy               | Compose substitutes from the `.env` written beside it; that placeholder is correct output                  |
| Backup password floor raised 8 → 12                                | Matches the master password; the file is the whole vault and is meant to leave the machine                 |

---

## What Worked

- **Probe before claiming.** Several suspicions were wrong. Running a scratch
  test first (`parseSearch` with a URL, `btoa` with an emoji, `String.fromCharCode`
  at 300 KB) turned each into a confirmed finding with a reproduction, and
  killed the ones that weren't real.
- **Revert-the-fix verification.** For every non-obvious fix, reverting it and
  watching the test fail proved the test actually pins the behaviour. This
  caught two tests that passed for the wrong reason.
- **Reading adjacent lines.** The strongest signal throughout was internal
  inconsistency: `key_id` escaped while `environment` wasn't, `exportNginx`
  handling upstreams while `chunkToString` didn't, `starters.ts` marking
  `PrivateKey` secret while `parseWgConf` didn't. Each was a real bug.
- **Round-trip testing** for parser/export pairs - far better at finding real
  breakage than asserting on strings.

## What Didn't Work

- **Assuming the docs.** `CLAUDE.md` said `remote_request` uses
  `danger_accept_invalid_certs(true)`. It does real handshake-time pinning.
  Reading the Rust was what revealed the TOFU deadlock. (Corrected in `CLAUDE.md`
  this session.)
- **Writing tests from assumed behaviour.** Three of my tests were wrong, not
  the code: `.env` backslash continuation concatenates without a separator
  (shell semantics), `yamlStr` quotes `nginx:alpine` because of the colon, and
  a `type="number"` input discards letters rather than passing them through.
- **Re-parsing `index.html` per test for listener-bound modules.** Orphans every
  handler; every click silently no-ops and the tests fail confusingly.
- **Testing `starters.ts`.** Pure constant factories - the tests would assert
  the constants against themselves. Left at 0% deliberately.

---

## Coverage at handoff

```
All files          40.02 %
permex 96.9   perm-editor 94.3   filters 92.2   parsers 91.8
utils 71.0    modals 68.4        projects 60.1  import-export 50.9
audit 49.4    state 44.1         chunk-ops 35.7 lock 31.4
lan 30.5      icons 28.7         remote-panel 27.1  vault 26.6
users 25.6    render 22.6        tools 19.8
starters 0 (deliberate)   chunks/env-link 0   chunks/edit-modal 0
```

---

## Next Steps

### 1. Commit - nothing from this session is committed

The working tree also carries unrelated pre-existing changes (CSS, `README.md`,
`Dockerfile`, the `apiv-*` → `envv-*` rename). Per-pass commit commands were
given in the session; each stages only its own files. Review the tree first -
`git status` shows ~40 modified paths, most of them not mine.

### 2. Manual smoke tests - three things the suite cannot reach

- **TOFU flow end to end.** Start `envv-server --tls`, add it as a remote, hit
  Test Connection. You should get a fingerprint confirm prompt, then a working
  connection. jsdom cannot exercise the Rust proxy.
- **Switch-to-local relock prompt.** Connect to a remote from the _startup_
  screen, then switch to Local Vault. Expect the relock screen reading
  "Switched to the local vault." Only fires under `inTauri`.
- **k8s Secret export** with a non-ASCII secret - verify the cluster decodes it.

### 3. Remaining untested surface

`chunks/env-link.ts` and `chunks/edit-modal.ts` are at 0% and were never
audited. `render.ts` (22.6 %) and `tools.ts` (19.8 %) have large untested
regions - the config-view click handlers and the generator/calculator tool panes
respectively.

### 4. Consider

- Add `npm test` to the pre-commit path (CI already runs it - see
  `.github/workflows/ci.yml`, frontend job).
- The three items flagged but deliberately not fixed, all judgement calls:
  `findLinkedCert`'s fallback can return a non-certificate entry;
  `ensureCertForDomain` can create a second entry sharing a provider name;
  `copyAll` exports the filtered view while `exportAs` exports everything.

---

## Notes for whoever picks this up

- **Every fix has a test.** If you change behaviour in `src/ts/`, a test will
  tell you. Run `npm test` before `vite build`.
- **The bug class above is the thing to keep watching.** When you add state that
  points at an entry, a project, a user or a certificate, ask: what clears it
  when that thing is renamed, deleted, or replaced wholesale? Three separate
  paths in this codebase got that wrong in three different ways.
- **`resetViewState()` is the hook.** Anything view-scoped that can outlive its
  data belongs in it.
