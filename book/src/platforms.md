# Platforms

|                                                         | Linux                                                   | Windows                                         | macOS                                         |
| ------------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------- | --------------------------------------------- |
| Desktop app and `unv` release builds                    | x86_64, arm64 (glibc), x86_64 musl for the command line | x64, Arm                                        | built and tested in CI only, no release asset |
| SQLCipher                                               | system library (AppImage and packages bundle it)        | vendored                                        | vendored                                      |
| Owner-only files                                        | mode 0600 / 0700                                        | protected access list holding only your account | mode 0600 / 0700                              |
| Device key (`approver.key`)                             | `~/.local/share/io.unenverse`                           | `%LOCALAPPDATA%\io.unenverse`, never roaming    | `~/Library/Application Support/io.unenverse`  |
| Node agent as a service                                 | systemd (`unv node install`)                            | Task Scheduler, by hand                         | launchd, by hand                              |
| `unv doctor` checks permissions and network filesystems | yes                                                     | yes                                             | yes                                           |

macOS is not released: there is no Apple Developer account behind the project, so nothing would be signed or notarized, and an unsigned app is blocked by Gatekeeper on first launch (right-click, Open). The CI job builds and tests it so the code does not rot; a release is the maintainer's decision.

## Owner-only files on Windows

Windows has no file modes, and a file inherits the access list of its folder, which usually lets other local accounts read it. The vault folder this program creates, the database, the salt, the recovery files and the scratch directories get a protected access list with a single entry: the account running the program. This keeps other ordinary accounts and group-wide grants out. It does not protect against an administrator, who can take ownership of any file. `unv doctor` reads each file's list with `icacls` and warns when `Everyone`, `Users` or `Authenticated Users` can read it. A folder that existed before is left alone, since it may hold other files.

## The device key

The key that signs approvals identifies one machine. On Windows an older build kept it under the roaming profile, which a domain can copy to other computers. It now lives under the machine-local profile; the first time a build finds only the old file it copies it across (the old file is kept, so an interrupted move leaves the old key working) and you can delete the roaming copy yourself once nodes accept the new location.

## Running the node agent without systemd

The agent is `unv node run --config <node.toml>`. Run it under whatever your system uses to keep a program alive. On macOS, `unv_app::node_install::launchd_plist` produces the job below (save it as `/Library/LaunchDaemons/io.unenverse.node.plist`, then `sudo launchctl load` it):

```xml
<plist version="1.0"><dict>
  <key>Label</key><string>io.unenverse.node</string>
  <key>ProgramArguments</key><array>
    <string>/usr/local/bin/unv</string><string>node</string><string>run</string>
    <string>--config</string><string>/usr/local/etc/unv-node/node.toml</string>
  </array>
  <key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
</dict></plist>
```

On Windows `unv.exe` is not a Windows service, so `sc.exe create` would not start it. Use Task Scheduler: `schtasks /Create /SC ONSTART /RU SYSTEM /TN unv-node /TR "\"C:\Program Files\unv\unv.exe\" node run --config C:\ProgramData\unv-node\node.toml"`.

## Network filesystems

SQLite's write-ahead log needs shared memory and locks that NFS and SMB do not provide reliably, so a vault on a network share can be corrupted. `unv doctor` warns when the vault is on one (Linux and macOS read the mount table, Windows treats a UNC path as a share). Keep `vault.db` on a local disk and copy backups to the share instead.

## Replacing files

A file written by "write a temp file, rename it over the target" is retried briefly on Windows when another program (an antivirus scan, a backup agent) holds the target open, instead of failing the save.
