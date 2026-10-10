#!/usr/bin/env bash
# Exports every demo project and feeds each file to the tool that will really read it:
# `docker compose config`, `nginx -t` and WireGuard's own parser. A config that only
# round-trips through our parser proves little; a config the real tool refuses is a bug.
#
#   UNV_SERVER_URL=http://localhost:18743 UNV_PASSWORD=demo-passphrase-123 \
#     tools/demo/validate-exports.sh path/to/unv
#
# Needs docker. Run against the throwaway demo server (tools/demo/run.sh), never a real one.
set -euo pipefail
UNV="${1:-unv}"
W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT
chmod 755 "$W"

for p in home-vpn site-vpn media-stack edge-proxy; do
  "$UNV" project export "$p" --out "$W/$p.out" >/dev/null
done
"$UNV" project export media-stack --format env --out "$W/media.env" >/dev/null

echo "== compose"
docker compose -f "$W/media-stack.out" --env-file "$W/media.env" config -q 2>&1 | tee "$W/compose.log"
if grep -q . "$W/compose.log"; then echo "compose complained (above)"; exit 1; fi
echo "ok"

echo "== nginx"
printf 'events {}\nhttp {\n    include /fx/edge-proxy.out;\n}\n' > "$W/main.conf"
docker run --rm -v "$W:/fx:ro" nginx:1.27-alpine sh -c \
  'mkdir -p /etc/ssl/example /var/log/nginx /var/www/app && apk add -q openssl >/dev/null \
   && openssl req -x509 -newkey rsa:2048 -nodes -keyout /etc/ssl/example/privkey.pem -out /etc/ssl/example/fullchain.pem -subj /CN=example.com -days 1 2>/dev/null \
   && nginx -t -c /fx/main.conf' 2>&1 | grep -v 'http2' | tee "$W/nginx.log"
grep -q "test is successful" "$W/nginx.log" || { echo "nginx refused the export"; exit 1; }

echo "== wireguard"
docker run --rm -v "$W:/fx:ro" debian:bookworm-slim bash -c '
  apt-get update -qq >/dev/null && apt-get install -y -qq --no-install-recommends wireguard-tools >/dev/null 2>&1
  for f in home-vpn site-vpn; do
    cp /fx/$f.out /tmp/$f.conf && chmod 600 /tmp/$f.conf
    out=$(cd /tmp && wg-quick strip ./$f.conf | wg setconf wgparse /dev/stdin 2>&1 || true)
    case "$out" in *"Configuration parsing error"*) echo "$f: $out"; exit 1;; esac
    echo "$f ok"
  done'
echo "all exports accepted"
