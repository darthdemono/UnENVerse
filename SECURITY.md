# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability or disclose secrets in
an issue. Use [GitHub's private vulnerability-reporting form](https://github.com/darthdemono/UnENVerse/security/advisories/new)
for this repository, or contact the maintainer through the email address on the
[GitHub profile](https://github.com/darthdemono).

Include a minimal reproduction, affected version, impact, and any proposed
mitigation.

### Security contacts

The maintainer, Jubair Hasan ([@darthdemono](https://github.com/darthdemono)), is
the only security contact. The private form above reaches the maintainer directly.
The same contact is in the site's
[security.txt](https://unenverse.darthdemono.com/security.txt).

### What to expect

- Reports are acknowledged within seven days.
- The maintainer confirms or rejects the report, then works with the reporter on a
  fix and a disclosure date. The reporter is credited unless they prefer not to be.
- Once a fix is released, the vulnerability is published as a
  [GitHub Security Advisory](https://github.com/darthdemono/UnENVerse/security/advisories)
  with a CVE where one applies. It names the affected versions, how to tell if you
  are affected, and the fixed version or the mitigation.
- Vulnerabilities in dependencies that do not affect UnENVerse are recorded with a
  reason in [`osv-scanner.toml`](osv-scanner.toml), `deny.toml` and the machine
  readable [`vex.openvex.json`](vex.openvex.json).

### What a good report contains

- The affected version and platform, and the component (vault, CLI, server, desktop app, site).
- The impact: what an attacker gains, and what they need first.
- Steps to reproduce or a proof of concept, and the root cause if you found it.
- Whether you plan to publish, and when.

Please keep the report private until a fix ships or 90 days have passed, whichever
comes first, and do not share it with third parties meanwhile. There is no bug
bounty and no PGP key: the private form above is encrypted in transit and visible
only to the maintainer.

### Safe harbour

Good-faith research that follows this policy is welcome. The maintainer will not
start legal action over it, provided you stay within the scope below, use only
vaults and systems you own or are authorised to test, do not read, change or
destroy other people's data, and stop and report if you reach real secrets.

### How reports are handled

1. Acknowledge the report (within seven days) and say what happens next.
2. Record it in a private advisory draft, reproduce it, and rate its impact.
3. Fix it on a private branch where the issue is severe, or in the open for a low risk one.
4. Verify the fix with the reporter before the release.
5. Release, publish the advisory (with a CVE where one applies), and credit the reporter.

### Security incidents

Incidents without a direct exploit are handled the same way: a compromised
dependency, a tampered release or workflow, a leaked signing key or token, or an
attack on the repository or the website. The maintainer confirms the incident,
revokes and rotates what was exposed (see "Secrets and credentials" below), ships a
release that pins any new public key, and publishes an advisory that says who is
affected and what to do. Legal reporting duties are met where they apply.

## Scope

UnENVerse's encrypted storage, desktop IPC, CLI redaction, server API,
authentication, authorization, and release artefacts are in scope. Please test
only against vaults and systems you own or are authorised to assess. The
[threat model](THREAT_MODEL.md) lists what the project does and does not defend
against.

## Supported versions

Only the latest release receives security fixes. When a newer release ships, the
previous one stops receiving security updates. How the vault is protected, and how
to verify a download, is described on the
[security page](https://unenverse.darthdemono.com/security/) and in the
[download verification steps](https://unenverse.darthdemono.com/download/#verify).

## Secrets and credentials used by the project

- The project's own credentials are the update-manifest signing key, the
  provider-catalogue signing key and the coverage upload token. They live only as
  GitHub Actions secrets. They are never committed, never printed in logs, and
  workflows triggered by forks do not receive them.
- Release assets are signed keylessly from the workflow's identity, so there is no
  long-lived release signing key to store.
- A long-lived key is rotated when it may have been exposed, and when a person who
  had access to it loses that access. Rotating a signing key means shipping a
  release that pins the new public key.
- GitHub secret scanning with push protection is on, `.gitignore` excludes local
  vaults and environment files, and test data must not look like real credentials.

## Dependency vulnerability policy

- Dependencies come from crates.io and npm through Cargo and npm, with committed
  lockfiles. Dependabot proposes updates weekly.
- Every CI run executes `cargo deny` (advisories, licences, bans, sources) and
  `npm audit` at high severity. A release cannot start from a red CI run.
- Threshold: a known vulnerability in a dependency that ships in a release is fixed
  by upgrading or replacing it before the next release. If it does not affect
  UnENVerse, it is recorded with the reason in `deny.toml`, `osv-scanner.toml` and
  `vex.openvex.json`, and reviewed when the dependency changes.
- Licences must be on the allowlist in `deny.toml`.

## Static analysis policy

- CodeQL (JavaScript/TypeScript and Rust), Clippy with defect lints denied, and
  type-aware ESLint run on every push and pull request.
- Threshold: any finding of medium severity or higher is fixed, or dismissed with a
  written reason, before the next release.

## Fuzzing and audits

- Every parser that reads untrusted text has a libFuzzer target in [`fuzz/`](fuzz/),
  run weekly by the Fuzz workflow and on demand.
- Milestones that change the attack surface (new storage format, new network
  route, new signing key, new third-party library with native code) get a review
  against the [threat model](THREAT_MODEL.md) before release. The
  [assurance case](ASSURANCE_CASE.md) lists the boundaries and weaknesses an
  independent audit would start from.
