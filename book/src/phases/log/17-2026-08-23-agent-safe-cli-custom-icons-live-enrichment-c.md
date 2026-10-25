# Phase log 17: 2026-08-23 - Agent-safe CLI, custom icons, live enrichment, cross-platform (phases 14–15)

One session, two halves. Part 1 is the agent-safety work; Part 2 is icons,
enrichment, Windows/distro portability and Docker memory.

---

## Part 1 - Making the CLI safe for agents to drive

## Goal

> "AI + automation optimize the cli, so something like claude / python can use it
> smartly without ever touching a single key/token."

Two scoping calls, both the user's: **redact by default** (`--reveal` opts back
in, breaking existing pipes), and **CLI-native only** - an MCP server was chosen
and then dropped mid-session, so nothing MCP-shaped was built.

The design goal that fell out of it, and the one worth remembering:

> The orchestrator decides _what happens_. Values move vault → target without
> passing through the orchestrator's context.

## Current Progress

- `cargo test --workspace` - 16 agent tests, 6 parity, 7 server, 76 vault-core, all passing
- `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets` - clean
- Live end-to-end against a scratch vault and a real `envv-server`: generation,
  redaction, `exec`, `render`, `describe`, `login`, exit codes, RBAC
- `envv-cli/examples/envv.py` driven against a live vault: create → rotate →
  exec → typed error, with no secret in the Python process

### The five mechanisms

1. **Redaction by default.** Every stdout path masks stored values as
   `sha256:<12 hex>`; in JSON as `{"redacted":true,"fingerprint":…,"length":n}`.
   Fingerprints are stable, so a caller can answer "did this change?" and "do
   these two entries hold the same secret?" without reading anything.
   `out.rs` is the single place that decides.
2. **Materialisation paths that skip stdout.** `--out FILE`, `envv exec -- cmd`
   (values enter the child's environment), `envv render tpl --out FILE`. The
   rule is enforced by construction: `Resolver::for_output` redacts,
   `Resolver::materialising` does not, and the exporters cannot obtain a real
   value without being handed the latter.
3. **Secrets that are never seen at all.** `entry add --generate`,
   `entry rotate --generate`, `user token new --out FILE`. The value is created
   inside the process that stores it; the caller gets a fingerprint.
4. **A JSON envelope and stable exit codes.** `{"ok":true,"command":…,"data":…}`
   or `{"ok":false,"error":{"code","message","details"}}`; 3 not_found,
   4 ambiguous (with `details.candidates`), 5 denied, 6 conflict,
   7 unavailable, 8 needs_confirmation, 9 redacted, 10 invalid.
5. **Credential-free operation.** `envv login` caches a session token (0600);
   `--password-command 'pass show envv'` and `--password-file` keep the password
   out of argv and out of any transcript. A human authenticates once; the agent
   runs everything else.

Plus `envv describe` - the whole command tree, flags, exit codes and schemas as
one JSON document, generated from the same `clap` definition the binary runs, so
it cannot describe a flag that does not exist.

Idempotency for provisioning: `entry add --if-missing`, `entry set --create`,
`project add --if-missing`, and a global `--dry-run` enforced inside
`Access::save` - the single write point, so a command that forgets to check the
flag still cannot write.

## The bugs that mattered

### 1. `user token new` minted a token and _then_ refused to print it

The redaction check ran after the call, so refusing left a live credential in the
database that nobody could ever read - an orphan that still authenticates. The
message even said "the token was NOT created". Now the check runs first and
nothing is created. Caught by asking, in a smoke test, whether the token count
went up after a refusal. It had.

### 2. A rejected cached session failed the same way forever

A stale `sessions.json` produced `401 Invalid or expired token` on every command
with no hint that a cached file was involved. The session is now cleared on a
denial and the error says to run `envv login` again.

### 3. `--user` as a `global = true` clap arg ate a positional

Not new this session but confirmed again while adding `--json`: a global's _id_
collides with any subcommand argument of the same name.

## Decisions made

- **`env_file` chunks are masked whole, per-field flags ignored.** `chunk set`
  writes `field_type: var` by default, so a real password added that way carries
  no secret flag. Trusting the flag inside a `.env` means the first unflagged
  password is the one that leaks. `PORT=3000` being masked is the price.
- **A `${ref}` is never masked.** It is a pointer, not a secret, and leaving it
  readable is exactly what lets an agent wire configs together blind.
- **Vault-wide `export` to stdout is refused (exit 9), not masked.** A masked
  `.env` looks deployable and is not; a refusal with "use `--out`" is honest.
  Project exports mask instead, because their structure is worth reading.
- **`exec` refuses to run with unresolved references.** A blank secret in a child
  process fails far away, usually as someone else's auth error.
- **The child's exit code becomes ours**, so `envv exec … -- pytest` scripts like
  `pytest` does.
- **`$${…}` escapes in templates**, so a rendered compose file can keep its own
  `${VAR}` syntax.
- **A Python wrapper ships as an example, not a package.** It demonstrates the
  three rules (always `--json`, never read a value, branch on `error.code`)
  without becoming a second thing to version.

## What Worked

- Threading redaction through a `Resolver` rather than checking a flag at each
  print site. The type carries the decision, so a new exporter cannot forget.
- Enforcing `--dry-run` at `Access::save` for the same reason.
- Writing the smoke tests as an adversary: "did the refusal actually refuse?"
  found bug 1 in about a minute.

## What Didn't Work

- A first pass classified errors with a `From<String>` blanket. Convenient, but
  it made everything `Code::Error`; the codes only became useful once each site
  was classified by hand.
- `pkill -f "envv-server --port 18744"` matched the shell running it and killed
  the whole command. Use a pid file, or match more narrowly.

---

## Part 2 - icons, live enrichment, and platforms

Same session, four more asks. Recorded here rather than in a new file because
nothing was handed over in between.

## Goal

1. "Allow custom slugs" - **which I misread.** I built custom _project ids_; the
   ask was storing an **icon file** in the app. Both now exist; see the
   correction below.
2. Docker `.env` as the credential source for the localhost:8743 server, so an
   agent can enrich a vault "without ever touching them".
3. "Intelligently name fields to enrich the data" - clarified mid-turn to mean
   **filling existing fields**, and clarified again to mean **wiring live online
   data**, not offline guesses. Both layers ended up mattering.
4. Optimise for all Linux distros and Windows; optimise Docker RAM.

## Current Progress

- `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets` - clean
- `cargo test -p envv-cli` - 22 passing (16 agent, 6 parity)
- `npm test` - 662 passing; `npm run typecheck` clean
- Docker image builds; RAM measured before and after on a 4-core host
- Live probes exercised against real GitHub and Stripe endpoints

### Custom icon files (the actual ask #1)

An entry can now store an image. Both spellings live in the **same
`custom_icon` field** - a Simple Icons slug, or a `data:image/…;base64,…` URI -
so everything that already renders, saves, copies or exports it keeps working,
and an entry can never hold a slug and a file that disagree.

- **UI**: "Upload file…" in the icon picker. The `<input type="file">` is created
  and removed per use, because WebKitGTK renders a native file widget for any
  such element present in the DOM regardless of `display:none` (the Phase 3
  ghost). The picked file becomes the picker's selection, so it flows through the
  existing `onClose` callback and both the card path and the add/edit form store
  it without either knowing files exist.
- The form field shows `Custom file (~14 KB)` with the URI parked in
  `dataset.iconData`. A 90 KB string as an input's visible value is unusable -
  it scrolls forever and hides the slug case entirely. `setIconField` /
  `readIconField` are a **pair**; using one without the other loses the icon on
  the next edit.
- **CLI**: `entry add|set --icon-file logo.ico`. The type comes from the file's
  **magic bytes**, not its extension: an icon that claims to be a PNG and is not
  simply fails to render, with nothing on screen explaining why.
- PNG / JPEG / GIF / WebP / BMP / ICO, 96 KB encoded, validated **on read** as
  well as on pick - a vault is untrusted input (invariant 4) and may have come
  from someone else's server. **SVG is refused**: it is a script-bearing format,
  and a stored image that is also a program is the one thing worth avoiding.
- In agent output an embedded icon collapses to
  `{embedded, mime, bytes, fingerprint}` rather than 90 KB of base64.

### The misread: custom project slugs

Built before the clarification, kept because it works and is defensible:
a project whose `id ≠ slugify(name)` is treated as having a **pinned slug**, and
rename and sub-project promotion preserve it. Derived rather than stored, so old
vaults need no migration and no flag can disagree with the data. `project add
--slug`, `project rename [NAME] --slug`, shared validation in `data.rs`.

**This was unasked scope.** It is one revert away if it is not wanted.

### `envv enrich`

Fills in the metadata an imported `.env` never has - type, environment, icon,
API URL, description, tags, env prefixes - from two sources that are safe to
look at, and one that is not free:

- **The name.** `STRIPE_LIVE_KEY` says who issued it and where it runs.
- **The secret's _prefix_.** `ghp_`, `sk-ant-`, `AKIA`, `xoxb-`, `sk_live_` and
  ~40 others are public, documented, deliberately recognisable issuer markers.
  Matching one reads the first few characters and nothing else - never the
  entropy, never echoed back. Plus structural shapes: PEM blocks, JWTs,
  `postgres://` and friends.
- **`--online`**: asks each issuer about its own credential and fills
  `account_name`, `scopes`, `expires_at`, `rate_limit` from the real answer
  (GitHub, GitLab, Slack, Stripe, DigitalOcean, npm, OpenAI, Anthropic). Live
  facts overwrite offline guesses for the same field - the issuer knows, the
  prefix table only infers.

Preview by default, writes on `--apply`, gap-filling unless `--force`, output
carries fingerprints and never values. `--online` is opt-in because it sends the
secret over TLS **to the service that issued it** - the one party that already
has it - and a command that reads a vault should not start making network calls
because someone ran it out of habit.

The most valuable thing it produces is not enrichment at all: an issuer
answering 401 is the only reliable way to learn a stored credential was revoked.
Verified live - fake GitHub and Stripe keys came back `REJECTED`.

### Docker `.env` as the credential source

`--env-file` reads `ENVV_PASSWORD` **and** `ENVV_SERVER_URL` from the same file
`docker compose` already reads to start the server, so exactly one copy of the
password exists on the machine and the compose stack owns it:

```bash
envv --env-file /srv/envv/.env enrich --online --apply
```

### Platforms

- **The build blocker was SQLCipher.** `rusqlite` linked the _system_ library,
  which Windows does not package - the first thing a Windows contributor met was
  a linker error about a library their platform has no concept of.
  `[target.'cfg(windows)'.dependencies]` now vendors SQLCipher and OpenSSL
  automatically; Unix keeps system linking because it builds in seconds rather
  than minutes. A `bundled` feature does the same on Linux for release
  artefacts - an AppImage linked against Fedora's `libsqlcipher0` will not start
  on a Debian box shipping a different soname.
- **The Linux display hacks were unconditional.** Forcing `GDK_BACKEND=x11` on a
  Wayland session routes the window through XWayland: blurry fractional scaling,
  wrong HiDPI cursors, and broken outright on distros now shipping without
  XWayland. It is now applied only when the session is not Wayland, and **every
  variable is a default rather than an override** - so
  `WEBKIT_DISABLE_COMPOSITING_MODE=0 envvault` is a working escape hatch instead
  of a no-op. The compositing and DMABUF flags stay on by default; they are a
  real fix for blank windows on Nvidia and older Mesa.
- **Bundle targets**: `appimage, deb, rpm, nsis` (was AppImage only), rpm
  dependencies, and a WebView2 `downloadBootstrapper` so the Windows installer
  stays small and fetches the runtime only where it is missing.
- **CLI**: `--password-command` ran `sh -c`; Windows has no `sh`, so it now
  picks `cmd /C`. Sessions live in `%LOCALAPPDATA%` - per-user and excluded from
  roaming profiles, so a bearer token cannot be synced to another machine by a
  domain policy nobody thinks about. `exec --clean` keeps `SYSTEMROOT`,
  `COMSPEC` and `PATHEXT` on Windows: without `SYSTEMROOT` anything touching the
  CRT or winsock fails with errors that name none of this.
- **CI** is a `[ubuntu-latest, windows-latest]` matrix, `fail-fast: false`, so a
  Windows-only break cannot hide behind a green Linux run.

### Docker RAM

Measured on a 4-core host, same image, same workload:

|                                         | idle RSS |
| --------------------------------------- | -------- |
| before (workers = ncpu, default arenas) | 3.99 MiB |
| after                                   | 1.32 MiB |
| after an Argon2id unlock                | 1.36 MiB |

- **tokio ran one worker per CPU** - sixteen threads on a sixteen-core host for a
  server handling a few small JSON requests, each with a stack and its own glibc
  arena. Now two (`ENVV_WORKER_THREADS` raises it), with 1 MB stacks.
- **`MALLOC_ARENA_MAX=2`** - glibc hands each thread up to eight arenas per core,
  each reserving a 64 MB heap it never fully returns. This is why **the saving
  scales with host core count**: the before-number on a sixteen-core box is far
  worse than the 3.99 MiB measured here.
- **`MALLOC_TRIM_THRESHOLD_`** - the 64 MB Argon2id buffer now returns to the OS
  immediately instead of staying resident for the life of the process. That is
  why the post-unlock figure is 1.36 MiB and not ~65.

Compose gets `mem_limit: 256m` (room for two concurrent unlocks without the OOM
killer turning a login into a restart), `shm_size: 16m`, and a healthcheck that
opens the port rather than asking the binary its version - a wedged process
still answers `--version`.

## The bugs that mattered (part 2)

### 1. `secretType` defaulting to `api_key` blocked every inference

Every writer stamps `api_key`, so "is this field empty?" answered _no_ for an
entry nobody had classified. Every imported `postgres://` URL would have stayed
an API key forever - the single most common thing an imported `.env` gets wrong,
and the exact case `enrich` exists for.

### 2. A 401 that was my own test infrastructure

`--env-file` appeared to fail against localhost:8743 with "Wrong master
password". The flag was fine: **the user's Docker envv-server already owned that
port**, my test server failed to bind, and the CLI was faithfully authenticating
against someone else's vault. Worth remembering before blaming the code -
`ss -ltnp` first.

### 3. `pkill -f "envv-server --port 18744"` killed the shell running it

The pattern matched the invoking shell's own command line. Kill by pid from
`pgrep`, or match something the wrapper cannot contain.

### 4. A comment inside a continued `ENV` line

Docker does not allow it; the `ENV a=1 \` continuation swallowed the comment and
the build failed. Comments go above the block.

## Decisions made (part 2)

- **A slug and a file share one field.** Two fields would eventually disagree,
  and every consumer would need to learn which wins.
- **SVG is not an accepted icon format**, even though it is the obvious one for
  logos. It carries script.
- **Magic bytes over file extension** for icon typing.
- **`enrich` fills gaps only** unless `--force`. A wrong guess that silently
  replaces a deliberate choice is worse than no guess.
- **Live answers beat inferred ones** for the same field.
- **`--online` is opt-in and says what it does.** It transmits a credential, even
  if only to its issuer.
- **Unix keeps system SQLCipher; Windows vendors.** Portability where it is
  needed, build speed where it is not.
- **Every Linux display variable is a default, not an override.** The previous
  code decided for the user on hardware it had never seen.
- **Two tokio workers, not one per core.** The work is IO-bound; the one CPU-
  heavy step is Argon2id, which is rare and self-limiting by cost.

## What Worked (part 2)

- Measuring the Docker change instead of asserting it. "Should use less memory"
  and "3.99 MiB → 1.32 MiB, and here is why it scales with core count" are
  different claims.
- Running the live probes against real endpoints with deliberately fake keys -
  the `REJECTED` path is the one that matters and it is the easy one to test.

## What Didn't Work (part 2)

- Guessing what "custom slugs" meant instead of asking. Two features exist now;
  only one was wanted.
- A closure capturing `proposals` in `plan_entry` - the tags branch needed the
  same borrow. A `macro_rules!` sidestepped it, which is the right tool here but
  reads oddly if you are not looking for the borrow.

## Next Steps

1. **Commit.** Phases 11–15 are all still uncommitted in one tree.
2. **Tests for part 2 are missing** - nothing covers the embedded-icon validator,
   `readIconField`/`setIconField` round-tripping, the enrich signature table, or
   the `is_blank` `secretType` special case. The last one is a bug that was
   already made once.
3. **Windows is reasoned about, not run.** No Windows machine was available: the
   build, the tray, window-state restore and the NSIS installer are unverified.
   The CI matrix is what will actually prove it.
4. **Decide about `entry get` without `--field`.** It still prints every
   substring match while every other command refuses ambiguity.
5. **`--reveal` is a breaking change** for anything piping `envv get` or
   `envv export`. Worth a README line before release.
6. **The project-slug feature was unasked scope** - keep or revert.
7. Remote mode still cannot pin a certificate (`reqwest` does full CA
   validation; pinning lives only in Tauri's `remote_request`), so a self-signed
   `envv-server` is unreachable from the CLI.

## Notes for whoever picks this up

- `out.rs` decides redaction; `error.rs` defines the codes. Nothing else should.
- Adding a command: return `CliResult`, emit through `out::ok(command, data,
text_closure)`, and classify failures with `CliError::not_found` /
  `::ambiguous` / `::conflict` rather than a bare string.
- `envv describe | jq` is the fastest way to see what the CLI currently offers -
  it is generated from the live `clap` definition, so it is never stale.
- The agent guarantees are pinned in `envv-cli/tests/agent.rs`. If you make
  redaction conditional on something new, that file is where it must be proven.
- Icon rules live in `src/ts/icons.ts` (`validateEmbeddedIcon`, `MAX_ICON_CHARS`)
  and are duplicated in `envv-cli/src/entries.rs` (`embed_icon`). The two limits
  must agree, or a file one accepts renders as a broken image in the other.
- `enrich`'s signature table is `SIGNATURES` in `envv-cli/src/enrich.rs`; the
  live probes are `PROBES` beneath it. Adding an issuer is one row in each.
- Docker memory settings are load-bearing, not decoration: `MALLOC_ARENA_MAX`
  and `ENVV_WORKER_THREADS` in the Dockerfile, `mem_limit` in compose.

---

## Also recorded in the rolling log

`CLAUDE.original.md` carried a condensed retelling of this session, kept
here so nothing is lost when that file stops being a log.

### 2026-08-23 - Phase 14: agent-safe CLI

Same day as phase 13, second ask: make the CLI safe for Claude or a Python script
to drive "without ever touching a single key/token". See `the session records/17-2026-08-23-agent-safe-cli-custom-icons-live-enrichment-c.md`.

- **Redact by default** (`--reveal` opts in) - a breaking change to any script
  piping `envv get` or `envv export`. Values become `sha256:<12 hex>`
  fingerprints, stable per value so drift and reuse are still detectable.
- **Materialisation paths that skip stdout**: `--out`, `envv exec -- cmd`,
  `envv render --out`. Enforced by which `Resolver` the caller can obtain, not by
  remembering to check a flag.
- **JSON envelope + stable exit codes** (3 not_found, 4 ambiguous, 5 denied,
  6 conflict, 7 unavailable, 8 needs_confirmation, 9 redacted, 10 invalid), and
  `envv describe` emitting the whole contract from the live clap definition.
- **`envv login` / `--password-command`** so a human authenticates once and the
  agent never holds a credential.
- Bug found by an adversarial smoke test: `user token new` minted the token and
  _then_ refused to print it, leaving a live orphan credential in the database.

---
