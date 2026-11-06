# UnENVerse paste guard (browser extension)

Stops you pasting a secret your vault holds into an AI chat. It matches the **exact values** your vault holds, so a key with no recognisable prefix is caught and ordinary text is not.

## How it works

1. A content script on the listed chat sites catches the `paste` event before the page sees it.
2. It sends the pasted text to the extension's background script, which passes it to the local program `unv guard host` over the browser's native-messaging channel (stdin and stdout of a process the browser starts itself).
3. The host matches the text against your vault and answers with the **names** of the entries found and where they sit in the text. It never sends a value, a hash or a fingerprint back.
4. If nothing is found, the paste goes through unchanged. If something is found you choose: cancel, replace each secret with its `${Entry}` reference, or paste anyway.

No network request is made by the extension or the host. The extension has one permission, `nativeMessaging`, and no host permission; the content script runs only on the sites in `manifest.json`.

## Install

```sh
unv login                                   # the host reads the vault through a cached session
unv guard install-host --browser firefox    # or chrome / chromium with --extension-id <id>
```

Then load `browser-extension/` as an unpacked extension (`about:debugging` in Firefox, `chrome://extensions` with developer mode in Chrome) and restart the browser. For Chrome and Chromium, copy the extension id shown on `chrome://extensions` into `--extension-id`; the host only answers the extension named in its manifest.

## What it cannot do

- It checks pastes, not typing, drag and drop, screenshots or text the page reads from elsewhere.
- It only knows secrets that are in the vault. Import the ones you keep elsewhere first (`unv scan`, `unv migrate leaks`).
- A locked vault cannot be checked: the paste goes through with a notice that it was not checked.
- Pasting into a site that is not in the list is not checked.
- Windows is not supported yet: the host needs a registry entry that `install-host` does not write.

## Tests

`tests/browser-extension.test.ts` pins the permission policy (a test fails if a host permission, a wider site match or a network permission is added), the rewriting logic and the paste interception.
