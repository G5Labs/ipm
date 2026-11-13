#!/usr/bin/env bash
# Regression tests for scripts/lib/native-publish.sh: the plan/apply split
# (every member validated before the first push, pinned SHAs and leases, one
# refspec per push, no URL rewrite) and the manifest member check.
# The API git mirror is a file:// tree of bare repos; the REST calls hit a
# fake `curl` on PATH that records requests and answers from canned files.
set -euo pipefail
for tool in git jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "✗ test-native-publish.sh needs $tool on PATH" >&2; exit 1; }
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HELPER="$ROOT_DIR/scripts/lib/native-publish.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

API="file://$tmp/api"
mkdir -p "$tmp/api/git" "$tmp/bin" "$tmp/fake"
export FAKE_CURL_API="$API"
export FAKE_CURL_DIR="$tmp/fake"

cat > "$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
# Fake curl: records "METHOD PATH BODY" and answers from $FAKE_CURL_DIR/<route>.
method=GET; url=""; body=""
while [ $# -gt 0 ]; do
  case "$1" in
    -X) method="$2"; shift 2 ;;
    -H) shift 2 ;;
    -d) body="$2"; shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
path="${url#"$FAKE_CURL_API"}"
printf '%s %s %s\n' "$method" "$path" "$body" >> "$FAKE_CURL_DIR/calls.log"
case "$path" in
  */manifest) route=manifest ;;
  *) echo "fake curl: unknown route $path" >&2; exit 22 ;;
esac
[ -f "$FAKE_CURL_DIR/$route" ] || exit 22
cat "$FAKE_CURL_DIR/$route"
EOF
chmod +x "$tmp/bin/curl"
export PATH="$tmp/bin:$PATH"

fail() { echo "FAIL: $*" >&2; exit 1; }

git_q() { git -c user.email=t@example.com -c user.name=t "$@"; }

# new_member <name>: a bare mirror repo <name> with branch "ws" at one
# commit, and a local clone of it at $tmp/local/<name>.
new_member() {
  local name="$1" seed="$tmp/seed-$1"
  git_q init -q -b ws "$seed"
  printf 'one\n' > "$seed/file"
  git_q -C "$seed" add -A && git_q -C "$seed" commit -q -m one
  git init -q --bare "$tmp/api/git/$name"
  git -C "$seed" push -q "$tmp/api/git/$name" ws:ws
  git clone -q --branch ws "$tmp/api/git/$name" "$tmp/local/$name"
}

commit_on() {
  local dir="$1" text="$2"
  printf '%s\n' "$text" > "$dir/file"
  git_q -C "$dir" add -A && git_q -C "$dir" commit -q -m "$text"
}

remote_head() { git -C "$tmp/api/git/$1" rev-parse "refs/heads/$2"; }

# The caller's own repository: its configuration is what reads without a
# member directory see, so the helper checks it for rewrites too.
git init -q "$tmp/caller" && cd "$tmp/caller"

# shellcheck source=lib/native-publish.sh
source "$HELPER"
native_publish_configure "$API" "test-token"

# ── 1. Apply pushes the SHA pinned at plan time, not a re-read HEAD ──────────
new_member alpha
commit_on "$tmp/local/alpha" two
planned="$(git -C "$tmp/local/alpha" rev-parse HEAD)"
native_publish_plan_reset
native_publish_plan alpha "$tmp/local/alpha" alpha ws >/dev/null
commit_on "$tmp/local/alpha" three
native_publish_apply >/dev/null
[ "$(remote_head alpha ws)" = "$planned" ] || fail "apply must push the planned SHA, not HEAD"
row="$(native_publish_plan_rows)"
[ "$row" = "alpha|$tmp/local/alpha|alpha|ws|$(git -C "$tmp/local/alpha" rev-parse HEAD~2)|$planned" ] \
  || fail "plan row shape: $row"
echo "ok 1 apply pushes the pinned SHA"

# ── 2. A later invalid member leaves an earlier valid one unpublished ─────────
new_member beta
new_member gamma
commit_on "$tmp/local/beta" two
beta_before="$(remote_head beta ws)"
# gamma's local history does not contain the mirror head.
rm -rf "$tmp/local/gamma"
git_q init -q -b ws "$tmp/local/gamma"
commit_on "$tmp/local/gamma" unrelated
native_publish_plan_reset
native_publish_plan beta "$tmp/local/beta" beta ws >/dev/null || fail "beta should plan"
if native_publish_plan gamma "$tmp/local/gamma" gamma ws 2>"$tmp/err"; then
  fail "divergent gamma must not plan"
fi
grep -q 'does not build on' "$tmp/err" || fail "divergence message: $(cat "$tmp/err")"
[ "$(remote_head beta ws)" = "$beta_before" ] || fail "nothing may be pushed before every member is planned"
echo "ok 2 planning stops before any push"

# ── 3. Missing branch and missing commit are plan-time failures ───────────────
native_publish_plan_reset
if native_publish_plan beta "$tmp/local/beta" beta nope 2>"$tmp/err"; then
  fail "missing branch must not plan"
fi
grep -q 'reserved for the fork service' "$tmp/err" || fail "missing branch message: $(cat "$tmp/err")"
mkdir -p "$tmp/local/empty" && git init -q "$tmp/local/empty"
if native_publish_plan empty "$tmp/local/empty" beta ws 2>"$tmp/err"; then
  fail "a repo without commits must not plan"
fi
[ "${#NATIVE_PUBLISH_PLAN[@]}" -eq 0 ] || fail "failed plans must not leave rows behind"
if native_publish_apply 2>"$tmp/err"; then
  fail "apply with an empty plan must fail"
fi
echo "ok 3 plan-time failures"

# ── 4. A stale lease is refused at apply and the mirror keeps its head ────────
new_member delta
commit_on "$tmp/local/delta" two
native_publish_plan_reset
native_publish_plan delta "$tmp/local/delta" delta ws >/dev/null
# The branch moves between plan and apply.
git clone -q --branch ws "$tmp/api/git/delta" "$tmp/local/delta-other"
commit_on "$tmp/local/delta-other" other
git -C "$tmp/local/delta-other" push -q "$tmp/api/git/delta" ws:ws
moved="$(remote_head delta ws)"
if native_publish_apply >/dev/null 2>"$tmp/err"; then
  fail "a moved branch must refuse the leased push"
fi
grep -q 'was refused' "$tmp/err" || fail "stale lease message: $(cat "$tmp/err")"
[ "$(remote_head delta ws)" = "$moved" ] || fail "a refused push must not move the branch"
if native_publish_plan_settled 2>/dev/null; then
  fail "a moved branch is not settled"
fi
echo "ok 4 stale lease refused"

# ── 5. Already-at-head rows are skipped; settled after a landed push ──────────
new_member eps
native_publish_plan_reset
# Plan rows live in the calling shell, so no pipeline or $(...) around plan.
native_publish_plan eps "$tmp/local/eps" eps ws > "$tmp/out"
grep -q 'nothing to push' "$tmp/out" || fail "unchanged member message"
native_publish_apply > "$tmp/out"
grep -q 'already at' "$tmp/out" || fail "unchanged member must be skipped at apply"
native_publish_plan_settled || fail "an unchanged plan is settled"
commit_on "$tmp/local/eps" two
native_publish_plan_reset
native_publish_plan eps "$tmp/local/eps" eps ws >/dev/null
native_publish_apply >/dev/null
native_publish_plan_settled || fail "a landed push is settled"
echo "ok 5 skip and settled"

# Publishing Git is complete without any projection-refresh HTTP request.
[ ! -s "$tmp/fake/calls.log" ] || fail "publication must not call a retired projection endpoint"

# ── 8. Inherited push configuration cannot widen the update ──────────────────
new_member zeta
commit_on "$tmp/local/zeta" two
git -C "$tmp/local/zeta" config push.followTags true
git_q -C "$tmp/local/zeta" tag -a v1 -m v1
native_publish_plan_reset
native_publish_plan zeta "$tmp/local/zeta" zeta ws >/dev/null
native_publish_apply >/dev/null
[ "$(remote_head zeta ws)" = "$(git -C "$tmp/local/zeta" rev-parse HEAD)" ] || fail "zeta must fast-forward"
if git -C "$tmp/api/git/zeta" rev-parse -q --verify refs/tags/v1 >/dev/null; then
  fail "push.followTags must not publish tags alongside the planned branch"
fi
echo "ok 8 inherited push configuration is ignored"

# ── 9. Every manifest member needs its branch before anything is pushed ───────
new_member eta
new_member theta
cat > "$tmp/fake/manifest" <<'EOF'
{"data":{"workspace_id":"ws-1","remote":{"enabled":true},
 "docs":{"repo_url":"x","repo_id":"eta","branch":"ws","tracking_mode":"tracking","path":"docs"},
 "repos":[{"name":"a","url":"x","repo_id":"theta","branch":"ws","path":"a"},
          {"name":"b","url":"x","repo_id":"theta","branch":"missing","path":"b"}]}}
EOF
if native_publish_manifest_members_exist ws-1 2>"$tmp/err"; then
  fail "a manifest member without its branch must fail the plan"
fi
grep -q 'theta has no refs/heads/missing' "$tmp/err" || fail "missing member message: $(cat "$tmp/err")"
cat > "$tmp/fake/manifest" <<'EOF'
{"data":{"workspace_id":"ws-1","remote":{"enabled":true},
 "docs":{"repo_url":"x","repo_id":"eta","branch":"ws","tracking_mode":"tracking","path":"docs"},
 "repos":[{"name":"a","url":"x","repo_id":"theta","branch":"ws","path":"a"}]}}
EOF
native_publish_manifest_members_exist ws-1 > "$tmp/out" || fail "a complete manifest must pass"
grep -q '2 checked' "$tmp/out" || fail "member count: $(cat "$tmp/out")"
for bad in '' '{}' '{"data":null}' '{"data":{"workspace_id":"ws-1","repos":[{"name":"a"}]}}'; do
  printf '%s' "$bad" > "$tmp/fake/manifest"
  if native_publish_manifest_members_exist ws-1 >/dev/null 2>&1; then fail "manifest '$bad' must fail the member check"; fi
done
echo "ok 9 manifest members must exist before a push"

# ── 10. A URL rewrite that could redirect the push refuses the plan ──────────
new_member iota
commit_on "$tmp/local/iota" two
iota_before="$(remote_head iota ws)"
git -C "$tmp/local/iota" config "url.$tmp/elsewhere/.pushInsteadOf" "$API/git/"
native_publish_plan_reset
if native_publish_plan iota "$tmp/local/iota" iota ws 2>"$tmp/err"; then
  fail "a pushInsteadOf rewrite of the API must refuse the plan"
fi
grep -q 'rewrites' "$tmp/err" || fail "rewrite message: $(cat "$tmp/err")"
[ "${#NATIVE_PUBLISH_PLAN[@]}" -eq 0 ] || fail "a refused rewrite must plan nothing"
git -C "$tmp/local/iota" config --unset "url.$tmp/elsewhere/.pushInsteadOf"
git -C "$tmp/local/iota" config "url.$tmp/elsewhere/.insteadOf" "$API/git/iota"
if native_publish_plan iota "$tmp/local/iota" iota ws >/dev/null 2>&1; then
  fail "an insteadOf rewrite of this repo must refuse the plan"
fi
git -C "$tmp/local/iota" config --unset "url.$tmp/elsewhere/.insteadOf"
git -C "$tmp/local/iota" config "url.$tmp/elsewhere/.insteadOf" "https://unrelated.example/"
native_publish_plan iota "$tmp/local/iota" iota ws >/dev/null || fail "an unrelated rewrite must not refuse the plan"
[ "$(remote_head iota ws)" = "$iota_before" ] || fail "planning must not push"
echo "ok 10 url rewrites are refused"

# ── 11. A rewrite in the caller's repository refuses configuration ───────────
git config "url.$tmp/elsewhere/.insteadOf" "$API/git/"
if native_publish_configure "$API" "test-token" 2>"$tmp/err"; then
  fail "a caller-local rewrite of the API must refuse configuration"
fi
grep -q 'rewrites' "$tmp/err" || fail "caller rewrite message: $(cat "$tmp/err")"
git config --unset "url.$tmp/elsewhere/.insteadOf"
native_publish_configure "$API" "test-token" || fail "configuration must pass without rewrites"
# Reads run in the member directory: a rewrite there is refused at plan time
# before the mirror head is read.
new_member kappa
git -C "$tmp/local/kappa" config "url.$tmp/elsewhere/.insteadOf" "$API/git/kappa"
native_publish_plan_reset
if native_publish_plan kappa "$tmp/local/kappa" kappa ws >/dev/null 2>&1; then
  fail "a member-local fetch rewrite must refuse the plan"
fi
echo "ok 11 the caller's configuration is checked too"

echo "All native-publish tests passed."
