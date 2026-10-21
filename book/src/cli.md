# CLI guide

Use `unv describe --json` to obtain the machine-readable command contract for
the installed version. The common workflow is:

```text
unv status
unv entry add GitHub --type password --username alice --key-stdin
unv get GitHub --field api_key --reveal --out token.txt
unv doctor
```

Use stdin for secret input where possible. Avoid `--reveal` on interactive
shells, transcripts, or CI logs. `unv backup archive` creates a recoverable
backup containing the vault database and its salt.

The CLI can connect to a remote vault only after a certificate pin is recorded;
this prevents a self-signed or local server from becoming an unauthenticated
trust exception.

## Bundles

A bundle is one card for several entries that belong together (a service's
password, its web session and its API app). Members stay ordinary entries.

```text
unv bundle new "Discord bot" --member bot=DiscordBot --member web=DiscordWeb
unv bundle new "Discord bot" --import config.py     # a Python config module, read as data
unv bundle add "Discord bot" Spotify --slot api
unv bundle remove "Discord bot" api                 # detach; the entry is kept
unv bundle dissolve "Discord bot"                   # members return to the grid
unv bundle delete "Discord bot" --yes               # deletes the members too
unv get bundle:Spotify/api --field ID               # the bundle: selector
```

References use the same selector: `${bundle:Spotify}`, `${bundle:Spotify/api}`,
`${bundle:Spotify/api/ID}` and `${bundle:Spotify/prefix}` for a bundle-local
variable. An ambiguous bundle name resolves to nothing rather than to a guess.

`--import` never executes the file. It reads string and f-string literals,
numbers, booleans and `None`; anything else is kept as source text with a
warning that names the line. A name assigned twice keeps the first value and
imports the second as `name_2`, and later f-strings are rewritten to match.

## Files a tool reads

```text
unv emit Npm                       # lists the formats this entry's type offers
unv emit Npm --as npmrc --out ~/.npmrc
unv emit Prod-DB --as dsn --out db.env
unv emit Home-WiFi --as wifi-uri --reveal
```

`emit` writes the credential, so it follows `export`: refused to stdout without
`--reveal`, and written `0600` with `--out`. Formats: `npmrc`, `pypirc`,
`cargo-credentials`, `docker-config`, `netrc` (registry tokens); `dsn`, `libpq`,
`jdbc` (databases); `wifi-uri` (Wi-Fi).

## Web sessions

```text
unv cookie import capture.txt                       # preview, writes nothing
unv cookie import capture.txt --entry Site --create
unv cookie import export.har --origin https://www.example.com --entry Site
unv cookie import cookies.sqlite --from firefox --host example.com --entry Site
```

Accepts DevTools **Copy as cURL** (bash, cmd or PowerShell), HAR, `Set-Cookie`
lines and a Firefox profile's `cookies.sqlite`. Only the chosen origin's cookies
and headers are kept; an `Authorization` header and other hosts' cookies are
dropped and listed by name, never by value. Chrome is refused by name: its
cookies are not readable from outside the browser.

## OAuth and recovery codes

```text
unv oauth refresh Slack            # ONLINE: sends the refresh token to its token_url
unv codes status GitHub-codes
unv codes next GitHub-codes --reveal   # reading does not spend a code
unv codes use GitHub-codes             # mark the next one used
```

`oauth refresh` stores a rotated refresh token in the vault before it reports
anything, because the issuer has already invalidated the old one.

## Updating a standalone `unv`

```text
unv update check                       # ONLINE: reads the signed release manifest, installs nothing
unv update install                     # asks first; --yes skips the question
unv update install --manifest unv-update.json --archive unv-1.2.3-linux-x86_64.tar.gz   # offline (linux-aarch64, -musl and windows-aarch64 archives also exist)
```

Only a `unv` unpacked from a release archive (it sits beside `.unv-standalone`)
replaces itself. A package-manager install (`.deb`, `.rpm`, Scoop, winget,
Homebrew, Nix, `cargo install`) is refused and the error names the manager's own
update command; so is anything not recognisably either. The manifest is checked
against a pinned update key that is separate from the provider catalogue's, the
version must be strictly newer (no downgrade), and the archive's size and SHA-256
must match the signed manifest before anything is extracted. The old executable is
kept as `unv.rollback` until the new one reports the right `--version`, and is
restored if it does not. The running file is never edited in place.

- **Manual rollback:** unpack the older release archive over `unv`.
- **Proxy or offline:** the fetch honours the system proxy settings; on failure
  download `unv-update.json` and the archive from the release page and use
  `--manifest` and `--archive`.
- **Key rotation:** needs a release that still trusts the old key and pins the
  new one; a build that never pinned a key refuses every update.
