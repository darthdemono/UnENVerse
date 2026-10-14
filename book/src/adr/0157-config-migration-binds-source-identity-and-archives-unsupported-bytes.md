# ADR-0157: Config migration binds source identity and archives unsupported bytes

Status: accepted

## Context

Importing a configuration file by variable name loses the distinction between two files that both contain `TOKEN`. Replacing a stored value from a later import can also erase a local edit. Comments, extension fields and unknown blocks are operational context even when UnENVerse cannot model them. Keeping an unencrypted copy next to the vault would defeat local encryption.

## Decision

R08 records a stable source id, a source value hash, the mapped entry id and the entry revision in SQLCipher. A repeat compares all three: unchanged source values skip, a changed source updates only when the mapped entry remains at the saved revision, and concurrent source and vault changes are named conflicts.

`migrations`, `migration_items`, `source_archives` and `vault_blobs` are an ordered additive schema migration. Apply writes the vault document, migration manifest and encrypted archive rows in one SQLCipher transaction. Reader output is redacted before it reaches CLI output or desktop IPC.

Template mode replaces imported value spans with `${UNV_MIG_...}` references. It escapes existing `${` as `$${`, retains source representation metadata inside SQLCipher, and verifies the rendered template against the original full SHA-256. Non-template imports archive comments and unsupported source bytes with provenance instead of dropping them.

## Consequences

Migration has a durable reconciliation base for the password-manager readers in R09-R11. A source archive may contain private paths or values, so it is exportable only to a user-selected private file and never printed. The desktop preview receives names and parser statuses only; CLI apply owns the local-file and transactional archive path.
