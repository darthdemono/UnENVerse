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
