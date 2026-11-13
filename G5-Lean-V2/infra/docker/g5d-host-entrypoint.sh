#!/usr/bin/env sh
set -eu

. /usr/local/bin/g5d-provider-env

if [ "$#" -eq 0 ] || [ "${1:-}" = "host" ]; then
  if [ "${1:-}" = "host" ]; then
    shift
  fi
  g5d_setup_provider_env
  exec g5d host "$@"
fi

exec "$@"
