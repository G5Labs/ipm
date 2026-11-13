#!/usr/bin/env bash
# Forward Docker container ports into the devcontainer's localhost
# so VS Code can auto-forward them to the host browser.
# Only useful when running inside a devcontainer with Docker-outside-of-Docker.
set -euo pipefail

# The stack identity every dev recipe follows (ontology
# g5.runtime.boot.backend.process-local): a preview stack beside the
# developer's own forwards its own ports to its own containers, never the
# developer's.
STACK="${G5_COMPOSE_PROJECT:-g5-dev}"
for mapping in ${G5_PORT_EDGE:-3000}:${STACK}-edge:80 ${G5_PORT_API:-7500}:${STACK}-api:7500; do
  LOCAL_PORT="${mapping%%:*}"
  REST="${mapping#*:}"
  CONTAINER="${REST%%:*}"
  REMOTE_PORT="${REST##*:}"

  pkill -f "socat.*TCP-LISTEN:${LOCAL_PORT}" 2>/dev/null || true

  C_IP=$(docker inspect "$CONTAINER" \
    --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")

  if [[ -n "$C_IP" ]]; then
    socat "TCP-LISTEN:${LOCAL_PORT},fork,reuseaddr" "TCP:${C_IP}:${REMOTE_PORT}" &
    echo "  Forwarding localhost:${LOCAL_PORT} → ${C_IP}:${REMOTE_PORT}"
  fi
done

sleep 0.5
