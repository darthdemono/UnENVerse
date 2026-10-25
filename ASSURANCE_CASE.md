# Assurance case

This file argues that UnENVerse meets the security requirements it documents. It
builds on [THREAT_MODEL.md](THREAT_MODEL.md) (assets, threats, attack surface),
[ARCHITECTURE.md](ARCHITECTURE.md) (components and request flow) and
[SECURITY.md](SECURITY.md) (reporting and policies). Each claim names the place
that enforces it, so it can be checked.

## Security requirements

1. Vault contents are unreadable without the master password.
2. Secrets do not leak through output, logs or the UI unless the user asks.
3. A user, token or node can do only what its permissions allow.
4. Talking to a server or node cannot be intercepted or silently redirected.
5. Updates, catalogues and releases are authentic.
6. Tampering with history or the audit log is detectable.

## Trust boundaries

| Boundary                                                            | Untrusted side                                 | Where it is checked                                                           |
| ------------------------------------------------------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------- |
| Disk to process                                                     | `vault.db`, `vault.salt`, backups and archives | SQLCipher, Argon2id key derivation, hash-chained history                      |
| UI to Rust (Tauri IPC)                                              | Everything the webview sends                   | Handler list pinned by `tests/ipc-contract.test.ts`; checks inside commands   |
| Network to `unv-server`                                             | Every request                                  | Per-route permissions, rate limiting, TLS, signed node requests               |
| Process to third parties (issuers, SFTP, update and catalogue URLs) | Their responses                                | Pinned or verified TLS, pinned host keys, signed manifests and catalogues     |
| Input to parsers                                                    | Imported files and third-party exports         | Importers treat input as untrusted; fuzz targets in `fuzz/`                   |
| Source to release                                                   | Anyone able to push, or a tampered dependency  | Ruleset on `main`, locked dependencies, CI gates, keyless signing, provenance |

## Secure design principles

- **Fail-safe defaults.** Redaction is fail-closed: a field prints only if listed
  public (`out.rs`). The server binds `127.0.0.1` by default. Materialising
  paths are explicit and logged.
- **Complete mediation.** Permissions apply before filtering, and gates are
  enforced at the write, not only in the UI (CONTRIBUTING.md rule 8; each gate is
  verified by reverting it alone). `--dry-run` is enforced inside `Access::save`.
- **Least privilege.** Scoped users and tokens; a node never holds a vault key;
  workflows use job-level minimal permissions.
- **Economy of mechanism and a single implementation.** One Rust implementation
  is reached over IPC; where logic must exist twice, one golden fixture is
  asserted from both sides (CONTRIBUTING.md rule 10).
- **Open design.** The format and algorithms are public; security rests on the
  key, not on secrecy of the design.
- **Separation of privilege.** Widening changes are owner-only; node pushes can
  require a human approval signed on the owner's device.

## Common weaknesses and how each is countered

| Weakness (OWASP / CWE)                                     | Countermeasure                                                                                                                                                                                                          |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Injection and XSS (CWE-79, CWE-89)                         | Every vault field is escaped through the `html` tagged template; only `http(s):` URLs become links (CONTRIBUTING.md rule 4); a strict CSP in `tauri.conf.json`; SQLCipher access through parameterised `rusqlite` calls |
| Memory safety (CWE-119, CWE-416)                           | Rust for the core, CLI, server and desktop shell; no C of our own                                                                                                                                                       |
| Weak or broken cryptography (CWE-327)                      | Argon2id (m=65536, t=3, p=1), SQLCipher, Ed25519 signatures, SHA-256, rustls with TLS 1.2+ (TLS 1.3 only for nodes); no SHA-1 or unauthenticated modes in security mechanisms                                           |
| Improper certificate validation (CWE-295)                  | One verifier in `vault-core/src/tls.rs`; there is no "do not verify" policy and `danger_accept_invalid_certs` appears nowhere in the workspace; pins are never silently re-pinned                                       |
| Missing authentication or authorisation (CWE-306, CWE-862) | Per-route permission checks, owner-only routes, sessions that end when a user, token, password or permission changes                                                                                                    |
| Brute force (CWE-307)                                      | Rate limiting on failed server logins; Argon2id cost on the vault                                                                                                                                                       |
| Timing side channels (CWE-208)                             | Constant-time comparison for secrets                                                                                                                                                                                    |
| Sensitive data exposure (CWE-200, CWE-532)                 | Fail-closed redaction, one list of secret fields (`SECRET_FIELDS`), owner-only file modes for every written output                                                                                                      |
| Vulnerable dependencies (CWE-1395)                         | Committed `Cargo.lock` and `package-lock.json`, `cargo deny`, `npm audit`, OSV scanner, Dependabot, CodeQL                                                                                                              |
| Tampered release or update (CWE-494)                       | Keyless Sigstore signatures, build provenance, SBOMs, separately signed update manifest and catalogue                                                                                                                   |
| Log and history forgery (CWE-117)                          | Hash-chained audit log and config history that anchor their first row; `unv audit --verify`                                                                                                                             |

## Residual risk

Listed in the "Threats and mitigations" and "Not defended against" sections of
[THREAT_MODEL.md](THREAT_MODEL.md): a weak master password, root on an unlocked
machine, an upstream release that is malicious and not yet flagged, and a parser
bug that the weekly fuzzing has not found.

## Keeping it true

Update this file in the same change that alters a boundary, a mitigation or a
policy named above. A claim here without a matching test or setting is a bug in
this file.
