# ADR-0170: macOS builds and a launchd node install

Status: accepted

## Context

ADR-0166 vendored SQLCipher on macOS and added an informational CI job, and left three things open: no release asset, the node agent installed by hand, and no Mac keys in the interface. Signing and notarization need a paid Apple Developer account, which the project does not have.

## Decision

The release job builds the desktop app and the `unv` and `unv-server` archives on an Apple silicon runner and an Intel runner and publishes an unsigned `.dmg` and `macos-aarch64` and `macos-x86_64` archives. The bundle settings live in `src-tauri/tauri.macos.conf.json`, which Tauri merges only on macOS, with an ad-hoc signature because Apple silicon will not run an unsigned binary at all. Both archives are in the signed update feed and the manifest guard requires them. An Intel Homebrew install under `/usr/local/Cellar` is handed to `brew upgrade unv`.

`unv node install` plans for launchd on macOS (`plan_macos`): a role account made with `dscl`, ACLs from `chmod +a`, a LaunchDaemon for the agent, and for `reload=<label>` a root job that watches the signal file and kickstarts that launchd service. `--relay` is refused there. The interface prints Command-key labels on a Mac, and the Caps Lock hint reads the session flags through CoreGraphics.

## Consequences

The macOS plan is unit tested as data on Linux, but nothing here has run on a Mac: the macOS CI job stays informational and is promoted into the required checks once it has been green for a while. Users get a Gatekeeper warning on first launch until the maintainer decides to pay for signing and notarization.
