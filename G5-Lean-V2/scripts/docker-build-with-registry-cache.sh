#!/usr/bin/env bash
# Build a Docker image with an optional BuildKit registry cache.
#
# The cache is an acceleration artifact only. The image is still built from the
# checked-out source, and cache export failures must not hide build failures.
set -euo pipefail

if [[ "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage:
  G5_DOCKER_CACHE_REF=<registry/ref:tag> docker-build-with-registry-cache.sh [docker build args...]

Environment:
  G5_DOCKER_CACHE_REF       Optional BuildKit registry cache ref.
  G5_DOCKER_CACHE_DISABLE   true/false, force plain docker build.
  G5_DOCKER_BUILDX_BUILDER  Optional Buildx builder name; default is unique per invocation.
USAGE
  exit 0
fi

bool_true() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

if [[ $# -eq 0 ]]; then
  echo "ERROR: docker build arguments are required." >&2
  exit 2
fi

cache_ref="${G5_DOCKER_CACHE_REF:-}"
builder="${G5_DOCKER_BUILDX_BUILDER:-}"
cleanup_builder=false

if [[ -z "$builder" ]]; then
  random_id="local"
  if [[ -r /proc/sys/kernel/random/uuid ]]; then
    random_id="$(cat /proc/sys/kernel/random/uuid)"
  fi
  builder_suffix="${random_id}-$(date +%s%N)-${RANDOM:-0}-$$"
  builder_suffix="${builder_suffix//[^A-Za-z0-9_.-]/-}"
  builder="g5-ci-cache-${builder_suffix}"
  cleanup_builder=true
fi

if [[ -z "$cache_ref" ]] || bool_true "${G5_DOCKER_CACHE_DISABLE:-false}"; then
  echo "Docker registry cache disabled; using plain docker build."
  exec docker build "$@"
fi

if ! docker buildx version >/dev/null 2>&1; then
  echo "WARN: docker buildx is unavailable; using plain docker build." >&2
  exec docker build "$@"
fi

if ! docker buildx inspect "$builder" >/dev/null 2>&1; then
  docker buildx create --name "$builder" --driver docker-container >/dev/null
fi

if bool_true "$cleanup_builder"; then
  trap 'docker buildx rm "$builder" >/dev/null 2>&1 || true' EXIT
fi

cache_from_args=()
if docker buildx imagetools inspect "$cache_ref" >/dev/null 2>&1; then
  cache_from_args=(--cache-from "type=registry,ref=${cache_ref}")
else
  echo "WARN: Docker BuildKit registry cache not found; starting cold: $cache_ref" >&2
fi

echo "Docker BuildKit registry cache: $cache_ref"
# cache_from_args is empty on a cold cache; bash 3.2 (macOS system bash)
# rejects a bare "${cache_from_args[@]}" on an empty array under `set -u`.
docker buildx build \
  --builder "$builder" \
  --progress=plain \
  --load \
  ${cache_from_args[@]+"${cache_from_args[@]}"} \
  --cache-to "type=registry,ref=${cache_ref},mode=max,ignore-error=true" \
  "$@"
