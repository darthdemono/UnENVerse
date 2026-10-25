# Phase log 7: 2026-06-11 - Rename: apiv → envv

> Salvaged from the Session History of `CLAUDE.original.md`, a rolling log
> that held every session before Phase 12 in one file. That file was split
> up and deleted on 2026-08-25; its pitfalls and decisions live in
> `CLAUDE.md`. This entry is its text verbatim - written as a log, not to
> the handoff template, so it has no Goal/Next Steps sections.

**Goal:** Rename all project identifiers from `apiv`/`API Vault` to `envv`/`EnvVault`.

#### What Changed

- Directories: `apiv-server/` → `envv-server/`, `apiv-cli/` → `envv-cli/`
- Build cache cleaned: `cargo clean` (10GB), `dist/`, `node_modules/.vite/`, `node_modules/.cache/`. `~/.cache/sccache` preserved.

#### Full Rename Mapping

| Old           | New           |
| ------------- | ------------- |
| `API Vault`   | `EnvVault`    |
| `api-vault`   | `envvault`    |
| `api_vault`   | `env_vault`   |
| `ApiVault`    | `EnvVault`    |
| `io.apivault` | `io.envvault` |
| `APIV`        | `ENVV`        |
| `apiv`        | `envv`        |

31 files touched: all Cargo.tomls, all Rust sources, all TypeScript, HTML, config files, README, CLAUDE.md.

#### Migration Note (for existing installations)

App identifier changed `io.apivault` → `io.envvault`. Existing users must move vault data:

```bash
mv ~/.local/share/io.apivault ~/.local/share/io.envvault
mv ~/.config/io.apivault ~/.config/io.envvault
```

#### Verify State

- Zero grep hits for old identifiers in source files
- `cargo check -p vault-core -p envv-server -p envv-cli` - clean
- `npx tsc --noEmit` - clean
- `cargo check --workspace` fails only because `dist/` was cleaned (Tauri macro checks for it at compile time) - `npm run build` or `cargo tauri dev` restores

---
