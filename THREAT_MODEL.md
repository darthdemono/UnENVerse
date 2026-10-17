# Threat model and attack surface

This is the project's own assessment. It is not a third-party audit. The public
summary is the [security page](https://unenverse.darthdemono.com/security/), and
the parts and actors are described in [ARCHITECTURE.md](ARCHITECTURE.md).

## Assets

1. The secrets in the vault, and the master password and key that protect them.
2. The integrity of the audit log and config history.
3. The integrity of releases and of the update and catalogue feeds.
4. Files written to other machines by nodes.

## Threats and mitigations

| Threat                                                   | Mitigation                                                                                                                           | Residual risk                                                                           |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| Someone obtains the disk or a copy of `vault.db`         | SQLCipher encryption; key from Argon2id (m=65536, t=3, p=1) and a random salt; key never written to disk, cleared on lock            | A weak master password is guessable offline                                             |
| Secrets leak through CLI output, logs or scripts         | Redaction is the default and fails closed (a field prints only if listed public); materialising paths are explicit and logged        | Anything copied to the clipboard, a file or a process environment is out of our control |
| A scoped user or token reads or writes beyond its rights | Permissions applied before filtering; owner-only routes for widening changes; gates enforced at the write, not only in the UI        | A bug in a permission rule; covered by chaos and permission tests                       |
| Brute force against the server login                     | Rate limiting on failed attempts; sessions end when a user, token, password or permission changes                                    | Distributed guessing of a weak user password                                            |
| Tampering with history                                   | The audit log and config history are hash-chained; `unv audit --verify` and `unv doctor` detect edits or deletions                   | An attacker with the key can rewrite from the start                                     |
| A compromised hub pushes a malicious file to a node      | Node requests are signed; a node can require a human approval bound to the exact bytes, signed on the owner's device                 | A node configured to accept pushes without approval                                     |
| Man in the middle on the server connection               | TLS 1.3 only; clients pin the server certificate fingerprint and never silently re-pin                                               | The first connection if the fingerprint is not checked                                  |
| Hostile vault data or imported files                     | Every field is escaped before display; only `http(s)` links; importers treat input as untrusted and are fuzzed                       | A parser bug; fuzzing is manual today                                                   |
| A malicious or tampered update or catalogue              | Updates install only after a separately signed manifest, archive checksum and version probe pass; the catalogue is signature checked | Compromise of the signing keys                                                          |
| Supply chain: tampered release or dependency             | Keyless Sigstore signatures, build provenance and SBOMs on releases; locked dependencies; `cargo deny`, `npm audit`, CodeQL in CI    | A malicious but unflagged upstream release                                              |

## Not defended against

Root on a running machine while the vault is unlocked (the key is in memory), a
forgotten master password (there is no recovery service), and files you copy out
of the program.

## Attack surface

These are the places input crosses a trust boundary. Each gets validation and
tests; changes to them need the matching test.

1. **CLI arguments, stdin and files** read by `unv`, including importers and
   migrators for third-party formats (fuzz targets in `fuzz/`).
2. **Desktop IPC.** Every Tauri command is checked against the handler list by a
   contract test; the UI never trusts vault data.
3. **The `unv-server` HTTP API** (`/api/*`), including the token-addressed
   calendar feed, with TLS, rate limiting and per-route permissions.
4. **Node routes** (`/api/nodes/*`) and the node agent's local config and files.
5. **Update and catalogue downloads**, verified against pinned keys.
6. **Recovery destinations**, including SFTP with pinned host keys.
7. **Local files**: the database, salt and session files (owner-only on Linux;
   `unv doctor` reports where Windows cannot enforce this).
8. **The build and release pipeline**: workflow permissions are minimal, untrusted
   input is never interpolated into shell steps, and a release needs a green CI
   run on the same commit.

## Review

Re-read this file when adding a route, a command that materialises values, a new
importer, or a new network destination. Changes to security-relevant code follow
the checks in [CONTRIBUTING.md](CONTRIBUTING.md).
