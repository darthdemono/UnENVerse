# Phase log 37: 2026-10-08 - Phase 24 and 24.1–24.5, everything that can be built

## Goal

"Finish Phase 24 fully, then all sub-phases of 24." Read the plan and the
previous handoffs, build every item that does not need a human or hardware, and
say plainly what is left.

## What landed

**24.1 bundles**

- `${bundle:Name}`, `${bundle:Name/slot}`, `${bundle:Name/slot/field}`,
  `${bundle:Name/local}` resolve in `chunk-ops.ts` and `refs.rs`, pinned by the new
  `parity/bundle-refs.json` (ambiguous names resolve to nothing; the field
  deny-list still applies).
- `envv bundle ls|new|add|remove|dissolve|delete`, the `bundle:Name[/slot]`
  selector for `get`/every lookup, and `bundle new --import config.py`.
- GUI: "Delete bundle and members" (counts entries, undoable), a
  "N entries belong together - bundle them?" banner with per-provider dismissal.
- **Python config importer on both sides** (`bundle-import.ts`,
  `vault-core/src/bundle_import.rs`, golden `parity/bundle-import.json`):
  f-strings become templates, ids above 2^53 stay strings, hex ints stay hex, a
  name assigned twice keeps the first and rewrites later references to `name_2`,
  everything else is source text plus a warning naming the line. Nothing executes.
- Python/JS/TS export keeps templates as f-strings / template literals in
  dependency order; `import → bundle → export → re-import` is a test that compares
  meaning.

**24.3 / 24.4** - routes tested over real HTTP through `build_router` (tower
`oneshot`): auth, 404/503 + `Retry-After`, revocation, 429. The uid limiter was a
fixed 60 s window "approximating" the design; it is now true token buckets per the
derived table, counted per value, overridable with `--uid-rate`.

**24.5**

- Session capture import in `vault-core/src/session_import.rs`: DevTools cURL
  (bash, cmd, PowerShell), HAR (one origin, others listed by name), `Set-Cookie`,
  Firefox `cookies.sqlite` (read from a copy). Chrome is refused by name.
  `envv cookie import`, and an "Import capture…" button on the web-session form.
- `type_emit.rs` + `envv emit` + the Copy menu: `.npmrc`, `pypirc`,
  `cargo-credentials`, Docker `config.json`, `netrc`, DSN, libpq, JDBC, Wi-Fi
  string. `emitters` now lives in `secret-types.json`; a test keeps Rust and the
  registry in agreement.
- Recovery codes (`envv codes`, card row, health finding at ≤2, parity fixture),
  BIP39 checksum against the bundled official wordlist (sha256 asserted),
  `envv oauth refresh` and the card's "Refresh access token…" (store-first, no
  redirects, https only), Wi-Fi QR drawn by a bundled encoder, licence and
  identity-document dates on the calendar (no holder, no number), local-service
  presets, `envv scan --json` findings carry `secretType` and `field`.

**D / C** - 131 ADRs seeded from the Key Decisions Log by script (ADR-0005…0135),
the log in `AGENTS.md` is now an index; book pages for the new CLI.

## Verification (2026-10-08)

- `npm run format:check`, `lint` (zero warnings), `typecheck` - pass.
- `vitest` - 58 files, 1,251 tests pass.
- `cargo test --workspace` - 420 pass, 0 fail; `clippy -D warnings`, `fmt --check` - pass.
- `npm run ui-lab` - **166 passed**; first time it has run to completion. The
  1,223 layout findings are the Phase 30 triage backlog (mostly
  `target-too-small`), not fixed here.
- `npm audit --omit=dev --audit-level=high` - 0. `cargo deny check` - all four ok.
- Not run: `npm run docs` and `mdbook build` (not installed here).

## The bugs that mattered

1. A pasted `Set-Cookie … Expires=((((…` **aborted the process** (stack overflow in
   `time` 0.3.37, RUSTSEC-2026-0009). Guarded; verified by removing the guard.
2. `rustls` 0.23.43 TLS 1.3 advisory, in the library the pin rides on: bumped.
3. The CI gates added in Phase 24 C had never been run: `js-yaml` high advisories
   and every `cargo deny` check were red. Fixed or ignored with reasons.
4. The UI lab's first run found a real row-height defect (long tag wrapped).
5. `bundleSuggestions` threw on a non-string `pool`, blanking the grid.

## Decisions

- Pinned crates (`rand =0.8.5`, `time =0.3.37`, `rsa` via `ssh-key`) were **not**
  bumped; their advisories are in `deny.toml` with the reason and the condition
  that ends them. Bumping `rand` to `=0.8.6` and `time` to `>=0.3.47` is yours.
- The Discord fixture is **synthetic and named so**. The acceptance test in the
  plan is the user's own config; do not describe this as that.
- Jellyfin preset omitted: its `Authorization: MediaBrowser Token="…"` needs a
  value template `auth_scheme` cannot express.

## Still open, and why

| Item                                                                                                  | Needs                                                                                                                                                                                                                                                                         |
| ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A4 Caps Lock OS query, A9 emoji, A10 drag in Settings                                                 | a real GTK/WebKit window on X11 and Wayland. A10's two hypotheses are already handled in code (`dragDropEnabled: false`, `setData`); A9's IM variables are left alone by `configure_linux_webkit`. An in-app emoji picker is the plan's conditional fallback and is not built |
| GitHub: Dependabot alerts, code scanning default setup, private vulnerability reporting, Pages source | repository settings                                                                                                                                                                                                                                                           |
| 24.1 acceptance                                                                                       | the user's sanitized or revoked Discord config                                                                                                                                                                                                                                |
| 24.4 p99 measurement                                                                                  | a 2 vCPU host, `POST /api/auth` and `GET /api/vault` under registry load                                                                                                                                                                                                      |
| `mdbook build`, `npm run docs`                                                                        | `mdbook` and Doxygen installed                                                                                                                                                                                                                                                |
| 24.5: JWKS export, GPG expiry parsing, passkey workflows                                              | OpenPGP / key-format parsers not in the tree; not started                                                                                                                                                                                                                     |
| Phase 30 triage of 1,223 layout findings                                                              | its own phase                                                                                                                                                                                                                                                                 |

## Notes for whoever picks this up

- Unset `ENVV_SERVER_URL` before smoke-testing the CLI against a scratch vault.
- `AGENTS.md` is gitignored; a backup of its pre-edit state was kept in the
  session scratchpad only. The decisions table is now an index into `book/src/adr/`.
- Nothing is committed. `book/`, `deny.toml`, `vault-core/data/` (the BIP39 list,
  MIT, from the bitcoin/bips repo) and the new `.rs` files are untracked.

> **Renumbered the same day:** storage is now **Phase 30** (route tests + `permex` became 25 and run first). See `AGENTS.md` → _Renumbering map - 2026-10-08_.

## Phase 25 (storage) - sized 2026-10-08, from the code

**Scope.** Row-per-entry, schema v2 (review-01 §2.1/§2.2). Today the vault is one
JSON string in `vault(id=1, data)`; `vault_meta` holds one key (`data_hash`).

**Size: large. About 3–4 focused sessions for the storage change itself; a fifth
if per-entry sync and merge are in scope.** It is one real project, not a pile.

| Piece                                                                                                                                                                  | Where                                                                                             | Weight                                               |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Schema v2 tables, `schema_version` in `vault_meta`, refuse-on-future-version, one-time blob→rows migration in a transaction with a pre-migration copy of `vault.db`    | `vault-core/src/lib.rs` (~350 lines: `load_vault` 359, `save_vault` 469, CAS 505, history, audit) | the core; ~1 session                                 |
| `version_history` (50 revisions) moves to its own table; hash chain and `actor` rows untouched                                                                         | same                                                                                              | medium                                               |
| Keep a whole-`Value` facade so callers do not change: `Access::load_vault/save` is used by 28 files (CLI, server, Tauri, `users.rs` merge/filter, `ics_feeds`, `pool`) | `access.rs` (526), `users.rs`                                                                     | this is the cheap path and the recommended first cut |
| Compare-and-swap per entry instead of whole-blob `data_hash` (the actual payoff: two people editing different entries stop conflicting)                                | `save_vault`, `envv-server` (4,028 lines, `PUT /api/vault`), `state.ts` conflict dialog           | the expensive half; it changes the wire protocol     |
| Old clients: a v1 CLI or app doing a whole-vault `PUT` to a v2 server must still work, or be refused with a message                                                    | `envv-server`                                                                                     | design decision, then ~1 session                     |
| `.vaultbak`, `backup archive`, importers, `vault_version` poll (hashes the blob today), LAN server                                                                     | `backup.rs`, `vault-watch.ts`, `src-tauri`                                                        | small each, many of them                             |
| Tests: every test that asserts `data_hash`; a migration fixture of a real v1 vault; a downgrade-refusal test; a crash-mid-migration test                               | 421 Rust + 1,251 TS                                                                               | ~25% of the work                                     |

**Why it is safe to start now.** Every data-model change it was waiting on has
landed (23, 24.1, 24.5). It must not be started with new `VaultEntry` fields still
planned: each one is a column or a JSON key decision made once.

**Recommended order.** (1) `schema_version` + refuse-future, alone, ships first and
protects every later step. (2) rows + facade, blob kept readable for one release.
(3) per-entry CAS and the conflict dialog. (4) drop the blob.

**Risks that decide the estimate.** Migration is irreversible on a user's only
copy of their secrets, so the pre-migration copy and a restore test are not
optional. The `data_hash` poll in `vault-watch.ts` and the CLI/app "who wrote
last" logic both assume one hash for the whole vault.
