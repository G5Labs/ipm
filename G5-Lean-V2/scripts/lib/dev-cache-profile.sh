#!/usr/bin/env bash
# Select the host-projected build cache before either runtime staging or the
# application build. Explicit caller settings take precedence over the profile.
g5_load_dev_cache_profile() {
    local profile record key value
    profile="${G5_DEV_CACHE_PROFILE:-/opt/g5/cache-profile.env}"
    [[ -f "$profile" ]] || return 0
    # Split each whole record at its first `=` so values keep every `=`; the
    # final record may lack a newline; only exportable names apply.
    while IFS= read -r record || [[ -n "$record" ]]; do
        [[ "$record" == *=* ]] || continue
        key="${record%%=*}"
        value="${record#*=}"
        [[ "$key" =~ ^[A-Z_][A-Z0-9_]*$ ]] || continue
        if [[ -z "${!key:-}" ]]; then export "$key=$value"; fi
    done < "$profile"
    # Serialize service builds inside a workspace VM and use its read-only
    # mirror, including a dependency tier not yet published at its exact key.
    export COMPOSE_PARALLEL_LIMIT="${COMPOSE_PARALLEL_LIMIT:-1}"
    export G5_BASE_IMAGE_MIRROR_PUSH="${G5_BASE_IMAGE_MIRROR_PUSH:-0}"
    export G5_BASE_IMAGE_FALLBACK_NEWEST="${G5_BASE_IMAGE_FALLBACK_NEWEST:-1}"
}
