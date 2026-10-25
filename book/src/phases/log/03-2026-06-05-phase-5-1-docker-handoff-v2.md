# Phase log 3: 2026-06-05 - Phase 5.1 + Docker (Handoff v2)

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Security hardening + Docker deployment + post-5 polish.

#### Security Hardening Applied

- **Sub-user password hashing** (`vault-core/src/users.rs`): SHA-256 → Argon2id PHC (`m=32768,t=2,p=1`). Legacy `<salt_hex>:<hash_hex>` transparently upgraded on login.
- **TOTP anti-replay** (`vault-core/src/users.rs`): `totp_last_step INTEGER` column; rejects any code whose counter step ≤ last accepted step.
- **Rate limiter: failures only, real IP** (`envv-server/src/main.rs`): `rate_is_allowed()` + `record_auth_failure()` split. IP from `ConnectInfo<SocketAddr>` (not spoofable `X-Forwarded-For`).
- **CORS restricted** (`envv-server/src/main.rs`): `CorsLayer::permissive()` → explicit allow-list (`http://localhost`, `http://127.0.0.1`, `tauri://localhost`, port variants).

#### TypeScript Fixes

- `modals.ts`: `certificate` keyLabel `'API Key'` → `'Certificate'`; `file_blob` → `'File Reference'`
- `modals.ts`: `formToEntry()` always includes `'Universal'` in `projectIds`
- `modals.ts`: `pushUndo()` removes by object reference (not `pop()` - multiple deletes within 5s had wrong entry removed)
- `lock.ts`: Master password minimum 8 → 12 characters for new vaults
- `lock.ts`: Auto-lock timer resets on user activity (mousemove, keydown, mousedown, touchstart)
- `vault.ts`: `entry.provider` HTML-escaped before context menu dropdown label
- `render.ts`: `api_url`/`callback_url` only linked when protocol is `http://` or `https://` (prevents `javascript:` XSS); `rel="noopener noreferrer"`

#### Docker Deployment

- `Dockerfile` - multi-stage (rust:1.85-bookworm + debian:bookworm-slim); non-root `envv` user (uid 1001); SQLCipher dynamic link (`libsqlcipher0`)
- `docker-compose.yml` in project root; storage: bind mount `./envv:/data`
- `auto_unlock(state, password)` in `envv-server/main.rs` - reads `ENVV_PASSWORD` env var, derives key via Argon2id, opens DB, inserts owner session before server binds
- `db_path.parent()` fix - was trying to create system dirs as non-root; parent of db path always exists
- `GET /api/stats` - no auth; `{ secrets_stored, users_total, users_connected, vault_unlocked }` for Homepage/Homarr widgets

#### Docker Operations Reference

```bash
cd ~/docker && docker compose up -d                        # full stack start
docker compose up -d --build envv-server                  # rebuild after code changes
docker logs envv-server                                    # check logs
docker compose down envv-server && docker volume rm docker_envv_data  # reset vault
curl -X POST http://localhost:8743/api/unlock -H 'Content-Type: application/json' \
  -d '{"password":"master-password"}'                      # manual unlock
```

#### Critical Bug Fixes

- **Ctrl+Shift+V global hotkey removed** - intercepted system-wide paste, caused vault lock and apparent data wipe for remote sessions
- **Lock-on-hide skipped for remote vault** (`state.ts`): `visibilitychange` skips `lockVault()` when `st.store.isRemote` - remote lock destroys in-memory session token
- **Reset vault removed from unlock modal** - was accessible without any authentication

#### Nginx + Chunk Improvements

- `chunkFieldCopyText()` - context-aware copy format per chunk type (nginx: `key value;`, WG: `Key = Value`, SSH: `    Key Value`, env: `KEY=VALUE`, k8s/Traefik: `key: value`)
- `parseNginxConf(text)` - full tokenizer respecting quoted strings; handles `server {}`, `upstream {}` inside `http {}`/`stream {}`
- nginx smart display: `renderListenBadge`, `renderServerNameBadges`, `renderReturnBadge`, `renderCertPanel`
- SSL cert auto-linking via `findLinkedCert()` (by `${REF}` or letsencrypt path pattern)
- `nginx_key` chunk type - PEM cert/key display with green=cert, amber=key coloured blocks
- `showPromptLarge` - textarea prompt for multi-line paste; `Ctrl+Enter` to confirm

#### Docker Project Overhaul

- Two-column layout (`3fr 2fr`)
- Left column: `renderDockerServicesCard()` aggregates all `docker_service` chunks with nested YAML display; per-service scalar fields, ports, environment, volumes; hover-reveal action buttons
- Right column: networks, volumes, env files
- `serviceChunkToYaml()`, `exportServicesSection()` added to `chunk-ops.ts`
- `exportDockerCompose` rewritten: proper `- KEY=value` env format, YAML port/volume lists, `yamlStr()` quoting helper

---
