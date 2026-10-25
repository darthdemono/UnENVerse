# Phase log 6: 2026-06-11 - Security Audit

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Find and fix all security vulnerabilities across all code.

#### Bugs Fixed

**Stored XSS via `custom_icon`** (`src/ts/icons.ts`):

- `iconImgURL(slug)` interpolated user-controlled slug directly into `src="${...}"` without encoding
- `x" onerror="alert(1)"` broke out of attribute - stored XSS executed on every card render
- Fix: `encodeURIComponent()` wrapping

**RBAC bypass on `/api/vault/expiring`** (`envv-server` + `vault-core`):

- Endpoint returned full JSON of all expiring secrets regardless of session type
- Non-owner with read access to one category could see every entry's `api_key`, `api_secret`
- Fix: `get_expiring_entries_for_user(conn, days, permissions)` added in `vault-core/src/lib.rs`; server uses filtered variant for non-owner sessions

**Missing owner check on TOTP handlers** (`envv-server/src/main.rs`):

- `totp_enable_handler` and `totp_disable_handler` checked auth (valid session) but not authorization (owner-only)
- Any authenticated user could enable TOTP on another user's account (harvest base32 secret) or disable another user's 2FA
- Fix: `require_owner` guard at top of both handlers

**Cert pinning that didn't actually pin** (`src-tauri/src/lib.rs`):

- Phase 6 set `danger_accept_invalid_certs(true)` when fingerprint supplied but NEVER verified the fingerprint
- MITM presenting any certificate was silently accepted; master password sent to impostor
- Fix: custom `FingerprintVerifier` implementing `rustls::client::danger::ServerCertVerifier`:
  - Computes `SHA-256(end_entity DER)` and compares against pinned hex string
  - Delegates `verify_tls12_signature`/`verify_tls13_signature` to active crypto provider (aws-lc-rs)
  - Rejects handshake **before any request data is sent** when fingerprint doesn't match
- New deps: `rustls = "0.23"` (aws_lc_rs feature), `rustls-pki-types = "1"`, `sha2 = "0.10"`, `hex = "=0.4.3"`

#### Known Non-Critical (documented, deferred)

- Rate-limiter memory growth: `HashMap<ip, RateBucket>` never pruned - fixed 2026-06-12
- O(n²) card render: `st.vault.api_keys.indexOf(entry)` per card in `renderGrid()` - negligible at <1000 entries
- Dead branch in `getExpiryBorderClass`: redundant `days < 0` before `days <= 7` - cosmetic

---
