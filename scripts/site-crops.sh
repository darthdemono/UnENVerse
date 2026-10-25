#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Strict-1.0.0
# Copyright Jubair Hasan.
# Cuts the site's screenshots out of the raw 1920x1200 demo shots and rebuilds
# website/assets/screens (WebP files and manifest.json).
#
#   OUT=shots/demo tools/demo/run.sh      # retake the raw shots (capped at 2 CPUs by default)
#   scripts/site-crops.sh shots/demo      # crop, convert, write the manifest
#
# Each line below is NAME  SOURCE-PNG  WIDTHxHEIGHT+X+Y (a plain ImageMagick geometry in
# source pixels); an empty geometry keeps the whole image.
set -euo pipefail
src="$1"
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
pairs=()
while read -r name file geom; do
  [ -z "$name" ] && continue
  if [ -n "$geom" ]; then
    magick "$src/$file.png" -crop "$geom" +repage "$tmp/$name.png"
  else
    cp "$src/$file.png" "$tmp/$name.png"
  fi
  pairs+=("$name=$name")
done <<'CROPS'
main           unenverse
card-crop      card-expanded       970x540+290+140
history-crop   config-history      1230x730+300+80
audit-crop     audit-log           1230x880+300+80
compose-crop   project-compose     1650x1100+270+60
wg-crop        project-wireguard   1650x1100+270+60
teams          user-permissions    1000x940+305+75
selfhost       login-filled        700x440+610+380
edit-crop      edit-secret         600x1090+660+55
bundle-crop    bundle              1620x445+290+530
CROPS
"$here/site-shots.sh" "$tmp" "${pairs[@]}"
