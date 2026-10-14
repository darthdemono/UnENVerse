# ADR-0155: SFTP host keys are pinned and drills use an encrypted identity

Status: accepted

## Context

Recovery packages must leave the local machine and prove that a current package
can be restored. SFTP is a broadly available off-machine transport, but a
password or private key does not authenticate the server. The original
one-time recovery identity design also made an unattended restore drill
impossible: the executor has the SQLCipher master key while unlocked, but not
the age private identity.

## Decision

Each SFTP destination records the explicitly confirmed SSH SHA-256 host-key
fingerprint. Setup first performs a credential-free host-key probe and displays
the fingerprint. Persisting the destination requires that exact fingerprint,
and every authenticated connection compares it before sending a vault
credential. A mismatch reports both fingerprints and is refused. There is no
automatic re-pin path.

The recovery writer uses a shared destination interface. It uploads a random
`.part` name, asks SFTP for `fsync@openssh.com` when advertised, uses a unique
final name after checking it does not exist, writes a completion receipt only
after a full package read-back SHA-256 and size verification, and reads the
receipt back as well. Baseline SFTP has no exposed no-overwrite rename, so the
unique UUID final name is the collision defense.

The recovery identity is still displayed once for the owner to retain outside
the vault. A second copy is stored only in SQLCipher `vault_meta` and is never
an output value. The unlocked executor uses that copy in memory for a temporary
restore drill, then removes the restored data. Retention chooses only receipts
with this destination ID and always retains the newest verified receipt.

## Consequences

An unlocked vault contains enough material to exercise its own recovery path,
which is appropriate because an unlocked vault already grants full secret
access. A database-only compromise remains insufficient without the SQLCipher
key. The external identity remains necessary for recovery on a new machine and
must still be stored independently.

Changing an SFTP host key is an intentional operational event: the owner must
review the new fingerprint and create or update the destination explicitly.
SMB and NFS remain local destinations, so their cache and flush semantics are
outside the application's control; scheduled restore drills detect the practical
failure mode.
