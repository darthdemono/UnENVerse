# Roadmap

Plan for 18 October 2026 to October 2027, ending in 1.0.0. It is a statement of
intent, not a promise: dates move when a defect, a security finding or a
dependency change takes priority. Compatibility and security work always goes
before new features. The same plan is on the
[project website](https://unenverse.darthdemono.com/roadmap/).

## Where it stands

Phases R00 to R09 are done: baseline, security hotfixes, CI release gating,
dependency modernisation, the shared `unv-app` layer, transactional SQLCipher
migrations with local recovery, SFTP backups with retention and restore drills,
the metadata and lifecycle core, the lossless config migration core, and
full-fidelity import from Bitwarden, 1Password and KeePass. The website (W1),
the CLI half of signed updates (R26) and a behaviour-test round across Rust and
TypeScript (Phase 56, 0.56.0) are also done.

## The year, by quarter

### October to December 2026: foundations

- **Release 0.56.0** and measure real test coverage for Rust and TypeScript.
- **R21 Compatibility contract and catalogue URL.** Publish what integrators can
  rely on (CLI output, exit codes, file formats), test it, and fix the catalogue
  URL defect.
- **R20 Delete, revoke, forget, rekey.** Every destructive verb does exactly what
  the UI says; "forget" purges old values from local history; the master password
  can be changed.
- **R19 Vault identity and single writer.** Detect cloned vaults, keep two hubs
  from pushing to the same node, and warn about live vaults in synced folders.
- **R18 Desktop CLI bridge.** `unv` and the tools that call it read allowed values
  from the unlocked desktop over a local-only channel.
- **R26 Signed desktop updates.** The Tauri updater, using the signed manifest the
  CLI already verifies.
- **R25 Community files and support bundle.** Issue forms, a demo vault and a
  redacted support bundle, so help costs a solo maintainer less.
- **R24 Site truth generation.** Release version, CLI reference, supported formats
  and tested integration versions are generated, so the site cannot drift.

### January to March 2027: deployment and sources

- **R12 Desktop hub and node deployment.** Deploy rendered configs to node agents
  from the desktop app, with health checks and approvals that keep their meaning
  under automation.
- **R10 SOPS, pass and gopass.** Import through the user's own tools, one-way sync
  while the source is authoritative, and an explicit cutover.
- **R15 UI quality of life.** Keyboard-first task flow, legible lists at scale, and
  an honest place in the UI for automation (parallel lane).
- **Coverage to the reachable maximum**, TypeScript and Rust (see the 1.0.0
  criteria).

### April to June 2027: rotation and reach

- **R11 OpenBao, Infisical, AWS and Bitwarden Secrets Manager.** Import with
  versions and references preserved; archive what cannot be operational.
- **R13 Rotation engine.** Rotate at the issuer, deploy through nodes, check
  consumers, revoke the old credential only when safe. PostgreSQL first.
- **R16 macOS and Windows parity.** macOS built, tested and documented (not
  claimed before then); Windows file protection and device-key placement brought
  to the Unix level (parallel lane).
- **R26.1 Package-manager channels.** winget and Scoop manifests; submission needs
  the maintainer's authorisation.
- **R27 Docker Compose file secrets.** Rotate a credential and deliver it as
  Compose secret files on every host that uses it.

### July to September 2027: extension and hardening

- **R14 Plugin foundation.** Third parties add readers, destinations, notifiers
  and connectors without entering the trusted core.
- **R17 Upstream integration experiments.** Two integrations that reuse mature
  open-source tools; kept or dropped on evidence.
- **R29 Home Assistant `secrets.yaml` via a node**, with validation and rollback.
- **R28 Scripts and sibling tools** (EltePortal, Dossify) and **R23 sibling site
  fixes** (SL2 Analyzer, PingDD). Both touch other repositories and wait for the
  maintainer's authorisation.
- **Independent security assessment** and the fixes it produces.
- **Release candidates** for 1.0.0.

### October 2027: 1.0.0

## Criteria for 1.0.0

1.0.0 ships when every item below is true, and not before.

1. Every phase above is complete, or removed from this page with the reason.
2. Test coverage is the reachable maximum for Rust and TypeScript, measured in CI,
   with each unreachable line (platform-only or defensive code) listed. Tests
   assert behaviour the documentation promises; the number is only a map.
3. The independent security assessment is done and its findings fixed or
   documented in [SECURITY.md](https://github.com/darthdemono/UnENVerse/blob/main/SECURITY.md).
4. Fuzz targets run as a release gate, and a release builds reproducibly.
5. Release tags are signed, and releases carry signatures, provenance and SBOMs.
6. The OpenSSF Best Practices Passing and Baseline badges hold, and Silver and
   Gold are met wherever one maintainer can meet them (see below).
7. macOS is shipped only if it is built, tested and documented.
8. A vault written by any 0.x release since 0.42 opens, with a backup kept first.

## OpenSSF Silver and Gold

Some criteria need more than one person: a bus factor of two, two unassociated
significant contributors, two-person review and a second holder of the release
keys. The plan is to keep the project open to a second maintainer (see
[CONTRIBUTING.md](https://github.com/darthdemono/UnENVerse/blob/main/CONTRIBUTING.md)
and the `good first issue` label) rather than to claim those criteria early.
Everything else is in the plan above: source headers, signed tags, reproducible
builds, fuzzing in the release gate and the coverage targets.

## What the project will not do

- Charge for it, gate a feature behind payment, or require a cloud service.
- Run an always-on daemon: scheduled work runs while the desktop app is open and
  the vault is unlocked, and the docs and UI say so.
- Claim macOS support, or any integration, before it is built and tested.
- Let automation satisfy a human-approval requirement.
