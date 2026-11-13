#!/usr/bin/env bash
set -euo pipefail

main() {
  local root canary pair capability evidence
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  canary="$root/scripts/canary-tungsten-microvm.sh"

bash -n "$canary"

# Every g5dev supported-surface capability must be exercised by the canary's
# chat prompt. The prompt expresses each as the actual verification command
# (reorg 389801a4 replaced the earlier quoted capability tokens), so assert the
# command evidence that uniquely proves each capability is still covered.
for pair in \
  "docker=docker version" \
  "postgres=psql --version" \
  "helm=helm version --short" \
  "rustfmt=cargo fmt --version" \
  "clippy=cargo clippy --version" \
  "cargo_cache=.g5-canary" \
  "frontend_dependencies=g5-dev-frontend-bootstrap" \
  "playwright=npx playwright --version" \
  "g5d_no_default_features=cargo check -p g5d --no-default-features"; do
  capability="${pair%%=*}"
  evidence="${pair#*=}"
  grep -Fq "$evidence" "$canary" || {
    echo "missing evidence assertion for $capability (expected: $evidence)" >&2
    exit 1
  }
done

grep -Fq 'g5-dev-frontend-bootstrap' "$canary"
grep -Fq 'npm run lint' "$canary"
grep -Fq 'npm run build' "$canary"
grep -Fq 'npx vitest run' "$canary"
grep -Fq 'cargo check -p g5d --no-default-features' "$canary"
grep -Fq 'Do not inspect /dev/kvm, run nested VMs, or compile g5d default features' "$canary"

echo "g5dev supported-surface canary contract: PASS"
}

main "$@"
