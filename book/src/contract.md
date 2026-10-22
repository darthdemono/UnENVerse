# The scripting contract

This page lists what a script, an integration or another tool can rely on when it calls `unv`. Anything not listed here may change in any release. The contract has a version, `contract_version` in the output of `unv describe`; it moves when something listed here changes in a way that breaks a caller.

## Stable

- **The binary name** is `unv`. Environment variables start with `UNV_`; the data directory is `io.unenverse`.
- **Commands and flags** that `unv describe` lists. `tests/fixtures/contract/surface.txt` holds every command path, positional argument and flag name, and a test fails when one is removed or renamed. Adding a command or a flag is allowed and is mentioned in the changelog under "Contract".
- **Exit codes**: 0 ok, 1 error, 2 usage, 3 not found, 4 ambiguous, 5 denied, 6 conflict, 7 unavailable (including a locked vault), 8 needs confirmation, 9 redacted, 10 invalid. `unv describe` lists them with their meaning.
- **The JSON envelope** with `--json`: stdout is exactly one document, `{"ok":true,"command":"...","data":...}` or `{"ok":false,"error":{"code":"...","message":"..."}}`. New keys may be added to `data`; existing keys are not renamed.
- **Redaction**: a secret value is never printed unless the caller asked (`--reveal`, `--out`, `exec`, `render`). A redacted value is `{"redacted":true,"fingerprint":"sha256:<12 hex>","length":n}` in JSON.
- **Reference syntax**: `${NAME}` and `${NAME.field}` in config fields and templates, resolved by `exec`, `render` and exports.
- **Lookups** by commands that act on one entry (`entry history`, `entry set`, `exec --entry` and the like) resolve by exact name, then by a unique case-insensitive part of it; an ambiguous name is refused with exit 4, never guessed. `provider:key_id` disambiguates. `get` and `list` are searches and return every match.
- **File formats**: the `.vaultbak` envelope, recovery packages (age) and the node protocol domains `envv-node-v1`, `envv-hub-v1`, `envv-approval-v1` and `envv-history-v2`. The old `envv` spelling stays in these on purpose.

## Not stable

Human-readable text output (without `--json`), the wording of messages, progress output, the order of keys and of list items unless a command documents one, the contents of `details` in an error, anything in `book/src/adr` marked as superseded, and the internal Rust crates (`unv_app`, `vault_core`) as a library.

## Deprecation

A listed command or flag is not removed in the release that deprecates it. The release notes say so under "Contract", `--help` says so, and the removal comes at least one minor version later with `contract_version` raised.

## Recipes

Each of these is run by a test (`unv-cli/tests/contract_recipes.rs`), so they stay true.

```sh
# Read one field into a variable (the caller asked for the real value)
unv --reveal get GitHub --field api_key --json

# Run a program with secrets in its environment, never on a terminal
unv exec --entry GitHub=GH_TOKEN -- ./deploy.sh

# Fill a template and write the result to a file
unv render template.txt --out config.ini

# Write one secret to a file that only you can read
( umask 077; unv exec --entry GitHub=TOKEN -- sh -c 'printf %s "$TOKEN"' > ~/.config/tool/token )

# Write a file stored in an entry (a certificate, a key file) to disk
unv file write Cert --out ./server.pem

# Find out whether a value changed, without reading it
unv get GitHub --field api_key --json    # compare data.value.fingerprint
```

## Built in and maintained

The exporters, importers and issuers that ship in the binary (`unv describe` and the Features page list them) are built in and follow this contract. Anything installed with `unv plugin` is maintained by whoever wrote it and follows the plugin protocol instead.
