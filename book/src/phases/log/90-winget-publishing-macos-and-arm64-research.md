# Phase log 90: winget publishing, macOS and arm64 research

Date: 2026-10-20/21. Nothing in the repo changed. Work lives in the winget-pkgs fork, local manifests and two gitignored research reports.

## Goal

Map macOS support and publishing channels (winget, Scoop, Homebrew, apt, dnf/COPR, Flathub, Snap, AUR, nixpkgs, crates.io), plan arm64 for Linux and Windows, then publish to winget: `DarthDemono.UnENVerse` (app), `DarthDemono.Unv` (CLI), `DarthDemono.UnvServer`, plus all PingDD versions. Nothing built for macOS or arm64.

## Current Progress

- Research: `research/reports/05_MACOS_AND_PACKAGE_CHANNELS.md` (macOS task ranking, channels, winget rules, submission log, section 7 on how PRs work) and `research/reports/06_ARM64_LINUX_WINDOWS.md` (9-step plan, risks). Both gitignored.
- Fork clone: `~/Code/winget-pkgs` (shallow, sparse; `origin` = darthdemono fork, `upstream` = microsoft). Local `master` clean, one branch per PR.
- 12 PRs open on microsoft/winget-pkgs, checked 2026-10-20 09:33 UTC, no merges, no moderator replies:
  - #455583 UnENVerse, #455584 Unv, #455585 UnvServer (all 0.56.1, New package): `Validation-Completed`, waiting for a moderator.
  - #455586 PingDD 2.1.2: `Validation-Executable-Error` (exe exits 1 with no args). Commented asking the validation to run `pingdd.exe --version`.
  - #455636, #455638, #455640 to #455644, #455646 (PingDD 1.0.0-18, 1.0.1 to 1.0.4, 1.0.6 to 1.0.8): `Validation-Executable-Error` plus `Manifest-Metadata-Consistency`. Commented asking for `--help`. Remaining consistency lines (`NestedInstallerType`, `arm64`) cannot be fixed for old bare-exe releases.
- Did not run: any local `winget validate` or install (no Windows machine); the pipeline does it on the PR.

## The bugs that mattered

1. `InstallerType: nsis` is invalid; NSIS is `nullsoft`. Fixed in 13 manifests and the generator.
2. App installer needs `ElevationRequirement: elevationRequired`, not `elevatesSelf`.
3. `(?ms)` regex truncated the tails of 12 PingDD locale files; replaced with a line-based rewriter.
4. ": " inside a plain YAML scalar broke ShortDescription.
5. UnENVerse 0.42.0 and 0.42.1 are MIT at their tags, not Apache-2.0.

## Decisions made

- IDs by publisher prefix: `DarthDemono.UnENVerse`, `.Unv`, `.UnvServer`.
- Add every version (users can pin), skip the EnvVault era.
- Unv and UnvServer: zip with nested portable and a command alias. PingDD 1.x: bare portable exe; 2.x: zip x86 plus arm64.
- Commit rule for the fork, this session only: one sentence, no trailer, no session line, no AI wording, groups not files, never `git add .`. The permission ended with the session.
- Decision to want arm64 Linux and Windows and later macOS; roadmap rule 7 forbids claiming them before built, tested and documented.

## What Worked / What Didn't Work

- Worked: asset `digest` field for hashes; offline schema check with python `jsonschema` against `https://aka.ms/winget-manifest.<type>.1.10.0.schema.json`; one branch, commit, PR per version.
- Not worked: `pkill -f` killed my own shell; shallow full clone was slow (sparse `--depth 1 --filter=blob:none` fixed it); `--force-with-lease` rejected, plain `--force` on my own branches was fine.
- Open: PingDD 1.1.2 passed in January with the same no-arg exit code, so the executable error may be a stricter validator. No evidence.

## Next Steps

1. Re-check the 12 PRs (`gh pr list --repo microsoft/winget-pkgs --author darthdemono`). Typical merge wait is hours to about 4 days, some over a week. If the three new packages are still open after about 4 days, leave one polite review request comment.
2. After a moderator answers the executable question: submit PingDD 1.0.9, 1.0.10, 1.0.11, 1.1.0, 1.1.1, 2.0.0, 2.1.0, 2.1.1. 2.x need a `--version` comment, 1.x a `--help` comment. Check 1.1.1 tags first.
3. Winget CI (when asked): job in `.github/workflows/build.yml` after the publish job using winget-releaser or wingetcreate. Needs a classic PAT secret `WINGET_TOKEN` (`public_repo`, `workflow`; only the maintainer can create it), the fork, an existing package, `installers-regex: '\.exe$'`. The release publishes with `GITHUB_TOKEN`, so a separate `on: release` workflow will not fire. Run the new job by hand once first.
4. arm64 Linux and Windows: follow `06_ARM64_LINUX_WINDOWS.md`. macOS: `05_...md`; vendored SQLCipher needed (`vault-core` vendors only on Windows).

## Notes for whoever picks this up

- Winget rules: one package version per PR; title `New package: <Id> version <v>` or `New version: ...`; commit `<Id> version <v>`; branch `<Id>-<v>-<uuid>`. PR checklist lines for validate and install say the pipeline did them.
- Bot compares each PR to the highest published PingDD version (1.1.2) for metadata consistency.

> 3 line(s) about the maintainer's own machines were left out of this page.
