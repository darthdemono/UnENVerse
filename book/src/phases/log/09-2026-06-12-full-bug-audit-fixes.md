# Phase log 9: 2026-06-12 - Full Bug Audit + Fixes

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Comprehensive audit of all TypeScript and Rust code for security vulnerabilities, functional bugs, listener leaks, and unintended behavior. Fix everything found.

#### Changes Applied

**Security: Non-constant-time legacy SHA-256 comparison** (`vault-core/src/users.rs`):

- `hex::encode(h.finalize()) == hash_hex` - regular string comparison, vulnerable to timing attacks on legacy password hashes
- Fix: decode `hash_hex` to bytes; constant-time XOR-fold comparison: `computed.iter().zip(expected.iter()).fold(0u8, |acc,(a,b)| acc | (a^b)) == 0`
- No new crate dependency (avoids adding `subtle`)

**Security: `PUT /api/vault` auth order** (`envv-server/src/main.rs`):

- Payload validation (api_keys check) ran before `extract_session()` - unauthenticated malformed requests got 400 instead of 401
- Fix: `extract_session()` moved before api_keys validation

**Security: `GET /api/status` unlocked logic** (`envv-server/src/main.rs`):

- `!state.sessions.lock().unwrap().is_empty()` - reported `unlocked: true` when only user sessions existed (no owner session)
- Fix: `state.sessions.lock().unwrap().values().any(|s| s.is_owner)`

**Functional: `openAdd()` listener leak** (`src/ts/modals.ts`):

- Added new `input`/`change` listeners on every call without removing previous ones
- After N modal opens, each form field had N `saveDraft` listeners stacked - multiple sessionStorage writes per keystroke
- Fix: module-level stable `_saveDraft` function + `_draftBound` flag; listeners bound once (overlay DOM is permanent)

**Functional: `showDropdown()` / `showContextMenu()` listener accumulation** (`src/ts/modals.ts`):

- `onClick` listener added to `#dropdown` on each call; `close` listener added to `document`
- Item-click set `display:none` but did NOT call `dd.removeEventListener(onClick)` or `document.removeEventListener(close)`
- Re-clicking anchor element skipped the `close` handler (`e.target === anchorEl` guard), so old listeners never removed
- After N opens: N stacked `onClick` listeners on `#dropdown`, N stacked `close` listeners on `document`
- Fix: `_ddCleanup` module-level ref storing `() => { dd.removeEventListener(onClick); document.removeEventListener(close); }`. Called on: re-open (removes previous), item-click, outside-click. Both `showDropdown` and `showContextMenu` share this pattern.

**Functional: Local vault switcher doesn't reload** (`src/ts/vault.ts`):

- Switching remote → local set `st.store = new TauriVaultStore()` but never called `finishInit()`
- Remote vault data stayed visible indefinitely after switching to local
- Fix: clear `st.vault.api_keys` and `st.vault.user_categories`, then call `finishInit()`

**Functional: `st.allExpanded` not reset on lock** (`src/ts/lock.ts`):

- `lockVault()` called `st.expanded.clear()` but not `st.allExpanded = false`
- After unlock, all cards appeared expanded if the user had activated expand-all before locking
- Fix: `st.allExpanded = false` added to `lockVault()`

**Functional: Visibility lock fires before vault unlocked** (`src/ts/state.ts` + `lock.ts` + `vault.ts`):

- With `autoLockMinutes > 0`, minimizing window before completing first unlock called `lockVault('visibility')`
- `showRelockScreen()` stacked on top of `showUnlockModal()` (two overlays)
- Fix: `st.vaultOpen: boolean` flag added to global state; set `true` after successful `finishInit()` data load; set `false` in `lockVault()`; visibility handler checks `&& st.vaultOpen` before locking

**Data integrity: Rate-limiter HashMap never pruned** (`envv-server/src/main.rs`):

- `HashMap<ip, RateBucket>` grew unboundedly - IP-rotating attacks could exhaust process memory
- Fix: `record_auth_failure()` calls `buckets.retain(...)` when count >1000, evicting buckets with `window_start` older than 300s

**Consistency: Env field detection gap** (`chunk-ops.ts` + `render.ts`):

- `chunk.fields.filter(f => f.description === 'env')` missed fields manually added via chunk edit modal with `field_type: 'env_var'`
- Docker service env fields created by docker-compose import have `description: 'env'`; manually-added ones have `field_type: 'env_var'`
- `copy-chunk-raw`, `copy-chunk-env` handlers in `render.ts` had the same gap
- `chunkEnvFields` and `isEnvField` in `chunk-ops.ts` had the same gap
- Fix: all 4 filter sites now check `f.description === 'env' || f.field_type === 'env_var'`

#### Verification

- `npx tsc --noEmit` - exit 0 (clean)
- `cargo check` - all three crates (src-tauri, vault-core, envv-server) exit 0 (clean)

#### What Worked

- TypeScript modular flag patterns (`_draftBound`, `_ddCleanup`) cleanly isolated all listener leaks without restructuring callers
- Rust XOR-fold constant-time comparison without adding `subtle` crate
- `st.vaultOpen` flag minimal footprint - single bool added to `st`, set in 3 places

#### What Didn't Work

Nothing failed. All fixes applied cleanly on first attempt.

#### Next Steps

- `cargo tauri dev` - end-to-end UI verification
- Test dropdown: verify no double-fire after multiple re-clicks on vault-switcher
- Test local vault reload after switching from remote vault
- Consider bumping `tauri.conf.json` version to 0.6.1

---
