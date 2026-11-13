#!/usr/bin/env bash
# Local chart validation only. Prefer installed Helm; otherwise use Docker.
set -euo pipefail
if command -v helm >/dev/null 2>&1; then
  exec helm "$@"
fi
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Match the deploy pipeline's Helm version. Mount only the chart sources.
exec docker run --rm -v "$REPO_ROOT/infra/helm:/workspace/infra/helm:ro" \
  -w /workspace alpine/helm:3.21.4 "$@"
