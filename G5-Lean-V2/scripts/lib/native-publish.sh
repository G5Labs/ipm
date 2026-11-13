#!/usr/bin/env bash
# =============================================================================
# native-publish.sh — publish staged workspace members with plain git + curl
# =============================================================================
# Shared by the seed scripts in place of the retired `g5d push`. Publication
# has two phases so that one invalid member publishes nothing:
#
#   plan   read the mirror head of every member, check that the staged commit
#          builds on it, and pin both SHAs as a row `label|dir|repo|branch|old|new`
#   apply  push each pinned `new` SHA (never a re-read HEAD) to
#          `<api>/git/<repoId>` leased to the pinned `old` SHA; a row whose
#          branch already sits at `new` is skipped
#
# Captured Git/file readers see the published refs directly.
#
# The mirror updates existing `refs/heads/*` only (workspace branch creation
# stays with the fork service), refuses the obsolete X-G5-Force-Push header,
# and compare-and-swaps against the head it advertised. It does not refuse an
# authorized non-fast-forward rewrite, which is why the plan phase checks
# ancestry before anything is pushed.
#
# Usage (after `set -euo pipefail`; needs git, curl, jq):
#   source "$SCRIPT_DIR/lib/native-publish.sh"
#   native_publish_configure "$API_URL" "$JWT"
#   native_publish_plan_reset
#   native_publish_plan "code" "$code_dir" "$CODE_REPO_ID" "$branch"
#   native_publish_plan "docs" "$docs_dir" "$DOCS_REPO_ID" "$branch"
#   native_publish_apply
# =============================================================================

NATIVE_PUBLISH_API_URL=""
NATIVE_PUBLISH_TOKEN=""
NATIVE_PUBLISH_PLAN=()

native_publish_configure() {
  NATIVE_PUBLISH_API_URL="${1%/}"
  NATIVE_PUBLISH_TOKEN="$2"
  if [ -z "$NATIVE_PUBLISH_API_URL" ] || [ -z "$NATIVE_PUBLISH_TOKEN" ]; then
    echo "✗ native publish needs an API URL and a bearer token" >&2
    return 1
  fi
  NATIVE_PUBLISH_PLAN=()
  # Reads that have no member directory (the manifest member check) run in
  # the caller's Git configuration, which must not rewrite the API either.
  native_publish_refuse_url_rewrites . "${NATIVE_PUBLISH_API_URL}/git/"
}

# The bearer token travels only to the API origin (`http.<url>.extraHeader`
# applies by URL prefix), never to a destination a rewrite could substitute.
native_publish_git() {
  git -c "http.${NATIVE_PUBLISH_API_URL}/.extraHeader=Authorization: Bearer ${NATIVE_PUBLISH_TOKEN}" "$@"
}

# Inherited url.<base>.insteadOf / pushInsteadOf rewrites can send a push
# somewhere other than the mirror the plan read (the retired runner compared
# the expanded destination with the expected URL), so any rewrite that
# applies to a member's mirror URL refuses its plan.
#   native_publish_refuse_url_rewrites <repo_dir> <mirror url>
native_publish_refuse_url_rewrites() {
  local repo_dir="$1" url="$2" rewrites key value
  rewrites="$(git -C "$repo_dir" config --get-regexp '^url\..*\.(insteadof|pushinsteadof)$' 2>/dev/null || true)"
  [ -n "$rewrites" ] || return 0
  while read -r key value; do
    [ -n "$key" ] || continue
    case "$url" in
      "${value}"*)
        echo "✗ Git config ${key}=${value} rewrites ${url}; unset it before publishing" >&2
        return 1
        ;;
    esac
  done <<<"$rewrites"
}

native_publish_api_get() {
  local path="$1"
  curl -fsS "${NATIVE_PUBLISH_API_URL}${path}" \
    -H "Authorization: Bearer ${NATIVE_PUBLISH_TOKEN}"
}

# Print the mirror's sha for refs/heads/<branch>; empty when the branch does
# not exist. A transport failure fails the pipeline. Runs in <repo_dir>
# (default: the caller's directory) so the read and the later push see the
# same Git configuration, which the plan has checked for URL rewrites.
#   native_publish_remote_head <repo_id> <branch> [repo_dir]
native_publish_remote_head() {
  local repo_id="$1" branch="$2" repo_dir="${3:-.}"
  native_publish_git -C "$repo_dir" ls-remote "${NATIVE_PUBLISH_API_URL}/git/${repo_id}" "refs/heads/${branch}" | cut -f1
}

# ─── Phase 1: plan ───────────────────────────────────────────────────────────

native_publish_plan_reset() {
  NATIVE_PUBLISH_PLAN=()
}

# Print the planned rows, one `label|dir|repo|branch|old|new` per line.
native_publish_plan_rows() {
  local row
  for row in "${NATIVE_PUBLISH_PLAN[@]}"; do
    printf '%s\n' "$row"
  done
}

# Validate one member and pin it: the mirror must have refs/heads/<branch>,
# and <repo_dir>'s HEAD must build on it. Nothing is pushed here.
native_publish_plan() {
  local label="$1" repo_dir="$2" repo_id="$3" branch="$4"
  local local_head remote_head
  case "$label$repo_dir$repo_id$branch" in
    *'|'*)
      echo "✗ ${label}: plan fields may not contain '|'" >&2
      return 1
      ;;
  esac
  if [ -z "$repo_id" ] || [ -z "$branch" ]; then
    echo "✗ ${label}: cannot plan a publish without a repo id and branch" >&2
    return 1
  fi
  if ! native_publish_refuse_url_rewrites "$repo_dir" "${NATIVE_PUBLISH_API_URL}/git/${repo_id}"; then
    echo "✗ ${label}: the publish destination would be rewritten; nothing is planned" >&2
    return 1
  fi
  if ! local_head="$(git -C "$repo_dir" rev-parse --verify 'HEAD^{commit}' 2>/dev/null)"; then
    echo "✗ ${label}: ${repo_dir} has no commit to publish" >&2
    return 1
  fi
  if ! remote_head="$(native_publish_remote_head "$repo_id" "$branch" "$repo_dir")"; then
    echo "✗ ${label}: could not read refs/heads/${branch} of repo ${repo_id} from the mirror" >&2
    return 1
  fi
  if [ -z "$remote_head" ]; then
    echo "✗ ${label}: branch ${branch} does not exist on repo ${repo_id}; workspace branch creation is reserved for the fork service" >&2
    return 1
  fi
  if [ "$remote_head" != "$local_head" ] \
    && ! git -C "$repo_dir" merge-base --is-ancestor "$remote_head" "$local_head" 2>/dev/null; then
    echo "✗ ${label}: staged ${local_head:0:12} does not build on ${branch} at ${remote_head:0:12}; only fast-forwards are published" >&2
    return 1
  fi
  NATIVE_PUBLISH_PLAN+=("${label}|${repo_dir}|${repo_id}|${branch}|${remote_head}|${local_head}")
  if [ "$remote_head" = "$local_head" ]; then
    echo "  ✓ ${label}: ${branch} already at ${local_head:0:12}; nothing to push"
  else
    echo "  … ${label}: ${branch} ${remote_head:0:12} → ${local_head:0:12} planned"
  fi
}

# ─── Phase 2: apply ──────────────────────────────────────────────────────────

# Push every planned row with its pinned SHAs. Stops at the first refusal.
native_publish_apply() {
  local row label repo_dir repo_id branch old new
  if [ "${#NATIVE_PUBLISH_PLAN[@]}" -eq 0 ]; then
    echo "✗ native publish: nothing planned" >&2
    return 1
  fi
  for row in "${NATIVE_PUBLISH_PLAN[@]}"; do
    IFS='|' read -r label repo_dir repo_id branch old new <<<"$row"
    if [ "$old" = "$new" ]; then
      echo "✓ ${label}: ${branch} already at ${new:0:12}"
      continue
    fi
    echo "  … ${label}: pushing ${new:0:12} to ${branch} (leased to ${old:0:12})"
    # One refspec only: inherited push.followTags or submodule recursion must
    # not widen the update beyond the planned branch (the retired client
    # passed the same flags).
    if ! native_publish_git -C "$repo_dir" push --quiet \
      --no-follow-tags --recurse-submodules=no \
      --force-with-lease="refs/heads/${branch}:${old}" \
      "${NATIVE_PUBLISH_API_URL}/git/${repo_id}" "${new}:refs/heads/${branch}"; then
      echo "✗ ${label}: push of ${branch} was refused (the branch moved or the mirror rejected the update)" >&2
      return 1
    fi
    echo "✓ ${label}: ${branch} fast-forwarded to ${new:0:12}"
  done
}

# True when every planned branch already sits at its pinned `new` SHA on the
# mirror: matching heads confirm a lost push response was successful and
# need no repair.
native_publish_plan_settled() {
  local row label repo_dir repo_id branch old new remote_head
  [ "${#NATIVE_PUBLISH_PLAN[@]}" -gt 0 ] || return 1
  for row in "${NATIVE_PUBLISH_PLAN[@]}"; do
    IFS='|' read -r label repo_dir repo_id branch old new <<<"$row"
    remote_head="$(native_publish_remote_head "$repo_id" "$branch" "$repo_dir")" || return 1
    if [ "$remote_head" != "$new" ]; then
      echo "  ${label}: ${branch} is at ${remote_head:-missing}, staged ${new:0:12}" >&2
      return 1
    fi
  done
}

# Every member of the workspace manifest (its docs member and each code repo)
# must have its branch on the mirror before anything is pushed, whether or
# not that member is planned: a workspace with a missing member is not
# published partially (the retired client read the whole manifest first).
#   native_publish_manifest_members_exist <workspace>
native_publish_manifest_members_exist() {
  local workspace_id="$1" resp members repo_id branch remote_head missing=0 count=0
  if ! resp="$(native_publish_api_get "/api/v1/workspaces/${workspace_id}/manifest")"; then
    echo "✗ could not read the manifest of workspace ${workspace_id}" >&2
    return 1
  fi
  # `-e` with one joined output: an empty body, a non-manifest or a manifest
  # without members all fail here instead of passing an empty loop below.
  if ! members="$(printf '%s' "$resp" | jq -re '
      .data
      | if type == "object" and (.workspace_id | type == "string") then . else error("no manifest") end
      | [ ([.docs // empty] + (.repos // []))[]
          | if (.repo_id | type) == "string" and (.branch | type) == "string"
            then "\(.repo_id)|\(.branch)"
            else error("manifest member without repo_id and branch") end ]
      | if length == 0 then error("manifest lists no member") else join("\n") end' 2>/dev/null)"; then
    echo "✗ workspace ${workspace_id} returned no usable manifest: ${resp:-<empty body>}" >&2
    return 1
  fi
  while IFS='|' read -r repo_id branch; do
    [ -n "$repo_id" ] || continue
    count=$((count + 1))
    if ! remote_head="$(native_publish_remote_head "$repo_id" "$branch")" || [ -z "$remote_head" ]; then
      echo "✗ manifest member ${repo_id} has no refs/heads/${branch} on the mirror; the workspace is not published partially (workspace branch creation is reserved for the fork service)" >&2
      missing=$((missing + 1))
    fi
  done <<<"$members"
  if [ "$missing" -gt 0 ]; then
    return 1
  fi
  echo "  ✓ every manifest member of workspace ${workspace_id} has its branch on the mirror (${count} checked)"
}
