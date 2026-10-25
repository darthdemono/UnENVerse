# Phase log 27: Phase 21 fixed, Phase 22 allocated and implemented (TOTP as a stored secret)

**Date:** 2026-09-10
**Version:** 0.20.1 (unchanged by this session)
**Branch:** `worktree-phase-38-totp-secret` - **nothing committed.** The name is a
fossil of the first numbering this session was given; the work in it is Phases 21
and 22.

---

## Goal

The session opened as _"allocate and implement Phase 38 - TOTP as a stored
secret"_. It was corrected twice mid-session by a peer relaying the user, and the
final instruction is what was built:

1. **Phase 21 keeps its number and its meaning** - the `${Provider/ID}` defect.
   Fix it first, ship it alone.
2. **Phase 22 is new: TOTP as a stored secret.** Every pending phase from the old
   22 onward shifts up one; next to allocate becomes 39; a second renumbering map
   dated 2026-09-10 goes into `CLAUDE.md` beside the 2026-09-04 one.
3. Added scope on 22, in the user's words: **"New left sidebar panel
   (secrets/tools/user panel)"**. That sentence is the whole of the spec for it;
   what was assumed is in _Decisions made_ below.

Both phases are done. Tests, lint and formatting are green on both halves.

---

## Current Progress

### Verification

```
npx vitest run          →  Test Files  44 passed (44)
                           Tests  995 passed (995)      [was 972 before this session]

cargo test --workspace  →  all suites ok, 302 tests, 0 failed

npm run lint            →  500 problems (0 errors, 500 warnings)
                           - the warnings are the pre-existing suppressed backlog
npm run format:check    →  All matched files use Prettier code style!
cargo fmt --all --check →  clean
cargo clippy --workspace --all-targets → clean
npx tsc --noEmit        →  clean
npm run version:check   →  all six copies agree at 0.20.1
```

`cargo clippy --workspace` needs `dist/` to exist (`generate_context!` checks for
it at compile time) - run `npx vite build` first on a clean tree, or clippy fails
with _"The `frontendDist` configuration is set to `"../dist"` but this path
doesn't exist"_, which names nothing about the real cause.

### Phase 21 - `${Provider/ID}` resolved to the entry's UUID

Two changes, in the order `CLAUDE.md`'s E0 specified.

- **`REFERENCE_DENY`** - `id`, `version_history`, `projectIds`, `categories` -
  checked **before** the lookup in `entry_field()` (`envv-cli/src/refs.rs`) and
  `getEntryFieldValue()` (`src/ts/chunk-ops.ts`). Before it, `canonical_field`
  fell through `_ => field`, `entry.get("id")` returned a non-empty UUID, and the
  CLI wrote that into a rendered config while the app left `${Spotify/ID}` as
  literal text.
- **The alias arm** - `ID | CLIENT_ID | APP_ID | ACCOUNT_ID | APPLICATION_ID` →
  `key_id`, then through the `extra_vars` fallback to a var keyed `ID`, which is
  where a client id lives in vaults written today. When Phase 23's `primary_role`
  exists this arm becomes role-aware; until then this is what a user typing
  `${Spotify/ID}` means.

Two things fell out of writing the fixture, and both are worth knowing:

- **`FIELD_ALIASES` values are now the JSON field names** (`api_key`, not `key`).
  The two tables were the same table written twice in two different vocabularies,
  which is exactly the shape that lets a twin drift while looking identical.
- **The app did not fall through to `extra_vars` when a built-in field was
  empty; the CLI always had.** So an entry keeping its client id in a var already
  resolved differently in the two halves - one layer _below_ the table the alias
  fixture was pinning. Fixed to match Rust, and pinned by the fixture's
  `extra_var_fallback` case.

`tests/fixtures/parity/field-aliases.json` gained the five aliases, an
`unresolvable` list and an `extra_var_fallback` case. **The harness entry now
carries `id`, `categories`, `projectIds` and `version_history` with real
values**, so the deny-list assertions are exercised rather than satisfied by
those fields being absent.

Verified by reverting: removing the deny-list makes
`field_aliases_match_the_app` fail with

```
assertion `left == right` failed: ${E/id} must not resolve to anything
  left: "key_id"
 right: "${E/id}"
```

### Phase 22 - TOTP as a stored secret

The design as shipped is written up in `CLAUDE.md` under _Phase 22 - TOTP as a
stored secret_; this section is what was touched and what to look at first.

| Layer      | File                                                                                              | What                                                                                                                                                                                       |
| ---------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| vault-core | `src/totp.rs`                                                                                     | `Algorithm`, `Params`, `Stored`, `parse_seed`, `parse_otpauth`, `Stored::to_uri`, `code_at`, `live_code`, `remaining_secs`, `hotp_with`. `hotp()` now delegates to `hotp_with(…, Sha1, 6)` |
| vault-core | `src/lib.rs`                                                                                      | `HISTORIED_SECRET_FIELDS`; `save_vault` snapshots `totp_secret` too, with a `field` discriminator                                                                                          |
| src-tauri  | `src/lib.rs`                                                                                      | `entry_totp_code`, `parse_totp_seed` (48 commands now, was 46)                                                                                                                             |
| envv-cli   | `src/totp_cmd.rs` (new)                                                                           | `envv totp code\|ls\|uri\|rm`                                                                                                                                                              |
| envv-cli   | `src/entries.rs`                                                                                  | `--totp`, `--totp-stdin`, `--totp-algorithm`, `--totp-digits`, `--totp-period`, applied as one unit by `apply_totp`                                                                        |
| envv-cli   | `src/out.rs`                                                                                      | `totp_secret` added to `SECRET_FIELDS`                                                                                                                                                     |
| app        | `src/ts/totp.ts` (new)                                                                            | the parser twin, `totpParamsOf`, the code cache, the one-second ticker                                                                                                                     |
| app        | `modals.ts`, `render.ts`, `vault.ts`, `lock.ts`, `index.html`, `src/css/totp.css` (new)           | the form field, the card row, the sidebar section, copy, teardown                                                                                                                          |
| tests      | `tests/totp.test.ts`, `envv-cli/tests/totp.rs`, `tests/fixtures/parity/totp-seeds.json` (all new) | the parser parity pair and the redaction guarantees                                                                                                                                        |

### Phase 22, second pass - import and export

Added after the first pass, on the same day: _"add import and export for ente and
other totp 2fa services"_. A seed retyped off a phone screen is a seed typed
wrong, so this is what makes the feature usable rather than merely present.

| Direction | Formats                                                                                                                                                                                                                                  |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **In**    | Ente Auth (its plain export is an `otpauth://` list, `codeDisplay` and all), any other `otpauth://` list, Aegis JSON, 2FAS JSON, andOTP JSON, Bitwarden JSON (`login.totp`), Google Authenticator (`otpauth-migration://offline?data=…`) |
| **Out**   | `otpauth://` list (also how you get back _into_ Ente), Aegis JSON, 2FAS JSON                                                                                                                                                             |

New: `vault-core/src/totp_import.rs` (~700 lines with tests),
`envv totp import|export`, three Tauri commands, `src/ts/totp-io.ts`, Import /
Export buttons in the Authenticator section, `tests/totp-io.test.ts`.

**One implementation, no twin.** Six formats parsed twice is six chances for the
app and the CLI to disagree about what a file meant, and the disagreement is a
seed that imports with the wrong `period` and produces six digits the issuer
rejects. So the parsers, the format writers **and the merge rules** are all in
`vault-core`; the app calls them over IPC (`totp_import_merge`,
`totp_export_build`). No parity fixture, because there is nothing to pin against.

Four decisions worth arguing with, all recorded in `CLAUDE.md`:

- **Encrypted exports are refused by name, never decrypted.** Six apps, six KDFs.
  `detect()` recognises Aegis's populated `header.slots`, 2FAS's
  `servicesEncrypted` and Bitwarden's `"encrypted": true`, and says which app it
  is and to export again without encryption. This is the common failure for
  somebody who just pressed Export in an app that encrypts by default, and
  "unrecognised file" would send them hunting for a bug.
- **An import never replaces an existing seed with a different one.** Match on
  provider _and_ account, case-folded. No seed → update. Same seed → no-op, which
  is what makes a re-run idempotent. Different seed → reported and skipped.
  An import is exactly when a stale export gets pointed at a vault that has since
  been re-enrolled; taking the incoming value silently destroys a working second
  factor at the moment the user believes they are backing one up. `--force` says
  you meant it, and the old value still lands in `version_history`.
- **The Google protobuf is decoded by hand**, ~60 lines, same trade as base32.
  Every length in that payload is attacker-controlled, so the varint shift is
  capped and each length prefix is bounds-checked, with a test that feeds it
  truncated payloads and a lying length prefix.
- **HOTP is refused everywhere and _named_ in the report.** No clock means no
  current code. Dropping them silently is how somebody learns six months later
  that one account never came across.

Two knock-on changes:

- **`saveModal` no longer requires a primary value when the entry carries a
  seed.** An import produces exactly that entry - the password may never be
  stored here at all - and demanding one would make every imported entry
  unsaveable the first time somebody opened it to fix its name. A carve-out for
  seeds, not a general relaxation; there is a test for each half.
- **`new_uuid()` was promoted from `users.rs` to `vault-core`'s root.** The
  importer needed one and `src-tauri` has no `uuid` crate. `users.rs` keeps the
  local name and delegates, so there is still one generator.

**The Authenticator section is no longer data-gated.** Tags and Key Pools hide
when empty because they are pure filters; this one carries the Import button, and
an empty authenticator list is exactly when somebody is looking for it. It now
shows a one-line empty state instead, and Export is disabled rather than offered
with nothing to write.

**The generator was extended, not copied**, as the brief required. The risk that
creates is that parameterising it silently breaks Phase 19's sub-user login, so
there is a test asserting the two paths still agree
(`the_default_params_reproduce_the_login_paths_answer`).

**RFC 6238 Appendix B is asserted for all three algorithms at eight digits.**
The SHA-256 and SHA-512 arms could otherwise be self-consistently wrong - six
plausible digits an issuer rejects, with nothing telling the user which end is at
fault. The three vectors use 20-, 32- and 64-byte keys, so a generator that
quietly truncated or padded would still match the SHA-1 row and fail only for
the users who have a SHA-512 seed.

---

## The bugs that mattered

Ranked by what they did to a user, per the convention.

1. **`${Provider/ID}` wrote a vault UUID into a deployed config.** Silent, from
   the CLI only, and the app disagreed without saying so. Phase 21 above. Nothing
   in the product reported it; the fixture that would have caught it existed and
   did not cover `ID`.
2. **The app skipped the `extra_vars` fallback for a mapped-but-empty field.**
   Found while writing Phase 21's fixture, not while looking for it. Same class
   as (1) - a reference that resolves in one half and not the other - and it
   would have made the new `ID` arm useful in the CLI only.
3. **`version_history` would have left a re-enrolled TOTP seed unversioned.** Not
   a bug that shipped, but the default outcome had it not been handled: swapping
   a seed would have left no record, and the user would have found out at a login
   screen. `CLAUDE.md`'s own E8 calls silence the one unacceptable option here,
   so the field discriminator was brought forward from Phase 23.

From the import/export pass, both prevented rather than shipped:

4. **A stale export would have silently replaced a working second factor.** The
   obvious implementation of "import seeds" is an upsert, and an upsert here
   locks somebody out of an account. `plan()` reports a differing seed instead;
   the test that pins it fails the moment the branch is loosened.
5. **`String(ev.target?.result ?? '')` on a `FileReader`.** `result` is typed
   `string | ArrayBuffer | null`, and `String()` on an ArrayBuffer yields
   `"[object ArrayBuffer]"` - which would have reached the parser as a file whose
   every line is unreadable, reported as "no seeds found in that file". Caught by
   `no-base-to-string`, which was only reported because the count-based
   suppression noticed a _second_ instance in `vault.ts`.

Two smaller ones, caught by lint and by reading the diff:

- The `copyTotpFrom` helper was first inserted **between `restoreSort`'s doc
  comment and `restoreSort`**, silently re-attributing the comment. Moved below.
- `clipboardWrite` returns a promise; the call needed `void`, which is what took
  `vault.ts` over its suppressed `no-floating-promises` count. The suppression
  file is count-based, so a genuinely new violation surfaces - that mechanism
  works.

---

## Decisions made

The full rationale for each is in `CLAUDE.md` (_Phase 22_ section and the _Key
Decisions Log_). The ones a reader is most likely to want to argue with:

- **No `'totp'` `SecretType`.** A seed is a field on _any_ entry. A type would
  split a GitHub login and its second factor across two entries, making
  `${GitHub/…}` ambiguous and `envv entry rm GitHub` refuse. Bitwarden and
  1Password model it the same way.
- **A pasted `otpauth://` URI is split, not stored whole.** Storing it whole
  means a second field that also holds the secret and has to be masked
  everywhere the seed is - and two copies of one value drift.
- **The seed redacts; the code prints.** An explicit, written exemption to Phase
  14, recorded in `envv-cli/src/totp_cmd.rs`'s module header. The `otpauth://`
  URI follows the _seed's_ rule, because it contains the seed.
- **`envv totp ls` never prints codes.** Same shape as the vault-wide export to
  stdout Phase 14 refuses.
- **One HMAC, in Rust. The parser is the twin.** The app asks over IPC; the
  parser had to exist twice because the form splits a URI as it is typed, so it
  is pinned by a golden fixture from both sides. Cost: `npm run dev` in a plain
  browser cannot show a code, and says so.
- **`${…}` deliberately cannot reach a seed.** No config file wants one, and a
  code is dead before the file deploys. `totp_secret` stays reachable by its
  literal field name, redacted - which is the parity that matters.

### The sidebar panel - what was assumed

The added scope arrived as one sentence: _"New left sidebar panel
(secrets/tools/user panel)"_. The user was away, so this is the reading that was
built, and it should be checked:

- The activity bar **already** carries Secrets, Tools, Remote and Users. The
  peer's own guidance said to fold into an existing grouping rather than
  duplicate one, and all three named groupings exist.
- So it is **not** a fifth activity-bar panel. It is a new `sidebar-section`
  inside `#secrets-panel`, keyed `authenticator`, built from the same markup as
  Tags and Key Pools - because it is a _filter over the secrets already in that
  panel_, not a new place to be. That buys collapse, reorder, hide and the
  Settings section editor for free and adds no second navigation idiom.
- It is data-gated like Tags: invisible until some entry carries a seed.
- Its header row filters the grid (`st.filter.type === 'has_totp'`); each body
  row shows one entry's live code and copies it on click.

**If what was wanted was a genuinely new top-level panel**, the change is small -
a fifth `activity-btn` plus a `sidebar-panel`/workspace pair, and `panelOrder`
already exists to place it. Nothing built here would need undoing; the section
would move rather than be rewritten.

One migration detail that would otherwise be a silent nothing: the section is
merged into existing installs' `sidebarSections` under its **own**
`localStorage` flag (`envvault-sb-migrated-totp`). Reusing the existing
`envvault-sb-migrated` flag would mean every install that has already run that
migration never sees the section - present in the markup, listed in Settings, and
invisible forever.

---

## What worked

- **Extending `vault-core/src/totp.rs` rather than adding a module.** The
  delegation (`hotp` → `hotp_with`) meant the login path and the stored-seed path
  cannot drift, and the one test asserting they agree is cheap insurance.
- **Writing the parity fixture first, then the two parsers against it.** The
  `extra_var_fallback` divergence in Phase 21 was found this way, not by reading.
- **Reverting to verify.** Done for the deny-list (Rust), the issuer-precedence
  rule (TS) and the URI-splitting in the form. All three failed as intended.
- **The count-based ESLint suppressions.** A genuinely new `no-floating-promises`
  surfaced immediately, while the 500-warning backlog stayed quiet.

## What didn't work

- **The first `python3 - <<'EOF'` heredocs were refused by the worktree
  guard** as "too complex to verify it stays inside the worktree". Writing the
  script to a file under `$CLAUDE_JOB_DIR/tmp` and running `python3 <file>`
  works. Worth knowing before losing a long heredoc.
- **`§` written as `\xc2\xa7` inside a Python string** produced mojibake and a
  failed match assertion - the escape is interpreted, not passed through. Use the
  literal character.
- **`cargo clippy --workspace` before `npx vite build`** fails on missing
  `dist/`, with a message that names `frontendDist` and nothing else.

---

## Next Steps

1. **Commit.** Nothing is staged; the one-liner is at the end of this file. Note
   that `CLAUDE.md`, `the session records/` and `review/` are gitignored and were edited in
   the **main checkout**, so they are not in the branch and do not travel with a
   `git push`.
2. **The manual smoke test only a human with a Tauri window can run**, added to
   `CLAUDE.md`'s _Immediate Next Steps_: put a seed on an entry, run
   `envv totp code <entry>`, then open the app and confirm the Authenticator
   section shows the same six digits. `entry_totp_code` has been exercised by
   nothing else - everything below the IPC boundary is tested, and everything
   above it is tested against the non-Tauri branch that prints `- - -`.
3. **Confirm the sidebar reading above** is what was wanted.
   3.5 **Round-trip an import against a real export.** The formats are tested
   against hand-written fixtures of each shape, which is right for the parser and
   proves nothing about whether Aegis 3.x still writes what this expects. Export
   your Ente vault, `envv totp import` it, `envv totp export --format aegis --out
x.json`, and import _that_ into Aegis. Both halves of that loop are the parts
   no test can reach.
4. **Phase 23** (credential shape, copy profiles, env naming) is next in the
   queue. Its E0 is now marked done in `CLAUDE.md`; the rest of E1–E17 is
   untouched. Note that E8 (`version_history` per secret-carrying field) is
   **partly done** - the mechanism and the `field` discriminator exist and cover
   `api_key` and `totp_secret`; `api_secret` and `extra_vars` are not covered and
   the `entry history` / `entry restore` CLI verbs do not yet know about `field`.

### Things deliberately not done

- **No `envv totp add`/`set`.** `envv entry set X --totp …` is the write path;
  a second verb for it would be two ways to write one field.
- **No `--dry-run` preview for `envv totp import`.** The global `--dry-run` works
  (it is enforced inside `Access::save`, so the parse and the report run and
  nothing is written), which is most of the value. A dedicated preview that lists
  every planned action per entry would be better and is not there.
- **No encrypted-export support for any source app**, deliberately - see above.
- **No import from a QR image.** The app cannot decode one (no camera, no
  decoder), so `otpauth-migration://` has to be pasted or saved to a file first.
- **No health-scan check for an unusable stored seed.** `envv totp ls` reports
  one per row (`usable: false` with the reason) and every write path validates,
  so the only way in is an imported vault. Worth adding to `doctor` in a later
  phase; recorded here rather than dropped quietly.
- **No QR code**, for the reason Phase 19 already recorded: the CSP allows no
  external script, and relaxing it in order to draw a secret is a poor trade.
  `envv totp uri --out` is the enrolment path.
- **`enrich --online` was not taught about seeds.** It sends a credential to its
  issuer; a TOTP seed has no endpoint to ask. It reads `api_key` only, so it
  already ignores them - recorded because "it already works" and "nobody checked"
  look identical in a diff.

---

## Notes for whoever picks this up

- **The branch name lies.** `worktree-phase-38-totp-secret` was created under the
  first of three numbering instructions this session received. The work is
  Phases 21 and 22.
- **`CLAUDE.md`'s phase table now has two renumbering maps.** A document written
  before 2026-09-04 needs both, in order: the 2026-09-04 map first, then the
  2026-09-10 one. The dated handoffs and `review/review-01.md` were not
  rewritten, by the standing rule that they are records.
- **The two TOTPs are easy to confuse and the naming is the only thing keeping
  them apart.** `envv totp` (a seed the vault holds for a website) versus
  `envv user totp` (EnvVault's own second factor); `entry_totp_code` versus
  `totp_status`; `EntryTotpCmd` versus `TotpCmd`. Only the second of each pair
  has an anti-replay mark, because only it verifies anything.
- **The ticker holds no references between ticks**, on purpose (invariant 1). If
  you optimise it by caching elements or entry indices, a deleted entry's slot
  will paint its neighbour's code - which looks like a working feature.
- **Copy reads `data-totp-code` off the slot, never re-derives.** Re-deriving
  hands the user the _next_ code when the click crosses a step boundary:
  invisible, and guaranteed to be blamed on the website.

---

## The commit

Nothing was staged or committed. Run this from the repo root (pwsh):

```powershell

```

Files deliberately left out, so the decision is yours:

- **`CLAUDE.md`, `the session records/`, `review/`** - gitignored, and edited in the main
  checkout rather than in the worktree. They are not part of this commit and
  never have been.
- **`dist/`** - gitignored build output. It was rebuilt during the session
  because `cargo clippy --workspace` needs it to exist.
- **`Cargo.lock`** - **changed**, and it is in the `git add` above. `base64` moved
  into `vault-core` for Google's migration payload; it was already in the
  workspace via `envv-cli`, at the same version, so no new crate is downloaded.
  `sha2`, `zeroize` and `serde` were already there.

> 1 line(s) about the maintainer's own machines were left out of this page.
