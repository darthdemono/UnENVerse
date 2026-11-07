# ADR-0173: The desktop runs the hub while unlocked and shares its decision logic

Status: accepted

## Context

Deploying a rendered config to a node needed `unv-server --nodes`, a second program holding a session key. Most people who own one vault and a few hosts have only the desktop (R12). The security-sensitive part of Nodes is what a node is sent: which bytes are rendered, whether a person said yes to exactly those bytes, whether an automation rule may stand in, and what is recorded. That code must not exist twice.

## Decision

`unv_app::hub` holds that logic once: `process` (answer one beat), `poll_node` (dial one listening node and answer it), and the small helpers they need. A host supplies four things through the `HubHost` trait: the owner's vault key (a session on the server, the unlocked app on the desktop), the vault file, the node registry, and the pull uploads an owner asked for. `unv-server` implements the trait over its `AppState` and keeps thin `process` and `poll_node` wrappers with their old names; the desktop implements it over its unlocked state.

The desktop starts a `Poller` when the vault unlocks and stops it when the vault locks, resets or is replaced, so a locked app holds no key and dials nobody. It dials listening nodes (`/node/v1/poll` then `/node/v1/act`, pinned and signed as before); it opens no port, so enrolling a listening node on the desktop does not use the server's enrol route. The Nodes pane for a local vault sends its usual `/api/nodes*` requests through `nodes_local_request`, answered by `unv_app::hub_owner` from the same registry and audit chain; what needs a server (pulling a file a node uploads) answers 404 with the reason.

A target lease (`NodeStore::lease_target`) pins a target to the content hash a rotation is deploying for at most an hour, held and released only by the same holder; while one lives the hub pushes that target nothing else. What a node reports is written to `deployment_state` against the active credential generation, found through the history snapshot of the same bytes, so no value is read.

## Consequences

Approval semantics are unchanged and tested once. Three things first left out were done in the next release:

- **Pairing.** A hub that opens no port cannot take the node's call to the enrol route, so the owner carries two messages by hand. `unv node pair` on the node makes its identity and certificate and prints a request of public values; the hub's owner pastes it into the Nodes pane (or `unv node pair-accept` against a server), which mints and spends a 30 second token inside one store call (`NodeStore::pair`, so every enrolment check applies) and returns the node id and the hub's transport key; `unv node pair-finish` writes the same state file an enrolment would. The fingerprint is shown on both sides for the owner to compare.
- **Deployment runs.** A push that is sent or held opens a `deploy.push` run in the operations engine, keyed by node, target and bytes. The node's result settles it; a failure waits in `retry_wait` and fails after five sends; new bytes for a target cancel the older run; a run open more than a week is cancelled.
- **Uncertain steps.** A lock leaves the run paused and its step uncertain (`mark_interrupted`). The node's next beat names the bytes it holds: if they are the run's, the run succeeded even though its result was lost; if not, the push is sent again as the same run.
