# ADR-0173: The desktop runs the hub while unlocked and shares its decision logic

Status: accepted

## Context

Deploying a rendered config to a node needed `unv-server --nodes`, a second program holding a session key. Most people who own one vault and a few hosts have only the desktop (R12). The security-sensitive part of Nodes is what a node is sent: which bytes are rendered, whether a person said yes to exactly those bytes, whether an automation rule may stand in, and what is recorded. That code must not exist twice.

## Decision

`unv_app::hub` holds that logic once: `process` (answer one beat), `poll_node` (dial one listening node and answer it), and the small helpers they need. A host supplies four things through the `HubHost` trait: the owner's vault key (a session on the server, the unlocked app on the desktop), the vault file, the node registry, and the pull uploads an owner asked for. `unv-server` implements the trait over its `AppState` and keeps thin `process` and `poll_node` wrappers with their old names; the desktop implements it over its unlocked state.

The desktop starts a `Poller` when the vault unlocks and stops it when the vault locks, resets or is replaced, so a locked app holds no key and dials nobody. It dials listening nodes (`/node/v1/poll` then `/node/v1/act`, pinned and signed as before); it opens no port, so enrolling a listening node on the desktop does not use the server's enrol route. The Nodes pane for a local vault sends its usual `/api/nodes*` requests through `nodes_local_request`, answered by `unv_app::hub_owner` from the same registry and audit chain; what needs a server (pulling a file a node uploads) answers 404 with the reason.

A target lease (`NodeStore::lease_target`) pins a target to the content hash a rotation is deploying for at most an hour, held and released only by the same holder; while one lives the hub pushes that target nothing else. What a node reports is written to `deployment_state` against the active credential generation, found through the history snapshot of the same bytes, so no value is read.

## Consequences

Approval semantics are unchanged and tested once. Not done: a deployment operation kind in the operations engine, reconciliation of an "uncertain" step after a lock (the node's next beat reports what it holds, so the hub re-pushes if it differs, but no step state is kept), and the desktop's enrolment path for a listening node (the owner pastes the node's key and listen address, which needs a node-side command for the response).
