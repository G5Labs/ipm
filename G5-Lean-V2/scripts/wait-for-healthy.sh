#!/usr/bin/env bash
# =============================================================================
# wait-for-healthy.sh — Poll Docker Compose service health status
# =============================================================================
# Usage: ./scripts/wait-for-healthy.sh [compose-file] [timeout-seconds]
# Default: infra/docker/docker-compose.test.yml, 120s timeout
#
# NOTE: Uses `docker inspect` on container_name values rather than
# `docker compose ps` to avoid project-name mismatch issues when running
# inside a devcontainer (the host path differs from the container path,
# causing compose to compute a different project name and return empty).
#
# Compatible with bash 3.2+ (macOS default) — no associative arrays.
# =============================================================================
set -euo pipefail

COMPOSE_FILE="${1:-infra/docker/docker-compose.test.yml}"
TIMEOUT="${2:-120}"

# Local validation supplies its isolated project explicitly. The conventional
# test entry points retain g5-test, including interpolated container names.
if [[ -n "${3:-}" || "$COMPOSE_FILE" == *docker-compose.test.yml ]]; then
  exec python3 "$(dirname "$0")/compose_health.py" "$COMPOSE_FILE" "$TIMEOUT" "${3:-${COMPOSE_PROJECT_NAME:-g5-test}}"
fi

# Parse container_name values from the compose file.
CONTAINERS=()
# The dev stack names its containers from the stack identity
# (`${G5_COMPOSE_PROJECT:-g5-dev}-api`); resolve that one interpolation the
# way Compose does, so a preview stack beside the default one is polled under
# its own names.
while IFS= read -r line; do
  if [[ "$line" =~ ^[[:space:]]+container_name:[[:space:]]*(.+)$ ]]; then
    cname="${BASH_REMATCH[1]}"
    cname="${cname//\$\{G5_COMPOSE_PROJECT:-g5-dev\}/${G5_COMPOSE_PROJECT:-g5-dev}}"
    CONTAINERS+=("$cname")
  fi
done < "$COMPOSE_FILE"

if (( ${#CONTAINERS[@]} == 0 )); then
  CONTAINERS=(g5-test-postgres g5-test-api)
fi

# Filter to only containers that actually exist (skip profile-only containers)
ACTIVE_CONTAINERS=()
for cname in "${CONTAINERS[@]}"; do
  if docker container inspect "$cname" &>/dev/null; then
    ACTIVE_CONTAINERS+=("$cname")
  fi
done

if (( ${#ACTIVE_CONTAINERS[@]} == 0 )); then
  echo "ERROR: No matching containers from ${COMPOSE_FILE} are currently present."
  exit 1
fi

echo "Waiting for services to be healthy (timeout: ${TIMEOUT}s)..."
echo "  Containers: ${ACTIVE_CONTAINERS[*]}"

for cname in "${ACTIVE_CONTAINERS[@]}"; do
  elapsed=0
  while true; do
    # Use docker inspect directly — works regardless of compose project name
    health=$(docker container inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}no-healthcheck{{end}}' "$cname" 2>/dev/null || echo "not-found")
    state=$(docker container inspect --format '{{.State.Status}}' "$cname" 2>/dev/null || echo "not-found")

    if [[ "$health" == "healthy" ]]; then
      echo "  ✓ $cname healthy"
      break
    fi

    # Services without a healthcheck (e.g. caddy): accept "running"
    if [[ "$health" == "no-healthcheck" && "$state" == "running" ]]; then
      echo "  ✓ $cname running (no healthcheck)"
      break
    fi

    # Services without a healthcheck that exited (e.g. worker): skip
    if [[ "$health" == "no-healthcheck" && "$state" == "exited" ]]; then
      echo "  - $cname exited (no healthcheck, skipping)"
      break
    fi

    if (( elapsed >= TIMEOUT )); then
      echo "ERROR: $cname not healthy after ${TIMEOUT}s (health: $health, state: $state)"
      echo "--- Last 50 lines of $cname logs ---"
      docker logs "$cname" --tail=50 2>/dev/null || true
      exit 1
    fi

    sleep 2
    elapsed=$((elapsed + 2))
  done
done

echo "All services healthy."
