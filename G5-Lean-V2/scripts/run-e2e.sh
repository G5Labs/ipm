#!/usr/bin/env bash
# Compatibility entry point for the repository's authoritative Docker
# validation harness. The harness provisions a bound `g5d host`, waits for its
# worker_hosts row, and starts checked workspace VMs; it has no per-task worker
# fallback.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ "${G5_E2E_LIVE_LLM:-false}" == "true" ]]; then
  exec just ci-docker 0 true
fi

exec just ci-docker
