# Phases 1 to 20: foundations

The first twenty phases built the product: the encrypted vault, the server and CLI, access control, the agent-safe command line and the release pipeline. They keep their original numbers.

### Phase 1: Tauri wrapper, static frontend

**Status:** Done.

### Phase 2: SQLCipher + Argon2id encrypted storage, unlock flow

**Status:** Done.

### Phase 3: TypeScript + Vite, inline handler elimination, Projects groundwork

**Status:** Done.

### Phase 4: Remote vault server + CLI

**Status:** Done.

### Phase 5: Multi-user / RBAC

**Status:** Done.

### Phase 5.1: Security hardening pass

**Status:** Done.

### Phase 6: TLS on server, cert pinning, tag sidebar filter, bug fixes

**Status:** Done.

### Phase 7–10.1: Audit, auth model, permission expressions, Open to LAN, lost-update fix

**Status:** Done.

### Phase 11: Frontend test suite + module-by-module audit, 66 bugs fixed

**Status:** Done.

### Phase 12: UX persistence, Tauri window state, LAN wrong-vault fix, experimental project ty

**Status:** Done.

UX persistence, Tauri window state, LAN wrong-vault fix, experimental project types

### Phase 13: CLI parity (`unv-cli` as lib + thin binary), exporter golden fixtures

**Status:** Done.

### Phase 14: Agent-safe CLI: redaction, JSON envelope, exit codes, `describe`

**Status:** Done.

### Phase 15: Custom icons, live enrichment, Windows/distro portability, Docker RAM

**Status:** Done.

### Phase 16: ESLint/Prettier/clippy, `scripts/version.mjs`, release-on-version-bump + GHCR +

**Status:** Done.

ESLint/Prettier/clippy, `scripts/version.mjs`, release-on-version-bump + GHCR + Pages, key pools, structured rate limits

### Phase 17: Foundation & security

**Status:** Done.

CLI cert pinning on a shared verifier, entropy-source abstraction (TRNG), context zeroize on vault switch, `backup archive`, strict write scoping

### Phase 18: Core features & integrations

**Status:** Done.

`unv doctor`, importers (Bitwarden/1Password/Proton), `chunks/` coverage, selective read endpoint, exporter `${ref}` fix, all 11 project types graduated

### Phase 19: UX & accessibility

**Status:** Done.

Onboarding wizard, TOTP for sub-users, accessibility pass, Settings-panel overflow fix, `unv use`

### Phase 20: Operability & release

**Status:** Done.

`tracing` in all three binaries, real `/api/health`, `Retry-After` + request ids
