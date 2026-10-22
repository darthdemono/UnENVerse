# ADR-0163: Approvals gain a kind, and automation can approve a rule, never a person

Status: accepted

## Context

A node can already refuse any push that does not carry a signed approval for exactly its bytes. That approval stands for a person. Deploying from the desktop with rotations needs an approval an automation can give for an operation that follows a written rule, without weakening what a person's approval means.

## Decision

Approval tokens have a second version, signed under its own domain (`unv-approval-v2`, where version 1 uses `envv-approval-v1`), so a signature made for one version never verifies as the other. Version 2 adds the kind (`human`, `device` or `rule`), an operation id, the credential generation and policy revision it was given under, and for a rule the rule's id. Version 1 tokens still verify and read as `human`, or `device` when signed on an owner's device.

A node's own configuration decides what it accepts, and the hub cannot change it. A target with `require_approval = true` needs a person: a `human` or `device` token, from the pinned hub key or the configured `approver` device, and a `rule` token never satisfies it, however well signed. A target with `accept_rule_approval = true` needs `automation_approver` (the automation public key) at the top of the node's config and takes a `rule` token signed by that key, or a person's approval, which is the stronger yes. The two settings cannot be combined. A target that sets neither applies a hub push as it always did and consults no token.

The hub keeps a separate automation seed in the vault's encrypted metadata, so it can be rotated and pinned apart from the approval key, and shows only the public half (`unv node automation-key`). A rule token lives five minutes, names its node, target, bytes and operation, and carries a fresh nonce that the node remembers once it has written, so it cannot be replayed.

## Consequences

An automation key that is compromised can approve only for nodes that opted into rule approvals, and only for minutes; it can never approve a target that asks for a person. Removing `automation_approver` from a node's config turns rule approvals off on that host without involving the hub. The desktop does not yet embed a hub, so deployments from the desktop and leases that stop an automatic push regressing a target during a rotation are still to come.
