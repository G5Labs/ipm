#!/bin/sh
# G5 edge entrypoint. Production serves plain :80 behind a TLS-terminating
# ingress. The CI stacks set G5_EDGE_TLS_APEX and G5_EDGE_PREVIEW_SUFFIX to
# activate the HTTPS site in infra/docker/caddy-ci-tls.caddyfile, which the
# main Caddyfile imports from /etc/caddy/sites.d/. Previews are served only
# from per-preview HTTPS hosts, so a browser lane needs this site.
set -eu
# The self-hosted TLS overlay matches `*.<G5_PREVIEW_HOST_SUFFIX>`; an empty
# value would leave a malformed host pattern, so give it an unmatchable default.
export G5_PREVIEW_HOST_SUFFIX="${G5_PREVIEW_HOST_SUFFIX:-preview.invalid}"
if [ -n "${G5_EDGE_TLS_APEX:-}" ] || [ -n "${G5_EDGE_PREVIEW_SUFFIX:-}" ]; then
  if [ -z "${G5_EDGE_TLS_APEX:-}" ] || [ -z "${G5_EDGE_PREVIEW_SUFFIX:-}" ]; then
    echo "edge: set both G5_EDGE_TLS_APEX and G5_EDGE_PREVIEW_SUFFIX or neither" >&2
    exit 1
  fi
  cp /etc/caddy/optional/caddy-ci-tls.caddyfile /etc/caddy/sites.d/caddy-ci-tls.caddyfile
  echo "edge: HTTPS site enabled for ${G5_EDGE_TLS_APEX} and *.${G5_EDGE_PREVIEW_SUFFIX}"
fi
exec caddy run --config /etc/caddy/Caddyfile --adapter caddyfile
