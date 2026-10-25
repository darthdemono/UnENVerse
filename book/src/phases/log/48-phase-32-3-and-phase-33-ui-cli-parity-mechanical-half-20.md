# Phase log 48: Phase 32.3 and Phase 33 (UI ⇄ CLI parity, mechanical half) (2026-10-08)

## Goal

32.3: remove what the efficacy probe proved dead. 33: "close the gap list, make the check mechanical" (design: phase log 23, "Phase 27 - UI ⇄ CLI parity" and the 17-entry gap list). The check was built first; gaps were closed cheapest-and-most-valuable first; the rest are sub-phases.

## 32.3

Deleted `copy-chunk-env` (button + `hasChunkEnvRefs` in `chunk-ops.ts`, handler in `render.ts`). `diffAndStashCopy` is still used by another handler. Rejected per-card-type probe sampling: one grid reset costs ~0.3 s.

## The mechanical check

- `tests/fixtures/parity/capabilities.json`: capabilities with `id, status, cli[], cli_flags[], ui[], tauri[], note`. Status `both | gap-ui | gap-cli | exempt`. UI evidence prefixes: `id:` (element id in index.html/tools-markup.ts), `tool:` (a `data-tool` pane), `src:` (file exists); `tauri:` commands in `tauri[]`.
- `envv-cli/tests/capabilities.rs` runs the real `envv describe` (`CARGO_BIN_EXE_envv`), takes every leaf `path`, reads `generate_handler!` from `src-tauri/src/lib.rs`, and fails on: a CLI or Tauri command in no capability; a name/id/pane/file in the map that does not exist; two capabilities claiming one command; `both` without a CLI side (command or `cli_flags`) and UI evidence; `gap-ui` that has a Tauri command; `gap-cli` that has a CLI command; an exemption with a note under 20 chars; a gap whose note names no phase; a `cli_flags` entry missing from `describe`; **a Tauri command offered as UI evidence that no `src/ts` file invokes** (and, in reverse, a command in `unused-ipc` that is now invoked). Checked by deleting a command from the map and adding a bogus one: both reported.
- That last rule found six dead registered commands: `get_expiring`, `parse_totp_seed`, `totp_import_parse`, `entropy_sources`, `load_settings`, `save_settings`. They sit in the exempt capability `unused-ipc`; 33.7 deletes or wires them. Notably, `load_settings`/`save_settings` are unused because settings live in the webview localStorage, so the old claim that "`envCopyField` can only be set from the app and changes what the CLI resolves" was false: the CLI never saw it. The real gap was the copy-affecting settings having no CLI flag.

## Gaps closed

- **enrich (offline)**: `enrich_plan` Tauri command wraps `envv_cli::enrich::plan_entry` (so `src-tauri` now depends on `envv-cli`); Tools -> Enrich (`src/ts/enrich-pane.ts`, markup in `tools-markup.ts`, nav button in `index.html`) previews proposals with reasons and fingerprints, applies only the ticked ones, resolves entries by id at apply time. `tests/enrich-pane.test.ts`.
- **doctor (document half)**: `doctor::document_findings` (schema, ids, bundle membership, pools) + `doctor_document` command + "Diagnose vault" in the Health pane (`initDoctorPane`, same file as enrich). The pane states that file/integrity/audit checks remain `envv doctor`.
- **Unique IDs pane**: `RemoteVaultStore.uidRequest` returns `{status, body}` (not null on error), `src/ts/uid-pane.ts`, markup + nav. Every button answers; 404/429/403 have specific messages; prune needs a date, has a dry run and a confirm. `tests/uid-pane.test.ts` (5). Trap: tests must import `state` AFTER `vi.resetModules()` or `instanceof RemoteVaultStore` is false.
- **`envv reset-vault`** (`reset_cmd.rs`, early-dispatched like `catalogue`, no password): `--yes` or tty answer, non-tty refusal (exit 8), `--dry-run`, leaves `.v1.bak`. `envv-cli/tests/reset.rs`.
- **`--env-case` / `--env-prefix`** global flags (+ `ENVV_ENV_CASE`, `ENVV_ENV_PREFIX`), stored once via `envfile::set_naming`, read by `export_naming()` in the dotenv/yaml/k8s exporters and `CopyOpts::default()`. Deliberately NOT applied to reference resolution or `exec` (a `${SPOTIFY_ID}` must not change meaning with a flag); tfvars keeps its fixed lower case. `envv-cli/tests/naming_flags.rs`.
- Reclassified, with reasons written in the map: settings (presentation, exempt), `lan` (exempt: `envv-server --host`), remote vaults (`both` via `--server/--fingerprint/--tofu`), seven pure dev utilities and two token tools (exempt), `exec`, `watch`, `shield`, `use`, catalogue publisher commands (exempt).

## Still open (AGENTS.md rows 33.1b-33.7)

gap-ui: import-vault, backup-archive, strict-write, entropy-sources. gap-cli: vault-diff, templates. Plus online enrich (needs a consent screen) and doctor's file checks (needs a local-DB IPC).

## Verification

`npm run check` green (73 files / 1375 tests); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` all ok. Not run: ui-lab; a real Tauri window for the new panes (jsdom with a stub bridge only); fault injection on the new panes' tests (only the capability gate was fault-injected).

## Traps

- A backtick inside an HTML comment inside a template literal broke `tools-markup.ts` (known pitfall); keep comments backtick-free.
- `src-tauri` -> `envv-cli` pulls the blocking reqwest into the app build; if build time or size matters, move `plan_entry` and the signature table into `vault-core` instead.
- The Health pane's Diagnose and the Enrich pane share `enrich-pane.ts`; rename if a third pane lands there.
- Pre-existing uncommitted changes in the tree (fixtures, `index.html` before this session) are not from this step.
