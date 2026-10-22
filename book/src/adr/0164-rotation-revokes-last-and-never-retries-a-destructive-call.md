# ADR-0164: Rotation revokes last and never retries a destructive call

Status: accepted

## Context

Rotating a credential has three dangers: revoking the old credential before the new one works, losing a new value the issuer created but never delivered, and repeating a call that already took effect.

## Decision

`vault_core::rotation` is one state machine that knows nothing about any issuer. Issuers arrive as a connector (`create`, `set_value`, `roll`, `revoke`, `verify`, `reconcile`) and are sorted into three classes: `create_overlapping` (both credentials exist at once), `set_value_client_generated` (UnENVerse chooses the value) and `roll_provider_generated` (the old value dies when the new one is issued).

The old credential is revoked last, and only when every required consumer was deployed to and checked in the same run. A value the client chooses is stored in `rotation_pending` before the issuer hears of it, and the new value is written to the entry as the next generation before it is deployed. Every transition is saved, so a crash or a lock resumes at the step in flight. A response lost after a destructive call is `reconcile`d against the issuer (find the token by its name, ask whether the old one is gone) and the step is repeated only when the answer is "not done". A lost in-place roll cannot be reconciled and stops as `needs_attention`. A property test runs every combination of step failures and asserts a revoke never precedes the checks.

## Consequences

The Cloudflare connector (`create` by default, `roll` only with `--allow-roll`) is the first issuer and `unv rotate plan|run|status` drives it. Without consumers a run ends `revocation_pending` unless `--revoke-now` says nothing else uses the old token. PostgreSQL, node targets as consumers, and a review screen are still to come.
