# Phase log 73: R06 SFTP, retention and restore drill

## Goal

Complete roadmap R06, project phase 50: move recovery packages to a pinned
off-machine SFTP destination, retain only verified recovery points, and prove
the latest package restores without activating it.

## Current Progress

Complete, uncommitted work. `unv-app::destinations` defines the six-method
destination contract and implements local storage and pure-Rust `russh` plus
`russh-sftp` transport. Package creation writes encrypted local staging data,
uploads a random `.part`, requests SFTP fsync when offered, renames to a unique
name, downloads and hashes the package, then writes and reads back its
completion receipt.

SFTP setup first reads only the server host key. The CLI requires the observed
SHA-256 fingerprint in `--host-key`; Settings shows a confirmation dialog and
passes the exact accepted value. Every authenticated use compares the pin and
reports both old and presented fingerprints on mismatch. Credentials come from
the selected entry's `api_key` field and are not serialized into destination
configuration.

Retention defaults to daily 7, weekly 4 and monthly 6. It selects only receipt
pairs that carry the exact destination ID, preserves the newest verified
receipt, and removes the receipt before its paired package so an interruption
cannot leave an incomplete artifact looking complete.

`unv backup drill --dest NAME` and Settings run a temporary download, restore
and full core verification with the unlocked master key. R06 stores a
SQLCipher-encrypted recovery-identity copy only for that unlocked drill;
`backup recovery-key` still displays the external identity exactly once. The
operation record reports package size and read-back verification or the drill
outcome. `backup schedule drill --dest NAME` creates a monthly schedule,
defaulting to the first day at 03:00 UTC.

Focused verification completed before the final full preflight:

- `cargo test -p unv-app recovery_ops --lib`: 8 passed.
- `cargo test -p unv-app destinations --lib`: 3 passed with Docker gate off.
- `UNV_SFTP_CONTAINER_TEST=1 cargo test -p unv-app docker_sftp_conforms_and_refuses_a_changed_host_key --lib -- --nocapture`: passed against `atmoz/sftp:latest`, Docker 29.9.0.
- `cargo test -p vault-core recovery --lib`: 5 passed.
- `cargo check -p unv-cli -p unenverse`: passed.
- `cargo test -p unv-cli --test capabilities`: passed.
- `npm run typecheck`, `npx vitest run tests/settings-panel.test.ts tests/efficacy-settings.test.ts`, and `npm run format:check`: passed.

`npm run preflight` completed successfully after the focused checks. It ran
formatting, workspace Clippy, the locked workspace Rust suite, `npm run check`
and the production frontend build.

## The bugs that mattered

1. An interruption after a transport wrote bytes could previously have made a
   package look complete. The common protocol now creates the manifest only
   after full remote read-back; a regression test leaves a `.part`, no manifest,
   and removes it before retry.
2. A changed SFTP host key could be accepted if a destination merely remembered
   a host name. The pinned fingerprint is now checked on setup and every use,
   with no automatic replacement path.
3. The prior one-time external age identity made an unattended monthly restore
   impossible. A non-output SQLCipher copy permits the unlocked executor to
   test recovery while the independent external identity remains necessary for
   a new machine.
4. Filename-only retention could remove a foreign backup. Retention requires a
   matching completion receipt and never deletes the newest verified receipt.

## Decisions made

- Destination configuration remains a JSON vault-meta row rather than a new
  table. It is SQLCipher-encrypted, preserves R05 local destination records and
  does not require a schema migration.
- Baseline SFTP rename has no exposed no-overwrite extension. UUID final names
  and a pre-rename existence check make collisions impracticable; ADR-0155
  records the residual server semantic.
- Password and private-key authentication read the selected entry's `api_key`.
  The private-key form currently supports an unencrypted OpenSSH private key;
  an encrypted-key passphrase source is deliberately not inferred.
- SMB/NFS is still a local destination with an explicit README warning. The
  restore drill is the verification mechanism for its actual durability path.

## What Worked / What Didn't Work

`russh-sftp` provided high-level async streaming files, directory listing,
read-back and optional fsync without an external SFTP binary. The first real
container test used an incorrect `atmoz/sftp` user specification
(`unv:...:1001:::upload`) and reported `No such file`; the corrected explicit
UID/GID form (`unv:...:1001:1001:upload`) passed. The initial image pull took
longer than the tool response window but did not create a test container until
download finished; the rerun was cached and passed. The test always names and
removes its own `unenverse-sftp-<uuid>` container.

## Next Steps

Commit the files using the per-file command from the session handoff. R07 is the
next planned roadmap phase. Do not change SFTP pinning or recovery identity
storage without updating ADR-0155 and the README recovery section.

## Notes for whoever picks this up

The optional real-server test is enabled with `UNV_SFTP_CONTAINER_TEST=1` and
skips when Docker is unavailable. It uses `atmoz/sftp:latest`, user `unv`,
password `fixture-password`, and remote `upload`; none are real credentials.
The new dependencies are `russh 0.64.1` and `russh-sftp 3.0.1`. Cargo currently
emits the pre-existing future-incompatibility warning for `proc-macro-error2
2.0.1`; the R06 checks otherwise pass.
