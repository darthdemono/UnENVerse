# Phases 43 to 56: the roadmap phases that are done

The operations roadmap has its own ids (R00 to R34). Each phase that is done or under way also has a project phase number, so the whole history reads in one sequence. R00 is the baseline and has no number of its own.

### R00: baseline + instruction split

**Status:** Done, 2026-10-10.

baseline green (717 Rust, 1501 Vitest); guides verified; /context check still for the maintainer.

### Phase 43: R01 security hotfixes

**Status:** Done, 2026-10-10.

all 7 items; preflight, ui-lab (194), bundled-engine workspace tests, deny, audit 0, Docker build and smoke test all green; only GitHub CI (needs a push) unrun.

### Phase 44: R22 UnENVerse website

**Status:** Done, 2026-10-10.

site, build and browser checks, link check, workflow, quickstart run in a clean container, enriched screenshots; unrun: live audit (DNS), workflow on a runner; maintainer: enable private vulnerability reporting and Pages HTTPS, then push.

### Phase 45: R02 CI release gating + speed

**Status:** Done, 2026-10-10.

all 9 scope items; validators run in containers; workflows linted and guarded by tests/workflows.test.ts; not run on a runner (needs a push): gate demonstration, Windows after-timing.

### Phase 47: R03 dependency modernisation

**Status:** Done, 2026-10-11.

dependency families modernised; legacy backup, Argon2 and OpenAPI goldens hold; preflight, deny and both npm audits clean; GitHub Linux/Windows CI needs a push.

### Phase 48: R04 unv-app extraction

**Status:** Done, 2026-10-11.

unv-app owns the shared engine; unv_cli re-exports it; contracts byte-identical; preflight, docs, Docker graph and complete frontend gate green.

### Phase 49: R05 ops runtime + local recovery

**Status:** Done, 2026-10-13.

transactional SQLCipher migrations; fenced single executor; local age-encrypted package, staged restore/verify, schedules and CLI/Tauri parity; final preflight green.

### Phase 50: R06 SFTP, retention, restore drill

**Status:** Done, 2026-10-13.

pinned pure-Rust SFTP transport; 7/4/6 receipt retention; monthly temporary restore drills; Docker OpenSSH conformance, host-key, interrupted-upload, retention and tamper coverage.

### Phase 51: R07 metadata + lifecycle core

**Status:** Done, 2026-10-14.

lifecycle tables and vault-scoped HMAC generations; id bindings; secret-free app/CLI timeline; incremental config history; old vault and archive compatibility.

### Phase 52: R08 config parsers + migration core

**Status:** Done, 2026-10-14.

encrypted migration manifests, source archives and template verification; Rust reader pipeline, CLI and desktop plan/apply parity; fuzz targets compile, runtime fuzzing awaits nightly.

### Phase 53: R09 password-manager sources

**Status:** Done, 2026-10-14.

full-fidelity Bitwarden, 1Password and KeePass migration sources; encrypted attachments and source archives; CLI-only atomic desktop boundary; JSON/ZIP fuzz coverage; format-derived labels; final gates.

### Phase 54: R26 signed desktop + CLI updates

**Status:** In progress, 2026-10-15.

CLI half built 2026-10-17 (unv update check|install|sign, ADR-0158, , release feed behind the UNV_UPDATE_KEY secret). Desktop Tauri updater not started: needs the maintainer's update key. Channel manifests follow as R26.1.

### Phase 56: T56 behaviour tests (Rust + TypeScript)

**Status:** Done, 2026-10-18.

not a roadmap phase; 0.56.0; ; roadmap phases below take numbers from 57; whole-workspace run still for the maintainer.
