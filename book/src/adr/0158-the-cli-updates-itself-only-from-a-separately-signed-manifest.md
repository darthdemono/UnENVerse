# ADR-0158: The CLI updates itself only from a separately signed manifest

Status: accepted

## Context

A standalone `unv` has no package manager to update it, and an unsigned release page is not a trust root. A leaked provider-catalogue key must not be able to ship a binary, and a package-manager-owned executable must never be overwritten by a second updater.

## Decision

`unv update check|install` (Phase 54, R26) reads one manifest in the catalogue's signed-document shape (Ed25519 over the exact payload bytes, then a shape check) with its **own** pinned key, `vault_core::update::UPDATE_KEY_HEX`. The key is empty until the maintainer generates it, and an unpinned build refuses every update.

The order is fixed: install origin, signature, strictly newer version (no downgrade, no reinstall), archive size and SHA-256 against the signed manifest, then extraction of the exact `unv` entry (size capped), then replacement. Only an executable beside a `.unv-standalone` marker shipped in the release archive is replaced; known package-manager paths win over the marker and are refused with the manager's own update command. Replacement renames the old file to `unv.rollback`, renames the new one in, runs `--version` and requires the manifest's version, and restores the old file on any failure. The running executable is never written in place, so the same sequence works on Windows.

The release job signs `unv-update.json` when the `UNV_UPDATE_KEY` secret exists, checks it with `scripts/check-update-manifest.sh` and proves that guard fails on a manifest missing a platform.

## Consequences

A key rotation needs a release that still trusts the old key and pins the new one. The desktop app keeps its own Tauri updater (separate key, same release job) and is not updated by the CLI. `.deb`, `.rpm`, Scoop and winget installs update through their managers.
