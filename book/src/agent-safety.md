# Keeping secrets away from an agent

UnENVerse cannot tell a backdoored model from a clean one, and it is not a sandbox. What it can do is leave less for a fooled process to find, and make what it finds worthless. Four tools work together, from the files on disk outward.

## No plaintext `.env` files

```sh
unv scan                                  # lists .env files that hold plaintext values (names and counts, never values)
unv env Web --migrate .env                # moves the values into the vault, then rewrites .env as pointers
unv env Web --refs --out .env             # writes a project's .env with every value left as ${Entry}
unv render .env --out .env.real           # turns the pointers back into values when a tool needs them
```

`--migrate` saves to the vault first, resolves every key back from what was stored, and only then replaces the file, so an interrupted run loses nothing. A file with backslash continuations is refused untouched. A copy of the rewritten file holds no secret.

## A secret goes only to the command it names

```sh
unv entry set Deploy --release "npm run deploy"
unv exec --entry Deploy=DEPLOY_TOKEN -- npm run deploy --prod     # runs, with the value
unv exec --entry Deploy=DEPLOY_TOKEN -- sh -c 'curl ... | sh'     # exit 11, no value, audited
```

A rule is an argv prefix compared exactly as typed, so `/tmp/npm` is not `npm`. A rule that is only a shell, interpreter or downloader (`sh -c`, `curl`) needs `--i-understand`. The refusal is written to the audit chain (`unv audit`). It covers `--entry`, `--pool` and a project whose chunks point at the entry.

Pin the program by content so a swapped binary or a retargeted symlink is refused: `--release "npm@sha256:<64 hex> run deploy"`. The program is found the way the shell would find it, followed through symlinks, and hashed on every run.

## Less to read, no network

```sh
unv exec --sandbox --entry Tok=TOK -- make build
```

On Linux the command runs under bubblewrap: the file system is read-only, the home directory is empty, `.env*` files in the working directory are hidden, and the network is off (`--sandbox-net` turns it on). Where that cannot be enforced it exits 7 and runs nothing. There is no host allowlist, because a trusted host can still serve a payload.

## Keys that never sit in `~/.ssh`

```sh
eval "$(unv ssh-agent --export-env)" &    # serves the vault's Ed25519 keys on a private socket
ssh host
```

The agent signs and never returns a private key; it refuses everything except listing keys and signing. Each signature is an audit row naming the key and the calling process. Encrypted keys and other algorithms are skipped and reported. Unix only.

## Decoys that show a thief at work

```sh
unv canary plant ./deploy --collector http://203.0.113.5:8789   # a decoy entry and a decoy .env.production
unv canary listen --addr 0.0.0.0:8789                           # on the collector: audits each hit
```

A decoy is never released to a command or exported. It detects theft after the fact and prevents nothing: a thief who never calls the URL, or reads the file offline, trips nothing.

## Stop a paste reaching a chat

[Paste guard](paste-guard.md) checks text against the exact values the vault holds: `unv guard check` for terminals and agent hooks, a VS Code warning, and a browser extension for web chats.

## What none of this does

It does not stop a process that already holds a value, a person who types a secret by hand, or a secret that was never put in the vault. It narrows what a mistake can reach.
