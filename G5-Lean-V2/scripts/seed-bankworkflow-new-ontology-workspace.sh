#!/usr/bin/env bash
# Seed the BankWorkflow v4 code repo into an existing local G5 workspace.
#
# This intentionally does not seed docs, views, chat records, or code/view
# grounding. It writes the real BankWorkflow source tree plus ontology markdown
# and ontology intent/grounding JSONL into the existing code repo so the public
# trace routes can resolve parent/child links from committed repo content.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GEN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$GEN_ROOT"

REPOS_VOLUME="${REPOS_VOLUME:-g5-lean_api_repos}"
COPY_IMAGE="${G5_BANKWORKFLOW_COPY_IMAGE:-${G5_COMPLIANCE_COPY_IMAGE:-g5/g5d-base:latest}}"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
DB_NAME="${DB_NAME:-g5_dev}"

TARGET_WORKSPACE="${G5_BANKWORKFLOW_WORKSPACE:-BankWorkflow}"
SOURCE_ROOT="${G5_BANKWORKFLOW_NEW_ONTOLOGY_ROOT:-$WORKSPACE_ROOT/bankworkflow-workspace/bankworkflow-code}"
BRANCH_NAME="${G5_BANKWORKFLOW_BRANCH_NAME:-}"
CODE_REPO_SLUG="${G5_BANKWORKFLOW_CODE_REPO_SLUG:-bankworkflow-code}"
COMMIT_MESSAGE="${G5_BANKWORKFLOW_NEW_ONTOLOGY_COMMIT_MESSAGE:-Seed BankWorkflow v4 ontology}"
PREPARE_ONLY="${G5_BANKWORKFLOW_PREPARE_ONLY:-false}"
GENERATED_AT="${G5_BANKWORKFLOW_GENERATED_AT:-2026-06-22T00:00:00Z}"
COPY_CODE="${G5_BANKWORKFLOW_COPY_CODE:-true}"

usage() {
  cat <<'EOF'
Usage:
  ./scripts/seed-bankworkflow-new-ontology-workspace.sh [--prepare-only] [workspace-name-or-id] [source-root]

Defaults:
  workspace-name-or-id  $G5_BANKWORKFLOW_WORKSPACE or BankWorkflow
  source-root           $G5_BANKWORKFLOW_NEW_ONTOLOGY_ROOT or ../bankworkflow-workspace/bankworkflow-code

Accepted source layouts:
  Repo-shaped:
    <source-root>/.semlift/specs/**
    <source-root>/intent_record/ontology/**
    <source-root>/intent_grounding/ontology/**

  Raw BankWorkflow v4:
    <source-root>/**/*.g5.md                         (for example BankWorkflow/new_ontology)
    <source-root>/../new_grounding/ontology/**       (override with G5_BANKWORKFLOW_NEW_GROUNDING_ROOT)
    <source-root>/../TRANSPILED_CODE/**              (override with G5_BANKWORKFLOW_CODE_SOURCE_ROOT)
    intent records are generated from current I/D/Q labels

Environment:
  G5_BANKWORKFLOW_BRANCH_NAME       Optional assertion of the workspace-selected Git ref
  G5_BANKWORKFLOW_CODE_REPO_SLUG    Preferred code repo slug (default: bankworkflow-code)
  G5_BANKWORKFLOW_CODE_SOURCE_ROOT  Optional code source tree override
  G5_BANKWORKFLOW_COPY_CODE         true/false; copy filtered code source tree (default: true)
  G5_BANKWORKFLOW_NEW_INTENT_ROOT   Optional ontology intent JSONL source override
  G5_BANKWORKFLOW_NEW_GROUNDING_ROOT Optional ontology grounding JSONL source override
  G5_BANKWORKFLOW_GENERATE_INTENT   true/false; default false for repo-shaped, true for raw v4
  REPOS_VOLUME                      Managed repo Docker volume (default: g5-lean_api_repos)
  PG_CONTAINER                      Postgres container (default: g5-dev-postgres)
  DB_NAME                           Database name (default: g5_dev)

The seed writes:
  filtered source files from the configured code source tree
  .semlift/specs/**
  semlift.yaml
  intent_record/ontology/**
  intent_grounding/ontology/**
EOF
}

POSITIONAL_COUNT=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    --prepare-only)
      PREPARE_ONLY=true
      shift
      ;;
    --*)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ "$POSITIONAL_COUNT" -eq 0 ]]; then
        TARGET_WORKSPACE="$1"
      elif [[ "$POSITIONAL_COUNT" -eq 1 ]]; then
        SOURCE_ROOT="$1"
      else
        echo "Unexpected extra argument: $1" >&2
        usage >&2
        exit 2
      fi
      POSITIONAL_COUNT=$((POSITIONAL_COUNT + 1))
      shift
      ;;
  esac
done

SOURCE_ROOT="$(cd "$SOURCE_ROOT" && pwd)"
SOURCE_LAYOUT=""
GENERATE_INTENT="${G5_BANKWORKFLOW_GENERATE_INTENT:-}"

if [[ -d "$SOURCE_ROOT/.semlift/specs" ]]; then
  SOURCE_LAYOUT="repo"
  ONTOLOGY_SOURCE="$SOURCE_ROOT/.semlift/specs"
  INTENT_SOURCE="${G5_BANKWORKFLOW_NEW_INTENT_ROOT:-$SOURCE_ROOT/intent_record/ontology}"
  GROUNDING_SOURCE="${G5_BANKWORKFLOW_NEW_GROUNDING_ROOT:-$SOURCE_ROOT/intent_grounding/ontology}"
  if [[ -z "$GENERATE_INTENT" ]]; then
    GENERATE_INTENT=false
  fi
elif find "$SOURCE_ROOT" -type f -name '*.g5.md' -print -quit | grep -q .; then
  SOURCE_LAYOUT="raw_v4"
  ONTOLOGY_SOURCE="$SOURCE_ROOT"
  SOURCE_PARENT="$(cd "$SOURCE_ROOT/.." && pwd)"
  INTENT_SOURCE="${G5_BANKWORKFLOW_NEW_INTENT_ROOT:-}"
  GROUNDING_SOURCE="${G5_BANKWORKFLOW_NEW_GROUNDING_ROOT:-$SOURCE_PARENT/new_grounding/ontology}"
  if [[ -z "$GENERATE_INTENT" ]]; then
    if [[ -n "${G5_BANKWORKFLOW_NEW_INTENT_ROOT:-}" ]]; then
      GENERATE_INTENT=false
    else
      GENERATE_INTENT=true
    fi
  fi
else
  echo "ERROR: Source root is neither repo-shaped nor a raw v4 ontology tree: $SOURCE_ROOT" >&2
  usage >&2
  exit 1
fi

CODE_SOURCE_ROOT="${G5_BANKWORKFLOW_CODE_SOURCE_ROOT:-}"
if [[ -z "$CODE_SOURCE_ROOT" && -n "${SOURCE_PARENT:-}" && -d "$SOURCE_PARENT/TRANSPILED_CODE" ]]; then
  CODE_SOURCE_ROOT="$SOURCE_PARENT/TRANSPILED_CODE"
elif [[ -z "$CODE_SOURCE_ROOT" && "$SOURCE_LAYOUT" == "repo" ]]; then
  CODE_SOURCE_ROOT="$SOURCE_ROOT"
fi
if [[ -n "$CODE_SOURCE_ROOT" ]]; then
  CODE_SOURCE_ROOT="$(cd "$CODE_SOURCE_ROOT" && pwd)"
fi

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

sql_literal() {
  local value="$1"
  value="${value//\'/\'\'}"
  printf "'%s'" "$value"
}

psql_exec() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -At -F $'\t' -v ON_ERROR_STOP=1 -c "$1"
}

psql_exec_quiet() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$1" >/dev/null
}

require_source_tree() {
  [[ -d "$ONTOLOGY_SOURCE" ]] || fail "Missing ontology source directory: $ONTOLOGY_SOURCE"
  if [[ "$GENERATE_INTENT" != "true" ]]; then
    [[ -d "$INTENT_SOURCE" ]] || fail "Missing ontology intent directory: $INTENT_SOURCE"
  fi
  [[ -d "$GROUNDING_SOURCE" ]] || fail "Missing ontology grounding directory: $GROUNDING_SOURCE"
  if [[ "$COPY_CODE" == "true" ]]; then
    [[ -n "$CODE_SOURCE_ROOT" ]] || fail "Missing code source root. Set G5_BANKWORKFLOW_CODE_SOURCE_ROOT or G5_BANKWORKFLOW_COPY_CODE=false."
    [[ -d "$CODE_SOURCE_ROOT" ]] || fail "Missing code source directory: $CODE_SOURCE_ROOT"
  fi
}

prepare_payload() {
  local stage_dir="$1"
  local stats_file="$2"

  python3 - "$SOURCE_ROOT" "$ONTOLOGY_SOURCE" "$INTENT_SOURCE" "$GROUNDING_SOURCE" \
    "$CODE_SOURCE_ROOT" "$COPY_CODE" "$SOURCE_LAYOUT" "$GENERATE_INTENT" "$GENERATED_AT" \
    "$stage_dir" "$stats_file" <<'PY'
import hashlib
import json
import os
import re
import shutil
import sys
from collections import defaultdict, deque
from pathlib import Path

source_root = Path(sys.argv[1])
ontology_source = Path(sys.argv[2])
intent_source_arg = sys.argv[3]
grounding_source = Path(sys.argv[4])
code_source_arg = sys.argv[5]
copy_code = sys.argv[6] == "true"
source_layout = sys.argv[7]
generate_intent = sys.argv[8] == "true"
generated_at = sys.argv[9]
stage_dir = Path(sys.argv[10])
stats_file = Path(sys.argv[11])

intent_source = Path(intent_source_arg) if intent_source_arg else None
code_source = Path(code_source_arg) if code_source_arg else None

CODE_EXCLUDE_DIRS = {
    ".angular",
    ".git",
    ".idea",
    ".vscode",
    "coverage",
    "dist",
    "logs",
    "node_modules",
    "target",
    "tmp",
}
CODE_EXCLUDE_FILES = {
    ".DS_Store",
    "jacoco.exec",
    "temp.java",
}
CODE_EXCLUDE_SUFFIXES = (
    ".class",
    ".log",
    ".log.gz",
)

def should_skip_code_path(path):
    parts = set(path.parts)
    if parts & CODE_EXCLUDE_DIRS:
        return True
    if path.name in CODE_EXCLUDE_FILES:
        return True
    return path.name.endswith(CODE_EXCLUDE_SUFFIXES)

def encode_file_name(source_name):
    return source_name.replace("/", "__")

def parse_scalar(value):
    value = value.strip()
    if value == "" or value == "[]":
        return [] if value == "[]" else ""
    if value.lower() == "null":
        return None
    if value.lower() == "true":
        return True
    if value.lower() == "false":
        return False
    if (value.startswith('"') and value.endswith('"')) or (
        value.startswith("'") and value.endswith("'")
    ):
        return value[1:-1]
    return value

def parse_frontmatter(path):
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        raise ValueError(f"{path}: missing YAML frontmatter")
    try:
        raw = text.split("\n---\n", 1)[0][4:]
    except IndexError as exc:
        raise ValueError(f"{path}: malformed YAML frontmatter") from exc

    lines = raw.splitlines()
    data = {}
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip() or line.lstrip().startswith("#"):
            i += 1
            continue
        match = re.match(r"^([A-Za-z0-9_]+):\s*(.*)$", line)
        if not match:
            i += 1
            continue
        key, rest = match.group(1), match.group(2)
        if rest.strip():
            data[key] = parse_scalar(rest)
            i += 1
            continue
        items = []
        i += 1
        while i < len(lines):
            child = lines[i]
            item_match = re.match(r"^\s*-\s*(.*)$", child)
            if not item_match:
                break
            items.append(parse_scalar(item_match.group(1)))
            i += 1
        data[key] = items
    return data

def component_from_ontology_file(file_name):
    if file_name.endswith(".g5.md"):
        return file_name[: -len(".g5.md")]
    if file_name.endswith(".semlift.md"):
        cleaned = file_name
        if cleaned.startswith(".semlift/specs/"):
            cleaned = cleaned[len(".semlift/specs/") :]
        return cleaned[: -len(".semlift.md")]
    return None

def repo_path_for_component(component_id, component_to_rel):
    rel = component_to_rel.get(component_id)
    if rel is None:
        return None
    return f".semlift/specs/{rel}"

def relevance_to_int(value):
    if value is None:
        return None
    try:
        numeric = float(value)
    except (TypeError, ValueError):
        return None
    if numeric <= 1:
        numeric *= 100
    return max(0, min(100, int(round(numeric))))

def normalized_rel_for_source(path):
    rel = path.relative_to(ontology_source).as_posix()
    if rel.endswith(".g5.md"):
        return rel[: -len(".g5.md")] + ".semlift.md"
    if rel.endswith(".semlift.md"):
        return rel
    raise ValueError(f"{path}: unsupported ontology suffix")

def source_file_for_component(component_id):
    rel = component_to_source_rel.get(component_id)
    if rel is None:
        return None
    return ontology_source / rel

LABEL_RE = re.compile(r"^\s*\[((?:I|D|Q)[0-9A-Za-z_-]*)\.[^\]]*\]")

def extract_intent_records(path, repo_path):
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    starts = []
    in_human = False
    for index, line in enumerate(lines):
        if re.search(r"<human\b", line):
            in_human = True
        match = LABEL_RE.match(line)
        if match:
            starts.append((index, match.group(1), in_human or "<human" in line))
        if "</human>" in line:
            in_human = False

    seen = set()
    records = []
    for item_index, (start, label, is_human) in enumerate(starts):
        if label in seen:
            raise ValueError(f"{path}: duplicate intent label {label}")
        seen.add(label)
        end = starts[item_index + 1][0] if item_index + 1 < len(starts) else len(lines)
        content = "\n".join(lines[start:end]).strip()
        record = {
            "file_type": "ontology",
            "file_name": repo_path,
            "intent_record_number": label,
            "is_human": is_human,
            "created_by": "Codex",
            "created_at": generated_at,
        }
        if content:
            record["content"] = content
        records.append(record)
    return records

if source_layout == "raw_v4":
    ontology_files = sorted(ontology_source.rglob("*.g5.md"))
else:
    ontology_files = sorted(ontology_source.rglob("*.semlift.md"))
if not ontology_files:
    raise SystemExit(f"No ontology files found under {ontology_source}")

nodes = []
component_to_rel = {}
component_to_source_rel = {}
for path in ontology_files:
    front = parse_frontmatter(path)
    component_id = front.get("component_id")
    if not isinstance(component_id, str) or not component_id:
        raise ValueError(f"{path}: missing component_id")
    rel = normalized_rel_for_source(path)
    source_rel = path.relative_to(ontology_source).as_posix()
    if component_id in component_to_rel:
        raise ValueError(f"Duplicate component_id: {component_id}")
    component_to_rel[component_id] = rel
    component_to_source_rel[component_id] = source_rel
    nodes.append(
        {
            "component_id": component_id,
            "title": front.get("title") or component_id.rsplit("/", 1)[-1],
            "parent_spec_id": front.get("parent_spec_id"),
            "child_spec_ids": front.get("child_spec_ids") or [],
            "dependencies": front.get("dependencies") or [],
            "rel": rel,
            "repo_path": f".semlift/specs/{rel}",
            "content_hash": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
    )

components = set(component_to_rel)
for node in nodes:
    parent = node["parent_spec_id"]
    if parent and parent not in components:
        raise ValueError(
            f"{node['component_id']}: parent_spec_id {parent!r} has no matching ontology file"
        )
    for child in node["child_spec_ids"]:
        if child not in components:
            raise ValueError(
                f"{node['component_id']}: child_spec_id {child!r} has no matching ontology file"
            )
    for dep in node["dependencies"]:
        if dep not in components:
            raise ValueError(
                f"{node['component_id']}: dependency {dep!r} has no matching ontology file"
            )

stage_ontology = stage_dir / ".semlift" / "specs"
if stage_ontology.exists():
    shutil.rmtree(stage_ontology)
stage_ontology.mkdir(parents=True, exist_ok=True)
for path in ontology_files:
    rel = normalized_rel_for_source(path)
    target = stage_ontology / rel
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, target)

source_semlift_config = source_root / "semlift.yaml"
if source_semlift_config.exists():
    shutil.copy2(source_semlift_config, stage_dir / "semlift.yaml")

code_file_count = 0
if copy_code:
    if code_source is None or not code_source.is_dir():
        raise ValueError(f"Missing code source directory: {code_source}")
    stage_code = stage_dir / "code"
    if stage_code.exists():
        shutil.rmtree(stage_code)
    stage_code.mkdir(parents=True, exist_ok=True)
    for root, dirs, files in os.walk(code_source):
        root_path = Path(root)
        rel_root = root_path.relative_to(code_source)
        dirs[:] = [
            name
            for name in sorted(dirs)
            if not should_skip_code_path(rel_root / name)
        ]
        for dirname in dirs:
            (stage_code / rel_root / dirname).mkdir(parents=True, exist_ok=True)
        for filename in sorted(files):
            rel = rel_root / filename
            if should_skip_code_path(rel):
                continue
            path = root_path / filename
            target = stage_code / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)
            code_file_count += 1

stage_intent = stage_dir / "intent_record" / "ontology"
stage_grounding = stage_dir / "intent_grounding" / "ontology"
stage_intent.mkdir(parents=True, exist_ok=True)
stage_grounding.mkdir(parents=True, exist_ok=True)

intent_records_by_repo_path = defaultdict(set)
intent_lines_by_repo_path = defaultdict(list)
if generate_intent:
    for node in nodes:
        source_file = source_file_for_component(node["component_id"])
        records = extract_intent_records(source_file, node["repo_path"])
        if not records:
            raise ValueError(f"{source_file}: no I/D/Q labels found for generated intent records")
        for record in records:
            label = record["intent_record_number"]
            intent_records_by_repo_path[node["repo_path"]].add(label)
            intent_lines_by_repo_path[node["repo_path"]].append(record)
else:
    for path in sorted(intent_source.glob("*.jsonl")):
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                continue
            record = json.loads(line)
            raw_file_name = record.get("file_name")
            component_id = component_from_ontology_file(raw_file_name or "") or raw_file_name
            repo_path = repo_path_for_component(component_id, component_to_rel)
            if repo_path is None:
                raise ValueError(f"{path}:{line_no}: unknown intent component {raw_file_name!r}")
            label = record.get("intent_record_number")
            if not label:
                raise ValueError(f"{path}:{line_no}: missing intent_record_number")
            record["file_type"] = "ontology"
            record["file_name"] = repo_path
            intent_records_by_repo_path[repo_path].add(label)
            intent_lines_by_repo_path[repo_path].append(record)

missing_intent_files = [
    node["component_id"]
    for node in nodes
    if node["repo_path"] not in intent_records_by_repo_path
]
if missing_intent_files:
    raise ValueError(
        "Missing ontology intent records for: " + ", ".join(missing_intent_files[:20])
    )

for repo_path, records in sorted(intent_lines_by_repo_path.items()):
    out_path = stage_intent / f"{encode_file_name(repo_path)}.jsonl"
    with out_path.open("w", encoding="utf-8") as fh:
        for record in records:
            fh.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")

grounding_lines_by_target = defaultdict(list)
skipped_non_ontology_source = 0
grounding_edges_seen = set()
for path in sorted(grounding_source.glob("*.jsonl")):
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        edge = json.loads(line)
        source_component = component_from_ontology_file(edge.get("source_file", ""))
        target_component = component_from_ontology_file(edge.get("target_file", ""))
        if target_component not in components:
            raise ValueError(f"{path}:{line_no}: unknown target ontology file {edge.get('target_file')!r}")
        if source_component not in components:
            skipped_non_ontology_source += 1
            continue

        source_repo_path = repo_path_for_component(source_component, component_to_rel)
        target_repo_path = repo_path_for_component(target_component, component_to_rel)
        source_id = edge.get("source_id")
        target_id = edge.get("target_id")
        if source_id not in intent_records_by_repo_path[source_repo_path]:
            raise ValueError(f"{path}:{line_no}: source block {source_repo_path}#{source_id} has no intent record")
        if target_id not in intent_records_by_repo_path[target_repo_path]:
            raise ValueError(f"{path}:{line_no}: target block {target_repo_path}#{target_id} has no intent record")

        normalized = {
            "source_file": source_repo_path,
            "source_file_type": "ontology",
            "target_file": target_repo_path,
            "target_file_type": "ontology",
            "source_id": source_id,
            "target_id": target_id,
        }
        if edge.get("relationship"):
            normalized["relationship"] = edge["relationship"]
        relevance = relevance_to_int(edge.get("relevance", edge.get("relevance_score")))
        if relevance is not None:
            normalized["relevance"] = relevance
        if edge.get("rationale"):
            normalized["rationale"] = edge["rationale"]
        if edge.get("created_by"):
            normalized["created_by"] = edge["created_by"]
        if edge.get("created_at"):
            normalized["created_at"] = edge["created_at"]

        key = (
            normalized["source_file"],
            normalized["source_id"],
            normalized["target_file"],
            normalized["target_id"],
            normalized.get("relationship"),
        )
        if key in grounding_edges_seen:
            continue
        grounding_edges_seen.add(key)
        grounding_lines_by_target[target_repo_path].append(normalized)

for repo_path in sorted(intent_records_by_repo_path):
    out_path = stage_grounding / f"{encode_file_name(repo_path)}.jsonl"
    with out_path.open("w", encoding="utf-8") as fh:
        for edge in grounding_lines_by_target.get(repo_path, []):
            fh.write(json.dumps(edge, ensure_ascii=False, sort_keys=True) + "\n")

indegree = {node["component_id"]: 0 for node in nodes}
children = defaultdict(list)
for node in nodes:
    parent = node["parent_spec_id"]
    if parent:
        indegree[node["component_id"]] += 1
        children[parent].append(node["component_id"])

queue = deque(sorted([cid for cid, degree in indegree.items() if degree == 0]))
ordered_components = []
while queue:
    cid = queue.popleft()
    ordered_components.append(cid)
    for child in sorted(children[cid]):
        indegree[child] -= 1
        if indegree[child] == 0:
            queue.append(child)
if len(ordered_components) != len(nodes):
    raise ValueError("Ontology parent graph contains a cycle")

stats = {
    "source_layout": source_layout,
    "code_source": str(code_source) if code_source else "",
    "copied_code": copy_code,
    "code_files": code_file_count,
    "generated_intent": generate_intent,
    "ontology_files": len(ontology_files),
    "intent_files": len(intent_lines_by_repo_path),
    "intent_records": sum(len(v) for v in intent_lines_by_repo_path.values()),
    "grounding_files": len(intent_records_by_repo_path),
    "ontology_grounding_edges": sum(len(v) for v in grounding_lines_by_target.values()),
    "skipped_non_ontology_grounding_edges": skipped_non_ontology_source,
    "stage_dir": str(stage_dir),
}
stats_file.write_text(json.dumps(stats, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

copy_payload_to_repo() {
  local stage_dir="$1"
  local org_id="$2"
  local repo_id="$3"
  local repo_branch="$4"
  local commit_message="$5"

  docker run --rm \
    --user 0:0 \
    -v "${REPOS_VOLUME}:/data/repos" \
    -v "${stage_dir}:/payload:ro" \
    "${COPY_IMAGE}" sh -euc '
      org_id="$1"
      repo_id="$2"
      branch="$3"
      commit_message="$4"
      base="/data/repos/${org_id}/${repo_id}"
      bare="/data/repos/${org_id}/${repo_id}.bare"

      chmod_repo_tree() {
        target="$1"
        [ -e "$target" ] || return 0
        chmod -R a+rwX "$target" 2>/dev/null || true
      }

      git config --global --add safe.directory "*"

      if [ ! -d "$base/.git" ]; then
        if [ -d "$bare" ]; then
          rm -rf "$base"
          git clone -q "$bare" "$base"
        else
          echo "Managed repo working tree was not found: $base" >&2
          echo "Run the full BankWorkflow workspace seed first, then rerun this ontology-only seed." >&2
          exit 1
        fi
      fi

      cd "$base"
      git config user.email "${G5_BANKWORKFLOW_SEED_EMAIL:-demo@g5labs.ai}"
      git config user.name "G5 BankWorkflow Ontology Seed"

      if git rev-parse --git-dir >/dev/null 2>&1; then
        git fetch -q origin "$branch" 2>/dev/null || true
      fi
      if git rev-parse --verify "refs/remotes/origin/${branch}^{commit}" >/dev/null 2>&1; then
        git checkout -q -B "$branch" "origin/${branch}"
      else
        git checkout -q -B "$branch"
      fi

      if [ -d /payload/code ]; then
        find . -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
        cp -R /payload/code/. .
      else
        rm -rf new_ontology
        rm -rf intent_record intent_grounding
        rm -rf .semlift/specs core_ontology
        rm -f ./*.semlift.md
      fi

      mkdir -p .semlift/specs intent_record intent_grounding
      cp -R /payload/.semlift/specs/. .semlift/specs/
      mkdir -p intent_record/ontology intent_grounding/ontology
      cp -R /payload/intent_record/ontology/. intent_record/ontology/
      cp -R /payload/intent_grounding/ontology/. intent_grounding/ontology/
      if [ -f /payload/semlift.yaml ]; then
        cp /payload/semlift.yaml semlift.yaml
      fi

      git add -A .
      tracked_semlift="$(git ls-files "*.semlift.md" || true)"
      if [ -n "$tracked_semlift" ]; then
        printf "%s\n" "$tracked_semlift" | xargs git add -A --
      fi
      find . -maxdepth 1 -name "*.semlift.md" -exec git add -A {} +

      if git diff --cached --quiet && git diff --quiet; then
        head="$(git rev-parse HEAD)"
      else
        git commit -q -m "$commit_message"
        head="$(git rev-parse HEAD)"
        if git remote get-url origin >/dev/null 2>&1; then
          git push -q -u origin "HEAD:${branch}"
          if [ -d "$bare" ]; then
            git --git-dir="$bare" symbolic-ref HEAD "refs/heads/${branch}" 2>/dev/null || true
          fi
        fi
      fi

      chmod_repo_tree "$base"
      chmod_repo_tree "$bare"
      printf "%s\n" "$head"
    ' sh "$org_id" "$repo_id" "$repo_branch" "$commit_message" | tail -n 1 | tr -d '[:space:]'
}

update_env_test_seed_sha() {
  local env_file="$GEN_ROOT/.env.test"
  local workspace_id="$1"
  local new_head="$2"

  [[ -f "$env_file" ]] || return 0
  if ! grep -Fxq "G5_E2E_TEST_WORKSPACE_ID=${workspace_id}" "$env_file"; then
    return 0
  fi

  python3 - "$env_file" "$new_head" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
new_head = sys.argv[2]
lines = path.read_text(encoding="utf-8").splitlines()
updated = False
for index, line in enumerate(lines):
    if line.startswith("G5_E2E_TEST_SEED_SHA="):
        lines[index] = f"G5_E2E_TEST_SEED_SHA={new_head}"
        updated = True
        break
if not updated:
    lines.append(f"G5_E2E_TEST_SEED_SHA={new_head}")
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
  echo "  updated env:    $env_file"
}

require_source_tree

STAGE_DIR="$(mktemp -d)"
STATS_FILE="$(mktemp)"
trap 'rm -rf "$STAGE_DIR" "$STATS_FILE"' EXIT

echo "=== BankWorkflow v4 ontology seed ==="
echo "  Workspace:   $TARGET_WORKSPACE"
echo "  Source:      $SOURCE_ROOT"
if [[ "$COPY_CODE" == "true" ]]; then
  echo "  Code:        $CODE_SOURCE_ROOT"
fi
echo "  Branch:      $BRANCH_NAME"
echo "  Repo slug:   $CODE_REPO_SLUG"

prepare_payload "$STAGE_DIR" "$STATS_FILE"
python3 - "$STATS_FILE" <<'PY'
import json
import sys
stats = json.loads(open(sys.argv[1], encoding="utf-8").read())
print("Prepared seed payload:")
print(f"  source layout:    {stats['source_layout']}")
print(f"  code source:      {stats['code_source'] or '(none)'}")
print(f"  copied code:      {stats['copied_code']} ({stats['code_files']} files)")
print(f"  generated intent: {stats['generated_intent']}")
print(f"  ontology files:  {stats['ontology_files']}")
print(f"  intent files:    {stats['intent_files']}")
print(f"  intent records:  {stats['intent_records']}")
print(f"  grounding files: {stats['grounding_files']}")
print(f"  ontology edges:  {stats['ontology_grounding_edges']}")
print(f"  skipped non-ontology source edges: {stats['skipped_non_ontology_grounding_edges']}")
PY

if [[ "$PREPARE_ONLY" == "true" ]]; then
  echo "Prepare-only mode complete. Stage directory: $STAGE_DIR"
  trap - EXIT
  exit 0
fi

command -v docker >/dev/null 2>&1 || fail "docker is required for this seed script"

workspace_literal="$(sql_literal "$TARGET_WORKSPACE")"
branch_literal="$(sql_literal "$BRANCH_NAME")"
repo_slug_literal="$(sql_literal "$CODE_REPO_SLUG")"

WS_ID="$(psql_exec "SELECT id::text FROM workspaces WHERE id::text = ${workspace_literal} OR lower(name) = lower(${workspace_literal}) ORDER BY created_at DESC LIMIT 1;" | tr -d '[:space:]')"
[[ -n "$WS_ID" ]] || fail "Workspace not found by id/name: $TARGET_WORKSPACE"

MEMBER_ROW="$(psql_exec "
SELECT member.org_id::text, member.repo_id::text, repo.slug,
       CASE WHEN workspace.is_base THEN repo.default_branch ELSE workspace.name END
FROM workspace_repos member
JOIN workspaces workspace ON workspace.id = member.workspace_id AND workspace.org_id = member.org_id
JOIN repos repo ON repo.id = member.repo_id AND repo.org_id = member.org_id
WHERE member.workspace_id = '${WS_ID}' AND member.role = 'code'
  AND repo.slug = ${repo_slug_literal};
")"
[[ -n "$MEMBER_ROW" ]] || fail "Code repository '$CODE_REPO_SLUG' is not a member of workspace $WS_ID"
IFS=$'\t' read -r ORG_ID REPO_ID RESOLVED_REPO_SLUG REPO_BRANCH_NAME <<<"$MEMBER_ROW"
[[ -z "$BRANCH_NAME" || "$BRANCH_NAME" = "$REPO_BRANCH_NAME" ]] || fail "Requested branch does not match workspace-selected ref $REPO_BRANCH_NAME"
echo "Resolved target: workspace=$WS_ID repo=$REPO_ID ref=$REPO_BRANCH_NAME"

NEW_HEAD="$(copy_payload_to_repo "$STAGE_DIR" "$ORG_ID" "$REPO_ID" "$REPO_BRANCH_NAME" "$COMMIT_MESSAGE")"
[[ ${#NEW_HEAD} -eq 40 ]] || fail "Could not resolve new repo HEAD after ontology seed"

update_env_test_seed_sha "$WS_ID" "$NEW_HEAD"

EXPECTED_SPEC_COUNT="$(python3 - "$STATS_FILE" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh)["ontology_files"])
PY
)"
SPEC_FILE_COUNT="$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" "${COPY_IMAGE}" sh -euc 'git -c safe.directory="*" -C "/data/repos/$1/$2" ls-tree -r --name-only "$3" -- .semlift/specs | sed -n "/\\.semlift\\.md$/p" | wc -l' sh "$ORG_ID" "$REPO_ID" "$NEW_HEAD" | tr -d '[:space:]')"
[[ "$SPEC_FILE_COUNT" == "$EXPECTED_SPEC_COUNT" ]] || fail "Expected ${EXPECTED_SPEC_COUNT} committed SemLift files, found ${SPEC_FILE_COUNT}"
TRACE_RECORD_COUNT="$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" "${COPY_IMAGE}" sh -euc "find /data/repos/${ORG_ID}/${REPO_ID}/intent_record/ontology -type f -name '*.jsonl' | wc -l" | tr -d '[:space:]')"
TRACE_GROUNDING_COUNT="$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" "${COPY_IMAGE}" sh -euc "find /data/repos/${ORG_ID}/${REPO_ID}/intent_grounding/ontology -type f -name '*.jsonl' | wc -l" | tr -d '[:space:]')"

echo "Seed complete."
echo "  new head:       $NEW_HEAD"
echo "  spec files:     $SPEC_FILE_COUNT"
echo "  intent JSONL:   $TRACE_RECORD_COUNT"
echo "  grounding JSONL:$TRACE_GROUNDING_COUNT"
