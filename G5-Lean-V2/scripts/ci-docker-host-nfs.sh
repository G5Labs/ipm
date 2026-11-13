#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 <init-host|prepare-volume|cleanup-volume> [docker-volume-name]" >&2
  exit 1
}

require_darwin() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "host NFS bootstrap is only supported on macOS in this helper" >&2
    exit 1
  fi
}

require_volume_name() {
  if [[ -z "${VOLUME_NAME:-}" ]]; then
    usage
  fi
}

macos_nfs_export_path() {
  local path="$1"
  case "$path" in
    /System/Volumes/Data/*)
      echo "$path"
      ;;
    /Users/*|/private/*|/tmp/*|/var/*)
      echo "/System/Volumes/Data${path}"
      ;;
    *)
      echo "$path"
      ;;
  esac
}

resolve_docker_desktop_host_ip() {
  local host_ip
  host_ip="$(
    docker run --rm alpine getent hosts "${HOST_ALIAS}" 2>/dev/null \
      | awk 'NR == 1 { print $1 }'
  )"
  if [[ -z "$host_ip" ]]; then
    echo "failed to resolve ${HOST_ALIAS} from inside Docker; cannot configure host-backed NFS volume" >&2
    exit 1
  fi
  echo "$host_ip"
}

derive_host_network() {
  local host_ip="$1"
  local octet1 octet2 octet3 octet4
  IFS=. read -r octet1 octet2 octet3 octet4 <<<"$host_ip"
  if [[ -z "$octet1" || -z "$octet2" || -z "$octet3" || -z "$octet4" ]]; then
    echo "expected an IPv4 Docker Desktop host address, got '$host_ip'" >&2
    exit 1
  fi
  echo "${octet1}.${octet2}.${octet3}.0"
}

strip_managed_block() {
  local source_file="$1"
  local target_file="$2"
  if [[ -f "$source_file" ]]; then
    awk -v begin="$MANAGED_BEGIN" -v end="$MANAGED_END" '
      $0 == begin { skipping = 1; next }
      $0 == end { skipping = 0; next }
      !skipping { print }
    ' "$source_file" > "$target_file"
  else
    : > "$target_file"
  fi
}

write_managed_exports() {
  local host_network="$1"
  local tmp_without_block tmp_next
  tmp_without_block="$(mktemp)"
  tmp_next="$(mktemp)"
  strip_managed_block "$EXPORTS_FILE" "$tmp_without_block"

  {
    cat "$tmp_without_block"
    if [[ -s "$tmp_without_block" ]]; then
      printf '\n'
    fi
    printf '%s\n' "$MANAGED_BEGIN"
    printf '%s -alldirs -mapall=%s:%s -network %s -mask %s\n' \
      "$EXPORT_SPEC_PATH" "$MAP_UID" "$MAP_GID" "$host_network" "$HOST_NETMASK"
    printf '%s\n' "$MANAGED_END"
  } > "$tmp_next"

  sudo install -m 644 "$tmp_next" "$EXPORTS_FILE"
  rm -f "$tmp_without_block" "$tmp_next"
}

clear_export_dir() {
  mkdir -p "$EXPORT_DIR"
  find "$EXPORT_DIR" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
}

ensure_nfsd_running() {
  sudo nfsd enable
  sudo nfsd checkexports
  sudo nfsd restart
}

assert_host_ready() {
  if [[ ! -f "$EXPORTS_FILE" ]] || ! grep -F "$MANAGED_BEGIN" "$EXPORTS_FILE" >/dev/null 2>&1; then
    echo "host NFS export is not initialized; run 'just ci-docker-nfs-init' first" >&2
    exit 1
  fi

  if ! grep -F "${EXPORT_SPEC_PATH} " "$EXPORTS_FILE" >/dev/null 2>&1; then
    echo "host NFS export does not match the current ci-docker path; rerun 'just ci-docker-nfs-init'" >&2
    exit 1
  fi

  if ! nfsd status 2>&1 | grep -F "nfsd service is enabled" >/dev/null; then
    echo "macOS nfsd is not enabled; run 'just ci-docker-nfs-init' first" >&2
    exit 1
  fi

  if ! nfsd status 2>&1 | grep -F "nfsd is running" >/dev/null; then
    echo "macOS nfsd is not running; run 'just ci-docker-nfs-init' first" >&2
    exit 1
  fi
}

recreate_nfs_volume() {
  local host_ip="$1"
  docker volume rm "$VOLUME_NAME" >/dev/null 2>&1 || true
  docker volume create \
    --driver local \
    --opt type=nfs \
    --opt "o=addr=${host_ip},rw,nfsvers=4,tcp,resvport" \
    --opt "device=:${EXPORT_SPEC_PATH}" \
    "$VOLUME_NAME" >/dev/null
}

smoke_test_volume() {
  docker run --rm \
    -v "${VOLUME_NAME}:/mnt/g5-repos" \
    alpine \
    sh -lc 'touch /mnt/g5-repos/.g5-nfs-smoke && rm -f /mnt/g5-repos/.g5-nfs-smoke'
}

init_host() {
  local host_ip host_network
  host_ip="$(resolve_docker_desktop_host_ip)"
  host_network="$(derive_host_network "$host_ip")"

  mkdir -p "$EXPORT_DIR"
  write_managed_exports "$host_network"
  ensure_nfsd_running

  echo "Initialized host-backed NFS export for ci-docker"
  echo "  export dir: ${EXPORT_DIR}"
  echo "  export spec path: ${EXPORT_SPEC_PATH}"
  echo "  docker host alias: ${HOST_ALIAS} (${host_ip})"
  echo "  export net: ${host_network}/${HOST_NETMASK}"
}

prepare_volume() {
  local host_ip
  require_volume_name
  assert_host_ready
  host_ip="$(resolve_docker_desktop_host_ip)"

  clear_export_dir
  recreate_nfs_volume "$host_ip"
  smoke_test_volume

  echo "Prepared host-backed NFS volume '$VOLUME_NAME'"
}

cleanup_volume() {
  require_volume_name
  docker volume rm "$VOLUME_NAME" >/dev/null 2>&1 || true
  clear_export_dir
}

COMMAND="${1:-}"
VOLUME_NAME="${2:-}"

HOST_ALIAS="${G5_CI_DOCKER_NFS_HOST:-host.docker.internal}"
HOST_NETMASK="${G5_CI_DOCKER_NFS_MASK:-255.255.255.0}"
HOST_NFS_ROOT="${G5_CI_DOCKER_NFS_ROOT:-${HOME}/.g5-ci-docker-nfs}"
EXPORT_DIR="${HOST_NFS_ROOT}/repos"
EXPORT_SPEC_PATH="$(macos_nfs_export_path "${EXPORT_DIR}")"
MANAGED_BEGIN="# >>> g5 ci-docker host nfs >>>"
MANAGED_END="# <<< g5 ci-docker host nfs <<<"
EXPORTS_FILE="/etc/exports"
MAP_UID="${G5_CI_DOCKER_NFS_UID:-$(id -u)}"
MAP_GID="${G5_CI_DOCKER_NFS_GID:-$(id -g)}"

require_darwin

case "$COMMAND" in
  init-host)
    init_host
    ;;
  prepare-volume)
    prepare_volume
    ;;
  cleanup-volume)
    cleanup_volume
    ;;
  *)
    usage
    ;;
esac
