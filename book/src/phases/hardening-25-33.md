# Phases 25 to 33: hardening, redaction, escaping and parity

These phases closed the first design review, built the redaction engine, the UI lab, escape-by-construction rendering, the config compiler, storage v2, the provider catalogue, the efficacy audit and UI to CLI parity. The numbers are queue positions: they were renumbered more than once, and the earlier numbers are listed on each phase.

### Phase 25: Close review-01, remainder

**Status:** Done. Earlier numbers: 21 → 24 → 25 → 26.

§7.1 real-router coverage and §2.4 shared Rust/TypeScript `permex` fixture and semantics. §8 (the lint backlog) moved to 24 because it turned CI red; §3.2 and §3.3 are already done.

### Phase 26: The redaction engine, outward

**Status:** Done. Earlier numbers: 24 → 29 → 30 → 31.

Shared Aho-Corasick matcher, explicit `unv shield`, file/directory `unv scan --exposed`, streaming-boundary protection and file-only exposure reports.

### Phase 27: The UI lab

**Status:** Done. Earlier numbers: 33 → 28 → 29 → 30.

Real-browser layout audit across a viewport matrix. First full run done; 1,155 findings triaged to 0 (a 24px target floor in `a11y.css`, three CSS fixes, four documented audit-rule judgements). Recurring after every major release.

### Phase 28: Escape by construction

**Status:** Done. Earlier numbers: 22 → 25 → 26 → 27.

Review-01 §2.3, an escaping tagged template across the renderer.

### Phase 28.1: Escape II: coverage the first pass left

**Status:** Done.

Hostile-value tests for the renderers Phase 28 migrated but did not test (health-scan rows, tool templates grid, LAN panel, onboarding steps, settings rows, bundle card, composite and web-session forms, calendar-feed dialog, UID-registry pane, context menus and the copy-as dropdowns); ui-lab screens the lab never visits (the same list, plus the Authenticator panel and the config-check panel with a stubbed IPC bridge); an ESLint rule that fails on an _untagged_ template literal containing an HTML tag inside the renderer modules (so a new renderer cannot be written in the old style);

### Phase 29: The config compiler

**Status:** Done. Earlier numbers: 23 → 30 → 31 → 32.

Cross-format semantic validation. Unblocked: it needed the Rust exporters, which landed in .

### Phase 29.1: Config compiler II: the next rules, and scope

**Status:** Done.

The rules Phase 29 deliberately left out because the first eight had not yet been used in anger: Traefik router `service` naming a missing `traefik_service`; an `nginx_upstream` no `proxy_pass` uses; Compose `depends_on` naming a service the project does not define; a service `networks` entry no `docker_network` chunk declares; a published port two services both bind; a `k8s_service` selector matching no Deployment label. Each ships with its firing and silent test and a mutation check, or not at all.

### Phase 30: Storage

**Status:** Done. Earlier numbers: 25 → 23 → 24 → 25.

Row-per-entry, schema v2 (review-01 §2.1). Scheduled after every data-model change so the vault migrates once: its own dependency note says it gets more expensive every phase: Phase 23 added ten fields to `VaultEntry`, 24.1 adds a type and a bundle table, and 24.5 adds sixteen types, so that argument is stronger than when it was written.

### Phase 30.1: Storage II: clients send deltas

**Status:** Done.

The remaining O(vault) cost in a save is the _client_ sending, and the server serialising, the whole document just to find the one entry that changed. Add `PATCH /api/vault` (and the matching Tauri `save_vault_rows` command and `Access::save_rows`) taking `{base, put: [rows], delete: [keys]}`, merged by exactly the same `storage::merge` rules, so the whole-document save stays as the compatibility path and as the import/restore path. The app tracks dirty entry ids (it already addresses everything by id, invariant 1) instead of re-sending `st.vault`. Also: the pinned-HTTPS proxy (`remote_request`) surfaces no response headers, so a remote store over it still writes without an `If-Match`;

### Phase 30.2: Storage III: rows below the project, retention, load

**Status:** Done.

A project is one row, so editing one chunk in a 200-chunk project rewrites the project; give chunks their own rows (kind `chunk`, parent project in the key) and merge them like entries, which also makes two people editing different chunks of one config a non-conflict. Retention by age, not only by count: `vault_saves` keeps the newest 2,000 saves, which a script saving thousands of times a day burns through in hours (an hour-old token then becomes a whole-vault conflict); keep saves for N days _and_ at least the last K, and say in `unv doctor` how far back a stale writer can still be merged. Full-load speed: 88 ms at 5,000 entries against about 35 ms for the old single parse;

### Phase 31: The provider catalogue

**Status:** Done. Earlier numbers: 34 → 36 → 37 → 38.

Envvault site as the public enrichment source. Unblocked: it waited on the `envv` name decision, closed as no-rename in Phase 20.

### Phase 31.1: Settings → Data row "Provider catalogue": status line from `catalogue_status`, *

**Status:** Done.

Settings → Data row "Provider catalogue": status line from `catalogue_status`, Update button calling `catalogue_update`, Rust's refusal (bad signature, rollback, unreachable) shown verbatim; disabled with an explanation in a plain browser. `tests/catalogue-row.test.ts`. _(new on 2026-10-08, from "not done")_

### Phase 31.2: `Provider` gained `acts_as`, `exposure`, `console_url` (validated against the 24

**Status:** Done.

`Provider` gained `acts_as`, `exposure`, `console_url` (validated against the 24.5 vocabularies; https only). `enrich` proposes them for catalogue-only prefixes and falls back to `AXIS_SIGNATURES` for compiled ones. `docs_url`/`rotate_url`/`revoke_url` stay carried-only. `catalogue/providers.json` re-exported.

### Phase 31.3: The catalogue holds only the 48 compiled prefixes, so it cannot yet add anything

**Status:** In progress.

The catalogue holds only the 48 compiled prefixes, so it cannot yet add anything. Curate more issuers from primary sources: for each, the issuer's own documentation URL naming the prefix, a `verified_on` date in the entry, and a mutation check that the prefix does not shadow a longer compiled one. Rules: no prefix from memory; a prefix under 4 characters needs a written reason (it will match many secrets); skip an issuer whose docs do not publish the format; fill `docs_url`/`rotate_url`/`revoke_url` the same way. Add `verified_on` and `source_url` to `Provider` (schema 1, optional, so old clients ignore them) before the first batch.

### Phase 32: Does it actually do anything?

**Status:** In progress. Earlier numbers: 30 → 26 → 27 → 28.

Behavioural tests + a control-by-control UI efficacy audit. Its own note said "could start today, and probably should"; placed here so it audits the surface 21–27 leave behind rather than one about to change.

### Phase 32.1: `tests/efficacy-cards.test.ts` (one entry of every registered secret type + a po

**Status:** Done.

`tests/efficacy-cards.test.ts` (one entry of every registered secret type + a pool, a TOTP seed, a 4-member bundle, same-provider twins; every `#card-grid [data-action]`), `tests/efficacy-config.test.ts` (one project per structured type with its starter chunks; every config-view control), `tests/efficacy-closers.test.ts` (each allow-listed closer pressed with its dialog open), shared `tests/probe.ts` (`probeKinds`, `judge`). A control _kind_ is silent only if no sampled instance did anything. Found and fixed: chunk up/down reordered the stored array while the screen (grouped by type) did not change, so a peer could be moved above the WireGuard interface invisibly;

### Phase 32.2: `tests/efficacy-surfaces.test.ts` (health-scan results, Settings sidebar-layout

**Status:** Done.

`tests/efficacy-surfaces.test.ts` (health-scan results, Settings sidebar-layout editor with a hidden section, icon picker, calendar-feed list against a stubbed `RemoteVaultStore`), `tests/efficacy-settings.test.ts` (every Settings input/select, segmented group and theme swatch must change a stored setting after Save; `settings-backup-pw` allowed), the config-view seeds extended (`.env` chunk, vault-reference fields, nginx key + certificate entry), and the lock screens and the `role="button"` card icon asserted in `honest-controls.test.ts`.

### Phase 32.3: Deleted the unreachable `copy-chunk-env` button, `hasChunkEnvRefs` and its handl

**Status:** Done.

Deleted the unreachable `copy-chunk-env` button, `hasChunkEnvRefs` and its handler (docker services are drawn by `renderDockerServicesCard`, which has `copy-env`). Per-card-type sampling of the grid probes was considered and rejected: a grid reset costs ~0.3 s, so sampling every type made each probe minutes long.

### Phase 33: UI ⇄ CLI parity

**Status:** Done. Earlier numbers: 32 → 27 → 28 → 29.

Close the gap list, make the check mechanical. After 23 and 24.x, which add surface to both halves.

### Phase 33.1b: `enrich --online` in the app: asks each issuer about its own credential, so it s

**Status:** Done.

`enrich --online` in the app: asks each issuer about its own credential, so it sends the secret to a third party and needs a consent screen naming the issuers first. The offline half shipped in 33. _(new on 2026-10-08)_ ✅: 2026-10-08: a checkbox in Tools -> Enrich plus a consent screen naming every issuer and the count sent to each; `enrich_online_targets` / `enrich_online`

### Phase 33.2b: `unv doctor`'s file-level checks (integrity, storage, salt pairing, file permis

**Status:** Done.

`unv doctor`'s file-level checks (integrity, storage, salt pairing, file permissions, audit chain) in the app: needs a local-database IPC command and an entry point on the unlock screen, where the need is greatest. _(new on 2026-10-08)_ ✅: 2026-10-08: `doctor_file` runs the same file checks as `unv doctor` over this machine's vault (not a remote one). `--fix` repairs remain CLI-only

### Phase 33.3: UI for `import-vault` (Bitwarden / 1Password / Proton, with the onboarding wizar

**Status:** Done.

UI for `import-vault` (Bitwarden / 1Password / Proton, with the onboarding wizard) and `backup archive` / `restore-archive` (full-fidelity, beyond `.vaultbak`). _(new on 2026-10-08)_ ✅: 2026-10-08: Tools -> Import -> From another password manager (`import_vault_plan`), Settings -> Full-fidelity archive (`backup_archive_build` / `backup_archive_restore`). Not tried in a real window

### Phase 33.4: UI for `user strict-write` (needs a server route too: `set_strict_write` is loca

**Status:** Done.

UI for `user strict-write` (needs a server route too: `set_strict_write` is local-database only today, so the CLI cannot set it on a remote either) and for entropy sources (the UI generators run in TypeScript and cannot honour `--entropy-source`; needs Rust-side generation behind the existing `entropy_sources` command). Bulk `uid register` in the unique-ID pane. _(new on 2026-10-08)_ ✅: 2026-10-08, except bulk `uid register` in the pane (CLI-only still): strict-write checkboxes + owner-only server routes, Settings -> Randomness with `entropy_fill` and a pool, `EntropyPoolEmpty` instead of a silent OS fallback

### Phase 33.6: `unv diff A B` (entry diff, redacted by default) and CLI management of the temp

**Status:** Done.

`unv diff A B` (entry diff, redacted by default) and CLI management of the template library. _(new on 2026-10-08)_ ✅: 2026-10-08: `unv diff`, `unv template ls|show`, `unv entry add --preset ID`; the ten presets moved to `secret-templates.json`, read by the pane and `vault_core::templates`

### Phase 33.7: Delete or wire the six registered-but-never-invoked Tauri commands listed under

**Status:** Done.

Delete or wire the six registered-but-never-invoked Tauri commands listed under `unused-ipc` in the capability map (`get_expiring`, `parse_totp_seed`, `totp_import_parse`, `entropy_sources`, `load_settings`, `save_settings`); their Rust tests reference them, so move those first. The test already forces a command that becomes used out of that list. _(new on 2026-10-08)_ ✅: 2026-10-08: five dead commands deleted; `entropy_sources` is now used; `unused-ipc` no longer exists
