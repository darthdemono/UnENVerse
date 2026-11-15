# Sites and login plans

## Several addresses per secret

A secret is matched by its name and id, never by one URL. An entry can list any number of addresses: a login page, an API, a service on your network with its port.

```
unv site add Sonarr localhost:8989
unv site add Sonarr https://sonarr.home.lan --label tv
unv site match http://127.0.0.1:8989
```

An address is a URL, `host:port` or a bare host. The path is dropped, because a credential belongs to a service and not to a page, and an address that carries a user name or password is refused. The older `api_url` counts as one more address.

Three rules decide whether a page may be offered a credential:

- `localhost`, `*.localhost`, `127.0.0.0/8` and `::1` are one host, so an entry saved for `localhost:8989` also answers at `127.0.0.1:8989`. The port stays significant: Sonarr and Radarr share a host and are different services.
- A bare address matches `https`, and matches `http` only on a loopback or private host, so a missing scheme never downgrades a public site to plain text.
- Hosts are compared after normalisation, so `Example.COM.` is `example.com` and a look-alike is not.

`unv site match` prints names and ids and never a value. The add and edit form has an Addresses box with one address per line.

## Site families

A family is a named set of hosts that share one entry, such as MetaBrainz: musicbrainz.org, listenbrainz.org and metabrainz.org.

```
unv site family add metabrainz musicbrainz.org listenbrainz.org metabrainz.org
unv site family use MetaBrainz metabrainz
```

An entry that names a family is offered on every host in it and its subdomains, never on a look-alike such as `evilmusicbrainz.org`, and over `https` only for a public host. Adding a host to the family extends every entry that uses it. A family is also what a recipe's `family_bound` lengthener shares a result across.

## Login plans

A login is one account made of up to four credentials, kept as a bundle, filled in a fixed order: the password, the 2FA code, the security phrase, then a recovery code. `unv login-plan BUNDLE` prints the steps that exist, in that order, whatever order the members were added in. A step with nothing stored is absent. The plan names entries and fields and never prints a value.

`--terminate-codes ask|auto|never` stores what happens to a recovery code once it has been filled: ask to mark it used, mark it used straight away, or leave it. Marking keeps the code text and adds `USED <date>`.

`unv codes import ENTRY FILE|-` adds the codes in a pasted text: one per line, numbered lists, dash or space groups, several on a line. Prose lines are skipped and a code already held is not added twice. `unv codes unuse ENTRY CODE` puts back a code a page rejected after it was marked used. `unv codes status` says how many are left, and the health check flags a set with two or fewer.
