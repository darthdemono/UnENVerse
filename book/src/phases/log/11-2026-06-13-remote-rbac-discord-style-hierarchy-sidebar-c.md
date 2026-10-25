# Phase log 11: 2026-06-13 - Remote RBAC, Discord-style hierarchy, sidebar/cert UX

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Close remote user-management gap, harden RBAC, add a role hierarchy + default admin, and a batch of QoL/UX fixes (sidebar sections, import parity, cert/nginx overhaul).

#### Env-file chunk copy resolves refs (`render.ts`, `chunk-ops.ts`)

- `chunkToString` env_file branch + `export-env-chunk` now resolve `${ref}` to actual values via `resolveFieldRef(.., true)`. Copying an env_file chunk yields a deployable `.env`, not `KEY=${Provider/key}`.

#### Remote user/class management - full parity (`envv-server/main.rs`, `state.ts`, `users.ts`)

- **Root cause:** the desktop Users panel only ever called local Tauri `invoke`; on a remote vault the local DB is locked → "Vault is locked".
- Added server routes (vault-core fns already existed): `PUT /api/users/{id}/rename|password|class`, `GET/POST /api/classes`, `PUT/DELETE /api/classes/{id}`, `GET/PUT /api/classes/{id}/permissions`.
- `RemoteVaultStore.api(path, method, body)` - public token-auth'd REST helper (handles 204, native-fetch + Tauri-https proxy paths).
- `users.ts` `invoke` is now a dispatcher: `st.store instanceof RemoteVaultStore` → maps each of 18 commands to its REST endpoint, else local Tauri. Call sites unchanged except `revokeToken` (now passes `userId`).

#### RBAC security audit - fixes (`vault-core/users.rs`, `envv-server/main.rs`)

- **CRITICAL** - `merge_user_vault_write` entry identity was `provider|account_name` (dropped `key_id`); collision let a write-scoped entry alias an out-of-scope entry → overwrite/delete bypass. Fixed: `key_id` added to `entry_ck`.
- **HIGH** - `get_vault`/`put_vault`/`expiring` used `get_user_permissions` (individual only), so class-granted access was dead. Switched to `get_user_effective_permissions` (class ∪ individual).
- **D** - write now requires ALL-scope coverage (`entry_write_allowed`): you may write an entry only if you hold write for every category + every non-Universal project it carries (read stays ANY-match). Blocks cross-project damage via shared entries.
- **C** - `filter_vault_for_user` no longer leaks the full `user_categories` taxonomy; only tags on visible entries (+ slash parents).
- **E** - `/api/auth` skips TOTP for access-token auth (`via_password` flag); TOTP gates interactive password login only.
- **TOTP** - `verify_totp_code` fails closed on a NULL secret (was `Ok(true)`).

#### Discord-style authority hierarchy + default admin (`vault-core/users.rs`, `envv-server/main.rs`)

- Tiers derived from caps (no schema column): owner=3, admin(`cap_manage_classes`)=2, moderator(`cap_manage_users`)=1, user=0. Helpers `authority_tier`, `user_authority_tier`, `class_authority_tier`, `token_user_id`. See `[[rbac-hierarchy-model]]` memory.
- Server now enforces capabilities (was blanket `require_owner`, which now only guards `/api/audit`): `actor_authority`, `guard_manage_user`, `min_tier_to_grant`. Act only on strictly-lower tiers; never grant a peer/superior; only owner mints admins or deletes built-in `cls-*`. `revoke_token` authorizes against the token's real owner (path user_id untrusted).
- `seed_default_admin` (first owner unlock, idempotent): user `admin` → class `cls-admin`; password from `ENVV_ADMIN_PASSWORD` else random printed once to stderr (no hardcoded cred, CWE-798). `admin` is `is_owner=false` - power via the class only.

#### Sidebar sections configurable (`settings-panel.ts`, `state.ts`, `types.ts`, `render.ts`, `tools.ts`, `index.html`)

- Added Environment, Tags, Env Prefixes to the Settings section editor (show/hide + reorder). VSCodium-style **drag-and-drop** reorder (grip + drop highlight) alongside ▲▼.
- Collapse buttons added to Tags + Env Prefixes; wired to `collapsedSections`.
- Tags/prefixes visible only when **enabled AND non-empty** (`isSidebarSectionEnabled`). One-time migration (`envvault-sb-migrated`) merges new sections into existing installs.

#### Password refs → `${name/password}` (`chunk-ops.ts`, `render.ts`)

- Env-link matcher + create-new-entry path emit `${name/password}` for password entries (`${name/username}` when value is a username). Added `PASSWORD`/`PASS`/`PWD` → `key` aliases so they resolve.

#### Import/export parity (`import-export.ts`, `tools.ts`, `vault.ts`, `index.html`)

- YAML import: Tools pane (`parseImport` yaml case + button) and Settings file import (`handleFileSelect` yaml branch). Settings picker accepts `.json/.yaml/.yml/.env`; button relabeled. Extracted `loadFullVault()`.

#### Cert type → Site / Issuer / Fullchain / Privkey + nginx auto-link (`modals.ts`, `types.ts`, `chunk-ops.ts`, `render.ts`, `index.html`, `config-view.css`)

- Modified `certificate` type (no DB migration): Provider→**Site/Domain**, Certificate→**Fullchain**, key→**Privkey**; new optional `cert_issuer` field (Let's Encrypt / Google / EnvV).
- nginx config view auto-surfaces a **canonical, full-PEM** cert card per referenced domain (`nginxCertDomains` from project name, `server_name`, ssl/`live/<domain>/` paths). Missing → one-click "Create cert entry" stub (`ensureCertForDomain`).
- Importing fullchain/privkey into a single-domain nginx project writes straight to that domain's cert entry - **no redundant `nginx_key` chunk**. `redundantCertKeyChunkIds` hides chunks whose PEM duplicates the cert entry + a "Remove N duplicate key-file chunks" button. Emojis removed from cert UI.

#### What Worked

- Capability-derived tier (no rank column) - zero schema churn, full hierarchy.
- Content-equality detection of redundant nginx_key chunks - robust for same-PEM dupes.
- Dispatcher pattern in `users.ts` - remote parity with unchanged call sites.

#### What Didn't Work / Deferred

- Users panel doesn't pre-hide management buttons from low-tier users (server rejects with 403 + toast; cosmetic only).
- `~/docker/.env` was reorganized (Gluetun prefixing, deleted vars not in compose) - **shared with AudioMuse-AI stack**; if that stack reads the same file it breaks (backup at `~/docker/.env.bak`).

#### Next Steps

- Restart server (`docker compose up -d --build envv-server`) to apply RBAC + admin seeding; grab the printed admin password from logs.
- Consider client-side gating of Users-panel buttons by tier (UX polish).
- Bump versions to 0.7.0.
