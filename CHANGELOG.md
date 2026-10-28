# Changelog

All notable changes to UnENVerse, newest first, compiled from the development session notes. Before the rename to UnENVerse (2026-10-09) the product was called EnvVault, the command `envv` and the server `envv-server`.

## Unreleased

### Fixed

- `unv get X_COOKIE` selects an entry by its generated variable name, as the 0.61.2 notes promised.
- `unv user strict-write` works against a server (owner only) instead of exiting 7.
- `unv backup recovery-key` asks before replacing an existing recovery identity (exit 8 without `--yes` and a terminal).

### Changed

- `unv import FILE --json` (full-vault JSON export) is now `--vault-json`, so the global `--json` envelope flag can be combined with importing a `.env`.

## 0.61.2 (2026-10-28)

### Added

- An entry is selected by its generated variable name: `unv get X_COOKIE` and `unv exec --entry X_COOKIE` need no key id when two entries of one provider differ by role.
- `unv entry add` accepts a second entry for a provider when its generated primary variable differs (a key and a cookie for one site), where it used to demand `--key-id`.
- Key pools are their own heading under "Group by secret type", and the order of the type headings can be changed in Settings.
- Settings has a "Created time shown to" choice: the day, the minute or the second, used by the Timeline.
- The website has an LLM support page, and the README gains a section on working with LLMs and AI agents, listing every agent-facing feature.

### Changed

- Key pool cards, the Key Pools sidebar rows and the Key Pools pane use the bundle card styling.
- One UI font and one mono font are used everywhere, and form controls inherit the page font.
- The website's left rail shows an icon with a label, like the app's activity bar.

## 0.61.1 (2026-10-26)

### Changed

- Release notes list each commit as a linked short hash with its linked pull request number, in place of the author name.
- Dependency updates: the GitHub Actions group (`taiki-e/install-action`, CodeQL), the Rust 1.98 build images, and the npm minor and patch group (Playwright, Tauri CLI, Vitest coverage, ESLint, jsdom, Prettier, simple-icons, typescript-eslint).

## 0.61.0 (2026-10-25)

### Added

- macOS builds: an unsigned `.dmg` for Apple silicon and Intel, and `unv` and `unv-server` archives for both, built and published by the release job and listed in the signed CLI update feed.
- `unv node install` runs on macOS: a role account, access lists, and launchd jobs for the agent and for reloading a service (ADR-0170).
- Shortcut labels show the Command key on a Mac.
- The Caps Lock hint works on macOS, and `unv update` hands an Intel Homebrew install (`/usr/local/Cellar`) to `brew upgrade unv`.
- `unv migrate leaks <dir>` runs your own gitleaks over a folder and offers what it finds as migration candidates; nothing is saved without `--apply` and no value is printed (ADR-0171).
- `unv acme issue|renew|check` gets a certificate through your own lego, keeps it as a certificate entry with the dates the CA signed, renews only inside the window, and compares the certificate a host serves with the entry (ADR-0171).

## 0.60.2 (2026-10-25)

### Added

- The env naming schema is written down as a scale from one key to an organisation (ADR-0169): prefix, provider, key id, version, label, role, and the account as a tie-breaker.
- Every secret with a primary value has a value role, and the form and `unv entry add` start a new one on it: `key` for an API key, `password` for a password, `value` for an environment variable, the type name for the rest. A new entry cannot be saved without one.
- A role is unique: one entry cannot generate the same name twice (two `secret` values, or a primary role `id` beside an additional value `ID`), and a second entry for the same provider and role needs a key id, version, label, account or key pool to tell it apart.
- When a copy holds entries with the same name and each has its own account, the account is appended: `ELTE_PASSWORD_DARTHDEMONO_GMAIL_COM`.
- A password generates its sign-in name first, `ELTE_USER` before `ELTE_PASSWORD`, in copies, exports and the preview. The card shows the username above the password and names the variable on hover.
- Fields are labelled and offered per secret type: a browser session has a Site URL, a password a Login URL, a database a Server URL, and so on. Callback URL, scopes and rate limit appear only where the type uses them, and always while they hold a value.
- Sort by date added and by expiry, each newest or oldest first and soonest or latest first, with undated entries last in both directions.

### Fixed

- A category filter and a type filter did not narrow together: with a category selected, the type chips were ignored and every card of the category stayed.
- The bundle header's copy button copied the primary member's value alone. It now copies the combined environment of every member and variable.
- **Group by secret type** did nothing in a vault that held a bundle, and ignored key pools; bundles, pools and the grouping now compose, with a Bundles heading of their own.
- Search read only the provider, account, key id and descriptions. It now also reads the username, email, name label, version, pool, purpose, app name, app purpose and URL (never a secret).

## 0.60.1 (2026-10-25)

### Added

- Additional values on a secret: a dropdown of roles (secret, id, token, password, username, url, or a custom one) and a **+ Add value** button next to the value field. Each addition is stored as an extra variable and shown in the generated variable names.
- App name and app purpose on a secret, separate from Purpose (what you told the issuer). Also `--app-name` and `--app-purpose` on `unv entry add` and `unv entry set`, in `unv diff`, and in the full copy profile.
- **Add key** and **Add env** on a bundle create a new entry that joins the bundle as a slot, instead of only adopting an existing entry by name.
- The site is laid out like a docs site, with new Install, Comparison, Support, Get help, Administration and Developer pages.

### Changed

- The Add and Edit form is split into sections (Secret, How it is used, Two-factor, About this secret, Organise, Lifecycle).
- An API key's default value role is `key`, so a new entry generates `GITHUB_KEY` rather than `GITHUB_API_KEY`. Existing entries keep their role.
- API Secret and Secret role are hidden on new entries and stay visible on an entry that already holds a secret.

### Fixed

- A named variable with an empty value was counted as the entry's payload, so a bundle or environment variable entry could be saved holding a blank secret. New empty rows are dropped, and a name with no value no longer makes the primary value optional.
- The Generated variable preview showed only the primary and the secret. It lists every name the entry generates and updates as rows change.
- The desktop window did not notice a change made from a terminal while another window covered it, and ignored the first outside write to a vault that was empty when it opened. A vault served by `unv-server` is now watched too, with a conditional request that the server answers with 304 while nothing changed.

## 0.60.0 (2026-10-22)

### Security

An external source-code audit of 0.58.0 found two high and several medium issues. All of them, and several low ones, are fixed here, each with a regression test.

- A node grant names a project, and a user with any write permission could add a project whose id equalled that name, so the hub rendered the attacker's project for the node. The hub now resolves a grant to exactly one project and refuses an ambiguous one.
- A `manage-users` delegate with a narrow data scope could reset the password of, mint a token for, or reset the second factor of a lower-tier user who could read more, and could widen a subordinate by clearing their individual restriction or changing their class. Acting on an account now also requires the actor's own scope to cover that account's, and a change that would leave the target wider than the actor is undone.
- Restoring a recovery package wrote members named `../x` or an absolute path outside its staging directory. Member names must now be plain relative paths.
- A permission expression of a few thousand nested parentheses crashed the server with a stack overflow. Expressions are limited to 256 tokens.
- `unv exec` and `unv shield` passed `UNV_PASSWORD`, the master password, to the child process unless `--clean` was given. The credential variables are now removed from what a child inherits.
- `unv backup restore-archive --force` replaced the vault even when you answered no.
- The container compose file and the documented `docker run` published the plain-HTTP port on every interface. They now bind to 127.0.0.1 and say how to serve other machines over TLS.
- The generated TLS private key and credential files written by `unv emit`, `unv cxf export`, `unv user token new --out`, the session file, feed URLs, migration archives and archive restores were created with the default mode and tightened afterwards. They are now created owner-only.
- The server minted a new salt for an existing vault whose salt file was missing, for any caller of `/api/unlock`. It now reports the missing salt. An interrupted password change is recovered before the salt is read.
- Calendar feed tokens were written to the log when a fetch was rejected; named users could be given an empty password (twelve characters are now required).

Not changed, by decision: the unauthenticated `/api/stats` counters, the first caller initialising an empty server, display fingerprints of weak secrets, and key material lingering in memory after lock. They are recorded for a later release.

## 0.59.0 (2026-10-22)

### Added

- A command reference page in the book, written from `unv describe`: a test fails when a command is added, renamed or re-described until the page is regenerated, so the page cannot drift from the code.
- A guide, "Use UnENVerse with Docker Compose secrets": write a vault value as an owner-only file without a trailing newline, mount it with `secrets: file:` and a `*_FILE` variable, validate with `docker compose config -q`, and recreate only the service that reads it. A test runs the guide's commands and, where Docker is installed, the Compose validation.
- `unv support bundle [--out FILE]` writes a small JSON file for a bug report: versions, operating system, SQLCipher version, schema version, the number of entries, project types by count, the doctor check ids with their level, and the last 50 operation outcomes by kind and state. It holds no value, entry, project or node name, path or host. `unv demo-vault --out DIR` writes a synthetic vault with invented values (its password is in a file beside it) for reproductions. `SUPPORT.md` states the response policy and the bug template asks for the bundle.
- `unv entry forget NAME` purges an entry's old values from this vault: its version history and the stored content of every config-history snapshot that held it. The snapshot stays on the timeline with its hash, so the history chain still verifies; the current value is untouched, a running rotation refuses it, and an audit row records the count, never a value. It prints what it cannot reach (backups, nodes, the import source, disk blocks). The delete prompt now says the credential keeps working at its provider.
- `unv vault rekey` changes the master password: the vault is re-encrypted into a new file under a new salt, checked row by row, and swapped in; a crash at any step leaves a vault that opens with the old or the new password, and the next unlock finishes or undoes an interrupted change. The old files are kept until the next successful unlock. Backups made before the change still open with the old password; recovery packages are not affected (ADR-0168). New book pages: "If something leaks" and "Formats we read".

### Security

- Encrypted backups: the key-derivation settings in a backup file are now validated before use, in the command line and in the desktop app. Only PBKDF2 with SHA-256 and a whole-number iteration count from 100,000 to 5,000,000 are accepted. Before, a hostile file could name a huge count and hang the import, a tiny one to weaken the key, or a count that wrapped around; `unv backup import` and archive restore now refuse such files with a clear message and change nothing.

## 0.58.0 (2026-10-22)

### Added

- Import from Git stores through your own tools: `unv import-vault sops`, `pass` and `gopass`. `sops decrypt`, `gpg --decrypt` and `gopass show` run with a cleared environment, a deadline, an output cap and scrubbed error text; a SOPS file becomes an entry of dotted key-path variables, a `pass` or gopass secret keeps its first line as the password and its whole body as the description, and SOPS metadata is archived as non-operational provenance.
- `unv import-service` imports from OpenBao (KV v2, paged, with every live version as history), Infisical (expanded values and a recorded reference graph), Bitwarden Secrets Manager (through your own `bws`) and, in a build with `--features aws`, AWS Secrets Manager and Parameter Store. The credential comes from the environment or a vault entry, the whole result is built before anything is written, and policies, auth methods and dynamic engines are archived or reported, not imported.
- Nodes: a `validate` command that contains `{file}` checks a temporary file and replaces the target only if it passes; a `check` (a command or an `http://` URL that must answer 2xx) runs after `reload` and a failure puts the previous file back and reloads it; each apply reports the steps it completed. Approval tokens have a second version (kind, operation, policy revision, its own signing domain), and a target with `accept_rule_approval = true` takes a short-lived rule approval signed by the hub's automation key (`unv node automation-key`). A rule approval never satisfies `require_approval`.
- After a Git-store import the source stays authoritative: editing a secret field of such an entry is refused with a message naming the source (the desktop app, the command line and the server share the rule), `unv migrate sync` refreshes the vault one way, `unv migrate drift` reports what changed at the source without importing it, and `unv migrate cutover` and `uncutover` hand authority to the vault and back, refusing to go back once a credential has changed.
- `unv rotate plan|run|status` rotates a Cloudflare API token: a new token with the same policies is created and verified, stored in the entry as the next generation, and the old one is deleted last (`--revoke-now`). A response lost after a create is found again by the token's name; a lost in-place roll (`--allow-roll`) is never retried. The rotation engine behind it is issuer-independent and is tested against every combination of step failures (ADR-0164). `--issuer postgres` rotates between two database roles: the new password is sent as a SCRAM-SHA-256 verifier, never as plaintext, and the old role is cleared last.
- Plugins (`unv plugin ls|install|enable|disable|rm|test`): external programs that speak `unv-plugin-rpc/1` over stdin and stdout, run with a cleared environment, deadlines, output caps and scrubbed stderr, and are refused if their program changed since install. `unv plugin test` runs a conformance suite (handshake, no side effects in plan, idempotent execute, reconcile, shutdown deadline, no secrets on stderr); `examples/plugins/ntfy-notifier` passes it and faulty copies fail the matching check (ADR-0165).
- A command palette (Ctrl+Shift+P) for navigation and the top actions; Info findings in the health scan fold behind a count; the price badge on a card is quiet (hidden for free, dimmed otherwise until the card is hovered).
- Windows: the vault folder, database, salt, recovery files and scratch directories get an owner-only access list, `unv doctor` checks it, and the device approval key lives in the machine-local profile (an old roaming key is adopted, never deleted). A rename over a file another program holds open is retried briefly. `unv doctor` warns when the vault is on a network filesystem. macOS builds with vendored SQLCipher and is tested in an informational CI job (ADR-0166).
- Vault identity: every vault has an id and every copy an instance, an epoch and a role. A restored vault starts as standby (it reads and verifies but does not push, run schedules or rotate); `unv vault status` shows it and `unv vault activate` makes it live. Hubs send `(vault id, epoch)` to nodes, and a node refuses a hub with a lower epoch or for another vault. Upgrade nodes before the hub. `unv doctor` warns about a vault inside a cloud-sync folder (ADR-0167).

### Contract

- `unv vault status|activate` are new. `unv describe` carries `contract_version` (the old `contract` key stays). A committed snapshot of every command, positional and flag name and every exit code fails a test when one is removed or renamed. The scripting contract is written down in the book with recipes that a test runs (`book/src/contract.md`).

### Fixed

- The catalogue signing key was replaced (the old private half was lost); builds from this one pin the new public key, older builds keep using the catalogue built into the binary. The signed provider catalogue is fetched from `https://unenverse.darthdemono.com/catalogue/catalogue.json`; the old `github.io/EnvVault` address answers 404.
- A node's `validate` or `reload` command that printed more than a pipe buffer held (about 64 KB) blocked on its own output and was reported as a timeout. Both pipes are now drained while it runs.

## 0.57.0 (2026-10-22)

### Added

- Release builds for arm64: Linux desktop bundles and `unv`/`unv-server` for `linux-aarch64`, the desktop installer and `unv`/`unv-server` for `windows-aarch64`, and a multi-arch `unv-server` image. Static musl archives (`linux-x86_64-musl`, `linux-aarch64-musl`) run on Alpine and any other distribution. `unv update` picks the musl archive on a musl build.
- An AMOLED theme (true black page, one step lighter surfaces), in Settings, Appearance.
- Put entries on a real project from a card's `⋯` menu (Projects…) or from the bulk bar, instead of editing each one. The entry form now says when there are no projects yet.
- Nested projects and categories in the sidebar fold shut with a caret, so a big project root can be closed. The fold is remembered.
- Site: a features page, an architecture page with a data-model diagram generated from `src/ts/types.ts` by TsUML2 (`node scripts/gen-uml.mjs`), an FAQ (why Rust, SQLCipher and Argon2id; FAQPage structured data), a changelog with a page for every release and an Atom feed, a detailed `llms.txt` and an `llms-full.txt`, and a Docker section and arm64 and static rows on the download page.
- Docs: every phase from 1 to 56 in the book, the roadmap phases done and the queue of those still to do, numbered in the same sequence, and a dependency modernisation plan.
- A security workflow with free scanners beyond CodeQL and cargo-deny: gitleaks over the whole history, zizmor on the workflows, hadolint, Trivy, OSV, Semgrep, `npm audit signatures` and dependency review.
- Roadmap: an agent-safety lane planned after 1.0.0 (ref-only project files, per-command secret release, an SSH agent, canary entries and a sandboxed exec), prompted by a published backdoored-model attack that steals plaintext `.env` files.
- Behaviour tests for user and node administration in text output, delegated administration (no one can climb above their own authority), export and `import --json` round trips, the entry flags, the scan and status reports, the doctor checks, Bitwarden and 1Password imports, authenticator imports, the credential emitters, the PHP reader, web-session capture parsers and bundle templates.
- A golden file shared by the app and the command line for the starter chunks of a new typed project, and a second exporter fixture of awkward inputs (`vault-edge.json`) that both implementations must render identically.
- The nightly branch-coverage job also uploads per-branch records.

### Changed

- The app opens on All secrets in the Secrets panel. Remembering the last filter made the first screen look random, so it is off by default and an install that had it on is switched off once.
- A card's footer keeps the copy button wide: rotate or verify, the timeline and duplicate moved into a `⋯` menu beside pin, edit and delete.
- Bundle cards are built from the same parts as entry cards (icon tile, badges, chevron, tag chips, footer) with an accent rule and one badge of their own, instead of a different component.
- A new entry's value role defaults to its type (`api_key` gives `GITHUB_API_KEY`), so two entries of one provider no longer both export as `GITHUB`. Existing entries keep their names.
- Tool panes, the users panel and the remote form use the window width instead of stopping at 440 to 700 px. Settings rows have more room, and the recovery-package controls are grouped under their own headings.
- Every page title and description on the site names UnENVerse, text blocks are wider, and each page carries structured data and microdata. The README opens with a summary, the features, installation and usage.
- Release workflows no longer leave the checkout token on disk, and every third-party action stays pinned to a commit.
- The KeePass reader is on 0.17 and the Rust lockfile moved forward by 46 crates; `age` 0.12 and TypeScript 7 wait for their ecosystems (see the dependency plan).

### Fixed

- `unv node approver ls` printed each fingerprint cut short with an ellipsis, and `unv node approver rm` refused that text. It now prints a plain prefix that `rm` accepts.
- `unv entry add --expires` accepted any text, so a mistyped date silently switched the expiry warning off. It now needs a real date such as `2030-01-31`. A malformed `--var` is reported as invalid input (exit 10).
- A Kubernetes Deployment with environment variables exported invalid YAML, because the `env:` block was indented one level too shallow, in the app and in the command line.
- Traefik middleware values were written unquoted, so a secret containing ` #` or quotes loaded back truncated. They are now quoted like router and service values.
- A Compose reference that resolves to nothing was written into the `.env` as an empty value (app) or an escaped literal (command line), so the container started with a blank secret. The placeholder is now written through, which Compose refuses loudly.
- Importing a Python config as bundle variables could mark source text it did not understand, such as `x = 'pw' + 'hunter2'`, as safe to print. Such text is never public now.
- `unv doctor` never reported pool members whose entry was deleted, because it read a pool-state layout that does not exist. It now checks the real file for the current vault.
- The PHP config reader (Nextcloud `config.php`) read `0755` as decimal, did not understand `0x1A`, `0b101` or `0o17`, and cut a float at a signed exponent such as `2.5E-2`.
- `unv import-vault onepassword` ignored the login fields and typed section values of a real 1Password export, so usernames, passwords and one-time passwords were lost. Implemented from the published `.1pux` structure; it has not been checked against a real export.
- A recovery-destination test depended on the order the filesystem lists a directory, and failed on some runners.

## 0.56.1 (2026-10-18)

### Changed

- Rust tests build the key-derivation and hashing crates optimised, so a test that unlocks a vault no longer pays for a debug-build Argon2id. A slow contract file went from 111 s to 9 s.
- CI runs the Rust tests only for the crates a change can affect, and none for a TypeScript-only change. The desktop and command-line release builds start beside the CI gate instead of after it; publishing still needs the gate.
- Rust coverage is measured on a runner (`coverage-rust.yml`) and uploaded to Codecov under the `rust` flag.

### Added

- Coverage reports from CI are uploaded to Codecov when a token is configured.
- A year roadmap to 1.0.0 on the website (`/roadmap/`), in the README and in the docs book, with the criteria for the release and what the project will not do.
- Every source file carries an SPDX licence line and a copyright line; `npm run spdx:check` (part of `npm run check`) fails when one is missing.
- `ASSURANCE_CASE.md` argues that the documented security requirements are met, and CONTRIBUTING.md now documents code review, good first issues and the Developer Certificate of Origin (a checkbox in the pull request template).
- `osv-scanner.toml` records why four dependency advisories do not apply to the shipped program, the same decisions as `deny.toml`.

## 0.56.0 (2026-10-18)

### Added

- Behaviour tests for the whole command line, the hub and node flows, the desktop IPC commands and the TypeScript interface. The command line tests run the real `unv`, a real hub and agent, a KeePass file and an SFTP server; the desktop commands are called on Tauri's mock runtime without a window.

### Fixed

- A failed restore drill reported success and exited 0; it now exits non-zero and says why.
- The desktop app panicked on any request to a server without a pinned certificate, on OAuth refresh, on online enrichment and on the catalogue update, because no TLS provider was installed.
- Deleting a class with members left a member with a broad rule of their own unrestricted; it is refused, in the command line and the desktop as it already was on the server.
- A taken user, class or node-token name showed raw database text and exited as invalid input; it is a conflict with a readable message.
- Renaming a chunk onto an existing name left two chunks that references could not tell apart.
- `enrich` proposed the same change on every run.
- Secret files written by `render --out` and the generic writers are owner-only.
- Nineteen interface defects found by the TypeScript tests, among them a start-up crash on a null stored setting, a restore without projects, and a stale edit target after a reload.

## 0.55.1 (2026-10-17)

### Added

- Public trust signals: an OpenSSF Scorecard workflow, build provenance attestations and CycloneDX software bills of materials for every release asset, a Vitest coverage floor enforced in CI, contributor guide, code of conduct, issue forms and a pull request template.
- Website: `llms.txt`, `agents.txt` and a `security.txt` file, and social, author, robots and breadcrumb tags on every page.

### Changed

- Docker base images are pinned by digest and Dependabot watches both Dockerfiles; the CodeQL token is limited to the job that uploads results.
- The `zip` crate is upgraded to 9 to clear its extraction advisories.

## 0.55.0 (2026-10-17)

### Added

- `unv update` installs a newer standalone release only after the signed manifest, the archive checksum and a version probe all pass. The manifest is signed separately from the release assets, so a compromised download host cannot push an update.
- The product website is rewritten around self-hosted use, team access and AI agents that use secrets without seeing them, and carries the importing, command line and node guides itself.

### Changed

- Bundle cards are redesigned: one Copy and Edit in the header, a chips row, and the rare actions inside the open card. A filtered bundle can be collapsed again.
- The desktop app reloads within a second when the vault changes outside it.
- The audit pane uses the full width, and the config history snapshot list is restyled.
- Release notes now open with the list of changes before the download tables.

## 0.54.1 (2026-10-16)

### Fixed

- Vaults written by SQLCipher 3 now open by migrating in place, with a backup kept first. A wrong password is still rejected.

## 0.54.0 (2026-10-15)

### Added

- R09 full-fidelity password-manager source migration: `unv import-vault bitwarden|onepassword|keepass` now plans and atomically records source identities, encrypted source archives and attachments alongside vault entries. Bitwarden JSON and ZIP attachments, 1Password JSON and `.1pux`, and KeePass KDBX3/KDBX4 password/key-file sources retain histories, TOTP where present, and report unsupported or missing-at-source material. The readers are format-derived and await sanitized real-export verification.
- R08 lossless config migration core: encrypted migration manifests, source archives and template bindings; Rust `.env`, Compose, WireGuard and nginx readers; `unv migrate plan|apply|verify|report|archive`; a desktop Rust-backed preview; and parser fuzz targets.
- R07 metadata and lifecycle core: ordered credential-generation, pending, lifecycle-event and deployment-state tables; vault-scoped HMAC generation fingerprints; stable-id project bindings; incremental config-history rerendering; and a secret-free entry timeline in the app and `unv entry timeline`.
- R05 recovery packages: age X25519 `.unvpkg` streams with online SQLCipher snapshots, manifest and hash validation, marked local/NAS destinations, staged restore and local verification. `unv backup package|dest|recovery-key|schedule`, `unv recover`, and `unv ops` share the same core with the desktop Recovery settings.
- Ordered SQLCipher schema migrations and durable, fenced operation records. The unlocked desktop executor holds an advisory lock, coalesces missed schedules, skips stale work, and stops before lock-time key zeroization.

## 0.44.x (2026-10-11 to 2026-10-12)

### Changed

- The application layer moves into a shared `unv-app` crate used by the CLI, the server and the desktop app (ADR on the application-layer boundary).
- New backups use a hardened PBKDF2 cost and header; backups written by earlier versions still decrypt.
- Vault salts, backup salts and IVs come straight from the OS random generator, and remote vault ids use `crypto.randomUUID`.

## 0.43.0 (2026-10-11)

### Changed

- Dependency modernisation across crypto, backup, TOTP, node and server transport code, with a reviewed OpenAPI snapshot added to catch API drift.
- YAML tools use typed parsing and serialization.

## 0.42.7 (2026-10-11)

### Fixed

- Concurrency: audit appends are atomic, pool state is written under an exclusive lock, TOTP replay marks are compare-and-swap, sessions end when a user, token, password or permission changes, and stale saves answer 409.
- Node pull acceptance is bound to the SHA-256 the reviewer saw (`unv node accept --sha256`), and a changed upload is refused with `ACCEPT_CHANGED`.
- `unv import-vault` never replaces a differing stored secret or an ambiguous name without `--force`.
- A failed commit rolls back, and a resent report is recorded as one audit row.

### Added

- Chaos tests for the server, storage, nodes and concurrent desktop clients.

## 2026-10-09 (0.42.x)

### Changed

- The product is now UnENVerse everywhere: the crates and folders are `unv-cli` and `unv-server`, the desktop package is `unenverse`, release archives are named `unv-<version>-<platform>`, the data directory is `io.unenverse`, the per-directory context file is `.unv.json`, browser-storage keys start with `unenverse-`, and calendar event ids end in `@unenverse`. The `ENVV_*` environment variables are no longer read; use `UNV_*`.
- The container image is published as `ghcr.io/<owner>/unenverse/unv-server`.
- The licence is now Apache-2.0.
- The README is rewritten for new users; this changelog replaces its history table.

### Added

- `unv node install`: sets up a managed node in one command (system user, hardened service, file access limited to the named targets, reload through a root-owned path unit, optional loopback relay to the hub). `--plan` shows the steps.
- Every release now attaches `unenverse-<version>.vsix`, the VS Code extension, which is versioned with the rest.

### Fixed

- `unv --env-file` now reads the `UNV_PASSWORD` and `UNV_SERVER_URL` entries of a compose `.env`.
- Node targets with a rooted path such as `/etc/x` are accepted on Windows.
- Windows and Linux CI: node end-to-end tests no longer race the hub's poll stamp, the history snapshot test no longer depends on scheduling, Rust formatting is enforced by `npm run preflight` before a push, and the Linux build job installs the `mold` linker.


## 2026-10-09

### second rename sweep, Apache-2.0, README and CHANGELOG rewrite, demo server and screenshots, CI hardening

3. Licence: LICENSE replaced by the official Apache-2.0 text (curl from apache.org), NOTICE added, `license` fields in package.json, vscode-extension, tauri.conf, 4 Cargo.toml, image label.
5. Demo + screenshots: `tools/demo/seed.py` (fake vault via the `unv` CLI against a throwaway server: 29 entries across the common providers and kinds, 3 projects, 12 categories, users alice/bob/ci-bot, classes Developers/Operators, permission expressions), `tools/demo/shots_demo.py`, `tools/demo/run.sh` (starts `unv-demo` on :18743, seeds, runs the real app in the viewer container over `--network host`). Output `Scr
6. CI: `unv-server/tests/nodes_e2e.rs` waits for `last_polled` (Windows race), `history.rs` snapshot test sleeps between saves (timing under load), `cargo fmt` applied everywhere (the Ubuntu failure was import order after the rename), `scripts/preflight.sh` + `npm run preflight` + `.githooks/pre-commit` mirror CI's gates.
7. Verification: `cargo test --workspace` 707 passed; `npm run check` 1497 passed; fmt/clippy clean for unv-cli.
- Nothing pushed; no CI run observed for these changes.
- VPS node binary is still release 0.42.1 (pre-rename strings); protocol domains unchanged so it interoperates. Apply (push) for wg0 still off; decision pending (root agent vs polkit).
- Peer comment lines (`# name`) of the live wg0.conf are not produced by the exporter.
- The CHANGELOG is machine-compiled from session notes; wording is uneven for older entries.
- VS Code extension settings keys are now `unenverse.*` (users must re-set them).

### the rename to UnENVerse, the Discord acceptance, restore-into-chunks, and a way to see the app

- New in this round: **public inference** (`ImportedVar.public`). A variable is marked public only when it is plainly not a secret: names that look secret (key, token, secret, password, webhook, sig, auth, passkey) always win; hex ints, bools and floats are public; short ints are public; URLs are public unless they carry userinfo, a credential query parameter, a webhook path or a token-looking path segment; prose strin
- History pane: a "Restore into chunks" button on each snapshot row for formats with a reader. It fetches the real text with `reveal: true`, confirms, applies, saves, repaints.
- Nodes pane: "Read into a chunk" is now offered for those formats as well as `.env`, using the same plan and confirmation.
- Values arrive as literal text (the deployed file had no references), and the confirmation says so. There is deliberately no Rust twin (six formats parsed twice would be six chances to disagree); no new Tauri command, so the capability map needs no entry. Test: `tests/restore-chunks.test.ts`.
- Lazy `version_history` loading in the app; concurrent polls for listening nodes; certificate and hub transport key rotation; a Nextcloud rotation adapter; the 24.4 container benchmark (needs a 2 vCPU host); a hardware or passkey approver; Grafana token expiry in the probe. Each needs either a decision or a real service; none is a code gap that tests can close.
- A4, A9, A10 smoke tests: the viewer now makes them possible (Xvfb has X11 only; Wayland is still not covered, and Caps Lock and the input method need `xdotool` and an IM inside the image).

### everything that was left: 30.1/30.2, 34.1, 37.1, 38.1, 29.1, 28.1, 31.3 and the leftovers of 24.5, 35 and 36

- **Restore a history snapshot into chunks** (35): only `.env`-style files have a Rust parser; the other formats' parsers are TypeScript, and a second implementation would be a new twin pair. The pulled-file path (`--into-chunk`) covers the `.env` case.
- **Lazy `version_history` in the app** (30.2), **concurrent polls** and **certificate rotation** for listening nodes (ADR-0145), **a hardware key as approver** (ADR-0147), **a Nextcloud rotation adapter** and **Grafana token expiry** (ADR-0148).

### 30.1 (delta saves, server and store half) and 29.1 (two more rules)

| File | Change and why |
| ---- | -------------- |
| `src/ts/state.ts` | `RemoteVaultStore.save` now calls private `send('PUT', data)`; new `saveRows(put, del)` calls `send('PATCH', {put, delete})`. One body of toast, `If-Match` and etag logic for both. |
| `tests/remote-save-rows.test.ts` (new) | Stubs `fetch`: PATCH method, body is exactly the delta, the returned ETag is sent as the next `If-Match`. |
| `vault-core/src/config_check.rs` | Two rules, RULES 12 to 14. `compose-network-undeclared` (error): a service `networks` entry that no `docker_network` chunk declares; silent unless the project declares at least one network, `default` always exists, refs are skipped, names compared case-insensitively. `k8s-service-selector-unmatched` (warning): the starters generate `selector: app: <name>`, so a Service selects the
- **App dirty tracking.** The Tauri side and `state.ts` stores still save the whole document; nothing calls `saveRows` yet, and no Tauri `save_vault_rows` / `Access::save_rows` exists. The app tracks no dirty ids. This is the larger half of 30.1 and the one that touches every mutation site in the renderer.
- Pinned-HTTPS proxy (`remote_request`) still returns no headers, so ETag and `X-Vault-Merged` do not reach the store over it.
- 30.2 (chunk rows, age-based retention, full-load speed) untouched.
- 29.1: `envv check --all-projects` and a separate `gate()` function remain unbuilt (the push gate already refuses on errors).
- 38.1, 34.1, 28.1 per-pane tests, 31.3 further catalogue entries unchanged.

### Sub-phases 28.1 and 29.1, the 1.0 viability review, the decision to defer 30.1/30.2


### Phase 39: pre-1.0 security review and pentest checklist


### Phase 38: stack integrations

- Integrations (Prometheus, Grafana, Homarr/Homepage, Nextcloud, VS Code) each hand-written in four places (Rust exporter, TS exporter, starters, rules) would add a dozen drift points. Phase 38 also had to name "the adapter shape they share".
- The shape: a JSON descriptor per integration (`vault-core/data/stack-adapters.json`) with chunk types (fields, secret flags, defaults), an output document grammar (literal, `{f,as,fmt}`, map, list, each, singleton, group, entry, when) and rules (unique, required, exclusive, together, choices, needs_one_of). Two interpreters: `vault-core/src/stack.rs` and `src/ts/stack.ts`, pinned by golden files. Adapters shipped: `p

### Phase 37: approval


### Phase 36: blast radius

1. Nothing recorded which secrets a rendered file *contained*.
2. Local `envv exec` and `--out` are deliberately outside the audit chain, so there was no record of what reached *this* machine.
3. By the time of a compromise an entry may have been rotated, renamed or deleted, so "what is in the vault now" is the wrong thing to look up.
- The app has no UI for `--host local` (by nature: the app's copy actions are not logged). If the app ever logs its own copy/export actions it should write the same ring through a Tauri command.
- `matlog::note` builds a `Matcher` per call (O(secrets)); fine for a handful of materialisations per command, wasteful in a loop. Cache the matcher in `remember` if a command ever materialises per-item.
- `exposed` lists are computed at snapshot time against the vault *then*: an entry created after the file was written is correctly absent; an entry whose value was identical by coincidence is present (reported, marked short if under 8 characters).

### Phase 35: the config time machine


### Phase 34: Nodes

- `listen` mode; pull-into-chunks; per-beat render cache (every beat loads the vault and renders; fine at tens of nodes, measure before hundreds; the ceiling is O(vault) per beat per node).
- `node.apply` rows carry the node's reported error text (cut to 400 characters, scrubbed on the node). A hostile node can still write misleading text into its own audit rows; it cannot forge another node's actor.
- The hub trusts a node's `HostInfo`/`TargetReport` for display only; none of it feeds a decision except `apply` (which only controls whether content is sent, and the node re-checks).
- No rate limit on a signed node's beats beyond the per-IP failure limiter (a valid node can beat as fast as it likes). Add one if nodes are ever less trusted than "the operator's own machines".
- Phase 39 (pentest) should cover: enrollment token guessing and the 12-attempt limiter, replay across restart, the signature input format, the config-check gate bypass by project-name tricks, command injection through `reload` (only the node's own file feeds it), and the long-poll as a resource-holding route.

## 2026-10-08

### Phases 33.1b to 33.7: the remaining parity gaps (2026-10-08)

- Bulk `uid register` in the pane; `--fix` in the app's doctor; strict-write on the CLI against a remote; the archive/WAL caveat above.

### Phase 32.3 and Phase 33 (UI ⇄ CLI parity, mechanical half) (2026-10-08)


### Phase 32.2: the last controls (2026-10-08)

- `tests/probe.ts`: `probeKinds` accepts an async `reset` and an `afterReset` hook (lets a reset click "Run Scan" and then clear the toast it raised).
- `tests/efficacy-surfaces.test.ts` + `efficacy-surfaces-silent.json` (empty): surfaces `health` (vault with a weak entry and an empty bundle; click `#health-scan-btn`; probe `#health-results [data-action]`), `settings` (sidebar sections minus `tags`, so "+ Show" exists; `#settings-overlay [data-action]`), `icons` (`openIconPicker()`; `#icon-picker-overlay [data-action]`), `feeds` (a `RemoteVaultStore` instance with 
- `tests/efficacy-settings.test.ts` + `efficacy-settings-silent.json`: for each of the 19 inputs/selects in the Settings overlay plus 5 segmented/theme groups, change it, press Save, and require `Settings.getAll()` to differ. Only `settings-backup-pw` is silent (an action's password field), allowed with that reason.
- `tests/efficacy-cards.test.ts`: selector now includes `#bundle-suggest [data-action]`; reset clears `dismissedBundleSuggestions` (a Dismiss probe persisted into the next probe).
- `tests/efficacy-config.test.ts`: `extras(type)` adds an `env_file` chunk (docker), a docker service with a `${Alpha}` env field, a wg_peer with a vault-reference field, an nginx_key + nginx_server, and a certificate entry for `example.com`.
- `tests/honest-controls.test.ts` +7 tests: banner Accept creates a bundle, banner Dismiss stores `'twin'` (lower-cased), a `${ref}` badge from a config view shows the entry, a health-scan finding for a filtered-out entry shows it, unlock and re-lock screens refuse an empty password visibly, Enter on the `role=button` card icon opens the icon picker.

### Phase 32.1: efficacy audit of the dynamic controls (2026-10-08)

- `tests/probe.ts` (new, shared): `snapshot()` (body HTML with inline style reduced to display/visibility, plus every input/textarea/select value/checked/disabled), `clearTransient()` (toast, every `.open`, and now every `.tool-status` line, so two probes ending in the same status text cannot hide each other), `hadEffect(act, waitMs)` (MutationObserver records OR snapshot diff), `probeKinds({reset, selector, sample, 
- `tests/efficacy.test.ts`: now uses the shared helpers (behaviour identical; 145 buttons, 143 clicked).
- `tests/efficacy-cards.test.ts` + `efficacy-cards-silent.json` (empty): one entry per registered secret type (from `registry()`), plus TOTP seed, 2-key pool, 4-member bundle, same-provider twins, long descriptions; `allExpanded`; probes `#card-grid [data-action]`. 582 controls, 34 kinds, 0 silent, partial: `bundle-order` (first member cannot move up), `bulk-toggle`, `bundle-slot-tab` (active tab).
- `tests/efficacy-config.test.ts` + `efficacy-config-silent.json` (empty): one project per type using the real starter factories (wireguard, docker, nginx, kubernetes, ssh_config, traefik, apache, haproxy, ansible, postgres), selected via `st.currentSelectedProjectIds` and `render()`. 346 controls, 122 kinds (type-prefixed), 0 silent after the fix below.
- `tests/efficacy-closers.test.ts`: every allow-listed "Closes a dialog" id is pressed with its dialog open (`.open` on the nearest `[role=dialog]`/`.modal-overlay`) and must close it; `settings-cancel` needs `openSettings()` because its handler is assigned there (invariant 9), so an `openers` map exists for such cases. 19 tests.
- `tests/honest-controls.test.ts`: +4 tests (chunk move refused with a toast, chunk move among peers works, pool card with a bundle present, icon apply with empty name).
- 15 `data-action` kinds unreached; listed in the 32.2 row with what each needs.
- `<select>`/checkbox/toggle controls not probed: most apply on Save. 32.2(b).
- The probe waits 40-60 ms; a control whose effect lands later reads as silent.
- State leaks between probes through module state not reset by `resetState`. Add to the reset rather than reordering.
- Pre-existing uncommitted changes in the tree (including `tests/fixtures/*`, `index.html`) are not from this step.

### Phase 32: "Does it actually do anything?" (2026-10-08)

- `tests/efficacy.test.ts` (new): boots the real shell exactly like `tests/vault-shell.test.ts` (real `index.html`, `mountToolsPanes()`, `import('../src/ts/vault')`, keep the bound nodes, re-append per probe), seeds a 2-entry vault, and for each of the 145 `button[id]` (143 clicked; 2 disabled) resets the toast and every `.open` dialog, clicks, waits 60 ms, and records "silent" when neither a `MutationObserver` recor
- `tests/efficacy-silent.json` (new): the allow-list, 40 entries, each with a reason (closers with no dialog open, controls hidden until a precondition, handlers bound only when a lock screen is shown, empty-input converters, native file picker).
- `tests/honest-controls.test.ts` (new, 26 tests): 12 Copy buttons x (empty -> "Nothing to copy yet" err toast; full -> "Copied ✓" ok toast), bulk delete with nothing ticked, import confirm with nothing staged.
- `src/ts/tools.ts`: `copyOrExplain(v)` replaces 11 `if (v) void clipboardWrite(v);` sites and the fmt-copy one. Bulk delete and import confirm toast when their precondition is unmet. `src/ts/modals.ts`: cookie split with no parseable cookies and the session-preset header copy with no preset now toast.
- 32.1 (new row in AGENTS.md) holds the rest: `[data-action]` card controls, menu items, selects/toggles, `role="button"` divs, config-view handlers, closers with their dialog open, the `audit-verify` probe artefact.
- The probe waits a fixed 60 ms per click; a control whose effect lands later (a slow IPC) is falsely silent. The 17 s runtime is mostly that wait.
- Probes leak state across each other (entries deleted, settings changed) since only the vault and a few `st` fields are reset; order effects like the audit one are possible. Add to the reset rather than reordering.
- Vitest here swallows `console.log`; debug by writing a file.
- Pre-existing uncommitted work in the tree (many files in `git status`) is not from this session; stage explicitly.

### Phase 31: the provider catalogue (2026-10-08)

- `vault-core/src/catalogue.rs` (new, owns all rules): `Provider`, `Catalogue`, wire form `{payload: <JSON string>, signature: <b64>}` so the signature covers exact bytes. `verify` (Ed25519 against a key arg), `sign` (seed), `store` (verify, refuse rollback by `generated_at`, atomic write 0600), `load_cached` (re-verifies every call), `cache_path` (`ENVV_CATALOGUE_FILE` override, else `dirs::data_dir()/io.envvault/ca
- `vault-core/Cargo.toml`: `ed25519-dalek = "2"` (already in Cargo.lock via ssh-key 0.6.7, no new crate downloaded; Cargo.lock got the dependency edge). `vault-core/src/lib.rs`: `pub mod catalogue;`.
- `envv-cli/src/catalogue_cmd.rs` (new): `CatalogueCmd` = `update [--url|--file]`, `show`, `diff`, `export` (compiled table as providers JSON), `sign <in> --key-file -o`. https only, `build_public_client` (the one allowed CA-validated client, per the guard test in tests/agent.rs), 4 MiB cap. Public reference data, so stdout is allowed (unlike emit).
- `envv-cli/src/main.rs`: `Commands::Catalogue`, handled before `run()` like `Describe` so it never asks for a password; added to the no-op arm in `dispatch`. `envv-cli/src/lib.rs`, `envv-cli/Cargo.toml` (`hex = "=0.4.3"`, same pin as vault-core).
- `envv-cli/src/enrich.rs`: new owned `Hit`, `catalogue()` (OnceLock, loaded once per process), `lookup()` = longest matching prefix across cached catalogue and compiled `SIGNATURES`, tie to the catalogue; `plan_entry` uses it; `bundled_providers()` public for export/diff. `AXIS_SIGNATURES` (acts_as/exposure) is still compiled-only and matched by prefix equality, so a catalogue-only prefix gets type/icon/url/env/tag 
- `src-tauri/src/lib.rs`: `catalogue_status`, `catalogue_update(url)` (https only, 20 s, 4 MiB, then `catalogue::store`), registered in `generate_handler!`.
- `catalogue/providers.json` (new, 331 lines): seed source, produced by `envv catalogue export` from the compiled table (NOT retyped). This is the file maintainers edit.
- `.github/workflows/docs.yml`: daily cron + a "Sign the provider catalogue" step writing `site/catalogue/catalogue.json`; skips cleanly when the `CATALOGUE_SIGNING_KEY` secret is absent.
- Default URL `https://darthdemono.github.io/EnvVault/catalogue/catalogue.json` is unverified against a live deployment (Pages enablement is still manual, Phase 24).
- No UI: the IPC commands exist but no button or Settings row calls them (invariant 10 gap, record as such). `tests/ipc-contract.test.ts` only checks frontend->registered, so it does not notice.
- Catalogue `docs_url/rotate_url/revoke_url` are carried and validated but nothing consumes them yet; axes (acts_as/exposure) are compiled-only.
- The catalogue content is only the 48 compiled prefixes; "a large set of providers" needs real curation, with sources, outside this change.

### Phase 30: storage, row-per-entry, schema v2

- A delta API from clients (the remaining O(vault) CPU), chunk-level rows, history-per-revision rows, and indexes beyond the audit ones: nothing queries by them yet. `load_entries_where` exists in `storage.rs` and has no caller.
- No visual check of the conflict dialog text in a real window.
- `time`-based pruning of `vault_saves`: retention is "newest 2000 saves", which a vault saved thousands of times a day (a polling script) would burn through in hours, making an hour-old token a whole-vault conflict. If that bites, raise `KEEP_SAVES` (it is one constant) or prune by age.
- The remote store over the pinned-HTTPS proxy surfaces no headers, so it still has no ETag there and writes unconditionally, as before.

### Phase 29: the config compiler

- Six designed checks, eight rule ids (two checks split into an error case and a softer one). All fire only on positive evidence; all skip `${ref}`, disabled chunks and anything that could be satisfied somewhere this function cannot see.
| # | id | severity | fires when | silent when |
| 1 | `nginx-proxy-pass-unknown-service` | warning | `proxy_pass` host is a bare name that is neither a docker service/`container_name` nor an `nginx_upstream` | the project has no `docker_service` chunk; host is dotted, an IP, `localhost`, a `$variable`, a unix socket or a `${ref}` |
| 2a | `wireguard-allowed-ips-duplicate` | **error** | two peers claim the same masked network | one peer repeating itself; `${ref}` values; disabled peers |
| 2b | `wireguard-allowed-ips-overlap` | warning | one peer's network strictly contains another peer's | a `/0` default route; different families; disjoint networks |
| 3 | `traefik-middleware-missing` | warning | a router `middlewares` item matches no `traefik_middleware` name (case-insensitive) | item contains `@` (another provider) |
| 4a | `k8s-deployment-secret-missing` | **error** | a Deployment's `secretEnv`/`secretMounts` names a Secret no `k8s_secret` chunk creates in that namespace | the project has no `k8s_secret` chunk; `${ref}` names; a mount with no path |
| 4b | `k8s-ingress-service-missing` | warning | Ingress `serviceName` (or its own name) has no `k8s_service` of that name in that namespace | the project has no `k8s_service` chunk |
| 5 | `compose-env-ref-unresolved` | warning | a `${NAME}` anywhere in a service value (or a `ref_name`) is neither an `env_file` key nor a vault entry name | `${NAME:-d}`, `-`, `:?`, `+` operator forms; `$${NAME}`; `chunk:`/`bundle:` refs |
| 6 | `pg-host-network-unreachable` | warning | `pg_connection.host` is a docker service S and a consumer C shares no network with it; consumer = lists S in `depends_on`, spells S in an env value, or reads `${chunk:<pg>/…}` | no consumer; either side has `network_mode`; both on the implicit default network |
- No ui-lab screen for the panel: the lab seeds a vault and does not run IPC, and the panel only exists under Tauri. The panel's markup and escaping are unit-tested; its visual layout in a real window is unchecked (WebKitGTK window tests remain a human task).
- "A gate a node runs before `apply`" from the design: Nodes (Phase 34) do not exist yet.

### Phase 28: escape by construction

- Exporter functions and other string-building code that is not markup were not touched (by design).
- `esc`/`escAttr` still exist and are exported; `render.ts` and `tools.ts` no longer import them.
- The `innerHTML = ''` sites became `setHtml(x, '')`, not `replaceChildren()`; either is fine.
- Static markup in `index.html` and `tools-markup.ts` is not templated (it has no interpolation).
- A real WebKitGTK window was not run (A4/A9/A10 remain human tasks).

### Phase 27, UI-lab triage (layout backlog 1,155 → 0)

| Check | Result |
| --- | --- |
| `npx vitest run` | 58 files, **1,257 tests pass** (82 s) |
| `npm run typecheck` | clean |
| `npx eslint ui-lab` | clean |
| `npx prettier --check ui-lab src/css` | clean (after `prettier --write src/css/a11y.css`: I had written `[role='button']`, prettier wants double quotes) |
- (If the count changes, read `SCREENS` x `VIEWPORTS` in `ui-lab/layout.spec.ts` plus the single tests in `layout.spec.ts` and `behaviour.spec.ts`.)
- This is the part that went wrong first and then right. Read the Dead ends section
- before changing the rule again.
- What the 19 actually were (`detail` strings from `layout-findings.json`):
- `#add-btn`, `#copy-all-btn`, `#copy-all-arrow` "overlaps `#settings-close` /
-   `button.settings-tab`" on **settings-*** screens: header buttons sitting *under* the

### Phase 26, outward redaction engine

- Phase 26 is complete and uncommitted.
- `envv-cli/src/shield.rs` builds one Aho-Corasick matcher from every vault value
-   the fail-closed CLI redactor treats as private. It uses the existing
-   `sha256:<12 hex>` fingerprints for replacements and reports.
- `envv shield -- <command>` runs a child with piped stdout and stderr, shields
-   both streams independently, and returns the child's exit code. It keeps the
-   final `max_secret_len - 1` bytes between reads, so a secret that crosses a
-   pipe read boundary cannot leak a prefix.
- The wrapper refuses NUL-containing or invalid-UTF-8 output rather than
-   half-masking binary data. It drains the rest of that pipe before failing so a
-   child cannot block on a full pipe. `--json` is refused because the wrapper
-   deliberately forwards the child's ordinary output.

### Phase 25, route coverage and permission-expression parity

- Phase 25 is complete, but remains uncommitted with the rest of the Phase 24
- worktree.
- `envv-server/src/lib.rs` now exercises the actual `build_router` path across
-   and revocation.
- `tests/fixtures/parity/permex.json` is the shared truth table for permission
-   expressions. `vault-core/src/permex.rs` and `tests/permex.test.ts` both read
-   it, so a semantic disagreement fails either suite.
- The TypeScript evaluator gained the core's composition primitives
-   (`combine`, `anyOf`, `requireAll`). The permission editor preview now shows
-   The UI had fields for those values but could not receive them, so its former
-   preview could not model the server's effective access rule.

### Phase 24 and 24.1–24.5, everything that can be built


## 2026-09-21

### Phase 24 continuation


### Phase 24 continuation


### Phase 24 continuation

- Phase 24 A4 now queries Caps Lock through a Tauri command (GDK on Linux,
-   `GetKeyState` on Windows) on focus and after Caps Lock events. The typed-key
-   fallback remains. X11 and Wayland still need a real native-window smoke test.
- A6/A7/A8/A10 were already implemented in the tree: labelled variable/2FA
-   groups, a shared sidebar-row height, the in-form generator popover, and
-   keyboard reorder. Added the A7 browser-layout assertion. The UI-lab server
-   could not start in this sandbox, so the assertion was not measured here.
- Fixed A2's remaining migration defect: `Settings.init()` now filters the old
-   `authenticator` sidebar key after merging settings sources, on every load.
- Fixed late Tauri bridge capture. `invokeTauri()` now resolves the current
-   proxy paths use that boundary. This repaired failures in the TOFU, users and
-   vault-watch tests.

## 2026-09-14

### Handoff 33 — 2026-09-14

**Phase 24, in the plan's own order:**
- **B.1** — `npx eslint --prune-suppressions`. `npm run lint` now exits 0
-   (501 warnings, no errors, no stale suppressions). Verified: `npm run lint`
-   exit code checked directly, not inferred from the problem count.
- **A1** — remote 2FA codes. `entry_totp_code`, `totp_import_parse`,
-   `totp_import_merge` and `totp_export_build` no longer take `State<VaultState>`
-   or check the local SQLCipher key — all four are pure over their arguments.
-   `liveCodeFor` (`src/ts/totp.ts`) gates on `st.vaultOpen` instead, which is
-   true for a local *or* a remote session. A refusal is no longer swallowed
-   silently: `tickTotp` now sets the slot's `title` to name it. 3 new Rust
-   unit tests, 3 new TS tests (`tests/totp-ipc.test.ts`).
- **A3** — exports. New Tauri command `write_export_file` writes a real file

### Planning Phase 24 and 24.1–24.4

- **Renumbered.** Old 24 (Storage) → 25, … old 38 (Pentest) → 39. A third
-   renumbering map (2026-09-14) is in `CLAUDE.md`; "Why this order" and the
-   dependency edges were rewritten against the new numbers. The next phase to
-   allocate is **40**.
- **Phase 24 planned** as defects + lint + CI hygiene + docs site, with four
-   sub-phases: **24.1** composite secrets, **24.2** secrets-grid legibility,
-   **24.3** calendar feeds, **24.4** unique-ID registry. Full design in
-   `CLAUDE.md` under *Phase 24*.
- **Three stale lines fixed in passing:** version (said 0.20.0, is 0.23.0), Phase
- Review-01 §8 (the lint backlog) moved from Phase 26 into Phase 24, because it is
-   what turned CI red.

### Handoff 31 — 2026-09-14


## 2026-09-13

### Handoff 30 — 2026-09-13


## 2026-09-11

### Handoff 29 — 2026-09-11


## 2026-09-10

### Handoff 28 — 2026-09-10


### Phase 21 fixed, Phase 22 allocated and implemented (TOTP as a stored secret)

- **No `envv totp add`/`set`.** `envv entry set X --totp …` is the write path;
-   a second verb for it would be two ways to write one field.
- **No `--dry-run` preview for `envv totp import`.** The global `--dry-run` works
-   (it is enforced inside `Access::save`, so the parse and the report run and
-   nothing is written), which is most of the value. A dedicated preview that lists
-   every planned action per entry would be better and is not there.
- **No encrypted-export support for any source app**, deliberately — see above.
- **No import from a QR image.** The app cannot decode one (no camera, no
-   decoder), so `otpauth-migration://` has to be pasted or saved to a file first.
- **No health-scan check for an unusable stored seed.** `envv totp ls` reports
-   one per row (`usable: false` with the reason) and every write path validates,
-   so the only way in is an imported vault. Worth adding to `doctor` in a later

## 2026-09-04

### Handoff 26 — 2026-09-04

- All six answered. Verification:
- ```
- npx vitest run            931 passed (42 files)   # was 928; +3 caps-lock tests
- npx tsc --noEmit          clean
- npm run lint              exit 0, 0 errors, 490 warnings (suppressed backlog)
- npm run format:check      exit 0
- cargo check --workspace   clean
- cargo doc --workspace --no-deps --locked   clean
- npx playwright test behaviour   3 passed
- ```
- `eslint-suppressions.json` lost one `no-floating-promises` entry via
- `--prune-suppressions` (never `--suppress-all`), because `init()` is now

### Caps Lock bug, and Phase 19 finished (TOTP, onboarding, accessibility)

- Everything below is complete and verified.
- ```
- npm test                     42 files, 928 tests, all passing
- cargo test --workspace       251 tests, all passing
- npm run typecheck            clean
- npm run lint                 0 errors, 490 warnings (the pre-existing ratchet)
- npm run lint:rust            clean
- npm run format:check         clean
- ```
- committing, which cuts a release: `build.yml` gates on whether `v<version>`
- already exists, so the push is the trigger.

### Phase 22 designed: credential shape, copy profiles, env naming

- All of it is in `CLAUDE.md`, in a new `## Phase 22` section:
- **The naming template** — `[PREFIX_] PROVIDER [_VERSION] [_LABEL] [_ROLE]`,
-   with normalisation rules, and `envName()` as the single builder that
-   `dotenvKey` and `import-export.ts`'s `envKey` both collapse into.
- **New fields** — `primary_role`, `secret_role`, `label`, `user_agent`,
-   `env_name_override`, plus `public` and `role` on `extra_vars`.
- **Copy profiles** — basic / extended / full, metadata as comments by default.
- **`env_var` without a mandatory primary**, with the audit list of six places
-   that assume the primary value is never empty.
- **Cookies** — a new `SecretType`, five copy targets, a paste-parser.
- **The IRL shapes table** — ten credential shapes taken from real issuers, and
-   what each one breaks today.

## 2026-08-31

### Review-01 defect fixes, the Key Pools sidebar, and the **Nodes** design

- **Phase 26 grows a third deliverable, 30c:** the parity audit becomes a
-   standing checklist, regenerated from `envv describe` rather than maintained by
-   hand — the same "prefer the command to the list" discipline `CLAUDE.md`
-   already prescribes and under-applies. A test that diffs the two capability
-   sets against a declared exemption file is the mechanism; anything else decays.
- **Several gaps are already phases.** `envv diff` is tier-1 #4, `doctor` in the
-   UI is tier-1 #9, `envv config` overlaps nothing yet. Fold rather than duplicate.
- **`enrich` in the UI is the one worth pulling forward.** It is the largest

## 2026-08-27

### Phase 20 observability, secret creation dates, an ICS calendar feed, and `envv use`

**Version 0.8.1** (unchanged — no bump this session; the release pipeline is
- triggered by a version bump, and this work is not released yet).
**Verification at the pause point:** 856 frontend tests across 37 files, all
- Rust tests passing (`envv-cli` 19 lib + 8 parity + 6 + 11 + 1, `envv-server` 21),
- `tsc --noEmit` clean, `cargo check --workspace` clean.

## 2026-08-25

### Phases 17 and 18 implemented, CI unblocked twice, README rewritten

**Version 0.8.1.** Tags `v0.7.1`, `v0.8.0` and `v0.8.1` all exist on the remote,
- so the release pipeline has now cut releases end to end. That was an open item in
- the 1.0 plan and it is closed.
**183 Rust tests, 814 frontend tests.** clippy clean, `cargo fmt --check` clean,
- `tsc --noEmit` clean, ESLint 0 errors, Prettier clean, `cargo doc` clean.

### CI unblocking, handoff reorganisation, and the 1.0 plan


### Tooling, release automation, key pools (phase 16)

- Everything below is committed and pushed. Verification, run at the end:
- ```
755 tests, 31 files (vitest)      — was 671/27 at Phase 11
134 tests (cargo test --workspace) — was 105
- eslint .            0 errors, 464 warnings, 237 suppressed
- prettier --check .  clean
- cargo fmt --check   clean
- cargo clippy        clean (workspace, --all-targets)
- tsc --noEmit        clean
- ```

## 2026-08-24

### Card sizing, repo cleanup, and the CLAUDE.md drift audit

- `src/css/cards.css` now derives every card dimension from `--cs-*` tokens
- selected by `#card-grid[data-card-size]`, which `applyGridSettings()` stamps in
- `src/ts/state.ts`. One setting moves width and height together.
| token | compact | medium | large |
| ----- | ------- | ------ | ----- |
| `--cs-card-h` | 200px | 236px | 280px |
| `--cs-icon` | 32 | 40 | 48 |
| `--cs-desc-lines` | 2 | 3 | 4 |
| `--cs-foot-h` | 34 | 40 | 44 |
- Collapsed cards have a fixed height; `.card-head` takes the slack (`flex: 1 1
- auto`) so the footer sits on the bottom edge. Expanded cards are `height: auto`
- and every clamp is lifted. `#card-grid` gained `align-items: start` so an

## 2026-08-23

### Agent-safe CLI, custom icons, live enrichment, cross-platform (phases 14–15)

- `cargo test --workspace` — 16 agent tests, 6 parity, 7 server, 76 vault-core, all passing
- `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets` — clean
- Live end-to-end against a scratch vault and a real `envv-server`: generation,
-   redaction, `exec`, `render`, `describe`, `login`, exit codes, RBAC
- `envv-cli/examples/envv.py` driven against a live vault: create → rotate →
-   exec → typed error, with no secret in the Python process
- `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets` — clean
- `cargo test -p envv-cli` — 22 passing (16 agent, 6 parity)
- `npm test` — 662 passing; `npm run typecheck` clean
- Docker image builds; RAM measured before and after on a 4-core host
- Live probes exercised against real GitHub and Stripe endpoints

### CLI parity with the UI, and two exporter bugs it uncovered (phase 13)

- Done and verified:
- `npm test` — **662 passed** (651 before; +7 parity fixtures, +4 regressions)
- `npm run typecheck` — clean
- `cargo check --workspace` — clean, no warnings
- `cargo test -p envv-cli` — 6 parity tests passing
- End-to-end smoke run against a scratch vault (`--db-path`) and a live
-   `envv-server` on port 18743: entry/project/chunk/category CRUD, exports,

## 2026-08-08

### UX persistence, window state, LAN fix (phase 12)

- ```
- npm test                 651 passed, 0 failed   (26 test files)  [was 564]
- npm run typecheck        clean
- npx vite build           clean
- cargo check --workspace  clean
- ```
- Test count by stage: 564 → 626 (QOL persistence) → 633 (LAN fix) → 651
- (experimental types).
**Committed:** `0088c90 QOL Improvements` — items 1 and 2.
**Uncommitted:** items 3 and 4 —
- `src/ts/lan.ts`, `src/ts/projects.ts`, `src/ts/remote-panel.ts`,
- `src/ts/settings-panel.ts`, `src/ts/state.ts`, `src/ts/types.ts`,

### Vault-switch render bug, three fixes

- ```
- npm test           564 passed, 0 failed   (22 test files)   [was 563]
- npm run typecheck  clean
- ```
- The new test was verified by reverting the fix and watching it fail:
- ```
- ❯ tests/remote-panel.test.ts:224
-     expect(document.getElementById('project-list')!.textContent).not.toContain('Remote WG')
-  Tests  1 failed | 25 passed (26)
- ```
---

### Frontend test suite and module audit (phase 11)

| Pass | Area | Bugs |
| ---- | ---- | ---- |
| 0 | Test infra + `filters` / `permex` / `utils` / `modals` / `render` | 2 |
| 2 | `projects.ts`, `import-export.ts`, `modals.ts` — same bug class | 9 |
| 3 | `tools.ts`, `chunk-ops.ts` + the three previously flagged | 11 |
| 4 | `users.ts`, `vault.ts` | 8 |
| 5 | `render.ts` escaping | 8 |
| 6 | `audit.ts`, `lock.ts`, `icons.ts` | 6 |
| 7 | `settings-panel.ts`, `filters.ts`, `perm-editor.ts` | 5 |
| 8 | `import-export.ts`, `remote-panel.ts` (+ Rust) | 6 |
| 9 | `chunks/parsers.ts`, `chunks/starters.ts` | 3 |
| 10 | `chunk-ops.ts` export functions | 4 |

## 2026-08-07

### Audit and phases 7–10.1

| Pass | What | Phase |
| ---- | ---- | ----- |
| 0 | Audit + first correctness batch | 7 |
| 1 | Auth model, audit attribution, permission scoping | 8 |
| 2 | Permission expression language | 9 |
| 3 | Open to LAN | 10 |
| 3.1 | Lost-update fix | 10.1 |
- ```
- cargo test --workspace        83 passed, 0 failed   (76 vault-core + 7 envv-server)
- cargo check --workspace --all-targets   clean, zero warnings
- npx tsc --noEmit              clean
- npx vite build                ✓ built

## 2026-06-13

### Remote RBAC, Discord-style hierarchy, sidebar/cert UX


## 2026-06-12

### Linking & Resolver Expansion (Prefix/Chunk/Cross-Chunk + Health/Server)


### Full Bug Audit + Fixes

**Security: Non-constant-time legacy SHA-256 comparison** (`vault-core/src/users.rs`):
- `hex::encode(h.finalize()) == hash_hex` — regular string comparison, vulnerable to timing attacks on legacy password hashes
- Fix: decode `hash_hex` to bytes; constant-time XOR-fold comparison: `computed.iter().zip(expected.iter()).fold(0u8, |acc,(a,b)| acc | (a^b)) == 0`
- No new crate dependency (avoids adding `subtle`)
**Security: `PUT /api/vault` auth order** (`envv-server/src/main.rs`):
- Payload validation (api_keys check) ran before `extract_session()` — unauthenticated malformed requests got 400 instead of 401
- Fix: `extract_session()` moved before api_keys validation
**Security: `GET /api/status` unlocked logic** (`envv-server/src/main.rs`):
- Fix: `state.sessions.lock().unwrap().values().any(|s| s.is_owner)`
**Functional: `openAdd()` listener leak** (`src/ts/modals.ts`):
- Added new `input`/`change` listeners on every call without removing previous ones
- After N modal opens, each form field had N `saveDraft` listeners stacked — multiple sessionStorage writes per keystroke

## 2026-06-11

### /12 — Bug Audit + ENV Link Feature

**Docker fixes:**
- Runtime dep: `libsqlcipher4` → `libsqlcipher0` (correct Debian bookworm package; build was failing exit 100)
- Storage: named volume `envv_data:/data` → bind mount `./envv:/data` (files visible, easy backup)
- `docker-compose.yml`: `ENVV_PASSWORD=${ENVV_PASSWORD:-}` env var; image tag bumped to `0.6.0`
**Auto-lock setting bug** (`settings-panel.ts`): `parseInt('0') || 20` evaluates falsy 0 to 20 — setting "disabled" displayed "20min". Fix: `parseInt(..., 10) || 0`.
**ENV chunk ↔ vault link** (`chunk-ops.ts`, `render.ts`, `index.html`, `sidebar-trees.css`):
- "Link" button on `env_file` chunk header (`data-action="link-env-chunk"`)
- `buildEnvLinkMatches()` — 4-tier confidence scoring:
-   - 100 = exact provider name match
-   - 95 = compound `PROVIDER_KEYID` match (strips suffix after last `_`)
-   - 88 = same raw secret value (≥6 chars) — strong signal even if key name differs
-   - 75/70/63 = suffix match after stripping 1/2/3 prefix segments (handles `ND_LASTFM_APIKEY` → `LASTFM`)

### Rename: apiv → envv


### Security Audit


## 2026-06-06

### Visual Patches + Docker Overhaul


## 2026-06-05

### v3 Polish (Handoff v3)


### Phase 5.1 + Docker (Handoff v2)


### Phases 4 + 5 (Handoff v1)


### Phases 1–3 (pre-2026-06-05)

- `VaultStore` abstraction (`LocalVaultStore` ↔ `TauriVaultStore`) — zero UI changes to swap implementations
- Schema-driven form tooltips via `schema.json`
- Simple Icons CDN with letter-avatar fallback
- CSS themes via `data-theme` on `<html>` (6 themes)
- `repeat(auto-fill, minmax(...))` set in JS — CSS `repeat(var(...))` is invalid
- `closeIconPicker` `onClose` callback pattern
- Tauri CSP as single string — array form rejected by Tauri 2 schema
- Removed `devUrl` from `tauri.conf.json` — Tauri 2 needs HTTP URI or absent
- `mod commands {}` — all `#[tauri::command]` fns in submodule to avoid E0255 proc-macro collision
- `execCommand` clipboard fallback for WebKitGTK silently-failing `navigator.clipboard`
- `data-action` delegation — replaced all `onclick="..."` in `innerHTML`
- `-webkit-appearance: none` on `<select>` for WebKit CSS colours
