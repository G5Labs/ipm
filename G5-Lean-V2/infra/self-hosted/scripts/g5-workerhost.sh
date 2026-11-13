#!/usr/bin/env bash
set -euo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
G5_HOME="${G5_HOME:-$(cd "$SELF_DIR" && pwd)}"
G5_DATA_DIR="${G5_DATA_DIR:-/var/lib/g5}"
G5_MANIFEST_FILE="${G5_MANIFEST_FILE:-$G5_HOME/install_manifest.json}"
G5_REGISTRY_TOKEN_FILE="${G5_REGISTRY_TOKEN_FILE:-$G5_HOME/.registry-token.json}"
DOCKER_CONFIG="${DOCKER_CONFIG:-$G5_HOME/.docker}"
export DOCKER_CONFIG
G5_WORKERHOST_DRY_RUN="${G5_WORKERHOST_DRY_RUN:-${G5_INSTALL_DRY_RUN:-false}}"
G5_HOST_REGISTRATION_TIMEOUT="${G5_HOST_REGISTRATION_TIMEOUT:-660}"
G5_HOST_STABILITY_SECONDS="${G5_HOST_STABILITY_SECONDS:-60}"
G5_REGISTRY_TOKEN_REFRESH_SKEW_SECS="${G5_REGISTRY_TOKEN_REFRESH_SKEW_SECS:-300}"
G5_RUNTIME_STAGE_DIR="${G5_RUNTIME_STAGE_DIR:-$G5_DATA_DIR/microvm-runtime}"
G5_HOST_STATE_DIR="${G5_HOST_STATE_DIR:-$G5_DATA_DIR/microvm-host-state}"
G5_HOST_DATA_DIR="${G5_HOST_DATA_DIR:-$G5_DATA_DIR/microvm-data}"
G5_HOST_XFS_IMAGE="${G5_HOST_XFS_IMAGE:-$G5_HOST_STATE_DIR/g5d-data.xfs.img}"
G5_HOST_XFS_SIZE_GB="${G5_HOST_XFS_SIZE_GB:-64}"
G5_PLATFORM_ROOT="${G5_PLATFORM_ROOT:-$G5_DATA_DIR/worker-platform}"

die() {
  echo "error: $*" >&2
  exit 1
}

log() {
  echo "g5-workerhost: $*"
}

is_true() {
  case "${1:-}" in
    true|TRUE|1|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

run() {
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

require_cmd() {
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    return 0
  fi
  command -v "$1" >/dev/null 2>&1 || die "$1 is required but was not found"
}

require_file() {
  [ -f "$1" ] || die "required file missing: $1"
}

python_json() {
  require_cmd python3
  python3 "$@"
}

registry_token_request_body() {
  python_json - <<'PY'
import json
import os

body = {
    "product": "g5-self-hosted",
    "version": os.environ.get("G5_TARGET_VERSION") or os.environ.get("G5_RELEASE_VERSION") or "latest",
    "platform": "linux/amd64",
}
print(json.dumps(body, separators=(",", ":")))
PY
}

registry_token_is_fresh() {
  [ -f "$G5_REGISTRY_TOKEN_FILE" ] || return 1
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    return 0
  fi

  python_json - "$G5_REGISTRY_TOKEN_FILE" "$G5_REGISTRY_TOKEN_REFRESH_SKEW_SECS" <<'PY'
import json
import os
import sys
import time
from datetime import datetime, timezone

path = sys.argv[1]
refresh_skew_secs = int(sys.argv[2])

try:
    with open(path, encoding="utf-8") as token_file:
        token_response = json.load(token_file)
except (OSError, json.JSONDecodeError):
    sys.exit(1)

now = time.time()
expires_at = token_response.get("expires_at")
if isinstance(expires_at, str) and expires_at.strip():
    normalized = expires_at.strip()
    if normalized.endswith("Z"):
        normalized = f"{normalized[:-1]}+00:00"
    try:
        expires_at_dt = datetime.fromisoformat(normalized)
        if expires_at_dt.tzinfo is None:
            expires_at_dt = expires_at_dt.replace(tzinfo=timezone.utc)
        sys.exit(0 if expires_at_dt.timestamp() - refresh_skew_secs > now else 1)
    except ValueError:
        pass

try:
    expires_in_secs = float(token_response["expires_in"])
    file_mtime = os.path.getmtime(path)
except (KeyError, TypeError, ValueError, OSError):
    sys.exit(1)

sys.exit(0 if file_mtime + expires_in_secs - refresh_skew_secs > now else 1)
PY
}

fetch_registry_token_if_needed() {
  if [ -n "${G5_REGISTRY_TOKEN_RESPONSE_FILE:-}" ]; then
    require_file "$G5_REGISTRY_TOKEN_RESPONSE_FILE"
    run cp "$G5_REGISTRY_TOKEN_RESPONSE_FILE" "$G5_REGISTRY_TOKEN_FILE"
    run chmod 0600 "$G5_REGISTRY_TOKEN_FILE"
    return 0
  fi

  if [ -f "$G5_REGISTRY_TOKEN_FILE" ]; then
    if registry_token_is_fresh; then
      return 0
    fi
    log "registry token response is expired or near expiry; requesting a fresh token"
  fi

  [ -n "${G5_LICENSE_KEY:-}" ] || die "G5_LICENSE_KEY is required to fetch registry credentials"
  [ -n "${G5_API_URL:-}" ] || die "G5_API_URL is required to fetch registry credentials"

  local body
  body="$(registry_token_request_body)"
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    printf '+ curl -fsSL -X POST %q -H %q -H %q -d %q -o %q\n' \
      "${G5_API_URL%/}/api/v1/registry/token" \
      "Authorization: Bearer [REDACTED]" \
      "Content-Type: application/json" \
      "$body" \
      "$G5_REGISTRY_TOKEN_FILE"
  else
    umask 077
    curl -fsSL \
      -X POST "${G5_API_URL%/}/api/v1/registry/token" \
      -H "Authorization: Bearer $G5_LICENSE_KEY" \
      -H "Content-Type: application/json" \
      -d "$body" \
      -o "$G5_REGISTRY_TOKEN_FILE"
  fi
}

write_docker_auth_config_from_token() {
  require_file "$G5_REGISTRY_TOKEN_FILE"
  local registry username password
  registry="$(python_json - "$G5_REGISTRY_TOKEN_FILE" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(data.get("registry", ""))
PY
)"
  username="$(python_json - "$G5_REGISTRY_TOKEN_FILE" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(data.get("username", "oauth2accesstoken"))
PY
)"
  password="$(python_json - "$G5_REGISTRY_TOKEN_FILE" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(data.get("password", ""))
PY
)"
  [ -n "$registry" ] || die "registry token response missing registry"
  [ -n "$password" ] || die "registry token response missing password"

  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    printf '+ write docker auth config %q for %q\n' "$DOCKER_CONFIG/config.json" "$registry"
  else
    REGISTRY="$registry" USERNAME="$username" PASSWORD="$password" DOCKER_CONFIG="$DOCKER_CONFIG" python_json - <<'PY'
import base64
import json
import os
from pathlib import Path

registry = os.environ["REGISTRY"]
username = os.environ["USERNAME"]
password = os.environ["PASSWORD"]
config_dir = Path(os.environ["DOCKER_CONFIG"])
config_dir.mkdir(parents=True, exist_ok=True)
config_path = config_dir / "config.json"

if config_path.exists():
    try:
        data = json.loads(config_path.read_text())
    except json.JSONDecodeError:
        data = {}
else:
    data = {}

auth = base64.b64encode(f"{username}:{password}".encode()).decode()
auths = data.setdefault("auths", {})
auth_entry = {"auth": auth}
auths[registry] = auth_entry
auths[f"https://{registry}"] = auth_entry

config_path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
config_path.chmod(0o600)
PY
  fi
}

ensure_manifest() {
  if [ -f "$G5_MANIFEST_FILE" ]; then
    return 0
  fi
  require_file "$G5_REGISTRY_TOKEN_FILE"
  run python3 "$G5_HOME/scripts/token-response-to-manifest.py" "$G5_REGISTRY_TOKEN_FILE" "$G5_MANIFEST_FILE"
}

resolve_worker_image() {
  if [ -n "${G5_IMAGE_WORKER:-}" ]; then
    printf '%s' "$G5_IMAGE_WORKER"
    return 0
  fi
  require_file "$G5_MANIFEST_FILE"
  python_json - "$G5_MANIFEST_FILE" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
image = (manifest.get("images") or {}).get("worker", "")
if not image:
    raise SystemExit("release manifest missing worker image")
print(image)
PY
}

validate_digest_pinned_base_image() {
  local image_ref="$1"
  if ! python_json - "$image_ref" <<'PY'
import re
import sys

reference = sys.argv[1]
registry = r"[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?(?::[0-9]+)?"
component = r"[a-z0-9]+(?:[._-][a-z0-9]+)*"
pattern = re.compile(
    rf"^{registry}/{component}(?:/{component})*@sha256:[0-9a-f]{{64}}$"
)
raise SystemExit(0 if pattern.fullmatch(reference) else 1)
PY
  then
    die "G5D_BASE_IMAGE must be digest-pinned with no tag: <registry>/<repository>@sha256:<64 lowercase hex>"
  fi
}

validate_isolation_configuration() {
  local isolation_mode="${G5D_ISOLATION_MODE:-microvm}"
  case "$isolation_mode" in
    microvm)
      local base_image="${G5D_HOST_BASE_IMAGE:-${G5D_BASE_IMAGE:-}}"
      [ -n "$base_image" ] \
        || die "G5D_HOST_BASE_IMAGE (or G5D_BASE_IMAGE) is required for $isolation_mode isolation"
      validate_digest_pinned_base_image "$base_image"
      if ! is_true "$G5_WORKERHOST_DRY_RUN"; then
        [ -c /dev/kvm ] || die "G5D_ISOLATION_MODE=$isolation_mode requires /dev/kvm as a character device"
        [ "$(uname -m)" = x86_64 ] || die "Cloud Hypervisor worker hosts require x86_64"
        [ -f /sys/fs/cgroup/cgroup.controllers ] || die "worker hosts require cgroup v2"
        require_cmd systemctl
        require_cmd systemd-analyze
        require_cmd systemd-escape
        require_cmd apparmor_parser
        local systemd_version
        systemd_version="$(systemctl --version | awk 'NR == 1 {print $2}')"
        [[ "$systemd_version" =~ ^[0-9]+$ && "$systemd_version" -ge 254 ]] \
          || die "worker hosts require systemd 254 or newer for scoped delegation"
        if [ -e "$G5_PLATFORM_ROOT" ] && [ ! -f "$G5_PLATFORM_ROOT/.g5-platform-root" ]; then
          die "refusing to replace a platform directory not owned by this installer: $G5_PLATFORM_ROOT"
        fi
      fi

      ;;
    *)
      die "unsupported G5D_ISOLATION_MODE for external worker host: $isolation_mode"
      ;;
  esac
}

resolve_kvm_gid() {
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    printf '%s' '<kvm-gid>'
    return 0
  fi

  local gid
  gid="$(stat -c '%g' /dev/kvm 2>/dev/null || stat -f '%g' /dev/kvm 2>/dev/null || true)"
  case "$gid" in
    ''|*[!0-9]*) die "could not resolve the numeric group for /dev/kvm" ;;
  esac
  printf '%s' "$gid"
}

probe_microvm_device_access() {
  local image="$1"
  case "${G5D_ISOLATION_MODE:-microvm}" in
    microvm) ;;
    *) return 0 ;;
  esac

  local kvm_gid
  kvm_gid="$(resolve_kvm_gid)"

  if ! run docker run --rm \
    --device=/dev/kvm \
    --group-add="$kvm_gid" \
    --entrypoint sh \
    "$image" \
    -c 'test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm'; then
    die "g5 worker image cannot read and write /dev/kvm as its non-root runtime user; verify host KVM ownership/group access"
  fi
}

prepare_microvm_runtime() {
  local image="$1"
  local staging_script="$G5_HOME/scripts/stage-local-worker-runtime.sh"
  local image_marker="$G5_RUNTIME_STAGE_DIR/release-worker-image"

  require_file "$staging_script"
  if ! is_true "$G5_WORKERHOST_DRY_RUN" \
    && [ -x "$G5_RUNTIME_STAGE_DIR/runtime/bin/g5d" ] \
    && [ -r "$image_marker" ] \
    && [ "$(cat "$image_marker")" = "$image" ]; then
    log "reusing guest runtime staged from $image"
    return 0
  fi

  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    printf '+ stage complete microVM guest runtime from %q into %q\n' "$image" "$G5_RUNTIME_STAGE_DIR"
    return 0
  fi

  local next_dir="${G5_RUNTIME_STAGE_DIR}.next"
  rm -rf "$next_dir"
  mkdir -p "$next_dir"
  log "staging complete microVM guest runtime from immutable worker image"
  DOCKER_CONFIG="$DOCKER_CONFIG" \
    "$staging_script" \
      --arch x64 \
      --out "$next_dir" \
      --g5d-from-image "$image"
  printf '%s\n' "$image" > "$next_dir/release-worker-image"
  rm -rf "$G5_RUNTIME_STAGE_DIR"
  mv "$next_dir" "$G5_RUNTIME_STAGE_DIR"
}

prepare_microvm_xfs() {
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    printf '+ provision and verify XFS reflink store %q at %q\n' \
      "$G5_HOST_XFS_IMAGE" "$G5_HOST_DATA_DIR"
    return 0
  fi

  require_cmd findmnt
  require_cmd mkfs.xfs
  require_cmd mount
  require_cmd mountpoint
  require_cmd truncate

  mkdir -p "$G5_HOST_STATE_DIR" "$G5_HOST_DATA_DIR"
  if ! mountpoint -q "$G5_HOST_DATA_DIR"; then
    if [ ! -e "$G5_HOST_XFS_IMAGE" ]; then
      log "provisioning ${G5_HOST_XFS_SIZE_GB}G XFS reflink store"
      truncate -s "${G5_HOST_XFS_SIZE_GB}G" "$G5_HOST_XFS_IMAGE"
      mkfs.xfs -f -q -m reflink=1 "$G5_HOST_XFS_IMAGE"
    fi
    mount -o loop "$G5_HOST_XFS_IMAGE" "$G5_HOST_DATA_DIR"
  fi

  [ "$(findmnt -n -o FSTYPE --target "$G5_HOST_DATA_DIR")" = xfs ] \
    || die "$G5_HOST_DATA_DIR must be mounted as XFS"
  local probe_src="$G5_HOST_DATA_DIR/.reflink-probe.src"
  local probe_dst="$G5_HOST_DATA_DIR/.reflink-probe.dst"
  printf 'reflink-probe-%s\n' "$$" > "$probe_src"
  cp --reflink=always "$probe_src" "$probe_dst"
  cmp "$probe_src" "$probe_dst"
  rm -f "$probe_src" "$probe_dst"
  mkdir -p \
    "$G5_HOST_DATA_DIR/state" \
    "$G5_HOST_DATA_DIR/cache/repos" \
    "$G5_HOST_DATA_DIR/oci-layers" \
    "$G5_HOST_DATA_DIR/lib/workspaces" \
    "$G5_HOST_DATA_DIR/rootfs"
  log "XFS reflink round-trip verified"
}

stop_previous_supervisor() {
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    log "drain the previous supervisor before replacing immutable artifacts"
    return
  fi
  if systemctl cat g5d-host.service >/dev/null 2>&1; then
    systemctl stop g5d-host.service
  fi
  if docker container inspect g5d-host >/dev/null 2>&1; then
    docker stop --time 2100 g5d-host
    docker rm g5d-host
  fi
}

start_microvm_service() {
  local image="$1"
  local service_env="$G5_HOST_STATE_DIR/host.env"
  export G5D_API_URL G5_HOST_JWT G5D_ORG_ID G5D_HOST_ID
  export G5D_HOST_BASE_IMAGE="${G5D_HOST_BASE_IMAGE:-${G5D_BASE_IMAGE:-}}"
  export G5D_ISOLATION_MODE=microvm G5D_WORKLOAD_ROOT=block
  export G5D_HOST_RUNTIME_BIN_DIR=/opt/g5-runtime/bin
  export G5D_ROOTFS_STATE_DIR=/var/lib/g5d/rootfs
  export G5D_OCI_CACHE_DIR=/var/g5d-data/oci-layers
  export G5D_HOST_STATE_DIR=/var/g5d-data/state
  export G5D_HOST_CACHE_ROOT=/var/g5d-data/cache/repos
  export G5D_HOST_WORKSPACES_ROOT=/var/g5d-data/lib/workspaces
  export G5D_HOST_MAX_MICROVMS="${G5D_HOST_MAX_MICROVMS:-1}"
  export G5D_SCRATCH_DISK_MIB="${G5D_SCRATCH_DISK_MIB:-10240}"
  export RUST_LOG="${RUST_LOG:-g5d=info}"
  # Serialize explicit inputs only; EnvironmentFile is host-private. It is not
  # bind-mounted into the service root or any guest.
  python3 - "$service_env" "$G5_WORKERHOST_DRY_RUN" <<'PYENV'
import json, os, sys
keys = """G5D_API_URL G5_HOST_JWT G5D_ORG_ID G5D_HOST_ID G5D_HOST_BASE_IMAGE
G5D_ISOLATION_MODE G5D_WORKLOAD_ROOT G5D_HOST_RUNTIME_BIN_DIR G5D_ROOTFS_STATE_DIR
G5D_OCI_CACHE_DIR G5D_HOST_STATE_DIR G5D_HOST_CACHE_ROOT G5D_HOST_WORKSPACES_ROOT
G5D_HOST_MAX_MICROVMS G5D_SCRATCH_DISK_MIB G5D_HOST_CPUS_PER_VM G5D_HOST_MEM_MIB_PER_VM
G5D_NESTED_VIRTUALIZATION G5D_GIT_HTTP_ALLOW_HOSTS G5D_GUEST_HOST_ALIASES G5D_LLM_PROVIDER G5D_LLM_MODEL
ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_MODEL OPENAI_API_KEY
OPENAI_BASE_URL OPENAI_MODEL OPENROUTER_API_KEY RUST_LOG""".split()
values = {key: os.environ[key] for key in keys if os.environ.get(key)}
values.update(HOME='/var/g5d-data/state/home', PATH='/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin')
if any(any(c in value for c in '\n\r\0') for value in values.values()):
    raise SystemExit('worker environment must contain only single-line values')
if sys.argv[2].lower() in ('true', '1', 'yes'):
    for key, value in values.items():
        print(f'{key}=' + ('[REDACTED]' if key.endswith('_KEY') or key == 'G5_HOST_JWT' else value))
else:
    fd = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w') as out:
        os.fchmod(out.fileno(), 0o600)
        for key, value in values.items():
            # systemd's double-quoted env syntax supports these escapes, but
            # does not interpret JSON's optional Unicode escaping.
            out.write(f'{key}={json.dumps(value, ensure_ascii=False)}\n')
PYENV
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    log "extract immutable platform worker $image into read-only service root $G5_PLATFORM_ROOT"
    log "install g5d-host.service: Delegate=yes, private KVM device, restricted capabilities, passt AppArmor"
    log "bind $G5_HOST_DATA_DIR:/var/g5d-data and canonical /var/lib/g5d/rootfs"
    return
  fi
  require_cmd systemctl
  require_cmd apparmor_parser
  local next_root cid
  next_root="$(mktemp -d "${G5_PLATFORM_ROOT}.next.XXXXXX")"
  cid="$(docker create --entrypoint /bin/true "$image")"
  # This is the platform worker image, never G5D_HOST_BASE_IMAGE.
  if ! docker export "$cid" | tar -xp -C "$next_root"; then
    docker rm "$cid" >/dev/null
    die "could not extract the trusted worker image"
  fi
  docker rm "$cid" >/dev/null
  touch "$next_root/.g5-platform-root"
  rm -rf "$G5_PLATFORM_ROOT"
  mv "$next_root" "$G5_PLATFORM_ROOT"
  python3 "$G5_HOME/scripts/install-worker-service.py" \
    --root "$G5_PLATFORM_ROOT" --runtime "$G5_RUNTIME_STAGE_DIR/runtime" \
    --data "$G5_HOST_DATA_DIR" --disk "$G5_HOST_XFS_IMAGE" --env-file "$service_env"
}

wait_for_registration() {
  local deadline=$((SECONDS + G5_HOST_REGISTRATION_TIMEOUT))
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    echo "+ wait for host registration"
    return 0
  fi
  while true; do
    if journalctl -u g5d-host.service --since "$G5_SERVICE_STARTED_AT" --no-pager 2>&1 | grep -E "Host registered with g5-api|[Hh]ost tunnel connected|control tunnel.*connected|host.*ready" >/dev/null; then
      log "workspace host registered: $G5D_HOST_ID"
      return 0
    fi
    if ! systemctl is-active --quiet g5d-host.service; then
      journalctl -u g5d-host.service -n 100 --no-pager >&2 || true
      die "g5d-host exited"
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      journalctl -u g5d-host.service -n 100 --no-pager >&2 || true
      die "timed out waiting for workspace host registration"
    fi
    sleep 5
  done
}

verify_microvm_stability() {
  if is_true "$G5_WORKERHOST_DRY_RUN"; then
    echo "+ verify microvm service stability"
    return 0
  fi

  local deadline=$((SECONDS + G5_HOST_STABILITY_SECONDS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    local status restart_count
    status="$(systemctl show --property=ActiveState --value g5d-host.service)"
    restart_count="$(systemctl show --property=NRestarts --value g5d-host.service)"
    case "$restart_count" in
      ''|*[!0-9]*) restart_count=1 ;;
    esac
    if [ "$status" != active ] || [ "$restart_count" -gt 0 ]; then
      journalctl -u g5d-host.service -n 100 --no-pager >&2 || true
      die "microvm service is not stable (status: ${status:-missing}, restarts: $restart_count)"
    fi
    sleep 5
  done
  log "workspace host remained stable for ${G5_HOST_STABILITY_SECONDS}s"
}

require_cmd docker
require_cmd python3
[ -n "${G5D_API_URL:-}" ] || die "G5D_API_URL is required"
[ -n "${G5D_ORG_ID:-}" ] || die "G5D_ORG_ID is required"
[ -n "${G5D_HOST_ID:-}" ] || die "G5D_HOST_ID is required"
[ -n "${G5_HOST_JWT:-}" ] || die "G5_HOST_JWT is required"
validate_isolation_configuration

run mkdir -p "$G5_HOME" "$G5_HOST_STATE_DIR" "$G5_HOST_DATA_DIR"
fetch_registry_token_if_needed
write_docker_auth_config_from_token
ensure_manifest
G5_IMAGE_WORKER="$(resolve_worker_image)"
[ -n "$G5_IMAGE_WORKER" ] || die "worker image is required"
run docker pull "$G5_IMAGE_WORKER"
# Resolve a release tag once. Every extraction and staging operation below
# consumes the exact same locally immutable image configuration.
if ! is_true "$G5_WORKERHOST_DRY_RUN"; then
  G5_IMAGE_WORKER="$(docker image inspect --format '{{.Id}}' "$G5_IMAGE_WORKER")"
fi
stop_previous_supervisor
probe_microvm_device_access "$G5_IMAGE_WORKER"
prepare_microvm_runtime "$G5_IMAGE_WORKER"
prepare_microvm_xfs
G5_SERVICE_STARTED_AT="$(date --iso-8601=seconds 2>/dev/null || date -u +%Y-%m-%dT%H:%M:%SZ)"
start_microvm_service "$G5_IMAGE_WORKER"
wait_for_registration
verify_microvm_stability
