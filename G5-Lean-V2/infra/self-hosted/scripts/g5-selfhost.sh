#!/usr/bin/env bash
set -euo pipefail

COMMAND="${1:-}"
ARG="${2:-}"

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
G5_HOME="${G5_HOME:-$(cd "$SELF_DIR" && pwd)}"
G5_ENV_FILE="${G5_ENV_FILE:-$G5_HOME/.env}"
G5_COMPOSE_FILE="${G5_COMPOSE_FILE:-$G5_HOME/compose.yml}"
G5_TLS_COMPOSE_FILE="${G5_TLS_COMPOSE_FILE:-$G5_HOME/compose.tls.yml}"
G5_TLS_CADDYFILE="${G5_TLS_CADDYFILE:-$G5_HOME/Caddyfile.tls}"
G5_EXTRA_COMPOSE_FILE="${G5_EXTRA_COMPOSE_FILE:-}"
G5_MANIFEST_FILE="${G5_MANIFEST_FILE:-$G5_HOME/install_manifest.json}"
G5_BACKUP_DIR="${G5_BACKUP_DIR:-$G5_HOME/backups}"
G5_RELEASE_DIR="${G5_RELEASE_DIR:-$G5_HOME/releases}"
G5_DATA_DIR="${G5_DATA_DIR:-/var/lib/g5}"
G5_HOST_DOCKER_CONFIG="${DOCKER_CONFIG:-$HOME/.docker}"
DOCKER_CONFIG="${DOCKER_CONFIG:-$G5_HOME/.docker}"
export DOCKER_CONFIG G5_HOME G5_TLS_CADDYFILE
G5_SELFHOST_DRY_RUN="${G5_SELFHOST_DRY_RUN:-false}"
G5_SELFHOST_SKIP_PULL="${G5_SELFHOST_SKIP_PULL:-false}"
G5_SELFHOST_WAIT_TIMEOUT="${G5_SELFHOST_WAIT_TIMEOUT:-180}"
G5_REGISTRY_TOKEN_FILE="${G5_REGISTRY_TOKEN_FILE:-$G5_HOME/.registry-token.json}"
G5_UPGRADE_LOCK_DIR=""

die() {
  echo "error: $*" >&2
  exit 1
}

log() {
  echo "g5-selfhost: $*"
}

is_true() {
  case "${1:-}" in
    true|TRUE|1|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

run() {
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

require_cmd() {
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    return 0
  fi
  command -v "$1" >/dev/null 2>&1 || die "$1 is required but was not found"
}

require_file() {
  [ -f "$1" ] || die "required file missing: $1"
}

load_env() {
  require_file "$G5_ENV_FILE"
  set -a
  # shellcheck disable=SC1090
  . "$G5_ENV_FILE"
  set +a
  G5_DATA_DIR="${G5_DATA_DIR:-/var/lib/g5}"
}

source_env_exports() {
  local env_file="$1"
  require_file "$env_file"
  set -a
  # shellcheck disable=SC1090
  . "$env_file"
  set +a
  G5_DATA_DIR="${G5_DATA_DIR:-/var/lib/g5}"
}

env_mode_is_private() {
  local mode
  mode="$(stat -c '%a' "$G5_ENV_FILE" 2>/dev/null || stat -f '%Lp' "$G5_ENV_FILE")"
  [ "$mode" = "600" ]
}

check_env_mode() {
  if ! env_mode_is_private; then
    die "$G5_ENV_FILE must be mode 0600 because it contains customer secrets"
  fi
}

ensure_docker_compose_available() {
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    return 0
  fi
  if docker compose version >/dev/null 2>&1; then
    return 0
  fi

  local source_plugin="$G5_HOST_DOCKER_CONFIG/cli-plugins/docker-compose"
  local target_plugin="$DOCKER_CONFIG/cli-plugins/docker-compose"
  if [ "$G5_HOST_DOCKER_CONFIG" != "$DOCKER_CONFIG" ] && [ -x "$source_plugin" ]; then
    mkdir -p "$(dirname "$target_plugin")"
    ln -sf "$source_plugin" "$target_plugin"
  fi

  docker compose version >/dev/null 2>&1 || die "Docker Compose is required but was not found"
}

ensure_layout() {
  run mkdir -p "$G5_HOME" "$G5_BACKUP_DIR" "$G5_RELEASE_DIR"
  run mkdir -p \
    "$G5_DATA_DIR/postgres" \
    "$G5_DATA_DIR/storage/repos" \
    "$G5_DATA_DIR/storage/worktrees"
}

resolve_image_user_id() {
  local image="$1"
  local flag="$2"
  docker run --rm --entrypoint id "$image" "$flag" 2>/dev/null | tr -d '[:space:]'
}

repair_api_storage_permissions() {
  source_env_exports "$G5_ENV_FILE"
  local repos_dir="$G5_DATA_DIR/storage/repos"
  local worktrees_dir="$G5_DATA_DIR/storage/worktrees"
  run mkdir -p "$repos_dir" "$worktrees_dir"
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    printf '+ repair api storage ownership %q %q using %q\n' "$repos_dir" "$worktrees_dir" "${G5_IMAGE_API:-}"
    return 0
  fi

  if [ -z "${G5_IMAGE_API:-}" ]; then
    echo "error: G5_IMAGE_API is required to repair API storage permissions" >&2
    return 1
  fi
  if ! docker image inspect "$G5_IMAGE_API" >/dev/null 2>&1; then
    echo "error: API image unavailable for permission repair: $G5_IMAGE_API" >&2
    return 1
  fi

  local uid gid
  uid="$(resolve_image_user_id "$G5_IMAGE_API" -u)"
  gid="$(resolve_image_user_id "$G5_IMAGE_API" -g)"
  case "$uid" in
    ''|*[!0-9]*) echo "error: API image returned non-numeric uid: ${uid:-<empty>}" >&2; return 1 ;;
  esac
  case "$gid" in
    ''|*[!0-9]*) echo "error: API image returned non-numeric gid: ${gid:-<empty>}" >&2; return 1 ;;
  esac

  chmod -R u+rwX "$repos_dir" "$worktrees_dir" 2>/dev/null || true
  if chown -R "$uid:$gid" "$repos_dir" "$worktrees_dir" 2>/dev/null; then
    return 0
  fi
  if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
    if sudo chmod -R u+rwX "$repos_dir" "$worktrees_dir" \
      && sudo chown -R "$uid:$gid" "$repos_dir" "$worktrees_dir"; then
      return 0
    fi
  fi
  if command -v setfacl >/dev/null 2>&1 \
    && setfacl -R -m "u:$uid:rwx" -m "g:$gid:rwx" "$repos_dir" "$worktrees_dir" 2>/dev/null; then
    find "$repos_dir" "$worktrees_dir" -type d \
      -exec setfacl -m "d:u:$uid:rwx" -m "d:g:$gid:rwx" {} + 2>/dev/null || true
    log "API storage permissions granted via ACL: $repos_dir $worktrees_dir"
    return 0
  fi
  if is_true "${G5_SELFHOST_ALLOW_CHMOD_PERMISSION_FALLBACK:-false}" \
    && [ -O "$repos_dir" ] && [ -O "$worktrees_dir" ]; then
    if chmod -R a+rwX "$repos_dir" "$worktrees_dir"; then
      log "API storage ownership repair fell back to broad mode bits: $repos_dir $worktrees_dir"
      return 0
    fi
  fi

  echo "error: failed to repair API storage ownership for $repos_dir and $worktrees_dir as $uid:$gid" >&2
  echo "error: rerun as root/sudo, install setfacl, or set G5_SELFHOST_ALLOW_CHMOD_PERMISSION_FALLBACK=true for a user-owned test directory" >&2
  return 1
}

repair_api_storage_permissions_with_env_file() {
  local env_file="$1"
  local old_env="$G5_ENV_FILE"
  local status
  G5_ENV_FILE="$env_file"
  set +e
  repair_api_storage_permissions
  status=$?
  set -e
  G5_ENV_FILE="$old_env"
  source_env_exports "$old_env"
  return "$status"
}

validate_tls_configuration() {
  local cert_file="${G5_TLS_CERT_FILE:-}"
  local key_file="${G5_TLS_KEY_FILE:-}"

  if [[ ! "$cert_file" =~ [^[:space:]] ]]; then
    cert_file=""
  fi
  if [[ ! "$key_file" =~ [^[:space:]] ]]; then
    key_file=""
  fi
  G5_TLS_CERT_FILE="$cert_file"
  G5_TLS_KEY_FILE="$key_file"
  export G5_TLS_CERT_FILE G5_TLS_KEY_FILE

  if { [ -n "$cert_file" ] && [ -z "$key_file" ]; } \
    || { [ -z "$cert_file" ] && [ -n "$key_file" ]; }; then
    die "G5_TLS_CERT_FILE and G5_TLS_KEY_FILE must be configured together"
  fi
  if [ -z "$cert_file" ]; then
    return 1
  fi
  case "$cert_file" in
    /*) ;;
    *) die "G5_TLS_CERT_FILE must be an absolute host path" ;;
  esac
  case "$key_file" in
    /*) ;;
    *) die "G5_TLS_KEY_FILE must be an absolute host path" ;;
  esac
  [ -f "$cert_file" ] && [ -r "$cert_file" ] \
    || die "G5_TLS_CERT_FILE must name a readable regular file: $cert_file"
  [ -f "$key_file" ] && [ -r "$key_file" ] \
    || die "G5_TLS_KEY_FILE must name a readable regular file: $key_file"
  require_file "$G5_TLS_COMPOSE_FILE"
  require_file "$G5_TLS_CADDYFILE"
  return 0
}

compose_base_args() {
  printf '%s\n' --env-file "$G5_ENV_FILE" -f "$G5_COMPOSE_FILE"
  if [ -n "$G5_EXTRA_COMPOSE_FILE" ]; then
    [ -f "$G5_EXTRA_COMPOSE_FILE" ] || die "extra compose file missing: $G5_EXTRA_COMPOSE_FILE"
    printf '%s\n' -f "$G5_EXTRA_COMPOSE_FILE"
  fi
  if validate_tls_configuration; then
    printf '%s\n' -f "$G5_TLS_COMPOSE_FILE"
  fi
}

compose() {
  ensure_docker_compose_available
  local args=()
  local rendered_args
  if ! rendered_args="$(compose_base_args)"; then
    return 1
  fi
  while IFS= read -r item; do
    args+=("$item")
  done <<< "$rendered_args"
  run docker compose "${args[@]}" "$@"
}

compose_with_env_file() {
  local env_file="$1"
  shift
  local old_env="$G5_ENV_FILE"
  local status
  G5_ENV_FILE="$env_file"
  source_env_exports "$env_file"
  set +e
  compose "$@"
  status=$?
  set -e
  G5_ENV_FILE="$old_env"
  source_env_exports "$old_env"
  return "$status"
}

compose_pull() {
  if is_true "$G5_SELFHOST_SKIP_PULL"; then
    log "skipping image pull because G5_SELFHOST_SKIP_PULL=true"
    return 0
  fi
  compose pull "$@"
}

compose_pull_with_env_file() {
  local env_file="$1"
  shift
  if is_true "$G5_SELFHOST_SKIP_PULL"; then
    log "skipping image pull because G5_SELFHOST_SKIP_PULL=true"
    return 0
  fi
  compose_with_env_file "$env_file" pull "$@"
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

fetch_registry_token() {
  load_env
  if [ -n "${G5_REGISTRY_TOKEN_RESPONSE_FILE:-}" ]; then
    require_file "$G5_REGISTRY_TOKEN_RESPONSE_FILE"
    run cp "$G5_REGISTRY_TOKEN_RESPONSE_FILE" "$G5_REGISTRY_TOKEN_FILE"
    run chmod 0600 "$G5_REGISTRY_TOKEN_FILE"
    return 0
  fi

  [ -n "${G5_LICENSE_KEY:-}" ] || die "G5_LICENSE_KEY is required to fetch registry credentials"
  [ -n "${G5_API_URL:-}" ] || die "G5_API_URL is required to fetch registry credentials"

  local body
  body="$(registry_token_request_body)"
  if is_true "$G5_SELFHOST_DRY_RUN"; then
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
  if is_true "${G5_INSTALL_SKIP_DOCKER_LOGIN:-false}"; then
    log "skipping docker registry auth because G5_INSTALL_SKIP_DOCKER_LOGIN=true"
    return 0
  fi
  if is_true "$G5_SELFHOST_DRY_RUN"; then
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

render_next_env_from_manifest() {
  local manifest="$1"
  local output="$2"
  require_file "$manifest"
  require_file "$G5_ENV_FILE"
  python_json "$G5_HOME/scripts/render-selfhost-env.py" "$G5_ENV_FILE" "$manifest" "$output"
  chmod 0600 "$output"
}

manifest_from_token_response() {
  local output="$1"
  require_file "$G5_REGISTRY_TOKEN_FILE"
  python_json "$G5_HOME/scripts/token-response-to-manifest.py" "$G5_REGISTRY_TOKEN_FILE" "$output"
}

setup_cmd() {
  require_cmd docker
  require_cmd python3
  require_cmd just
  require_file "$G5_COMPOSE_FILE"
  require_file "$G5_MANIFEST_FILE"
  load_env
  check_env_mode
  ensure_layout
  if [ -n "${G5_LICENSE_KEY:-}" ] || [ -n "${G5_REGISTRY_TOKEN_RESPONSE_FILE:-}" ]; then
    fetch_registry_token
    write_docker_auth_config_from_token
    manifest_from_token_response "$G5_HOME/install_manifest.next.json"
    render_next_env_from_manifest "$G5_HOME/install_manifest.next.json" "$G5_HOME/.env.next"
    run mv "$G5_HOME/.env.next" "$G5_ENV_FILE"
    run mv "$G5_HOME/install_manifest.next.json" "$G5_MANIFEST_FILE"
  fi
  compose_pull
  log "setup complete"
}

run_migrations_if_needed() {
  local initialized="$G5_HOME/.initialized"
  if [ ! -f "$initialized" ] || is_true "${G5_RUN_MIGRATIONS_ON_UP:-false}"; then
    compose run --rm db-migrate
    run touch "$initialized"
  fi
}

starter_repo_git() {
  local repo_path="$1"
  shift
  git -c "safe.directory=$repo_path" -C "$repo_path" "$@"
}

seed_git_repo_if_empty() {
  local org_id="$1"
  local repo_id="$2"
  local seed_name="$3"
  # Repository storage is <base>/<org-id>/<repo-id>.
  local repo_path="$G5_DATA_DIR/storage/repos/$org_id/$repo_id"
  require_cmd git
  run mkdir -p "$repo_path"

  if is_true "$G5_SELFHOST_DRY_RUN"; then
    printf '+ seed %q starter repo at %q\n' "$seed_name" "$repo_path"
    printf '0000000000000000000000000000000000000000\n'
    return 0
  fi

  if [ ! -d "$repo_path/.git" ]; then
    starter_repo_git "$repo_path" init >/dev/null
  fi
  starter_repo_git "$repo_path" config user.name "G5 System"
  starter_repo_git "$repo_path" config user.email "g5@system.local"

  if starter_repo_git "$repo_path" rev-parse --verify HEAD >/dev/null 2>&1; then
    starter_repo_git "$repo_path" rev-parse HEAD
    return 0
  fi

  case "$seed_name" in
    docs)
      mkdir -p "$repo_path/designs"
      cat > "$repo_path/README.md" <<'EOF'
# G5 Self-Hosted Docs

This repository stores customer-owned G5 documentation and design artifacts.
EOF
      cat > "$repo_path/designs/pre-production.md" <<'EOF'
# Pre Production Deployment

This self-hosted G5 environment is intended for pre-production validation until
your organization explicitly promotes it.
EOF
      ;;
    tinydb)
      local seed_dir="$SELF_DIR/starter-repos/tinydb"
      require_file "$seed_dir/tinydb/__init__.py"
      cp -R "$seed_dir/." "$repo_path/"
      ;;
    *)
      die "unknown starter repo seed: $seed_name"
      ;;
  esac

  starter_repo_git "$repo_path" add -A
  if starter_repo_git "$repo_path" diff --cached --quiet; then
    die "starter repo seed produced no files for $seed_name"
  fi
  starter_repo_git "$repo_path" commit -m "Bootstrap $seed_name starter repo" >/dev/null
  starter_repo_git "$repo_path" branch -M main
  starter_repo_git "$repo_path" rev-parse HEAD
}

bootstrap_self_hosted_starter_workspace() {
  local org_id="$1"
  local org_slug="$2"
  local admin_email="$3"
  local seed_row base_workspace_id docs_repo_id code_repo_id docs_head code_head

  seed_row="$(
    compose exec -T postgres psql \
      -U "${G5_DB_USER:-g5}" \
      -d "${G5_DB_NAME:-g5}" \
      -v ON_ERROR_STOP=1 \
      -v org_id="$org_id" \
      -v admin_email="$admin_email" \
      -At -F $'\t' <<'SQL'
WITH org_row AS (
    SELECT id, slug FROM orgs WHERE id = :'org_id'
),
user_row AS (
    SELECT id FROM users WHERE email = :'admin_email'
),
workspace_insert AS (
    INSERT INTO workspaces (
        id, org_id, name, is_base,
        docs_branch_name, docs_tracking_mode,
        visibility, created_by, created_at, updated_at
    )
    SELECT gen_random_uuid(), org_row.id, 'Base', true,
           'main', 'tracking',
           'org', user_row.id, now(), now()
    FROM org_row, user_row
    WHERE NOT EXISTS (
        SELECT 1 FROM workspaces WHERE org_id = org_row.id AND is_base
    )
    RETURNING id, org_id
),
workspace_row AS (
    SELECT id, org_id FROM workspace_insert
    UNION ALL
    SELECT id, org_id FROM workspaces
     WHERE org_id = (SELECT id FROM org_row)
       AND is_base
    LIMIT 1
),
docs_insert AS (
    INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
    SELECT gen_random_uuid(), org_row.id, 'docs', 'docs', 'g5-local://' || org_row.slug || '/docs', 'main', now()
    FROM org_row
    WHERE NOT EXISTS (
        SELECT 1 FROM repos WHERE org_id = org_row.id AND slug = 'docs'
    )
    RETURNING id, org_id
),
docs_row AS (
    SELECT id, org_id FROM docs_insert
    UNION ALL
    SELECT id, org_id FROM repos
     WHERE org_id = (SELECT id FROM org_row)
       AND slug = 'docs'
    LIMIT 1
),
code_insert AS (
    INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
    SELECT gen_random_uuid(), org_row.id, 'tinydb', 'tinydb', 'g5-local://' || org_row.slug || '/tinydb', 'main', now()
    FROM org_row
    WHERE NOT EXISTS (
        SELECT 1 FROM repos WHERE org_id = org_row.id AND slug = 'tinydb'
    )
    RETURNING id, org_id
),
code_row AS (
    SELECT id, org_id FROM code_insert
    UNION ALL
    SELECT id, org_id FROM repos
     WHERE org_id = (SELECT id FROM org_row)
       AND slug = 'tinydb'
    LIMIT 1
),
workspace_docs AS (
    UPDATE workspaces
       SET docs_repo_id = (SELECT id FROM docs_row),
           updated_at = now()
     WHERE id = (SELECT id FROM workspace_row)
    RETURNING id
),
docs_member AS (
    INSERT INTO workspace_repos (workspace_id, org_id, repo_id, role, root_path)
    SELECT workspace_row.id, workspace_row.org_id, docs_row.id, 'docs', 'docs'
    FROM workspace_row, docs_row
    ON CONFLICT (workspace_id, repo_id) DO NOTHING
    RETURNING repo_id
),
code_member AS (
    INSERT INTO workspace_repos (workspace_id, org_id, repo_id, role, root_path)
    SELECT workspace_row.id, workspace_row.org_id, code_row.id, 'code', 'tinydb'
    FROM workspace_row, code_row
    ON CONFLICT (workspace_id, repo_id) DO NOTHING
    RETURNING repo_id
),
settings_row AS (
    INSERT INTO workspace_settings (workspace_id, org_id)
    SELECT workspace_row.id, workspace_row.org_id
    FROM workspace_row
    ON CONFLICT (workspace_id) DO NOTHING
    RETURNING workspace_id
)
SELECT workspace_row.id, docs_row.id, code_row.id
FROM workspace_row, docs_row, code_row;
SQL
  )"

  IFS=$'\t' read -r base_workspace_id docs_repo_id code_repo_id <<< "$seed_row"
  [ -n "$base_workspace_id" ] || die "self-hosted default workspace bootstrap did not return a workspace id"
  [ -n "$docs_repo_id" ] || die "self-hosted default workspace bootstrap did not return a docs repo id"
  [ -n "$code_repo_id" ] || die "self-hosted default workspace bootstrap did not return a code repo id"

  docs_head="$(seed_git_repo_if_empty "$org_id" "$docs_repo_id" docs)"
  code_head="$(seed_git_repo_if_empty "$org_id" "$code_repo_id" tinydb)"

  log "self-hosted starter workspace ready: $org_slug/base ($base_workspace_id), repos docs=$docs_repo_id tinydb=$code_repo_id"
}

update_env_values() {
  local updates_json="$1"
  UPDATES_JSON="$updates_json" python_json - "$G5_ENV_FILE" <<'PY'
import json
import os
import re
import sys
from pathlib import Path

SAFE_ENV_VALUE = re.compile(r"^[A-Za-z0-9_./:@%+=,-]*$")

def shell_env_value(value: str) -> str:
    if "\0" in value:
        raise SystemExit("environment values cannot contain NUL bytes")
    if SAFE_ENV_VALUE.fullmatch(value):
        return value
    escaped = []
    for ch in value:
        if ch == "\\":
            escaped.append("\\\\")
        elif ch == "'":
            escaped.append("\\'")
        elif ch == "\n":
            escaped.append("\\n")
        elif ch == "\r":
            escaped.append("\\r")
        elif ch == "\t":
            escaped.append("\\t")
        else:
            escaped.append(ch)
    return "$'" + "".join(escaped) + "'"

path = Path(sys.argv[1])
updates = json.loads(os.environ["UPDATES_JSON"])
lines = []
seen = set()
for line in path.read_text().splitlines():
    if "=" in line and not line.startswith("#"):
        key, _ = line.split("=", 1)
        if key in updates:
            lines.append(f"{key}={shell_env_value(str(updates[key]))}")
            seen.add(key)
            continue
    lines.append(line)
for key, value in updates.items():
    if key not in seen:
        lines.append(f"{key}={shell_env_value(str(value))}")
path.write_text("\n".join(lines) + "\n")
path.chmod(0o600)
PY
}

bootstrap_self_hosted_defaults() {
  load_env
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    echo "+ bootstrap self-hosted default org"
    return 0
  fi

  local org_name org_slug admin_email admin_name org_id
  org_name="${G5_SELF_HOSTED_DEFAULT_ORG_NAME:-Default Organization}"
  org_slug="${G5_SELF_HOSTED_DEFAULT_ORG_SLUG:-default}"
  admin_email="${G5_SELF_HOSTED_DEFAULT_ADMIN_EMAIL:-admin@g5.local}"
  admin_name="${G5_SELF_HOSTED_DEFAULT_ADMIN_NAME:-G5 Self-Hosted Admin}"
  org_id="$(
    compose exec -T postgres psql \
      -U "${G5_DB_USER:-g5}" \
      -d "${G5_DB_NAME:-g5}" \
      -v ON_ERROR_STOP=1 \
      -v org_name="$org_name" \
      -v org_slug="$org_slug" \
      -v admin_email="$admin_email" \
      -v admin_name="$admin_name" \
      -At <<'SQL'
WITH org_row AS (
    INSERT INTO orgs (name, slug)
    VALUES (:'org_name', :'org_slug')
    ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
    RETURNING id
),
user_row AS (
    INSERT INTO users (email, display_name, oauth_provider, oauth_id)
    VALUES (:'admin_email', :'admin_name', 'self_hosted', :'admin_email')
    ON CONFLICT (email) DO UPDATE SET display_name = EXCLUDED.display_name
    RETURNING id
),
membership_row AS (
    INSERT INTO org_memberships (org_id, user_id, role)
    SELECT org_row.id, user_row.id, 'admin'
    FROM org_row, user_row
    ON CONFLICT (org_id, user_id) DO UPDATE
      SET role = EXCLUDED.role
    RETURNING org_id
)
SELECT id FROM org_row;
SQL
  )"
  [ -n "$org_id" ] || die "self-hosted default org bootstrap did not return an org id"

  bootstrap_self_hosted_starter_workspace "$org_id" "$org_slug" "$admin_email"
  log "self-hosted default org ready: $org_slug ($org_id)"
}

up_cmd() {
  require_cmd docker
  load_env
  check_env_mode
  ensure_layout
  # Retire removed services before migrations: a renamed maintenance container
  # must not keep writing while the current bundle upgrades the database.
  compose up -d --remove-orphans --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" postgres
  run_migrations_if_needed
  bootstrap_self_hosted_defaults
  repair_api_storage_permissions
  compose up -d --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" g5-api g5-daemon edge
  compose ps
}

logs_cmd() {
  require_cmd docker
  load_env
  if [ -n "$ARG" ]; then
    compose logs -f --tail=200 "$ARG"
  else
    compose logs -f --tail=200
  fi
}

down_cmd() {
  require_cmd docker
  load_env
  compose down --remove-orphans
}

backup_manifest() {
  local stamp="$1"
  local dest="$2"
  python_json - "$stamp" "$dest" "$G5_ENV_FILE" "$G5_MANIFEST_FILE" <<'PY'
import json
import os
import sys
from pathlib import Path

stamp, dest, env_file, manifest_file = sys.argv[1:5]
manifest = {}
path = Path(manifest_file)
if path.exists():
    manifest = json.loads(path.read_text())

images = {}
release_version = ""
for line in Path(env_file).read_text().splitlines():
    if line.startswith("G5_RELEASE_VERSION="):
        _, release_version = line.split("=", 1)
    if line.startswith("G5_IMAGE_") and "=" in line:
        key, value = line.split("=", 1)
        images[key] = value

payload = {
    "timestamp": stamp,
    "releaseVersion": release_version or manifest.get("version", ""),
    "manifest": manifest,
    "images": images,
}
Path(dest).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY
}

backup_cmd() {
  require_cmd docker
  require_cmd python3
  load_env
  check_env_mode
  local stamp backup_path dump_path
  stamp="$(date -u +%Y%m%d-%H%M%S)"
  backup_path="$G5_BACKUP_DIR/$stamp"
  dump_path="$backup_path/postgres.dump"
  run mkdir -p "$backup_path"
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    # The directory above was not created, so the dump cannot be redirected.
    compose exec -T postgres pg_dump -U "${G5_DB_USER:-g5}" -d "${G5_DB_NAME:-g5}" -Fc
  else
    compose exec -T postgres pg_dump -U "${G5_DB_USER:-g5}" -d "${G5_DB_NAME:-g5}" -Fc > "$dump_path"
    [ -s "$dump_path" ] || die "backup dump is missing or empty: $dump_path"
  fi
  run cp "$G5_ENV_FILE" "$backup_path/.env"
  run cp "$G5_COMPOSE_FILE" "$backup_path/compose.yml"
  if [ -f "$G5_TLS_COMPOSE_FILE" ]; then
    run cp "$G5_TLS_COMPOSE_FILE" "$backup_path/compose.tls.yml"
  fi
  if [ -f "$G5_TLS_CADDYFILE" ]; then
    run cp "$G5_TLS_CADDYFILE" "$backup_path/Caddyfile.tls"
  fi
  run cp "$G5_MANIFEST_FILE" "$backup_path/install_manifest.json"
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    log "dry run: would write $backup_path/backup_manifest.json"
  else
    backup_manifest "$stamp" "$backup_path/backup_manifest.json"
  fi
  G5_LAST_BACKUP_PATH="$backup_path"
  log "backup complete: $backup_path"
}

restore_database_from_backup() {
  local backup_path="$1"
  local dump_path="$backup_path/postgres.dump"
  [ -s "$dump_path" ] || die "cannot restore missing backup dump: $dump_path"
  compose exec -T postgres dropdb -U "${G5_DB_USER:-g5}" --if-exists "${G5_DB_NAME:-g5}"
  compose exec -T postgres createdb -U "${G5_DB_USER:-g5}" "${G5_DB_NAME:-g5}"
  compose exec -T postgres pg_restore -U "${G5_DB_USER:-g5}" -d "${G5_DB_NAME:-g5}" --clean --if-exists < "$dump_path"
}

stop_app_services() {
  compose stop g5-api g5-daemon edge >/dev/null || true
}

rollback_current_release() {
  local backup_path="$1"
  stop_app_services
  restore_database_from_backup "$backup_path"
  repair_api_storage_permissions
  compose up -d --force-recreate --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" g5-api g5-daemon edge
}

latest_backup_path() {
  find "$G5_BACKUP_DIR" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1
}

restore_cmd() {
  require_cmd docker
  load_env
  check_env_mode
  ensure_layout
  local backup_path
  backup_path="${ARG:-}"
  if [ -z "$backup_path" ] || [ "$backup_path" = "latest" ]; then
    backup_path="$(latest_backup_path)"
  fi
  [ -n "$backup_path" ] || die "no backup found"
  [ -d "$backup_path" ] || die "backup directory not found: $backup_path"

  compose up -d --remove-orphans --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" postgres
  stop_app_services
  restore_database_from_backup "$backup_path"
  repair_api_storage_permissions
  compose up -d --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" g5-api g5-daemon edge
  log "restore complete: $backup_path"
}

# Stages the target release's manifest at "$1" and names the file to read it
# from in G5_TARGET_RELEASE_MANIFEST: the staged copy, or, in a dry run with a
# supplied manifest, that file itself, because the copy is only printed.
resolve_target_release() {
  local manifest="$1"
  G5_TARGET_RELEASE_MANIFEST="$manifest"
  if [ -n "${G5_TARGET_MANIFEST_FILE:-}" ]; then
    require_file "$G5_TARGET_MANIFEST_FILE"
    run cp "$G5_TARGET_MANIFEST_FILE" "$manifest"
    if is_true "$G5_SELFHOST_DRY_RUN"; then
      G5_TARGET_RELEASE_MANIFEST="$G5_TARGET_MANIFEST_FILE"
    fi
    return 0
  fi
  fetch_registry_token
  write_docker_auth_config_from_token
  manifest_from_token_response "$manifest"
}

promote_next_files() {
  local manifest_next="$1"
  local env_next="$2"
  run rm -f "$G5_ENV_FILE.previous" "$G5_MANIFEST_FILE.previous"
  run mv "$G5_ENV_FILE" "$G5_ENV_FILE.previous"
  run mv "$G5_MANIFEST_FILE" "$G5_MANIFEST_FILE.previous"
  run mv "$env_next" "$G5_ENV_FILE"
  run mv "$manifest_next" "$G5_MANIFEST_FILE"
}

upgrade_cmd() {
  require_cmd docker
  require_cmd python3
  load_env
  check_env_mode
  ensure_layout
  local lock_dir manifest_next env_next backup_path release_version release_record_dir
  lock_dir="$G5_HOME/.upgrade.lock"
  if ! mkdir "$lock_dir" 2>/dev/null; then
    die "another upgrade is already running"
  fi
  G5_UPGRADE_LOCK_DIR="$lock_dir"
  trap 'if [ -n "${G5_UPGRADE_LOCK_DIR:-}" ]; then rmdir "$G5_UPGRADE_LOCK_DIR" 2>/dev/null || true; fi' EXIT

  manifest_next="$G5_HOME/install_manifest.next.json"
  env_next="$G5_HOME/.env.next"
  resolve_target_release "$manifest_next"
  render_next_env_from_manifest "$G5_TARGET_RELEASE_MANIFEST" "$env_next"

  backup_cmd
  # The backup just taken, by its own path: a dry run creates no directory
  # and must not adopt an older backup as this upgrade's.
  backup_path="${G5_LAST_BACKUP_PATH:-}"
  [ -n "$backup_path" ] || die "pre-upgrade backup was not created"

  compose_pull_with_env_file "$env_next"
  compose up -d --remove-orphans --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" postgres
  stop_app_services

  if ! compose_with_env_file "$env_next" run --rm db-migrate; then
    log "migration failed; rolling back to previous release"
    rollback_current_release "$backup_path"
    run rm -f "$manifest_next" "$env_next"
    exit 1
  fi

  if ! repair_api_storage_permissions_with_env_file "$env_next"; then
    log "target API storage permission repair failed; rolling back to previous release"
    rollback_current_release "$backup_path"
    run rm -f "$manifest_next" "$env_next"
    exit 1
  fi

  if ! compose_with_env_file "$env_next" up -d --wait --wait-timeout "$G5_SELFHOST_WAIT_TIMEOUT" g5-api g5-daemon edge; then
    log "target health failed; rolling back to previous release"
    rollback_current_release "$backup_path"
    run rm -f "$manifest_next" "$env_next"
    exit 1
  fi

  compose_with_env_file "$env_next" ps
  # The release being installed, read from the target manifest before it is
  # promoted: a dry run prints the promotion without performing it.
  release_version="$(python_json - "$G5_TARGET_RELEASE_MANIFEST" <<'PY'
import json, sys
print(json.load(open(sys.argv[1])).get("version", "unknown"))
PY
)"
  promote_next_files "$manifest_next" "$env_next"
  release_record_dir="$G5_RELEASE_DIR/$release_version"
  run mkdir -p "$release_record_dir"
  if is_true "$G5_SELFHOST_DRY_RUN"; then
    log "dry run: would record the upgrade in $release_record_dir/upgrade.json"
  else
    python_json - "$release_record_dir/upgrade.json" "$release_version" "$backup_path" <<'PY'
import json, sys
from pathlib import Path
path, version, backup = sys.argv[1:4]
Path(path).write_text(json.dumps({"version": version, "backup": backup}, indent=2) + "\n")
PY
  fi
  rmdir "$lock_dir" 2>/dev/null || true
  G5_UPGRADE_LOCK_DIR=""
  trap - EXIT
  log "upgrade complete: $release_version"
}

case "$COMMAND" in
  setup) setup_cmd ;;
  up) up_cmd ;;
  logs) logs_cmd ;;
  down) down_cmd ;;
  backup) backup_cmd ;;
  restore) restore_cmd ;;
  upgrade) upgrade_cmd ;;
  doctor) load_env; check_env_mode; ensure_layout; compose ps ;;
  *) die "usage: $0 {setup|up|logs|down|backup|restore|upgrade|doctor}" ;;
esac
