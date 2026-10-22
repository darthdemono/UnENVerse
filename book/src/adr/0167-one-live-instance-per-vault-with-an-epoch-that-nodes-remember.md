# ADR-0167: One live instance per vault, with an epoch that nodes remember

Status: accepted

## Context

A restore from a recovery package, or a copy made by a sync folder, produced a second vault with the same hub identity and the same nodes, and nothing said which of the two was allowed to act.

## Decision

The vault stores a `vault_id` (for life), an `instance_id`, an `instance_epoch` and an `instance_role` in `vault_meta` (migration 5). A restore leaves a `vault.restored` marker beside the database; the next time the database is opened the vault becomes a new instance in `standby`. A standby instance reads and verifies; the hub answers its nodes as locked and records nothing, the operations executor and scheduled runs do nothing, and a rotation is refused. `unv vault activate` asks whether the old instance is retired, sets the role to `active` and raises the epoch by one.

Every hub reply carries `(vault_id, epoch)`. A node keeps the highest epoch it has seen for its vault and refuses a lower one before it acknowledges anything, and refuses a hub for another vault. A hub or node that predates the field is unaffected, which is why nodes are upgraded first. A role or epoch that cannot be read counts as standby, the safe side.

## Consequences

A forgotten clone cannot push over the live instance once the live one has been activated at a higher epoch. It cannot stop a clone that was never activated from acting if the original is also still active at the same epoch; the cure for that is the warning in `doctor` about synced folders and the documentation, not code. Activation is a command-line act for now; the desktop has no button for it.
