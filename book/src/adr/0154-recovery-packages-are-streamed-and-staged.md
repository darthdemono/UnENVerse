# ADR-0154: Recovery packages are streamed and staged

Status: accepted

## Context

A recovery artifact needs the SQLCipher vault, salt and selected local recovery material without copying the vault into process memory or risking a partial restore. Operators also need to inspect a package before possessing its recovery identity.

## Decision

`.unvpkg` v1 writes a small cleartext, non-secret envelope manifest followed by an age X25519 encrypted tar stream. The encrypted tar repeats the manifest and contains a SQLite online-backup snapshot, salt, node/LAN material and the materialisation log. `approver.key` is categorically excluded.

A destination must contain an UnENVerse marker. Writing uses a unique `.unvpkg.part`, file sync, rename, directory sync, completion receipt and read-back hash verification. Restore decrypts and validates every path, hash and byte count into a sibling staging directory, then installs only into an empty target. Verification checks hashes, SQLite, SQLCipher, storage, audit, configuration history, entry count and vault version.

## Consequences

`recover inspect` exposes package metadata but not vault contents. R06 supersedes the identity-storage detail for an unlocked restore drill in ADR-0155: the external private recovery identity remains necessary for recovery on a new machine, while an encrypted internal copy is never output. A bad identity, tampered package, missing marker or failed write leaves no accepted recovery package or partially populated target.
