# Phase log 21: 2026-08-25 - Phases 17 and 18 implemented, CI unblocked twice, README rewritten

Continues directly from `20-2026-08-25-ci-unblocking-handoff-reorganisation-and-the.md`, which covers the first half of the same
session: the CI Perl and artifact fixes, the handoff reorganisation, and the 1.0
plan. This file covers what was actually built afterwards.

## Goal

Implement the staged phases in order. The ask was "do phase 1", clarified to
Phase 17, then 18, then a redirect back to finish what was left of 18 before
moving on. Three CI failures arrived mid-flight and were fixed as they appeared.
The session ended with a full README rewrite.

## Current Progress

**Version 0.8.1.** Tags `v0.7.1`, `v0.8.0` and `v0.8.1` all exist on the remote,
so the release pipeline has now cut releases end to end. That was an open item in
the 1.0 plan and it is closed.

**183 Rust tests, 814 frontend tests.** clippy clean, `cargo fmt --check` clean,
`tsc --noEmit` clean, ESLint 0 errors, Prettier clean, `cargo doc` clean.

### Phase 17 - Foundation and security (complete, 12 tasks)

| Task     | What landed                                                                                                                                                                    |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1.1–1.3  | `vault-core/src/tls.rs`: one verifier shared by the app and the CLI. CLI moved to rustls. `--fingerprint`, `--ca-cert`, `--tofu`, pin persisted per server in `sessions.json`. |
| 1.4      | `connectRemote()` zeroizes the local key. `keepLocalUnlocked` setting, off by default.                                                                                         |
| 1.5      | `backup archive` / `restore-archive`: db + salt + manifest, 0600, checksums verified before writing.                                                                           |
| 1.6      | `permex::require_all`, `strict_write` on users and classes, `envv user strict-write`.                                                                                          |
| 1.7      | The salt truth stated in `status`, `doctor`, and enforced by `check_salt_pairing`.                                                                                             |
| 1.8–1.12 | `vault-core/src/entropy.rs`, all four generators, `--entropy-source`, `envv gen sources`.                                                                                      |

Verified live against a self-signed `envv-server`: no pin refused (exit 5), wrong
pin refused (exit 5, distinct message), `--tofu` learned and pinned, later
commands pinned with no flags, re-TOFU refused.

### Phase 18 - Core features and integrations (complete, 7 tasks)

- `envv doctor`: integrity, salt, file modes, schema, pools, audit chain. Exit 0
  or 10. Runs without an open vault, which is the case it exists for.
- `envv import-vault bitwarden|onepassword|proton`. Preview by default,
  fingerprints not values, idempotent on re-run.
- `GET /api/vault/entries`: selective and batch reads, permissions applied
  before the filter.
- `chunks/env-link.ts` and `chunks/edit-modal.ts` from 0% to 100% lines.
- **All eleven project types graduated**, each on evidence from the software it
  targets. Docker was available locally, so the whole matrix ran here rather
  than waiting on CI.

| Type       | Evidence                                                           |
| ---------- | ------------------------------------------------------------------ |
| kubernetes | Applied to real k3s v1.31.4; value decoded back out of the cluster |
| ssh_config | `ssh -G` resolved every directive                                  |
| apache     | `httpd -t` returned `Syntax OK`                                    |
| haproxy    | `haproxy -c` returned 0                                            |
| ansible    | `ansible-playbook --syntax-check` returned 0                       |
| postgres   | A live server authenticated using only the generated `.pgpass`     |
| traefik    | Traefik v3.3 API reported router and middleware `enabled`          |

Every check has a control case that fails if the validator accepts nonsense.

### CI, fixed three more times

1. **rustdoc.** An outer `///` on `pub mod entropy;` and `pub mod tls;` in
   `lib.rs`, while both files already had `//!` docs. rustdoc merges the two and
   resolves the combined text in the _parent's_ scope, so every intra-doc link
   written inside the module failed and `-D warnings` failed the build.
2. **Prettier.** Eight files, including the new test suites.
3. **k3d.** `AbsaOSS/k3d-action@v2` fetches k3d from a `raw.githubusercontent`
   URL that now 404s. Replaced with `docker run rancher/k3s`, which is what had
   already been validated locally.
4. **`docker exec` and stdin.** The unicode manifest arrived empty because
   `docker exec` without `-i` does not attach stdin, so `cat` saw EOF and wrote
   nothing. It surfaced as `error: no objects passed to apply`.

### README

Rewritten, 1063 lines, zero em-dashes, in Jubair's voice via the `joy-writing`
skill. Covers every Phase 17 and 18 feature plus new sections for entropy
sources and backups. Numbers checked against the code rather than carried
forward, which caught the old claim of "ten tool panes" against nineteen real
`data-tool` buttons.

## The bugs that mattered

Ranked by what they did, not by which line was wrong.

1. **Opening a vault whose salt had gone missing generated a fresh one.** Every
   unlock then reported "wrong password" for a correct password, forever, and
   the evidence was already destroyed because the missing file had been
   replaced. `check_salt_pairing` refuses now, in both the CLI and the desktop
   unlock path.
2. **Six of the seven newer exporters never resolved `${refs}`.** Apache,
   HAProxy, Ansible, Postgres, ssh_config and Traefik interpolated `f.value`
   raw. A `.pgpass` shipped the literal `${PgProd/password}` as the password.
   This is invariant 5, and it is the _same_ bug found in `exportNginx` in Phase
   13 - caught there by a golden fixture, missed here because these types had
   none.
3. **`exportAnsible` produced YAML no parser can read.** A mapping immediately
   followed by a sequence in one document. Its output could never have been
   consumed by the tool it was written for. Found by running
   `ansible-playbook --syntax-check` against it, not by review.
4. **`vault.db` and `vault.salt` were created mode 0644.** Contents are
   encrypted, so it is not disclosure of secrets, but it hands every local user
   the ciphertext and an offline-attack head start. Found by `envv doctor` on
   its first run against a healthy vault.
5. **`env-link.ts` let array order decide which entry an env var links to.**
   `AWS` matched both `AWS` and `AWS_PROD` at confidence 100, and `score > best`
   broke the tie by array position. Adding an unrelated entry could silently
   repoint an env var at a different secret. Invariant 1 in another costume.
6. **The CLI linked a different TLS stack from the app.** `reqwest` default
   features pull native-tls; `src-tauri` has always used rustls. One product,
   two trust decisions, neither aware of the other.
7. **Bitwarden logins were hard-coded `secret_type: "password"`** while the
   other two readers classify by value, so an imported `postgres://` URL
   rendered with the wrong fields forever.

Three of mine, caught by tests rather than review:

- `OnceLock` silently dropped the TOFU pin, so login went out under CA
  validation after successfully learning the right fingerprint.
- Adding `aws_lc_rs` left rustls unable to choose a provider, panicking
  `envv-server` _after_ it had printed its banner, URL and fingerprint.
- `access::set_paths` is write-once, so parallel `#[test]` functions fought over
  one global vault path.

## Decisions made

- **Entropy is always `HKDF-SHA256(os || device)`.** Raw device output would
  hand anyone controlling the device every key generated on that machine.
  Absence exits 7 rather than falling back silently.
- **Salts stay on the OS RNG.** A device-dependent vault salt turns a lost
  YubiKey into a lost vault.
- **No `--repair` for a missing salt, ever.** Nothing can reconstruct 16 bytes
  of CSPRNG output. A flag that appeared to offer it would be discovered as a
  lie during a restore.
- **Strict write narrows writes only.** Narrowing reads too would make the
  user's own vault look empty the moment the flag went on.
- **A type graduates on evidence from the real tool, never on a fixture.** A
  fixture proves we agree with ourselves.
- **The experimental-gate tests were kept, not deleted.** Graduating the last
  gated type left the gate with nothing to gate; deleting its tests would remove
  the coverage protecting the next unproven type. They now run against a
  synthetic `some_future_type`.
- **`enrich --online` uses a separate public client.** Applying the server's pin
  to `api.github.com` would fail every handshake, and if it somehow did not, the
  pin was not being enforced.

## What Worked

- Auditing before planning. Five premises in the 1.0 brief were wrong, and one
  of them (`doctor --repair` restoring a salt) would have shipped a feature that
  cannot exist.
- **Running the real tools locally instead of waiting for CI.** Docker was
  available, so k3s, Traefik, HAProxy, Ansible and Postgres all ran here. That
  is what found the Ansible YAML bug.
- **Extracting a workflow's `run:` blocks and executing them.** The k8s job was
  proven with a good manifest (exit 0) and a sabotaged one (exit 1) before being
  pushed. Same move as replacing the k3d action: stop guessing, run it.
- Checking a dependency's own source. `OPENSSL_SRC_PERL` was found by reading
  `openssl-src/src/lib.rs:157`, not by theorising about PATH.
- Verifying fixes by reverting them. Removing the resolver from `exportPostgres`
  failed both the golden file and the property test.

## What Didn't Work

- **`pkill -f "<pattern>"` killed my own shell, twice.** The pattern appears in
  the invoking shell's own command line, so `pkill -f` matches it. Use a stored
  PID.
- Trusting the version I read at the start of the session. It moved to 0.8.1
  mid-session and the rewritten README shipped saying 0.7.1. Read
  `npm run version` in the same command that writes the file.
- Three pasted CI analyses were confidently wrong: the rustdoc one blamed
  decorative brackets, the k3d one guessed at action versions, and the
  `docker exec` one proposed the exact form the workflow already had. Reproducing
  each locally settled all three in minutes.

## Next Steps

Phases 19 and 20 remain, both staged in `CLAUDE.md`.

- **Phase 19:** onboarding wizard, TOTP for sub-users (greenfield, zero `totp`
  in the tree), accessibility pass, `envv use` per-directory context, README and
  docs. The Settings-panel overflow fix (3.5) is already done.
- **Phase 20:** `tracing` in all three binaries, a real `/api/health`,
  `Retry-After` and request ids, the lint-debt decision, the audit index.

Decisions only a human can make:

1. The `envv` binary-name collision with envvault.dev. Breaking change after 1.0.
2. Whether 237 lint suppressions ship in a 1.0.
3. Whether lock-on-switch stays the default now that it is in.

## Notes for whoever picks this up

- **Read the version before writing it anywhere.** `npm run version` is the
  source of truth and it changes between turns.
- `.github/workflows/exporters.yml` has never run in CI. It was validated
  locally against real containers, job by job, but the runner has not executed
  it. Watch the first run.
- The gate machinery in `experimental-ptypes.test.ts` now tests a type that does
  not exist. That is deliberate. Do not "clean it up".
- `envv doctor` is the fastest way to find out what a vault is missing. Run it
  first when something looks wrong.
- The next handoff is `22-2026-08-27-phase-20-observability-secret-creation-dates.md`.
