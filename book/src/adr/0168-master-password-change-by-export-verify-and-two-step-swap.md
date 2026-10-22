# ADR-0168: Master password change by export, verify and a two-step swap

Status: accepted

## Context

The vault key is Argon2id over the master password and `vault.salt`, so changing the password changes two files, and two files cannot be replaced atomically. There was no way to change the password at all.

## Decision

`unv vault rekey` (module `vault_core::rekey`) holds a write lock through a second connection (`ATTACH` cannot run inside a transaction), copies the old `vault.db` and `vault.salt` to `*.pre-rekey`, writes a new salt as `vault.salt.new`, exports the data with `sqlcipher_export` into `vault.db.rekey-new` under the new key, opens that copy, checks integrity and that every table has the same row count, and only then renames the database and then the salt into place.

A stop at any step leaves a vault that opens with the old password or the new one, never neither. Every unlock first runs `recover_interrupted`: if `vault.salt.new` exists and the password opens the database through it, the database was already swapped and the salt rename is finished; otherwise the leftovers are removed. The `*.pre-rekey` copies are removed by the first successful unlock. The new password must be at least 12 characters and differ from the old one. The new password is read from `UNV_NEW_PASSWORD` or a prompt typed twice, never a flag.

## Consequences

Backups made before the change still open with the old password; that is what a backup is, and the documentation says so. Recovery packages are encrypted to an age recipient, not to the password, and are unaffected. The old password remains valid for the kept copy until the next unlock, and for any file copy anyone already took. Changing the password does not make old copies safe; the compromise playbooks say what to do then. The desktop has no Change master password action yet.

## Forget (same phase)

`unv entry forget NAME` (module `vault_core::forget`) deletes the entry's `vault_history` row and blanks the stored content of each config-history snapshot that lists the entry as exposed, marking it `forgotten`. The snapshot keeps its hash and chain link, and `verify` hashes the stored `sha256` for such a row, so the chain still verifies and an edit to the kept hash still breaks it. `config_history::get` refuses a forgotten snapshot, so a restore can never write an empty file. The save log tables hold hashes only and need no purge. It refuses while a rotation of the entry runs, and an `entry.forget` audit row records counts, never values. The current value is deliberately left alone: a live credential is rotated or deleted, not forgotten.
