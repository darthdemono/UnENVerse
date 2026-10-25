# Phase log 24: Phase 22 designed: credential shape, copy profiles, env naming

> **Edited 2026-09-04 - phase numbers renumbered.** Phases 21–36 as originally
> written in this file were ids allocated in design order; they are now queue
> positions, so every phase reference here has been rewritten to the new numbers
> and the plan table re-sorted. Nothing else was changed: findings, verification
> output, decisions and dates are as written. The old→new map is in `CLAUDE.md`
> under _Phase Status → Renumbering map_. **A record that is edited and says so
> beats one that is quietly wrong** - that was the call, and this line is how it
> stays honest.
>
> In particular: the phase this file designs was "Phase 36" and is now
> **Phase 22**; the defect it splits out was "36.0" and is now **Phase 21**.

**Date:** 2026-08-31
**Version at start and end:** 0.8.1 (`npm run version`)
**Code changed:** none. This session produced a design and one live-defect
finding, both in `CLAUDE.md`. No source file was touched, no test was run beyond
`npx prettier --check CLAUDE.md`.

---

## Goal

The user reported that copy produces the wrong text for real credentials, and
asked for a phase entry covering five things: an `id` vs `key` distinction that
survives into the copied output, extra variable info travelling with a copied
var, cookies stored with their User-Agent, `env_var` entries without a mandatory
primary field, and a configurable copy profile (basic / extended / all). Plus
version and label appearing in the generated name:
`spotify → v2 → game` giving `SPOTIFY_V2_GAME_ID=`.

Then, in two follow-ups: find more edge cases in keys/secrets/cookies "like
IRL", and write the fix and the usage for every one of them.

The goal moved once. It started as "add a phase row" and became "design the
phase, catalogue what breaks, and give every item a landing site", because the
first pass at the design kept running into things the existing code already gets
wrong.

---

## Current Progress

All of it is in `CLAUDE.md`, in a new `## Phase 22` section:

- **The naming template** - `[PREFIX_] PROVIDER [_VERSION] [_LABEL] [_ROLE]`,
  with normalisation rules, and `envName()` as the single builder that
  `dotenvKey` and `import-export.ts`'s `envKey` both collapse into.
- **New fields** - `primary_role`, `secret_role`, `label`, `user_agent`,
  `env_name_override`, plus `public` and `role` on `extra_vars`.
- **Copy profiles** - basic / extended / full, metadata as comments by default.
- **`env_var` without a mandatory primary**, with the audit list of six places
  that assume the primary value is never empty.
- **Cookies** - a new `SecretType`, five copy targets, a paste-parser.
- **The IRL shapes table** - ten credential shapes taken from real issuers, and
  what each one breaks today.
- **E0–E17** - eighteen edge cases, each with mechanism (`file:line`), fix, and
  what usage looks like afterwards.
- **An ordered work plan**, 0–7, with every E number mapped to a step.

Verification performed: `npx prettier --check CLAUDE.md` passes. Nothing else -
there was no code to test.

---

## The bugs that mattered

### 1. `${Provider/ID}` resolves to the entry's UUID in the CLI (live, unfixed)

`canonical_field()` (`envv-cli/src/refs.rs:22`) has no `ID` arm, so `ID` falls
through `_ => field` and becomes `"id"`. `entry_field()` then reads
`entry.get("id")` - the entry's stable UUID - finds it non-empty, and returns
it. The TypeScript twin (`src/ts/chunk-ops.ts`) has no `id` case in its switch,
so it falls to the `extra_vars` lookup and returns `undefined`.

**What it does to the user:** the same reference behaves differently in the two
halves of the product. `envv render` writes a UUID into a config file that is
then deployed; the desktop app leaves the literal text `${Spotify/ID}`. Neither
is an error anyone sees. A service starts, authenticates with a 36-character
random string, and fails with whatever the provider says about a bad client id.

`tests/fixtures/parity/field-aliases.json` - the fixture that exists precisely
because `PASSWORD` had already drifted between these two tables - never covered
`ID`.

Recorded as **E0** and as phase row **21**. Not fixed; the user chose to keep
this session documentation-only.

### 2. Nothing quotes a value on the way into a `.env`

`Exporter.dotenv` emits `${b}=${k.api_key}` raw. Today that is survivable
because most stored values are opaque tokens. The moment cookies and
User-Agents land - which is what this phase adds - it stops being survivable:
`Mozilla/5.0 (X11; Linux x86_64)` and `sid=x; csrf=y` both produce a file that
parses back as something else. The round-trip test exercises the parser only.

Recorded as **E1**.

### 3. A generated-name collision is a silent overwrite

`Exporter.yaml` writes `doc[b] = …`. Two normal setups hit it: a key pool is by
definition several entries for one provider, so "Copy All" emits N identical
names and the last wins; and one entry with `primary_role: 'id'` plus an
`extra_vars` entry keyed `ID` collides with itself. Recorded as **E2**, with the
rule that collisions are compared case-insensitively because Windows
environment variables are.

---

## Decisions made

| Decision                                                                   | Rationale                                                                                                                                                                                                                                                         |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`key_id` is not repurposed as "the client id value"**                    | It is identity: `vault-core/src/lib.rs:236` folds it into the legacy `entry_ck` for entries with no `id`, and the CLI disambiguates `provider:key_id`. Repurposing it silently re-targets RBAC scoping and `entry rm` on every pre-`id` vault.                    |
| **`label` is a new field, not `account_name`**                             | Account names in real vaults are email addresses. `SPOTIFY_DARTHDEMONO_GMAIL_COM_ID` is not a variable name anyone wants.                                                                                                                                         |
| **Absent `primary_role` keeps the bare `PROVIDER=` name**                  | Defaulting it to `key` renames the variable in every `.env` already deployed, and the user finds out when a service comes back up without its credentials.                                                                                                        |
| **`public` is a per-_value_ opt-out, never per type**                      | A client id, a region and an account SID are safe to print; the secret beside them is not. Without this the `basic` copy profile is either all-masked (useless) or all-clear (unsafe).                                                                            |
| **Metadata copies as comments, not variables**                             | An `.env` is loaded into a process. Injecting six non-functional variables per credential into every container is a cost nobody asked for by pressing Copy. `metadataStyle: 'var'` exists for those who want it.                                                  |
| **Cookie entries mask whole, per-var `secret` flags ignored**              | Same reasoning as `env_file` chunks: the flag defaults to unset, so the first unflagged cookie is the one that leaks.                                                                                                                                             |
| **`label` is _not_ added to the legacy `entry_ck` tuple**                  | That changes every existing ck value. Backfill `id` instead - the app already does it in `finishInit()`; the gap is entries written by an older CLI.                                                                                                              |
| **Cookies are excluded from the rotation scan**                            | Rotation for a session credential is "log in again in a browser", which the app cannot do. A nag with no available fix trains people to ignore the whole health scan.                                                                                             |
| **`cookies.txt` export is refused when per-cookie attributes are unknown** | Writing a file `yt-dlp` silently ignores is worse than not offering the export.                                                                                                                                                                                   |
| **No new fixed columns for AWS/Twilio/Apple shapes**                       | They have no primary value at all. The role-tagged `extra_vars` shape built for `env_var` is the general case; two fixed slots stay only as the legacy shape. Adding an "id" column is how the schema ends up with four columns and a sixth shape it cannot hold. |
| **`auth_scheme` + `auth_param` are added**                                 | _How_ to send a credential (`X-Api-Key`, `Bearer`, basic, query, cookie) is part of the credential, is nowhere in the model, and is what the curl and cookie exports need anyway.                                                                                 |
| **The phase is numbered 36, scheduled first**                              | 19–35 were already allocated in `23-2026-08-31-review-01-defect-fixes-the-key-pools-sidebar.md`. Renumbering them would mean editing a handoff, which the contract forbids. The number is an id; the table already says numbers are not a queue position.         |

---

## What worked

- **Reading the code before writing the design.** Every one of E0–E17 came from
  source, not from reasoning about the feature. The `${Provider/ID}` defect in
  particular is invisible from the docs and obvious from `refs.rs:22`.
- **Surveying real issuers rather than generalising from one.** The
  key-plus-secret pair turns out to be the _rarest_ shape once you list what
  Spotify, AWS, Twilio, Apple, Azure, GCP, Stripe and Cloudflare actually hand
  out. That is what killed the "add an id column" design.
- **Giving every edge case a numbered step.** The gap list in `23-2026-08-31-review-01-defect-fixes-the-key-pools-sidebar.md`
  reached seventeen entries without anyone deciding on one of them; E0–E17 each
  name the step they land in, and the plan says a dropped one gets written down.

## What didn't work

- **Trusting `CLAUDE.md`'s phase table.** It stopped at Phase 20 while
  `23-2026-08-31-review-01-defect-fixes-the-key-pools-sidebar.md` had a verified plan through 35, so the new phase was drafted
  as "21" - a number already in use. Caught only when the user asked whether the
  work had been saved anywhere. The table is now synced, with a note saying to
  check the newest handoff for the highest number in use.
- **Assuming `CLAUDE.md` is tracked by git.** It is in `.gitignore:47`. Three
  commit one-liners were offered that would have staged nothing. The harness
  labels the file "checked into the codebase"; git disagrees, and git is right.

---

## Next Steps

1. **Fix E0.** Deny-list the identity fields (`id`, `version_history`,
   `projectIds`, `categories`) in `entry_field` on both sides, then add the
   `ID | CLIENT_ID | APP_ID | ACCOUNT_ID | APPLICATION_ID` alias arm, then add
   those plus a negative case for `id` to
   `tests/fixtures/parity/field-aliases.json`. Verify by reverting the deny-list
   and watching the Rust side hand back a UUID. This is phase 21 and ships
   alone.
2. Then steps 1–7 of the work order in `CLAUDE.md`'s Phase 22 section. Do not
   start at cookies or `env_var`; both sit on the name builder.
3. **Human-only:** decide whether Phase 22 really does come before Phase 24
   (close review-01). The argument for 22 first is that it changes the data
   model and every day of delay is more vaults written under the old shape. The
   argument against is that review-01's §3 defects are known-live.

---

## Notes for whoever picks this up

- `CLAUDE.md` and `the session records/` are both gitignored. Nothing written this session
  is in git, and `git status` being clean does not mean the work is committed -
  it means the work is invisible to git.
- **E0 is a real defect in shipped code**, not a design note. If you do nothing
  else from this handoff, do that one; it is a one-line change per side.
- The Phase 22 section is long. The order that matters is: the naming template
  first (everything else sits on it), then roles and label, then profiles, then
  the visible features. The section says this twice because the temptation to
  start with cookies is strong and it means writing the template logic twice.
- The IRL shapes table is the part most worth re-reading before adding any field
  to `VaultEntry`. It is the argument against the next fixed column.
