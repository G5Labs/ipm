#!/usr/bin/env bash
# g5-dev-rust-launcher — self-configuring launcher for the baked Rust toolchain
# (ontology concepts/deployment g5.scope.deployment.images.g5dev-toolchain).
#
# Installed in the g5d-g5dev image as every Rust entry point in /usr/local/bin
# (cargo, rustc, rustup, cargo-fmt, rustfmt, cargo-clippy, clippy-driver), each
# a symlink to this script. The guest runtime starts agent turns, terminals and
# previews with a cleared environment (command path, home and Git identity
# only), so the image's ENV never reaches them and a bare rustup proxy fails
# with "rustup could not choose a version of cargo to run". Each launcher fills
# the image's toolchain identity for the keys the caller left unset, then execs
# the rustup proxy of the same name. A caller's explicit value always wins, and
# RUSTC_WRAPPER="" keeps sccache off, matching cargo's own convention. The three
# paths below must match the image ENV in Dockerfile.g5d-g5dev.
set -euo pipefail

G5_RUSTUP_HOME=/usr/local/rustup
G5_CARGO_HOME=/usr/local/cargo
G5_SCCACHE_DIR=/opt/g5-build-cache/sccache

tool="$(basename "$0")"
proxy="${G5_CARGO_HOME}/bin/${tool}"

# Does the cache door answer its registry ping? curl when the image has it,
# else a bare TCP connect (bash's /dev/tcp) to the door's host and port.
door_answers() {
  local base="${1%/}" hostport
  if command -v curl >/dev/null 2>&1; then
    curl -fsS --max-time 2 -o /dev/null "${base}/v2/" 2>/dev/null
    return $?
  fi
  hostport="${base#http://}"
  hostport="${hostport#https://}"
  hostport="${hostport%%/*}"
  [[ "${hostport}" == *:* ]] || hostport="${hostport}:80"
  (exec 3<>"/dev/tcp/${hostport%:*}/${hostport##*:}") 2>/dev/null
}
if [[ ! -x "${proxy}" ]]; then
  echo "g5 rust launcher: ${proxy} is not an executable rustup proxy" >&2
  exit 127
fi

# Unset-only defaults: an explicitly empty value is the caller's choice too
# (cargo and rustup fall back to $HOME on an empty home variable).
export RUSTUP_HOME="${RUSTUP_HOME-${G5_RUSTUP_HOME}}"
export CARGO_HOME="${CARGO_HOME-${G5_CARGO_HOME}}"

# sccache rejects incremental compilation, so the wrapper and
# CARGO_INCREMENTAL=0 travel together. An unwritable object cache would fail
# every compile at sccache server start, so degrade to plain rustc instead.
# A caller's own SCCACHE_DIR is created the way sccache itself would.
if [[ -z "${RUSTC_WRAPPER+set}" ]]; then
  sccache_dir="${SCCACHE_DIR-${G5_SCCACHE_DIR}}"
  if [[ -n "${sccache_dir}" ]]; then
    mkdir -p "${sccache_dir}" 2>/dev/null || true
  fi
  if [[ -n "${sccache_dir}" && -d "${sccache_dir}" && -w "${sccache_dir}" ]] && command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER=sccache
  fi
fi
if [[ "${RUSTC_WRAPPER-}" == sccache ]]; then
  export SCCACHE_DIR="${SCCACHE_DIR-${G5_SCCACHE_DIR}}"
  export CARGO_INCREMENTAL="${CARGO_INCREMENTAL-0}"
  # Cache door (ontology g5.runtime.cache.door): a worker host whose bring-up
  # published a profile serves sccache objects read-through at
  # <door>/sccache (host store, this image's primed objects, the
  # organization's fleet prefix), in the layout sccache's WebDAV backend
  # reads. A caller's own endpoint wins. The profile is read as data, never
  # sourced: a value with spaces (the tier list) would run as a command. The
  # runtime disk's empty placeholder (ontology g5.runtime.cache.profile, a
  # host without a door) has no door line and reads as no profile.
  # sccache 0.10 aborts server startup when its backend cannot be read (the
  # `.sccache_check` probe) and SCCACHE_IGNORE_SERVER_IO_ERROR only covers
  # later compile responses, so a door that does not answer keeps sccache on
  # the image's local cache instead of failing every compile.
  # One probe per build: the outcome is exported, so children (rustc runs
  # once per crate under the same launcher) inherit it instead of probing
  # again, whichever way it went.
  if [[ -z "${SCCACHE_WEBDAV_ENDPOINT+set}" && -z "${G5_DEV_CACHE_DOOR_PROBED-}" ]]; then
    export G5_DEV_CACHE_DOOR_PROBED=1
    cache_profile="${G5_DEV_CACHE_PROFILE-/opt/g5/cache-profile.env}"
    door=""
    if [[ -n "${cache_profile}" && -r "${cache_profile}" ]]; then
      door="$(grep -E '^G5_CACHE_DOOR=' "${cache_profile}" 2>/dev/null | head -n 1 | cut -d= -f2- || true)"
    fi
    if [[ -n "${door}" ]]; then
      if door_answers "${door}"; then
        export SCCACHE_WEBDAV_ENDPOINT="${door%/}/sccache"
        # The ping and the server start are two moments; start sccache's
        # server now (--show-stats would not: without a server it prints
        # synthetic statistics) and fall back to the local cache if the door
        # went away in between, rather than letting the first compile fail
        # on a server that could not start. "Address in use" means a server
        # is already up with whatever backend it started on, which is fine.
        if ! start_out="$(sccache --start-server 2>&1)"; then
          case "${start_out}" in
            *"Address in use"*) ;;
            *)
              unset SCCACHE_WEBDAV_ENDPOINT
              if [[ "${tool}" == cargo ]]; then
                echo "g5 rust launcher: sccache could not start on cache door ${door}; sccache stays on the local cache" >&2
              fi
              ;;
          esac
        fi
      elif [[ "${tool}" == cargo ]]; then
        echo "g5 rust launcher: cache door ${door} is not answering; sccache stays on the local cache" >&2
      fi
    fi
  fi
  if [[ -n "${SCCACHE_WEBDAV_ENDPOINT-}" ]]; then
    export SCCACHE_IGNORE_SERVER_IO_ERROR="${SCCACHE_IGNORE_SERVER_IO_ERROR-1}"
  fi
fi

exec "${proxy}" "$@"
