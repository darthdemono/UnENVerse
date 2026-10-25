# Phase log 2: 2026-06-05 - Phases 4 + 5 (Handoff v1)

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Add remote vault server, CLI, and full multi-user RBAC.

#### New Workspace Layout

```
vault-core/        ← shared Rust library (KDF, SQLCipher, load/save, integrity, audit, users)
envv-server/       ← Axum 0.8 HTTP server
envv-cli/          ← CLI binary (envv)
src-tauri/         ← Tauri 2 desktop app (thin wrappers over vault-core)
src/ts/            ← TypeScript frontend (split into 14 modules)
```

#### Database Schema Additions (Phase 5)

```sql
-- vault_meta: integrity hash
CREATE TABLE IF NOT EXISTS vault_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- key='data_hash', value=SHA-256(vault JSON) - verified on every unlock

-- user_classes: role templates
CREATE TABLE IF NOT EXISTS user_classes (
    id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL DEFAULT '',
    cap_manage_users INTEGER NOT NULL DEFAULT 0, cap_manage_classes INTEGER NOT NULL DEFAULT 0,
    cap_delete_projects INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
);
-- Seeded: cls-admin (all caps), cls-moderator (manage_users only), cls-viewer (no caps)

-- user_class_permissions
CREATE TABLE IF NOT EXISTS user_class_permissions (
    class_id TEXT NOT NULL, scope_type TEXT NOT NULL, scope_value TEXT NOT NULL, permission TEXT NOT NULL,
    PRIMARY KEY (class_id, scope_type, scope_value)
);

-- users: two columns added via idempotent ALTER TABLE
ALTER TABLE users ADD COLUMN class_id TEXT;     -- references user_classes.id
ALTER TABLE users ADD COLUMN totp_secret TEXT;  -- base32-encoded TOTP secret, NULL if disabled
```

#### vault-core Public API (Phase 5 additions)

- `verify_vault_integrity(conn)` - SHA-256 of vault JSON vs stored hash in `vault_meta`
- WAL mode: `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL` in `open_db`
- User classes: `list/create/update/delete_user_class`, `get/set_class_permissions`, `assign_user_class`
- Effective permissions: `get_user_effective_permissions` - class ∪ individual, write supersedes read
- TOTP (HMAC-SHA1, RFC 6238): `enable_totp`, `disable_totp`, `totp_enabled`, `verify_totp_code` (±1 window)

#### envv-server API (Phase 5)

Binds `127.0.0.1:8743` by default. Rate limiting: 10 failed attempts/IP/60s on `/api/unlock` and `/api/auth` → 429.

| Method          | Path                          | Auth  | Description                                                      |
| --------------- | ----------------------------- | ----- | ---------------------------------------------------------------- |
| POST            | `/api/unlock`                 | no    | `{password}` → `{token}` (rate-limited, integrity-checked)       |
| DELETE          | `/api/unlock`                 | yes   | Zeroize key, invalidate token                                    |
| GET             | `/api/status`                 | no    | `{unlocked, vault_exists, cert_fingerprint}`                     |
| POST            | `/api/auth`                   | no    | `{username,password,totp_code?}` or `{token}` → `{token}`        |
| GET             | `/api/ping`                   | yes   | Keep-alive: `{ok:true, ts:"..."}`                                |
| GET             | `/api/vault`                  | yes   | Full VaultData (filtered for non-owner)                          |
| PUT             | `/api/vault`                  | yes   | Save VaultData (merged for non-owner)                            |
| GET             | `/api/vault/expiring?days=30` | yes   | Expiring entries (RBAC-filtered for non-owner)                   |
| GET             | `/api/audit`                  | owner | Audit rows                                                       |
| GET             | `/api/stats`                  | no    | `{secrets_stored, users_total, users_connected, vault_unlocked}` |
| GET/POST/DELETE | `/api/users/*`                | owner | User management                                                  |
| GET/PUT         | `/api/users/{id}/permissions` | owner | Permission management                                            |
| POST/DELETE     | `/api/users/{id}/totp`        | owner | TOTP management                                                  |
| GET             | `/api/openapi.json`           | no    | OpenAPI spec                                                     |

**Axum 0.8 route syntax:** use `{param}` not `:param`. Old syntax panics at startup.

#### envv CLI

```
envv [--server URL] [--password PWD] <command>

list          [--project NAME] [--type TYPE]
get           <provider>
export        [--format dotenv|yaml|json] [--project NAME]
rotate-check  [--days 30]
import        <file.env>
audit         [--limit 50]
completions   <bash|zsh|fish|elvish|powershell>
watch         <file.env> [--project ID]
```

#### New TypeScript Modules

- `remote-panel.ts` - `initRemotePanel`, `renderRemotePanel`, `connectRemote`, `disconnectRemote`, `setRemoteFinishInitFn`
- `templates.ts` - 10 predefined secret templates (GitHub PAT, AWS Key, Stripe, PostgreSQL DSN, OpenAI, Cloudflare, SSH, TLS Cert, .env Variable, Docker Registry)
- `lock.ts` updated: server URL field, remote-first login, auto-lock redesign (0=never, 1-min warning toast)
- `state.ts` updated: `switchPanel` handles `'remote'`, OS theme watcher, lock-on-hide, `autoLockMinutes: 60`, `activeRemoteId`
- `settings-panel.ts`: 6 tabs (Appearance/Layout/Security/Data/Remote/Advanced), panel order editor
- `users.ts`: TOTP enable/disable, class assignment, change password, classes sub-panel

**CRITICAL - Tauri 2 camelCase:** All `invoke()` calls use camelCase args (`userId` not `user_id`, `classId` not `class_id`). Wrong case → "missing required key userId" error.

#### User Class System

```
Owner (vault creator) → all capabilities by default (no class needed)
Admin (cls-admin)     → vault.* write + cap_manage_users + cap_manage_classes + cap_delete_projects
Moderator             → vault.* write + cap_manage_users only
Viewer                → vault.* read only, no capabilities
```

Seeded on first vault unlock. Permission resolution: class ∪ individual; write supersedes read for same scope.

#### Phase 4+5 Bug Fixes

- All `invoke()` calls in `users.ts` now use camelCase - fixed "missing required key userId"
- Axum 0.8 route params: `:param` → `{param}` everywhere
- Vault-switcher dropdown replaces broken v2 stub

---
