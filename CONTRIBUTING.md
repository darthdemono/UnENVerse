# Contributing

Thanks for helping. UnENVerse is a local-first secrets manager, so the bar for
changes that touch storage, crypto, redaction or the server is high. Small fixes,
tests, docs and new importers or exporters are very welcome.

## Before you start

- Open an issue first for anything bigger than a bug fix, so the design is agreed
  before the work is done.
- **Security problems are not issues.** Report them privately, as described in
  [SECURITY.md](SECURITY.md).
- By contributing you agree your work is licensed under Apache-2.0, the licence of
  this repository, and you certify the
  [Developer Certificate of Origin 1.1](https://developercertificate.org/): you wrote
  the change or have the right to submit it under that licence. The pull request
  template carries this as a checkbox you tick; adding `Signed-off-by` to your
  commits (`git commit -s`) is welcome but not required. Be kind: see the
  [Code of Conduct](CODE_OF_CONDUCT.md).

## Good first issues

Small tasks for new or casual contributors carry the
[`good first issue`](https://github.com/darthdemono/UnENVerse/labels/good%20first%20issue)
label. They are scoped to one file or one command, say what the expected behaviour
is, and name the test to extend. Typical ones: a documentation fix, a missing test
for a documented behaviour, a clearer error message, a new importer fixture. Comment
on the issue before you start so two people do not do the same work.

## Reporting a bug

Open an issue with the bug form. Say which version (`unv --version`), where it
happens (desktop app, `unv`, `unv-server`), your operating system, the steps, and
what you expected. Do not paste secrets, vault files or `--reveal` output. For a
security problem do not open an issue; see [SECURITY.md](SECURITY.md).

## Build and run

Rust (stable) and Node 22 are required. On Linux the app also needs WebKitGTK,
GTK 3, an appindicator library, librsvg, patchelf, SQLCipher and mold; the exact
package names for Fedora and Debian/Ubuntu are under "Building from source" in
the [README](README.md#building-from-source). See also the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm ci
npm run dev            # UI only, in the browser
cargo tauri dev        # the full desktop window
```

## The checks a pull request must pass

```bash
npm run preflight      # what CI runs: fmt, clippy, cargo test, npm run check, vite build
```

It is slow the first time. While iterating, `npm run check` (TypeScript side) and
`cargo test -p <crate>` (Rust side) are quicker. `npm run format:rust` and
`npm run format:check` fix and check formatting.

## What a good change looks like

- **A bug fix comes with a regression test** whose comment says what the bug did.
  Revert the fix and confirm the test fails.
- **UI and CLI stay in step.** Whatever the desktop app can do, the `unv` CLI can
  do, and the reverse. The exceptions are listed in `tests/fixtures/parity/capabilities.json`.
- **Never identify an entry by array index**, and treat vault data as untrusted
  input (escape every field). The full list is under "Rules every change must
  respect" below.
- Keep refactors, features, schema migrations and dependency upgrades in separate
  commits.
- A user-visible change adds a line to `CHANGELOG.md` under Unreleased, and notes the
  security effect when there is one.
- Test data must not look like real credentials (GitHub push protection will
  reject the push).
- Do not run a formatter over `index.html` or `tests/fixtures/`.

## How review works

Every change reaches `main` through a pull request and the `CI result` check
(ruleset in [GOVERNANCE.md](GOVERNANCE.md)). The reviewer, who is never only the
author when a second person is available, checks:

1. **Behaviour.** The change does what the issue or the README says, and a bug fix
   has a regression test that fails without the fix.
2. **Invariants.** Nothing breaks the rules under "Rules every change must
   respect" below.
3. **Security.** Redaction stays fail-closed, no secret reaches a log, an error or a
   test fixture, and a new route or command has a permission check and a test. Read
   [THREAT_MODEL.md](THREAT_MODEL.md) when the change adds an input.
4. **Scope.** Refactors, features, migrations and dependency upgrades are separate
   changes, and the diff has no unrelated edits.
5. **Checks.** `npm run preflight` is green, coverage did not fall, and a
   user-visible change has a `CHANGELOG.md` line.

A change is acceptable when all five hold. The reviewer approves or says what is
missing; unresolved comments block the merge. While the project has one maintainer,
the maintainer reviews outside contributions and checks their own against the same
list before merging.

## Rules every change must respect

1. **Never identify an entry by array index.** Use `entryId(entry)`; resolve
   `data-idx` at once and never hold it across an `await` or user interaction.
2. **Renames and deletes cascade.** Projects and categories nest by `/`; remap
   `projectIds` before the dangling-reference prune. A provider rename also renames
   the references in chunk fields.
3. **Replacing the vault resets view state** in both directions of a vault switch
   and repaints. Locking also clears the undo stack.
4. **Vault data is untrusted input.** Escape every field with the `html` tagged
   template; only `http(s):` URLs become links.
5. **Copy and export resolve references** with the shared resolver, never by
   matching `${...}` by hand (Docker Compose output keeps `${VAR}` on purpose).
6. **Encoding.** Use `TextEncoder` before base64 for UTF-8, and never spread a large
   byte array into `String.fromCharCode`.
7. **Persisted ids are validated on read**, and a feature flag that hides UI gates
   creation only.
8. **A hidden control is not a gate.** Enforce a rule at the write as well as at the
   paint, and prove each gate by reverting it alone.
9. **Handlers on re-shown screens are assigned** (`el.onclick =`), not added.
10. **Capability parity.** What the UI can do the CLI can do and the reverse, with
    exemptions in `capabilities.json`. Prefer one Rust implementation reached over
    IPC; when logic must exist twice, pin it with one golden fixture asserted from
    both sides.

Security rules that are never relaxed: redaction is fail-closed (a field prints only
if listed public), there is one list of secret fields, `--dry-run` is enforced inside
the save path, secrets are compared in constant time, a certificate is never silently
re-pinned, and a hash chain anchors its first row.

## Dependencies

Dependencies are chosen from crates.io and npm, with caret requirements and the
committed `Cargo.lock` and `package-lock.json` so builds are repeatable. Prefer a
dependency that is maintained and already in the tree over a new one. Exact `=`
pins are used only where an upgrade would change an on-disk format or a derived
key (`rusqlite`/SQLCipher, `argon2`), each guarded by a golden test. Dependabot
proposes updates weekly; `cargo deny` and `npm audit` run in CI. The policy for
vulnerable dependencies is in [SECURITY.md](SECURITY.md).

## Commits and pull requests

One sentence per commit message, in the imperative, saying what changed
("Stop the bundle card collapsing under a filter"). Fill in the pull request
template; a short description of how you checked the change is enough.
