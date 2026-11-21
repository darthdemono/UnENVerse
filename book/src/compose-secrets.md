# Use UnENVerse with Docker Compose secrets

Compose can hand a container a secret as a file under `/run/secrets/<name>`, and the official images (Postgres, MariaDB, and others) read it through a `*_FILE` variable. That keeps the value out of `docker inspect` and out of the process environment. UnENVerse supplies the file; Compose does the rest. This works for Linux containers.

## 1. Keep the value in the vault

```sh
unv entry add "Demo Postgres" --key "$(openssl rand -base64 24)"
```

## 2. Write it as a file Compose can mount

```sh
mkdir -p secrets
( umask 077; unv exec --entry "Demo Postgres=DB_PASSWORD" -- sh -c 'printf %s "$DB_PASSWORD"' > secrets/db_password )
```

`printf %s` writes the value without a trailing newline (a newline would become part of the password), and `umask 077` makes the file readable by you alone. Keep `secrets/` out of Git.

## 3. Point Compose at the file

```yaml
services:
  db:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password
    secrets:
      - db_password

secrets:
  db_password:
    file: ./secrets/db_password
```

Check the file before you start anything:

```sh
docker compose config -q
```

`config -q` prints nothing and exits 0 when the file is valid. Then `docker compose up -d`.

## 4. Values that are not secrets, or images with no `_FILE` variable

Put `${...}` references in a template and render it:

```sh
unv render env.template --out .env
```

An `.env` file is read by Compose itself and ends up in the container's environment, so use it for settings, and keep passwords in files where the image allows it.

## 5. Change the value

Write the file again (step 2), then recreate only the service that reads it:

```sh
docker compose up -d --force-recreate db
```

Changing the file does not change a running database's stored password; rotate the account in the database as well, and only revoke the old value once the service is healthy with the new one.

## 6. Let a node write and recreate it (R27, part B)

When the stack runs on another host, a node agent can keep the file there. Make a project of the
type **Compose secret** with one value, then add a target to the node's own config
(the hub cannot add one):

```toml
[[target]]
id = "db_password"
path = "/srv/stack/secrets/db_password"
project = "stack-db-password"
exporter = "compose-secret"
apply = true
owner = "999:999"                 # the user inside the Postgres image; omit to keep the node's user
compose_project = "stack"
compose_file = "/srv/stack/compose.yaml"
services = ["db"]
check = "http://127.0.0.1:8080/health"   # optional: a command, or an http URL that must answer 2xx
```

What the node does for a push, in order: write the file beside the target with mode `0400` (set
`file_mode` to change it) and the owner you named, run `docker compose -p stack -f … config -q`
against the written file, then `docker compose … up -d --no-deps --force-recreate db`, so only the
listed services restart, then the `check`. A failed step puts the previous file back with its mode,
and a failed `check` also recreates the service from it. The previous copy is kept only while the
change can still be rolled back; it is not left in the node's state folder afterwards. A node that
dies mid-write leaves the old file, never half of the new one (write, sync, rename). Each push
reports `written`, `validated`, `reloaded` and `checked` separately.

`compose_project` generates `validate` and `reload`, so a target that sets it cannot also set them.
`owner` is numeric (`uid` or `uid:gid`); giving a file to another user needs a node running as root.
Rotating the database account is still a separate step (section 5): revoke the old value only after
the service is healthy with the new one.
