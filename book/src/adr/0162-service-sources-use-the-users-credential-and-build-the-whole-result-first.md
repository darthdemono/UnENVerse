# ADR-0162: Service sources use the user's credential and build the whole result first

Status: accepted

## Context

OpenBao, Infisical, Bitwarden Secrets Manager and AWS hold secrets behind APIs. An import has to authenticate, page through lists that can be long, tolerate paths the credential may not read, and not leave a half-applied vault when the network drops.

## Decision

`unv import-service` reads a service with a credential the user already holds. The credential comes from the environment or from a vault entry named on the command line (`--token-entry`, `--identity-entry`), never from an argument that would reach `ps` and shell history, and it is never printed or echoed in an error. HTTP uses the verified-TLS issuer client with redirects disabled, so a token is not forwarded to wherever a redirect points.

Every reader builds its complete result in memory before anything reaches the vault. A listing that fails halfway is an error and nothing is applied; a path the credential may not read is reported as a warning and skipped. The result then goes through the same planner and single SQLCipher transaction as a file import: source identities and archives with it, a named conflict instead of a silent overwrite, and a rerun of an unchanged source is a no-op.

OpenBao KV v2 is paged with `limit` and `after`, every live version is read, and older versions become history records with the source's own times; destroyed and deleted versions are reported. Mounts, policies and engines are archived as non-operational provenance, and dynamic engines are named in the report, not imported. Infisical values are imported expanded; the reference graph is recorded as text without the `${...}` syntax so nothing stored can be mistaken for a UnENVerse reference. Bitwarden Secrets Manager is read only through the user's `bws` program, with the token in that child's environment for the call, and the Bitwarden SDK is never linked because its licence is not OSI approved. AWS Secrets Manager and Parameter Store are behind the `aws` cargo feature, off by default and labelled as a commercial service. No AWS SDK is linked: the two JSON APIs are called directly and signed with Signature Version 4, implemented here and checked against Amazon's published test vector, using static keys from the environment or a vault entry.

## Consequences

Server and API versions are recorded in the report because these APIs drift. The OpenBao, Infisical and AWS readers are tested against local mock servers built from the documented responses, not against a live account: the first run against a real instance may find a field the mock does not have, and the report would say so. A one-way publish target (writing UnENVerse values back to OpenBao KV) is not part of this change.
