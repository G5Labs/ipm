#!/usr/bin/env bash
# Poll until every given marker file exists, or fail after the timeout.
#
# Usage: ci-wait-markers.sh <timeout-seconds> <marker-path> [<marker-path> ...]
#
# Cloud Build steps that overlap other steps and only need their results at
# the END (for example the provider-agent lane's receipt tail reading the
# broad shard reports) wait here on the other steps' EXIT-trap-guaranteed
# marker files instead of in `waitFor`, which would serialize the whole step.
# Exit 0 once all markers exist, printing how long it waited; exit 1 with the
# missing paths on timeout.
# Inline step scripts are capped at 10,000 characters by Cloud Build, so the
# loop lives here rather than being repeated in each step.
set -euo pipefail

timeout="${1:?timeout seconds}"
shift
[[ $# -gt 0 ]] || { echo "ci-wait-markers.sh: at least one marker path is required" >&2; exit 2; }

elapsed=0
while :; do
  missing=()
  for marker in "$@"; do
    [[ -f "$marker" ]] || missing+=("$marker")
  done
  if [[ ${#missing[@]} -eq 0 ]]; then
    # Timing evidence (ontology g5.quality.delivery.ci.timing-evidence): this
    # is waiting on other lanes, not the caller's own execution.
    echo "ci-wait-markers: waited ${elapsed}s for: $*"
    exit 0
  fi
  if (( elapsed >= timeout )); then
    echo "timed out after ${timeout}s waiting for: ${missing[*]}" >&2
    exit 1
  fi
  sleep 10
  elapsed=$((elapsed + 10))
done
