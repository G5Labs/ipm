#!/bin/bash
# Usage: [CONTAINER=name] scripts/iris_sync.sh — copy validation/iris into the container at /tmp/uplift (owned by irisowner).
set -euo pipefail
EXP=$(cd "$(dirname "$0")/.." && pwd)
C=${CONTAINER:-ipm-uplift-iris}
docker exec -u root "$C" rm -rf /tmp/uplift
docker cp "$EXP/validation/iris" "$C":/tmp/uplift
docker exec -u root "$C" chown -R irisowner /tmp/uplift
