# Phase log 25: 2026-09-04 - Caps Lock bug, and Phase 19 finished (TOTP, onboarding, accessibility)

> **Edited 2026-09-04 - phase numbers renumbered.** Phases 21–36 as originally
> written in this file were ids allocated in design order; they are now queue
> positions, so every phase reference here has been rewritten to the new numbers
> and the plan table re-sorted. Nothing else was changed: findings, verification
> output, decisions and dates are as written. The old→new map is in `CLAUDE.md`
> under _Phase Status → Renumbering map_. **A record that is edited and says so
> beats one that is quietly wrong** - that was the call, and this line is how it
> stays honest.

## Goal

Two things, in the order asked:

1. Fix a reported bug: the unlock screen showed **"Caps Lock is on" regardless of
   the actual lock state**, and the warning disappeared as soon as anyone typed.
2. Do Phases 19 and 20.

Phase 20 turned out to be **already complete** - `tracing`, `/api/health`,
`Retry-After` and request ids all verified present in the tree, and 20.4 (the
`envv` binary-name collision) was closed as a decision in phase log 22 with no code
to write. So the phase work was all of Phase 19: TOTP for sub-users, the
onboarding wizard, and the accessibility pass. `envv use` (19.4) and the
Settings-panel overflow fix (19.5) were already done.

## Current Progress

Everything below is complete and verified.

```
npm test                     42 files, 928 tests, all passing
cargo test --workspace       251 tests, all passing
npm run typecheck            clean
npm run lint                 0 errors, 490 warnings (the pre-existing ratchet)
npm run lint:rust            clean
npm run format:check         clean
```

Version was 0.8.1 during the work and the user bumped it to **0.20.0** when
committing, which cuts a release: `build.yml` gates on whether `v<version>`
already exists, so the push is the trigger.

### The Caps Lock bug (`src/ts/ui-qol.ts`)

`wireCapsLockHint` read `KeyboardEvent.getModifierState('CapsLock')`. WebKitGTK
derives that from GDK's raw modifier mask, which is **not** the Caps Lock state:
it read true with Caps Lock off, so the hint appeared for every user on every
unlock - and `keydown` and `keyup` were both bound to the same handler and
disagreed about the value, so the hint flickered off again the moment anyone
typed. That is exactly the two symptoms as reported.

The fix stops asking. Caps Lock is now **derived from the character a key
actually produced**: `a` unshifted means off, `A` unshifted means on, Shift
inverts both. That is unambiguous and identical on every platform. Keys that
carry no evidence (digits, punctuation, Backspace, Enter, Shift itself) leave the
last reading alone rather than clearing it - which is the half that fixes
"disappears when typing". Blur voids the reading entirely, because Caps Lock can
be toggled while the field is unfocused.

The cost is that the hint cannot appear before the first letter is typed. That is
the right trade: a warning that is silent for one keystroke beats one that is
confidently wrong all session.

Eight tests in `tests/ui-persist.test.ts`; five of them fail against the old
implementation.

### 19.2 - TOTP for sub-users

Greenfield: `vault-core` had TOTP in Phase 5.1 and it was removed in Phase 7
because nothing ever reached it. It is back with all four surfaces.

| Layer                     | What landed                                                                                                                                                                                                                                                                                                        |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `vault-core/src/totp.rs`  | RFC 6238, HMAC-SHA1, 6 digits, 30-second step, ±1 skew. Base32 hand-rolled (forty lines, the way HKDF is in `entropy.rs`). Ten tests, including the **published RFC 4226 and RFC 6238 vectors** - without those the file could be self-consistently wrong and every authenticator on earth would disagree with it. |
| `vault-core/src/users.rs` | `totp_secret` / `totp_enabled` / `totp_last_step` columns by idempotent `ALTER TABLE`; `totp_enroll`, `totp_confirm`, `totp_disable`, `totp_status`, `totp_required`, `verify_user_totp`. Eleven tests.                                                                                                            |
| `envv-server`             | `/api/auth` takes `totp`; four routes under `/api/users/{id}/totp`, gated by `guard_manage_user` - the same tier check that governs setting a password, because managing someone's second factor is exactly as strong as resetting their credentials.                                                              |
| `envv-cli`                | `envv user totp status\|enroll\|confirm\|disable`, plus a global `--totp` / `ENVV_TOTP`.                                                                                                                                                                                                                           |
| Desktop app               | A Two-Factor section in the Users panel, four new Tauri commands (46 total), and a code prompt on remote sub-user login that only appears when the server says one is required.                                                                                                                                    |

Four decisions worth keeping:

- **Enrollment is two-phase and does not enable anything.** `enroll` mints the
  secret; `confirm` requires a working code before the factor is switched on.
  There is no QR code (the CSP allows no external script, and relaxing it in
  order to draw a _secret_ is a poor trade), so the secret is typed by hand from
  a grouped display - and manual entry that did not take is a routine outcome,
  not an edge case. Enabling on enrollment would lock those users out.
- **Sub-users only, enforced in `vault-core`.** The owner authenticates by
  deriving the SQLCipher key; there is no stored hash and no login form in that
  path, so a factor there gates nothing while looking as though it gates
  everything. Enforced in the shared crate rather than at each caller, because a
  check that lives in the server is one the CLI and the app each have to
  remember separately.
- **Token auth skips TOTP entirely.** Only the password path is gated. Demanding
  a code from CI, where no human is present to read a phone, is a regression this
  project already shipped once (Phase 5.1, in the bug history).
- **The secret is a materialising path.** `envv user totp enroll` refuses to
  print without `--out` or `--reveal`, and refuses _before_ minting - the same
  rule and the same ordering as `user token new`, for the same reason: enrolling
  and then declining to print leaves a secret nobody can reach and no `confirm`
  can be run against.

### 19.1 - Onboarding wizard (`src/ts/onboarding.ts`)

Four steps, shown once after a vault is created, re-runnable from Settings →
Setup wizard. Deliberately not a tutorial: every step either writes a setting the
user would otherwise never find (auto-lock, lock-on-hide, masking) or performs
the one action that turns an empty vault into a useful one.

- **The overlay is built and destroyed per showing, not hidden.** Every handler
  is therefore bound to nodes that exist for exactly one run, so invariant 9
  holds by construction - and it sidesteps the WebKitGTK ghost widgets that
  static hidden markup produces.
- **Only "Next" commits a step.** Skipping leaves every setting exactly as it
  was; the wizard never writes something the user merely looked at.
- **Skip marks it done.** Re-showing a wizard somebody dismissed on purpose is
  the fastest way to make it an obstacle.
- **It never runs on a remote vault.** Connecting to a remote is not a first run,
  and the wizard writes local display settings that have nothing to do with the
  server just reached.

Fifteen tests in `tests/onboarding.test.ts`.

### 19.3 - Accessibility pass

The starting point, measured rather than assumed: `index.html` had **3
`aria-label`s, 0 roles**, 157 buttons and 58 labels of which **1** was bound to
its control. `render.ts` had zero `aria-` anywhere.

Sequenced deliberately around Phase 25 (escape by construction). phase log 23 warns
that doing the a11y pass first means writing every attribute into raw template
literals and then migrating them. What landed here is the half that does **not**
pay that cost twice: static attributes, landmark structure, live regions, state
attributes maintained in the same place the CSS class is maintained, and
keyboard behaviour - none of which changes shape when the renderer moves to an
escaping tagged template. The interpolated `aria-label`s on card buttons use
`escAttr`, exactly as every other attribute in that file already does, so they
migrate with it rather than in addition to it.

- **Landmarks and a skip link.** `role="banner"`, a labelled `<aside>`, a
  focusable `<main>`, and a skip link as the first focusable element - a keyboard
  user previously crossed the header, the activity bar and the entire sidebar
  before reaching one secret.
- **The activity bar is a real tablist** - roles, `aria-selected`, a roving
  navigation. A roving tabindex without arrow keys would have left three tabs
  unreachable, which is worse than the plain button list it replaced.
- **State attributes move with the class they mirror.** `aria-expanded` on the
  card chevron, the description disclosure, the sidebar toggle and the sidebar
  sections; `aria-pressed` on reveal and pin; `aria-current` on every filter,
  which is the single most important piece of state in this app - invariant 7
  exists because a restored filter can hide every secret, and a colour alone
  cannot say so. Each is written where the class is written, including the paths
  that mutate a card in place instead of re-rendering.
- **Every overlay is a dialog** with `aria-modal` and a name, and there is now
  real focus management (`wireModalFocus` in `ui-qol.ts`): focus moves in on
  open, Tab is trapped in the topmost overlay, and focus returns to whatever
  opened it. These are divs that gain a class, not `<dialog>`, so the browser
  does none of it for us.
- **Div-based widgets declare what they replaced.** The project picker is a
  `role="listbox"` with keyboard selection, because it exists only to dodge a
  WebKitGTK ghost listbox and the native control it replaced was operable. The
  project-type picker is a `radiogroup`. The sidebar resizer is a `separator`
  mouse-only handle leaves a keyboard user stuck with whatever width they
  inherited.
- **46 labels bound to their controls**, 12 settings toggles named, and 4
  settings rows restructured so the _title_ is the accessible name and the hint
  is a description - previously `aria-labelledby` on those rows would have
  announced a sixty-word paragraph as a checkbox's name.
- **`src/css/a11y.css`**: `.sr-only`, the skip link, one `:focus-visible` ring
  for the whole app, `prefers-reduced-motion`, and `forced-colors` borders.

`tests/a11y.test.ts` (21 tests) asserts the contract mechanically rather than by
inspection: every button and control has an accessible name, no `for=` or
`aria-*` reference dangles, the tablist has exactly one tab stop and it is the
selected one, every overlay is a named modal dialog, and the focus trap actually
traps. Five of them fail with `wireModalFocus` disabled.

## The bugs that mattered

Ranked by consequence.

1. **`totp_confirm` ignored the anti-replay mark, and could move it backwards.**
   Mine, found by an end-to-end CLI run and not by any unit test. It passed
   `None` as the last accepted step, so the confirming code worked a second time
   - and re-confirming with a code from an _earlier_ step wrote that lower step
     back to the database, re-opening every step in between for replay on the login
     path. The anti-replay rule is the entire reason this module has a `last_step`
     column, and the one code path that wrote it did not read it. Fixed, with a
     regression test that asserts both the refusal and that the mark does not move.
     **The unit tests all passed while this was broken**; running the real binary
     is what found it.
2. **The Caps Lock warning, as reported.** Wrong for every user, all the time,
   on the one screen where a wrong warning does damage: a Caps Lock slip on a
   master password is indistinguishable from a wrong password, so a hint that is
   permanently on trains people to ignore the one signal that would explain it.
3. **Two `.route()` calls for the same path panic axum 0.8 at startup, not at
   compile time.** The TOTP status and disable handlers were written that way
   first; the binary compiled and would have failed to boot. Same class as the
   `:param` → `{param}` change already in the pitfalls list. There is now a test
   that constructs the router.
4. **`offsetParent !== null` is the usual "is it visible" shorthand and it is
   wrong twice over.** It is null for _every_ `position: fixed` element even when
   visible - and every overlay in this app is fixed - and jsdom does no layout,
   so it is null for everything under test. The focus trap found no focusable
   elements at all. Replaced with a computed-style ancestor walk, which is
   correct in both and also catches `visibility: hidden`.
5. **A MutationObserver fires after its document is gone.** The focus observer
   threw an unhandled `ReferenceError: document is not defined` on test teardown,
   with a stack naming only itself. Guarded.

## Decisions made

- **Caps Lock is derived from typed characters, never from `getModifierState`.**
  Platform-independent and provably correct, at the cost of one keystroke of
  silence.
- **`confirm` and `verify` share one anti-replay mark**, and enrollment is the
  only thing that clears it.
- **The a11y pass was split around Phase 25** rather than deferred wholesale.
  phase log 23's warning is about attributes written into raw template literals;
  landmarks, roles, focus management and keyboard behaviour do not have that
  shape, and holding them back would have meant holding back the entire phase for
  a migration that has not been scheduled.
- **`aria-labelledby` on a settings row points at a `<span>` around the title,
  not at the whole label block.** Pointing at the block makes the hint paragraph
  part of the control's _name_.
- **The onboarding wizard marks itself done when skipped.** Settings has an
  explicit "Run setup again".
- **TOTP routes are not in the OpenAPI document.** Neither is any other
  user-management route (`ApiDoc` covers ten paths, all vault-level), so this
  keeps an existing boundary rather than opening a new gap. `envv describe`
  _does_ cover the CLI side, for free, because it is generated from the same
  `clap` tree.

## What Worked / What Didn't Work

**Worked.**

- **Running the real binary after the unit tests were green.** That is what found
  the `totp_confirm` replay hole. Twenty-one passing tests said the module was
  correct; one end-to-end CLI session said otherwise.
- **Published test vectors.** RFC 4226 and RFC 6238 rows caught nothing, which is
  the point - without them, "our implementation agrees with our implementation"
  is all the suite would have proved.
- **Asserting the a11y contract mechanically.** The test that names every button
  and control found `copy-all-arrow` and twelve settings toggles that a reading
  pass had already walked past; the dangling-reference test is what makes the 46
  scripted `for=` bindings trustworthy.
- **Reverting each fix to check its tests fail.** Done for the Caps Lock change
  (5 of 8 fail) and the focus trap (5 of 5 fail).

**Didn't work.**

- **Assuming a phase was unstarted because the table said 🟡.** Phase 20 was
  finished; the table had not moved. Checking the tree first cost ten minutes and
  saved a day.
- **`npm run build` when only the frontend changed.** It runs the full Tauri
  release build. `npx vite build` is the thing to run.
- **`setTimeout(5)` as a way to wait for a `setTimeout(0)`.** Not reliably
  ordered after one registered a microtask later; the focus tests raced. Two
  nested macrotasks.

## Next Steps

- **Phase 21 is still the next thing**, unchanged: `${Provider/ID}` resolves to
  the entry UUID in the CLI and to nothing in the app. It is a live defect, one
  line each side, and it ships alone.
- **Manual smoke tests only a human can run** (jsdom has no Tauri, and none of
  this has been in a real WebKitGTK window):
  1. **The Caps Lock fix, on the actual unlock screen.** Open the app with Caps
     Lock _off_, type a letter - no hint. Turn it on, type a letter - hint.
     Keep typing digits and Backspace - the hint stays. This is the reported bug
     and it is the one thing here that must be confirmed on the real widget.
  2. **The onboarding wizard**, which only fires on a genuinely new vault.
     Easiest path is Settings → Setup wizard.
  3. **TOTP enrollment in the Users panel** against a real authenticator app -
     the base32 grouping and the `otpauth://` URI are what a phone has to accept.
  4. **Keyboard-only navigation**: Tab from the top of the window should hit the
     skip link first; the activity bar should be one tab stop with arrow keys
     moving between panels; opening any modal should move focus into it and
     return it on close.
- The three items only a human can decide are unchanged from phase log 22: whether
  237 lint suppressions ship in a 1.0, whether lock-on-switch stays the default,
  and whether the project is renamed.

## Notes for whoever picks this up

- **`vault-core/src/totp.rs` is a twin of nothing.** It is the only
  implementation, and both the CLI and the app reach it - the shape `pools.ts`
  established and the one to prefer. Do not add a TypeScript TOTP.
- **`getModifierState` is not trustworthy under WebKitGTK.** It is in the
  pitfalls list now. If another modifier hint is ever added, derive it from
  observable behaviour rather than asking.
- **`tests/a11y.test.ts` fails loudly on new unnamed controls.** That is
  deliberate. Adding a button with no text and no `aria-label` will break the
  suite; add the label rather than the exemption.
- **The tree is committed and pushed** at `3a3e3f0`, version 0.20.0.
- **The push was blocked once by GitHub secret scanning**, on
  `sk_live_` + 48 zeros in `ui-lab/seed.ts` - a zero-filled layout placeholder
  from the _previous_ commit (`a8b67fe`), not from this work, and not a real
  credential. Fixed properly rather than allowlisted: the three fixtures in that
  file now carry `EXAMPLE_<PROVIDER>_` prefixes at identical character counts
  (length is the only property the UI lab needs), and the two unpushed commits
  were rewritten with `git filter-branch --tree-filter` so the string is in no
  tree at all. Both pitfalls are in `CLAUDE.md` under _Git (working-tree
  safety)_. The repo still holds ~12 similar fixtures (`ghp_`, `glpat-`,
  `dop_v1_`, `AKIA`) in `envv-cli/tests/` that pass only because they are short
  or already pushed - expect one of them to block a push eventually.

> 2 line(s) about the maintainer's own machines were left out of this page.
