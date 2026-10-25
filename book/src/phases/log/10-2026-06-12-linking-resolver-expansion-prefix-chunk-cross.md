# Phase log 10: 2026-06-12 - Linking & Resolver Expansion (Prefix/Chunk/Cross-Chunk + Health/Server)

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Turn the prefix→service insight into a general linking system. Add reverse navigation, cross-chunk refs, rotation/health intelligence, infra-as-code export, and server-side concurrency safety. Build green at every batch (TS + envv-cli + envv-server + vault-core).

#### Data Model Additions

- `VaultEntry.rotation_days?: number | null` - rotation cadence; health flags overdue since `last_rotated_at` (`types.ts`)
- `VaultEntry.compromised?: boolean` - known-leak / emergency-rotate flag; critical health issue + red `⚠ LEAKED` card badge
- `SecretChunk.last_copied_snapshot?: Record<string,string>` + `last_copied_at?: string` - powers "changed since last copy" diff
- (Already added prior batch this session: `SecretType` includes `'chunk'`; `extra_vars[]`; `env_prefixes[]`)

#### New Ref Grammar (resolver)

`resolveFieldRef` (`chunk-ops.ts`) + Rust CLI `resolve_ref` (`envv-cli/src/main.rs`) now both support:

- `${Provider}` - vault entry `api_key` (legacy)
- `${Provider/field}` - direct field, with `FIELD_ALIASES` (APIKEY→key, SECRET→secret, USER→username, URL→api_url, …)
- `${Provider_keyid/field}` - disambiguate multiple keys from one provider (TMDB_v2)
- **NEW** `${chunk:ChunkName/FieldKey}` - resolve any chunk field across all projects; recursive (depth-guarded ≤4) so chained refs resolve. Source tagged `'chunk'`; purple `→ chunk:` badge.
- This single grammar makes SSH `IdentityFile`, WireGuard keys, pg↔env_file, any-chunk↔any-chunk linkable with the existing mechanism.

#### Frontend - Linking & Navigation

- **Reverse "used by" map** (`render.ts`): `buildRefIndex()` scans all chunk `${refs}`, keyed by `provider` + `provider_keyid`; each card shows consumer chips (`Navidrome · ND_LASTFM_APIKEY`). Pure read - answers "safe to delete?"
- **Click `${ref}` → jump** (`render.ts` + `chunk-ops.ts`): vault ref badges in config view get `data-action="jump-ref"` → switches to secrets panel, expands + `flash-highlight` scroll-to target. Resolves `Provider` / `Provider_keyid` split.
- **Env-prefix sidebar filter** (`state.ts`, `filters.ts`, `render.ts`, `vault.ts`, `index.html`): `st.activePrefixFilter` + `#sidebar-section-prefixes` chips; mirrors tag-filter plumbing. `prefix-filter-btn` toggles.

#### Frontend - Rotation, Health, Secrets

- **Rotation link** (`modals.ts` `markAsRotated`): one action now stamps `last_rotated_at`, prepends old value to `version_history` (cap 50), and clears `compromised`. Previously three disconnected facts.
- **Health scan extensions** (`tools.ts`): compromised, common weak literals (`password`/`changeme`/…), rotation-overdue, duplicate values, stale `${refs}` (project-scoped), orphans (in-project but unreferenced). Stale-ref findings include Levenshtein **"did you mean ${X/…}?"** suggestion.
- **Bulk generate chunk secrets** (`vault.ts` `genChunkSecrets` + chunk-type card button): fills empty `secret` extra_vars with crypto base64url(32B); never clobbers existing.
- **Diff-since-last-copy** (`render.ts` `diffAndStashCopy`): copying a chunk's resolved env stores a snapshot; next copy toasts delta (`N changed (POSTGRES_PASSWORD)`).
- **Rotation/compromised form fields** (`modals.ts` + `index.html`): `#f-rotation-days`, `#f-compromised`; `formToEntry`/`fillForm` wired; preserved through `saveModal` spread.

#### Frontend - Export (prior batch this session)

- `exportK8sSecret()` (stringData manifest), `exportTfvars()`, `exportEncryptedBackup()` / `importEncryptedBackup()` - AES-256-GCM + PBKDF2-SHA256 (210k iters), separate backup password, `.vaultbak`. Wired to settings buttons (`import-export.ts`, `vault.ts`, `index.html`).

#### CLI (`envv-cli`)

- **`envv env <project>`** - resolves a project's `env_file` chunks into a deployable `.env`; resolves `${...}` incl. cross-chunk; warns unresolved to stderr. Helpers: `canonical_field`, `entry_field`, `find_entry`, `resolve_ref` (mirrors TS).

#### Server (`envv-server` + `vault-core`)

- **Read-event audit** (`vault-core` new `pub fn record_event` wrapping private `append_audit` with `iso_now()`): `GET /api/vault` writes a hash-chained `read` row.
- **Optimistic concurrency**: `vault_version()` = SHA-256 of vault JSON. `GET` returns it as `ETag`. `PUT` with `If-Match` returns **409 Conflict** on drift. Backward compatible - no `If-Match` = old last-write-wins.
- **Client wiring** (`state.ts` RemoteVaultStore): `_apiFetch` surfaces `etag` (native fetch path only); `load()` stores `lastVersion`; `save()` sends `If-Match`, toasts on 409, clears version after clean save.
- **Token-expiry UI** (`users.ts`): expired/expiring(<7d) tokens highlighted red/amber, expired rows dimmed.

#### CSS (`cards.css`)

- `.badge-compromised`, `.badge-rotation-due`, `.usedby-badge`/`.usedby-row`, `.chunk-ref-chunk`, `.chunk-ref-jump:hover`, `@keyframes flashHighlight` / `.card.flash-highlight`

#### Verification

- `npx tsc --noEmit` - exit 0 (clean) after every batch
- `cargo check -p envv-cli -p envv-server -p vault-core` - exit 0 (clean)

#### What Worked

- **Generalizing one resolver** (`${chunk:Name/field}`) unlocked SSH/WG/pg linking without per-type code - the prefix-multiplier pattern applied to the whole graph
- ETag/If-Match optimistic concurrency closes the multi-client correctness gap **without** WebSocket infrastructure
- `BufferSource` typing + `new Uint8Array(len)` (not `Uint8Array.from`) satisfies the strict `Uint8Array<ArrayBuffer>` lib types for Web Crypto
- Mirroring TS resolver logic into Rust CLI kept `.env` output identical between app and CLI

#### What Didn't Work / Deferred (honest)

- **WebSocket push** - NOT built. Needs broadcast channel + ws route + frontend ws client + reconnection. The 409 conflict detection covers correctness; WS is latency/UX only. Deferred to keep build green - the one feature that needs its own batch.
- **409 on Tauri-https path** - `remote_request` Tauri command drops response headers, so ETag is unavailable there → falls back to last-write-wins. Works on http/local/web. Fix requires the Rust command to return headers.
- **Global link, copy-with-prefix selector, prefix-from-project suggestion** - env-link-modal reworks, not landed.
- **project-as-zip** - needs JSZip dep; skipped to avoid an install (k8s/tfvars/encrypted backup shipped instead).
- **Template stacks / preflight button** - largely subsumed: pg/redis starter chunks already exist (`makePostgresStarterChunks`); project-scoped stale-ref + expiry health covers preflight.
- **SSH/WG key-GEN → chunk auto-fill** - `generate_certificate` exists; wg/ssh keygen-to-chunk not wired, though the resolver now makes the _linking_ work once a key entry exists.

#### Next Steps

- **WebSocket push** as a focused pass: broadcast channel in `AppState`, `/api/ws` route, notify on `put_vault`, frontend ws client with reconnect + auto-reload
- Return response headers from `remote_request` Tauri command → enable 409 conflict detection on Tauri-https path
- Wire SSH/WireGuard keygen Tauri commands → auto-fill chunk fields with `${ref}` to the generated vault entry
- Global link button (link one entry's refs across all projects at once)
- Bump `tauri.conf.json` / crate versions to 0.7.0 (linking milestone)

---
