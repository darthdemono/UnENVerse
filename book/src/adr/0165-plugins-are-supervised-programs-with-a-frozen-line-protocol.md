# ADR-0165: Plugins are supervised programs speaking a frozen line protocol

Status: accepted

## Context

Users want destinations, notifiers and issuers UnENVerse does not ship. Loading code into the application would put the master key in reach of third-party code; a sandbox would promise more than a program the user installs can honestly be held to.

## Decision

A plugin is a separate program with a manifest, started by the host over stdin and stdout with a JSON-lines protocol, `unv-plugin-rpc/1` (handshake, plan, execute, reconcile, shutdown; see the Plugins page). The version is frozen: a change that breaks it is a new protocol name. The host gives a plugin a cleared environment, a private scratch directory, only the credential slots the user bound (as environment variables) and one operation's parameters; it never hands over the master key, other entries or signing keys, because none of them are in the process that starts it. Every call has a deadline, lines and stderr are capped, stderr is scrubbed, and a call without an answer kills the process and is reported as lost, so the caller reconciles instead of repeating. The program's SHA-256 is recorded at install and checked before each start.

Installing never enables. Enabling prints the declared capabilities and that the plugin runs with the user's rights, and needs a yes. The conformance suite (`unv plugin test`) is the contract a plugin must meet, and runs in CI against the example plugin and against deliberately faulty copies.

## Consequences

The documentation says plainly that plugins are not sandboxed. WASM isolation, a marketplace and automatic updates are out of scope. The Settings, Plugins screen and recording a plugin's hash in `ops_runs.plugin_provenance` for scheduled steps are still to come.
