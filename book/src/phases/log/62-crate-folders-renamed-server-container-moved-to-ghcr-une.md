# Phase log 62: crate folders renamed, server container moved to ghcr unenverse, vault migrated to composites, node on the VPS (half done)

Date: 2026-10-09. Version 0.42.1 (user bumped; agents never bump). Phases 1-42 done, next phase 43.

## Goal

## Done (evidence)

1. Folders: `git mv envv-cli unv-cli`, `git mv envv-server unv-server`; crate names unv-cli/unv-server, libs `unv_cli`/`unv_server`; 134 tracked files rewritten by script (skipped the session records/, review/, ADR-0149 body; lines with XDG_DATA_HOME / `.join("envv-server")` kept because the DATA directory spelling must not change). ADR-0149 got a struck-through line. `cargo test --workspace` 708 pass, `npm run check` 1497 pass, `cargo fmt --check` ok.
2. Windows CI: `vault-core/src/nodes.rs:467` accepts `has_root()` paths ("/etc/x" is not absolute on Windows). Tests were failing there only.
3. CLI bug found while connecting: `--env-file`/`UNV_ENV_FILE` read only `UNV_PASSWORD`/`UNV_SERVER_URL` keys, but his `~/docker/.env` says `ENVV_PASSWORD`, so every command answered "Wrong master password". `unv-cli/src/main.rs` `dotenv_get` now falls back to the `ENVV_` spelling. 176 unv-cli tests pass; clippy -D warnings clean. NOT covered by a new test (add one: dotenv with only ENVV_PASSWORD).
4. Docker (`~/docker/docker-compose.yml`, backups `*.bak-20261009`): service/container renamed `unv-server`, image `ghcr.io/darthdemono/unenverse/unv-server:latest` (pulled; reports 0.42.1), env `UNV_PASSWORD=${ENVV_PASSWORD}`, `command: ["--nodes"]`, volume `envv_data` kept (data). `homepage/config/services.yaml` container/url renamed (backup too). Old container `envv-server` removed. Vault volume backed up before the swap: `~/docker/envv/vault-backup-20261009.tgz` (root-owned, 588 KB). Healthy, auto-unlocked.
5. Vault (remote, via `target/debug/unv` with the env file): 7 ICS entries converted `env_var` -> `composite` kind `link` with named secret parts (script in session scratchpad, verified each by rendering `${NAME}` to a file and comparing sha256 to the original URL): RADARR_ICS, SONARR_ICS (`{apikey}`), ELTE_NEPTUN_ICS (`{calendar_token}`, `{calendar_id}`), ELTE_INF_ICS and ELTE_STUDENT_ICS (`{mailbox_id}`, `{calendar_key}`), GCAL_DARTHDEMONO_ICS and GCAL_THEDARKTERRA_ICS (`{account}` public, `{private_key}`). The primary `api_key` of those 7 and of CANVAS_ICS was emptied (the template renders it; one copy of the secret). `enrich --only X --apply` for TMDB, Groq, Pinecone, GitHub, Ultahost. Deliberately NOT applied: OmniRoute and OpenCode (enrich guessed OpenAI + api_url from the `sk-` prefix; they are proxy keys).

## Blocked / not done (needs the user)

- Apply (push) for wg0 is off on purpose: reload of wg0 needs root and PostUp changes need a wg-quick restart; a wrong push can cut the tunnel. Decide: root agent, or polkit rule, then `apply=true`, `validate`, `reload`, `require_approval=true` first.
- Deleting the old ghcr packages `envvault/envv-server`, `envvault/unv-server` and `git remote set-url origin git@github.com:darthdemono/UnENVerse.git`: refused by the classifier; user runs them (gh token now has delete:packages).
- Not touched: `docker` project (chunks fine, `unv check --all-projects` clean), wg/pc, nginx projects, entry roles/labels/bundles for other entries, the 33 api_key entries beyond enrich, the weak `elte`/`neptun` passwords and the Canvas key expiring 2026-10-09 (health scan flags them).
- No test for the dotenv ENVV fallback; peer comment lines not exported.

## Traps

- Never `cargo clean -p envvault` (deleted 33 GB). Stale paths after the rename: `cargo clean -p tauri -p tauri-plugin-global-shortcut -p tauri-plugin-shell -p tauri-plugin-window-state`.
- `unv render` prints fingerprints on stdout; use `--out FILE` to compare real values, then delete the file.
- The service name in compose changed to `unv-server`; anything referencing http://envv-server:8743 breaks (only homepage did; fixed).
- Part values passed via `--var` appear in argv briefly on this machine.

> 4 line(s) about the maintainer's own machines were left out of this page.
