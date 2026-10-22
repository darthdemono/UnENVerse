# ADR-0168: Master password change by export, verify and a two-step swap

Status: accepted

## Context

The vault key is Argon2id over the master password and `vault.salt`, so changing the password changes two files, and two files cannot be replaced atomically. There was no way to change the password at all.

## Decision

`unv vault rekey` (module `vault_core::rekey`) holds a write lock through a second connection (`ATTACH` cannot run inside a transaction), copies the old `vault.db` and `vault.salt` to `*.pre-rekey`, writes a new salt as `vault.salt.new`, exports the data with `sqlcipher_export` into `vault.db.rekey-new` under the new key, opens that copy, checks integrity and that every table has the same row count, and only then renames the database and then the salt into place.

A stop at any step leaves a vault that opens with the old password or the new one, never neither. Every unlock first runs `recover_interrupted`: if `vault.salt.new` exists and the password opens the database through it, the database was already swapped and the salt rename is finished; otherwise the leftovers are removed. The `*.pre-rekey` copies are removed by the first successful unlock. The new password must be at least 12 characters and differ from the old one. The new password is read from `UNV_NEW_PASSWORD` or a prompt typed twice, never a flag.

## Consequences

Backups made before the change still open with the old password; that is what a backup is, and the documentation says so. Recovery packages are encrypted to an age recipient, not to the password, and are unaffected. The old password remains valid for the kept copy until the next unlock, and for any file copy anyone already took. Changing the password does not make old copies safe; the compromise playbooks say what to do then. The desktop has no Change master password action yet.
