# Phase log 57: 2026-10-09 - Sub-phases 28.1 and 29.1, the 1.0 viability review, the decision to defer 30.1/30.2

Version unchanged (0.38.0). Nothing committed. **Commit script:** `~/Code/envvault-commit-phase39.ps1` (3 commits: `src/ts/utils.ts`, `tests/utils.test.ts`, `tests/escape-coverage.test.ts`). The other two files touched here (`eslint.config.js`, `vault-core/src/config_check.rs`) were first touched by Phase 38, so their per-file messages in `envvault-commit-phase38.ps1` carry a clause for this work. Run 34 to 39 in order. (The script is named 39 only because it is the next script number; the work is 28.1/29.1.)

## 28.1 - escape II

- `eslint.config.js`: a new block for the 17 renderer modules (`render, modals, users, tools, lan, onboarding, remote-panel, settings-panel, auth-panel, nodes-pane, history-pane, pools, audit, icons, perm-editor, vault, lock`) adds a selector that rejects a `TemplateElement` matching `<[a-zA-Z][^>]*>` unless the literal is the quasi of a tagged template. Proven live: appending `` `<b>${n}</b>` `` to `lan.ts` fails with "A template literal holding markup must be tagged". Existing code already passed (nothing to migrate). Exporters (XML/INI on purpose) and `utils.ts` (SVG constants) are not in the list.
- `src/ts/utils.ts`: `esc` and `escAttr` deleted (no caller outside their own tests); their two test blocks removed from `tests/utils.test.ts`.
- `tests/escape-coverage.test.ts` (2 tests): every id in the secret-type registry, with every string field, extra vars, tags, composite template and URL set to markup or an attribute breakout, rendered expanded; then a bundle with two members, a pool and a hostile project name through a full `render()`. Asserts no `img/script/iframe/[on*]` exists afterwards and the text is present.
- Not done from the row: per-pane tests for health-scan rows, the tools templates grid, LAN panel, onboarding, settings rows, calendar-feed dialog, UID pane, context menus; ui-lab screens for the Authenticator and config-check panels. The generic grid test plus the lint rule cover the old-style-renderer risk; the panes are left for a later pass.

## 29.1 - four rules

`vault-core/src/config_check.rs` (12 rule ids now, `RULES` updated):

| rule                                                                                                                                                                                                                                                                                                                                                                                                                                                     | severity | fires                                                                                         | silent                                                                                          |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `traefik-service-missing`                                                                                                                                                                                                                                                                                                                                                                                                                                | warning  | router `service` not defined by any `traefik_service`                                         | no service chunk at all; `name@provider`; `${ref}`                                              |
| `nginx-upstream-unused`                                                                                                                                                                                                                                                                                                                                                                                                                                  | warning  | an `nginx_upstream` no `proxy_pass` names                                                     | no server/location chunk; any `proxy_pass` is a `${ref}`                                        |
| `compose-depends-on-unknown-service`                                                                                                                                                                                                                                                                                                                                                                                                                     | error    | `depends_on` names no service/alias in the project                                            | no `docker_service` chunk; `${ref}`                                                             |
| `compose-port-clash`                                                                                                                                                                                                                                                                                                                                                                                                                                     | error    | two services publish the same host port+protocol (same bind ip, or either wildcard/`0.0.0.0`) | container-only ports, ranges, different protocol or ip, refs, one service listing several ports |
| 4 new tests; 9 mutations (`mut6.py`): service lookup removed, empty-silence removed, ref-silence removed, no-server silence removed, case folding removed, protocol compare removed, wildcard handling removed, `0.0.0.0` normalisation removed, digit check removed. All caught. Not built: the network and k8s-selector rules, `envv check --all-projects`, and a separate `gate()` (node pushes already refuse on error findings via `render_gated`). |

## 30.1 and 30.2 deliberately deferred

Both change the wire protocol and the storage schema. A 1.0 freeze should not make those changes in the same release, and the current cost (88 ms full load at 5,000 entries; whole-document client saves merged per row on the server) is acceptable. The merge token already allows protocol negotiation later. Recorded in review-02.

## Verification

`cargo fmt --check`, `clippy -D warnings`: clean. `cargo test --workspace`: 651 passed. `npm run check`: 1449 passed in 82 files. Earlier in the session: `npm run ui-lab` 167 passed (before these edits, which touch no markup).

## Also this session

- phase log 55 (Phase 38) and phase log 56 (Phase 39 security review); `review/review-02.md` (1.0 viability, ordered list of what must happen before the tag).
- Docker image: `docker build .` succeeded (EXIT 0) after the Rust 1.90 bump; before it, the image could not be built at all.

## Next

Remaining: 31.3 (catalogue content from primary sources only), 34.1 (`listen` mode for nodes), 38.1 (Nextcloud importer, Grafana probe), bundle acceptance (needs the maintainer's Discord config). Then, for the maintainer, the checklist in review-02.

## Addendum: 31.3 and the Docker finding

- `Provider` gained optional `verified_on` and `source_url` (`vault-core/src/catalogue.rs`, `envv-cli/src/enrich.rs` fills `None`). A new test parses `catalogue/providers.json` and checks every sourced entry (https source, both fields together, prefix of at least 4 characters, no duplicates). Two entries added, from the issuer's own documentation page: Tailscale `tskey-` (kb/1085, shown in the `--auth-key` example) and Grafana `glsa_` (service-accounts page, shown in a curl example). Both are example-only evidence, which is weak: Sentry's page names no prefix at all, so it was skipped as the rule demands. Do not add from memory.
- **Docker image did not build at all** (found by actually running `docker build`): `rust:1.85` cannot compile locked crates needing rustc 1.88 (`icu_*`, `idna_adapter`). Dockerfile now uses `rust:1.90-bookworm`. Result of the rebuild: see the final message; if it fails further down it will be a real finding for phase log 58.
- Decided not to build 34.1 (`listen`/dialing out from the hub): it adds an inbound listener on every node, which is new attack surface a 1.0 should not take on last-minute.
