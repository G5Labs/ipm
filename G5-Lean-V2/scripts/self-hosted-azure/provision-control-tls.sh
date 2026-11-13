#!/usr/bin/env bash
set -euo pipefail

FQDN="${G5_CANARY_FQDN:?G5_CANARY_FQDN is required}"
TLS_DIR="${G5_TLS_OUTPUT_DIR:-/var/lib/g5/tls}"
STATE_DIR="${G5_TLS_BOOTSTRAP_STATE_DIR:-/var/lib/g5/caddy-bootstrap}"
TIMEOUT="${G5_TLS_BOOTSTRAP_TIMEOUT:-600}"
CONTAINER="g5-canary-caddy-bootstrap"

die() {
  echo "azure TLS bootstrap failed: $*" >&2
  exit 1
}

case "$FQDN" in
  *[!A-Za-z0-9.-]*|'') die "invalid G5_CANARY_FQDN" ;;
esac

mkdir -p "$TLS_DIR" "$STATE_DIR/data" "$STATE_DIR/config"
chmod 0700 "$TLS_DIR" "$STATE_DIR"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

cat >"$STATE_DIR/Caddyfile" <<EOF
$FQDN {
  respond /healthz "OK" 200
}
EOF

docker run -d --name "$CONTAINER" \
  -p 80:80 -p 443:443 \
  -v "$STATE_DIR/Caddyfile:/etc/caddy/Caddyfile:ro" \
  -v "$STATE_DIR/data:/data" \
  -v "$STATE_DIR/config:/config" \
  caddy:2.10.0-alpine >/dev/null

deadline=$((SECONDS + TIMEOUT))
cert=""
key=""
while [ "$SECONDS" -lt "$deadline" ]; do
  container_status="$(docker inspect --format '{{.State.Status}}' "$CONTAINER" 2>/dev/null || true)"
  if [ "$container_status" != "running" ]; then
    docker logs "$CONTAINER" >&2 || true
    die "bootstrap Caddy exited before certificate issuance (status: ${container_status:-missing})"
  fi
  cert="$(find "$STATE_DIR/data/caddy/certificates" -type f -path "*/$FQDN/$FQDN.crt" -print -quit 2>/dev/null || true)"
  key="$(find "$STATE_DIR/data/caddy/certificates" -type f -path "*/$FQDN/$FQDN.key" -print -quit 2>/dev/null || true)"
  if [ -n "$cert" ] && [ -s "$cert" ] && [ -n "$key" ] && [ -s "$key" ]; then
    break
  fi
  sleep 5
done

[ -n "$cert" ] && [ -s "$cert" ] || { docker logs "$CONTAINER" >&2 || true; die "certificate was not issued before timeout"; }
[ -n "$key" ] && [ -s "$key" ] || { docker logs "$CONTAINER" >&2 || true; die "private key was not issued before timeout"; }

install -m 0600 "$cert" "$TLS_DIR/fullchain.pem"
install -m 0600 "$key" "$TLS_DIR/privkey.pem"
openssl x509 -in "$TLS_DIR/fullchain.pem" -noout -checkend 3600 >/dev/null || die "issued certificate is invalid or expires too soon"
openssl x509 -in "$TLS_DIR/fullchain.pem" -noout -subject -issuer -dates
echo "mounted TLS material ready for $FQDN"
