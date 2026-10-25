# ADR-0171: Leak-scan import and ACME certificates through installed tools

Status: accepted

## Context

R17 asked for two upstream tools to be tried and kept or dropped on evidence: gitleaks, to find secrets left in plain text, and lego, to get certificates from an ACME server. Neither may become a dependency of the vault, and neither may see more than it needs.

## Decision

`unv migrate leaks <dir>` runs the user's own gitleaks (8.19 or newer, `dir` mode) through `tool_exec` and offers each finding as a migration candidate. It reuses the migration ledger, encrypted archives and template mode, so a repeat is a no-op and a conflict refuses the whole import. A scan prints rule, file, line and a short fingerprint, never a value, and nothing is written without `--apply`. A secret that is not in the file as written (gitleaks also decodes base64) is reported as unsupported, never guessed. History-only findings are out of scope: there is nothing to cut out, rotate instead. TruffleHog stays excluded (AGPL, live verification of found credentials).

`unv acme issue|renew|check` runs the user's own lego (5 or newer; the 5.x flags differ from 4.x) in a private scratch folder, with the DNS token handed over as one environment variable taken from a vault entry. The result is a `certificate` entry holding the full chain and the key, with the dates read from the certificate the CA signed. `renew` does nothing until the certificate is inside its window, so it is safe from a timer. `check` handshakes with a host and calls a different certificate a conflict. The ACME directory flag is `--directory`, because `--server` is the global flag for a remote vault and clap merges arguments that share an id.

## Evidence

gitleaks 8.30.1: found and imported real findings in a test folder (`real_gitleaks_finds_and_imports`). lego 5.5.2 against Pebble, the Let's Encrypt test CA: issued a certificate over HTTP-01 and the entry matched it (`pebble_issues_a_real_certificate`, opt-in with `UNV_PEBBLE=1`). Both are kept behind explicit commands.

## Consequences

Limits: gitleaks sees the working tree only and trusts a `gitleaks.toml` in the scanned folder. lego gets a new ACME account on every run, because an account key would otherwise rest in plaintext. Renewal is not an ops schedule yet: the ops executor has a connection but no path-aware `Access` to write the vault, the same blocker as scheduled git sync in R10, to be solved with the desktop hub in R12. Meanwhile `unv acme renew` runs from cron or a systemd timer. Deploying a certificate uses the existing node apply with references to `certificate_data`. The Pebble test is not in CI; adding it needs a run by hand on a branch first.
