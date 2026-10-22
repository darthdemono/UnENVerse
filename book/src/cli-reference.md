# Command reference

Written from `unv describe` by `unv-cli/tests/contract_surface.rs`; do not edit it by hand. A command added or renamed in code changes this page, and the test fails until it is regenerated (`UNV_UPDATE_CONTRACT=1 cargo test -p unv-cli --test contract_surface`). Run `unv <command> --help` for the flags of one command.

Contract version 1.

| Command | What it does |
| --- | --- |
| `unv audit` | Show the append-only audit log, or verify its hash chain |
| `unv backup archive <file>` | Archive the vault database **and its salt** into one encrypted file |
| `unv backup dest add local <id> <path>` | A local path or mounted NAS share. The mount must already be available |
| `unv backup dest add sftp <id>` | A pin-verified SFTP directory. The credential is read from the entry's api_key field |
| `unv backup dest ls` |  |
| `unv backup dest rm <id>` |  |
| `unv backup drill` | Download, restore and verify the latest completed package without activating it |
| `unv backup export <file>` | Write an encrypted .vaultbak the desktop app can restore |
| `unv backup import <file>` | Restore a .vaultbak, replacing the current vault |
| `unv backup package` | Write an age-encrypted streamed recovery package to a named destination |
| `unv backup recovery-key` | Create and print a recovery identity exactly once. An encrypted internal copy is retained only while the vault is unlocked to run restore drills |
| `unv backup restore-archive <file>` | Restore a vault + salt archive written by `backup archive` |
| `unv backup schedule add` |  |
| `unv backup schedule drill` | Schedule a monthly restore drill. Defaults to 03:00 UTC on the first day |
| `unv backup schedule ls` |  |
| `unv blast-radius` | Which credentials were on a machine, and when (Phase 36): the entries to rotate, only those whose exposed value is still live, and one command that rotates exactly them. `--host NODE` asks the hub; `--host local` reads this machine's materialisation log |
| `unv bundle add <bundle> <entry>` | Add an existing entry to a bundle under a slot name |
| `unv bundle delete <bundle>` | Delete the bundle AND all of its member entries |
| `unv bundle dissolve <bundle>` | Return every member to the grid. Deletes nothing |
| `unv bundle ls` | List bundles with their member slots and local variable names |
| `unv bundle new <name>` | Create a bundle; each `--member SLOT=ENTRY` joins an existing entry |
| `unv bundle remove <bundle> <slot>` | Detach the member in a slot; the entry itself is kept |
| `unv calendar export` | Write an iCalendar (.ics) feed of every date the vault knows |
| `unv calendar feed ls` | List feeds. The owner sees every feed; a sub-user sees only their own |
| `unv calendar feed new` | Mint a feed. The token is shown **once** — `unv-server` stores only its hash — so this refuses to print it without `--reveal`/`--out`, the same rule as `user token new` |
| `unv calendar feed revoke <id>` | Revoke a feed by id. Irreversible — the URL stops working immediately |
| `unv catalogue diff` | What the cached catalogue adds to, or changes in, the compiled table |
| `unv catalogue export` | Publisher: dump the compiled table as providers JSON, to seed a source file |
| `unv catalogue show` | Which table enrich is using: the cached catalogue or the compiled one |
| `unv catalogue sign <input>` | Publisher: stamp and sign a providers JSON file with an Ed25519 seed |
| `unv catalogue update` | Fetch the whole catalogue, verify it against the pinned key and cache it |
| `unv category add <name>` |  |
| `unv category ls` |  |
| `unv category rename <name> <new_name>` |  |
| `unv category rm <name>` | Delete a category and its slash-nested children |
| `unv check [project]` | Cross-chunk checks on a project's config: an nginx proxy_pass to a service that is not defined, two WireGuard peers claiming one network, a Traefik middleware that does not exist, and three more. Prints names, never values |
| `unv class add <name>` |  |
| `unv class ls` |  |
| `unv class rm <class>` |  |
| `unv class set <class>` | Replace a class's name, description and capabilities |
| `unv codes next <entry>` | The next unused code. Reading does not consume it; `use` does |
| `unv codes status <entry>` | How many codes are left. Prints no codes |
| `unv codes use <entry> [code]` | Mark a code used (the first unused one when CODE is omitted) |
| `unv completions <shell>` | Generate shell completion scripts |
| `unv cookie curl <provider>` | A `curl` command carrying the jar and its User-Agent |
| `unv cookie header <provider>` | The `Cookie:` header value — what a request actually sends |
| `unv cookie import <file>` | Import a DevTools capture into a web-session entry |
| `unv cookie json <provider>` | The browser-extension array shape, so a jar round-trips back into a browser |
| `unv cookie txt <provider>` | Netscape `cookies.txt`, for `curl -b` and `yt-dlp --cookies` |
| `unv curl <provider> [url]` | Build a `curl` command that sends one entry's credential |
| `unv cxf export` | Write every entry as a CXF document. Materialising by construction — refused without --out, the same Phase 14 rule every export here obeys |
| `unv cxf import <file>` | Read a CXF file. Every item is appended as one or more new entries — see `vault_core::cxf`'s module doc for the one-credential-vs-several (bundle) rule and why this is append-only rather than merge-aware |
| `unv demo-vault` | Write a synthetic vault with invented values into an empty directory, for bug reports. Needs no password; the vault's own password is printed |
| `unv describe` | Print the machine-readable contract: commands, flags, exit codes, schemas |
| `unv diff <a> <b>` | Compare two entries field by field. Secrets show as fingerprints unless `--reveal`, so equal and different are answerable without reading either |
| `unv doctor` | Check the vault: database integrity, salt, file modes, audit chain, pools |
| `unv emit <entry>` | Write a credential in the file its tool reads: `.npmrc`, `pypirc`, Docker `config.json`, a DSN, libpq variables, a Wi-Fi QR string |
| `unv enrich` | Infer and fill entry metadata from names and public key prefixes |
| `unv entry add <provider>` | Create a new entry |
| `unv entry compromise <provider>` | Mark an entry as known-leaked (a critical health issue until rotated) |
| `unv entry forget <provider>` | Purge the entry's old values from this vault's history and stored config copies |
| `unv entry get <provider>` | Print one entry, or one of its fields |
| `unv entry history <provider>` | Show previous values of an entry's secret |
| `unv entry ls` | List entries (same filters as `unv list`) |
| `unv entry pin <provider>` | Pin an entry to the top of every view |
| `unv entry rename <provider> <new_name>` | Rename an entry, rewriting every ${ref} that points at it |
| `unv entry restore <provider>` | Restore a previous value by its position in `entry history` |
| `unv entry rm <provider>` | Delete an entry (its credential keeps working at the provider; revoke it there) |
| `unv entry rotate <provider>` | Record a rotation, optionally storing the new secret |
| `unv entry set <provider>` | Change fields on an existing entry |
| `unv entry tag <provider>` | Add or remove tags |
| `unv entry timeline <provider>` | Show secret-free lifecycle events and credential generations |
| `unv entry verify <provider>` | Record that a stored browser session still works |
| `unv env <project>` | Resolve a project's env_file chunks into a deployable .env (${refs} resolved) |
| `unv exec <argv>` | Run a command with secrets in its environment — they never reach stdout |
| `unv export` | Export vault entries |
| `unv file write <provider>` | Write the credential to disk and print the `.env` line that names it |
| `unv gen cert <common_name>` | A self-signed certificate and its private key |
| `unv gen password` | A password from the selected character sets |
| `unv gen secret` | Random bytes as hex / base64 / base64url |
| `unv gen sources` | List entropy sources and whether each one works on this machine |
| `unv gen ssh` | An ed25519 SSH keypair |
| `unv get [provider]` | Print full details of a single entry |
| `unv history diff [project]` | Unified diff between two snapshots of one config; by default the newest two. A rotated secret shows as a changed fingerprint |
| `unv history diff-file <file> [project]` | Compare a stored snapshot with a file on this machine - typically a node's live config fetched with `unv node pull`. Without `--reveal` only the names of the differing lines are shown, never what follows them |
| `unv history ls [project]` | List snapshots, newest first. Shows names, times and hashes, never content |
| `unv history policy` | Show or change the retention policy and whether history is on |
| `unv history prune` | Delete snapshots that are both beyond the newest `--keep` and older than `--days`, leaving a checkpoint. Deletes the only record of what was deployed then |
| `unv history show <seq>` | Show one snapshot. Fingerprinted unless `--reveal`; `--out FILE` writes the real file (0600) |
| `unv history snapshot [project]` | Record a snapshot of every config that changed, now |
| `unv history stats` | Snapshot count, size and oldest |
| `unv history verify` | Recompute every hash and chain and check each checkpoint against the audit chain |
| `unv history where <sha256>` | Which snapshot a file's SHA-256 came from (e.g. the hash a node reports) |
| `unv import <file>` | Import entries from a .env file (creates/updates env_var entries) |
| `unv import-service <service>` | Import from a secrets service: OpenBao, Infisical, Bitwarden Secrets Manager or AWS |
| `unv import-vault <vendor> <file>` | Import from another password manager or a Nextcloud `config.php` |
| `unv list` | List vault entries (table view) |
| `unv login` | Authenticate once and cache the session (remote servers) |
| `unv logout` | Forget cached sessions |
| `unv migrate apply <format> <file>` | Save imported entries, source identity and encrypted archives together |
| `unv migrate archive export <id>` | Write one encrypted archive record to a user-selected private file |
| `unv migrate archive ls` |  |
| `unv migrate cutover <migration>` | Make this vault the authority for the entries a Git-store import (SOPS, pass, gopass) created |
| `unv migrate drift <migration>` | Compare a Git source, as it is now, with what was last imported. Read-only on both sides |
| `unv migrate plan <format> <file>` | Inspect a source without changing the vault |
| `unv migrate report` | List migration summaries without source values or file contents |
| `unv migrate sync <migration>` | Refresh the vault from the source this migration read, one way. Preview unless `--apply` |
| `unv migrate uncutover <migration>` | Hand authority back to the source. Refused when a credential changed after cutover |
| `unv migrate verify <migration>` | Re-render a template migration and compare its SHA-256 with its source |
| `unv node accept <node> <target>` | Record a pull target's current file as accepted into the vault, so it reads `in_sync` until it changes again |
| `unv node approvals [node]` | Pushes waiting for a human, with `--all` the decided ones too |
| `unv node approve <approval>` | Approve one held push. Shows the diff against what the node has and the hash being approved first (secrets as fingerprints unless `--reveal`); `--yes` skips the question for someone who has looked. The approval is for those exact bytes and lapses in an hour |
| `unv node approver ls` | Devices the hub accepts |
| `unv node approver register` | Tell the hub this machine's key counts as your approval (hub, owner only) |
| `unv node approver rm <device>` | Stop accepting a device, by (at least 8 characters of) its fingerprint |
| `unv node approver show` | This machine's approver public key and fingerprint (the key is made on first use, 0600). Put the public key in a node's config as `approver` |
| `unv node automation-key` | Show the hub's automation public key for a node's `automation_approver` |
| `unv node check` | Validate a node config and show what it would report. Contacts no one |
| `unv node enroll` | On the managed host: spend an enrollment token and save this node's identity. Nothing is read from the vault |
| `unv node install` | On the managed host, as root: install the agent as a hardened systemd service, enrol it, and give it access to exactly the files you name. `--plan` prints every step without running any |
| `unv node ls` | List enrolled nodes and each target's status (hub) |
| `unv node policy <node>` | Hold every push to a node for a human (`required`), or stop (`none`) |
| `unv node pull <node> <target>` | Fetch a pull target's file from the node. Written with `--out` (0600); there is no stdout form, because the file is the node's own and holds its secrets |
| `unv node reject <approval>` | Reject one held push; the same bytes are not asked about again |
| `unv node revoke <node>` | Stop a node receiving config. Files already written stay where they are |
| `unv node run` | On the managed host: beat to the hub and carry out what this node's own config allows. Runs until stopped. A node enrolled with `--listen` waits for the hub to dial it instead |
| `unv node show <node>` | One node in full: key fingerprint, host, and every target (hub) |
| `unv node token new <name>` | The token is a one-time credential: written to `--out` (0600), or printed only with `--reveal` |
| `unv oauth refresh <entry>` | Refresh an `oauth_client` entry's access token at its `token_url` |
| `unv ops cancel <id>` |  |
| `unv ops ls` |  |
| `unv ops run` | Run an immediate package to the named destination |
| `unv ops show <id>` |  |
| `unv perm check <expression>` | Parse an expression without storing it |
| `unv perm set <kind> <subject>` | Set the read and/or write expression. An empty value denies everything |
| `unv perm show <kind> <subject>` | Show the read/write expressions for a user or class |
| `unv plugin disable <name>` | Stop a plugin being called |
| `unv plugin enable <name>` | Allow a plugin to be called, after showing what it declares |
| `unv plugin install <name>` | Record a plugin found in the plugins directory (disabled) |
| `unv plugin ls` | List installed plugins and the ones found in the plugins directory |
| `unv plugin rm <name>` | Forget an installed plugin (its files stay on disk) |
| `unv plugin test <path>` | Run the conformance suite on a plugin directory |
| `unv pool ls` | List every pool in the vault with member counts and cooldowns |
| `unv pool next <pool>` | Take the next usable key and advance the cursor |
| `unv pool report <pool> [member]` | Report a key as rate limited (or recovered) |
| `unv pool reset <pool>` | Forget a pool's cursor, cooldowns and counts on this machine |
| `unv pool show <pool>` | Show each member of one pool: use count and whether it is cooling |
| `unv project add <name>` | Create a project, with starter chunks for its type |
| `unv project chunk add <project> <name>` | Add an empty chunk |
| `unv project chunk disable <project> <chunk>` | Exclude a chunk from exports without deleting it |
| `unv project chunk enable <project> <chunk>` | Re-include a disabled chunk |
| `unv project chunk ls <project>` | List a project's chunks |
| `unv project chunk rename <project> <chunk> <new_name>` | Rename a chunk, rewriting ${chunk:…} references to it |
| `unv project chunk rm <project> <chunk>` | Delete a chunk |
| `unv project chunk set <project> <chunk> [pairs]` | Set fields as key=value pairs |
| `unv project chunk show <project> <chunk>` | Print a chunk as its native config text (or raw JSON with --raw) |
| `unv project chunk unset <project> <chunk> [keys]` | Remove fields by key |
| `unv project export <project>` | Export a project's config in its native format |
| `unv project ls` | List projects with entry and chunk counts |
| `unv project rename <project> [new_name]` | Rename a project, carrying its sub-projects and entry links |
| `unv project rm <project>` | Delete a project; sub-projects are promoted to top level |
| `unv project show <project>` | Print a project as JSON, chunks included |
| `unv recover inspect <package>` | Read non-secret envelope metadata without decrypting the package |
| `unv recover restore <package>` | Decrypt into an empty directory after validating every member |
| `unv recover verify <directory>` | Verify hashes, SQLCipher, storage, audit and configuration history |
| `unv render [template]` | Substitute ${refs} in a template file (use `-` for stdin) |
| `unv reset-vault` | Delete the local vault and its salt (the app's Settings -> Reset) |
| `unv rotate plan <entry>` | Show the steps without changing anything |
| `unv rotate run <entry>` | Rotate now |
| `unv rotate status [run]` | List rotation runs, or show one |
| `unv rotate-check` | List entries expiring within N days (default: 30) |
| `unv scan` | Health scan — weak, expiring, duplicated and stale-reference secrets |
| `unv sessions` | List the cached sessions: which servers, which users, which is default |
| `unv shield <argv>` | Run a command while replacing known vault values in its text output |
| `unv status` | Where this CLI is pointed and what the vault holds |
| `unv support bundle` | Versions, counts and check results of the local vault: no values, names, paths or hosts |
| `unv tags` | List every tag in the vault with its entry count |
| `unv template ls` | List the presets |
| `unv template show <id>` | Show what a preset pre-fills and which fields it expects |
| `unv totp add <name>` | Guided add: attach a seed to an existing entry (matched by exact provider name) or create a bare `password`-typed one with an empty primary. Mirrors the desktop Authenticator panel's "Add 2FA" form |
| `unv totp advance <provider>` | Advance a counter-based (HOTP) seed to its next position |
| `unv totp code <provider>` | Print the current code for an entry |
| `unv totp export` | Write every stored seed in a format another authenticator reads |
| `unv totp import <file>` | Import seeds from another authenticator app's export |
| `unv totp ls` | List the entries that carry a seed. Names and parameters, never codes |
| `unv totp rm <provider>` | Forget an entry's seed. The old value stays in `entry history` |
| `unv totp uri <provider>` | Write the otpauth:// URI, for enrolling a replacement phone |
| `unv uid check [values]` | Advisory only: "unique right now". `register`/`mint` are the operations that actually reserve a value |
| `unv uid lookup <value>` | One value → issued/unknown plus the metadata the caller may see |
| `unv uid mint` | Generate in-process, check, retry up to 3 times on collision, register, and return the value that was actually stored |
| `unv uid prune <before>` | Delete registry rows before a date. **Deletes the only evidence an ID was issued** — a pruned ID can be issued again undetected. Dry-run first |
| `unv uid register [values]` | Record values minted elsewhere. Fails closed per value on a collision — nothing else in the batch is rolled back |
| `unv uid stats` | Row count, size on disk, oldest entry |
| `unv update check` | Say whether a newer signed release exists. Installs nothing |
| `unv update install` | Download, verify and install the newer release over this executable |
| `unv update sign` | Publisher: build and sign the manifest for a release's archives |
| `unv use [project]` | Pin a project and environment to this directory (writes `.unv.json`) |
| `unv user add <username>` | Create a user. Prompts for a password unless --no-password |
| `unv user class <user> [class]` | Assign a class, or remove the current one with --none |
| `unv user ls` |  |
| `unv user passwd <user>` | Set or clear a user's password |
| `unv user rename <user> <new_name>` |  |
| `unv user rm <user>` |  |
| `unv user strict-write <user>` | Require a write to match EVERY scope, not any one of them |
| `unv user token ls <user>` |  |
| `unv user token new <user>` | Mint a token. Write it to a file with --out so it never reaches stdout |
| `unv user token revoke <user> <token_id>` |  |
| `unv user totp confirm <user> <code>` | Phase two: prove the authenticator works, which enables the factor |
| `unv user totp disable <user>` | Remove the factor and destroy the secret |
| `unv user totp enroll <user>` | Phase one: mint a secret. Does **not** enable the factor |
| `unv user totp status <user>` | Show whether a user is enrolled and whether the factor is enabled |
| `unv vault activate` | Make this instance the live one (asks first; the old instance must be retired) |
| `unv vault rekey` | Change the master password (re-encrypts the vault; asks first) |
| `unv vault status` | Show the vault id, the instance id, its epoch and role |
| `unv watch <file>` | Watch a .env file for changes and sync into the vault |
| `unv whoami` | Show which identity this CLI would use, and whether its session still works |
