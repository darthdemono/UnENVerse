# Phases

UnENVerse was built in numbered phases. Each phase is one coherent piece of work with its own tests, and a phase number is a **queue position**, not an id: the early plan was renumbered several times as work was inserted ahead of it, and each phase page lists the earlier numbers it carried so old notes stay readable. The numbers used here are the ones the project uses today.

- **Phases 1 to 20** built the product: the vault, the server, access control, the agent-safe CLI and the release pipeline.
- **Phases 21 to 42** were the road to 1.0.0: correctness, hardening, parity, nodes, history, approvals and the rename to UnENVerse.
- **Phases 43 to 56** are the operations roadmap (its ids are R00 to R34) and the behaviour-test round, numbered in the same sequence.
- **Phases 57 and later** are the queue of roadmap phases still to do, with the agent-safety lane planned after 1.0.0. See [the queue](roadmap-queue.md).

## Every phase

| Phase  | Name                                                                             | Status      | Page                               |
| ------ | -------------------------------------------------------------------------------- | ----------- | ---------------------------------- |
| 1      | Tauri wrapper, static frontend                                                   | Done        | [read](foundations-1-20.md)        |
| 2      | SQLCipher + Argon2id encrypted storage, unlock flow                              | Done        | [read](foundations-1-20.md)        |
| 3      | TypeScript + Vite, inline handler elimination, Projects groundwork               | Done        | [read](foundations-1-20.md)        |
| 4      | Remote vault server + CLI                                                        | Done        | [read](foundations-1-20.md)        |
| 5      | Multi-user / RBAC                                                                | Done        | [read](foundations-1-20.md)        |
| 5.1    | Security hardening pass                                                          | Done        | [read](foundations-1-20.md)        |
| 6      | TLS on server, cert pinning, tag sidebar filter, bug fixes                       | Done        | [read](foundations-1-20.md)        |
| 7–10.1 | Audit, auth model, permission expressions, Open to LAN, lost-update fix          | Done        | [read](foundations-1-20.md)        |
| 11     | Frontend test suite + module-by-module audit, 66 bugs fixed                      | Done        | [read](foundations-1-20.md)        |
| 12     | UX persistence, Tauri window state, LAN wrong-vault fix, experimental project ty | Done        | [read](foundations-1-20.md)        |
| 13     | CLI parity (`unv-cli` as lib + thin binary), exporter golden fixtures            | Done        | [read](foundations-1-20.md)        |
| 14     | Agent-safe CLI: redaction, JSON envelope, exit codes, `describe`                 | Done        | [read](foundations-1-20.md)        |
| 15     | Custom icons, live enrichment, Windows/distro portability, Docker RAM            | Done        | [read](foundations-1-20.md)        |
| 16     | ESLint/Prettier/clippy, `scripts/version.mjs`, release-on-version-bump + GHCR +  | Done        | [read](foundations-1-20.md)        |
| 17     | Foundation & security                                                            | Done        | [read](foundations-1-20.md)        |
| 18     | Core features & integrations                                                     | Done        | [read](foundations-1-20.md)        |
| 19     | UX & accessibility                                                               | Done        | [read](foundations-1-20.md)        |
| 20     | Operability & release                                                            | Done        | [read](foundations-1-20.md)        |
| 21     | `${Provider/ID}` resolved to the entry UUID                                      | Done        | [read](credentials-21-24-5.md)     |
| 22     | TOTP as a stored secret                                                          | Done        | [read](credentials-21-24-5.md)     |
| 22.2   | HOTP, Steam, the next code and the Authenticator screen                          | Done        | [read](credentials-21-24-5.md)     |
| 23     | Credential shape, copy profiles, env naming                                      | Done        | [read](credentials-21-24-5.md)     |
| 24     | Shipped-feature defects, lint to zero, CI hygiene                                | In progress | [read](credentials-21-24-5.md)     |
| 24.1   | Composite secrets and bundles                                                    | Done        | [read](credentials-21-24-5.md)     |
| 24.2   | The secrets grid, legible at scale                                               | Done        | [read](credentials-21-24-5.md)     |
| 24.3   | Calendar feeds                                                                   | Done        | [read](credentials-21-24-5.md)     |
| 24.4   | Unique-ID registry                                                               | In progress | [read](credentials-21-24-5.md)     |
| 24.5   | A richer credential model                                                        | In progress | [read](credentials-21-24-5.md)     |
| 25     | Close review-01, remainder                                                       | Done        | [read](hardening-25-33.md)         |
| 26     | The redaction engine, outward                                                    | Done        | [read](hardening-25-33.md)         |
| 27     | The UI lab                                                                       | Done        | [read](hardening-25-33.md)         |
| 28     | Escape by construction                                                           | Done        | [read](hardening-25-33.md)         |
| 28.1   | Escape II: coverage the first pass left                                          | Done        | [read](hardening-25-33.md)         |
| 29     | The config compiler                                                              | Done        | [read](hardening-25-33.md)         |
| 29.1   | Config compiler II: the next rules, and scope                                    | Done        | [read](hardening-25-33.md)         |
| 30     | Storage                                                                          | Done        | [read](hardening-25-33.md)         |
| 30.1   | Storage II: clients send deltas                                                  | Done        | [read](hardening-25-33.md)         |
| 30.2   | Storage III: rows below the project, retention, load                             | Done        | [read](hardening-25-33.md)         |
| 31     | The provider catalogue                                                           | Done        | [read](hardening-25-33.md)         |
| 31.1   | Settings → Data row "Provider catalogue": status line from `catalogue_status`, * | Done        | [read](hardening-25-33.md)         |
| 31.2   | `Provider` gained `acts_as`, `exposure`, `console_url` (validated against the 24 | Done        | [read](hardening-25-33.md)         |
| 31.3   | The catalogue holds only the 48 compiled prefixes, so it cannot yet add anything | In progress | [read](hardening-25-33.md)         |
| 32     | Does it actually do anything?                                                    | In progress | [read](hardening-25-33.md)         |
| 32.1   | `tests/efficacy-cards.test.ts` (one entry of every registered secret type + a po | Done        | [read](hardening-25-33.md)         |
| 32.2   | `tests/efficacy-surfaces.test.ts` (health-scan results, Settings sidebar-layout  | Done        | [read](hardening-25-33.md)         |
| 32.3   | Deleted the unreachable `copy-chunk-env` button, `hasChunkEnvRefs` and its handl | Done        | [read](hardening-25-33.md)         |
| 33     | UI ⇄ CLI parity                                                                  | Done        | [read](hardening-25-33.md)         |
| 33.1b  | `enrich --online` in the app: asks each issuer about its own credential, so it s | Done        | [read](hardening-25-33.md)         |
| 33.2b  | `unv doctor`'s file-level checks (integrity, storage, salt pairing, file permis  | Done        | [read](hardening-25-33.md)         |
| 33.3   | UI for `import-vault` (Bitwarden / 1Password / Proton, with the onboarding wizar | Done        | [read](hardening-25-33.md)         |
| 33.4   | UI for `user strict-write` (needs a server route too: `set_strict_write` is loca | Done        | [read](hardening-25-33.md)         |
| 33.6   | `unv diff A B` (entry diff, redacted by default) and CLI management of the temp  | Done        | [read](hardening-25-33.md)         |
| 33.7   | Delete or wire the six registered-but-never-invoked Tauri commands listed under  | Done        | [read](hardening-25-33.md)         |
| 34     | Nodes                                                                            | Done        | [read](nodes-and-history-34-42.md) |
| 35     | Config time machine                                                              | Done        | [read](nodes-and-history-34-42.md) |
| 36     | Blast radius                                                                     | Done        | [read](nodes-and-history-34-42.md) |
| 37     | Approval                                                                         | Done        | [read](nodes-and-history-34-42.md) |
| 38     | Stack integrations                                                               | Done        | [read](nodes-and-history-34-42.md) |
| 39     | Pentest before 1.0.0                                                             | In progress | [read](nodes-and-history-34-42.md) |
| 41     | Sub-phases 28.1, 29.1, 30.1, 30.2, 34.1, 37.1, 38.1 and the 24.5 leftovers, comm | Done        | [read](nodes-and-history-34-42.md) |
| 42     | The rename to UnENVerse, restore a snapshot or a node's file into chunks (`src/t | Done        | [read](nodes-and-history-34-42.md) |
| 43     | R01 security hotfixes                                                            | Done        | [read](roadmap-phases-43-56.md)    |
| 44     | R22 UnENVerse website                                                            | Done        | [read](roadmap-phases-43-56.md)    |
| 45     | R02 CI release gating + speed                                                    | Done        | [read](roadmap-phases-43-56.md)    |
| 46     | Chaos testing program                                                            | Done        | [read](roadmap-phases-43-56.md)    |
| 47     | R03 dependency modernisation                                                     | Done        | [read](roadmap-phases-43-56.md)    |
| 48     | R04 unv-app extraction                                                           | Done        | [read](roadmap-phases-43-56.md)    |
| 49     | R05 ops runtime + local recovery                                                 | Done        | [read](roadmap-phases-43-56.md)    |
| 50     | R06 SFTP, retention, restore drill                                               | Done        | [read](roadmap-phases-43-56.md)    |
| 51     | R07 metadata + lifecycle core                                                    | Done        | [read](roadmap-phases-43-56.md)    |
| 52     | R08 config parsers + migration core                                              | Done        | [read](roadmap-phases-43-56.md)    |
| 53     | R09 password-manager sources                                                     | Done        | [read](roadmap-phases-43-56.md)    |
| 54     | R26 signed desktop + CLI updates                                                 | In progress | [read](roadmap-phases-43-56.md)    |
| 56     | T56 T56 behaviour tests (Rust + TypeScript)                                      | Done        | [read](roadmap-phases-43-56.md)    |

The release history is in the [changelog](https://unenverse.darthdemono.com/changelog/), and the decisions behind the phases are the [decision records](../adr/README.md).
