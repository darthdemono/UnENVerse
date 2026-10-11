# ADR-0151: The shared application engine is `unv-app`

Status: accepted

## Context

The `unv-cli` package had become the shared implementation for the CLI, desktop application and optional server. That made its library name and package boundary misleading, and it prevented future engine code from depending on CLI-independent features without making the CLI package the centre of the dependency graph.

## Decision

Move the reusable library modules from `unv-cli` into the workspace library package `unv-app`. The `unv` binary remains in `unv-cli`, where it owns argument parsing and terminal output wiring. `src-tauri` and `unv-server` import `unv-app` directly.

`unv-cli` retains a `unv_cli` library facade that re-exports `unv_app`, so existing Rust callers continue to compile while they migrate. `unv-app` declares empty `connectors`, `destinations` and `aws` features now; its default enables the first two, while `unv-server` opts out of defaults. The features are names and dependency boundaries only until a later phase gives them implementation.

## Consequences

There is one implementation of rendering, import, backup, history and node support. The CLI contract and desktop/server behaviour do not change. Future connector and destination dependencies can be enabled by the CLI and desktop app without entering the server's default build.
