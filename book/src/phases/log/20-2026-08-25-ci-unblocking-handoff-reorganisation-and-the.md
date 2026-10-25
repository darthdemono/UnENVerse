# Phase log 20: 2026-08-25 - CI unblocking, handoff reorganisation, and the 1.0 plan

## Goal

Three things, none of which were the plan at the start.

1. Unblock the release pipeline, which had failed twice.
2. Organise the handoff files, which turned into salvaging a 730-line rolling log
   and renumbering every session file.
3. Produce a release plan from 0.7.1 to 1.0.0. This one _was_ the ask, and it
   arrived twice - the second time with two new requirements attached.

## Current Progress

### CI - two separate failures, two fixes

**Perl.** `openssl-src` builds OpenSSL from source and its `Configure` is a Perl
program needing `Locale::Maketext::Simple`. Git for Windows' MSYS perl lacks it.
The Phase 16 fix put Strawberry Perl on `GITHUB_PATH` and asserted the module
loads - and it still failed, because `shell: bash` is Git bash and its msys
runtime injects its own `/usr/bin` when converting the Windows PATH.

Fixed by not depending on PATH order at all. `openssl-src` reads
`OPENSSL_SRC_PERL` before falling back (verified at
`~/.cargo/registry/src/*/openssl-src-300.6.1+3.6.3/src/lib.rs:157-158`), so the
workflows now name the interpreter outright:

```
Add-Content -Path $env:GITHUB_ENV -Value 'OPENSSL_SRC_PERL=C:\Strawberry\perl\bin\perl.exe'
```

The verify step checks `${OPENSSL_SRC_PERL:-perl}` rather than whatever the shell
resolves - the old guard could pass while the build ran a different binary.
Applied at three sites: `build.yml` desktop + headless, `ci.yml`.

**Artifact download.** `actions/download-artifact@v4` failed with
`Not a valid zip file` after its own five retries - twice, so not transient. Its
streaming unzip truncates on the larger artifacts (desktop staging is AppImage +
deb + rpm + NSIS). Replaced with `gh run download`, one artifact at a time, three
attempts each.

**The load-bearing detail:** the `release` job declares an explicit `permissions`
block, so everything unlisted is `none`. `gh run download` needs `actions: read`
added or it 403s. Nothing about the symptom would point at this.

### Handoff reorganisation

`CLAUDE.original.md` called itself a "Rolling Handoff Log" and was one: 730 lines,
15 dated session entries, ~74% of the file. Split into `the session records/`:

- 11 entries covering Phases 1–6 existed nowhere else → `01-phases-1-3-pre-2026-06-05.md`…`11.md`
- 4 entries retold sessions that already had files → appended to those files
  under _Also recorded in the rolling log_, rather than duplicating a number
- Old `phase log 1..8` → `phase log 12..19`, chronological, zero-padded
- 14 cross-references remapped across `CLAUDE.md` and 3 handoffs

Then, on request, `CLAUDE.original.md` was merged into `CLAUDE.md` and deleted.
Its Pitfalls Reference, Consolidated Bug Fix History and Key Decisions Log are now
sections of this file. **The three-document contract is two documents.**

### The 1.0 plan

Published as an artifact:
https://claude.ai/code/artifact/fd37ff58-8060-4eaf-a1bb-3682e39cc2a1

Four milestones, 32 tasks, ~15 weeks at two developers. Staged into the phase
table above as Phases 17–20.

## The bugs that mattered

Ranked by what they did, not by which line was wrong.

1. **The CLI has no TLS configuration at all.** Three bare
   `reqwest::blocking::Client::new()` calls (`access.rs:133,179,193`) and, worse
   than the README's "cannot pin": the CLI links a _different TLS stack_ from the
   desktop app. `reqwest` default features means native-tls; `src-tauri` uses
   rustls. One product, two trust decisions.
2. **`doctor --repair` for a missing salt cannot exist.** Asked for twice. The
   salt is 16 CSPRNG bytes written once, derived from nothing, duplicated
   nowhere. A flag that appears to offer recovery is worse than no flag, because
   someone will rely on it and discover the truth at restore time.
3. **The Settings overflow is the label, not the toggle.**
   `src/css/settings.css:143`, `.settings-label { flex-shrink: 0 }`. Right for
   every other row; wrong here because this row nests a ~60-word
   `.settings-hint` _inside_ the label (`index.html:1074-1083`). An unshrinkable
   flex item sizes to its content, so the row demands that paragraph on one line.
   The fix needs `min-width: 0` as well as `flex-shrink` - a flex item's
   automatic minimum size is `min-content`.
4. **Generation already runs on two different RNGs.** `gen.rs:12,75` uses
   `thread_rng()`; `generators.rs:34` uses `OsRng`. Both sound, but there is no
   single seam to attach an entropy source to - building one is most of the TRNG
   work, and the brief assumed it existed.
5. **TOTP is entirely absent.** `grep -rl totp` over `vault-core/`,
   `envv-server/`, `src-tauri/` and `src/ts/` returns nothing. It was removed in
   Phase 7. Two briefs described it as needing to be done "properly", which would
   have under-budgeted it by a milestone.

## Decisions made

- **Name the Perl interpreter rather than order the PATH.** Three suggestions
  offered by an external tool all shared a false premise (that `GITHUB_PATH`
  written from pwsh does not reach bash - it does, it is runner-level). Two of
  the three would have made it worse.
- **`gh run download` over `download-artifact`.** Different extract path, names
  the failing artifact, and preserves the one-directory-per-artifact layout the
  assemble step depends on. Do not add `merge-multiple: true`; it flattens them.
- **Fold, do not duplicate.** Four rolling-log entries retold existing sessions;
  each was appended to the file it retells rather than given its own number.
- **Handoffs 12–19 keep their references to `CLAUDE.original.md`.** They are
  dated records of sessions where it existed. `INDEX.md` carries the warning.
- **Entropy is always mixed, never raw.** `HKDF-SHA256(os ‖ device)`. A device
  can be absent, wedged, counterfeit or backdoored; mixing means the worst case
  is no better than `OsRng` rather than catastrophically worse.
- **Absence fails closed.** Selecting a token that is not present exits 7. Silent
  fallback means the user believes they used hardware and did not.
- **Salts stay on the OS RNG in 1.0.** A device-dependent vault salt turns a lost
  YubiKey into a lost vault.
- **An exporter that fails its live check stays experimental and 1.0 ships.**
  "Every stable type is proven" is defensible; "every type is stable" is not.
- **Phases 17–20 group deliverables; they are not a serial order.** A break from
  1–16, documented at the table because it changes how the numbers read.

## What Worked

- Auditing before planning. Five premises in the brief were wrong, and one of
  them (`--repair`) would have shipped a feature that cannot work.
- Checking the dependency's own source for `OPENSSL_SRC_PERL` instead of
  theorising about PATH. The fix holds whichever PATH theory is right.
- `git rev-list --all --objects | grep -i handoff` - settled in one command that
  no handoff had ever been committed, and that the "big file in the git commits"
  was `CLAUDE.original.md`.

## What Didn't Work

- The first CI fix (Phase 16, PATH ordering) was correct reasoning about the
  wrong layer and passed review twice before failing in the real runner.
- Re-running the release job after the first artifact failure. Reasonable once;
  the second failure is what made it a bug rather than weather.

## Next Steps

1. **Watch the next release run.** Both fixes are pushed but neither has been
   observed succeeding. The Perl fix is untested in anger - I could not verify
   the original failure either, since `gh` is not installed on this machine.
2. **Install `gh`**, or the next CI investigation is again done blind.
3. **Human decisions the plan cannot make:**
   - Locking the local vault on switch to remote (task 1.4) changes behaviour -
     switching back costs a master password. Plan recommends shipping it with an
     opt-in toggle, default off.
   - The `envv` binary-name collision with envvault.dev. Rename or claim, but
     decide before 1.0 or it becomes a breaking change.
   - Whether 237 lint suppressions ship in a 1.0.
4. **Do the Settings CSS fix now.** It is three declarations, has no
   dependencies, and is diagnosed to the line.
5. `.gitignore` has uncommitted changes (`the session records/` added, dead `*.original.md`
   rule removed).

## Notes for whoever picks this up

- `CLAUDE.original.md` no longer exists. Its pitfalls and decisions are sections
  of `CLAUDE.md`. If a doc tells you to read it, that doc is stale.
- The next handoff is `21-2026-08-25-phases-17-and-18-implemented-ci-unblocked-twi.md`.
- The 1.0 plan is an artifact, not a file in this repo. It is grounded in a source
  audit dated 2026-08-25 with file:line references - but it is a plan, and this
  project's own doctrine is that the roadmap is the part of a document most likely
  to describe something nobody kept. Check the code first.
- `CLAUDE.md` still carries session logs it should not (Phase History, the
  Post-Phase-6 section). That is the last piece of the document contract.
