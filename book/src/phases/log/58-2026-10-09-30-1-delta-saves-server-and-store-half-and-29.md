# Phase log 58: 2026-10-09 - 30.1 (delta saves, server and store half) and 29.1 (two more rules)

Written for the next LLM agent: changelog and decision record. Nothing committed; version unchanged (0.38.0), sub-phases take no version bump. Read phase log 57 first (28.1, 29.1 first batch, 31.3, Docker fix).

## 1. What was built

| File                                   | Change and why                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `envv-server/src/lib.rs`               | New `patch_vault` handler, mounted as `.patch(patch_vault)` on `/api/vault`. Body `{ "put": [entry...], "delete": [id...] }`. Owner only (`require_owner`): a sub-user write is merged against the filtered document it was served, which a delta cannot reconstruct. Every `put` entry needs a non-empty string `id` (400 otherwise), `delete` takes a list of id strings. The server loads the stored document, removes deleted ids, replaces or appends put entries by id, and saves through the same `save_vault` as PUT with the same `If-Match` value, so the per-row merge, `409` conflict naming and `X-Vault-Merged` behave identically. Also wakes nodes and snapshots history exactly as PUT does. +1 test (`patch_saves_only_the_delta_through_the_same_compare_and_swap`): add via delta keeps the unsent entry, a stale-base edit disjoint from a newer row merges, delete by id, malformed put is 400. |
| `src/ts/state.ts`                      | `RemoteVaultStore.save` now calls private `send('PUT', data)`; new `saveRows(put, del)` calls `send('PATCH', {put, delete})`. One body of toast, `If-Match` and etag logic for both.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `tests/remote-save-rows.test.ts` (new) | Stubs `fetch`: PATCH method, body is exactly the delta, the returned ETag is sent as the next `If-Match`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `vault-core/src/config_check.rs`       | Two rules, RULES 12 to 14. `compose-network-undeclared` (error): a service `networks` entry that no `docker_network` chunk declares; silent unless the project declares at least one network, `default` always exists, refs are skipped, names compared case-insensitively. `k8s-service-selector-unmatched` (warning): the starters generate `selector: app: <name>`, so a Service selects the Deployment of the same name and namespace; fires when Deployments exist and none matches; silent with no Deployment. +2 tests.                                                                                                                                                                                                                                                                                                                                                                                        |

## 2. Decisions and rejected alternatives

1. **The server still loads the whole document to apply a delta** (marked `ponytail:` in the handler). The saving is what crosses the wire and what the client must build. A row-level apply inside `storage.rs` was rejected for now: it would need its own merge entry point and its own tests for a gain nobody has measured. Revisit with `storage::tests::measure_one_edit_against_the_blob`.
2. **Owner only.** Sub-user PATCH rejected for the filter-reconstruction reason above. A sub-user keeps using PUT.
3. **Unconditional when no `If-Match`**, like PUT.
4. **The k8s selector rule is by name, not by label.** The model has no independent selector or label fields (the exporter writes `app: {name}` for both), so the design row's "selector matching no Deployment label" can only mean name equality. Written down here instead of inventing fields.
5. **Network rule is silent without positive evidence** (no network chunk means no finding), same discipline as the other rules.

## 3. Verification

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`: clean. Clippy first flagged `map_or(true, ..)` in `patch_vault`; fixed with `is_none_or`.
- `cargo test --workspace`: 655 passed, 0 failed (+3).
- `npm run check`: 1450 tests in 83 files, all pass; lint, typecheck, prettier clean.
- Fault injection (all caught): delete not applied (patch test fails); `compose-network-undeclared` condition forced false; `k8s-service-selector-unmatched` condition forced skip. Each restored and diffed against a full-path backup.
- Not run: ui-lab (no UI change), Docker, Windows.

## 4. Not done (30.1 and 29.1 remainders)

- **App dirty tracking.** The Tauri side and `state.ts` stores still save the whole document; nothing calls `saveRows` yet, and no Tauri `save_vault_rows` / `Access::save_rows` exists. The app tracks no dirty ids. This is the larger half of 30.1 and the one that touches every mutation site in the renderer.
- Pinned-HTTPS proxy (`remote_request`) still returns no headers, so ETag and `X-Vault-Merged` do not reach the store over it.
- 30.2 (chunk rows, age-based retention, full-load speed) untouched.
- 29.1: `envv check --all-projects` and a separate `gate()` function remain unbuilt (the push gate already refuses on errors).
- 38.1, 34.1, 28.1 per-pane tests, 31.3 further catalogue entries unchanged.

## 5. Traps

- `cargo test` in the background with `&` inside a subshell returns at once; poll the output file, do not trust the task-completion notice.
- A `cargo fmt` between an edit script and a second `str.replace` silently breaks the replace (again hit here); assert `old in s`.
- The k8s rule will fire on a hand-written Service whose pods are labelled differently; the starters are the only generator, so this is by design.

## 6. Commit script

`~/Code/envvault-commit-phase40.ps1` holds the one new file (the TS test). The other three changed files were first touched by Phases 34, 38 and 39, so their messages in the 34, 38 and 39 scripts carry a "30.1 also" / "29.1 also" clause. Run 34 to 40 in order.
