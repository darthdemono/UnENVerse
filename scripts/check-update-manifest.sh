#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright the UnENVerse contributors.
# Fail unless the CLI update manifest names every required platform with a
# 64-hex checksum and an https URL, and carries a signature. Usage:
#   check-update-manifest.sh <unv-update.json> <version>
# The signature itself is verified by `unv update check` in the release job; this
# is the assembly guard that a feed is never published with a missing piece.
set -euo pipefail
f="$1"; version="$2"
for target in linux-x86_64 windows-x86_64; do
  payload=$(jq -r '.payload' "$f")
  echo "$payload" | jq -e --arg t "$target" --arg v "$version" '
    .version == $v and
    (.assets | map(select(.target == $t and (.sha256 | test("^[0-9a-f]{64}$"))
                         and (.url | startswith("https://")))) | length == 1)' >/dev/null \
    || { echo "::error::update manifest lacks a valid $target asset for $version"; exit 1; }
done
jq -e '.signature | length > 80' "$f" >/dev/null \
  || { echo "::error::update manifest is unsigned"; exit 1; }
