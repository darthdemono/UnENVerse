# If something leaks

What to do for each kind of compromise, in order. Changing a password or key here never repairs a copy someone already took: a backup, a synced file or a disk image keeps the old secrets for good, so rotate what the copy held.

## One credential leaked

1. Rotate it at the provider first (`unv rotate` where an issuer exists, otherwise the provider's console). The old value is dead only when the provider says so.
2. Store the new value (`unv entry set`). The old one stays in the entry's history.
3. Remove the old value from local history with `unv entry forget NAME`. It cannot reach backups, nodes or the import source; it prints that list.
4. Redeploy to nodes, and read the audit log for use of the old value.

## The master password leaked

1. Change it: `unv vault rekey` (see ADR-0168). Anyone holding a copy of `vault.db` and `vault.salt` from before can still open that copy with the old password.
2. Treat every credential in a copy that left your control as leaked and rotate it (previous section).
3. Backups made earlier open with the old password. Make a new backup now and remove the old ones you do not need.

## The recovery identity leaked

Recovery packages are encrypted to an age recipient. Anyone with the identity and a package can read the vault inside it. Create a new recovery key (`unv recover` setup), make a fresh package, delete the old packages from every destination, then rotate the credentials the old packages held.

## An approver key leaked

An approver key can sign approvals for nodes that require them. Remove the approver, enrol a new one, and treat approvals signed since the leak as untrusted: re-run the changes they allowed. Nodes remember the highest hub epoch, so a replaced hub is also refused by epoch.
