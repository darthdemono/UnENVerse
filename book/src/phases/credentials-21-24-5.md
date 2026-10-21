# Phases 21 to 24.5: references, authenticators, credential shape and the grid

Reference correctness, stored authenticators, credential shape and env naming, the shipped-feature defect pass, composite secrets and bundles, the legible grid, calendar feeds, the unique-ID registry and the richer credential model.

### Phase 21: `${Provider/ID}` resolved to the entry UUID

**Status:** Done. Earlier numbers: 36.0.

`canonical_field` had no `ID` arm, so the CLI wrote a vault UUID into a rendered config while the app left the reference literal. Fixed with a metadata deny-list plus an `ID`/`CLIENT_ID`/`APP_ID`/`ACCOUNT_ID`/`APPLICATION_ID` arm on both sides, and negative rows in `parity/field-aliases.json`.

### Phase 22: TOTP as a stored secret

**Status:** Done.

The vault holds a base32 seed (or an `otpauth://` URI) for a _third-party_ service and emits a live code, the way Bitwarden and 1Password do. Distinct from Phase 19's TOTP, which is UnENVerse's own sub-user second factor; they share `vault-core/src/totp.rs` and nothing else. Design below. _(new on 2026-09-10)_

### Phase 22.2: HOTP, Steam, the next code and the Authenticator screen

**Status:** Done.

Counter-based and Steam Guard seeds, a next-code view, and a fifth activity-bar panel. Reverses Phase 22's "not a fifth panel" decision, on request and with the reasoning written down. _(new on 2026-09-11)_

### Phase 23: Credential shape, copy profiles, env naming

**Status:** Done. Earlier numbers: 36 → 22.

Value roles (`id` vs `key`), `label`, version-aware env names, cookie + User-Agent secrets, `env_var` entries with no mandatory primary, three copy profiles. All 8 steps and every edge case E0–E17 landed (2026-09-13/14 + ).

### Phase 24: Shipped-feature defects, lint to zero, CI hygiene

**Status:** In progress.

2FA codes blank on every remote vault; every app export (ICS included) a blob download WebKitGTK ignores while the toast says "exported"; the Authenticator sidebar section removed; Inject unreachable while the form is open; Caps Lock hint from OS state; emoji input; settings sidebar drag; uneven sidebar row heights; a `*` on an optional value; a 2FA card whose variables do not read as nested. CI red from stale ESLint suppressions: all 501 warnings and 236 suppressed errors fixed, rules promoted to `error`. Dependabot, CodeQL, `cargo-deny`. A `book/` docs site on Pages. Design below. _(new on 2026-09-14; takes review-01 §8 from 26)_

### Phase 24.1: Composite secrets and bundles

**Status:** Done.

The two container shapes, designed together. A _composite_ is one value with secrets inside it (a template with named `{part}` placeholders; `link`, `signed_link`, `connection`, `custom` kinds). A _bundle_ is one card holding several whole entries: a Spotify password, its web session and its API app; TMDB v3 and v4; four differently scoped keys: plus its own typed local variables, acting as a scope that composites and derived values (Python f-strings, template literals) resolve against; the acceptance test is a real Discord bot config. Per-variable value kinds (colour, large ids, bitfields, templates, dev formats). Must land before 25. _(new on 2026-09-14; bundles added the same day)_

### Phase 24.2: The secrets grid, legible at scale

**Status:** Done.

A multi-toggle type chip bar in All Secrets, a key pool collapsed to one card that expands to its members, a health scan that names type, field and state, and an "Add 2FA" flow from the Authenticator panel. _(new on 2026-09-14)_

### Phase 24.3: Calendar feeds

**Status:** Done.

A subscribable `.ics` URL per user, served by `unv-server` and Open to LAN, RBAC-filtered, token-addressed and revocable, never carrying a value. _(new on 2026-09-14)_

### Phase 24.4: Unique-ID registry

**Status:** In progress.

Server-side, keyed hash only, globally unique, RBAC-gated storage, rate-limited mint, check and lookup (limits derived from a measured benchmark), registration of externally minted IDs, prune-before-date. Opt-in. _(new on 2026-09-14)_

### Phase 24.5: A richer credential model

**Status:** In progress.

A secret-type registry so a type is a descriptor rather than twelve code sites; sixteen new types across dev/infra, self-hosted/media and personal-vault sets; `cookie` generalised to a _web session_ (several cookies, storage tokens, header recipes, bound UA/IP/origin, provider presets); API-key taxonomy as orthogonal axes auto-filled from issuer prefixes; FIDO CXF import and export. Must land before 25. _(new on 2026-09-14)_
