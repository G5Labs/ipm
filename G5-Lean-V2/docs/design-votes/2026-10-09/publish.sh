#!/usr/bin/env bash
# Publish the design-vote page into a G5 workspace as a managed explainer.
#
# Creates the question set through the owner's question route (an explicit
# Save that commits and pushes docs/<workspace>/questions/<set>.g5q.json),
# then commits the page as explainers/design-votes-2026-10-09.html in the
# docs repository, with every placeholder pointed at that set. Run it as the
# workspace owner while the workspace is running.
#
#   G5_JWT=... ./publish.sh --base https://dev.g5labs.ai --workspace <workspace-id>
#
# Options: --base <api base url> (default http://localhost:3000)
#          --workspace <workspace uuid> (required)
#          --jwt <bearer> (default $G5_JWT)
set -euo pipefail

BASE="http://localhost:3000"
WORKSPACE=""
JWT="${G5_JWT:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --base) BASE="$2"; shift 2 ;;
    --workspace) WORKSPACE="$2"; shift 2 ;;
    --jwt) JWT="$2"; shift 2 ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$WORKSPACE" ] || { echo "--workspace is required" >&2; exit 2; }
[ -n "$JWT" ] || { echo "set G5_JWT or pass --jwt" >&2; exit 2; }

HERE="$(cd "$(dirname "$0")" && pwd)"
PAGE="$HERE/design-votes-2026-10-09.html"
QUESTIONS="$HERE/questions.json"
SET_ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["setId"])' "$QUESTIONS")"
SET_PATH="docs/$WORKSPACE/questions/$SET_ID.g5q.json"
HTML_PATH="docs/$WORKSPACE/explainers/$SET_ID.html"

api() { # method path [json-file]
  local method="$1" path="$2" body="${3:-}"
  if [ -n "$body" ]; then
    curl -sS -X "$method" "$BASE$path" -H "Authorization: Bearer $JWT" \
      -H 'Content-Type: application/json' --data-binary "@$body"
  else
    curl -sS -X "$method" "$BASE$path" -H "Authorization: Bearer $JWT"
  fi
}
field() { python3 -c 'import json,sys
d=json.load(sys.stdin); d=d.get("data",d)
for k in sys.argv[1].split("."): d=d.get(k) if isinstance(d,dict) else None
print("" if d is None else d)' "$1"; }

echo "workspace $WORKSPACE at $BASE"
detail="$(api GET "/api/v1/workspaces/$WORKSPACE")"
REPO_ID="$(printf '%s' "$detail" | field docsRepoId)"
[ -n "$REPO_ID" ] || { echo "the workspace has no docs repository: $detail" >&2; exit 1; }
VM_ID="$(api GET "/api/v1/workspaces/$WORKSPACE/vm" | field vmId)"
[ -n "$VM_ID" ] || { echo "the workspace is not running; start it first" >&2; exit 1; }

# 1. The question set: created once; a second run keeps the existing file but
#    makes sure it is committed, since the owner's read is the live file and
#    an agent-created or half-published set would leave readers with
#    unresolved references.
existing="$(api GET "/api/v1/workspaces/$WORKSPACE/questions/set?path=$SET_PATH")"
if [ "$(printf '%s' "$existing" | field path)" = "$SET_PATH" ]; then
  [ -z "$(printf '%s' "$existing" | field invalid.code)" ] || { echo "the existing set is invalid: $existing" >&2; exit 1; }
  echo "set exists: $SET_PATH; publishing its current bytes"
  current="$(api GET "/api/v1/workspaces/$WORKSPACE/files/$SET_PATH?repoId=$REPO_ID")"
  tmpset="$(mktemp)"
  printf '%s' "$current" | python3 -c '
import json, sys
d = json.load(sys.stdin); d = d.get("data", d)
print(json.dumps({"content": d["content"], "encoding": "utf-8", "expectedHash": d["hash"],
                  "expectedVmId": sys.argv[1], "commitAndPush": True}))' "$VM_ID" > "$tmpset"
  republished="$(api PUT "/api/v1/workspaces/$WORKSPACE/files/$SET_PATH?repoId=$REPO_ID" "$tmpset")"
  rm -f "$tmpset"
  if printf '%s' "$republished" | grep -q '"error"'; then
    echo "set not published: $republished" >&2; exit 1
  fi
else
  created="$(api POST "/api/v1/workspaces/$WORKSPACE/questions" "$QUESTIONS")"
  status="$(printf '%s' "$created" | field publication.status)"
  [ "$status" = "published" ] || { echo "set not published: $created" >&2; exit 1; }
  echo "set published: $SET_PATH"
fi

# 2. The page, with every placeholder pointed at this workspace's set, as an
#    explicit Save (commit and push) so readers see it. A re-run replaces the
#    page: the write names the current page's hash (null for a new page), as
#    the set step above does.
tmp="$(mktemp)"; page_now="$(mktemp)"; trap 'rm -f "$tmp" "$page_now"' EXIT
api GET "/api/v1/workspaces/$WORKSPACE/files/$HTML_PATH?repoId=$REPO_ID" > "$page_now"
python3 - "$PAGE" "$WORKSPACE" "$VM_ID" "$page_now" > "$tmp" <<'PY'
import json, sys
current = json.load(open(sys.argv[4], encoding="utf-8"))
data = current.get("data", current)
if isinstance(data, dict) and data.get("hash"):
    expected = data["hash"]
elif (current.get("error") or {}).get("code") == "NOT_FOUND":
    expected = None
else:
    sys.exit("cannot read the current page: " + json.dumps(current)[:500])
html = open(sys.argv[1], encoding="utf-8").read().replace("__WORKSPACE_ID__", sys.argv[2])
print(json.dumps({"content": html, "encoding": "utf-8", "expectedHash": expected,
                  "expectedVmId": sys.argv[3], "commitAndPush": True}))
PY
written="$(api PUT "/api/v1/workspaces/$WORKSPACE/files/$HTML_PATH?repoId=$REPO_ID" "$tmp")"
if printf '%s' "$written" | grep -q '"error"'; then
  echo "page not written: $written" >&2; exit 1
fi
echo "page committed: $HTML_PATH"

# 3. Where to open it.
doc_id="$(api GET "/api/v1/workspaces/$WORKSPACE/docs?surface=workspace" | python3 -c '
import json,sys
d=json.load(sys.stdin); d=d.get("data",d); docs=d if isinstance(d,list) else d.get("docs",[])
want=sys.argv[1]
print(next((x["id"] for x in docs if x.get("filePath")==want), ""))' "$HTML_PATH")"
echo "open: Project docs → Explainers → $SET_ID (doc id: ${doc_id:-not listed yet})"
