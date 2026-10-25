# Phase log 55: 2026-10-09 - Phase 38: stack integrations

Written for the next LLM agent: a changelog and decision record. Version bumped to **0.38.0**. Nothing is committed. Read phase log 51 to 54 first. **Commit scripts:** `~/Code/envvault-commit-phase38.ps1` holds 35 commits (every file first touched by Phase 38); files shared with 34 to 37 are committed by their scripts, whose messages gained a "Phase 38 also ..." clause. Run 34, 35, 36, 37, 38 in order: `pwsh -File ~/Code/envvault-commit-phase38.ps1`.

## 1. The problem

Integrations (Prometheus, Grafana, Homarr/Homepage, Nextcloud, VS Code) each hand-written in four places (Rust exporter, TS exporter, starters, rules) would add a dozen drift points. Phase 38 also had to name "the adapter shape they share".

## 2. What was built

The shape: a JSON descriptor per integration (`vault-core/data/stack-adapters.json`) with chunk types (fields, secret flags, defaults), an output document grammar (literal, `{f,as,fmt}`, map, list, each, singleton, group, entry, when) and rules (unique, required, exclusive, together, choices, needs_one_of). Two interpreters: `vault-core/src/stack.rs` and `src/ts/stack.ts`, pinned by golden files. Adapters shipped: `prometheus` (global, scrape configs, remote_write), `grafana` (datasources), `homepage` (services with widget credentials).

### New files

| File                                                                                | Why                                                                                                                                           |
| ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `vault-core/data/stack-adapters.json`                                               | the descriptors                                                                                                                               |
| `vault-core/src/stack.rs`                                                           | interpreter, YAML writer, rules, `new_chunk`, `starter_chunks`; 15 tests                                                                      |
| `src/ts/stack.ts`                                                                   | TS interpreter (typed node interfaces)                                                                                                        |
| `tests/stack.test.ts`, `tests/stack-ui.test.ts`, `tests/vscode-refs.test.ts`        | TS tests (17, UI, 12)                                                                                                                         |
| `envv-cli/tests/stack_parity.rs`, `stack_cli.rs`                                    | Rust golden parity (8) and CLI (3)                                                                                                            |
| `tests/fixtures/parity/stack-{vault.json,prometheus.yml,grafana.yml,homepage.yaml}` | shared vault and goldens                                                                                                                      |
| `vscode-extension/*`                                                                | completion/hover of `${Provider/FIELD}` and `envv scan --exposed`; pure logic in `src/refs.ts`; compiled with `tsc`, not run in a real editor |
| `book/src/adr/0144-...md`                                                           | decision record                                                                                                                               |

### Modified files

`vault-core/src/config_check.rs` (`check_project` runs adapter rules and a rule for `${ref}` naming nothing in the vault); `vault-core/src/nodes.rs` (`valid_exporter` accepts adapter ids); `vault-core/src/lib.rs`; `envv-cli/src/{data,chunks,exporters,starters,main}.rs` (dynamic type lists, `export_stack`, adapter arms); `src/ts/{types,chunk-ops,projects,render}.ts`, `index.html` (three project-type buttons); `envv-server/src/nodes.rs` (+2 tests); `tests/efficacy-config.test.ts`; `.github/workflows/exporters.yml` (promtool, Grafana API and Homepage jobs with bad-input controls, images pinned by digest); `.gitignore`, `.prettierignore`, `eslint.config.js`, `tsconfig.json`; README, `book/src/SUMMARY.md`, version files, AGENTS.md.

## 3. Decisions

1. Descriptors as data, not four code sites (ADR-0144). Rejected: a plugin trait per integration.
2. YAML scalar bare only if `^[A-Za-z_][A-Za-z0-9_.-]*$` and not a reserved word; else JSON-quoted. Tests initially expected wrong quoting (`30s` and `/metrics` are quoted, `on` is reserved); the tests were wrong, not the code.
3. The new types are **experimental** until CI validates against the real software; existing rule says a fixture alone graduates nothing.
4. Grafana finding: two same-named datasources do not error, only the last is provisioned. The first claim in the rule message ("rejects") was wrong, verified with a live container and corrected, plus a CI premise check.
5. Not built (38.1): Nextcloud importer, Grafana API probe (need live services and unverified formats).

## 4. Verification

- Local Docker: `promtool check config` accepts the golden and rejects basic_auth plus authorization; Grafana provisioned both datasources (`secureJsonFields` set); Homepage loaded groups, services, widgets, and returned `[]` for invalid YAML.
- Fault injection (`mut5.py`): 16 mutations (quoting, reserved words, group default, disabled chunks, `when`, secret flag, exclusive/together, duplicates, check not run, CLI ignoring adapters, nodes refusing adapter exporters, four TS twins). First run 3 BUILD ERRORs from unused-variable errors; rewritten with `let _ =` and `true ||` idioms; **all 16 caught**.
- Gates: see the end of this file for the final run.

## 5. Traps

- Adding `index.html` buttons changes the efficacy probes; `tests/efficacy-config.test.ts` needs a factory per new type.
- ADR citations must exist before `npm run check` passes (0144 was cited first once more).
- `cargo test <filter>` skips integration test files; use `--test`.

## 6. Next

Phase 39 (pentest checklist, read-only against the built system), then sub-phases 28.1, 29.1, 30.1, 30.2, 31.3, 34.1, 38.1, bundle acceptance, and a written 1.0.0 viability review.
