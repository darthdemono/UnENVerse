# ADR-0153: One unlocked executor holds a fenced lease

Status: accepted

## Context

Scheduled recovery packages must not run twice when the CLI and desktop app, or two app instances, share a vault. A PID alone is not a durable authority after a crash, and a background worker owns a copy of the derived vault key.

## Decision

The desktop starts one executor only after unlock. It owns an advisory file lock and records a monotonically increasing lease epoch in `ops_lease`; state writes check that epoch. Runs and schedule occurrences live in SQLCipher tables and use an explicit guarded state graph. On lock or reset the executor receives its stop signal and joins before any vault key is zeroized; in-flight work becomes paused.

Schedules support interval and IANA-zone daily rules. Several missed intervals coalesce into one run with a count; a run later than its configured maximum becomes `skipped_stale` instead of silently running old work.

## Consequences

Automation is deliberately not a daemon: it runs only while an unlocked desktop app remains open. A second executor names the recorded holder and exits unavailable rather than racing the first.
