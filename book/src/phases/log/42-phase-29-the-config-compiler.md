# Phase log 42: Phase 29: the config compiler

Date: 2026-10-08. Phase 29 (queue position; was 32 → 29 after the 2026-10-08 renumbering; design in phase log 23 "T3 / Phase 30"). Lane A5, "Rust side first", then the TypeScript hook now that Phase 28 has landed. Version untouched. Nothing committed.

## Goal

The 29 chunk types are a typed IR that nobody treats as one. Phase 18 checks each generated file against its own tool, so every check sees one format; the errors that bite sit between chunks. Ship the first six rules and stop. A false positive costs more than a missing check, so no rule language and no guessing.

## Result

- `vault-core/src/config_check.rs` (new): `check_project(project, vault_names) -> Vec<Finding>`, `Finding::to_json`, `RULES` (8 ids). Pure over JSON. 13 unit tests, each rule with its firing case and its silent cases.
- `secretEnv`/`secretMounts` on the Kubernetes Deployment chunk, emitted by both k8s exporters (see below).
- `envv check [PROJECT] [--fail-on error|warning] [--json]` (`envv-cli/src/check_cmd.rs`, wired in `main.rs`/`lib.rs`). Project defaults to `envv use` / `$ENVV_PROJECT`, else every project. Table or JSON envelope (`command: "check"`, `data: {projects, errors, warnings, rules, findings}`). `--fail-on` exits 10 (`invalid`) with the findings in `error.details` in JSON mode.
- `config_check_project` Tauri command (pure over its arguments, so it works on a remote session; A1 rule) registered in `generate_handler!`.
- `src/ts/config-check.ts`: `checkProject`, `mountConfigCheck`, `vaultNames`. `render.ts` `renderConfigView` places a hidden `<section class="config-check" role="region">` under the header and fills it when Rust answers; silent when clean and in a plain browser. A finding's chunk name is a button that scrolls to and flashes the card (found by comparing `dataset.chunkId`, never by building a selector from the id).
- CSS in `src/css/config-view.css`: `.config-check*`, and `.chunk-card.flash` / `.docker-service-card.flash` (colour change, not a transform, same reason as `.auth-card.flash`).
- ADR-0137, `book/src/SUMMARY.md` entry, Key Decisions Log row, phase table row 29 done, next handoff is 43.
- Tests: `envv-cli/tests/check.rs` (real binary, 12 CLI calls, ~33 s because of Argon2), `tests/config-check.test.ts` (6).

Verification (last run, after the final edit): `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` all `ok` (vault-core lib 278, envv-cli lib 283, the new `check` integration test 1); `tsc --noEmit` clean; `eslint src tests --max-warnings 0` clean; `prettier --check src tests` clean; `vitest run` 62 files / 1294 tests (1288 at the end of Phase 28 plus the 6 in `tests/config-check.test.ts`). Mutation check on the Rust rules: removing the "no docker_service chunk" guard, the `@provider` skip, the self-duplicate dedupe, the network-disjoint condition and the "no k8s_service chunk" guard each failed a test.

## The rules, as built

Six designed checks, eight rule ids (two checks split into an error case and a softer one). All fire only on positive evidence; all skip `${ref}`, disabled chunks and anything that could be satisfied somewhere this function cannot see.

| #   | id                                 | severity  | fires when                                                                                                                                                                      | silent when                                                                                                               |
| --- | ---------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| 1   | `nginx-proxy-pass-unknown-service` | warning   | `proxy_pass` host is a bare name that is neither a docker service/`container_name` nor an `nginx_upstream`                                                                      | the project has no `docker_service` chunk; host is dotted, an IP, `localhost`, a `$variable`, a unix socket or a `${ref}` |
| 2a  | `wireguard-allowed-ips-duplicate`  | **error** | two peers claim the same masked network                                                                                                                                         | one peer repeating itself; `${ref}` values; disabled peers                                                                |
| 2b  | `wireguard-allowed-ips-overlap`    | warning   | one peer's network strictly contains another peer's                                                                                                                             | a `/0` default route; different families; disjoint networks                                                               |
| 3   | `traefik-middleware-missing`       | warning   | a router `middlewares` item matches no `traefik_middleware` name (case-insensitive)                                                                                             | item contains `@` (another provider)                                                                                      |
| 4a  | `k8s-deployment-secret-missing`    | **error** | a Deployment's `secretEnv`/`secretMounts` names a Secret no `k8s_secret` chunk creates in that namespace                                                                        | the project has no `k8s_secret` chunk; `${ref}` names; a mount with no path                                               |
| 4b  | `k8s-ingress-service-missing`      | warning   | Ingress `serviceName` (or its own name) has no `k8s_service` of that name in that namespace                                                                                     | the project has no `k8s_service` chunk                                                                                    |
| 5   | `compose-env-ref-unresolved`       | warning   | a `${NAME}` anywhere in a service value (or a `ref_name`) is neither an `env_file` key nor a vault entry name                                                                   | `${NAME:-d}`, `-`, `:?`, `+` operator forms; `$${NAME}`; `chunk:`/`bundle:` refs                                          |
| 6   | `pg-host-network-unreachable`      | warning   | `pg_connection.host` is a docker service S and a consumer C shares no network with it; consumer = lists S in `depends_on`, spells S in an env value, or reads `${chunk:<pg>/…}` | no consumer; either side has `network_mode`; both on the implicit default network                                         |

## Model change that came with it

The Deployment chunk could not express "mounts a Secret", so rule 4a would have been uncheckable. Two fields were added: `secretEnv` (list of Secret names) and `secretMounts` (list of `secret:/path`). `exportK8s` (TS) and `export_k8s` (Rust) emit `envFrom`, `volumeMounts` and a `volumes` block, de-duplicating repeated names; `parity/k8s.yaml` and the k8s project in `parity/vault.json` pin the bytes from both suites (regenerated with `PARITY_UPDATE=1`, Rust side verified against the new golden). The k8s starter Deployment gets the two empty fields so the form offers them. No migration: an older build shows them as extra rows and ignores them.

## Earlier narrowing, reversed

The first pass of this phase narrowed rule 2 to identical networks and replaced rule 4 with Ingress to Service, because the model had no secret fields. On request the original scope was restored: rule 2 reports overlap too (a warning, with the `/0` pattern exempt because it is the one standard setup that overlaps by design), and rule 4 is the Deployment/Secret check with the fields it needs. The Ingress check stays as 4b.

## Still outside this phase

"A gate a node runs before `apply`" needs Nodes (Phase 34). Scope is one project; no cross-project rule.

## Decisions

- **Location: `vault-core`, called over IPC by the app** (the `pools.ts`/`totp` shape from the twin-pair table), rather than a TypeScript twin with a parity fixture. Rejected: a TS implementation (a second opinion about what a project means is what the feature removes); putting the module in `envv-cli` (the Tauri crate does not depend on it).
- **Vault names passed in, not the vault.** Rule 5 needs "is there an entry called X"; the caller sends provider and `provider_keyid` strings, the function never sees an entry value. Matching is deliberately generous (case/punctuation normalised, prefix match on `PROVIDER_`) because a missed finding is cheaper than a false one.
- **Findings carry names, never values**, so `envv check` is not a materialising path and needs no `--reveal`/`--out`. A test puts a secret-typed field value in the project and greps the output.
- **`--fail-on` instead of failing on any finding.** Warnings are advisory; CI chooses its threshold. Rejected: always exit non-zero on findings (breaks interactive use and agents that read the report).
- **Panel hidden when clean.** A "no problems" line on every config view is noise. Rejected: always-visible status.

## Dead ends / things that bit

- `proxy_pass http://unix:/run/x.sock` parses to host `unix` and was flagged; fixed by checking the `unix:` prefix after stripping the scheme (caught by the unit test I wrote for exactly that input).
- `--bg2` is not a CSS variable in this app (`--surface`, `--surface2`, `--surface3` are); I used it first and caught it by grepping the stylesheet, not by looking at a screen.
- The integration test takes ~33 s: every CLI call derives the key with Argon2id (12 calls). Left as is; the alternative (building the vault JSON directly) would bypass the code path that proves `--json`, `--fail-on` and exit codes.

## What was not done

- No ui-lab screen for the panel: the lab seeds a vault and does not run IPC, and the panel only exists under Tauri. The panel's markup and escaping are unit-tested; its visual layout in a real window is unchecked (WebKitGTK window tests remain a human task).
- "A gate a node runs before `apply`" from the design: Nodes (Phase 34) do not exist yet.
- No `envv describe` golden update was needed (it is generated from clap); `agent.rs` still passes.

## Traps for the next agent

- A rule must keep its **silent cases** in its test. The cheap way to make a rule "always right" is to give it an evidence precondition; the mutation check above is how to confirm the precondition is actually tested.
- Do not put a field value in a finding message. `findings_carry_names_and_never_field_values_that_could_be_secret` is the guard; extend it, do not loosen it.
- `check_project` skips `disabled` chunks because the exporters do. If an exporter starts including disabled chunks, change both.
- Adding a rule means: a function, an entry in `RULES`, the call in `check_project`, a firing and a silent test, an ADR-0137 note.
- `ENVV_PROJECT` is read by `envv check`; `envv_check` in the test removes it from the environment.

## Commit (for the user; I never commit)

Phase 29 only. The tree also carries unrelated earlier uncommitted edits in several of these files (render.ts and config-view.css especially: Phase 24-28 changes). Left out: `AGENTS.md`, `the session records/`, `HANDOFF.md` (gitignored/untracked), `book/` (untracked; holds ADR-0137 and the SUMMARY line).

```powershell
git add vault-core/src/config_check.rs vault-core/src/lib.rs envv-cli/src/check_cmd.rs envv-cli/src/exporters.rs envv-cli/src/lib.rs envv-cli/src/main.rs envv-cli/tests/check.rs src-tauri/src/lib.rs src/ts/config-check.ts src/ts/chunk-ops.ts src/ts/chunks/starters.ts src/ts/render.ts src/css/config-view.css tests/config-check.test.ts tests/fixtures/parity/k8s.yaml tests/fixtures/parity/vault.json; git commit -m "Add the config compiler: six cross-chunk rules in vault-core, envv check with --fail-on, and a config-view panel that asks Rust, each rule silent without positive evidence."
```

## Follow-ups scheduled

The items under "What was not done" are scheduled as sub-phase 29.1 in `AGENTS.md` (Phase table and the Wave-E lanes); the rows there are the source of truth for their scope, footprint and edges.
