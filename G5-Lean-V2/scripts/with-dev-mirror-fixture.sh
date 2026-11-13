#!/usr/bin/env bash
# CI-only filesystem destination for the real dev-mirror CLI regression.
# Shared-dev runs use their externally managed g5-dev-api instead.
set -euo pipefail

FIXTURE_ID=""
OWNER=""

cleanup_owned_fixture() {
  [[ "$FIXTURE_ID" =~ ^[0-9a-f]{64}$ ]] || {
    echo "Refusing cleanup: invalid fixture container ID" >&2
    return 1
  }
  local actual_owner
  actual_owner="$(docker inspect --format '{{index .Config.Labels "g5.e2e.dev-mirror-owner"}}' "$FIXTURE_ID")" || return 1
  if [[ "$actual_owner" != "$OWNER" ]]; then
    echo "Refusing cleanup: dev-mirror fixture owner mismatch" >&2
    return 1
  fi
  docker rm -f "$FIXTURE_ID" >/dev/null
}

finish_fixture() {
  local status=$?
  trap - EXIT
  if [[ -n "$FIXTURE_ID" ]]; then
    if ! cleanup_owned_fixture; then
      echo "Dev-mirror fixture cleanup failed" >&2
      if (( status == 0 )); then status=1; fi
    fi
  fi
  exit "$status"
}

run_fixture() {
  if [[ "${1:-}" == "--cleanup" && $# == 2 && -n "$2" ]]; then
    OWNER="$2"
    # Teardown backstop: never select another build's or a developer's API.
    FIXTURE_ID="$(docker ps -aq --no-trunc --filter 'name=^/g5-dev-api$' \
      --filter "label=g5.e2e.dev-mirror-owner=$OWNER")"
    if [[ -n "$FIXTURE_ID" ]]; then cleanup_owned_fixture; fi
    return 0
  fi
  if [[ $# -lt 4 || "${3:-}" != "--" || -z "$1" || -z "$2" ]]; then
    echo "Usage: $0 <candidate-api-image> <build-id> -- <command> [args...]" >&2
    echo "       $0 --cleanup <build-id>" >&2
    return 2
  fi
  local candidate_image="$1"
  OWNER="$2"
  shift 3
  trap finish_fixture EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  # docker create is atomic: an existing name fails without replacing it.
  # The candidate image supplies real Git and /data/repos. No API process,
  # credentials, host mounts, ports, or network are needed for cp/exec/chown.
  FIXTURE_ID="$(docker create --name=g5-dev-api \
    --label="g5.e2e.dev-mirror-owner=$OWNER" --network=none \
    --no-healthcheck --user=0:0 --entrypoint=/bin/sh \
    "$candidate_image" -c 'exec sleep infinity')"
  [[ "$FIXTURE_ID" =~ ^[0-9a-f]{64}$ ]] || return 1
  docker start "$FIXTURE_ID" >/dev/null
  docker exec "$FIXTURE_ID" /bin/sh -ec \
    'git --version; test -d /data/repos; test -w /data/repos'
  "$@"
}

run_fixture "$@"
