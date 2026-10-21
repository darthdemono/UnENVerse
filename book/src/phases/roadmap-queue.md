# The queue: roadmap phases still to do

Every roadmap phase that is not done has a project phase number, allocated in queue order from 57. A phase moves up when a defect, a security finding or a dependency change takes priority, and the number is a queue position, not an id. The R id is kept in the second column because the roadmap and its research use it.

| Phase | Roadmap id | Name                                   | Depends on                              | Scope                                                                                       |
| ----- | ---------- | -------------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------- |
| 57    | R10        | git sources + cutover                  | R08                                     | SOPS, pass, gopass via external tools; authority, one-way sync, drift, cutover              |
| 58    | R11        | service sources                        | R10                                     | OpenBao, Infisical, AWS SM/SSM (feature), Bitwarden SM (bws)                                |
| 59    | R12        | desktop hub + deployment               | R07                                     | embedded hub in desktop, listen-mode deploy, health checks, target leases, approval v2      |
| 60    | R13        | rotation engine                        | R06, R12                                | rotation state machine; PostgreSQL, Cloudflare, OAuth connectors; reconcile                 |
| 61    | R14        | plugin foundation                      | R11, R13                                | trait conformance suite, external-process plugin protocol v1, manifest, provenance in plans |
| 62    | R15        | UI QoL                                 | R05 (items gated on later phases noted) | keyboard path, progressive form, virtualised grid, Operations centre, health grouping, CSP  |
| 63    | R16        | platform parity                        | R05                                     | macOS build/test, Windows DACLs and local app data, node agent on Windows/macOS             |
| 64    | R17        | upstream experiments                   | R08, R13                                | gitleaks findings import; ACME via lego                                                     |
| 65    | R18        | desktop CLI bridge                     | R04                                     | local-only socket/pipe so scripts use the unlocked desktop                                  |
| 66    | R19        | vault identity + single writer         | R05                                     | vault/instance ids, standby restores, node epoch check, sync-folder warnings                |
| 67    | R20        | delete / forget / rekey                | R06, R07                                | truthful delete/revoke, forget purge, master-password rekey                                 |
| 68    | R21        | compatibility contract + catalogue URL | R01                                     | published CLI/format contract, describe snapshot, catalogue URL fix                         |
| 69    | R23        | SL2 + PingDD site fixes                | W1 + authorisation                      | SL2/PingDD site fixes                                                                       |
| 70    | R24        | site truth generation                  | W1, R21                                 | generated release/CLI/format facts                                                          |
| 71    | R25        | community files + support bundle       | R00                                     | CONTRIBUTING, templates, demo vault, support bundle                                         |
| 72    | R26.1      | package-manager channels               | R26                                     | winget and Scoop manifests                                                                  |
| 73    | R27        | Compose file secrets                   | W1 / R12                                | Compose secret files: recipe now, node targets after R12                                    |
| 74    | R28        | scripts + sibling tools                | R21, R18 + authorisation                | script recipes; EltePortal/Dossify fixes                                                    |
| 75    | R29        | Home Assistant recipe                  | R08, R12                                | HA secrets.yaml via node                                                                    |
| 76    | R30        | ref-only project files                 | R21                                     | unv env --refs, migrate plaintext .env to refs, scan for plaintext secret files             |
| 77    | R31        | per-command secret release             | R30                                     | secrets released only to bound argv, optional binary hash pin                               |
| 78    | R32        | SSH agent                              | R16, R31                                | unv ssh-agent, keys served over the agent protocol, never on disk                           |
| 79    | R33        | canary entries                         | R07, R12                                | decoy .env/keys, hit alerts through audit and hub                                           |
| 80    | R34        | sandboxed exec                         | R16, R31                                | opt-in exec --sandbox with egress and filesystem limits                                     |

Phases 76 to 80 (R30 to R34) are the agent-safety lane, planned after 1.0.0. They exist because a model that has been tampered with can steal every plaintext secret in a working directory, so the lane makes those secrets worthless and the theft visible: ref-only project files, per-command secret release, an SSH agent, canary entries and a sandboxed exec. The reasoning is in the [roadmap](../roadmap.md).
