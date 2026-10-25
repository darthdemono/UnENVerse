# Phase log 22: 2026-08-27 - Phase 20 observability, secret creation dates, an ICS calendar feed, and `envv use`

Continues from `21-2026-08-25-phases-17-and-18-implemented-ci-unblocked-twi.md`, which closed Phases 17 and 18. **This session
was paused partway through Phase 19 at the user's request (token budget), so
this file has an unusually large Next Steps section - three Phase 19 items are
specified but unwritten.**

## Goal

The ask was three things at once: implement the staged Phases 19 and 20, add a
panel showing when each secret was created, and add an iCalendar feed covering
every creation and expiry date in the vault.

The audit at the start found that **none** of Phase 19 or 20 existed in the
tree - no `tracing`, no `/api/health`, no `Retry-After`, no `totp` anywhere, no
`.envv.json` - and, more importantly, that **`VaultEntry` had no creation date
at all**. The requested "creation date panel" therefore needed a new field plus
a migration story before it could show anything, and that shaped the whole
session.

Two decisions were put to the user before starting. Both are recorded under
_Decisions made_.

## Current Progress

**Version 0.8.1** (unchanged - no bump this session; the release pipeline is
triggered by a version bump, and this work is not released yet).

**Verification at the pause point:** 856 frontend tests across 37 files, all
Rust tests passing (`envv-cli` 19 lib + 8 parity + 6 + 11 + 1, `envv-server` 21),
`tsc --noEmit` clean, `cargo check --workspace` clean.

### Phase 20 - Operability (3 of 4 items complete)

| Item                             | What landed                                                                                                                                                                                                |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 20.1 `tracing`                   | `vault-core/src/telemetry.rs`, behind a new `telemetry` feature, enabled by all three binaries. `ENVV_LOG` (then `RUST_LOG`) sets the filter; `ENVV_LOG_FORMAT=json` switches to one JSON object per line. |
| 20.2 `/api/health`               | Liveness and lock state with **no counts**, 503 when the server is bound but cannot read its own vault.                                                                                                    |
| 20.3 `Retry-After` + request ids | 429 carries `Retry-After`; every response carries `x-request-id`; every error body carries `request_id`.                                                                                                   |
| 20.4 `envv` name collision       | Closed as a decision, no code. See _Decisions made_.                                                                                                                                                       |

**Logging rules, written into the module and worth keeping:**

1. **stderr, always.** The CLI's stdout is a machine-readable contract. A log
   line written there corrupts the JSON envelope an agent is parsing, so
   `ENVV_LOG=debug envv get X --json | jq` has to keep working - and it does,
   because `with_writer(std::io::stderr)` is not optional.
2. **Never log a value.** Fingerprints, entry ids and provider names only. A log
   file is not encrypted and is routinely shipped somewhere else.
3. **Quiet by default.** `envv-server` and the desktop app default to `info`;
   the CLI defaults to `error`.

Defaults differ on purpose: a long-lived server's log is read after the fact,
and a CLI's stderr is read _now_, by a human who ran one command.

**`/api/health` is deliberately not `/api/stats`.** Usage data about a secrets
manager is itself sensitive, and this endpoint is the one an unauthenticated
monitor polls every few seconds. It reports `status`, `version`, `unlocked`,
`vault_exists`, `uptime_secs` and nothing else. A test asserts the absence of
`secrets_stored`, `users_total` and `users_connected`, so a later "just add a
count" cannot pass silently.

It also had to fail when the process is **wedged** rather than merely bound.
The old Docker healthcheck opened the TCP port, which a hung process answers
perfectly well - `restart: unless-stopped` sat on top of a check that could not
go red. When an owner session claims the vault is unlocked, `/api/health` now
opens the database and runs a query, and answers 503 when it cannot.

Verified live against a running debug server: unlocked → 200 `ok`; the database
file overwritten with garbage under the running process → **503 `degraded`**,
with the ERROR line in the log carrying the same request id as the response.

**Rate limiting now says when to retry.** `rate_is_allowed` returned a bool, so
a client could not tell "slow down" from "denied" - the agent-driven CLI is the
main caller and had to guess. It is now `rate_retry_after` returning
`Option<u64>` seconds, floored at 1 (a client told to retry in 0 seconds retries
instantly, is refused again, and reads the server as broken). Verified live:
twelve parallel bad-token requests, then `HTTP/1.1 429` with `retry-after: 43`.

**Request ids use a `tokio::task_local!`, not an axum `Extension`.** `err_json`
is a free function called from forty places, several frames deep inside
`forbidden` and `guard_manage_user`, none of which can reach an extractor.
Scoping the id to the request future gives all of them the same answer without
threading a parameter through every call site. Outside a request - the desktop
app calling a handler directly, unit tests - `current_request_id()` is `None`
and the field is omitted rather than emitted as null, because a correlation
handle that leads nowhere is worse than no handle.

### Secret creation dates (new feature)

`VaultEntry.created_at`, optional, ISO-8601, **never rewritten on edit**. Set by
the desktop form, by `envv entry add`, and by every import path (`.env` import
in both halves, and the vendor importers). "When it entered _this_ vault" - an
imported Bitwarden item does not inherit Bitwarden's creation date, because that
is the date another vault first held it.

The migration question is the interesting part. Options were: stamp "now" on
every undated entry, infer, or leave blank. **The user chose infer-else-unknown,
and the implementation is stricter than that sounds:**

- `backfillCreatedAt()` writes a date **only** from an `add` row in the audit
  log, which is exact evidence. Oldest matching row wins.
- **An ambiguous provider is skipped entirely.** Audit rows identify an entry by
  `entry_provider` alone, so two entries sharing a provider cannot be told
  apart, and attributing one entry's creation to its namesake is the array-index
  bug class wearing a different hat. The CLI refuses ambiguous lookups for
  exactly this reason; so does this.
- **`version_history` is not a source.** Its oldest `saved_at` is when a value
  was _replaced_ - an upper bound, not a creation date. The panel shows it as
  "before 4 Mar" and never writes it, because a stored date is
  indistinguishable from a known one the moment anything else reads it back.

### Secret Timeline panel (new feature)

Tools → Secret Timeline. Columns: Secret, Created, Age, Expires, Rotation due.
Sortable four ways, with undated entries pinned last **in both directions** -
`a ?? Infinity` would sink them one way and float them the other, putting the
least informative rows at the top of the ascending view.

The Created column carries a certainty: `known`, `bound` ("before …"), or
`unknown`, and `.tl-muted` is the visual form of "we do not actually know this".
A bound rendered at full contrast reads as a creation date, which is the one
thing this pane must not imply.

### iCalendar feed (new feature)

`src/ts/calendar.ts` and `envv-cli/src/calendar.rs` - **the fourth twin pair**,
and it follows the rule the other three established: one golden file
(`tests/fixtures/parity/calendar.ics`), asserted from both sides. It was
byte-identical on the first run.

Three event kinds per entry, all all-day: `Created`, `Expires` (7-day alarm),
`Rotate` (3-day alarm). Rotation counts from `last_rotated_at`, falling back to
`created_at` - a 90-day key that has never been rotated is due 90 days after it
was issued, not never - and an entry with neither anchor gets no event, because
putting a deadline in someone's calendar that no evidence supports is worse than
leaving it out.

UIDs are `<entry-id>-<kind>@envvault`, stable across exports. That is what makes
a re-import an _update_ rather than a duplicate.

**What is deliberately not in the file: values, and fingerprints.** An `.ics` is
the least private artefact this program produces - its whole purpose is to be
handed to Google Calendar or a phone, which stores it unencrypted on hardware
nobody here controls. A fingerprint is stable per value, so a feed carrying them
would be an equality oracle over the vault published to a third party. Both
absences are asserted, in Rust and in TypeScript, against the real fixture
vault rather than a toy entry.

Surfaces: `envv calendar [--kind created|expires|rotation] [--project P] [-o FILE]`
and an "Export .ics" button in the panel, which confirms first and says plainly
that **names travel** even though values do not.

`envv calendar` may write to stdout under the default redacting policy, which is
an exception to the Phase 14 rule and a deliberate one: `export` is refused
because a masked `.env` still _looks_ deployable, while an `.ics` has no values
to mask - it carries names and dates, which `envv list` already prints.

**Validated against a real parser, not just against itself.** Python's
`icalendar` 7.3.0 parses both the golden fixture and live CLI output; every
event's `DTEND - DTSTART` is exactly one day (an all-day `DTEND` is exclusive,
and a zero-length event is dropped by several clients); every raw line is ≤ 75
octets; there are no bare LFs. The control case - the same file with
`END:VCALENDAR` removed - is rejected, so the check proves something about the
good file.

### Phase 19 - UX (1 of 4 items complete)

**`envv use` (19.4).** Writes `.envv.json` in the working directory; commands
run there inherit the project and environment. Upward search from the working
directory, so `envv get` from `src/deep/nested/` finds the context at the top of
a checkout. Precedence: explicit flag > `ENVV_PROJECT` / `ENVV_ENV` > file.
`--show`, `--clear`, and each half settable independently (`--env staging` in a
directory already pinned to a project must not silently unpin it).

The file names things and never holds a value, so it is written 0644 and is safe
to commit. A context file that held credentials would need every consumer to
remember to gitignore it, which is a rule that gets forgotten exactly once.

**A stale pin is an error, not a shrug.** A context naming a project the vault no
longer has exits 3 and says how to fix it. The desktop app's answer to invariant
7 is to drop the reference and repaint; a CLI cannot do that, because "matched
nothing" and "no filter" produce visibly different output for `list` and
_identical_ output for a vault with one project. Silently widening a scope the
user believes is narrow is the worse failure.

`--show` and `--clear` are dispatched **before** `open_access`, for the same
reason `doctor` and `restore-archive` are: the state they are most needed in is
the state where opening a vault fails.

Applied to every command that already had `--project`: `list`, `entry ls`,
`export`, `calendar`, `exec`, `watch`, `import`, `import-vault`.

**19.5 (Settings-panel overflow) was already done** before this session, per
phase log 21.

## The bugs that mattered

Ranked by what they did, not by which line was wrong. Three of these were mine,
caught before they shipped.

1. **`'\;'` in a JavaScript string literal is `;`.** The ICS escaper's semicolon
   branch was a no-op, so `icsEscape` would have emitted a raw `;` - a parameter
   separator in RFC 5545, which corrupts the property it appears in. Caught by
   the parity fixture disagreeing with Rust, then _again_ by the unit test,
   whose expectation contained the identical mistake and had to be fixed before
   it could fail correctly. Rust has the same trap and rejects it at compile
   time (`unknown character escape: ;`), which is how the Rust half was caught.
2. **`/api/health` reported "Wrong master password" for a corrupt database.**
   SQLCipher cannot tell the two apart - the header simply fails to decrypt -
   so passing the underlying error through sent an operator hunting a password
   problem while the file underneath the running server had been replaced. The
   detail string is now written here, not forwarded, and a test asserts it never
   contains the word "password".
3. **The first attempt at `/api/health` stole `stats`'s `#[utoipa::path]`.** The
   new function was inserted between the existing attribute and the function it
   decorated, so the attribute landed on `health` and the new one on `stats` -
   `conflicting implementation for __path_health`, seven compile errors deep.
   Inserting _before_ a function is only safe when the function has no
   attributes; check for them.
4. **The parity fixture's project filter existed twice.** `envv calendar` needed
   the same `--project` matching `envfile::export_vault` had inline. Extracted
   to `data::entries_in_project` rather than copied, because two definitions of
   what `--project web` means agree right up until one of them learns about
   sub-projects.
5. **`serde_json` map key order is not insertion order** without the
   `preserve_order` feature. A context-file test asserted the field list in
   declaration order and failed on a correctly written file. Compare as a set.

## Decisions made

- **The `envv` binary-name collision is closed with no rename.** Put to the user
  as a Phase 20 item; the answer was that envvault.dev does not work and a
  project rename may happen anyway. No alias binary, no deprecation shim.
- **Undated entries stay undated unless there is exact evidence.** Stamping
  "now" would make every pre-existing secret claim it was created today, and
  that lie would then be repeated by the timeline panel and by every exported
  `.ics`. Chosen by the user from three options.
- **An inferred upper bound is displayed, never stored.** Once written to
  `created_at`, a bound is indistinguishable from a known date to the CLI, to an
  export, and to the calendar feed.
- **Ambiguity refuses rather than guesses**, in the backfill as everywhere else
  in this codebase.
- **`.ics` carries no fingerprints.** They are stable per value, and the file is
  handed to a third party by design.
- **Logs go to stderr in every binary, and the rule lives in one place.** Three
  copies of a subscriber setup means three chances for one of them to write to
  stdout and silently break `--json` for its binary only.
- **`/api/health` stays free of counts** so it can remain unauthenticated. That
  is the same constraint `/api/stats` lives under, stated as a test.
- **The `.envv.json` context is validated on use, not on every read.** Writing it
  checks the project exists; reading it checks again only when the value came
  from the file or the environment rather than from a flag, so a flag's error is
  not reported twice in different words.

## What Worked

- **Auditing before implementing, again.** Every Phase 19/20 item was assumed
  present by the plan and none of them was; `VaultEntry` having no creation date
  at all changed the shape of the requested feature. Ten minutes of `grep`.
- **Asking two questions and no more.** The binary-name collision and the
  backfill policy genuinely could not be inferred. Everything else was decided
  from the code.
- **Running the real thing.** The server was started, corrupted underneath
  itself, and hammered with parallel bad credentials; the calendar was fed to a
  real iCalendar parser with a control case. Both found things review would not
  have.
- **The twin-pair convention paid for itself immediately.** The golden fixture
  caught the semicolon bug across implementations on its first run, exactly as
  it did for the exporters in Phase 13.
- **Reverting each guard to check the test.** The backfill's ambiguity guard and
  the timeline's `esc()` were each removed independently, and each produced
  exactly one failure.

## What Didn't Work

- **Assuming the shell's working directory persists.** It was reset between tool
  calls mid-session, so a `printf > hv/vault.db` wrote nothing and a "corrupt
  the database" check silently tested the healthy path. Use absolute paths in
  every command that writes.
- **Testing the rate limiter serially against a debug build.** Argon2id at
  m=65536 is slow enough unoptimised that eleven sequential failed unlocks took
  longer than the 60-second window, so the limiter reset instead of tripping.
  Twelve parallel requests to `/api/auth` reproduced it immediately.
- **Writing an escape sequence twice and getting it wrong both times.** The test
  expectation was copied from the implementation, so it agreed with the bug.
  A golden file shared with a second implementation is what broke the tie.

## Next Steps

**Three Phase 19 items remain, all unwritten.** Work stopped at the point of
reading `vault-core/src/users.rs` for the TOTP schema.

1. **TOTP for sub-users (19.2).** Greenfield - there is no `totp` in the tree;
   the Phase 5.1 implementation was removed in Phase 7 because nothing reached
   it. The design settled on before the pause:
   - `vault-core/src/totp.rs`: RFC 6238, HMAC-SHA1, 6 digits, 30-second step.
     Needs a `sha1` dependency (`hmac` is already there). Base32 for the
     `otpauth://` secret, hand-rolled the way HKDF is in `entropy.rs` rather
     than pulling a crate for forty lines.
   - Schema: `totp_secret` and `totp_last_step` columns on `users`, added by
     idempotent `ALTER TABLE` in `init_users_schema` alongside `class_id` and
     `strict_write`.
   - **Anti-replay is not optional.** Reject any code whose counter step is ≤
     the last accepted step; a ±1-step skew window without it means a code is
     reusable for ninety seconds.
   - Enrollment is two-phase: `enroll` mints a secret and returns the URI,
     `confirm` requires a correct code before the factor is _enabled_. Enabling
     on enrollment locks out anyone whose authenticator did not actually take.
   - **Sub-users only.** The owner authenticates by deriving the SQLCipher key;
     there is no hash to gate and nothing for a second factor to protect.
   - Server: `/api/auth` gains `totp`, required only when the user has the
     factor enabled **and** authenticated by password. Token auth must skip it,
     or every CI integration breaks - that regression is already in the bug
     history from Phase 5.
   - CLI: `envv user totp enable|confirm|disable|status`, plus a global
     `--totp` for authenticating to a server.
   - UI: the Users panel. A QR code needs a library the CSP will not load, so
     show the base32 secret in groups of four and the `otpauth://` URI; every
     authenticator accepts manual entry.
2. **Onboarding wizard (19.1).** Nothing exists. Note that the unlock and
   first-run screens are shown, hidden and shown again - invariant 9 applies,
   so every handler must be _assigned_, never added.
3. **Accessibility pass (19.3).** The new Timeline pane was written with a
   `<table>`, `<th scope="col">` and real `<label for>` bindings, so it is a
   reasonable starting point rather than another item on the list. The rest of
   the app has not been looked at.

**Then:**

- Fold this session's permanent material into `CLAUDE.md`: the twin-pair table
  gains a fourth row, `created_at` belongs in the Data Model, the phase table
  needs 19 and 20 updated, and three new pitfalls (the `'\;'` literal, inserting
  a function between an attribute and its target, `serde_json` key order).
- Bump the version to cut a release. `npm run version <x.y.z>`, then push - the
  tag check in `build.yml` is what triggers it.

**Only a human can decide:**

1. Whether 237 lint suppressions ship in a 1.0. Unchanged from phase log 21.
2. Whether lock-on-switch stays the default.
3. Whether the project itself gets renamed, which was raised as the reason not
   to bother resolving the `envv` collision now.

## Notes for whoever picks this up

- **Read `npm run version` in the same command that writes it anywhere.** It was
  0.8.1 all session, and phase log 21 records shipping a README that said 0.7.1
  because the value moved mid-session.
- **The tree is uncommitted at the pause point**, and `README.md` and
  `.gitignore` were already modified before this session started - they are not
  part of this work. A commit one-liner covering only this session's files was
  handed to the user.
- `envv doctor` remains the fastest way to find out what a vault is missing.
- The new `.ics` validation script is not in the repo; it was a scratch Python
  file using `icalendar`. If the format changes, re-run that check rather than
  trusting the golden file alone - the golden proves the two implementations
  agree, not that either is legal RFC 5545.
- The next handoff is `23-2026-08-31-review-01-defect-fixes-the-key-pools-sidebar.md`.
