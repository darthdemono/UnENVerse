# Sources we import from

The lists below are written from the command definitions by `scripts/formats-status.mjs`; a test fails when they are stale. They say what the command accepts, not that every source has been tried against a live product: the importers are tested against fixtures and, for the services, mock servers (and, weekly, OpenBao and LocalStack).

<!-- formats:start -->

- `unv import-vault <source> <file>` reads: `bitwarden`, `onepassword`, `keepass`, `sops`, `pass`, `gopass`, `proton`, `nextcloud`.
- `unv import-service <service>` reads: `openbao`, `infisical`, `bws`, `aws-sm`, `aws-ssm`.

<!-- formats:end -->

How long a vault, backup or package format stays readable is a separate promise: see [Formats we read](format-policy.md).
