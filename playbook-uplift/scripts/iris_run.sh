#!/bin/bash
# Usage: [CONTAINER=name] scripts/iris_run.sh <script-file> <log-file> [namespace]
# Pipes an ObjectScript terminal script into the uplift IRIS container and records the session.
set -euo pipefail
NS=${3:-USER}
C=${CONTAINER:-ipm-uplift-iris}
{ echo "# container: $C  image: $(docker inspect --format '{{.Config.Image}}' "$C")  at: $(date -u +%FT%TZ)"; echo "# script: $1"; docker exec -i "$C" iris session IRIS -U "$NS" < "$1"; } > "$2" 2>&1
