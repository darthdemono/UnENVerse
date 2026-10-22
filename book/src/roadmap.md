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
TypeScript (Phase 56, 0.56.0) are also done. Release 0.57.0 adds arm64 builds
for Linux and Windows, static musl archives for Alpine and other distributions, a
multi-architecture server image, a redesigned card footer and bundle cards, an
AMOLED theme and a product site with a changelog, features page and FAQ.

Release 0.58.0 lands the first slices of phases 57 to 66 and R21: imports from Git stores and
secrets services, approval version 2 and rule approvals for nodes, the rotation engine with
Cloudflare and PostgreSQL connectors, external-process plugins with a conformance suite, a command
palette, Windows owner-only access lists, vault identity with standby restores, and the scripting
contract. Several are partial by design and say so in their ADRs (0160 to 0167): the desktop hub
and node targets as rotation consumers, a plugins screen, the master-password change and `forget`
of R20, and the desktop bridge of R18 are still to do.

Release 0.59.0 adds the command-line halves of R20 and R25: `unv vault rekey` changes the master
password crash-safely and `unv entry forget` purges old values from history (ADR-0168), plus
`unv support bundle`, `unv demo-vault`, a tested Docker Compose secrets guide, a command reference
generated from the code, playbooks for a leak and the format policy, and a fix that bounds the key
derivation settings read from a backup file. The desktop actions for the password change, forget
and the support bundle, and the desktop bridge of R18, are still to do.

Release 0.60.0 is a security release: it fixes the issues an external audit of 0.58.0 found (node
grants, delegated user management, recovery package paths, permission expression depth, the master
password reaching child processes, the restore confirmation, container exposure and file modes).
Four low findings are recorded and left for later: the public counters, first-caller
initialisation, display fingerprints of weak secrets and key residue after lock.

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

## After 1.0.0: the agent-safety lane

In October 2026 ProjectDiscovery showed that a small fine-tune of an open model
(Qwen2.5-7B, about $8 of GPU time for the run) can look clean on every test and
still, on a trigger phrase, make a coding agent fetch a script that posts every
`.env` file and SSH key in the working directory to a stranger's server. No
benchmark or scanner can tell such a model from a clean one, so the defence is to
limit what the agent can read and reach. UnENVerse is not a sandbox and cannot
detect a backdoored model, but it can make the stolen thing worthless and the
theft visible. Five phases, planned after 1.0.0 (see [the queue](phases/roadmap-queue.md)):

- **Phase 76, R30: ref-only project files.** `unv env --refs` writes a file of
  `${refs}`, not values; a migrate command replaces a plaintext `.env` with it; the
  health scan reports plaintext secret files in a project directory.
- **Phase 77, R31: per-command secret release.** A secret is released only to the
  commands it was bound to, so a model-written shell line gets nothing.
- **Phase 78, R32: an SSH agent.** Keys are served over the agent protocol and
  never sit as files under `~/.ssh`.
- **Phase 79, R33: canary entries.** Decoy credentials planted where an attacker
  looks first, with an alert when one is used. It detects theft after the fact; it
  prevents nothing.
- **Phase 80, R34: sandboxed exec.** An opt-in launcher with filesystem and
  egress limits, failing closed on a platform that cannot enforce them.

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
