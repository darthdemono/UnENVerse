# Phase log 4: 2026-06-05 - v3 Polish (Handoff v3)

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Fix outstanding bugs, add tool panes, new project types.

#### Bug Fixes Applied

- **B1 Tag editor**: `#f-tags-input` in modal; `formToEntry()` reads space-separated tags; `fillForm()` restores; `pinned` preserved on edit save
- **B2 Bulk ops checkbox**: selector fixed `[data-card-idx]` → `[data-idx]`
- **B3 `initTools()` idempotent**: `let _toolsInited = false;` guard - no accumulated duplicate listeners on re-unlock
- **B4 Keep-alive ping**: 90-second `setInterval` (`startPing`) on `connectRemote`; stops on `disconnectRemote`

#### New Tool Panes (5)

| Tool                | ID                | Description                                                        |
| ------------------- | ----------------- | ------------------------------------------------------------------ |
| Secret Diff         | `diff`            | Field-by-field compare of two vault entries; secrets masked        |
| Expiry Calendar     | `expiry-calendar` | Month grid with red/amber/green expiry dots; prev/next nav         |
| Cron Explainer      | `cron`            | 5-field parse + named shortcuts + next 5 fire times                |
| CIDR Calculator     | `cidr`            | Network/broadcast/hosts/mask from CIDR                             |
| JSON/YAML Formatter | `formatter`       | Format (JSON 2-space), validate, minify; real `js-yaml` round-trip |

#### Card Improvements

- Expiry border: red (`expiry-urgent`) if expired or ≤7d; amber (`expiry-warn`) ≤30d; green otherwise
- Tags as color-stable chips (string hash → 6 preset palette entries)
- Rotation age badge (green <30d, amber otherwise)
- Pin button in card footer; `entry.pinned` floats to top in `sorted()`

#### New Project Types (4)

| Type       | Chunk types                                             | Import               | Export             |
| ---------- | ------------------------------------------------------- | -------------------- | ------------------ |
| `apache`   | `apache_vhost`, `apache_directory`                      | `parseApacheConf()`  | `exportApache()`   |
| `haproxy`  | `haproxy_global`, `haproxy_frontend`, `haproxy_backend` | `parseHaproxyConf()` | `exportHaproxy()`  |
| `ansible`  | `ansible_vars`, `ansible_task`                          | -                    | `exportAnsible()`  |
| `postgres` | `pg_connection`, `pg_role`                              | -                    | `exportPostgres()` |

#### Chunk Additions

- `SecretChunk.notes?: string` - freetext italic under chunk header; editable in chunk edit modal
- `SecretChunk.disabled?: boolean` - greyed-out card (`opacity:.45; filter:saturate(.3)`); excluded from all exports

#### Icons

~70 new registry entries: Hetzner, Linode, OVH, Prometheus, Grafana, Redis, MongoDB, Authentik, RabbitMQ, Kafka, NATS, MinIO, Argo CD, TrueNAS, Pi-hole, AdGuard, DeepSeek, Groq, and more.

---
