# ADR-0152: Vault schema changes are ordered and recorded

Status: accepted

## Context

The vault is a long-lived SQLCipher file. Historically, opening it ran a mix of `CREATE IF NOT EXISTS` statements and ignored `ALTER TABLE` failures. That made an interrupted or partial upgrade indistinguishable from an already-applied change.

## Decision

`schema_migrations` records each numbered migration, its name, app version and application time. Migrations run in one immediate transaction. Additive column changes first inspect `PRAGMA table_info`; an unexpected SQL failure is returned, never ignored.

The pre-R05 schema is migration 0001 and durable operations tables are migration 0002. The 0.42.x encrypted golden vault opens through both migrations in a test.

## Consequences

New tables and columns are explicit, reviewable and recoverable. A version may add migrations but must not rewrite an applied migration; correcting one means a later migration.
