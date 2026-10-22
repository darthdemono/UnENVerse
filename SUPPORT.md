# Support

UnENVerse is free software kept by one maintainer in spare time. Help is given in good faith and on a best-effort basis; there is no support contract and no paid tier.

## Where to ask

- **A bug or something that does not work as documented:** open a bug report. Attach the output of `unv support bundle` (it holds versions, counts and check results, never a value, a name, a path or a host; read it before you send it) and, if you can, a reproduction on a demo vault made with `unv demo-vault --out DIR`.
- **A feature or an integration you need:** open a feature request and name the tool and its version.
- **A security problem:** do not open a public issue. Use the private form described in [SECURITY.md](SECURITY.md); reports are acknowledged within seven days.

## What to expect

- Bugs that lose data or expose a secret come first. Everything else is read when time allows, usually within a week or two, sometimes longer.
- One maintainer cannot own every integration. A request for a new connector or exporter is easier to accept when it comes with a fixture, a test and a named person who uses the tool.
- Questions that the book answers are pointed at the book. Please do not paste secrets, vault files or `--reveal` output anywhere.
