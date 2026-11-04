# Use UnENVerse from scripts

Run your tools with secrets from the vault without typing the master password each time.

1. In the desktop app: Settings, Security, switch on **Allow local scripts**.
2. On each entry a script may read, tick **Let local scripts read this**. From a terminal: `unv entry set Stripe --local-scripts true`.
3. Unlock the desktop. From then on, on the same computer:

```bash
unv exec --entry Stripe_KEY -- ./deploy.sh
unv render .env.template -o .env
unv get Stripe --field KEY --out key.txt
```

These readers use the unlocked desktop when no password was given. They read only the flagged entries and cannot change the vault. If the desktop is locked or closed they stop with exit code 7 at once; add `--allow-prompt` to be asked for the password instead, or `--no-bridge` to skip the desktop altogether.

Any program running as your own user can do the same, which is why the switch is off by default, per entry, and goes off when you lock or quit. Windows is not supported yet. See ADR-0172.
