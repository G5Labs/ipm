#!/usr/bin/env bash
# The dev stack's identity (ontology g5.runtime.boot.backend.process-local):
# the Compose project, the volume prefix, the image tag and the published
# ports select the stack every lifecycle recipe acts on (start, restart,
# teardown, worker start). All of them resolve it the same way, from the same
# sources, before anything addresses the stack (review R22, R24, R26, R29):
#   1. values exported by the caller, such as the `full-stack` preview profile
#      running beside the developer's own stack;
#   2. otherwise the optional local overlays (`.env.github-app.local` over
#      `.env.platform-issue.local`, at their configurable paths) over `.env`,
#      in the order startup sources them;
#   3. otherwise the defaults every consumer applies itself.
# `g5_pin_stack_identity` resolves and exports that identity and remembers it;
# a recipe that sources dotenv files afterwards for its other knobs calls
# `g5_restore_stack_identity` so the sourcing cannot move the selection.
G5_STACK_IDENTITY_KEYS=(
  G5_COMPOSE_PROJECT
  G5_COMPOSE_VOLUMES
  G5_IMAGE_TAG
  G5_PORT_EDGE
  G5_PORT_API
  G5_PORT_POSTGRES
  G5_PORT_OAUTH_STUB
)

# Print `KEY=VALUE` lines for the identity the configuration files select,
# evaluated in a subshell so nothing else they set leaks into the caller.
g5_configured_stack_identity() {
  (
    set +u
    if [[ -f .env ]]; then set -a; source .env; set +a; fi
    file="${G5_DEV_PLATFORM_ISSUE_ENV_FILE:-.env.platform-issue.local}"
    if [[ -f "$file" ]]; then set -a; source "$file"; set +a; fi
    file="${G5_DEV_GITHUB_APP_ENV_FILE:-.env.github-app.local}"
    if [[ -f "$file" ]]; then set -a; source "$file"; set +a; fi
    for key in "${G5_STACK_IDENTITY_KEYS[@]}"; do
      if [[ -n "${!key+x}" ]]; then printf '%s=%s\n' "$key" "${!key}"; fi
    done
  ) 2>/dev/null
}

g5_pin_stack_identity() {
  __g5_pinned_stack_identity=()
  local key pair exported=()
  for key in "${G5_STACK_IDENTITY_KEYS[@]}"; do
    if [[ -n "${!key+x}" ]]; then
      __g5_pinned_stack_identity+=("$key=${!key}")
      exported+=("$key")
    fi
  done
  while IFS= read -r pair; do
    [[ -n "$pair" ]] || continue
    key="${pair%%=*}"
    local skip=0
    for seen in "${exported[@]+"${exported[@]}"}"; do
      [[ "$seen" == "$key" ]] && skip=1
    done
    (( skip )) && continue
    __g5_pinned_stack_identity+=("$pair")
  done < <(g5_configured_stack_identity)
  g5_restore_stack_identity
}

g5_restore_stack_identity() {
  local pair
  for pair in "${__g5_pinned_stack_identity[@]+"${__g5_pinned_stack_identity[@]}"}"; do
    export "$pair"
  done
}
