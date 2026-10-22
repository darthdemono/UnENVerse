# ADR-0166: Windows owner-only access lists and a device key that does not roam

Status: accepted

## Context

Unix files get mode 0600. Windows had no equivalent here, so the vault, salt and recovery files inherited their folder's access list, and `unv doctor` could only say the check was not enforceable. The device approval key sat under the roaming profile, which a domain copies between computers, so two machines could silently be the same approver.

## Decision

`vault_core::restrict_to_owner` on Windows replaces the file's access list with a protected one that allows only the current user (`SetNamedSecurityInfoW` with a one-entry list and `PROTECTED_DACL_SECURITY_INFORMATION`). `create_private_dir` does the same for a directory with an inheritable entry, so what is created inside is owner-only from the first byte, and the vault's folder is made that way when this program creates it. A folder that already existed is not touched. The list is not a defence against an administrator; the documentation says so. `unv doctor` reads each file's list with `icacls` and warns on `Everyone`, `Users` and `Authenticated Users`.

The device key's directory is the machine-local profile on Windows. A build that finds a valid key only at the old roaming path reads it and writes it to the new one; it never deletes the old file, so an interrupted migration leaves the old key usable. The same release adds a bounded retry for renames over a file another process holds open, a network-filesystem warning in `unv doctor`, vendored SQLCipher on macOS, and an informational macOS job in CI.

## Consequences

The Windows access-list tests run in the Windows CI job and read the result back with `icacls`; they cannot run on a Linux machine. macOS stays unreleased and unsigned because signing needs a paid Apple account. The node agent is still installed on Linux by `unv node install` and on macOS and Windows by hand.
