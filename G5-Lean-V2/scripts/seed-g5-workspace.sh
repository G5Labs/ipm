#!/usr/bin/env bash
# =============================================================================
# seed-g5-workspace.sh -- Seed the prepared split G5 workspace into dev stack
# =============================================================================
# Registers a workspace prepared by prepare-g5-workspace.sh with the running
# dev API/Postgres stack. This script intentionally delegates the heavy lifting
# to seed-compliance-workspace.sh because that path now uses the same product
# seed lifecycle as external workspaces:
#
#   - authenticate through the API
#   - create a blank target workspace through the API
#   - stage prepared docs/code repos as local Git repos
#   - install them as managed repos and publish natively (fast-forward git
#     push through scripts/lib/native-publish.sh)
#   - sync committed docs/views and ontology metadata through the API
#
# Usage:
#   ./scripts/seed-g5-workspace.sh [workspace-dir] [api-url]
#
# Defaults:
#   workspace-dir = ../g5_workspace
#   api-url       = http://localhost:${G5_PORT_API:-7500}
#
# Common flow:
#   just prepare-g5-workspace
#   just seed-g5-workspace
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GEN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$GEN_ROOT"
source "$SCRIPT_DIR/lib/native-publish.sh"

WORKSPACE_ARG="${1:-${G5_WORKSPACE_DIR:-$REPO_ROOT/g5_workspace}}"
WORKSPACE_PARENT="$(cd "$(dirname "$WORKSPACE_ARG")" && pwd)"
WORKSPACE_DIR="$WORKSPACE_PARENT/$(basename "$WORKSPACE_ARG")"
API_URL="${2:-http://localhost:${G5_PORT_API:-7500}}"

DOCS_REPO_NAME="${G5_WORKSPACE_DOCS_REPO_NAME:-g5-docs}"
CODE_REPO_NAME="${G5_WORKSPACE_CODE_REPO_NAME:-g5-code}"
DOCS_SOURCE_REPO="$WORKSPACE_DIR/$DOCS_REPO_NAME"
CODE_SOURCE_REPO="$WORKSPACE_DIR/$CODE_REPO_NAME"

# =============================================================================
# Remote push mode (Stage-1 align-g5-repo — view D31 platform push smoke)
# =============================================================================
# Pushes a FAITHFUL copy of the Stage-1 repos to a remote platform workspace
# such as https://dev.g5labs.ai/internal-g5-devs/refactor-test. This is a
# disposable smoke seed, not the heavy DB/storage reseed runbook.
#
# Human-owned setup:
#   1. Provide a JWT: export G5D_TOKEN/G5_REFACTOR_JWT, or sign in once with
#      `g5d --api-url https://dev.g5labs.ai auth login` (add --org-id when the
#      installed g5d supports it) so ~/.g5/auth_state.json holds a refreshable
#      session. The script only reads that file; it never runs g5d.
#   2. Commit the root repo and g5_docs content that should be seeded, or set
#      G5_REFACTOR_SOURCE=worktree for an explicit local snapshot.
#   3. Provide G5_REFACTOR_ORG_ID or use an auth_state.json that contains orgId
#      if the script must resolve the workspace by name/slug.
#
# Script-owned automation:
#   - resolve canonical workspace id and existing code/docs branches
#   - stage root code from `git archive HEAD`, or tracked/nonignored local files
#     when G5_REFACTOR_SOURCE=worktree is set
#   - stage docs from `git -C g5_docs archive HEAD`, or tracked/nonignored local
#     files when G5_REFACTOR_SOURCE=worktree is set
#   - preserve existing remote seed lineage when branches already exist
#   - plan the code/docs publish (mirror head, ancestry and lease of every
#     staged member, and the branch of every manifest member, are validated
#     before the first push), then fast-forward push the pinned SHAs natively
#     (a branch already at the staged head is skipped)
#   - clone remote code/docs back and byte-verify against the staged trees
#
# Enable with G5_SEED_REMOTE_PUSH=1. Optional inputs:
#   G5_REFACTOR_WORKSPACE_URL  full URL, default https://dev.g5labs.ai/internal-g5-devs/refactor-test
#   G5_REFACTOR_API_URL        API origin override, default parsed from URL
#   G5_REFACTOR_WORKSPACE_ID   canonical workspace UUID override
#   G5_REFACTOR_WORKSPACE_REF  workspace name/slug, default parsed from URL
#   G5_REFACTOR_ORG_ID         org UUID, default from auth_state.json
#   G5_REFACTOR_CODE_REPO      code repo name/slug override, default discovered or g5-code
#   G5_REFACTOR_SOURCE         head (default) or worktree for exact local files
#   G5_REFACTOR_DEV_REPAIR_SERVER_REPOS=1
#                              dev-only kubectl repair: set updateInstead and
#                              reset/clean resolved server repo worktrees
#   G5_REFACTOR_VERIFY=0       skip clone-back verification
#   G5_REFACTOR_ALLOW_DIRTY=1  allow seeding committed HEAD while worktree is dirty
#
# Local prepared-workspace mode delegates to seed-compliance-workspace.sh, whose
# defaults validate the compliance ontology root (`root -> core_validation`).
# This G5 wrapper overrides those defaults to the G5 ontology root (`root`) unless
# the caller explicitly sets G5_SEED_EXPECTED_ROOT_COMPONENT / CHILD_COMPONENT.

REMOTE_API_URL=""
API_JWT=""
RESOLVED_ORG_ID=""
RESOLVED_WORKSPACE_ID=""
RESOLVED_WORKSPACE_NAME=""
RESOLVED_CODE_REPO_NAME=""
CODE_REPO_ID=""
CODE_BRANCH_NAME="main"
DOCS_REPO_ID=""
DOCS_BRANCH_NAME="main"
BRANCHES_JSON=""
REMOTE_SOURCE_MODE="head"
REMOTE_HISTORY_MODE="archive"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "✗ required command not found: $1" >&2
    exit 1
  }
}

remote_api_from_url() {
  printf '%s\n' "$1" | sed -E 's#^(https?://[^/]+).*#\1#'
}

remote_ref_from_url() {
  local without_query trimmed
  without_query="${1%%\?*}"
  without_query="${without_query%%#*}"
  trimmed="${without_query%/}"
  printf '%s\n' "${trimmed##*/}"
}

safe_stage_name() {
  local raw="$1" safe
  safe="$(printf '%s' "$raw" | tr -c 'A-Za-z0-9._-' '-' | sed -E 's/^-+//; s/-+$//')"
  printf '%s\n' "${safe:-workspace}"
}

normalize_remote_source_mode() {
  local mode="${G5_REFACTOR_SOURCE:-head}"
  case "$mode" in
    head|HEAD|commit|committed)
      printf '%s\n' "head"
      ;;
    worktree|working-tree|working_tree|dirty)
      printf '%s\n' "worktree"
      ;;
    *)
      echo "✗ Unsupported G5_REFACTOR_SOURCE='$mode'." >&2
      echo "  Use 'head' for committed HEAD or 'worktree' for exact local tracked/nonignored files." >&2
      exit 1
      ;;
  esac
}

normalize_remote_history_mode() {
  # archive (default): seed a flat single-commit snapshot via `git archive HEAD`,
  #   which produces NEW commit SHAs unrelated to GitHub lineage (existing smoke
  #   behavior; cannot fast-forward to/from GitHub).
  # full: seed the real commit objects from the local checkout so the platform
  #   repo shares lineage (identical HEAD SHA) with GitHub, enabling fast-forward
  #   platform<->GitHub pushes (align_g5_repo.arch_view.md D12).
  local mode="${G5_REFACTOR_HISTORY:-archive}"
  case "$mode" in
    archive|snapshot|flat)
      printf '%s\n' "archive"
      ;;
    full|history|lineage)
      printf '%s\n' "full"
      ;;
    *)
      echo "✗ Unsupported G5_REFACTOR_HISTORY='$mode'." >&2
      echo "  Use 'archive' for a flat snapshot or 'full' for real GitHub lineage." >&2
      exit 1
      ;;
  esac
}

global_auth_file() {
  printf '%s\n' "${G5D_AUTH_STATE:-${G5_HOME:-$HOME/.g5}/auth_state.json}"
}

load_remote_auth() {
  local auth_file refresh_token refresh_resp auth_org
  auth_file="$(global_auth_file)"
  RESOLVED_ORG_ID="${G5_REFACTOR_ORG_ID:-}"
  if [ -z "$RESOLVED_ORG_ID" ] && [ -f "$auth_file" ]; then
    auth_org="$(jq -r '.orgId // empty' "$auth_file")"
    [ -n "$auth_org" ] && RESOLVED_ORG_ID="$auth_org"
  fi

  if [ -n "${G5D_TOKEN:-}" ]; then
    API_JWT="$G5D_TOKEN"
    return
  fi
  if [ -n "${G5_REFACTOR_JWT:-}" ]; then
    API_JWT="$G5_REFACTOR_JWT"
    return
  fi
  if [ ! -f "$auth_file" ]; then
    echo "✗ No auth token found." >&2
    echo "  Human action: run g5d auth login --api-url ${REMOTE_API_URL} --org-id <org-uuid>" >&2
    echo "  or export G5D_TOKEN / G5_REFACTOR_JWT." >&2
    exit 1
  fi

  refresh_token="$(jq -r '.refreshToken // empty' "$auth_file")"
  if [ -n "$refresh_token" ] && [ -n "$RESOLVED_ORG_ID" ]; then
    refresh_resp="$(
      curl -fsS -X POST "${REMOTE_API_URL}/api/v1/auth/refresh" \
        -H 'content-type: application/json' \
        -d "{\"refreshToken\":\"${refresh_token}\",\"orgId\":\"${RESOLVED_ORG_ID}\"}"
    )"
    API_JWT="$(printf '%s' "$refresh_resp" | jq -r '.data.jwt // .jwt // empty')"
  fi
  if [ -z "$API_JWT" ]; then
    API_JWT="$(jq -r '.jwt // empty' "$auth_file")"
  fi
  if [ -z "$API_JWT" ]; then
    echo "✗ Could not resolve a JWT from $auth_file." >&2
    echo "  Human action: rerun g5d auth login --api-url ${REMOTE_API_URL} --org-id <org-uuid>." >&2
    exit 1
  fi
}

api_get() {
  curl -fsS -H "Authorization: Bearer ${API_JWT}" "${REMOTE_API_URL}$1"
}

api_post_json() {
  local path="$1" body="$2"
  curl -fsS -X POST "${REMOTE_API_URL}${path}" \
    -H "Authorization: Bearer ${API_JWT}" \
    -H 'content-type: application/json' \
    -d "$body"
}

check_clean_repo() {
  local label="$1" repo="$2" status
  if [ "$REMOTE_SOURCE_MODE" = "worktree" ]; then
    return
  fi
  if [ "${G5_REFACTOR_ALLOW_DIRTY:-0}" = "1" ]; then
    return
  fi
  status="$(git -C "$repo" status --short)"
  if [ -n "$status" ]; then
    echo "✗ $label has uncommitted changes, but remote seeding uses git archive HEAD." >&2
    echo "  Human action: commit/stash the intended content, rerun with G5_REFACTOR_SOURCE=worktree," >&2
    echo "  or rerun with G5_REFACTOR_ALLOW_DIRTY=1 to seed committed HEAD only." >&2
    printf '%s\n' "$status" >&2
    exit 1
  fi
}

overlay_git_worktree_snapshot() {
  local source_repo="$1" dest_repo="$2"
  shift 2

  (
    cd "$source_repo"
    git ls-files -z --cached --others --exclude-standard -- "$@" |
      while IFS= read -r -d '' path; do
        [ -e "$path" ] || [ -L "$path" ] || continue
        [ ! -d "$path" ] || [ -L "$path" ] || continue
        printf '%s\0' "$path"
      done |
      COPYFILE_DISABLE=1 tar --null -T - -cf -
  ) | ( cd "$dest_repo" && COPYFILE_DISABLE=1 tar -xf - )

  (
    cd "$source_repo"
    git ls-files -z --deleted -- "$@" |
      while IFS= read -r -d '' path; do
        rm -rf "$dest_repo/$path"
      done
  )
}

resolve_remote_workspace() {
  local workspace_ref workspaces workspace_line old_ifs workspace_json workspace_org
  workspace_ref="${G5_REFACTOR_WORKSPACE_ID:-${G5_REFACTOR_WORKSPACE_REF:-}}"
  if [ -z "$workspace_ref" ] && [ -n "${G5_REFACTOR_WORKSPACE_URL:-}" ]; then
    workspace_ref="$(remote_ref_from_url "$G5_REFACTOR_WORKSPACE_URL")"
  fi
  workspace_ref="${workspace_ref:-refactor-test}"

  if [ -n "${G5_REFACTOR_WORKSPACE_ID:-}" ]; then
    RESOLVED_WORKSPACE_ID="$G5_REFACTOR_WORKSPACE_ID"
    workspace_json="$(api_get "/api/v1/workspaces/${RESOLVED_WORKSPACE_ID}")"
    RESOLVED_WORKSPACE_NAME="$(printf '%s' "$workspace_json" | jq -r '.data.name // .name // empty')"
    # The workspace's own organization names its repositories' directory.
    workspace_org="$(printf '%s' "$workspace_json" | jq -r '.data.orgId // .orgId // empty')"
    [ -z "$workspace_org" ] || RESOLVED_ORG_ID="$workspace_org"
    return
  fi

  if [ -z "$RESOLVED_ORG_ID" ]; then
    echo "✗ Cannot resolve workspace '${workspace_ref}' without an org id." >&2
    echo "  Human action: export G5_REFACTOR_ORG_ID=<org-uuid> or rerun g5d auth login with --org-id." >&2
    exit 1
  fi

  workspaces="$(api_get "/api/v1/orgs/${RESOLVED_ORG_ID}/workspaces")"
  workspace_line="$(
    printf '%s' "$workspaces" | jq -r --arg ref "$workspace_ref" '
      .data[]? |
      select((.id // "") == $ref or (.slug // "") == $ref or (.name // "") == $ref) |
      [(.id // ""), (.name // .slug // "")] | @tsv
    ' | head -n 1
  )"
  if [ -z "$workspace_line" ]; then
    echo "✗ Workspace '${workspace_ref}' was not found in org ${RESOLVED_ORG_ID}." >&2
    echo "  Available workspaces:" >&2
    printf '%s' "$workspaces" | jq -r '.data[]? | "  - \(.id)  \(.name // "")  \(.slug // "")"' >&2
    exit 1
  fi
  old_ifs="$IFS"; IFS=$'\t'; read -r RESOLVED_WORKSPACE_ID RESOLVED_WORKSPACE_NAME <<EOF
$workspace_line
EOF
  IFS="$old_ifs"
}

resolve_remote_branches() {
  local code_line docs_line old_ifs discovered_code_name
  BRANCHES_JSON="$(api_get "/api/v1/workspaces/${RESOLVED_WORKSPACE_ID}/branches")"

  code_line="$(
    printf '%s' "$BRANCHES_JSON" | jq -r '
      .data[]? |
      select(((.repoName // .repoSlug // .repo_name // .name // "") | ascii_downcase) != "docs") |
      [(.repoName // .repoSlug // .name // ""), (.repoId // .repo_id // ""), (.branchName // "")] |
      @tsv
    ' | head -n 1
  )"
  if [ -n "$code_line" ]; then
    old_ifs="$IFS"; IFS=$'\t'; read -r discovered_code_name CODE_REPO_ID CODE_BRANCH_NAME <<EOF
$code_line
EOF
    IFS="$old_ifs"
  fi
  RESOLVED_CODE_REPO_NAME="${G5_REFACTOR_CODE_REPO:-${discovered_code_name:-g5-code}}"
  [ -n "$CODE_REPO_ID" ] && [ -n "$CODE_BRANCH_NAME" ] || { echo "✗ Workspace code membership or selected Git ref is missing" >&2; exit 1; }

  docs_line="$(
    printf '%s' "$BRANCHES_JSON" | jq -r '
      .data[]? |
      select(((.repoName // .repoSlug // .repo_name // .name // "") | ascii_downcase) == "docs") |
      [(.repoId // .repo_id // ""), (.branchName // "")] |
      @tsv
    ' | head -n 1
  )"
  if [ -n "$docs_line" ]; then
    old_ifs="$IFS"; IFS=$'\t'; read -r DOCS_REPO_ID DOCS_BRANCH_NAME <<EOF
$docs_line
EOF
    IFS="$old_ifs"
  fi
  [ -n "$DOCS_REPO_ID" ] && [ -n "$DOCS_BRANCH_NAME" ] || { echo "✗ Workspace docs membership or selected Git ref is missing" >&2; exit 1; }
}

clear_stage_worktree() {
  local repo_dir="$1"
  ( cd "$repo_dir" && git rm -r -q --ignore-unmatch . >/dev/null 2>&1 || true )
  find "$repo_dir" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
}

init_or_clone_stage_repo() {
  local label="$1" repo_dir="$2" remote_url="$3" branch="$4"
  rm -rf "$repo_dir"
  if [ -n "$remote_url" ]; then
    echo "  ${label}: cloning existing remote branch ${branch}"
    git -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
      clone --quiet --branch "$branch" "$remote_url" "$repo_dir"
  else
    echo "  ${label}: initializing fresh local seed branch ${branch}"
    mkdir -p "$repo_dir"
    git -C "$repo_dir" init -q -b "$branch"
  fi
}

commit_stage_repo_if_needed() {
  local repo_dir="$1" message="$2" seed_email="${G5_SEED_EMAIL:-demo@g5labs.ai}"
  git -C "$repo_dir" config user.email "$seed_email"
  git -C "$repo_dir" config user.name "G5 Refactor Seed"
  git -C "$repo_dir" config gc.auto 0
  git -C "$repo_dir" add -Af
  if git -C "$repo_dir" rev-parse --verify HEAD >/dev/null 2>&1 \
    && git -C "$repo_dir" diff --quiet \
    && git -C "$repo_dir" diff --cached --quiet; then
    echo "  no content changes to commit"
    return
  fi
  git -C "$repo_dir" commit -q -m "$message"
}

stage_remote_code_repo() {
  local repo_dir="$1" remote_url=""
  [ -n "$CODE_REPO_ID" ] && remote_url="${REMOTE_API_URL}/git/${CODE_REPO_ID}"
  init_or_clone_stage_repo "code" "$repo_dir" "$remote_url" "$CODE_BRANCH_NAME"
  clear_stage_worktree "$repo_dir"
  git -C "$REPO_ROOT" archive HEAD | COPYFILE_DISABLE=1 tar -x -C "$repo_dir"
  if [ "$REMOTE_SOURCE_MODE" = "worktree" ]; then
    overlay_git_worktree_snapshot "$REPO_ROOT" "$repo_dir" \
      ':!g5_docs' ':!g5_docs/**' ':!.tmp' ':!.tmp/**'
  fi
  rm -rf "$repo_dir/g5_docs"
  [ -f "$repo_dir/.semlift/specs/g5.semlift.md" ] || { echo "✗ staged code repo missing root .semlift/specs (not faithful)" >&2; exit 1; }
  { [ -d "$repo_dir/generation/g5-api" ] || [ -d "$repo_dir/backend/g5-api" ]; } || { echo "✗ staged code repo missing an expected G5 API source directory (not faithful)" >&2; exit 1; }
  # Both the post-rename archived_aux_files_readonly name and the historical
  # aux_files name (pre-rename branches) satisfy the code-side archive check
  # (issue #2094 rename).
  [ -d "$repo_dir/archived_aux_files_readonly" ] || [ -d "$repo_dir/aux_files" ] || { echo "✗ staged code repo missing archived_aux_files_readonly (must stay code-side)" >&2; exit 1; }
  [ ! -e "$repo_dir/g5_docs" ] || { echo "✗ staged code repo contains g5_docs content; docs must stay in docs repo" >&2; exit 1; }
  commit_stage_repo_if_needed "$repo_dir" "Stage-1 refactor code repo (faithful ${REMOTE_SOURCE_MODE} copy, generation-nested)"
}

stage_remote_docs_repo() {
  local repo_dir="$1" remote_url=""
  [ -n "$DOCS_REPO_ID" ] && remote_url="${REMOTE_API_URL}/git/${DOCS_REPO_ID}"
  init_or_clone_stage_repo "docs" "$repo_dir" "$remote_url" "$DOCS_BRANCH_NAME"
  clear_stage_worktree "$repo_dir"
  git -C "$REPO_ROOT/g5_docs" archive HEAD | COPYFILE_DISABLE=1 tar -x -C "$repo_dir"
  if [ "$REMOTE_SOURCE_MODE" = "worktree" ]; then
    overlay_git_worktree_snapshot "$REPO_ROOT/g5_docs" "$repo_dir"
  fi
  [ -d "$repo_dir/views" ] && [ -d "$repo_dir/designs" ] || { echo "✗ staged docs repo missing designs/views" >&2; exit 1; }
  [ ! -e "$repo_dir/aux_files" ] && [ ! -e "$repo_dir/archived_aux_files_readonly" ] || { echo "✗ docs repo unexpectedly contains the code-side archive tree (must stay code-side)" >&2; exit 1; }
  commit_stage_repo_if_needed "$repo_dir" "Stage-1 refactor docs repo (${REMOTE_SOURCE_MODE} copy = G5_Docs)"
}

verify_remote_repo_matches() {
  local label="$1" repo_id="$2" branch="$3" expected_dir="$4"
  local tmp_dir clone_dir diff_out expected_count remote_count
  if [ -z "$repo_id" ]; then
    echo "✗ Cannot verify ${label}: missing remote repo id" >&2
    exit 1
  fi
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/g5-refactor-verify.XXXXXX")"
  clone_dir="$tmp_dir/$label"
  git -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
    clone --quiet --branch "$branch" "${REMOTE_API_URL}/git/${repo_id}" "$clone_dir"
  expected_count="$(git -C "$expected_dir" ls-files | wc -l | tr -d ' ')"
  remote_count="$(git -C "$clone_dir" ls-files | wc -l | tr -d ' ')"
  diff_out="$(diff -rq --exclude=.git "$expected_dir" "$clone_dir" 2>&1 || true)"
  if [ -n "$diff_out" ]; then
    echo "✗ Remote ${label} repo differs from staged ${label} repo." >&2
    printf '%s\n' "$diff_out" | sed -n '1,40p' >&2
    rm -rf "$tmp_dir"
    exit 1
  fi
  echo "✓ Remote ${label} repo byte-verified (${remote_count} files, branch ${branch})"
  if [ "$expected_count" != "$remote_count" ]; then
    echo "✗ Remote ${label} file count mismatch: staged=${expected_count}, remote=${remote_count}" >&2
    rm -rf "$tmp_dir"
    exit 1
  fi
  rm -rf "$tmp_dir"
}

dev_server_repo_repair_requested() {
  [ "${G5_REFACTOR_DEV_REPAIR_SERVER_REPOS:-0}" = "1" ] \
    || [ "${G5_REFACTOR_DEV_SERVER_REPO_REPAIR:-0}" = "1" ]
}

repair_dev_server_repos_if_requested() {
  local reason="$1" context namespace selector repo_root pods repo_id pod
  if ! dev_server_repo_repair_requested; then
    return
  fi

  require_cmd kubectl
  context="${G5_REFACTOR_KUBE_CONTEXT:-$(kubectl config current-context)}"
  namespace="${G5_REFACTOR_KUBE_NAMESPACE:-g5}"
  selector="${G5_REFACTOR_API_POD_SELECTOR:-app=g5-api}"
  repo_root="${G5_REFACTOR_SERVER_REPO_ROOT:-/mnt/g5-repos/repos}"
  case "$RESOLVED_ORG_ID" in
    ''|*[!A-Za-z0-9-]*)
      echo "✗ Refusing dev repair without a valid org id: '${RESOLVED_ORG_ID}'" >&2
      exit 1
      ;;
  esac
  # Repositories live at <base>/<org-id>/<repo-id>.
  repo_root="${repo_root}/${RESOLVED_ORG_ID}"

  echo "=== Dev-only server repo repair (${reason}) ==="
  echo "  context:   $context"
  echo "  namespace: $namespace"
  echo "  selector:  $selector"

  pods="$(kubectl --context="$context" -n "$namespace" get pods -l "$selector" \
    -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')"
  if [ -z "$pods" ]; then
    echo "✗ No g5-api pods matched selector '$selector' in namespace '$namespace'." >&2
    exit 1
  fi

  for repo_id in "$CODE_REPO_ID" "$DOCS_REPO_ID"; do
    [ -n "$repo_id" ] || continue
    case "$repo_id" in
      *[!A-Za-z0-9._-]*)
        echo "✗ Refusing unsafe repo id for dev repair: $repo_id" >&2
        exit 1
        ;;
    esac

    while IFS= read -r pod; do
      [ -n "$pod" ] || continue
      echo "  repairing repo $repo_id on pod $pod"
      kubectl --context="$context" -n "$namespace" exec "$pod" -c g5-api -- sh -lc "
        set -e
        repo_path='${repo_root}/${repo_id}'
        test -d \"\$repo_path/.git\" || test -f \"\$repo_path/HEAD\"
        git -C \"\$repo_path\" config receive.denyCurrentBranch updateInstead
        if [ \"\$(git -C \"\$repo_path\" rev-parse --is-bare-repository)\" = \"false\" ]; then
          git -C \"\$repo_path\" reset --hard HEAD >/dev/null
          git -C \"\$repo_path\" clean -fd >/dev/null
        fi
        printf '    '
        git -C \"\$repo_path\" status --short | sed -n '1,5p'
        if [ -z \"\$(git -C \"\$repo_path\" status --short)\" ]; then
          echo '    status=clean'
        fi
      "
    done <<EOF
$pods
EOF
  done
}

# The two staged members are planned, then the complete manifest is checked:
# a workspace whose other code member lacks its branch is not published
# partially (the retired client read the whole manifest before pushing).
plan_remote_members() {
  native_publish_plan_reset
  native_publish_plan "code" "$WS/repos/code" "$CODE_REPO_ID" "$CODE_BRANCH_NAME" \
    && native_publish_plan "docs" "$WS/repos/docs" "$DOCS_REPO_ID" "$DOCS_BRANCH_NAME" \
    && native_publish_manifest_members_exist "$RESOLVED_WORKSPACE_ID"
}

# Validate every member (mirror head, ancestry, lease) before the first push,
# so one invalid member publishes nothing. Recovery order after a refused
# push: if the mirror already holds the staged heads, publication succeeded;
# otherwise the dev-only storage repair runs (when
# requested) and the members are re-planned and pushed once more.
publish_remote_members_with_recovery() {
  if ! plan_remote_members; then
    echo "✗ publish plan rejected; nothing was pushed." >&2
    echo "  If a branch is missing, create the workspace through the fork service first." >&2
    echo "  If the staged commit does not build on the mirror head, the target is divergent; use" >&2
    echo "  G5_REFACTOR_HISTORY=full or archived_aux_files_readonly/deploy/reseed_remote_workspace_api_only.md." >&2
    exit 1
  fi

  if native_publish_apply; then
    return
  fi

  if native_publish_plan_settled; then
    echo "  The mirror already holds the staged heads (lost push response); publication succeeded." >&2
    return
  fi

  repair_dev_server_repos_if_requested "publish-failure"
  if dev_server_repo_repair_requested; then
    echo "=== Retry publish after dev server repo repair ==="
    if plan_remote_members && native_publish_apply; then
      return
    fi
  fi

  echo "✗ native publish failed." >&2
  echo "  If the error says 'refusing to update checked out branch', the target server repo is a" >&2
  echo "  non-bare checkout. On dev, rerun with G5_REFACTOR_DEV_REPAIR_SERVER_REPOS=1." >&2
  echo "  If the error says 'Working directory has unstaged changes', a previous updateInstead push" >&2
  echo "  left the server checkout dirty; rerun with G5_REFACTOR_DEV_REPAIR_SERVER_REPOS=1 to" >&2
  echo "  reset/clean the resolved dev server repo worktrees before retrying." >&2
  echo "  If the target is disposable and intentionally divergent, use archived_aux_files_readonly/deploy/reseed_remote_workspace_api_only.md for the explicit force/delete fallback." >&2
  exit 1
}

# =============================================================================
# Full-history seed (G5_REFACTOR_HISTORY=full)
# =============================================================================
# Stage the REAL commit objects from the local checkout (not a `git archive`
# snapshot) so the platform repo carries identical HEAD SHAs and shares lineage
# with GitHub. Because Base already has populated repos, the fast-forward
# publish would reject the unrelated GitHub history; full-history seeding
# therefore force-pushes the real objects (the mirror still compare-and-swaps
# against the ref it advertised; the former X-G5-Force-Push header is refused
# by the server). Captured file readers see the published commits directly.

stage_remote_code_repo_full_history() {
  local repo_dir="$1" head_sha local_branch staged_head commit_count
  head_sha="$(git -C "$REPO_ROOT" rev-parse HEAD)"
  local_branch="$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)"
  rm -rf "$repo_dir"
  echo "  code: cloning full history from local checkout (branch ${local_branch}, HEAD ${head_sha:0:12})"
  # --no-local forces a real object transfer into a standalone repo (no shared
  # alternates), so the staged repo is independently pushable.
  git clone --quiet --no-local --single-branch --branch "$local_branch" \
    "file://$REPO_ROOT/.git" "$repo_dir"
  git -C "$repo_dir" checkout -q -B "$CODE_BRANCH_NAME" "$head_sha"
  git -C "$repo_dir" remote remove origin >/dev/null 2>&1 || true
  # Drop the source local branch ref so only the seed branch ancestry remains.
  if [ "$local_branch" != "$CODE_BRANCH_NAME" ]; then
    git -C "$repo_dir" branch -D "$local_branch" >/dev/null 2>&1 || true
  fi

  staged_head="$(git -C "$repo_dir" rev-parse HEAD)"
  [ "$staged_head" = "$head_sha" ] || {
    echo "✗ staged code HEAD ${staged_head} != local HEAD ${head_sha}" >&2; exit 1; }
  [ -f "$repo_dir/.semlift/specs/g5.semlift.md" ] || {
    echo "✗ staged code repo missing root .semlift/specs (not faithful)" >&2; exit 1; }
  { [ -d "$repo_dir/generation/g5-api" ] || [ -d "$repo_dir/backend/g5-api" ]; } || {
    echo "✗ staged code repo missing an expected G5 API source directory (not faithful)" >&2; exit 1; }
  [ -d "$repo_dir/archived_aux_files_readonly" ] || [ -d "$repo_dir/aux_files" ] || {
    echo "✗ staged code repo missing archived_aux_files_readonly (must stay code-side)" >&2; exit 1; }
  commit_count="$(git -C "$repo_dir" rev-list --count HEAD)"
  echo "  code: staged ${commit_count} commits on ${CODE_BRANCH_NAME} (docs history is independent)"
}

stage_remote_docs_repo_full_history() {
  local repo_dir="$1" head_sha commit_count
  head_sha="$(git -C "$REPO_ROOT/g5_docs" rev-parse HEAD)"
  rm -rf "$repo_dir"
  echo "  docs: cloning full history from independent g5_docs checkout (HEAD ${head_sha:0:12})"
  git clone --quiet --no-local "$REPO_ROOT/g5_docs" "$repo_dir"
  git -C "$repo_dir" checkout -q -B "$DOCS_BRANCH_NAME" "$head_sha"
  git -C "$repo_dir" remote remove origin >/dev/null 2>&1 || true
  [ -d "$repo_dir/views" ] && [ -d "$repo_dir/designs" ] || {
    echo "✗ staged docs repo missing designs/views" >&2; exit 1; }
  [ ! -e "$repo_dir/aux_files" ] && [ ! -e "$repo_dir/archived_aux_files_readonly" ] || {
    echo "✗ docs repo unexpectedly contains the code-side archive tree (must stay code-side)" >&2; exit 1; }
  commit_count="$(git -C "$repo_dir" rev-list --count HEAD)"
  echo "  docs: staged ${commit_count} commits on ${DOCS_BRANCH_NAME}"
}

ensure_full_history_seed_targets() {
  [ -n "$CODE_REPO_ID" ] && [ -n "$DOCS_REPO_ID" ] || {
    echo "Target workspace must already contain code and docs members; create it from a complete Base cut before seeding." >&2
    exit 1
  }
}

remote_ref_sha() {
  # Print the remote ref's current sha for a branch, or empty on failure.
  local repo_id="$1" branch="$2"
  git -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
    ls-remote "${REMOTE_API_URL}/git/${repo_id}" "refs/heads/${branch}" 2>/dev/null \
    | awk '{print $1}' | head -n 1
}

remote_branch_api_head() {
  # Print the workspace branch API head for a branch row/repo, or empty on failure.
  local repo_id="$1" branch="$2"
  api_get "/api/v1/workspaces/${RESOLVED_WORKSPACE_ID}/branches" 2>/dev/null |
    jq -r --arg repo "$repo_id" --arg branch "$branch" '
      ((.data.branches? // .data) // [])[]? |
      (.repoId // .repo_id // "") as $record_repo |
      (.branchName // .branch // .branch_name // "") as $record_branch |
      select(
        (($record_repo == $repo) and ($record_branch == $branch))
      ) |
      (.headCommit // .head_commit // "")
    ' | head -n 1
}

wait_remote_branch_api_head() {
  local repo_id="$1" branch="$2" expected_head="$3"
  local api_head="" attempt

  for attempt in 1 2 3 4 5; do
    api_head="$(remote_branch_api_head "$repo_id" "$branch")"
    if [ "$api_head" = "$expected_head" ]; then
      printf '%s\n' "$api_head"
      return 0
    fi
    [ "$attempt" = "5" ] || sleep 1
  done

  printf '%s\n' "${api_head:-unknown}"
  return 1
}

full_history_chunk_size() {
  local value="${G5_REFACTOR_FULL_HISTORY_CHUNK_COMMITS:-100}"
  case "$value" in
    ''|*[!0-9]*)
      echo "✗ G5_REFACTOR_FULL_HISTORY_CHUNK_COMMITS must be a non-negative integer" >&2
      return 1
      ;;
  esac
  printf '%s\n' "$value"
}

force_push_full_history_ref_with_recovery() {
  local label="$1" repo_dir="$2" repo_id="$3" src_ref="$4" branch="$5" expected_head="$6" description="$7"
  local remote_head push_status=0

  echo "  ${label}: force-pushing ${description} to ${branch} (${repo_id})"
  set +e
  git -C "$repo_dir" \
    -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
    push --force "${REMOTE_API_URL}/git/${repo_id}" "${src_ref}:refs/heads/${branch}"
  push_status=$?
  set -e

  remote_head="$(remote_ref_sha "$repo_id" "$branch")"
  if [ -n "$remote_head" ] && [ "$remote_head" = "$expected_head" ]; then
    if [ "$push_status" != "0" ]; then
      echo "  ${label}: remote ref reached ${expected_head:0:12} despite push error; treating ${description} as success" >&2
    fi
    return 0
  fi

  if [ "$push_status" = "0" ]; then
    echo "  ${label}: push returned success but remote ref is ${remote_head:-unknown}, expected ${expected_head:0:12}" >&2
  fi
  return 1
}

force_push_full_history_chunked_repo() {
  local label="$1" repo_dir="$2" repo_id="$3" branch="$4"
  local chunk_size total commits_file chunks_file tmp_ref total_chunks chunk_index sha expected_head remote_head

  chunk_size="$(full_history_chunk_size)" || return 1
  if [ "$chunk_size" = "0" ]; then
    echo "  ${label}: chunked full-history retry disabled (G5_REFACTOR_FULL_HISTORY_CHUNK_COMMITS=0)" >&2
    return 1
  fi

  total="$(git -C "$repo_dir" rev-list --first-parent --count "$branch")" || return 1
  if [ "$total" -le "$chunk_size" ]; then
    echo "  ${label}: not enough first-parent commits for chunked retry (${total} <= ${chunk_size})" >&2
    return 1
  fi

  commits_file="$(mktemp "${TMPDIR:-/tmp}/g5-${label}-commits.XXXXXX")" || return 1
  chunks_file="$(mktemp "${TMPDIR:-/tmp}/g5-${label}-chunks.XXXXXX")" || {
    rm -f "$commits_file"
    return 1
  }

  git -C "$repo_dir" rev-list --first-parent --reverse "$branch" > "$commits_file" || {
    rm -f "$commits_file" "$chunks_file"
    return 1
  }
  awk -v chunk="$chunk_size" '
    { last = $0; if (NR % chunk == 0) print $0 }
    END { if (NR > 0 && NR % chunk != 0) print last }
  ' "$commits_file" > "$chunks_file" || {
    rm -f "$commits_file" "$chunks_file"
    return 1
  }

  total_chunks="$(wc -l < "$chunks_file" | tr -d ' ')"
  tmp_ref="refs/heads/g5-seed-local-${label}-$$"
  echo "  ${label}: retrying full-history force-push as ${total_chunks} chunks of up to ${chunk_size} first-parent commits"

  chunk_index=0
  while IFS= read -r sha; do
    [ -n "$sha" ] || continue
    chunk_index=$((chunk_index + 1))
    git -C "$repo_dir" update-ref "$tmp_ref" "$sha" || {
      git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true
      rm -f "$commits_file" "$chunks_file"
      return 1
    }

    if ! force_push_full_history_ref_with_recovery \
      "$label" "$repo_dir" "$repo_id" "$tmp_ref" "$branch" "$sha" "chunk ${chunk_index}/${total_chunks} (${sha:0:12})"; then
      git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true
      rm -f "$commits_file" "$chunks_file"
      return 1
    fi
  done < "$chunks_file"

  git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true
  rm -f "$commits_file" "$chunks_file"

  expected_head="$(git -C "$repo_dir" rev-parse HEAD)" || return 1
  remote_head="$(remote_ref_sha "$repo_id" "$branch")"
  if [ "$remote_head" = "$expected_head" ]; then
    echo "  ${label}: chunked force-push reached staged HEAD ${expected_head:0:12}"
    return 0
  fi

  echo "  ${label}: chunked force-push ended at ${remote_head:-unknown}, expected ${expected_head:0:12}" >&2
  return 1
}

# Force-push with recovery. The platform git endpoint can return a transient
# gateway error (e.g. HTTP 502) on the response sideband AFTER the ref already
# updated; the server then enforces a compare-and-swap against the prior value,
# so a blind retry spuriously fails with "cannot lock ref ... but expected ...".
# So on any push failure, first check whether the remote ref already reached the
# staged HEAD and treat that as success before attempting a real retry.
force_push_full_history_with_recovery() {
  local label="$1" repo_dir="$2" repo_id="$3" branch="$4"
  local staged_head remote_head push_status=0 count chunk_attempted=0
  staged_head="$(git -C "$repo_dir" rev-parse HEAD)"

  remote_head="$(remote_ref_sha "$repo_id" "$branch")"
  if [ -n "$remote_head" ] && [ "$remote_head" = "$staged_head" ]; then
    echo "  ${label}: remote ref already at staged HEAD ${staged_head:0:12}; skipping force-push"
    return 0
  fi
  if [ -n "$remote_head" ] && [ "${G5_REFACTOR_FULL_HISTORY_FORCE_ONESHOT:-0}" != "1" ]; then
    echo "  ${label}: populated remote ref ${remote_head:0:12} differs from staged HEAD ${staged_head:0:12}; using chunked force-push"
    chunk_attempted=1
    if force_push_full_history_chunked_repo "$label" "$repo_dir" "$repo_id" "$branch"; then
      return 0
    fi
    echo "  ${label}: chunked force-push did not complete; falling back to one-shot push" >&2
  fi

  count="$(git -C "$repo_dir" rev-list --count "$branch")"
  echo "  ${label}: force-pushing ${count} commits to ${branch} (${repo_id})"
  set +e
  git -C "$repo_dir" \
    -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
    push --force "${REMOTE_API_URL}/git/${repo_id}" "${branch}:${branch}"
  push_status=$?
  set -e
  if [ "$push_status" = "0" ]; then
    return 0
  fi

  remote_head="$(remote_ref_sha "$repo_id" "$branch")"
  if [ -n "$remote_head" ] && [ "$remote_head" = "$staged_head" ]; then
    echo "  ${label}: remote ref already at staged HEAD ${staged_head:0:12} (transient response error); treating as success" >&2
    return 0
  fi

  if [ "$chunk_attempted" != "1" ]; then
    if force_push_full_history_chunked_repo "$label" "$repo_dir" "$repo_id" "$branch"; then
      return 0
    fi
  fi

  repair_dev_server_repos_if_requested "${label}-force-push-failure"
  if dev_server_repo_repair_requested; then
    if force_push_full_history_chunked_repo "$label" "$repo_dir" "$repo_id" "$branch"; then
      return 0
    fi
  fi

  echo "  ${label}: force-pushing ${count} commits to ${branch} (${repo_id})"
  set +e
  git -C "$repo_dir" \
    -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
    push --force "${REMOTE_API_URL}/git/${repo_id}" "${branch}:${branch}"
  push_status=$?
  set -e
  if [ "$push_status" = "0" ]; then
    return 0
  fi

  remote_head="$(remote_ref_sha "$repo_id" "$branch")"
  if [ -n "$remote_head" ] && [ "$remote_head" = "$staged_head" ]; then
    echo "  ${label}: remote ref reached staged HEAD ${staged_head:0:12} after retry; treating as success" >&2
    return 0
  fi

  echo "✗ ${label}: force push did not land staged HEAD ${staged_head} (remote ref=${remote_head:-unknown})." >&2
  echo "  If the error mentions 'refusing to update checked out branch', the Base server repo is a" >&2
  echo "  non-bare checkout; rerun with G5_REFACTOR_DEV_REPAIR_SERVER_REPOS=1 (needs dev kubectl/gcloud auth)." >&2
  exit 1
}

ensure_remote_branch_api_head() {
  local label="$1" repo_dir="$2" repo_id="$3" branch="$4"
  local expected_head remote_head api_head parent tmp_ref max_repair_attempts repair_attempt

  expected_head="$(git -C "$repo_dir" rev-parse HEAD)"
  if [ -z "$repo_id" ]; then
    echo "✗ ${label}: cannot verify workspace branch API head because no repository id was resolved" >&2
    exit 1
  fi

  if api_head="$(wait_remote_branch_api_head "$repo_id" "$branch" "$expected_head")"; then
    echo "  ${label}: workspace branch API head verified at ${api_head:0:12}"
    return
  fi

  remote_head="$(remote_ref_sha "$repo_id" "$branch")"
  if [ "$remote_head" != "$expected_head" ]; then
    echo "✗ ${label}: workspace branch API head is ${api_head:-unknown}, and Git ref is ${remote_head:-unknown}; expected ${expected_head}." >&2
    echo "  The seed cannot repair metadata until the Git ref itself reaches the staged HEAD." >&2
    exit 1
  fi

  parent="$(git -C "$repo_dir" rev-parse "${expected_head}^" 2>/dev/null || true)"
  if [ -z "$parent" ]; then
    echo "✗ ${label}: Git ref is at staged HEAD but workspace branch API head is ${api_head:-unknown}; no parent commit exists for ref-only metadata repair." >&2
    exit 1
  fi

  echo "  ${label}: workspace branch API head is ${api_head:-unknown} while Git ref is ${expected_head:0:12}; repairing with ref-only receive"
  max_repair_attempts="${G5_REFACTOR_API_HEAD_REPAIR_ATTEMPTS:-5}"
  case "$max_repair_attempts" in
    ''|*[!0-9]*|0)
      echo "✗ G5_REFACTOR_API_HEAD_REPAIR_ATTEMPTS must be a positive integer" >&2
      exit 1
      ;;
  esac

  tmp_ref="refs/heads/g5-seed-api-head-repair-${label}-$$"
  repair_attempt=1
  while [ "$repair_attempt" -le "$max_repair_attempts" ]; do
    echo "  ${label}: API-head repair attempt ${repair_attempt}/${max_repair_attempts}"
    git -C "$repo_dir" update-ref "$tmp_ref" "$parent"
    if ! force_push_full_history_ref_with_recovery \
      "$label" "$repo_dir" "$repo_id" "$tmp_ref" "$branch" "$parent" "API-head repair parent (${parent:0:12})"; then
      git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true
      echo "✗ ${label}: failed to move ${branch} to parent during API-head repair" >&2
      exit 1
    fi

    git -C "$repo_dir" update-ref "$tmp_ref" "$expected_head"
    if ! force_push_full_history_ref_with_recovery \
      "$label" "$repo_dir" "$repo_id" "$tmp_ref" "$branch" "$expected_head" "API-head repair staged HEAD (${expected_head:0:12})"; then
      git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true
      echo "✗ ${label}: failed to move ${branch} back to staged HEAD during API-head repair" >&2
      exit 1
    fi

    if api_head="$(wait_remote_branch_api_head "$repo_id" "$branch" "$expected_head")"; then
      git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true
      echo "  ${label}: workspace branch API head repaired to ${api_head:0:12}"
      return
    fi

    echo "  ${label}: workspace branch API head still ${api_head:-unknown} after repair attempt ${repair_attempt}/${max_repair_attempts}" >&2
    repair_attempt=$((repair_attempt + 1))
    [ "$repair_attempt" -gt "$max_repair_attempts" ] || sleep 2
  done
  git -C "$repo_dir" update-ref -d "$tmp_ref" >/dev/null 2>&1 || true

  echo "✗ ${label}: Git ref is ${expected_head}, but workspace branch API head remains ${api_head:-unknown} after ref-only repair." >&2
  echo "  The workspace reader would select stale file content; inspect the server receive path before retrying." >&2
  exit 1
}

verify_remote_history_matches() {
  local label="$1" repo_id="$2" branch="$3" expected_dir="$4"
  local tmp_dir clone_dir expected_head remote_head expected_count remote_count
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/g5-refactor-hist.XXXXXX")"
  clone_dir="$tmp_dir/$label"
  git -c "http.extraHeader=Authorization: Bearer ${API_JWT}" \
    clone --quiet --branch "$branch" "${REMOTE_API_URL}/git/${repo_id}" "$clone_dir"
  expected_head="$(git -C "$expected_dir" rev-parse HEAD)"
  remote_head="$(git -C "$clone_dir" rev-parse HEAD)"
  expected_count="$(git -C "$expected_dir" rev-list --count HEAD)"
  remote_count="$(git -C "$clone_dir" rev-list --count HEAD)"
  rm -rf "$tmp_dir"
  if [ "$expected_head" != "$remote_head" ]; then
    echo "✗ Remote ${label} HEAD ${remote_head} != staged HEAD ${expected_head} (lineage not preserved)" >&2
    exit 1
  fi
  if [ "$expected_count" != "$remote_count" ]; then
    echo "✗ Remote ${label} history depth ${remote_count} != staged ${expected_count}" >&2
    exit 1
  fi
  echo "✓ Remote ${label} lineage verified: HEAD ${remote_head:0:12}, ${remote_count} commits (shared GitHub lineage)"
}

remote_push_full_history() {
  local ws="$1"
  echo "=== Stage full-history code repo (real GitHub lineage, generation-nested) ==="
  stage_remote_code_repo_full_history "$ws/repos/code"
  echo "=== Stage full-history docs repo from g5_docs (= G5_Docs) ==="
  stage_remote_docs_repo_full_history "$ws/repos/docs"

  ensure_full_history_seed_targets "$ws"
  repair_dev_server_repos_if_requested "pre-full-history-force-push"

  # Base repos are non-bare checkouts; the pre-push repair already set
  # receive.denyCurrentBranch=updateInstead and cleaned the worktrees when
  # G5_REFACTOR_DEV_REPAIR_SERVER_REPOS=1 was requested.
  echo "=== Force-push real objects ==="
  force_push_full_history_with_recovery "code" "$ws/repos/code" "$CODE_REPO_ID" "$CODE_BRANCH_NAME"
  force_push_full_history_with_recovery "docs" "$ws/repos/docs" "$DOCS_REPO_ID" "$DOCS_BRANCH_NAME"

  echo "=== Verify workspace branch API heads ==="
  ensure_remote_branch_api_head "code" "$ws/repos/code" "$CODE_REPO_ID" "$CODE_BRANCH_NAME"
  ensure_remote_branch_api_head "docs" "$ws/repos/docs" "$DOCS_REPO_ID" "$DOCS_BRANCH_NAME"

  if [ "${G5_REFACTOR_VERIFY:-1}" != "0" ]; then
    echo "=== Verify remote lineage + content match staged full-history repos ==="
    verify_remote_history_matches "code" "$CODE_REPO_ID" "$CODE_BRANCH_NAME" "$ws/repos/code"
    verify_remote_history_matches "docs" "$DOCS_REPO_ID" "$DOCS_BRANCH_NAME" "$ws/repos/docs"
    verify_remote_repo_matches "code" "$CODE_REPO_ID" "$CODE_BRANCH_NAME" "$ws/repos/code"
    verify_remote_repo_matches "docs" "$DOCS_REPO_ID" "$DOCS_BRANCH_NAME" "$ws/repos/docs"
  else
    echo "⚠ Skipped remote verification because G5_REFACTOR_VERIFY=0"
  fi

  echo "✓ Seeded full-history Stage-1 code + docs repos to ${REMOTE_API_URL} workspace ${RESOLVED_WORKSPACE_ID}"
  echo "  Code HEAD now shares GitHub lineage; platform<->GitHub pushes can fast-forward."
  echo "  Staged workspace kept at: $ws"
}

remote_push() {
  for c in git tar jq curl sed diff mktemp; do require_cmd "$c"; done

  local default_url="${G5_REFACTOR_WORKSPACE_URL:-https://dev.g5labs.ai/internal-g5-devs/refactor-test}"
  REMOTE_API_URL="${G5_REFACTOR_API_URL:-$(remote_api_from_url "$default_url")}"
  REMOTE_API_URL="${REMOTE_API_URL%/}"
  REMOTE_SOURCE_MODE="$(normalize_remote_source_mode)"
  REMOTE_HISTORY_MODE="$(normalize_remote_history_mode)"

  if [ "$REMOTE_HISTORY_MODE" = "full" ] && [ "$REMOTE_SOURCE_MODE" = "worktree" ]; then
    echo "✗ G5_REFACTOR_HISTORY=full requires committed history (G5_REFACTOR_SOURCE=head)." >&2
    echo "  Worktree overlay would add a non-GitHub commit and break SHA/lineage parity." >&2
    echo "  Commit the intended content, then rerun with G5_REFACTOR_SOURCE=head." >&2
    exit 1
  fi

  "$REPO_ROOT/scripts/check-g5-docs-checkout" "$REPO_ROOT/g5_docs" >/dev/null

  echo "=== G5 remote seed automation ==="
  echo "Human-owned prerequisites:"
  echo "  - A JWT: export G5D_TOKEN/G5_REFACTOR_JWT, or sign in with"
  echo "    g5d --api-url ${REMOTE_API_URL} auth login (the script reads ~/.g5/auth_state.json)"
  if [ "$REMOTE_SOURCE_MODE" = "worktree" ]; then
    echo "  - You explicitly requested exact local worktree content with G5_REFACTOR_SOURCE=worktree"
  else
    echo "  - Root repo and g5_docs are committed at the content you want to seed"
  fi
  echo "  - Target workspace/org are correct (script resolves refactor-test unless overridden)"
  echo "Automation-owned steps: resolve target, stage repos, fast-forward publish, clone-back verify"
  echo "  - Source mode: ${REMOTE_SOURCE_MODE}"

  check_clean_repo "root repo" "$REPO_ROOT"
  check_clean_repo "g5_docs repo" "$REPO_ROOT/g5_docs"

  load_remote_auth
  resolve_remote_workspace
  resolve_remote_branches
  repair_dev_server_repos_if_requested "pre-push"

  local stage_name WS
  stage_name="$(safe_stage_name "${G5_REFACTOR_WORKSPACE_REF:-${RESOLVED_WORKSPACE_NAME:-$(remote_ref_from_url "$default_url")}}")"
  WS="$REPO_ROOT/.tmp/seed-workspaces/$stage_name"
  case "$WS" in "/"|"$REPO_ROOT"|"$GEN_ROOT") echo "✗ refusing unsafe staging path: $WS" >&2; exit 1;; esac
  rm -rf "$WS"; mkdir -p "$WS/repos/code" "$WS/repos/docs"

  echo "=== Resolved target ==="
  echo "  API:       $REMOTE_API_URL"
  echo "  Org:       ${RESOLVED_ORG_ID:-unknown}"
  echo "  Workspace: ${RESOLVED_WORKSPACE_ID} ${RESOLVED_WORKSPACE_NAME:+($RESOLVED_WORKSPACE_NAME)}"
  echo "  Code repo: ${RESOLVED_CODE_REPO_NAME} branch ${CODE_BRANCH_NAME}${CODE_REPO_ID:+ repo $CODE_REPO_ID}"
  echo "  Docs repo: docs branch ${DOCS_BRANCH_NAME}${DOCS_REPO_ID:+ repo $DOCS_REPO_ID}"
  echo "  History:   ${REMOTE_HISTORY_MODE}"

  if [ "$REMOTE_HISTORY_MODE" = "full" ]; then
    remote_push_full_history "$WS"
    return
  fi

  echo "=== Stage faithful code repo from ${REMOTE_SOURCE_MODE} (generation-nested) ==="
  stage_remote_code_repo "$WS/repos/code"

  echo "=== Stage docs repo from g5_docs ${REMOTE_SOURCE_MODE} (= G5_Docs, no aux_files) ==="
  stage_remote_docs_repo "$WS/repos/docs"

  echo "=== Publish staged code + docs (plan every member, then fast-forward with the pinned lease) ==="
  native_publish_configure "$REMOTE_API_URL" "$API_JWT"
  publish_remote_members_with_recovery

  if [ "${G5_REFACTOR_VERIFY:-1}" != "0" ]; then
    echo "=== Verify remote code/docs match staged content ==="
    resolve_remote_branches
    verify_remote_repo_matches "code" "$CODE_REPO_ID" "$CODE_BRANCH_NAME" "$WS/repos/code"
    verify_remote_repo_matches "docs" "$DOCS_REPO_ID" "$DOCS_BRANCH_NAME" "$WS/repos/docs"
  else
    echo "⚠ Skipped remote clone-back verification because G5_REFACTOR_VERIFY=0"
  fi

  echo "✓ Pushed faithful Stage-1 code + docs repos to ${REMOTE_API_URL} workspace ${RESOLVED_WORKSPACE_ID}"
  echo "  Staged workspace kept at: $WS"
}

# Legacy grounding anchor for historical whole-script edges.
seed-g5-workspace.sh() {
  remote_push "$@"
}

if [ -n "${G5_SEED_REMOTE_PUSH:-}" ]; then
  remote_push
  exit 0
fi

if [ ! -d "$WORKSPACE_DIR" ]; then
  echo "error: prepared workspace not found: $WORKSPACE_DIR" >&2
  echo "hint: run 'just prepare-g5-workspace' first" >&2
  exit 1
fi

if [ ! -d "$DOCS_SOURCE_REPO" ]; then
  echo "error: docs repo not found: $DOCS_SOURCE_REPO" >&2
  echo "hint: expected repo name can be overridden with G5_WORKSPACE_DOCS_REPO_NAME" >&2
  exit 1
fi

if [ ! -d "$CODE_SOURCE_REPO" ]; then
  echo "error: code repo not found: $CODE_SOURCE_REPO" >&2
  echo "hint: expected repo name can be overridden with G5_WORKSPACE_CODE_REPO_NAME" >&2
  exit 1
fi

if [ ! -f "$CODE_SOURCE_REPO/semlift.yaml" ]; then
  echo "error: prepared code repo is missing semlift.yaml: $CODE_SOURCE_REPO" >&2
  exit 1
fi

if [ ! -d "$CODE_SOURCE_REPO/.semlift/specs" ]; then
  echo "error: prepared code repo is missing .semlift/specs: $CODE_SOURCE_REPO" >&2
  exit 1
fi

echo "=== G5 Prepared Workspace Seed ==="
echo "  Workspace: $WORKSPACE_DIR"
echo "  Docs repo: $DOCS_SOURCE_REPO"
echo "  Code repo: $CODE_SOURCE_REPO"
echo "  API:       $API_URL"
echo ""

export G5_COMPLIANCE_WORKSPACE_NAME="${G5_SEED_WORKSPACE_NAME:-G5}"
export G5_COMPLIANCE_WORKSPACE_DESCRIPTION="${G5_SEED_WORKSPACE_DESCRIPTION:-G5 self-hosting workspace seeded from prepared split repos}"
export G5_COMPLIANCE_SEED_LABEL="${G5_SEED_LABEL:-G5 Prepared Workspace Seed}"
export G5_COMPLIANCE_DOCS_REPO_NAME="${G5_SEED_DOCS_REPO_NAME:-docs}"
export G5_COMPLIANCE_DOCS_REPO_SLUG="${G5_SEED_DOCS_REPO_SLUG:-docs}"
export G5_COMPLIANCE_CODE_REPO_NAME="${G5_SEED_CODE_REPO_NAME:-$CODE_REPO_NAME}"
export G5_COMPLIANCE_CODE_REPO_SLUG="${G5_SEED_CODE_REPO_SLUG:-g5-code}"
export G5_COMPLIANCE_LEGACY_REPO_SLUG="${G5_SEED_LEGACY_REPO_SLUG:-g5-repo}"
export G5_COMPLIANCE_ENV_FILE="${G5_SEED_ENV_FILE:-$GEN_ROOT/.env.g5-workspace}"
export G5_COMPLIANCE_SEED_EMAIL="${G5_SEED_EMAIL:-demo@g5labs.ai}"
export G5_COMPLIANCE_EXPECTED_ROOT_COMPONENT="${G5_SEED_EXPECTED_ROOT_COMPONENT:-root}"
export G5_COMPLIANCE_EXPECTED_CHILD_COMPONENT="${G5_SEED_EXPECTED_CHILD_COMPONENT:-}"
export G5_COMPLIANCE_EXPECTED_ONTOLOGY_SHAPE_LABEL="${G5_SEED_EXPECTED_ONTOLOGY_SHAPE_LABEL:-g5-code_project -> root}"
export G5_COMPLIANCE_REQUIRED_DOC_LINK_PATH="${G5_SEED_REQUIRED_DOC_LINK_PATH:-views/pm_views/sticky_chat_runtime.pm_view.md}"
export G5_COMPLIANCE_REQUIRED_DOC_LINK_MIN="${G5_SEED_REQUIRED_DOC_LINK_MIN:-4}"

# The prepared G5 workspace preserves existing chat-targeted grounding. Allow it
# explicitly for this self-hosting seed; compliance seeds keep the stricter
# default unless their caller opts in.
export G5_COMPLIANCE_ALLOW_CHAT_TARGET_GROUNDING="${G5_SEED_ALLOW_CHAT_TARGET_GROUNDING:-true}"

# G5 self-hosting starts from exact selected GitHub commits. Historical ledger
# placement is part of those immutable source trees; the managed product router
# owns canonical ingestion, while the generic compliance seed keeps its stricter
# source-layout preflight by default.
export G5_COMPLIANCE_VALIDATE_PREPARED_OWNERSHIP="${G5_SEED_VALIDATE_PREPARED_OWNERSHIP:-false}"
export G5_COMPLIANCE_VERIFY_SOURCE_TREE="${G5_SEED_VERIFY_SOURCE_TREE:-true}"

exec "$SCRIPT_DIR/seed-compliance-workspace.sh" "$DOCS_SOURCE_REPO" "$CODE_SOURCE_REPO" "$API_URL"
