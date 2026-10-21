# Dependency modernisation plan

**Policy.** Dependencies move to the latest release unless the newer version is
actually worse for the project: it does not build, it drops a feature or platform
the product ships, it changes an on-disk format or derived key, or something the
project must run (the linter, the test runner, Tauri) does not support it yet.
Every exception is listed below with what would end it. Caret requirements plus
the committed `Cargo.lock` hold the floor; exact `=` pins exist only where an
upgrade changes stored data (`rusqlite` and SQLCipher, `argon2`), each guarded by
a golden test. Behaviour-preserving refactors, features, schema migrations and
dependency upgrades go in separate commits.

## How a round runs

1. `cargo update` (compatible versions) and `npm update`, then the full test gates.
2. `cargo update --dry-run -v` lists what is behind a major version, and
   `npm outdated` does the same for the interface. Each major is tried in a
   branch of its own.
3. A major that builds and passes the golden and contract tests is taken. One that
   does not is recorded here with the reason and a date to look again.
4. cargo-deny, OSV and the other scanners in the [security workflow](security.md)
   run on the result.

## Where it stands (22 October 2026)

| Dependency                                                                  | Current   | Latest         | Decision   | Why, and what ends it                                                                                                                                                                                                                                                    |
| --------------------------------------------------------------------------- | --------- | -------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust lockfile (46 crates)                                                   | refreshed |                | Done       | Compatible updates taken; the core and application test suites pass.                                                                                                                                                                                                     |
| `keepass`                                                                   | 0.17      | 0.17           | Done       | Upgraded from 0.15; the KeePass migration and contract tests pass.                                                                                                                                                                                                       |
| `age`                                                                       | 0.11      | 0.12           | Wait       | 0.12 does not build against the current `ml-kem` and `kem` crates (the `Encapsulate` and `Decapsulate` traits no longer match). Recheck when `ml-kem` and `age` publish a matching pair. Moving also clears the unmaintained `basic-toml` advisory through `i18n-embed`. |
| `gtk` and the `toml` 0.8.2 it pulls in                                      | 0.18      | 0.19           | Wait       | Owned by Tauri's GTK3 stack, which fixes the version. Ends when Tauri moves to gtk-rs 0.19 or later.                                                                                                                                                                     |
| `matchit`, `brotli-decompressor`, `generic-array`                           |           | patch releases | Wait       | Transitive through axum, Tauri and the crypto crates; they move with their owners.                                                                                                                                                                                       |
| TypeScript                                                                  | 6.0       | 7.0            | Wait       | The type-aware lint gate uses typescript-eslint, which declares support for TypeScript below 6.1. Ends when typescript-eslint supports 7.                                                                                                                                |
| Vite                                                                        | 7.3       | 8.3            | Try next   | Vitest 5 already supports Vite 8. Taken if the frontend build, the Tauri bundle and the Playwright layout lab are unchanged.                                                                                                                                             |
| Patch releases of eslint, vitest, prettier, jsdom, Playwright and Tauri CLI |           | latest patch   | Next round | Taken together once the registry is reachable from the build machine.                                                                                                                                                                                                    |

## Rules the plan keeps

- A dependency that is unmaintained or has an advisory with no fix is listed in
  `deny.toml` with the reason it is harmless and what ends the exception, never
  silently.
- A new dependency needs a reason a few lines of code cannot answer.
- Format-sensitive crates (SQLCipher, Argon2, age, the backup format) are upgraded
  only with the golden fixtures green, and the old vault fixture opened.
