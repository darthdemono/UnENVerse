# Phase log 19: 2026-08-25 - Tooling, release automation, key pools (phase 16)

**Date:** 2026-08-24 → 2026-08-25
**Branch:** `main`, at `0539b03 Added Key Pools panel, sharing pool state with the CLI`.
Working tree clean. Version **0.7.1** across all six files.

---

## Goal

Four things, asked for in one line: add linting, format all code, add an
auto-versioning system, add publish/release to CI, add Doxygen.

Then it widened three times, each at the user's direction:

1. _"only push releases on version up, best way to not make a double push on a
   readme change"_ - which turned the release workflow from tag-triggered into
   version-bump-triggered.
2. Two CI failures pasted back from GitHub, both caused by this session's work.
3. A feature: multiple API keys per issuer swapped from the CLI to get past rate
   limits, a `purpose` field, and `rate_limit` as a strict int plus a period.
4. A UI panel for the pools.

---

## Current Progress

Everything below is committed and pushed. Verification, run at the end:

```
755 tests, 31 files (vitest)      - was 671/27 at Phase 11
134 tests (cargo test --workspace) - was 105
eslint .            0 errors, 464 warnings, 237 suppressed
prettier --check .  clean
cargo fmt --check   clean
cargo clippy        clean (workspace, --all-targets)
tsc --noEmit        clean
```

### 1. Linting and formatting

- **ESLint 10 flat config** (`eslint.config.js`), type-aware via an explicit
  `project: ['./tsconfig.json']`, plus **Prettier 3** (`.prettierrc.json`),
  **rustfmt.toml**, and clippy levels in `[workspace.lints.clippy]`.
- Every TS/CSS/JSON/YAML/MD file reformatted; `cargo fmt` across the workspace.
- `npm run lint | lint:fix | format | format:check | lint:rust | format:rust`,
  and `npm run check` for all of it plus typecheck and tests.

**Two exclusions that are load-bearing, not laziness.** `index.html` is in
`.prettierignore` because a formatter has already silently deleted an element
from it once (`#new-category-form`, Phase 3) and the app queries ids no tool can
prove are still present. `tests/fixtures/` is excluded because those files _are_
the assertion - reformatting one makes both parity suites fail against a file
that no longer describes any real config format.

### 2. Auto-versioning

`scripts/version.mjs` is the single writer for the version, which lives in six
files plus `Cargo.lock`.

```bash
npm run version            # print
npm run version:check      # verify all six agree - CI runs this
npm run version 0.7.0      # set
npm run version -- minor   # or patch / major
```

It does not commit, tag or push. Pushing the bump to `main` is what cuts the
release.

### 3. Release CI

`build.yml` now triggers on a push to `main` and asks **"does a tag `v<version>`
already exist?"** README fix → the run stops after the ~10-second `meta` job.
Version bump → the same push builds every bundle, publishes, and creates the tag.

Also added: a **GHCR image** for `envv-server` (`:x.y.z`, `:x.y`, `:latest`) with
a start-up smoke test, and `docs.yml` publishing Doxygen (`/ts`) plus `cargo doc`
(`/rust`) to GitHub Pages.

`ci.yml` gained `cargo fmt --check`, `cargo clippy`, the version cross-check,
Prettier and ESLint.

### 4. Doxygen

`Doxyfile` + `scripts/docs.mjs` (injects the version, works under PowerShell).
`npm run docs | docs:rust | docs:all`.

Doxygen cannot parse Rust - there is no front end and no extension mapping that
produces anything useful - so the crates are documented by `cargo doc` and the
two trees are published side by side.

### 5. Key pools, `purpose`, structured rate limits

```bash
envv entry add GitHub --key-id ci-1 --pool github-ci --generate
envv get --pool github-ci                       # redacted, like every stdout path
envv exec --pool github-ci -- gh pr list        # value → child env, never stdout
./deploy.sh || envv pool report github-ci --limited --for 15m
envv pool ls | show <name> | next <name> | reset <name>
```

`rate_limit` became `rate_limit_count: number` + `rate_limit_period`
(`second`…`year`), migrated **on read**. `purpose` records the justification
given to the issuer. New form fields, card meta rows, diff-tool rows and
`schema.json` entries for all of them.

### 6. Key Pools panel

Tools → **Key Pools**. A card per pool; per member: label, use count, last used,
available/cooling, and Mark limited (15m) / Clear. Reset per pool. The footer
names the `pools.json` path.

---

## The bugs that mattered

Ranked by what they did, worst first.

### 1. I destroyed one of the user's tests - unrecovered

`tests/card-sections.test.ts` was `MM` in `git status`: staged, **and further
modified in the worktree**. ESLint's `--fix` produced a broken tree three times
while I was isolating a bad rule, and I recovered each time with
`git checkout -- tests`, which resets to the **index** - silently discarding the
unstaged edit. One test disappeared (680 → 679) and I did not notice until a
test-count diff.

Searched git objects, the vitest cache, `coverage/`, and VSCodium's local
history. Nothing: an unstaged change is never hashed, so there is no object to
recover. The five tests in the staged version survived.

**The lesson is not "be careful with checkout".** It is that `MM` means two sets
of changes exist and only one of them is recoverable, and a bulk `git checkout --
&lt;dir>` cannot tell them apart. Check `git status --porcelain` for `MM` before
any bulk revert, and prefer `git stash push` - which keeps the worktree half.

### 2. `eslint --fix` broke a tree that had just typechecked clean

Two rules from the recommended/stylistic type-checked sets -
`no-unnecessary-type-assertion` and `non-nullable-type-assertion-style` -
misread this codebase's DOM helpers:

```ts
const $ = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;
```

Ask the checker for the type of `$('x') as HTMLSelectElement` and it infers
`T = HTMLSelectElement` from the assertion's own context, then reports the
assertion as redundant. It is not: drop it and the expression is `HTMLElement`,
on which `.value` does not exist. `--fix` rewrote ~170 assertions across
`src/ts/` and `tests/`, and `tsc` reported 130+ TS2339/TS2345 errors.

Diagnosed by elimination - with `projectService`, with an explicit `project`, and
with each rule disabled in turn. Both are now off with the evidence written into
`eslint.config.js`. **A lint rule whose autofix does not typecheck is worse than
no rule.**

### 3. The docs job could never have worked

`Doxyfile` sets `OUTPUT_DIRECTORY = docs/ts`, and Doxygen creates only **one**
level of it. `docs/` is gitignored, so on a fresh checkout the parent does not
exist:

```
error: tag OUTPUT_DIRECTORY: Output directory 'docs/ts' does not exist and cannot be created
```

Invisible locally purely because I had run `mkdir -p docs` before first testing
it. Fixed in `scripts/docs.mjs`, not in the workflow: a `mkdir -p` step in YAML
fixes CI and leaves `npm run docs` broken on a clean clone. Verified after
`rm -rf docs`.

### 4. Windows release builds died in OpenSSL, and my own check could not catch it

`vault-core` vendors SQLCipher and OpenSSL on Windows, and OpenSSL's `Configure`
is a Perl program needing `Locale::Maketext::Simple`. `windows-latest` ships
Strawberry Perl, which has it - and Git for Windows' MSYS Perl, which does not.
The build steps run under `shell: bash`, where Git's `/usr/bin/perl` comes first
on `PATH`.

The pre-existing guard in `ci.yml` tested `command -v perl`, which passed for the
entire life of the bug. **Presence was never the property that mattered.** Both
workflows now prepend Strawberry to `GITHUB_PATH` and assert
`perl -MLocale::Maketext::Simple -e 1`.

**Unverified: there is no Windows machine here.**

### 5. A stale `rate_limit_note` contradicted the limit beside it

The note holds rate-limit text that could not be read as a number and a window
("varies by endpoint"). It was carried forward unconditionally, so setting
`--rate-limit "100 req/min"` on such an entry left **both** - a card reading
`100/minute` with a note beside it flatly contradicting it.

The note is _derived_, never authored: there is no input for it anywhere. So a
readable limit now clears it. Regression test on both sides.

### 6. Two latent workflow bugs, neither of which had fired yet

- The changelog step excluded `${GITHUB_REF_NAME}` from `git describe`. On the
  new branch-push path that is `main`, the exclusion matches nothing, and
  `describe` returns the tag about to be created - **an empty changelog on every
  release**. Now excludes `v<version>` by name.
- `concurrency.cancel-in-progress` was true for anything not a tag. Harmless
  while `main` never reached this workflow; after adding the branch trigger it
  meant a second push could kill a release mid-upload. Now false on `main` too.

### 7. `site/` was outside ESLint's ignores

The Pages assembly dry-run left ~12 MB of vendored rustdoc/Doxygen JavaScript in
the repo, which ESLint then tried to type-check - 14 parser errors on files
outside every tsconfig. Self-inflicted and caught immediately, but it is the
reason both `docs/` and `site/` are ignored in three places now
(`.gitignore`, `.prettierignore`, `eslint.config.js`).

---

## Decisions made

**The 237-violation lint backlog is suppressed, not fixed.** `eslint
--suppress-all` wrote `eslint-suppressions.json`, so `npm run lint` passes today
and fails on anything **new**. Fixing them was rejected because a large share are
"unused import" findings in `vault.ts`, and deleting an unused named import can
drop a module from the bundle whose import has side effects - a behaviour change
disguised as a cleanup. Shrink it with `npx eslint --prune-suppressions`; never
regenerate with `--suppress-all`, which absorbs real regressions along with the
old debt.

**The release gate is tag existence, and only that.** Re-running the workflow,
pushing an empty commit, and a merge that changes nothing all resolve to "the tag
exists" - the same answer however many times it is asked. A `paths-ignore`
filter was rejected because it gets the one case that matters wrong: a commit
that bumps the version _and_ edits the README would be skipped.

**`version.mjs` patches `Cargo.lock` textually.** Regenerating it needs a
resolver run, which needs the registry, so the script would fail offline and in
any sandbox - for an edit that touches four lines of a file already in the tree.

**Pool membership is explicit** (`pool` field), not implicit by provider. Two
keys for one provider pooling automatically would turn `envv get GitHub`'s
ambiguity refusal into "pick one" _everywhere_, including `entry rm`.

**Pool state is not in the vault.** `save_vault` appends a `vault_audit` row per
update, so a CI loop would grow the hash chain without bound - the same reason
read events stopped being audited - and it is a compare-and-swap, so concurrent
`envv get` calls would start returning conflicts for _reads_. It lives in
`pools.json` at 0600, beside `sessions.json`. The cost is a per-machine cursor:
two CI runners each start at the first key. That is the honest trade.

**Exhaustion is reported, not detected.** `envv exec` hands the secret to a child
and never sees the child's HTTP responses, so the CLI cannot see a 429. The
caller, which did, reports it.

**`rate_limit` kept its name and gained neighbours.** The legacy string is still
written, regenerated from the pair, so a vault edited by a current build stays
readable to an older one. Migration runs **on read**, because an entry can arrive
from an older build, a remote server or a restored backup at any time - there is
no single moment when "the vault has been migrated" is true.

**The pool state file lives in `vault-core`.** Two programs read it, and this
repo has a convention about two implementations of one format. `envv-cli` keeps
only the CLI-specific parts and delegates. `vault-core` does not enable `time`'s
`parsing` feature, so `parse_rfc3339` hand-parses the exact shape `iso_at` writes

- strictness costs nothing when it only ever reads back its own output.

**No secret crosses the pool IPC boundary.** The frontend sends only the four
identity fields `entry_ck` needs; Rust computes the key with the same `entry_ck`
version history and audit attribution use.

---

## What Worked

- **Verifying every test by breaking the code.** The escaping tests, the
  untrusted-`pool`-field guard and the `rate_limit_note` fix were each confirmed
  by reverting the fix and watching the test fail. Two of them passed for the
  wrong reason until checked.
- **The golden-fixture convention, applied to a parser.** `rate-limit.json` is
  asserted from `tests/ratelimit.test.ts` and `envv-cli/tests/ratelimit.rs`. It
  cost twenty minutes and makes divergence a test failure.
- **Smoke-testing the CLI against a scratch vault.** Round-robin, exclusion of
  non-pooled keys, cooldown skipping and the exit-7 exhaustion path were all
  confirmed against a real SQLCipher file, not just unit tests.
- **`envv describe` needing no maintenance.** Generated from the clap tree, so
  the new `pool` group and flags appeared without being written down twice.

## What Didn't Work

- **`git checkout -- <dir>` as an undo.** See bug 1. It cost a test.
- **Trusting a recommended lint preset's autofix.** See bug 2.
- **Testing a docs build in a directory I had already prepared.** See bug 3.
- **Checking that a tool exists rather than that it works.** See bug 4.
- **`[ … ] && VAR=…` under `set -e`** - the test's exit status becomes the
  script's, so the _normal_ case aborts the job. Already commented in
  `build.yml`; I wrote it again in `docs.yml` and caught it on review.
- **Python heredocs for multi-line Rust edits.** `cargo fmt` reflows between
  edits, so a second `str.replace` against pre-format text silently fails its
  assertion. Format last, or match on something short.

---

## Next Steps

1. **`v0.7.0` never released, and there are no tags - local or remote.** That is
   correct behaviour, not a bug: `release` needs `desktop`, the Windows job died
   in OpenSSL, so nothing was published. The Perl fix should unblock it, and the
   next push of a bumped version is the test. **Watch that run.**
2. **Enable GitHub Pages - only the human can do this.** Settings → Pages →
   Build and deployment → Source → **GitHub Actions**. `docs.yml` will fail at
   `deploy-pages` until it is set.
3. **The Windows Perl fix is unverified.** No Windows here. If it still fails,
   the new step names the cause instead of dying a thousand lines into
   `openssl-src`.
4. **The Key Pools panel has never run in a Tauri window.** Tests cover the pure
   logic and the non-Tauri path; the IPC path holds by construction (shared
   `entry_ck`, shared state module) and by nothing else. Check: two pooled keys,
   `envv pool report … --limited` from a terminal, then open the panel and
   confirm it shows _cooling_ within one Refresh.
5. **Rewrite the lost test** in `tests/card-sections.test.ts`.
6. **Shrink the suppressions file** as the backlog is paid down. The
   `no-floating-promises` and `no-misused-promises` entries (144 of the 237) are
   the ones worth reading: each is a place where a failure is currently silent.

---

## Notes for whoever picks this up

`npm run check` is the one command that runs everything on the frontend side;
`cargo clippy --workspace --all-targets --locked` and `cargo test --workspace
--locked` are the Rust half. Both are what CI runs, so a green local run means a
green CI run - that was the point of putting the lint levels in `Cargo.toml`
rather than in a CI flag.

Three files now exist in twin pairs, and each pair is pinned by a fixture or a
shared module rather than by review:

| Concept            | TypeScript            | Rust                        | Pinned by                               |
| ------------------ | --------------------- | --------------------------- | --------------------------------------- |
| Config exporters   | `src/ts/chunk-ops.ts` | `envv-cli/src/exporters.rs` | `tests/fixtures/parity/*`               |
| Rate-limit parsing | `src/ts/ratelimit.ts` | `envv-cli/src/ratelimit.rs` | `tests/fixtures/parity/rate-limit.json` |
| Pool state file    | _(reads via IPC)_     | `vault-core/src/pool.rs`    | one implementation, shared              |

The third is the pattern to prefer where it is available: the app does not
reimplement the state file at all, it asks Rust. That only worked because the
app's `app_data_dir` and the CLI's `dirs::data_dir()/io.envvault` resolve to the
same directory - verified, not assumed. If that ever stops being true, the panel
will silently show a different vault's cursors.

Finally: the user commits and pushes themselves, and asked (2026-08-25) for
commit commands as a single chained line with a one-line message:

```
git add --all && git commit -m "<subject>" && git push -u origin main
```

---

## Also recorded in the rolling log

`CLAUDE.original.md` carried a condensed retelling of this session, kept
here so nothing is lost when that file stops being a log.

### 2026-08-24/25 - Phase 16: tooling, release automation, key pools

**Landed:** ESLint 10 + Prettier + rustfmt + clippy across the workspace, every
file formatted; `scripts/version.mjs` as the single writer for a version stated
in six files plus `Cargo.lock`; `build.yml` reworked to release on a version bump
rather than a hand-pushed tag, plus a GHCR image and a Pages docs workflow;
Doxygen for `src/ts/` and `cargo doc` for the crates; key pools (`pool` field,
`envv pool` group, `get`/`exec --pool`, a Tools panel); `purpose`; `rate_limit`
split into a count and a period. 671→755 frontend tests, 105→134 Rust tests.

**Cost a test.** `git checkout -- tests` after a bad `eslint --fix` reset to the
index and destroyed an unstaged edit to `tests/card-sections.test.ts`. See the
Git pitfalls section; it is the most expensive mistake in this file.

**Two CI failures were caused by this session's own work** and only surfaced on
push: Doxygen could not create `docs/ts` on a fresh checkout, and Windows release
builds died vendoring OpenSSL because Git's MSYS perl shadowed Strawberry Perl -
past a guard that had only ever checked `command -v perl`.

**Design note worth keeping.** Pool swap state deliberately lives outside the
vault, in `pools.json`. Putting a read-path counter in the vault would grow the
audit hash chain without bound and turn concurrent reads into compare-and-swap
conflicts. The file is shared between the CLI and the desktop app through one
implementation in `vault-core/src/pool.rs` - the third twin pair in the project,
and the first solved by sharing rather than by a golden fixture.
