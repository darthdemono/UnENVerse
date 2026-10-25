# Phase log 50: Goal

Everything that happened after phase log 49 and was not written down: the 0.33.0 release prep (README, version bump, one big per-file commit script), the five CI failures found after the first push, the Dependabot backlog, and a bundle-card layout defect reported with screenshots. No new phase. Phase 31.3 and the other open items are unchanged (see "Still open").

## 1. README rewrite (before the bump)

`README.md` was brought to Phase 33. New or rewritten sections: Authenticator codes, Copy profiles, Web sessions / composites / bundles, Provider catalogue, Calendar feeds and unique-ID registry, UI and CLI parity (capability map + gate), the efficacy probe, CI / supply chain, a rewritten "not finished", and the phase table through 33. The version line reads 0.33.0 (README line 13, hand edited because `scripts/version.mjs` does not touch the README).

## 2. Version 0.33.0

Rule from the user: phase N is version `0.N.0`, bumped before the commit. `npm run version 0.33.0` patched `src-tauri/tauri.conf.json`, `package.json`, four `Cargo.toml`, and `Cargo.lock` (textually). It did **not** patch `package-lock.json`; lines 3 and 9 were edited by hand. `npm run version:check` agrees.

## 3. The big commit script

`~/Code/envvault-commit-all.ps1`: 349 pairs of `git add <file>; git commit -m '<sentence>'`, in the user's format (one file, one detailed sentence). 209 messages hand written, 139 generated for ADR files (title-derived). `package.json` and `package-lock.json` messages mention the 0.33.0 bump. `HANDOFF.md` at the repo root (a stale untracked Phase 26 note) was deliberately left out. Sources were in the scratchpad (`commit-all.ps1`, `msgs.py`); they are gone with the session, the ps1 in `~/Code` is the artefact. The assistant never ran `git commit` or `git push`; the user ran the script and pushed.

## 4. CI failures after the first push (all five fixed, commits handed over per file)

1. **Docker build**: `include_str!` of `secret-templates.json` (new, repo root) and `vault-core/data` (BIP39 wordlist) failed because the Dockerfile copies files selectively. Fix: `COPY secret-types.json secret-templates.json ./` and `COPY vault-core/data vault-core/data`. Commit `dae1b56`. Trap: any new `include_str!` outside an already-copied path breaks the image and nothing local notices; grep `include_str!` when adding a data file.
2. **Windows `glib-sys`**: my own `gtk` dependency in `src-tauri/Cargo.toml` was unconditional, so Windows tried to find glib through pkg-config. Moved under `[target.'cfg(target_os = "linux")'.dependencies]`. Commit `6df0c21`. A pasted suggestion to install GTK on the Windows runner was rejected: the crate is Linux-only, installing it papers over a wrong manifest.
3. **cargo-deny**: 0.18.5 cannot parse CVSS 4.0 scores now in the advisory database. `.github/workflows/ci.yml` pins cargo-deny 0.20.2 with a comment saying why. Commit `e6ad6a2`.
4. **clippy `needless_borrows_for_generic_args`**, appearing only on the newer stable (rustc/clippy 1.99.0; local was 1.95, hence invisible). The pasted CI analysis said `vault-core/src/lib.rs`; the site is `vault-core/src/users.rs:1892`, `bundle.is_some_and(&writable)` -> `is_some_and(writable)`. Commit `2ee0b6b`. Reproduced locally only after `rustup update stable`. Lesson: CI uses floating `stable`; run `rustup update stable` before trusting a local clippy.
5. **Windows `the_capability_map_matches_both_halves`**: `envv describe` printed nothing. Hypothesis (**not confirmed on a Windows runner**): the clap tree is big enough that parsing/describing overflows Windows' 1 MB main-thread stack in a debug build (Linux gives 8 MB), the process dies silently, stdout is empty. Fix: `envv-cli/src/main.rs` `fn main()` spawns `real_main` on a thread with `stack_size(32 * 1024 * 1024)`, exits 101 if that thread panics. Commit `5af9571`. Linux behaviour unchanged (`describe` still emits JSON; a "Broken pipe" panic seen locally was `head -c` closing the pipe). **If Windows still fails**, the next step is to make `capabilities.rs` include the child's stderr and exit status in its assertion message; today the empty-stdout failure names neither.

Local verification after 4 and 5: `cargo clippy --workspace --all-targets -- -D warnings` clean on 1.99.0; `cargo test --workspace` 483 passed, 0 failed; `cargo test -p envv-cli --test capabilities` ok. Not run: Docker build, any Windows job.

## 5. Dependabot backlog (15 PRs, #2-#17)

The user pasted a third-party summary with a suggested merge order and asked for nothing explicit. The assistant read **titles only**, not the diffs, and advised:

- `.github/dependabot.yml`: `toml` was missing from the cargo ignore list although vault-core pins `toml = "=0.8.2"`; added. Close PR #11 (toml 1.1.4). Commit `6e5a2c5`. The ignore list is now rusqlite, argon2, rand, hex, zeroize, time, rcgen, ssh-key, toml; the AGENTS "Dependency Pinning" block lists eight and does not mention toml (not edited, see Still open).
- Order I recommended, differing from the pasted one: Actions bumps first (#13, #15, #16, #17); then the two minor/patch groups (#3, #7); `aes-gcm` (#12) alone, because it is the `.vaultbak` / `.vaultarc` envelope that must stay byte compatible with the app's WebCrypto format (run `cargo test -p envv-cli` backup tests); hold on a branch until CI is green: Vite 8 (#8), TypeScript 7 (#4), vitest / coverage 5 (#5, #6), `base64` 0.23 (#10), `tower-http` 0.7 (#9). The Rust base image bump (#2, 1.85 -> 1.98) is a separate decision and interacts with item 4 above (a newer toolchain is what tripped clippy); the pasted order put it first as "the foundation"; I would not.
- Nothing merged, closed or rebased by the assistant. One PR at a time with CI between them.

## 6. Bundle card layout (screenshots from the user, narrow and wide)

Symptom: the bundle header row clipped ("Select visible members", Edit, Copy, Export all... overflowed at narrow width); every member section showed Primary / Rename slot / up / down / Remove wrapped awkwardly; member cards had a large empty area under the title and the footer buttons (`.env`, count) were cut off.
Cause: `src/ts/render.ts` (`buildBundleCard`, ~L805-1100) reuses the pool card wrapper (`pool-card-wrap`, `pool-card-summary`, `pool-card-members`) and adds `bundle-member`, `bundle-member-controls`, `bundle-actions`, `bundle-local-vars`, `bundle-union`, `bundle-collision`, `bundle-bulk-select` classes, of which only `.bundle-slot-tabs` and `.bundle-suggest` had CSS. The summary is a non-wrapping flex row with `min-height: var(--cs-card-h)`; `.card:not(.expanded)` has the same floor, so each member card kept the 236 px floor.
Fix (`src/css/cards.css`, appended block, comment explains it): summary `flex-wrap: wrap`; name `flex: 1 1 8em`; badge `margin-left: 0`; member grid `minmax(min(300px, 100%), 1fr)` with `align-items: start`; `.bundle-member` flex column; control rows `display:flex; flex-wrap: wrap; gap: 6px`; `.bundle-actions` spans the row; `.bundle-member .card:not(.expanded) { min-height: 0 }`; `.bundle-member .card-foot { flex-wrap: wrap }`. Commit `c05396f`.
Verification: `npx vitest run tests/bundle` 23 passed. **Not verified visually**: jsdom cannot see CSS and the user's screenshots were not re-taken. Not run: `npm run ui-lab`. The lab has no bundle screen (it seeds no bundle), which is why it never saw this; adding one (a seeded 3-member bundle expanded, at the narrow and wide viewports) is the proper regression test and was not done. Rejected alternative: restructuring the header into two rows in `render.ts`; CSS wrap was the smaller change and the markup has tests.
Residual suspicion: the "Select visible members" button sits before the icon and name in the DOM, which reads oddly; left as is because `tests/bundle-*` pin the order.

## 7. Memory and process notes

- Stale snapshot trap: `gitStatus` in the session header showed the version files as modified and `HANDOFF.md` untracked; the log shows the user committed since. Run `git status` before relying on it.

## Still open

- 31.3 catalogue content (from primary sources); maintainer-only: enable Pages, add the `CATALOGUE_SIGNING_KEY` secret (seed in scratchpad `catalogue-signing.seed.hex`, delete scratch copies after), confirm the published URL.
- Bulk `uid register` in the pane; `--fix` in the app's doctor; `envv user strict-write` against a remote; archive/WAL caveat (`VACUUM INTO` is the proper fix).
- Unverified in real environments: Docker build, Windows CI (stack theory), new panes / archive restore / online enrich in a Tauri window, ui-lab, fault injection on the Phase 33 pane tests.
- Add a bundle screen to ui-lab (section 6).
- AGENTS.md "Dependency Pinning" lacks `toml =0.8.2`; its pins list is also behind `.github/dependabot.yml`.
- Declined-online-enrich status line gets overwritten (cosmetic).
- Root `HANDOFF.md` is a stale untracked Phase 26 note, safe to delete, left alone.

## Notes for the next agent

- Whole phases map to `0.N.0`; sub-phases do not bump. The next whole phase is 40 (pentest 39 stays last). Phase 34 (Nodes) is the next queue item.
- After a toolchain-sensitive CI failure, update local tooling first (`rustup update stable`) and reproduce before editing; the pasted analyses were wrong twice (file name for clippy, and a Windows GTK install for a manifest bug).
- Floating `stable` in CI means new clippy lints can turn CI red with no code change; consider pinning the toolchain in `ci.yml` and bumping it deliberately alongside Dependabot PR #2.

> 2 line(s) about the maintainer's own machines were left out of this page.
