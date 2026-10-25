# Phase log 72: R05 local operations and recovery

## Goal

Complete roadmap R05 (project phase 49): a local-only operations runtime and
recoverable, encrypted vault packages with identical CLI and desktop capability.

## Current Progress

R05 is complete from `7d76292` with uncommitted work. The implementation adds
recorded SQLCipher migrations; fenced single-executor operation tables and state
machine; local destination markers and schedules; streamed age-encrypted
`.unvpkg` packages; inspect, staged restore and verification; and CLI, Tauri and
Settings controls pinned by the capability map.

Verification passed:

- `npm run preflight` - final complete pass: Rust format/Clippy/workspace tests,
  JavaScript format/lint/typecheck, 91 Vitest files / 1,561 tests, and Vite build.
- Core recovery tests reject a wrong identity and tampering without populating a
  restore target; migration golden tests open the existing vault fixture.
- A synthetic CLI round trip created a key and destination, packaged, inspected,
  restored and verified a vault with `UNV_SERVER_URL` and `UNV_PASSWORD` unset.

## The bugs that mattered

1. The Settings efficacy guard initially treated the six recovery action fields
   as silently ignored stored settings. The controls work through explicit action
   buttons, but without the documented allow-list the CI run failed; the list now
   names each input and its consuming action.
2. Existing schema helpers silently ignored `ALTER TABLE` failures. The ordered
   migration runner now checks the schema and propagates real failures instead of
   claiming a vault has migrated when it has not.

## Decisions made

- ADR-0152: numbered migrations are atomic and recorded in `schema_migrations`.
- ADR-0153: exactly one unlocked executor holds a fenced lease per vault.
- ADR-0154: packages expose only a non-secret envelope for inspection; their
  contents remain streamed age encryption and restore only after full validation.

## What Worked / What Didn't Work

The shared `unv-app` engine kept the CLI and Tauri paths thin, and the mechanical
capability fixture caught parity drift. The first complete preflight exposed
Prettier differences; the second exposed the stale efficacy allow-list; the final
preflight passed. No user vault or destination was used in verification.

## Next Steps

1. Start R06: SFTP transport, retention policy and a real restore drill.
2. In a real Tauri window, create a destination and package, then lock during a
   package to confirm the visible paused state and zeroization boundary.

## Notes for whoever picks this up

The executor intentionally exists only while the desktop vault is unlocked; idle
locking and long-lived background behaviour are R15 concerns. `approver.key` is
excluded from recovery packages. The first R05 commit must include the tracked
source, documentation and capability/test changes; handoff records are ignored.
