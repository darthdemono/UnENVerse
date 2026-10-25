# Phase log 8: 2026-06-11/12 - Bug Audit + ENV Link Feature

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Fix all data-safety and reliability bugs across CLI, server, Docker, and GUI. Add ENV chunk ↔ vault link feature.

#### Changes Applied

**Docker fixes:**

- Runtime dep: `libsqlcipher4` → `libsqlcipher0` (correct Debian bookworm package; build was failing exit 100)
- Storage: named volume `envv_data:/data` → bind mount `./envv:/data` (files visible, easy backup)
- `docker-compose.yml`: `ENVV_PASSWORD=${ENVV_PASSWORD:-}` env var; image tag bumped to `0.6.0`

**Auto-lock setting bug** (`settings-panel.ts`): `parseInt('0') || 20` evaluates falsy 0 to 20 - setting "disabled" displayed "20min". Fix: `parseInt(..., 10) || 0`.

**ENV chunk ↔ vault link** (`chunk-ops.ts`, `render.ts`, `index.html`, `sidebar-trees.css`):

- "Link" button on `env_file` chunk header (`data-action="link-env-chunk"`)
- `buildEnvLinkMatches()` - 4-tier confidence scoring:
  - 100 = exact provider name match
  - 95 = compound `PROVIDER_KEYID` match (strips suffix after last `_`)
  - 88 = same raw secret value (≥6 chars) - strong signal even if key name differs
  - 75/70/63 = suffix match after stripping 1/2/3 prefix segments (handles `ND_LASTFM_APIKEY` → `LASTFM`)
- `#env-link-overlay` modal with per-row checkboxes; colour-coded: `--match`, `--create`, `--linked`, `--skip`
- `applyEnvLink()` - matched rows replace field value with `${REF}`; create rows add new `VaultEntry` then link

**Critical reconnect wipe fix:**

- `envv-server/main.rs` `get_vault()`: `Ok(None)` now returns `404 Not Found` instead of `200 + {"api_keys":[]}`
- `vault.ts` `finishInit()`: guard added - if remote returns 404/empty but local has data, prompts push-or-clear

**Duplicate event listener fix:**

- `remote-panel.ts`: `_remotePanelListenersAdded` flag - listeners added once only
- `users.ts`: `_usersPanelInited` flag - same pattern

**Silent save surfaced** (`state.ts` `RemoteVaultStore.save()`): `catch { /* ignore */ }` → toast with HTTP status + server error message

**Server `put_vault` validation**: rejects PUT missing `api_keys` array → 400 before writing

**Atomic vault save** (`vault-core/src/lib.rs`): data row + hash row previously two separate SQL statements. Crash between them left mismatched hash, failed integrity check on next unlock. Fix: `BEGIN IMMEDIATE … COMMIT` wraps both writes.

**CLI HTTP status checks** (`envv-cli/src/main.rs`): `get_vault()` 404 → empty struct; other non-2xx → `Err(status + body)`. Same for `get_expiring()`, `get_audit()`.

**TypeScript HTTP status checks** (`remote-panel.ts`, `settings-panel.ts`): `if (!r.ok) throw new Error(...)` before each `.json()` call.

#### What Worked

- Two-layer fix (server 404 + client guard) cleanly handles reconnect without data loss path
- `BEGIN IMMEDIATE` - simplest correct approach for SQLite atomicity
- Confidence-scored matching handles real-world prefixed env vars
- `_remotePanelListenersAdded` / `_usersPanelInited` flag pattern - minimal change, no listener refactor

---
