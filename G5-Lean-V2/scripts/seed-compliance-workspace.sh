#!/usr/bin/env bash
# =============================================================================
# seed-compliance-workspace.sh — Seed a workspace from prepared compliance repos
# =============================================================================
# Creates or replaces a workspace in the running dev stack from prepared docs
# and code repos. By default (G5_SEED_WORKSPACE_BACKEND=native; the retired
# `g5d-push` name is accepted as an alias) it stages the prepared repos as
# local Git repos, installs them as the workspace's managed repos, publishes
# them with the native fast-forward recipe in scripts/lib/native-publish.sh
# and validates committed files and document references through their readers.
# Set G5_SEED_WORKSPACE_BACKEND=legacy-db to use the older direct DB/repo-volume
# seed path for diagnostics.
#
# Usage:
#   ./scripts/seed-compliance-workspace.sh [docs-repo] [code-repo] [api-url]
#
# Defaults:
#   docs-repo = ../compliance-workspace/compliance-docs
#   code-repo = ../compliance-workspace/compliance-code
#   api-url   = http://localhost:${G5_PORT_API:-7500}
#
# For prepared repos that intentionally preserve existing chat -> code/ontology
# grounding, set G5_COMPLIANCE_ALLOW_CHAT_TARGET_GROUNDING=true. The compliance
# seed keeps the stricter default.
# =============================================================================
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/seed-workspace-model.sh"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GEN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$GEN_ROOT"
source "$SCRIPT_DIR/lib/native-publish.sh"

DOCS_SOURCE_REPO="${1:-${COMPLIANCE_DOCS_SOURCE_REPO:-$WORKSPACE_ROOT/compliance-workspace/compliance-docs}}"
CODE_SOURCE_REPO="${2:-${COMPLIANCE_CODE_SOURCE_REPO:-$WORKSPACE_ROOT/compliance-workspace/compliance-code}}"
API_URL="${3:-http://localhost:${G5_PORT_API:-7500}}"

REPOS_VOLUME="${REPOS_VOLUME:-g5-lean_api_repos}"
COPY_IMAGE="${G5_COMPLIANCE_COPY_IMAGE:-g5/g5d-base:latest}"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
DB_NAME="${DB_NAME:-g5_dev}"
AUTH_EMAIL="${G5_COMPLIANCE_SEED_EMAIL:-demo@g5labs.ai}"
WORKSPACE_NAME="${G5_COMPLIANCE_WORKSPACE_NAME:-compliance-service}"
WORKSPACE_DESCRIPTION="${G5_COMPLIANCE_WORKSPACE_DESCRIPTION:-Imported compliance_service workspace}"
SEED_LABEL="${G5_COMPLIANCE_SEED_LABEL:-G5 Compliance Workspace Seed}"
DOCS_REPO_NAME="${G5_COMPLIANCE_DOCS_REPO_NAME:-compliance-docs}"
DOCS_REPO_SLUG="${G5_COMPLIANCE_DOCS_REPO_SLUG:-compliance-docs}"
CODE_REPO_NAME="${G5_COMPLIANCE_CODE_REPO_NAME:-compliance-code}"
CODE_REPO_SLUG="${G5_COMPLIANCE_CODE_REPO_SLUG:-compliance-code}"
LEGACY_REPO_SLUG="${G5_COMPLIANCE_LEGACY_REPO_SLUG:-compliance-service-repo}"
BRANCH_NAME="${G5_COMPLIANCE_BRANCH_NAME:-$WORKSPACE_NAME}"
[ "$BRANCH_NAME" = "$WORKSPACE_NAME" ] || { echo 'Fixture Git ref must match the workspace name' >&2; exit 1; }
ENV_FILE="${G5_COMPLIANCE_ENV_FILE:-$GEN_ROOT/.env.compliance-workspace}"
ALLOW_CHAT_TARGET_GROUNDING="${G5_COMPLIANCE_ALLOW_CHAT_TARGET_GROUNDING:-false}"
VALIDATE_PREPARED_OWNERSHIP="${G5_COMPLIANCE_VALIDATE_PREPARED_OWNERSHIP:-true}"
VERIFY_SOURCE_TREE="${G5_COMPLIANCE_VERIFY_SOURCE_TREE:-false}"
EXPECTED_ROOT_COMPONENT="${G5_COMPLIANCE_EXPECTED_ROOT_COMPONENT:-root}"
EXPECTED_CHILD_COMPONENT="${G5_COMPLIANCE_EXPECTED_CHILD_COMPONENT-core_validation}"
REQUIRED_DOC_LINK_PATH="${G5_COMPLIANCE_REQUIRED_DOC_LINK_PATH-views/receiver_management_lifecycle.pm_view.md}"
REQUIRED_DOC_LINK_MIN="${G5_COMPLIANCE_REQUIRED_DOC_LINK_MIN-4}"
REUSE_WS_ID=""
SEED_USER_ID=""

if [ ! -d "$DOCS_SOURCE_REPO" ]; then
  echo "✗ Docs source repo not found: $DOCS_SOURCE_REPO" >&2
  exit 1
fi

if [ ! -d "$CODE_SOURCE_REPO" ]; then
  echo "✗ Code source repo not found: $CODE_SOURCE_REPO" >&2
  exit 1
fi

DOCS_SOURCE_REPO="$(cd "$DOCS_SOURCE_REPO" && pwd)"
CODE_SOURCE_REPO="$(cd "$CODE_SOURCE_REPO" && pwd)"

if [ ! -f "$CODE_SOURCE_REPO/semlift.yaml" ]; then
  echo "✗ Code source repo is missing semlift.yaml: $CODE_SOURCE_REPO" >&2
  exit 1
fi

validate_prepared_repo_ownership() {
  python3 - "$DOCS_SOURCE_REPO" "$CODE_SOURCE_REPO" "$ALLOW_CHAT_TARGET_GROUNDING" <<'PY'
import json
import sys
from pathlib import Path

docs_root = Path(sys.argv[1])
code_root = Path(sys.argv[2])
allow_chat_target_grounding = sys.argv[3].strip().lower() in {"1", "true", "yes", "y", "on"}

DOCS_TYPES = {"view", "chat"}
CODE_TYPES = {"ontology", "code"}
KNOWN_TYPES = DOCS_TYPES | CODE_TYPES | {"external_doc"}
DOCS_PREFIXES = ("docs/", "designs/", "views/", "project_docs/")
MOUNT_PREFIXES = ("g5_docs/", "g5-docs/")
ALIASES = {
    "views": "view",
    "pm_view": "view",
    "arch_view": "view",
    "test_view": "view",
    "ui_view": "view",
    "security_view": "view",
    "external": "external_doc",
    "external_design_doc": "external_doc",
    "externaldoc": "external_doc",
}

def normalize(value):
    if value is None:
        return None
    key = str(value).strip().lower().replace("-", "_")
    key = ALIASES.get(key, key)
    if key in KNOWN_TYPES:
        return key
    return None

def canonical_doc_path(value):
    path = str(value or "").replace("\\", "/").lstrip("./")
    for prefix in MOUNT_PREFIXES:
        if path.startswith(prefix):
            path = path[len(prefix):]
            break
    return path

def owner(file_type, file_name=""):
    if file_type == "external_doc":
        canonical = canonical_doc_path(file_name)
        return "docs" if canonical.startswith(DOCS_PREFIXES) else "code"
    if file_type in DOCS_TYPES:
        return "docs"
    if file_type in CODE_TYPES:
        return "code"
    return None

def supported_edge(source_type, target_type):
    if source_type == "chat" and target_type in CODE_TYPES and not allow_chat_target_grounding:
        return False
    return True

def read_jsonl(path):
    records = []
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_no}: invalid JSON: {exc}") from exc
    return records

def type_from_subdir(path):
    return normalize(path.parent.name)

errors = []
for repo_label, repo_root in (("docs", docs_root), ("code", code_root)):
    for jsonl_file in sorted((repo_root / "intent_record").rglob("*.jsonl")):
        for record in read_jsonl(jsonl_file):
            ft = normalize(record.get("file_type")) or type_from_subdir(jsonl_file)
            expected_owner = owner(ft, record.get("file_name", ""))
            if expected_owner != repo_label:
                errors.append(f"{jsonl_file}: {ft or 'unknown'} intent belongs in {expected_owner or 'unknown'} repo, not {repo_label}")

    for jsonl_file in sorted((repo_root / "intent_grounding").rglob("*.jsonl")):
        for edge in read_jsonl(jsonl_file):
            source_ft = normalize(edge.get("source_file_type"))
            target_ft = normalize(edge.get("target_file_type")) or type_from_subdir(jsonl_file)
            expected_owner = owner(target_ft, edge.get("target_file", ""))
            if expected_owner != repo_label:
                errors.append(f"{jsonl_file}: {target_ft or 'unknown'}-targeted grounding belongs in {expected_owner or 'unknown'} repo, not {repo_label}")
            if not supported_edge(source_ft, target_ft):
                errors.append(f"{jsonl_file}: {source_ft}->{target_ft} grounding is unsupported for prepared compliance seeds")

if errors:
    print("Prepared compliance repos violate grounding ownership:", file=sys.stderr)
    for error in errors[:40]:
        print(f"  - {error}", file=sys.stderr)
    if len(errors) > 40:
        print(f"  ... {len(errors) - 40} more", file=sys.stderr)
    sys.exit(1)

print("✓ Prepared repo ownership validated")
PY
}

if [ "$VALIDATE_PREPARED_OWNERSHIP" = "true" ]; then
  validate_prepared_repo_ownership
else
  echo "✓ Preserving exact source-repo ledger placement; managed routing remains authoritative"
fi

validate_copy_image() {
  if ! docker image inspect "$COPY_IMAGE" >/dev/null 2>&1; then
    echo "✗ Copy image not found: ${COPY_IMAGE}" >&2
    echo "  Build the local g5d base image or set G5_COMPLIANCE_COPY_IMAGE to an image containing git and tar." >&2
    exit 1
  fi

  if ! docker run --rm "$COPY_IMAGE" sh -c 'command -v git >/dev/null && command -v tar >/dev/null' >/dev/null 2>&1; then
    echo "✗ Copy image lacks required commands: ${COPY_IMAGE}" >&2
    echo "  Set G5_COMPLIANCE_COPY_IMAGE to an image that already contains git and tar." >&2
    exit 1
  fi
}

api_health_url() {
  local url="$1"
  curl -sf "${url}/healthz" >/dev/null 2>&1
}

if ! api_health_url "$API_URL"; then
  local_port="${G5_PORT_API:-7500}"
  if api_health_url "http://localhost:${local_port}"; then
    API_URL="http://localhost:${local_port}"
  else
    API_IP=$(docker inspect "$API_CONTAINER" --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
    if [ -n "$API_IP" ] && api_health_url "http://${API_IP}:7500"; then
      API_URL="http://${API_IP}:7500"
    else
      echo "✗ Cannot reach g5-api at ${API_URL}" >&2
      exit 1
    fi
  fi
fi

api_post() {
  local path="$1"
  local data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -d "$data"
}

api_authed_post() {
  local path="$1"
  local data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

api_authed_put() {
  local path="$1"
  local data="$2"
  curl -sf "${API_URL}${path}" \
    -X PUT \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

api_authed_get() {
  local path="$1"
  curl -sf "${API_URL}${path}" \
    -H "Authorization: Bearer ${AUTH_TOKEN}"
}

api_authed_delete() {
  local path="$1"
  curl -sf "${API_URL}${path}" \
    -X DELETE \
    -H "Authorization: Bearer ${AUTH_TOKEN}"
}

psql_exec() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -tAc "$1"
}

psql_exec_quiet() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$1" >/dev/null 2>&1 || true
}

sql_literal() {
  python3 - "$1" <<'PY'
import sys

value = sys.argv[1]
print("'" + value.replace("'", "''") + "'")
PY
}

recreate_blank_seed_workspace() {
  local ws_id="$1" name_sql description_sql
  name_sql="$(sql_literal "$WORKSPACE_NAME")"
  description_sql="$(sql_literal "$WORKSPACE_DESCRIPTION")"
  [ -n "$SEED_USER_ID" ] || { echo 'Fixture owner is required' >&2; exit 1; }
  psql_exec "
    BEGIN;
    INSERT INTO workspaces (id, org_id, name, description, owner_user_id, created_by)
      VALUES ('${ws_id}', '${ORG_ID}', ${name_sql}, ${description_sql}, '${SEED_USER_ID}', '${SEED_USER_ID}');
    INSERT INTO workspace_settings (workspace_id, org_id) VALUES ('${ws_id}', '${ORG_ID}');
    COMMIT;
  " >/dev/null
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "✗ Required command not found: $1" >&2
    exit 1
  fi
}

# Group A disposable set (issue #2189 Q2/D6, seed_scripts D31): upstream-tracked
# build/runtime artifacts governed by a committed ignore rule — CI log dumps
# under outputs/ci-logs/** and the .env* files committed ignore rules actually
# ignore (e.g. .env.tinydb.test). These are excluded from the
# canonical seed restore and from the tree-hash equality scope; untracking them
# from upstream history is separate cleanup.
#
# Membership is the tracked-but-ignored set (`git ls-files -i -c
# --exclude-standard`, which HONORS committed `!` negations) intersected with
# the Group A path classes — NOT a naive `.env*` name glob. A name glob would
# wrongly capture the committed `!`-negated `.env.example` /
# `.env.platform-issue.example` templates, which are re-included, canonical, and
# must survive reseed. Canonical tracked-but-ignored shipped/methodology assets
# (.claude/**, provided_agent_assets/**) are likewise NOT in this set.
list_seed_disposable_paths() {
  local repo_dir="$1"
  {
    git -C "$repo_dir" ls-files -i -c --exclude-standard -- 'outputs/ci-logs' 2>/dev/null || true
    git -C "$repo_dir" ls-files -i -c --exclude-standard 2>/dev/null | grep -E '(^|/)\.env[^/]*$' || true
  } | sort -u
}

# Drop the Group A disposable set from a seed repo's working tree and index so
# it is neither force-restored nor committed into the canonical seed.
remove_seed_disposable_paths() {
  local repo_dir="$1"
  local path
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    # `-f` is required: these paths were just force-staged by `git add -f -A`,
    # so they have changes staged in the index and an unforced `git rm` refuses
    # with "has changes staged in the index". Failures must NOT be swallowed —
    # a silently skipped removal would leave the Group A disposable set inside
    # the staged tree while the canonical hash excludes it, so the seed would
    # either publish disposable artifacts or fail the equality gate with a
    # misleading mismatch (seed_scripts D31 requires failing loudly on drift).
    git -C "$repo_dir" rm -q -f --ignore-unmatch -- "$path" >/dev/null
  done < <(list_seed_disposable_paths "$repo_dir")
}

# Canonical source tree hash: the source commit tree with the Group A
# disposable set removed, so the seed tree-hash equality gate compares the
# canonical scope only (issue #2189 Q2/D6, seed_scripts D31). Drift within the
# canonical set still fails closed; the disposable set is out of scope.
canonical_source_tree_hash() {
  local source_repo="$1"
  local tmp_index path
  tmp_index="$(mktemp)"
  GIT_INDEX_FILE="$tmp_index" git -C "$source_repo" read-tree HEAD
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    # `-f` for the same reason as remove_seed_disposable_paths: the scratch
    # index is seeded from HEAD, so an unforced `rm --cached` can refuse when the
    # worktree differs. Failures are not swallowed: a skipped removal would make
    # the "canonical" hash silently include Group A and defeat the gate.
    GIT_INDEX_FILE="$tmp_index" git -C "$source_repo" rm --cached -q -f --ignore-unmatch -- "$path" >/dev/null
  done < <(list_seed_disposable_paths "$source_repo")
  GIT_INDEX_FILE="$tmp_index" git -C "$source_repo" write-tree
  rm -f "$tmp_index"
}

# D31's tracking-aware staging requirement applies to any copied tree that is
# "already-git-tracked" upstream. That precondition is a property of the SOURCE,
# not of an env flag: a source directory with no git history has no upstream
# tracked set to be faithful to, so plain `git add -A` is the only meaningful
# staging for it. Detect it per source instead of gating the whole behavior on
# G5_COMPLIANCE_VERIFY_SOURCE_TREE (which left the default seed flow using a
# plain `git add -A` with no canonical comparison, violating D31).
source_repo_is_git_tracked() {
  local source_repo="$1"
  git -C "$source_repo" rev-parse --git-dir >/dev/null 2>&1 &&
    git -C "$source_repo" rev-parse --verify 'HEAD^{commit}' >/dev/null 2>&1
}

# D31 stages "the exact set of paths tracked in the upstream source commit (from
# a verified clean clone)" and gates on tree-hash equality against that commit.
# Both properties require the source worktree/index to match HEAD: a dirty
# source would stage uncommitted content that diverges from the canonical HEAD
# tree the gate compares against. Reject it early with an actionable error
# instead of letting it surface later as a confusing tree-hash mismatch.
require_clean_source_repo() {
  local label="$1" source_repo="$2"
  if ! git -C "$source_repo" diff --quiet HEAD -- 2>/dev/null; then
    echo "✗ ${label} source repo has uncommitted changes to tracked files: $source_repo" >&2
    echo "  Tracking-aware seed staging requires a clean source tree so the seed is" >&2
    echo "  faithful to the upstream tracked set (issue #2189 seed_scripts D31)." >&2
    echo "  Commit or stash the tracked changes below, then reseed:" >&2
    git -C "$source_repo" status --porcelain --untracked-files=no -- >&2 || true
    exit 1
  fi
}

# Stage exactly the source repo's upstream tracked set (`git ls-files`) into the
# current seed working tree — NOT a whole-worktree `git add -f -A`. This is D31's
# "stage the exact set of paths tracked in the upstream source commit": a
# whole-worktree force-add would drag ignored UNTRACKED artifacts that the rsync
# copy carried over (build output, local caches, dev-only files) into the seed
# even though they are not part of the canonical upstream tree. `-f` is still
# required so canonical tracked-but-ignored assets (.claude/**,
# provided_agent_assets/**) — which match ignore rules but are tracked upstream —
# are staged rather than silently skipped. The Group A disposable subset is
# removed afterward by remove_seed_disposable_paths. The source is verified clean
# (require_clean_source_repo) before this runs, so each staged path's copied
# content equals its committed blob and the staged tree equals the canonical
# source tree (Group A excluded on both sides). NUL-delimited so paths with
# spaces or newlines are handled safely.
stage_upstream_tracked_set() {
  local source_repo="$1"
  git -C "$source_repo" ls-files -z | xargs -0 -r git add -f --
}

init_local_seed_repo() {
  local repo_dir="$1"
  local commit_message="$2"
  # "true" when the copied tree came from an already-git-tracked source, so
  # D31 tracking-aware staging applies.
  local tracking_aware="${3:-false}"
  # The already-git-tracked source repo the copy came from; required when
  # tracking_aware=true so staging can read its upstream tracked set.
  local source_repo="${4:-}"

  (
    cd "$repo_dir"
    if ! git init -q -b "$BRANCH_NAME" >/dev/null 2>&1; then
      git init -q
      git checkout -q -B "$BRANCH_NAME"
    fi
    git config user.email "$AUTH_EMAIL"
    git config user.name "G5 Workspace Seed"
    git config gc.auto 0
    git config maintenance.auto false
    if [ "$tracking_aware" = "true" ]; then
      # Restore canonical assets only (issue #2189 Q2/D6, seed_scripts D31):
      # stage the upstream tracked set (NOT a whole-worktree `git add -f -A`) so
      # canonical tracked-but-ignored shipped/methodology assets (.claude/**,
      # provided_agent_assets/**) survive reseed while ignored UNTRACKED
      # artifacts never enter the seed, then drop the disposable Group A set so
      # it is neither force-restored nor part of the tree-hash-equality scope.
      # This is the DEFAULT for any already-git-tracked source, not an opt-in.
      if [ -z "$source_repo" ]; then
        echo "✗ init_local_seed_repo: tracking-aware staging requires a source repo path" >&2
        exit 1
      fi
      stage_upstream_tracked_set "$source_repo"
      remove_seed_disposable_paths .
    else
      # Source has no upstream git history, so there is no tracked set to
      # preserve and D31 does not apply.
      git add -A
    fi
    if git diff --cached --quiet; then
      echo "seed marker" > .g5-seed-marker
      git add .g5-seed-marker
    fi
    git commit -q -m "$commit_message"
  )
}

materialized_head_for_branch() {
  local repo_id="$1"
  api_authed_get "/api/v1/workspaces/${WS_ID}/branches" | jq -er --arg repo "$repo_id" '.data[] | select(.repoId == $repo) | .headCommit'
}

wait_for_materialized_branch_head() {
  local repo_id="$1"
  local label="$2"
  local head=""

  for _attempt in $(seq 1 20); do
    head="$(materialized_head_for_branch "$repo_id")"
    if [[ "$head" =~ ^[0-9a-f]{40}$ ]] && [ "$head" != "0000000000000000000000000000000000000000" ]; then
      printf '%s\n' "$head"
      return 0
    fi
    sleep 1
  done

  echo "✗ ${label} branch did not materialize after publish (head=${head:-missing})" >&2
  exit 1
}

verify_managed_repo_symbolic_head() {
  local repo_id="$1"
  local label="$2"
  local expected_branch="${3:-}"
  local head_ref

  head_ref="$(docker exec "$API_CONTAINER" git -C "/data/repos/${ORG_ID}/${repo_id}" symbolic-ref HEAD 2>/dev/null || true)"
  if [ -z "$head_ref" ]; then
    echo "✗ Managed ${label} repo ${repo_id} has no symbolic HEAD" >&2
    exit 1
  fi

  if ! docker exec "$API_CONTAINER" git -C "/data/repos/${ORG_ID}/${repo_id}" rev-parse --verify "${head_ref}^{commit}" >/dev/null 2>&1; then
    echo "✗ Managed ${label} repo ${repo_id} HEAD points at unresolved ${head_ref}" >&2
    exit 1
  fi

  if [ -n "$expected_branch" ] && [ "$head_ref" != "refs/heads/${expected_branch}" ]; then
    echo "✗ Managed ${label} repo ${repo_id} HEAD is ${head_ref}, expected refs/heads/${expected_branch}" >&2
    exit 1
  fi

  echo "✓ Managed ${label} repo HEAD: ${head_ref}"
}

repair_unresolved_managed_repo_head() {
  local repo_id="$1"
  local label="$2"
  local expected_branch="$3"
  local repo_path="/data/repos/${ORG_ID}/${repo_id}"

  if docker exec "$API_CONTAINER" git -C "$repo_path" rev-parse --verify HEAD^{commit} >/dev/null 2>&1; then
    return 0
  fi
  if ! docker exec "$API_CONTAINER" git -C "$repo_path" rev-parse --verify "refs/heads/${expected_branch}^{commit}" >/dev/null 2>&1; then
    echo "✗ Cannot repair ${label} repo HEAD: refs/heads/${expected_branch} is unresolved" >&2
    exit 1
  fi
  docker exec "$API_CONTAINER" git -C "$repo_path" symbolic-ref HEAD "refs/heads/${expected_branch}"
  echo "✓ Repaired unresolved ${label} repo HEAD to refs/heads/${expected_branch}"
}

verify_seeded_document_references() {
  local docs link_count required_doc_link_count
  docs=$(api_authed_get "/api/v1/workspaces/${WS_ID}/docs" | jq -ce \
    '.data.docs | select(type == "array" and all(.[]; (.linkedNodeCount | type == "number" and . >= 0)))')
  link_count=$(printf '%s' "$docs" | jq '[.[].linkedNodeCount] | add // 0')
  if [ "$link_count" -eq 0 ]; then
    echo "✗ Seeded documents have no resolvable file references" >&2
    exit 1
  fi
  if [ -n "$REQUIRED_DOC_LINK_PATH" ]; then
    required_doc_link_count=$(printf '%s' "$docs" | jq --arg path "$REQUIRED_DOC_LINK_PATH" \
      '[.[] | select(.filePath == $path) | .linkedNodeCount] | add // 0')
    if [ "$required_doc_link_count" -lt "${REQUIRED_DOC_LINK_MIN:-1}" ]; then
      echo "✗ Required document references incomplete for ${REQUIRED_DOC_LINK_PATH}: ${required_doc_link_count}/${REQUIRED_DOC_LINK_MIN:-1}" >&2
      exit 1
    fi
  fi
  echo "✓ Document file references resolved: ${link_count}"
}

validate_seed_spec_files() {
  local spec_paths spec_count component
  spec_paths=$(docker exec "$API_CONTAINER" git -C "/data/repos/${ORG_ID}/${CODE_REPO_ID}" \
    ls-tree -r --name-only "$CODE_HEAD" -- .semlift/specs/)
  spec_count=$(printf '%s\n' "$spec_paths" | awk '/\.semlift\.md$/ { count++ } END { print count+0 }')
  [ "$spec_count" -gt 0 ] || { echo "✗ Seeded code commit has no SemLift files" >&2; exit 1; }
  for component in "$EXPECTED_ROOT_COMPONENT" "$EXPECTED_CHILD_COMPONENT"; do
    [ -n "$component" ] || continue
    if ! docker exec "$API_CONTAINER" git -C "/data/repos/${ORG_ID}/${CODE_REPO_ID}" \
      cat-file -e "${CODE_HEAD}:.semlift/specs/${component}.semlift.md"; then
      echo "✗ Seeded code commit lacks the expected spec file: ${component}" >&2
      exit 1
    fi
  done
  echo "✓ Committed SemLift files verified: ${spec_count}"
}

cleanup_previous_seeded_workspace() {
  local old_ws_id old_repo_ids repo_id repo_slug orphan_slug orphan_repo_id

  REUSE_WS_ID=""
  old_ws_id="$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WORKSPACE_NAME}' LIMIT 1;" | tr -d '[:space:]')"
  if [ -n "$old_ws_id" ]; then
    echo "Found existing workspace: $old_ws_id"
    old_repo_ids="$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE workspace_id = '${old_ws_id}';" | tr -d '\r')"
    psql_exec_quiet "DELETE FROM session_inputs WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM provider_requests WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${old_ws_id}');"
    psql_exec_quiet "DELETE FROM task_dependencies WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM task_attempts WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${old_ws_id}');"
    psql_exec_quiet "DELETE FROM timeline_entries WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${old_ws_id}');"
    psql_exec_quiet "DELETE FROM tasks WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM chat_sessions WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM docs WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM workspace_repos WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM workspace_settings WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM workspace_forks WHERE source_workspace_id = '${old_ws_id}' OR target_workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM workspace_ownership_transfers WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM worker_vm_events WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM workspace_vms WHERE workspace_id = '${old_ws_id}';"
    psql_exec_quiet "DELETE FROM workspace_flushes WHERE workspace_id = '${old_ws_id}';"
    psql_exec "DELETE FROM workspaces WHERE id = '${old_ws_id}';" >/dev/null
    seed_remove_fixture_parent "$old_ws_id"
    recreate_blank_seed_workspace "$old_ws_id"
    REUSE_WS_ID="$old_ws_id"

    for repo_id in $old_repo_ids; do
      [ -z "$repo_id" ] && continue
      repo_slug="$(psql_exec "SELECT slug FROM repos WHERE id = '${repo_id}';" | tr -d '[:space:]')"
      if { [ "$repo_slug" = "$DOCS_REPO_SLUG" ] && [ "$repo_slug" != "docs" ]; } || \
         [ "$repo_slug" = "$CODE_REPO_SLUG" ] || \
         [ "$repo_slug" = "$LEGACY_REPO_SLUG" ]; then
        psql_exec_quiet "DELETE FROM repos WHERE id = '${repo_id}';"
        docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${repo_id}" >/dev/null 2>&1 || true
        docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${repo_id}.bare" >/dev/null 2>&1 || true
      fi
    done
    echo "✓ Previous workspace data cleared; preserving workspace ID ${REUSE_WS_ID}"
  else
    echo "No previous seeded workspace found"
  fi

  for orphan_slug in "$DOCS_REPO_SLUG" "$CODE_REPO_SLUG" "$LEGACY_REPO_SLUG"; do
    if [ "$orphan_slug" = "docs" ]; then
      continue
    fi
    orphan_repo_id="$(psql_exec "SELECT id::text FROM repos WHERE org_id = '${ORG_ID}' AND slug = '${orphan_slug}' LIMIT 1;" | tr -d '[:space:]')"
    if [ -n "$orphan_repo_id" ]; then
      psql_exec_quiet "DELETE FROM workspace_repos WHERE repo_id = '${orphan_repo_id}';"
      psql_exec_quiet "DELETE FROM repos WHERE id = '${orphan_repo_id}';"
      docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${orphan_repo_id}" >/dev/null 2>&1 || true
      docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${orphan_repo_id}.bare" >/dev/null 2>&1 || true
      echo "✓ Orphaned repo removed for slug ${orphan_slug}"
    fi
  done
}

seed_with_native_publish() {
  local auth_resp orgs_resp host_org_id user_id
  local seed_workspace_basename seed_workspace_dir local_docs_repo local_code_repo
  local docs_head code_head docs_branch_name docs_repo_name_actual docs_repo_slug_actual
  local staged_docs_tree managed_docs_tree

  require_command curl
  require_command jq
  require_command git
  require_command rsync

  echo "=== ${SEED_LABEL} ==="
  echo "  Backend:     native Git publish"
  echo "  Docs source: $DOCS_SOURCE_REPO"
  echo "  Code source: $CODE_SOURCE_REPO"
  echo "  API:         $API_URL"
  if [ "$ALLOW_CHAT_TARGET_GROUNDING" != "false" ]; then
    echo "  Chat grounding: code/ontology targets allowed"
  fi
  echo ""

  echo "=== Step 1: Authenticate for seed setup ==="
  auth_resp="$(api_post "/api/v1/auth/test-token" "{\"email\": \"${AUTH_EMAIL}\"}" 2>/dev/null || echo "")"
  AUTH_TOKEN="$(echo "$auth_resp" | jq -r '.data.jwt // .jwt // empty' 2>/dev/null)"
  if [ -z "$AUTH_TOKEN" ]; then
    echo "✗ Failed to get auth token" >&2
    echo "Response: $auth_resp" >&2
    exit 1
  fi
  echo "✓ Authenticated as ${AUTH_EMAIL}"

  echo ""
  echo "=== Step 2: Resolve org ==="
  host_org_id="$(psql_exec "SELECT org_id::text FROM worker_hosts WHERE status = 'ready' ORDER BY last_heartbeat DESC NULLS LAST LIMIT 1;" 2>/dev/null | tr -d '[:space:]')"
  orgs_resp="$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")"
  if [ -n "$host_org_id" ]; then
    ORG_ID="$host_org_id"
    user_id="$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]')"
    if [ -n "$user_id" ]; then
      SEED_USER_ID="$user_id"
      psql_exec_quiet "INSERT INTO org_memberships (org_id, user_id, role) VALUES ('${ORG_ID}', '${user_id}', 'admin') ON CONFLICT (org_id, user_id) DO NOTHING;"
    fi
  else
    ORG_ID="$(echo "$orgs_resp" | jq -r '.data[0].orgId // empty' 2>/dev/null)"
  fi
  if [ -z "$ORG_ID" ]; then
    echo "✗ No org found" >&2
    exit 1
  fi
  if [ -z "$SEED_USER_ID" ]; then
    SEED_USER_ID="$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]' || true)"
  fi
  echo "✓ Org: $ORG_ID"

  echo ""
  echo "=== Step 3: Cleanup previous seeded workspace ==="
  cleanup_previous_seeded_workspace

  echo ""
  echo "=== Step 4: Create or reuse blank target workspace ==="
  if [ -n "$REUSE_WS_ID" ]; then
    WS_ID="$REUSE_WS_ID"
  else
    WS_ID=$(seed_create_fixture_workspace "$WORKSPACE_NAME")
  fi
  echo "✓ Workspace: $WS_ID"
  DOCS_REPO_ID=$(seed_add_fixture_repo "$DOCS_REPO_NAME" "$DOCS_REPO_SLUG" docs)
  CODE_REPO_ID=$(seed_add_fixture_repo "$CODE_REPO_NAME" "$CODE_REPO_SLUG" code)

  echo ""
  echo "=== Step 5: Stage local seed repos ==="
  seed_workspace_basename="${WORKSPACE_NAME//[^A-Za-z0-9._-]/-}"
  seed_workspace_dir="${G5_COMPLIANCE_LOCAL_WORKSPACE_DIR:-$GEN_ROOT/.tmp/seed-workspaces/$seed_workspace_basename}"
  case "$seed_workspace_dir" in
    "/"|"$GEN_ROOT"|"$WORKSPACE_ROOT")
      echo "✗ Refusing to replace unsafe seed workspace path: $seed_workspace_dir" >&2
      exit 1
      ;;
  esac
  rm -rf "$seed_workspace_dir"
  mkdir -p "$seed_workspace_dir/repos"
  local_docs_repo="$seed_workspace_dir/repos/$DOCS_REPO_NAME"
  local_code_repo="$seed_workspace_dir/repos/$CODE_REPO_NAME"
  mkdir -p "$local_docs_repo" "$local_code_repo"
  rsync -a --delete --exclude '.git/' "$DOCS_SOURCE_REPO"/ "$local_docs_repo"/
  rsync -a --delete --exclude '.git/' "$CODE_SOURCE_REPO"/ "$local_code_repo"/
  if [ "$VERIFY_SOURCE_TREE" != "true" ] && [ "$VERIFY_SOURCE_TREE" != "false" ]; then
    echo "✗ G5_COMPLIANCE_VERIFY_SOURCE_TREE must be true or false" >&2
    exit 1
  fi

  # D31 tracking-aware staging + canonical-scope verification run by DEFAULT for
  # every already-git-tracked source, not only under
  # G5_COMPLIANCE_VERIFY_SOURCE_TREE=true. The flag now only ESCALATES the
  # precondition: it additionally requires both sources to be git-tracked, so a
  # self-hosting seed cannot silently degrade to plain staging (D11).
  local docs_tracking_aware="false"
  local code_tracking_aware="false"
  if source_repo_is_git_tracked "$DOCS_SOURCE_REPO"; then
    docs_tracking_aware="true"
  fi
  if source_repo_is_git_tracked "$CODE_SOURCE_REPO"; then
    code_tracking_aware="true"
  fi
  if [ "$VERIFY_SOURCE_TREE" = "true" ] &&
    { [ "$docs_tracking_aware" != "true" ] || [ "$code_tracking_aware" != "true" ]; }; then
    echo "✗ G5_COMPLIANCE_VERIFY_SOURCE_TREE=true requires git-tracked source repos" >&2
    echo "  Docs git-tracked: ${docs_tracking_aware} ($DOCS_SOURCE_REPO)" >&2
    echo "  Code git-tracked: ${code_tracking_aware} ($CODE_SOURCE_REPO)" >&2
    exit 1
  fi

  # A tracking-aware seed stages the source's committed tracked set and gates on
  # equality against its HEAD tree, so a dirty source must be rejected early
  # (seed_scripts D31 "from a verified clean clone").
  if [ "$docs_tracking_aware" = "true" ]; then
    require_clean_source_repo "Docs" "$DOCS_SOURCE_REPO"
  fi
  if [ "$code_tracking_aware" = "true" ]; then
    require_clean_source_repo "Code" "$CODE_SOURCE_REPO"
  fi

  init_local_seed_repo "$local_docs_repo" "seed: stage ${WORKSPACE_NAME} docs" \
    "$docs_tracking_aware" "$DOCS_SOURCE_REPO"
  init_local_seed_repo "$local_code_repo" "seed: stage ${WORKSPACE_NAME} code" \
    "$code_tracking_aware" "$CODE_SOURCE_REPO"
  docs_head="$(git -C "$local_docs_repo" rev-parse HEAD)"
  code_head="$(git -C "$local_code_repo" rev-parse HEAD)"

  # Canonical-scope equality gate: compare against the source commit tree with
  # the Group A disposable set removed, so canonical tracked-but-ignored assets
  # must match exactly while Group A stays out of the equality scope (issue
  # #2189 Q2/D6, seed_scripts D31). Runs per repo whenever that repo's source is
  # git-tracked — the default flow included.
  verify_canonical_seed_tree() {
    local label="$1" source_repo="$2" staged_repo="$3" tracking_aware="$4"
    if [ "$tracking_aware" != "true" ]; then
      echo "• ${label}: source has no git history; canonical tree-hash gate not applicable"
      return 0
    fi
    local source_tree staged_tree
    source_tree="$(canonical_source_tree_hash "$source_repo")"
    staged_tree="$(git -C "$staged_repo" rev-parse 'HEAD^{tree}')"
    if [ "$source_tree" != "$staged_tree" ]; then
      echo "✗ Staged seed trees do not match their selected source commits (canonical scope)" >&2
      echo "  ${label}: source=${source_tree} staged=${staged_tree}" >&2
      exit 1
    fi
    echo "✓ ${label}: staged Git tree matches the selected source commit (canonical scope)"
  }
  verify_canonical_seed_tree "Docs" "$DOCS_SOURCE_REPO" "$local_docs_repo" \
    "$docs_tracking_aware"
  verify_canonical_seed_tree "Code" "$CODE_SOURCE_REPO" "$local_code_repo" \
    "$code_tracking_aware"

  echo "✓ Local seed workspace: $seed_workspace_dir"
  echo "✓ Local docs HEAD: $docs_head"
  echo "✓ Local code HEAD: $code_head"

  echo ""
  echo "=== Step 6: Install managed repos and publish ==="
  seed_install_fixture_repo "$DOCS_REPO_ID" "$local_docs_repo"
  seed_install_fixture_repo "$CODE_REPO_ID" "$local_code_repo"
  seed_capture_fixture_parent "$WS_ID" "$WORKSPACE_NAME"
  # The installed repos already carry the staged heads on refs/heads/<name>,
  # so the plan verifies both members against the mirror and the apply moves
  # nothing; file readers select those published commits directly.
  native_publish_configure "$API_URL" "$AUTH_TOKEN"
  native_publish_plan_reset
  native_publish_plan "docs" "$local_docs_repo" "$DOCS_REPO_ID" "$BRANCH_NAME"
  native_publish_plan "code" "$local_code_repo" "$CODE_REPO_ID" "$BRANCH_NAME"
  native_publish_apply

  echo ""
  echo "=== Step 7: Verify remote branch heads ==="
  DOCS_REPO_ID="$(psql_exec "SELECT docs_repo_id::text FROM workspaces WHERE id = '${WS_ID}';" | tr -d '[:space:]')"
  docs_branch_name="$(psql_exec "SELECT name FROM workspaces WHERE id = '${WS_ID}';" | tr -d '[:space:]')"
  if [ -z "$DOCS_REPO_ID" ] || [ -z "$docs_branch_name" ]; then
    echo "✗ Failed to resolve seeded docs repo metadata" >&2
    exit 1
  fi
  CODE_BRANCH_NAME="$WORKSPACE_NAME"
  DOCS_HEAD="$(wait_for_materialized_branch_head "$DOCS_REPO_ID" "docs")"
  CODE_HEAD="$(wait_for_materialized_branch_head "$CODE_REPO_ID" "code")"
  if [ "$VERIFY_SOURCE_TREE" = "true" ]; then
    staged_docs_tree="$(git -C "$local_docs_repo" rev-parse "${docs_head}^{tree}")"
    managed_docs_tree="$(docker exec "$API_CONTAINER" git -C "/data/repos/${ORG_ID}/${DOCS_REPO_ID}" rev-parse "${DOCS_HEAD}^{tree}" 2>/dev/null || true)"
    if [ -z "$managed_docs_tree" ] || [ "$managed_docs_tree" != "$staged_docs_tree" ] || [ "$CODE_HEAD" != "$code_head" ]; then
      echo "✗ Managed repos do not match the source-tree-verified staged cut" >&2
      echo "  Docs tree: staged=${staged_docs_tree} managed=${managed_docs_tree:-unresolved}" >&2
      echo "  Code: staged=${code_head} managed=${CODE_HEAD}" >&2
      exit 1
    fi
    echo "✓ Managed docs tree matches staged source; managed code head matches staged commit"
  fi
  repair_unresolved_managed_repo_head "$CODE_REPO_ID" "code" "$CODE_BRANCH_NAME"
  verify_managed_repo_symbolic_head "$DOCS_REPO_ID" "docs"
  verify_managed_repo_symbolic_head "$CODE_REPO_ID" "code" "$CODE_BRANCH_NAME"
  docs_repo_name_actual="$(psql_exec "SELECT name FROM repos WHERE id = '${DOCS_REPO_ID}';" | tr -d '[:space:]')"
  docs_repo_slug_actual="$(psql_exec "SELECT slug FROM repos WHERE id = '${DOCS_REPO_ID}';" | tr -d '[:space:]')"
  echo "✓ Remote docs repo: ${DOCS_REPO_ID} (${docs_repo_name_actual}/${docs_repo_slug_actual}, branch ${docs_branch_name}, HEAD ${DOCS_HEAD})"
  echo "✓ Remote code repo: ${CODE_REPO_ID} (HEAD ${CODE_HEAD})"

  echo ""
  echo "=== Step 8: Verify committed specs and document references ==="
  validate_seed_spec_files
  verify_seeded_document_references

  cat > "$ENV_FILE" <<EOF
export G5_COMPLIANCE_WORKSPACE_ID=${WS_ID}
export G5_COMPLIANCE_DOCS_REPO_ID=${DOCS_REPO_ID}
export G5_COMPLIANCE_DOCS_HEAD_COMMIT=${DOCS_HEAD}
export G5_COMPLIANCE_CODE_REPO_ID=${CODE_REPO_ID}
export G5_COMPLIANCE_CODE_HEAD_COMMIT=${CODE_HEAD}
export G5_COMPLIANCE_WORKSPACE_NAME=${WORKSPACE_NAME}
export G5_COMPLIANCE_LOCAL_WORKSPACE_DIR=${seed_workspace_dir}
EOF

  echo ""
  echo "Seed complete."
  echo "  Workspace ID: $WS_ID"
  echo "  Docs repo:    $DOCS_REPO_ID"
  echo "  Docs HEAD:    $DOCS_HEAD"
  echo "  Code repo:    $CODE_REPO_ID"
  echo "  Code HEAD:    $CODE_HEAD"
  echo "  Local seed:   $seed_workspace_dir"
  echo "  Env file:     $ENV_FILE"
}

case "${G5_SEED_WORKSPACE_BACKEND:-native}" in
  native|g5d-push)
    seed_with_native_publish
    exit 0
    ;;
  legacy-db)
    validate_copy_image
    ;;
  *)
    echo "✗ Unsupported G5_SEED_WORKSPACE_BACKEND='${G5_SEED_WORKSPACE_BACKEND}' (expected native or legacy-db)" >&2
    exit 1
    ;;
esac


copy_repo_to_volume() {
  local source_dir="$1"
  local repo_id="$2"
  local commit_message="$3"

  docker run --rm \
    --user 0:0 \
    -v "${REPOS_VOLUME}:/data/repos" \
    -v "${source_dir}:/source:ro" \
    "${COPY_IMAGE}" sh -c "
      set -e
      BASE=/data/repos/${ORG_ID}/${repo_id}
      BARE=/data/repos/${ORG_ID}/${repo_id}.bare
      chmod_repo_tree() {
        target=\"\$1\"
        [ -e \"\$target\" ] || return 0
        if find \"\$target\" -ignore_readdir_race -exec chmod a+rwX {} + >/dev/null 2>&1; then
          return 0
        fi
        if ! chmod -R a+rwX \"\$target\" 2>/tmp/g5-seed-chmod.err; then
          if grep -v 'No such file or directory' /tmp/g5-seed-chmod.err >&2; then
            return 1
          fi
        fi
        rm -f /tmp/g5-seed-chmod.err
      }
      rm -rf \"\$BASE\" \"\$BARE\"
      mkdir -p \"\$BASE\" \"\$BARE\"
      cd /source
      tar cf - --exclude=.git . | tar xf - -C \"\$BASE\"
      git init --bare -q \"\$BARE\"
      git --git-dir=\"\$BARE\" config gc.auto 0
      git --git-dir=\"\$BARE\" config maintenance.auto false
      cd \"\$BASE\"
      git config --global --add safe.directory '*'
      git init -q -b ${BRANCH_NAME}
      git config gc.auto 0
      git config maintenance.auto false
      git config user.email '${AUTH_EMAIL}'
      git config user.name 'G5 Compliance Seed'
      git add .
      git commit -q -m '${commit_message}'
      git remote add origin \"\$BARE\"
      git push -q -u origin HEAD:${BRANCH_NAME}
      git --git-dir=\"\$BARE\" symbolic-ref HEAD refs/heads/${BRANCH_NAME}
      chmod_repo_tree \"\$BASE\"
      chmod_repo_tree \"\$BARE\"
      git rev-parse HEAD
    " | tail -n 1 | tr -d '[:space:]'
}

verify_seeded_repo_head() {
  local repo_id="$1"
  local label="$2"
  local expected_head="$3"
  local suffix="${4:-}"
  local repo_path="/data/repos/${ORG_ID}/${repo_id}${suffix}"
  local head_ref resolved_head

  head_ref="$(docker exec "$API_CONTAINER" git -C "$repo_path" symbolic-ref HEAD 2>/dev/null || true)"
  if [ "$head_ref" != "refs/heads/${BRANCH_NAME}" ]; then
    echo "✗ Managed ${label}${suffix} HEAD is '${head_ref:-unborn}', expected refs/heads/${BRANCH_NAME}" >&2
    exit 1
  fi

  resolved_head="$(docker exec "$API_CONTAINER" git -C "$repo_path" rev-parse --verify HEAD^{commit} 2>/dev/null || true)"
  if [ "$resolved_head" != "$expected_head" ]; then
    echo "✗ Managed ${label}${suffix} HEAD resolves to '${resolved_head:-unresolved}', expected ${expected_head}" >&2
    exit 1
  fi

  echo "✓ Managed ${label}${suffix} HEAD: ${head_ref} (${resolved_head})"
}

echo "=== ${SEED_LABEL} ==="
echo "  Docs repo:   $DOCS_SOURCE_REPO"
echo "  Code repo:   $CODE_SOURCE_REPO"
echo "  API:         $API_URL"
if [ "$ALLOW_CHAT_TARGET_GROUNDING" != "false" ]; then
  echo "  Chat grounding: code/ontology targets allowed"
fi
echo ""

echo "=== Step 1: Authenticate ==="
AUTH_RESP=$(api_post "/api/v1/auth/test-token" "{\"email\": \"${AUTH_EMAIL}\"}" 2>/dev/null || echo "")
AUTH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty' 2>/dev/null)
if [ -z "$AUTH_TOKEN" ]; then
  echo "✗ Failed to get auth token" >&2
  echo "Response: $AUTH_RESP" >&2
  exit 1
fi
echo "✓ Authenticated as ${AUTH_EMAIL}"

echo ""
echo "=== Step 2: Resolve org ==="
HOST_ORG_ID=$(psql_exec "SELECT org_id::text FROM worker_hosts WHERE status = 'ready' ORDER BY last_heartbeat DESC NULLS LAST LIMIT 1;" 2>/dev/null | tr -d '[:space:]')
ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")
if [ -n "$HOST_ORG_ID" ]; then
  ORG_ID="$HOST_ORG_ID"
  USER_ID=$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]')
  if [ -n "$USER_ID" ]; then
    SEED_USER_ID="$USER_ID"
    psql_exec_quiet "INSERT INTO org_memberships (org_id, user_id, role) VALUES ('${ORG_ID}', '${USER_ID}', 'admin') ON CONFLICT (org_id, user_id) DO NOTHING;"
  fi
else
  ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)
fi
if [ -z "$ORG_ID" ]; then
  echo "✗ No org found" >&2
  exit 1
fi
if [ -z "$SEED_USER_ID" ]; then
  SEED_USER_ID="$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]' || true)"
fi
echo "✓ Org: $ORG_ID"

echo ""
echo "=== Step 3: Cleanup previous seeded workspace ==="
REUSE_WS_ID=""
OLD_WS_ID=$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WORKSPACE_NAME}' LIMIT 1;" | tr -d '[:space:]')
if [ -n "$OLD_WS_ID" ]; then
  echo "Found existing workspace: $OLD_WS_ID"
  OLD_REPO_IDS=$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}';" | tr -d '\r')
  psql_exec_quiet "DELETE FROM session_inputs WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM provider_requests WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM task_dependencies WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM task_attempts WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM timeline_entries WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM tasks WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM chat_sessions WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM docs WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_settings WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_forks WHERE source_workspace_id = '${OLD_WS_ID}' OR target_workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_ownership_transfers WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM worker_vm_events WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_vms WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_flushes WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec "DELETE FROM workspaces WHERE id = '${OLD_WS_ID}';" >/dev/null
    seed_remove_fixture_parent "$OLD_WS_ID"
  recreate_blank_seed_workspace "$OLD_WS_ID"
  REUSE_WS_ID="$OLD_WS_ID"

  for repo_id in $OLD_REPO_IDS; do
    [ -z "$repo_id" ] && continue
    repo_slug=$(psql_exec "SELECT slug FROM repos WHERE id = '${repo_id}';" | tr -d '[:space:]')
    if [ "$repo_slug" = "$DOCS_REPO_SLUG" ] || [ "$repo_slug" = "$CODE_REPO_SLUG" ] || [ "$repo_slug" = "$LEGACY_REPO_SLUG" ]; then
      psql_exec_quiet "DELETE FROM repos WHERE id = '${repo_id}';"
      docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${repo_id}" >/dev/null 2>&1 || true
      docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${repo_id}.bare" >/dev/null 2>&1 || true
    fi
  done
  echo "✓ Previous workspace data cleared; preserving workspace ID ${REUSE_WS_ID}"
else
  echo "No previous seeded workspace found"
fi

for orphan_slug in "$DOCS_REPO_SLUG" "$CODE_REPO_SLUG" "$LEGACY_REPO_SLUG"; do
  ORPHAN_REPO_ID=$(psql_exec "SELECT id::text FROM repos WHERE org_id = '${ORG_ID}' AND slug = '${orphan_slug}' LIMIT 1;" | tr -d '[:space:]')
  if [ -n "$ORPHAN_REPO_ID" ]; then
    psql_exec_quiet "DELETE FROM workspace_repos WHERE repo_id = '${ORPHAN_REPO_ID}';"
    psql_exec_quiet "DELETE FROM repos WHERE id = '${ORPHAN_REPO_ID}';"
    docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${ORPHAN_REPO_ID}" >/dev/null 2>&1 || true
    docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine rm -rf "/data/repos/${ORG_ID}/${ORPHAN_REPO_ID}.bare" >/dev/null 2>&1 || true
    echo "✓ Orphaned repo removed for slug ${orphan_slug}"
  fi
done

echo ""
echo "=== Step 4: Create or reuse workspace ==="
if [ -n "$REUSE_WS_ID" ]; then
  WS_ID="$REUSE_WS_ID"
else
  WS_ID=$(seed_create_fixture_workspace "$WORKSPACE_NAME")
fi
echo "✓ Workspace: $WS_ID"

echo ""
echo "=== Step 5: Create dedicated repos and branches ==="
DOCS_REPO_ID=$(seed_add_fixture_repo "$DOCS_REPO_NAME" "$DOCS_REPO_SLUG" docs)
CODE_REPO_ID=$(seed_add_fixture_repo "$CODE_REPO_NAME" "$CODE_REPO_SLUG" code)
echo "✓ Docs repo: $DOCS_REPO_ID"
echo "✓ Code repo: $CODE_REPO_ID"

echo ""
echo "=== Step 6: Copy prepared repos into Docker volume ==="
DOCS_HEAD=$(copy_repo_to_volume "$DOCS_SOURCE_REPO" "$DOCS_REPO_ID" "Initial compliance docs repo")
CODE_HEAD=$(copy_repo_to_volume "$CODE_SOURCE_REPO" "$CODE_REPO_ID" "Initial compliance code repo")

if [ -z "$DOCS_HEAD" ] || [ -z "$CODE_HEAD" ]; then
  echo "✗ Failed to initialize one or more repos in Docker volume" >&2
  exit 1
fi
echo "✓ Docs HEAD: $DOCS_HEAD"
echo "✓ Code HEAD: $CODE_HEAD"
verify_seeded_repo_head "$DOCS_REPO_ID" "docs repo" "$DOCS_HEAD"
verify_seeded_repo_head "$DOCS_REPO_ID" "docs repo" "$DOCS_HEAD" ".bare"
verify_seeded_repo_head "$CODE_REPO_ID" "code repo" "$CODE_HEAD"
verify_seeded_repo_head "$CODE_REPO_ID" "code repo" "$CODE_HEAD" ".bare"

echo ""
echo "=== Step 7: Sync branch heads and workspace metadata ==="
seed_capture_fixture_parent "$WS_ID" "$WORKSPACE_NAME"
echo "✓ Workspace metadata synchronized"

echo ""
echo "=== Step 8: Seed document metadata ==="
SQL_FILE=$(mktemp)
python3 - "$DOCS_SOURCE_REPO" "$CODE_SOURCE_REPO" "$ORG_ID" "$WS_ID" "$DOCS_REPO_ID" "$CODE_REPO_ID" "$DOCS_HEAD" "$CODE_HEAD" > "$SQL_FILE" <<'PY'
import re
import sys
import uuid
from pathlib import Path

docs_root = Path(sys.argv[1])
code_root = Path(sys.argv[2])
org_id = sys.argv[3]
workspace_id = sys.argv[4]
docs_repo_id = sys.argv[5]
code_repo_id = sys.argv[6]
docs_head = sys.argv[7]
code_head = sys.argv[8]

NS = uuid.UUID("0b670ad0-e3c5-41f6-b3e5-3f1ec9e0b6fe")

def parse_scalar(value: str):
  value = value.strip()
  if not value:
    return ""
  if value == "[]":
    return []
  if value.lower() == "null":
    return None
  if value.lower() == "true":
    return True
  if value.lower() == "false":
    return False
  if (value.startswith('"') and value.endswith('"')) or (value.startswith("'") and value.endswith("'")):
    return value[1:-1]
  return value

def parse_frontmatter(path: Path):
  text = path.read_text(encoding="utf-8")
  if not text.startswith("---\n"):
    return {}
  parts = text.split("\n---\n", 1)
  if len(parts) != 2:
    return {}
  lines = parts[0][4:].splitlines()
  data = {}
  i = 0
  while i < len(lines):
    line = lines[i]
    if not line.strip() or line.lstrip().startswith("#"):
      i += 1
      continue
    m = re.match(r"^([A-Za-z0-9_]+):\s*(.*)$", line)
    if not m:
      i += 1
      continue
    key, rest = m.group(1), m.group(2)
    if rest == "":
      items = []
      i += 1
      while i < len(lines):
        nxt = lines[i]
        if re.match(r"^[A-Za-z0-9_]+:\s*", nxt):
          break
        stripped = nxt.strip()
        if stripped.startswith("- "):
          item = stripped[2:]
          if " #" in item:
            item = item.split(" #", 1)[0]
          item = parse_scalar(item)
          if item not in (None, ""):
            items.append(item)
        i += 1
      data[key] = items
      continue
    data[key] = parse_scalar(rest)
    i += 1
  return data

def sql_string(value: str) -> str:
  return "'" + value.replace("'", "''") + "'"

def infer_doc_type(path: Path):
  name = path.name
  suffix_map = {
    '.pm_view.md': 'pm_view',
    '.arch_view.md': 'arch_view',
    '.security_view.md': 'security_view',
    '.performance_view.md': 'performance_view',
    '.test_view.md': 'test_view',
    '.ui_view.md': 'ui_view',
  }
  for suffix, doc_type in suffix_map.items():
    if name.endswith(suffix):
      return doc_type
  return 'note'

STANDARD_PROJECT_DOC_TYPES = {'spec', 'design', 'api', 'guide', 'adr', 'note'}

def infer_project_doc_type(path: Path, front: dict):
  raw_type = front.get('doc_type')
  if isinstance(raw_type, str):
    doc_type = raw_type.strip().lower()
    if doc_type in STANDARD_PROJECT_DOC_TYPES:
      return doc_type

  name = path.name.lower()
  if 'adr' in name:
    return 'adr'
  if 'api' in name:
    return 'api'
  if 'spec' in name:
    return 'spec'
  if 'guide' in name or 'handoff' in name or 'readme' in name:
    return 'guide'
  if 'architecture' in name or 'design' in name or 'plan' in name:
    return 'design'
  return 'note'

def infer_doc_title(path: Path, front: dict):
  raw_title = front.get('title')
  if isinstance(raw_title, str) and raw_title.strip():
    return raw_title.strip()
  try:
    for line in path.read_text(encoding='utf-8').splitlines():
      title = line.strip()
      if title.startswith('# '):
        return title[2:].strip()
  except UnicodeDecodeError:
    pass
  return path.stem

print('BEGIN;')

seed_docs = []

def add_seed_doc(doc_path: Path, doc_type: str):
  front = parse_frontmatter(doc_path)
  title = infer_doc_title(doc_path, front)
  rel_path = doc_path.relative_to(docs_root).as_posix()
  doc_id = str(uuid.uuid5(NS, f"doc:{rel_path}"))
  seed_docs.append({
    'id': doc_id,
    'name': title,
    'doc_type': doc_type,
    'file_path': rel_path,
  })

for view_path in sorted((docs_root / 'views').rglob('*.md')):
  add_seed_doc(view_path, infer_doc_type(view_path))

for project_docs_dir in (docs_root / 'docs', docs_root / 'project_docs', docs_root / 'designs', docs_root / 'aux_files'):
  for project_doc_path in sorted(project_docs_dir.rglob('*.md')):
    front = parse_frontmatter(project_doc_path)
    add_seed_doc(project_doc_path, infer_project_doc_type(project_doc_path, front))

print('ALTER TABLE docs DISABLE ROW LEVEL SECURITY;')
print('ALTER TABLE docs DISABLE TRIGGER trg_docs_set_org;')
for doc in seed_docs:
  print(
    'INSERT INTO docs '
    '(id, workspace_id, org_id, repo_id, name, doc_type, file_path, content_updated_at, last_uplifted_commit_sha, last_downlifted_commit_sha, created_at, updated_at) '
    f"VALUES ({sql_string(doc['id'])}, {sql_string(workspace_id)}, {sql_string(org_id)}, {sql_string(docs_repo_id)}, {sql_string(doc['name'])}, {sql_string(doc['doc_type'])}, {sql_string(doc['file_path'])}, now(), {sql_string(docs_head)}, {sql_string(docs_head)}, now(), now());"
  )
print('ALTER TABLE docs ENABLE TRIGGER trg_docs_set_org;')
print('ALTER TABLE docs ENABLE ROW LEVEL SECURITY;')
print('ALTER TABLE docs FORCE ROW LEVEL SECURITY;')

print('COMMIT;')
PY

docker exec -i "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 < "$SQL_FILE" >/dev/null
rm -f "$SQL_FILE"

DOC_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}';" | tr -d '[:space:]')
DOC_BRANCH_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}' AND repo_id = '${DOCS_REPO_ID}';" | tr -d '[:space:]')
DOC_CLEAN_MARKER_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}' AND last_uplifted_commit_sha = '${DOCS_HEAD}' AND last_downlifted_commit_sha = '${DOCS_HEAD}';" | tr -d '[:space:]')
if [ "$DOC_COUNT" != "$DOC_BRANCH_COUNT" ]; then
  echo "✗ Docs branch ownership mismatch: ${DOC_BRANCH_COUNT}/${DOC_COUNT} docs are scoped to the docs branch" >&2
  exit 1
fi
if [ "$DOC_COUNT" != "$DOC_CLEAN_MARKER_COUNT" ]; then
  echo "✗ Docs clean-marker mismatch: ${DOC_CLEAN_MARKER_COUNT}/${DOC_COUNT} docs are stamped to docs HEAD" >&2
  exit 1
fi
echo "✓ Docs seeded:           $DOC_COUNT"
verify_seeded_document_references

cat > "$ENV_FILE" <<EOF
export G5_COMPLIANCE_WORKSPACE_ID=${WS_ID}
export G5_COMPLIANCE_DOCS_REPO_ID=${DOCS_REPO_ID}
export G5_COMPLIANCE_DOCS_HEAD_COMMIT=${DOCS_HEAD}
export G5_COMPLIANCE_CODE_REPO_ID=${CODE_REPO_ID}
export G5_COMPLIANCE_CODE_HEAD_COMMIT=${CODE_HEAD}
export G5_COMPLIANCE_WORKSPACE_NAME=${WORKSPACE_NAME}
EOF

echo ""
echo "Seed complete."
echo "  Workspace ID: $WS_ID"
echo "  Docs repo:    $DOCS_REPO_ID"
echo "  Docs HEAD:    $DOCS_HEAD"
echo "  Code repo:    $CODE_REPO_ID"
echo "  Code HEAD:    $CODE_HEAD"
echo "  Env file:     $ENV_FILE"
