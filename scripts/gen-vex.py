#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Strict-1.0.0
# Copyright Jubair Hasan.
"""Write vex.openvex.json from the advisory decisions in deny.toml and osv-scanner.toml.

Those two files are the source of truth (each ignore carries its reason); this
turns them into an OpenVEX document so a consumer can read the same decisions
by machine. Run from the repository root after changing either file.
"""
import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PRODUCT = "pkg:github/darthdemono/UnENVerse"
TIMESTAMP = "2026-10-17T00:00:00Z"

decisions = {}
for d in tomllib.loads((ROOT / "deny.toml").read_text())["advisories"]["ignore"]:
    decisions[d["id"]] = d["reason"]
for d in tomllib.loads((ROOT / "osv-scanner.toml").read_text())["IgnoredVulns"]:
    decisions.setdefault(d["id"], d["reason"])


def justification(reason: str) -> str:
    # An RSA decryption oracle is the precondition of the rsa advisory; nothing here exposes one.
    if "oracle" in reason:
        return "vulnerable_code_cannot_be_controlled_by_adversary"
    return "vulnerable_code_not_in_execute_path"


doc = {
    "@context": "https://openvex.dev/ns/v0.2.0",
    "@id": "https://github.com/darthdemono/UnENVerse/blob/main/vex.openvex.json",
    "author": "Jubair Hasan",
    "timestamp": TIMESTAMP,
    "version": 1,
    "statements": [
        {
            "vulnerability": {"name": vuln},
            "products": [{"@id": PRODUCT}],
            "status": "not_affected",
            "justification": justification(reason),
            "impact_statement": reason,
        }
        for vuln, reason in sorted(decisions.items())
    ],
}
(ROOT / "vex.openvex.json").write_text(json.dumps(doc, indent=2) + "\n")
print(f"wrote vex.openvex.json with {len(doc['statements'])} statements", file=sys.stderr)
