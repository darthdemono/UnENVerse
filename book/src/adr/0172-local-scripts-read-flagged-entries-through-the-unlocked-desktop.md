# ADR-0172: Local scripts read flagged entries through the unlocked desktop

Status: accepted

## Context

Every `unv` call derives the vault key itself, so a script on the same machine as an unlocked desktop still asks for the master password (R18). Handing the password to scripts is worse than the problem.

## Decision

While the desktop is unlocked and Settings, Security, "Allow local scripts" is on, it listens on a Unix domain socket (mode 0600 inside a 0700 directory, `$XDG_RUNTIME_DIR/unenverse/bridge.sock`, else beside the vault). It stops and unlinks on lock, quit, restore and reset; a socket left by a crash is removed on the next start and a live one is never stolen. Off by default, and per entry: only entries with `local_scripts: true` (a public field) are served. The wire format is a 4-byte length and JSON, one request per connection, requests capped at 4 KiB.

The server answers one operation that matters, `snapshot`: the document reduced to the flagged entries (`bridge::served_vault`, the one place that decides). The CLI reads that snapshot as `Access::Bridge`, so `exec`, `render`, `get`, `env`, `curl`, `cookie`, `file` and `shield` work unchanged and every materialisation is recorded by the existing log. The bridge is read-only: `Access::save` refuses. It is used only when no password source was given and the command is one of those readers; `--no-bridge` opts out, `--allow-prompt` falls back to the password prompt when the desktop is locked or closed, and otherwise a locked or closed desktop is exit 7 within a second.

## Consequences

Any process of the same OS user can connect and read the flagged entries; that is the price of not typing a password, and why it is off by default. Not done, on purpose or for later: Windows named pipes (the type compiles, `serve` says it is unavailable), a per-use confirmation dialog, recording the peer process id, and separate `get_field`/`exec_env`/`render` server operations (the snapshot is already filtered, so they would add surface and no safety). An unflagged entry is simply absent, so naming it is "not found" (exit 3), not "denied" (exit 5).
