# ADR-0156: Credential generations use vault-scoped fingerprints and bindings use entry ids

Status: accepted

## Context

The value history on a JSON entry says only that an old value existed. It cannot safely identify a repeated value, record issuance precision, attach a deployment to the generation it used, or survive a provider rename in a config reference. A plain SHA-256 fingerprint would make equal secrets comparable across vaults.

## Decision

R07 adds relational lifecycle tables beside the row-per-entry document. Each secret-bearing field receives a monotonically increasing credential generation. Fingerprints use HMAC-SHA-256 under a key derived from a random root held in SQLCipher `vault_meta`, scoped to that vault and field. Existing secrets seed generation one with `source = migration` and an unknown issue date.

Optional entry metadata remains in the JSON document for forward compatibility. Redacted output keeps only documented non-locating metadata and masks adapter data, attachment contents and provenance locators.

A project binding has a human name and immutable `entry_id`, with an optional field. The resolver uses an exact binding before the legacy provider-name resolver. Missing or duplicate bindings fail closed, and provider rename cascades continue to rewrite only legacy text references.

## Consequences

Lifecycle timelines contain actions and generation numbers, never secret material. Future rotation, deployment and adapter work has stable tables to query. Config history can retain a per-project set of row revisions and skip unaffected renders after a save. The name based `${Provider}` grammar remains supported for older vaults, but new bindings avoid its rename hazard.
