# Phase log 44: Phase 31: the provider catalogue (2026-10-08)

## Goal

Phase 31 (AGENTS.md: "envvault site as the public enrichment source"; design in `the session records/23-2026-08-31-review-01-defect-fixes-the-key-pools-sidebar.md` under its oldest number, Phase 36). Make `envv enrich`'s issuer-prefix table updatable without a release. Design constraints taken from phase log 23: static/Pages, fetched WHOLE never per provider, bundled snapshot stays, signed with a key pinned in the binary, no secrets/user data, `envv catalogue update|show|diff`, same over IPC (invariant 10). The phase log 23 blocker (domain / name collision with envvault.dev) was closed as no-rename in Phase 20; the default URL is the repo's own Pages site, NOT envvault.dev.

## What was built (every file and why)

- `vault-core/src/catalogue.rs` (new, owns all rules): `Provider`, `Catalogue`, wire form `{payload: <JSON string>, signature: <b64>}` so the signature covers exact bytes. `verify` (Ed25519 against a key arg), `sign` (seed), `store` (verify, refuse rollback by `generated_at`, atomic write 0600), `load_cached` (re-verifies every call), `cache_path` (`ENVV_CATALOGUE_FILE` override, else `dirs::data_dir()/io.envvault/catalogue.json`, same dir the app/CLI already share). `validate` runs AFTER the signature: schema==1, fixed-width `generated_at`, 1..20000 providers, prefix 3..64 graphic ASCII, issuer/icon length caps, `secret_type` must exist in `secret_types::find`, all URLs https. 4 unit tests.
- `vault-core/Cargo.toml`: `ed25519-dalek = "2"` (already in Cargo.lock via ssh-key 0.6.7, no new crate downloaded; Cargo.lock got the dependency edge). `vault-core/src/lib.rs`: `pub mod catalogue;`.
- `envv-cli/src/catalogue_cmd.rs` (new): `CatalogueCmd` = `update [--url|--file]`, `show`, `diff`, `export` (compiled table as providers JSON), `sign <in> --key-file -o`. https only, `build_public_client` (the one allowed CA-validated client, per the guard test in tests/agent.rs), 4 MiB cap. Public reference data, so stdout is allowed (unlike emit).
- `envv-cli/src/main.rs`: `Commands::Catalogue`, handled before `run()` like `Describe` so it never asks for a password; added to the no-op arm in `dispatch`. `envv-cli/src/lib.rs`, `envv-cli/Cargo.toml` (`hex = "=0.4.3"`, same pin as vault-core).
- `envv-cli/src/enrich.rs`: new owned `Hit`, `catalogue()` (OnceLock, loaded once per process), `lookup()` = longest matching prefix across cached catalogue and compiled `SIGNATURES`, tie to the catalogue; `plan_entry` uses it; `bundled_providers()` public for export/diff. `AXIS_SIGNATURES` (acts_as/exposure) is still compiled-only and matched by prefix equality, so a catalogue-only prefix gets type/icon/url/env/tag but no axes. Compiled order-sensitivity (`sk-ant-` before `sk-`) is unchanged for the compiled half.
- `src-tauri/src/lib.rs`: `catalogue_status`, `catalogue_update(url)` (https only, 20 s, 4 MiB, then `catalogue::store`), registered in `generate_handler!`.
- `catalogue/providers.json` (new, 331 lines): seed source, produced by `envv catalogue export` from the compiled table (NOT retyped). This is the file maintainers edit.
- `.github/workflows/docs.yml`: daily cron + a "Sign the provider catalogue" step writing `site/catalogue/catalogue.json`; skips cleanly when the `CATALOGUE_SIGNING_KEY` secret is absent.
- `book/src/adr/0139-...md` + `SUMMARY.md` line; AGENTS.md phase row 31 and "next handoff" updated; `the session records/INDEX.md`.

## Decisions and rejected alternatives

- Ed25519 over minisign/cosign: dalek already in the lock; ~40 lines; no external tool in the client.
- Payload-as-string: rejected signing canonical JSON (re-serialisation drift is a classic verify bug).
- Re-verify the cache on every load: rejected "verify on download only" because a writable data dir would then be a trust root.
- Lookup picks longest prefix across both sets instead of "catalogue first": a catalogue `sk-` must not shadow compiled `sk-ant-`.
- Stamped daily rather than every 15 min (phase log 23 intention): content only changes on commit; a daily re-stamp keeps staleness visible without churn.
- Catalogue lives in the existing docs Pages deployment (Pages has one deployment; a second workflow would race it).

## Verification

- `cargo test -p vault-core catalogue`: 4 pass (tamper, wrong key, 1-char prefix and http URL refused at sign, rollback refused).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo test --workspace`: all result lines ok (no FAILED).
- E2E with the REAL pinned key: `catalogue export`, appended `zzt_live_`, `sign` with the generated seed, `update --file` (49 providers cached, file mode 0600), `show`, `diff` (added: zzt_live_), `enrich` on a scratch vault proposed type/icon/api_url/tag from the catalogue-only prefix; with the cache absent `enrich` said nothing to enrich. A one-letter edit of the signed file: "signature does not match the pinned key", exit 10.
- Fault injection NOT done on the rollback guard by reverting it; the unit test exists and passes, but I did not break `store_with` to watch it fail.
- `npm test` / ui-lab not run (no TS touched).

## Not done / traps

- THE SIGNING KEY: the keypair was generated in this session's scratchpad (`.../scratchpad/catalogue-signing.pem` and `catalogue-signing.seed.hex`), outside the repo. The user must move the 64-hex seed into the GitHub secret `CATALOGUE_SIGNING_KEY` and delete the scratch copies. Until then Pages publishes no catalogue and nothing breaks. If the user prefers their own key, replace `PINNED_KEY_HEX` and release.
- Default URL `https://darthdemono.github.io/EnvVault/catalogue/catalogue.json` is unverified against a live deployment (Pages enablement is still manual, Phase 24).
- No UI: the IPC commands exist but no button or Settings row calls them (invariant 10 gap, record as such). `tests/ipc-contract.test.ts` only checks frontend->registered, so it does not notice.
- Catalogue `docs_url/rotate_url/revoke_url` are carried and validated but nothing consumes them yet; axes (acts_as/exposure) are compiled-only.
- The catalogue content is only the 48 compiled prefixes; "a large set of providers" needs real curation, with sources, outside this change.
- `time` is pinned =0.3.37 and used only for formatting `generated_at`.
- Do not add a catalogue prefix shorter than 3 chars; `sign` refuses it by design.

## Addendum (same session): 31.1 and 31.2, and the remaining work as phases

Requested: write down what is left, map the UI button and the content to phases, do them. AGENTS.md now has rows 31.1, 31.2 (done) and 31.3 (planned).

- **31.1 UI**: `index.html` row `#s-catalogue-status` + `#s-catalogue-update` after the Setup wizard row; `wireCatalogueRow()` in `src/ts/settings-panel.ts` (assigned handlers, invariant 9; `inTauri` false -> button disabled with text pointing at `envv catalogue update`). `catalogue_update` now takes `Option<String>` and defaults to the new `vault_core::catalogue::DEFAULT_URL`, which `envv-cli` re-exports (one constant, was going to be three). `tests/catalogue-row.test.ts`: markup ids exist, browser branch disabled, IPC branch shows Rust's refusal verbatim (stub bridge installed before `vi.resetModules()` import). a11y + ipc-contract suites still pass (65 tests across the three files).
- **31.2 consumers**: `Provider` + `Hit` carry `acts_as`, `exposure`, `console_url`; `validate` rejects unknown axis values and non-https console URL; `plan_entry` uses the catalogue's axes first, else `AXIS_SIGNATURES`; `console_url` proposed. `bundled_providers()` now fills axes from `AXIS_SIGNATURES`, so `catalogue export` carries them and `catalogue/providers.json` was re-exported (not retyped). New unit test `an_unknown_axis_value_is_refused`. clippy clean, `cargo test -p envv-cli` all ok.
- **31.3 content: NOT done on purpose.** Writing issuer prefixes from memory would put unverified claims into a signed file. Needs web research with primary sources per issuer; rules are in the AGENTS.md row. The maintainer-only items (Pages, secret, URL check) are in the same row and in the main handoff above.
- Not verified: the UI row in a real Tauri window (jsdom only); `docs_url` etc. still unused by any reader.
