#!/usr/bin/env bash
# Local dev only; keep stack selection consistent with dev-up.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/lib/stack-identity.sh
g5_pin_stack_identity
exec python3 scripts/seed-lean-workspace.py "$@"
