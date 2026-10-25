# ADR-0169: The env naming schema, from one key to an organisation

Status: accepted

## Context

Every secret becomes an environment variable, and the name is the contract between the vault and whatever reads the file. The template was `[PREFIX_]PROVIDER[_KEYID][_VERSION][_LABEL][_ROLE]`, with the role optional. That left four gaps: a bare `GITHUB=` could be an id, a key or a token; two entries that differed only by the account they belonged to collided; a password's sign-in name was not generated at all; and nothing stopped one entry or one provider from producing the same name twice.

## Decision

A name is a path from the widest scope to the narrowest, and each segment is optional so the same schema serves a single key and a large estate.

| Segment  | Field                                           | What it answers                                              | Typical scale |
| -------- | ----------------------------------------------- | ------------------------------------------------------------ | ------------- |
| prefix   | `env_prefixes[0]` (when "include prefix" is on) | which application or namespace reads this                    | team and up   |
| provider | `provider`                                      | which service                                                | everyone      |
| key id   | `key_id`                                        | which of several credentials for that service                | a few keys    |
| version  | `version`                                       | which generation (`v3`)                                      | rotation      |
| label    | `label`                                         | which environment, region, tenant or purpose (`prod`, `eu1`) | organisation  |
| role     | `primary_role`, an additional value's key       | what the value is (`key`, `id`, `password`, `user`)          | everyone      |
| account  | `account_name`, appended only to break a tie    | which login                                                  | shared sites  |

Examples: `GITHUB_KEY` (one key); `GITHUB_2_KEY` (second key); `ACME_DB_PROD_EU1_PASSWORD` is prefix `ACME`, provider `DB`, label `prod_eu1`, role `password`.

Rules:

1. **Every secret with a primary value has a value role.** A new entry starts on `key` (API key), `password`, `value` for an environment variable (the template omits `value`, so the variable keeps its own name) or the type name. The form and `unv entry add` both apply this; a new entry cannot be saved without one. An entry stored before roles existed, and never given one, keeps its bare name so deployed files keep working, and is only held to the rule once it has a role.
2. **A role appears once per entry.** The primary role, the legacy second secret (`SECRET`), `URL`, `USER` and every additional value are compared case-insensitively; the form refuses a save that generates one name twice.
3. **A name appears once per provider.** Two entries may share a provider and role only when a key id, version, label, account or key pool tells them apart; otherwise the save is refused with the name and the entry that owns it.
4. **Account is the tie-breaker, not a segment.** When a copy contains several entries with the same name and each has its own account, every one gets its account appended (`ELTE_PASSWORD_DARTHDEMONO_GMAIL_COM`). Entries with no account, or the same one, fall back to the ordinal (`_2`).
5. **A password generates its sign-in name first.** `ELTE_USER` precedes `ELTE_PASSWORD`; the card shows the username above the password and names the variable on hover.
6. **Segments are normalised alike on both sides.** Upper case by default, anything outside `A-Z0-9_` becomes `_`, a leading digit gains `_`; the TypeScript and Rust implementations are held to one fixture.

Field audit, with the stored name unchanged in every case (renaming a stored field breaks vaults, the CLI and exports):

| Field                       | Decision                                                                                                                                                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `api_url`                   | Relabelled per type by `fieldSchema`: Site URL (browser session, certificate), Login URL (password), Server URL (database, connection string, Usenet), Registry URL, Service URL, Tracker URL, Keyserver URL, and so on. |
| `callback_url`              | Offered for API keys and OAuth clients only.                                                                                                                                                                             |
| `scopes`                    | Offered for API keys, OAuth clients and registry tokens only.                                                                                                                                                            |
| rate limit                  | Offered for API keys, OAuth clients, registry tokens and local services only.                                                                                                                                            |
| `api_secret`, `secret_role` | Replaced by additional values with a role (secret, id, token, password, username, url or a custom one). Shown only on entries that already hold one.                                                                     |
| `username`                  | Kept; now generated as `USER` and shown first.                                                                                                                                                                           |

A field is hidden only while it is empty. An entry that holds a value in a hidden-by-type field shows it anyway.

## Consequences

A copy of an entry with a username gains one line, `PROVIDER_USER`. An entry that relied on a bare name keeps it until it is given a role. Names longer than the 255 characters some systems cap are not refused; the preview is where to notice them. An environment segment is not part of the schema: a label carries `prod` or `eu1`, and keeping one field for it avoids two ways to say the same thing.
