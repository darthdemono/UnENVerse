# Phase log 16: 2026-08-23 - CLI parity with the UI, and two exporter bugs it uncovered (phase 13)

## Goal

Two asks:

1. **Fix the tag font**, which read as a rendering glitch next to every other badge.
2. **Bring the CLI to parity with the UI** - "add everything that can be done in
   UI, to be in cli".

Scope for (2) was settled with the user before any Rust was written: full parity
for vault _data_ operations, plus config exporters for the four
`STABLE_PROJECT_TYPES` only. The seven experimental types stay UI-only rather
than existing as a second implementation that drifts. Destructive commands
prompt unless `--yes`.

## Current Progress

Done and verified:

- `npm test` - **662 passed** (651 before; +7 parity fixtures, +4 regressions)
- `npm run typecheck` - clean
- `cargo check --workspace` - clean, no warnings
- `cargo test -p envv-cli` - 6 parity tests passing
- End-to-end smoke run against a scratch vault (`--db-path`) and a live
  `envv-server` on port 18743: entry/project/chunk/category CRUD, exports,
  backup round trip, RBAC, sub-user and token auth.

### The tag font

`.tag-chip-card` / `.tag-chip-sidebar` (`src/css/activity-tools.css`) declared no
`font-family`, so they inherited **Syne** from `body` - a display face at 9.5px,
sitting beside badges that are all JetBrains Mono. Now mono, weight 500,
`line-height: 1.5`, `inline-flex` centring, and letter-spacing dropped to 0
(mono does not need the tracking Syne did).

### The CLI

`envv-cli` was one 700-line `main.rs` with nine commands. It is now a
`lib` + `bin` split across thirteen modules, so integration tests can call the
exporters directly.

New command groups: `entry`, `project` (with `project chunk`), `category`,
`tags`, `gen`, `backup`, `scan`, `status`, `user` (with `user token`), `class`,
`perm`. Every pre-existing command still works with its old flags.

New global flags: `--user` / `--token` (scoped auth, remote only), `--yes`,
`--db-path` / `--salt-path` (a second vault, and what makes testing possible),
`--init`.

## The bugs that mattered

Ranked by what they did to the user.

### 1. Disabled chunks were exported anyway

`SecretChunk.disabled` is documented as "excluded from exports", and the four
_experimental_ exporters (apache, haproxy, ansible, postgres) each check it. The
four people actually deploy - WireGuard, Compose, nginx, k8s (and ssh, traefik) -
did not.

So turning a WireGuard peer off greyed the card out and then wrote it into
wg0.conf regardless: **the tunnel kept trusting a peer the user believed they had
removed.** A disabled Compose service still came up.

Fixed with a shared `activeChunks(project)` helper in `chunk-ops.ts`, used by
every stable exporter.

### 2. `exportNginx` never resolved `${…}` references

It interpolated `f.value` raw while every other exporter resolved. The nginx
starter template ships `ssl_certificate ${example_cert}` fields, so a generated
nginx.conf reached the server as literal placeholder text and nginx refused to
start - while copying the same chunk from its card resolved correctly. The two
disagreed about what the config said, which is the worse half of the bug.

### 3. `envv watch` grew the vault without bound

`cmd_import` appended unconditionally, and `watch` re-ran it on every file save.
Watching a 20-line `.env` for an afternoon added twenty entries per save. Import
now upserts by provider (`--allow-duplicates` restores the old behaviour), so
watch is idempotent - the one thing it exists to do.

### 4. The CLI could not reach a scoped user or a LAN host

`RemoteClient` only ever called `/api/unlock` with the master password. That
meant the master credential on the wire for every command, no way to use the RBAC
the server already enforces, and no way at all to talk to a desktop "Open to LAN"
host - which refuses `/api/unlock` outright. `--user` and `--token` now go
through `/api/auth`. Verified: a read-only user's `list` is filtered and their
write is refused with `403 No write permissions`, with the owner's data intact
afterwards.

### 5. A `global = true` argument shares its id with subcommand arguments

`--user` as a global with field name `user` collided with the positional `user`
in `envv user token ls <user>`: clap wrote the positional's value into the global,
and the command died with "`--user` requires `--server`". The global is now
`as_user` with `long = "user"`. Worth remembering - nothing warns about it.

## Decisions made

- **Exporters for stable types only.** Porting all ten would double the Rust and
  create ten pairs of implementations with nothing checking they agree.
- **Golden-file parity instead of trusting review.** `tests/fixtures/parity/`
  holds one fixture vault and the expected output of each format.
  `tests/cli-parity.test.ts` pins the TypeScript; `envv-cli/tests/parity.rs`
  pins the Rust against the same bytes. Change either and one fails.
  Regenerate with `PARITY_UPDATE=1 npx vitest run tests/cli-parity.test.ts`.
  **This is what found bugs 1 and 2** - the fixture made the app's own output
  legible for the first time.
- **`--db-path` implies the salt beside it.** Pairing a vault with the wrong salt
  derives the wrong key and reports "wrong password" for a correct password.
- **Ambiguous lookups are refused, never guessed.** `envv entry rm git` with
  GitHub and GitLab present lists both and exits non-zero. Same rule as never
  identifying an entry by array index: the cost of guessing is deleting the wrong
  secret.
- **A non-tty stdin refuses confirmation rather than assuming yes.** A script
  that forgot `--yes` must fail loudly.
- **The experimental gate is enforced at the write**, matching
  `setProjectCreateType()`: `envv project add --type kubernetes` refuses without
  `--experimental`.
- **Backup format is byte-compatible with the app.** Verified in both directions
  against Node's WebCrypto - CLI-written `.vaultbak` decrypts in the browser
  path, and a browser-written one restores through the CLI.

## What Worked

- Writing the fixture _first_ and letting both implementations meet in the
  middle. Two of the five bugs were in code nobody had touched this session.
- Reverting each fix to watch the test fail (project convention) - all four
  regression tests were confirmed this way.
- A real `envv-server` on a scratch port for the remote paths. jsdom cannot reach
  them and neither can a unit test.

## What Didn't Work

- Assuming `serde_json` preserves object key order. It does not (BTreeMap), so a
  vault round-tripped through the CLI comes back with keys sorted. Harmless -
  `entry_ck` keys on `id` and `version_history` only appends when `api_key`
  changes - but it means a CLI write is not byte-identical to a UI write.
- `chunkToString` and `exportNginx` disagreeing about upstream `server` lines
  (one splits on commas, the other does not). Both behaviours are now pinned by
  goldens; neither was changed, because the split-vs-not question is a real
  format decision, not an obvious bug.

## Next Steps

1. **Commit.** Phase 11 is _still_ uncommitted underneath this work (listed in
   Handoffs 2, 3, 4). The tree also holds unrelated pre-existing changes.
2. **Manual smoke tests only a human can run** (unchanged from Handoff 4): window
   state after hide-to-tray, the LAN gate while connected to a remote, the TOFU
   fingerprint prompt, and switching to the local vault from the startup screen.
3. **Ship the disabled-chunk fix knowingly.** Anyone who disabled a WireGuard
   peer and re-exported has a wg0.conf on a server that still lists it. The fix
   changes what the next export writes; it cannot reach the deployed file.
4. **Experimental exporters are still UI-only.** If the CLI needs them, port them
   _with_ golden fixtures - the pattern is in place now, and it is what makes the
   port safe.
5. Consider whether `envv entry get` should keep printing every substring match.
   Every other command refuses ambiguity; `get` still guesses, only because
   changing it would break existing scripts.

## Notes for whoever picks this up

- The CLI is a `lib` (`envv_cli`) plus a thin `bin`. Add commands to
  `main.rs`; add logic to the module.
- `refs.rs` is the Rust twin of `resolveFieldRef`. If you touch one, touch both,
  and run `cargo test -p envv-cli` - the goldens will tell you.
- `env_copy_field()` reads `~/.config/io.envvault/settings.json` so the CLI
  resolves bare `${Provider}` refs to the same field the UI would. The parity
  test passes `"api_key"` explicitly so it does not depend on the machine.
- Remote mode cannot pin a certificate: `reqwest` does full CA validation and the
  fingerprint pinning lives only in Tauri's `remote_request`. A self-signed
  `envv-server` is unreachable from the CLI. Terminate TLS at a proxy with a real
  cert, or add `--ca-cert`.

---

## Also recorded in the rolling log

`CLAUDE.original.md` carried a condensed retelling of this session, kept
here so nothing is lost when that file stops being a log.

### 2026-08-23 - Phase 13: CLI/UI parity, exporter golden fixtures

Two asks: fix the tag font, and make the CLI able to do everything the UI can.
See `the session records/16-2026-08-23-cli-parity-with-the-ui-and-two-exporter-bugs.md`. 651 → 662 tests.

- **`envv-cli` is now lib + bin** across thirteen modules, so tests can call the
  exporters directly. New groups: `entry`, `project` (+ `project chunk`),
  `category`, `tags`, `gen`, `backup`, `scan`, `status`, `user` (+ `user token`),
  `class`, `perm`. Old commands keep their flags.
- **Scoped auth over `/api/auth`** (`--user`, `--token`). Before this the CLI
  only knew `/api/unlock`, so every remote command put the _master password_ on
  the wire and a desktop "Open to LAN" host was unreachable outright.
- **Golden-file parity between the two exporter implementations** - the decision
  that mattered. It immediately found two live bugs in the app: disabled chunks
  were exported by every stable exporter (a disabled WireGuard peer still went
  into wg0.conf), and `exportNginx` never resolved `${…}` so the starter
  template's `ssl_certificate` reached nginx as literal text.
- **`envv watch` grew the vault without bound** - import appended
  unconditionally, so every save of a watched `.env` added a full copy. Import
  now upserts.
- Tag chips inherited Syne from `body` at 9.5px while every other badge is
  JetBrains Mono.
