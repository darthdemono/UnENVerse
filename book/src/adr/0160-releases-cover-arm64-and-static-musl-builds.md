# ADR-0160: Releases cover arm64 and static musl builds

Status: accepted

## Context

Releases shipped `linux-x86_64` and `windows-x86_64` only. Raspberry Pi and other arm64 homelab boxes, Windows on Arm and Alpine (musl) had no binary, and the server image was amd64 only. `unv update` picks its archive from `std::env::consts::OS` and `ARCH`, so release names must match those strings: `aarch64`, not `arm64`.

## Decision

The release matrix gains `linux-aarch64` (desktop bundles and CLI/server), `windows-aarch64` (desktop installer and CLI/server), and static musl CLI/server archives `linux-x86_64-musl` and `linux-aarch64-musl`. All of them build natively on GitHub's arm64 and x64 runners; the musl archives build inside an `rust:alpine` container so the toolchain is musl-native. `vault_core::update::current_target()` appends `-musl` on a musl build, so an Alpine `unv` never downloads the glibc archive. The signed update manifest must carry all six targets (`scripts/check-update-manifest.sh`). The `unv-server` image is built per architecture on native runners and joined into one multi-arch tag by a `docker-manifest` job. Package formats stay AppImage, deb, rpm and NSIS; the static archives are the answer for every other distribution.

## Consequences

arm64 Linux tests run in CI as an informational job (`test-arm64`, not part of `CI result`) until it has been green for a while. macOS is not shipped. Flatpak, an Arch `PKGBUILD` and Nix packaging are not part of this change.
