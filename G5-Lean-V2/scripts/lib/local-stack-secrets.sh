#!/usr/bin/env bash

# Source after explicit .env configuration. Capture the secret-bearing output;
# never let it enter logs, arguments, or a Docker build context.
g5_load_local_stack_secrets() {
  local exports script_dir trace_enabled=false
  [[ "$-" == *x* ]] && trace_enabled=true
  set +x
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  if ! exports=$(python3 "$script_dir/stack_secrets.py" local); then
    [[ "$trace_enabled" == true ]] && set -x
    return 1
  fi
  eval "$exports"
  unset exports
  [[ "$trace_enabled" == true ]] && set -x
  return 0
}
