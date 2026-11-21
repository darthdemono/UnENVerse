# Home Assistant secrets

Home Assistant reads values written as `!secret name` from a plaintext `secrets.yaml` beside `configuration.yaml`. UnENVerse can keep the real values in the vault and write that file from a template, with a check before it goes live. The file on the Home Assistant host stays plaintext; this does not change that.

## 1. Keep the values in the vault

```sh
unv entry add "HA Postgres" --key "$(openssl rand -base64 24)"
unv entry add "HA API token" --key "…"
```

## 2. Write a template

`secrets.yaml.tpl` holds the file with `${Entry}` references instead of values:

```yaml
recorder_db_url: postgresql://homeassistant:${HA Postgres}@db/homeassistant
long_lived_token: ${HA API token}
```

The template holds no secret, so it can live in your configuration repository.

## 3. Render and check

```sh
unv render secrets.yaml.tpl --strict --out /tmp/secrets.yaml
hass --script check_config -c /config   # with the new file in place; Home Assistant OS and Container have this command
```

`--strict` fails when a reference cannot be resolved, so a typo never becomes an empty secret. `--out` writes the file readable by you alone, and only a file receives real values; stdout shows fingerprints.

Keep the previous file until the check passes, then move the new one over it:

```sh
cp /config/secrets.yaml /config/secrets.yaml.prev
mv /tmp/secrets.yaml /config/secrets.yaml
hass --script check_config -c /config || mv /config/secrets.yaml.prev /config/secrets.yaml
```

## What to know

- The values are plaintext on the Home Assistant host, and Home Assistant backups contain `secrets.yaml`. Protect the host and the backups accordingly.
- Automations and scripts edited in the Home Assistant UI cannot use `!secret`; only YAML files can.
- Whether a node can run beside Home Assistant depends on the install: on **Home Assistant
  Container** and **Core** the host can run the agent; **Home Assistant OS** has no host shell for it
  (the add-on or SSH environment is not where `secrets.yaml` is meant to be managed), so use the manual recipe there.

## Let a node do it

Make a project of the type **Home Assistant secrets**: one chunk per secret, the chunk name is the
`!secret` name, the value is a `${Entry}` reference. A name used twice is an error, because the
second line would replace the first. The generated file is plain `name: value` lines; a value that
YAML would read as a number or boolean (a PIN of `123456`) is quoted.

On the Home Assistant host, in the node's own config:

```toml
[[target]]
id = "ha-secrets"
path = "/config/secrets.yaml"
project = "ha-secrets"
exporter = "homeassistant"
apply = true
validate = "docker exec homeassistant python3 -m homeassistant --script check_config -c /config"
reload = "docker restart homeassistant"
check = "http://127.0.0.1:8123/manifest.json"
```

The node writes the file, runs the check, and when it fails puts the previous `secrets.yaml` back
and does not reload. After the reload it waits for the `check` URL to answer, and when it does not it
puts the old file back and restarts again. CI runs Home Assistant's own `check_config` on the
generated file, and a control that names a secret the file does not hold must fail.
