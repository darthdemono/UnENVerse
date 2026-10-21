# Security model

The vault is encrypted at rest with SQLCipher. A master password is processed
locally with Argon2id; the derived key is not written to disk and is cleared
when the vault is locked. The database salt is required to recover a vault, so
backups must keep it with the encrypted database.

`unv` defaults to redacted output. Commands that would expose stored values on
stdout require an explicit reveal option; use file output or `unv exec` when a
consumer needs a real secret. The optional server supports TLS pinning, scoped
users, and compare-and-swap writes to avoid silent overwrite conflicts.

No security control removes the need to protect an unlocked desktop session or
the machine on which it runs. Report vulnerabilities through the repository's
[security policy](https://github.com/darthdemono/UnENVerse/security/policy).

## How the code is checked

Free scanners run on every change or every week: CodeQL, cargo-deny (RustSec
advisories, licences and registries), OSV Scanner on both lockfiles, Semgrep with
the Rust, TypeScript, Docker, GitHub Actions and secrets rule packs, Gitleaks over
the whole history, zizmor on the workflows, Hadolint and Trivy on the Dockerfile
and repository files, `npm audit signatures`, dependency review on pull requests,
the OpenSSF Scorecard, and fuzzing of the importers. Releases carry Sigstore
signatures, build provenance and SBOMs. None of this replaces an independent
audit, which is still planned. The workflow is
[`security.yml`](https://github.com/darthdemono/UnENVerse/blob/main/.github/workflows/security.yml).
