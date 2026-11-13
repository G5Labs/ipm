#!/usr/bin/env bash
# =============================================================================
# prepare-g5-workspace.sh -- Build a prepared multi-repo workspace for G5 itself
# =============================================================================
# Creates or replaces a local g5_workspace folder in the same shape as the
# prepared compliance workspace:
#
#   g5_workspace/
#     .g5/workspace.yaml
#     g5-docs/  -> faithful selected G5_Docs GitHub branch
#     g5-code/  -> faithful selected G5 GitHub branch (generation stays nested)
#
# GitHub is the source: ignored compiled/runtime artifacts never enter the
# prepared repos, and both prepared HEADs retain their GitHub commit identity.
#
# Usage:
#   ./scripts/prepare-g5-workspace.sh [output-dir]
#
# Defaults:
#   output-dir = ../g5_workspace
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GEN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$GEN_ROOT"

OUT_ARG="${1:-${G5_WORKSPACE_DIR:-$REPO_ROOT/g5_workspace}}"
OUT_PARENT="$(cd "$(dirname "$OUT_ARG")" && pwd)"
OUT_DIR="$OUT_PARENT/$(basename "$OUT_ARG")"

DOCS_REPO_NAME="${G5_WORKSPACE_DOCS_REPO_NAME:-g5-docs}"
CODE_REPO_NAME="${G5_WORKSPACE_CODE_REPO_NAME:-g5-code}"
BRANCH_NAME="${G5_WORKSPACE_BRANCH_NAME:-main}"
DOCS_REPO="$OUT_DIR/$DOCS_REPO_NAME"
CODE_REPO="$OUT_DIR/$CODE_REPO_NAME"
CODE_SOURCE_REPO="${G5_WORKSPACE_CODE_SOURCE_REPO:-$REPO_ROOT}"
DOCS_SOURCE_REPO="${G5_WORKSPACE_DOCS_SOURCE_REPO:-$REPO_ROOT/g5_docs}"
CODE_SOURCE_BRANCH="${G5_WORKSPACE_CODE_SOURCE_BRANCH:-}"
DOCS_SOURCE_BRANCH="${G5_WORKSPACE_DOCS_SOURCE_BRANCH:-}"
CODE_SOURCE_URL=""
DOCS_SOURCE_URL=""
CODE_SOURCE_SHA=""
DOCS_SOURCE_SHA=""

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command not found: $1" >&2
    exit 1
  fi
}

require_command git
require_command python3

case "$OUT_DIR" in
  "/"|"$REPO_ROOT"|"$GEN_ROOT")
    echo "error: refusing to replace unsafe output path: $OUT_DIR" >&2
    exit 1
    ;;
esac

resolve_source_branch() {
  local source_repo="$1"
  local configured_branch="$2"
  if [ -n "$configured_branch" ]; then
    printf '%s\n' "$configured_branch"
    return
  fi
  git -C "$source_repo" branch --show-current
}

resolve_github_remote() {
  local source_repo="$1"
  git -C "$source_repo" remote get-url github 2>/dev/null \
    || git -C "$source_repo" remote get-url origin 2>/dev/null \
    || {
      echo "error: source repo has neither a 'github' nor an 'origin' remote: $source_repo" >&2
      exit 1
    }
}

clone_github_source() {
  local label="$1"
  local source_repo="$2"
  local configured_branch="$3"
  local destination="$4"
  local branch remote_url remote_sha cloned_sha

  git -C "$source_repo" rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
    echo "error: $label source is not a git checkout: $source_repo" >&2
    exit 1
  }
  branch="$(resolve_source_branch "$source_repo" "$configured_branch")"
  [ -n "$branch" ] || {
    echo "error: $label source is detached; set G5_WORKSPACE_${label}_SOURCE_BRANCH" >&2
    exit 1
  }
  remote_url="$(resolve_github_remote "$source_repo")"
  remote_sha="$(git ls-remote "$remote_url" "refs/heads/$branch" | awk 'NR == 1 {print $1}')"
  [ -n "$remote_sha" ] || {
    echo "error: $label branch '$branch' does not exist on $remote_url" >&2
    exit 1
  }

  git clone -q --single-branch --branch "$branch" "$remote_url" "$destination"
  cloned_sha="$(git -C "$destination" rev-parse HEAD)"
  [ "$cloned_sha" = "$remote_sha" ] || {
    echo "error: $label clone HEAD $cloned_sha does not match GitHub $remote_sha" >&2
    exit 1
  }
  git -C "$destination" checkout -q -B "$BRANCH_NAME" "$remote_sha"
  git -C "$destination" remote remove origin
  git -C "$destination" config gc.auto 0
  git -C "$destination" config maintenance.auto false

  case "$label" in
    CODE)
      CODE_SOURCE_URL="$remote_url"
      CODE_SOURCE_BRANCH="$branch"
      CODE_SOURCE_SHA="$remote_sha"
      ;;
    DOCS)
      DOCS_SOURCE_URL="$remote_url"
      DOCS_SOURCE_BRANCH="$branch"
      DOCS_SOURCE_SHA="$remote_sha"
      ;;
  esac
}

validate_spec_source_paths() {
  local validator="${G5_WORKSPACE_G5D_BIN:-}"
  local tmp_dir json_file stderr_file validator_status=0
  if [ -z "$validator" ]; then
    # Without the microVM feature, g5d supports hosts without KVM. A
    # separate target keeps the default-featured build intact, and an
    # inherited target triple would move the binary under it.
    env -u CARGO_BUILD_TARGET CARGO_TARGET_DIR="$REPO_ROOT/target/g5d-validator" \
      cargo build --quiet --manifest-path "$REPO_ROOT/Cargo.toml" -p g5d --no-default-features >&2
    validator="$REPO_ROOT/target/g5d-validator/debug/g5d"
  fi
  [ -x "$validator" ] || {
    echo "error: source-path validator is not executable: $validator" >&2
    exit 1
  }

  tmp_dir="$(mktemp -d)"
  json_file="$tmp_dir/validation.json"
  stderr_file="$tmp_dir/validation.stderr"
  "$validator" -w "$CODE_REPO" spec validate --all --json \
    >"$json_file" 2>"$stderr_file" || validator_status=$?

  if ! python3 - "$json_file" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    payload = json.loads(path.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError) as exc:
    raise SystemExit(f"invalid g5d spec-validation JSON: {exc}")

needle = "source file does not exist"
matches = set()

def visit(value):
    if isinstance(value, dict):
        for child in value.values():
            visit(child)
    elif isinstance(value, list):
        for child in value:
            visit(child)
    elif isinstance(value, str) and needle in value.lower():
        matches.add(value)

visit(payload)
if matches:
    print("Prepared G5 code repo has invalid source_files paths:", file=sys.stderr)
    for match in sorted(matches)[:80]:
        print(f"  - {match}", file=sys.stderr)
    if len(matches) > 80:
        print(f"  ... {len(matches) - 80} more", file=sys.stderr)
    raise SystemExit(1)
PY
  then
    cat "$stderr_file" >&2
    rm -rf "$tmp_dir"
    exit 1
  fi

  if [ "$validator_status" -ne 0 ]; then
    echo "  Source paths valid (g5d reported unrelated ontology diagnostics)"
  else
    echo "  Source paths validated by g5d"
  fi
  rm -rf "$tmp_dir"
}

echo "=== Prepare G5 Workspace ==="
echo "  Output:    $OUT_DIR"
echo "  Docs repo: $DOCS_REPO_NAME"
echo "  Code repo: $CODE_REPO_NAME"
echo ""

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/.g5"

echo "=== Step 1: Clone faithful docs repo from GitHub ==="
clone_github_source "DOCS" "$DOCS_SOURCE_REPO" "$DOCS_SOURCE_BRANCH" "$DOCS_REPO"
echo "  Source: $DOCS_SOURCE_URL"
echo "  Branch: $DOCS_SOURCE_BRANCH"
echo "  SHA:    $DOCS_SOURCE_SHA"

echo ""
echo "=== Step 2: Clone faithful code repo from GitHub ==="
clone_github_source "CODE" "$CODE_SOURCE_REPO" "$CODE_SOURCE_BRANCH" "$CODE_REPO"
echo "  Source: $CODE_SOURCE_URL"
echo "  Branch: $CODE_SOURCE_BRANCH"
echo "  SHA:    $CODE_SOURCE_SHA"

if [ ! -f "$CODE_REPO/semlift.yaml" ]; then
  echo "error: code repo is missing semlift.yaml" >&2
  exit 1
fi
if [ ! -d "$CODE_REPO/.semlift/specs" ]; then
  echo "error: code repo is missing .semlift/specs" >&2
  exit 1
fi
if [ ! -d "$CODE_REPO/.g5/experience" ]; then
  echo "error: code repo is missing .g5/experience" >&2
  exit 1
fi
if [ -d "$CODE_REPO/backend/g5-api" ]; then
  source_paths=(frontend backend/g5-api backend/g5d)
else
  source_paths=(generation/client generation/g5-api generation/g5d)
fi
for required_path in \
  .g5/preview_profiles/full-stack.yaml \
  "${source_paths[@]}"
do
  if [ ! -e "$CODE_REPO/$required_path" ]; then
    echo "error: faithful code repo is missing $required_path" >&2
    exit 1
  fi
done
if [ -e "$CODE_REPO/g5_docs" ]; then
  echo "error: code repo contains g5_docs; docs must remain an independent repo" >&2
  exit 1
fi

echo ""
echo "=== Step 3: Validate ontology source paths ==="
validate_spec_source_paths

echo ""
echo "=== Step 4: Preserve verified GitHub commits ==="
DOCS_HEAD="$(git -C "$DOCS_REPO" rev-parse HEAD)"
CODE_HEAD="$(git -C "$CODE_REPO" rev-parse HEAD)"
if [ "$DOCS_HEAD" != "$DOCS_SOURCE_SHA" ] || [ "$CODE_HEAD" != "$CODE_SOURCE_SHA" ]; then
  echo "error: prepared repo HEAD drifted after GitHub verification" >&2
  exit 1
fi
echo "  Docs HEAD: $DOCS_HEAD"
echo "  Code HEAD: $CODE_HEAD"

WORKSPACE_ID="$(
  python3 - "$OUT_DIR" <<'PY'
import sys
import uuid

print(uuid.uuid5(uuid.NAMESPACE_URL, "file://" + sys.argv[1]))
PY
)"

cat > "$OUT_DIR/.g5/workspace.yaml" <<EOF
workspace_id: "$WORKSPACE_ID"
remote:
  enabled: true
  api_url: http://localhost:${G5_PORT_API:-7500}
repos:
- name: $DOCS_REPO_NAME
  url: file://$DOCS_REPO
  branch: $BRANCH_NAME
  path: $DOCS_REPO_NAME
- name: $CODE_REPO_NAME
  url: file://$CODE_REPO
  branch: $BRANCH_NAME
  path: $CODE_REPO_NAME
EOF

echo ""
echo "Prepared G5 workspace complete."
echo "  Workspace: $OUT_DIR"
echo "  workspace.yaml: $OUT_DIR/.g5/workspace.yaml"
echo "  Docs source: $DOCS_SOURCE_URL#$DOCS_SOURCE_BRANCH@$DOCS_SOURCE_SHA"
echo "  Code source: $CODE_SOURCE_URL#$CODE_SOURCE_BRANCH@$CODE_SOURCE_SHA"
echo "  Docs repo files: $(git -C "$DOCS_REPO" ls-files | wc -l | tr -d ' ')"
echo "  Code repo files: $(git -C "$CODE_REPO" ls-files | wc -l | tr -d ' ')"
