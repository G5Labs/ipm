#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
helper="$repo/scripts/lib/dev-cache-profile.sh"
cat > "$tmp/cache profile.env" <<'PROFILE'
# Host-projected settings are data, including spaces and shell syntax.
G5_BASE_IMAGE_REGISTRY=cache.example.test/tiers
G5_BASE_IMAGE_MIRROR_TIERS=base base-rust base-rust-warm base-node
G5_CACHE_LITERAL=$(exit 99)
G5_CACHE_EQUALS=abc=
invalid-key=ignored
1G5_NUMERIC_LEADING=ignored
PROFILE
# The last record has no trailing newline.
printf 'G5_CACHE_FINAL=unterminated' >> "$tmp/cache profile.env"

# Match the preview's clean environment: runtime staging needs exported cache
# settings before dev-up has had a chance to load them.
env -i PATH="$PATH" bash -euo pipefail -c '
    source "$1"
    export G5_DEV_CACHE_PROFILE="$2"
    g5_load_dev_cache_profile
    bash -euo pipefail -c '\''
        [[ "$G5_BASE_IMAGE_REGISTRY" == cache.example.test/tiers ]]
        [[ "$G5_BASE_IMAGE_MIRROR_TIERS" == "base base-rust base-rust-warm base-node" ]]
        [[ "$G5_CACHE_LITERAL" == "\$(exit 99)" ]]
        [[ "$G5_CACHE_FINAL" == unterminated ]]
        [[ "$G5_CACHE_EQUALS" == "abc=" ]]
        [[ "$COMPOSE_PARALLEL_LIMIT" == 1 ]]
        [[ "$G5_BASE_IMAGE_MIRROR_PUSH" == 0 ]]
        [[ "$G5_BASE_IMAGE_FALLBACK_NEWEST" == 1 ]]
    '\''
' -- "$helper" "$tmp/cache profile.env"

env -i PATH="$PATH" bash -euo pipefail -c '
    source "$1"
    export G5_DEV_CACHE_PROFILE="$2" G5_BASE_IMAGE_REGISTRY=caller.example.test/tiers
    export COMPOSE_PARALLEL_LIMIT=3 G5_BASE_IMAGE_MIRROR_PUSH=1 G5_BASE_IMAGE_FALLBACK_NEWEST=0
    g5_load_dev_cache_profile
    [[ "$G5_BASE_IMAGE_REGISTRY" == caller.example.test/tiers ]]
    [[ "$COMPOSE_PARALLEL_LIMIT" == 3 ]]
    [[ "$G5_BASE_IMAGE_MIRROR_PUSH" == 1 ]]
    [[ "$G5_BASE_IMAGE_FALLBACK_NEWEST" == 0 ]]
' -- "$helper" "$tmp/cache profile.env"

env -i PATH="$PATH" bash -euo pipefail -c '
    source "$1"
    export G5_DEV_CACHE_PROFILE="$2"
    g5_load_dev_cache_profile
    [[ -z "${G5_BASE_IMAGE_REGISTRY+x}" ]]
    [[ -z "${COMPOSE_PARALLEL_LIMIT+x}" ]]
    [[ -z "${G5_BASE_IMAGE_MIRROR_PUSH+x}" ]]
    [[ -z "${G5_BASE_IMAGE_FALLBACK_NEWEST+x}" ]]
' -- "$helper" "$tmp/missing.env"

echo "dev-cache-profile: OK (exported settings, literal values, unterminated and invalid records, caller overrides, no profile)"
