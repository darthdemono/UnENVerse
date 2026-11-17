# ADR-0175: The login sequence is decided once and carries no values

Status: accepted

## Context

One account can hold up to four credentials: the password, a 2FA code, a security phrase and a recovery code. A page asks for them in a fixed order, whatever order the entries were added in. The CLI, the desktop card and a future browser host all need that order (R36).

## Decision

`vault_core::login::login_sequence` is the only place the order is decided: password, 2FA, security phrase, recovery code. A step with nothing stored is absent, not empty. The result is a plan of `{step, label, entry_id, field}`; it names entries and fields and never carries a value, so printing or sending a plan cannot leak a secret. `unv login-plan BUNDLE` prints it.

What happens to a recovery code after it is filled is the bundle's `terminate_codes` setting: `ask` (default, "Mark this code as used?"), `auto` or `never`. Unset or unreadable means `ask`. Reading the next code never consumes it; only `unv codes use` does, and `unv codes unuse` puts one back after a page rejected it.

## Consequences

A caller that wants a different order is a bug in the caller. The setting is stored and printed today; nothing fills a page or marks a code until the browser extension exists (R37), so the setting has no effect before then.
