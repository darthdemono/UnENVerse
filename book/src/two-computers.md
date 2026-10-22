# Using UnENVerse on two computers

A vault has one live instance. Two computers that both believe they are the vault, with the same hub identity, can push different things to the same node, so UnENVerse gives every vault an id and every copy an instance, an epoch and a role.

## What to do

- **A second desktop that only reads and edits**: connect it to the first as a remote vault (LAN or server mode). One database, two windows.
- **A copy for safety**: use `unv recover package` and keep the package somewhere safe. Do not copy `vault.db` by hand or put it in a synced folder.
- **Moving to a new computer**: restore the recovery package there (`unv recover restore`), check it (`unv recover verify`), retire the old computer, then run `unv vault activate` on the new one.

## Instances

`unv vault status` shows the vault id (for life), the instance id (new for every restore), the epoch (only grows) and the role.

A restored vault starts as **standby**. It can be opened, read, verified and exported. It does not push to nodes, does not run scheduled operations or rotations, and `unv doctor` says so. `unv vault activate` makes it the live instance after asking whether the old one is retired, and raises the epoch above anything it has seen. If you activate the wrong machine, run `unv vault activate` on the intended one: the higher epoch wins.

## Nodes

The hub tells each node `(vault id, epoch)` with every reply. A node remembers the highest epoch it has seen for its vault and refuses a lower one with a message naming `unv vault activate`; it refuses a hub for a different vault. A standby hub answers as locked, so nodes keep observing and wait.

Upgrade nodes before the hub: an older node ignores the new fields and keeps working, but it does not protect you from a stale hub.

## Synced folders and network drives

`unv doctor` and `unv vault status` warn when `vault.db` is inside a Dropbox, OneDrive, Google Drive, iCloud Drive, Nextcloud, ownCloud, pCloud, MEGA, Box or Syncthing folder, or on a network filesystem. Those tools copy files while SQLite is writing them and resolve conflicts by making a second copy; the vault does not survive either.
