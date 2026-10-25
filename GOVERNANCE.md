# Governance

UnENVerse is maintained by one person. This file says who has access to what, what
each role is responsible for, and how that access is granted.

## Members with access to sensitive resources

| Who                                                           | Role       | Sensitive resources                                                                                                                                                                          |
| ------------------------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Jubair Hasan ([@darthdemono](https://github.com/darthdemono)) | Maintainer | Repository admin, GitHub Actions secrets (update-manifest signing key, catalogue signing key, coverage upload token), release publishing, private vulnerability reports, the project website |

There are no other collaborators with write access. Releases are built and signed
by the GitHub Actions workflow (keyless Sigstore signing from the workflow's
identity), so no personal signing key is involved in a release.

## Roles and responsibilities

- **Maintainer.** Reviews and merges changes, cuts releases, triages issues,
  answers vulnerability reports within the time in [SECURITY.md](SECURITY.md),
  keeps the secrets listed above safe, and decides who else gets access.
- **Contributor.** Anyone who sends a pull request or a patch. Follows
  [CONTRIBUTING.md](CONTRIBUTING.md). Has no special access.
- **Reporter.** Anyone who opens an issue or a private vulnerability report.

## Granting access

- New collaborators start with read or triage access. Write access is granted only
  after the maintainer has reviewed a series of that person's contributions and is
  satisfied with their care and judgement. Admin access, and access to any secret,
  is not delegated while the project has a single maintainer.
- Every account with write access or above must use two-factor authentication on
  GitHub.
- Access is reviewed at each release and removed when no longer needed.

## Legal

The project licence, PolyForm Strict 1.0.0, does not let anyone but the licensor make
changes or new works, so a contribution is accepted only on these terms: by opening a
pull request you confirm you have the right to contribute the work, and you grant the
maintainer a perpetual, worldwide, royalty-free licence to use, modify and distribute it
as part of the project under the project licence or any later licence the maintainer
chooses. You keep the copyright in what you wrote.
