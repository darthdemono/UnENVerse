# Secret phrases and recovery codes

Two kinds of secret are words and single-use codes rather than one string: a seed or recovery phrase, and the backup codes a service gives you for when your second factor is gone. Both are kept encrypted like everything else, masked by default, and never exported, rendered or put in an environment.

## Secret phrases

Pick the type **Secret Phrase**. The words are the value, separated by single spaces: paste a whole phrase, a numbered grid or lines of words and the form joins them. Three variables sit beside it: `wordlist` (a name for the list the words come from, empty or `bip39-english` for the standard one), `passphrase` (a 25th word, secret) and `derivation_path`.

A BIP39 phrase is checked when you save it and by `unv scan`: the word count, every word in the list and the checksum. A failed check is a warning you can keep, never a silent change, and the message never repeats a word. A phrase whose `wordlist` names another list is not checked.

For a login that asks a memorable phrase or a security answer beside the password, use **Security Phrase** instead; it is filled third in the [login plan](sites-and-login-plans.md).

## Recovery codes

The type **Recovery Codes** keeps one code per line in its `codes` variable. A used code keeps its text and gains `USED <date>`, so you can still tell which you burned. Reading a code never spends it: only you know the service accepted it.

In the desktop, **Tools, Recovery codes** lists each account's codes, masked until you tick **Show the codes**, with how many are left (a warning at two or fewer). **Mark next used** asks first. **Restore** puts back a code a page rejected. **Add the codes** takes the text the service showed you, in any of the common layouts (one per line, numbered, dash or space groups, several on a line) and skips prose and codes already held. **Print sheet** prints the unused codes as a plain page and empties it again; nothing leaves the machine.

On the command line:

```
unv codes status ENTRY        # how many are left, no codes printed
unv codes next ENTRY          # the next unused code (needs --reveal)
unv codes use ENTRY [CODE]    # mark one used
unv codes unuse ENTRY CODE    # put one back
unv codes import ENTRY FILE   # add pasted codes (- for stdin)
```

The parsing, adding and restoring are one Rust implementation; the desktop calls the same code over IPC.

Using a code writes an audit row "recovery codes changed" with the acting user, because the code list is the only thing that changes.

## Not here yet

A recovery phrase and recovery codes for the vault itself (a second way to unlock a forgotten master password) change the threat model and are designed separately with the key slots.
