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
- This recipe is a manual one. A node target that renders, checks and rolls back for you needs the desktop hub (R12), which is not finished, so it is not offered here. It has not been run against a live Home Assistant in CI.
