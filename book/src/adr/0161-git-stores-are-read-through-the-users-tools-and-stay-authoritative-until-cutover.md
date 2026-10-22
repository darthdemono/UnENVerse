# ADR-0161: Git stores are read through the user's tools and stay authoritative until cutover

Status: accepted

## Context

SOPS files, a `pass` store and a gopass store hold secrets as ciphertext in a Git repository. Their keys (age, PGP, a cloud KMS) belong to tools the user already trusts and configures. A password-manager export is a one-time move; a Git store is usually still in use after the import, edited by other people and other machines. Two questions follow: who decrypts, and who owns the value afterwards.

## Decision

UnENVerse never reimplements age, PGP or a key service. It runs the user's own `sops`, `gpg` and `gopass` through one adapter (`tool_exec`): arguments are passed as an array and never through a shell, the child gets a cleared environment plus only the names the caller allows (`GNUPGHOME`, `SOPS_AGE_KEY_FILE` and similar), a deadline kills a tool waiting on a passphrase prompt, stdout has a size cap, both pipes are drained at once, and anything in stderr that looks like a secret is replaced before it can reach a report. The tool versions are recorded in the report. The source files are only read; the import records a manifest of file hashes instead of a copy of the store.

After an import an entry records `authority.owner = "source"`. For a Git store the source stays authoritative: the storage layer refuses an edit to a secret field of such an entry (the error names the source and says to change it there or cut over), tags, notes and categories stay editable, and `unv migrate sync` refreshes values from the source one way. `unv migrate drift` compares the source with the last import and imports nothing. `unv migrate cutover` makes the vault authoritative, records a lifecycle event for each entry and stores a digest of each secret; `uncutover` hands authority back, and refuses when a secret no longer matches the digest, because handing authority back would discard the newer value.

A password-manager export (Bitwarden, 1Password, KeePass) is not source-owned afterwards. It is a move, and an edit that disagrees with the export is reported as a conflict at the next import, as before.

## Consequences

The write protection lives in `vault-core`'s save, so the desktop app, the CLI and the server share it, and the server answers 409. Scheduled `import.sync` and `import.drift` runs in the operations engine are not part of this change: the engine opens the vault through the process-wide database path, which a scheduler running beside a test vault cannot redirect. They follow with the operations work on the desktop hub. Git history import for `pass` is not implemented; the report says the working tree was read.
