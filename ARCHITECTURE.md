# Architecture

UnENVerse keeps secrets in one encrypted vault on your own machine. Three
programs share it. This file names the parts, the people and programs that act on
them, and what each is allowed to do. The security consequences are in
[THREAT_MODEL.md](THREAT_MODEL.md).

## Codebases

One repository holds everything that is compiled into a release.

| Path                   | What it is                                                                                    |
| ---------------------- | --------------------------------------------------------------------------------------------- |
| `vault-core/`          | Storage (SQLCipher), crypto, permissions and roles, history, audit chain, nodes, update check |
| `unv-app/`             | The application layer: every operation, written once and reached by the three front ends      |
| `unv-cli/`             | The `unv` command line and the node agent                                                     |
| `unv-server/`          | The optional `unv-server` (HTTP/TLS API for teams, and the hub for nodes)                     |
| `src-tauri/`, `src/`   | The desktop app: Rust shell (Tauri 2) and the TypeScript interface                            |
| `vscode-extension/`    | A thin editor client of the `unv` CLI                                                         |
| `website/`, `scripts/` | The product site and the build, release and check scripts                                     |
| `fuzz/`                | Fuzz targets for the importers and migrators                                                  |

## Actors

| Actor                      | What it is                                                      | What it can do                                                                                           |
| -------------------------- | --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Owner                      | The person who knows the master password                        | Everything: read, write, manage users, back up, restore                                                  |
| Named user                 | A login on `unv-server`, in a class with scoped permissions     | Only the projects and actions its permissions allow; rate limited on failed logins                       |
| API token                  | A credential for scripts and AI agents, exchanged for a session | The same scoped permissions as its user; output is redacted unless a materialising command is used       |
| Desktop app                | Tauri window plus TypeScript UI                                 | Calls `unv-app` over IPC; trusts the person at the keyboard                                              |
| `unv` CLI                  | The command line                                                | Calls `unv-app` directly, or a server over HTTPS; redacted output by default                             |
| `unv-server`               | The optional server                                             | Serves the vault filtered by permission; applies permissions before any filter                           |
| Hub and node agent         | The server in node mode, and an agent on another machine        | A node watches a config file, reports drift, and writes only what its own config allows; signed requests |
| Recovery destinations      | Local folders, NAS, or SFTP servers                             | Receive encrypted recovery packages; SFTP host keys are pinned                                           |
| Update and catalogue feeds | A signed manifest for `unv update`, a signed provider catalogue | Offer new versions or provider data; both are verified against keys pinned in the binary                 |
| Release pipeline           | GitHub Actions                                                  | Builds, signs (Sigstore), attests and publishes release assets from a tagged commit                      |

## How a request flows

1. A front end (desktop, CLI or server route) receives an action.
2. `unv-app` is the engine: it resolves `${refs}`, checks permissions, and holds
   the single write point, `Access::save`, which also enforces `--dry-run`.
3. `vault-core` reads and writes the SQLCipher database. The key comes from the
   master password through Argon2id and is held only in memory while unlocked.
4. Output goes through a resolver chosen for the purpose: redacting for display,
   materialising only for `exec`, `render`, `--reveal` and `--out`.

## Where to read more

Design decisions with their reasons are in `book/src/adr/` (published at
`/docs/`). The command contract is `unv describe`. The server API is described
by `unv-server/tests/fixtures/openapi.json`.
