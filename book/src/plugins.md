# Plugins

A plugin is a program that adds a destination, a notifier, an issuer or an importer. It lives in its own directory under the `plugins` folder beside the vault, next to a `plugin.json`. Plugins run with your rights. UnENVerse supervises them (timeouts, output caps, a cleared environment, scrubbed stderr, a hash check before each start) but does not sandbox them, so install only programs you would run yourself.

```json
{
  "name": "ntfy-notifier",
  "version": "1.0.0",
  "protocol": "unv-plugin-rpc/1",
  "exec": "plugin.py",
  "capabilities": ["notifier"],
  "slots": ["url"]
}
```

`exec` is a path inside the plugin's directory. `capabilities` is one or more of `destination`, `notifier`, `issuer`, `importer`. `slots` name the credentials you bind to a plugin; each arrives as `UNV_SLOT_<NAME>` in the environment, never as an argument, and the plugin sees nothing else from the vault.

## Commands

```sh
unv plugin ls                    # installed, and found in the plugins folder
unv plugin install ntfy-notifier # records the program's SHA-256; enables nothing
unv plugin enable ntfy-notifier  # shows capabilities and "can act as you", asks first
unv plugin test ./ntfy-notifier  # the conformance suite, no vault needed
```

Installing again accepts a changed program and disables the plugin until you enable it. A program that changed since it was installed is refused before it starts.

## Protocol `unv-plugin-rpc/1`

One JSON object per line on stdin and stdout. The host sends `{"id":N,"method":"...","params":{...}}`; the plugin answers `{"id":N,"result":{...}}` or `{"id":N,"error":{"message":"..."}}`. A line is at most 1 MiB, stderr is capped at 64 KiB and scrubbed before it is shown.

| Method      | Params                            | Result                                       |
| ----------- | --------------------------------- | -------------------------------------------- |
| `handshake` | `host`, `protocol`                | `protocol`, `name`, `version`                |
| `plan`      | `op`, `params`                    | `steps`. No side effects, same answer twice  |
| `execute`   | `op`, `params`, `idempotency_key` | `status`. The same key gives the same result |
| `reconcile` | `op`, `idempotency_key`           | `state`: `done`, `not_done` or `unknown`     |
| `shutdown`  | none                              | `{}`, then the plugin exits                  |

A call that gets no answer in time kills the plugin and counts as lost: for `execute` the step is uncertain and is reconciled, never repeated blind. `unv plugin test` checks the handshake, that `plan` writes nothing, that `execute` is idempotent, that `reconcile` answers, that the plugin exits within two seconds of `shutdown`, and that it does not write a credential slot's value to stderr.

See `examples/plugins/ntfy-notifier` for a complete plugin in under 80 lines.
