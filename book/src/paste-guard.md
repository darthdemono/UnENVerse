# Paste guard

`unv guard check` reads text on stdin and tells you whether it contains a value your vault holds. It matches the exact values, so a secret with no recognisable prefix is found and ordinary text is not. It prints the entry and field names, never a value, a hash or a fingerprint, and it makes no network call.

```sh
printf '%s' "$TEXT" | unv guard check
```

Exit 0 means no value of eight or more characters was found. Exit 10 means one was; the names are printed. A shorter value is reported as a warning and does not fail the check, because short strings occur in text by chance. Exit 7 means the vault could not be opened: a locked vault is never read as clean. The check needs the vault password, a cached `unv login` session or `UNV_PASSWORD`; it does not use the desktop bridge, because that serves only the entries you flagged for scripts and would miss the rest.

## Stop a prompt reaching an agent

A Claude Code `UserPromptSubmit` hook receives the prompt as JSON on stdin. Exit code 2 blocks the prompt, so the hook turns exit 10 into 2:

```sh
#!/bin/sh
# .claude/hooks/paste-guard.sh
jq -r .prompt | unv guard check >&2 || exit 2
```

## What it cannot stop

Screenshots, typing a secret by hand, a secret that is not in the vault, and a locked vault. Import secrets you keep elsewhere first with `unv scan` and `unv migrate leaks` so they become matchable.
