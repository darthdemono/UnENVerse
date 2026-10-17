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
  this repository. Be kind: see the [Code of Conduct](CODE_OF_CONDUCT.md).

## Build and run

Rust (stable) and Node 22 are required. On Linux, Tauri needs its system
libraries; see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

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
  input (escape every field). The full list of rules every change must respect is
  under "Invariants" and "Working rules" in [AGENTS.md](AGENTS.md).
- Keep refactors, features, schema migrations and dependency upgrades in separate
  commits.
- Test data must not look like real credentials (GitHub push protection will
  reject the push).
- Do not run a formatter over `index.html` or `tests/fixtures/`.

## Commits and pull requests

One sentence per commit message, in the imperative, saying what changed
("Stop the bundle card collapsing under a filter"). Fill in the pull request
template; a short description of how you checked the change is enough.
