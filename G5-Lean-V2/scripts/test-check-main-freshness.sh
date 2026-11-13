#!/usr/bin/env bash
# Shell-level regression tests for check-main-freshness.sh (ontology
# cloudbuild_pipelines [D5]/[D24]). The detector's verdict logic is unit-tested
# in test_check_deploy_freshness.py; these tests cover the wrapper's
# ORCHESTRATION, which bash -n cannot: builds-list eventual-consistency
# retries, the unresolved-deployment fallback's three confirmation rounds,
# deploy-control.env output, the approved-skip red path, the unapproved-skip
# clean green path, and the fail-closed approval lookup after exactly four
# attempts. All external commands (gcloud, kubectl, the python3 detector,
# sleep) are deterministic PATH stubs driven by per-scenario plan files; no
# network calls, no real sleeps.
#
# Run from anywhere: scripts/test-check-main-freshness.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="scripts/check-main-freshness.sh"

tmp_root="$(mktemp -d)"
trap 'rm -rf "${tmp_root}"' EXIT

STUB_BIN="${tmp_root}/bin"
mkdir -p "${STUB_BIN}"

# Each stub consumes the next line of its plan file per call and appends one
# line to its calls file, so tests can assert exact call counts.
cat > "${STUB_BIN}/plan-next" <<'STUB'
#!/usr/bin/env bash
f="$1"
line="$(head -n1 "$f")"
tail -n +2 "$f" > "$f.tmp" && mv "$f.tmp" "$f"
printf '%s' "$line"
STUB

cat > "${STUB_BIN}/gcloud" <<'STUB'
#!/usr/bin/env bash
ws="${WORKSPACE_DIR:?}"
case "$1 $2" in
  "container clusters")
    exit 0
    ;;
  "builds triggers")
    echo x >> "${ws}/trigger.calls"
    step="$(plan-next "${ws}/trigger.plan")"
    if [[ "${step}" == "FAIL" ]]; then
      echo "stub: triggers describe transient failure" >&2
      exit 1
    fi
    echo "deploy-trigger-id"
    ;;
  "builds list")
    echo x >> "${ws}/list.calls"
    printf '%s\n' "$*" >> "${ws}/list.args"
    step="$(plan-next "${ws}/list.plan")"
    if [[ "${step}" == "FAIL" ]]; then
      echo "stub: builds list transient failure" >&2
      exit 1
    fi
    echo "[]"
    ;;
  "builds describe")
    echo x >> "${ws}/describe.calls"
    step="$(plan-next "${ws}/describe.plan")"
    if [[ "${step}" == "FAIL" ]]; then
      echo "stub: builds describe transient failure" >&2
      exit 1
    fi
    if [[ "${step}" != "NONE" ]]; then
      printf '%s\n' "${step}"
    fi
    ;;
  "artifacts docker")
    cat "${ws}/artag.out" 2>/dev/null || true
    ;;
  *)
    echo "stub gcloud: unexpected args: $*" >&2
    exit 64
    ;;
esac
STUB

cat > "${STUB_BIN}/kubectl" <<'STUB'
#!/usr/bin/env bash
cat "${WORKSPACE_DIR:?}/kubectl.out" 2>/dev/null || true
STUB

cat > "${STUB_BIN}/python3" <<'STUB'
#!/usr/bin/env bash
ws="${WORKSPACE_DIR:?}"
echo x >> "${ws}/detector.calls"
step="$(plan-next "${ws}/detector.plan")"
if [[ "${step}" == "FAIL" ]]; then
  echo "stub: detector raised" >&2
  exit 1
fi
printf '%s\n' "${step}"
STUB

cat > "${STUB_BIN}/sleep" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB

chmod +x "${STUB_BIN}"/*

fail() { echo "FAIL: $*" >&2; exit 1; }

assert_contains() { # file needle context
  grep -qF -- "$2" "$1" || { cat "$1" >&2; fail "$3: output missing '$2'"; }
}

assert_calls() { # calls-file expected context
  local got=0
  [[ -f "$1" ]] && got="$(wc -l < "$1" | tr -d ' ')"
  [[ "${got}" == "$2" ]] || fail "$3: expected $2 calls, got ${got}"
}

new_workspace() {
  ws="${tmp_root}/ws-$1"
  mkdir -p "${ws}"
  cat > "${ws}/delivery.env" <<'EOF'
export AR=region-docker.pkg.dev/proj/repo
export IMAGE_VERSION=cafe0000feed0000cafe0000feed0000cafe0000
export TARGET_PROJECT=tgt-proj
EOF
  : > "${ws}/image-refs.env"
  : > "${ws}/trigger.plan"
  cat > "${ws}/deploy-queue.env" <<'EOF'
export DEPLOY_QUEUE_WAIT_SECONDS=42
export DEPLOY_QUEUE_BUILD_CREATE_TIME=2026-07-17T00:00:00+00:00
EOF
}

run_script() { # mode cluster
  local status=0
  (
    cd "${ROOT_DIR}" &&
    PATH="${STUB_BIN}:${PATH}" \
    WORKSPACE_DIR="${ws}" \
    COMMIT_SHA="cafe0000feed0000cafe0000feed0000cafe0000" \
    BUILD_ID="build-under-test" \
    PROJECT_ID="host-proj" \
    _PROMOTION_MODE="$1" \
    _TARGET_PROJECT="tgt-proj" \
    _GKE_CLUSTER="$2" \
    _GKE_ZONE="zone" \
    _HELM_NAMESPACE="ns" \
    _AR_REGION="region" \
    bash "${SCRIPT}"
  ) > "${ws}/log" 2>&1 || status=$?
  return "${status}"
}

# S1 — build mode: a proceed verdict is accepted on the first fetch and
# deploy-control.env carries the exact SKIP_DEPLOY=false contract ([D5]).
new_workspace s1
printf 'OK\n' > "${ws}/list.plan"
printf 'PROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tnewest\n' > "${ws}/detector.plan"
run_script build "" || fail "S1: build-mode proceed should exit 0 (log: ${ws}/log)"
assert_calls "${ws}/detector.calls" 1 "S1 detector single-shot"
assert_contains "${ws}/deploy-control.env" "export SKIP_DEPLOY=false" "S1 deploy-control"
assert_contains "${ws}/deploy-control.env" "export ARTIFACT_COMMIT=cafe0000feed0000cafe0000feed0000cafe0000" "S1 deploy-control"
assert_contains "${ws}/deploy-control.env" "export DEPLOY_FRESHNESS_HEAD=cafe0000feed0000cafe0000feed0000cafe0000" "S1 deploy-control"
# The deploy-trigger filter must be one the Cloud Build API applies
# server-side; a substitutions.* filter scans the whole build history.
assert_contains "${ws}/list.args" '--filter=trigger_id="deploy-trigger-id"' "S1 server-side trigger filter"
if grep -q 'substitutions\.' "${ws}/list.args"; then
  fail "S1: builds list must not filter on substitutions (client-side full-history scan)"
fi
echo "S1 ok: build-mode proceed + deploy-control.env contract + server-side trigger filter"

# S2 — `gcloud builds list` eventual consistency: two transient list failures
# are retried (not fatal) and the third fetch succeeds ([D5]/[I4]).
new_workspace s2
printf 'FAIL\nFAIL\nOK\n' > "${ws}/list.plan"
printf 'PROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tnewest\n' > "${ws}/detector.plan"
run_script build "" || fail "S2: list-lag retry should recover and exit 0 (log: ${ws}/log)"
assert_calls "${ws}/list.calls" 3 "S2 list retries"
assert_calls "${ws}/trigger.calls" 1 "S2 trigger id resolved once"
assert_calls "${ws}/detector.calls" 1 "S2 detector runs only after a good list"
assert_contains "${ws}/log" "not visible yet (attempt 1/8)" "S2 retry message"
echo "S2 ok: transient builds-list failures retry instead of failing closed"

# S3 — unresolved deployed commit: promotion mode falls back to the legacy
# newest-main predicate, which requires three consecutive proceed verdicts
# before mutating staging/prod ([D24] fallback contract).
new_workspace s3
: > "${ws}/kubectl.out"
printf 'OK\nOK\nOK\n' > "${ws}/list.plan"
printf 'PROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tnewest\nPROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tnewest\nPROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tnewest\n' > "${ws}/detector.plan"
run_script deploy fake-cluster || fail "S3: fallback proceed should exit 0 (log: ${ws}/log)"
assert_contains "${ws}/log" "falling back to the newest-main predicate" "S3 fallback"
assert_contains "${ws}/log" "confirming (1/3)" "S3 stabilization round 1"
assert_contains "${ws}/log" "confirming (2/3)" "S3 stabilization round 2"
assert_calls "${ws}/detector.calls" 3 "S3 three confirmation rounds"
assert_contains "${ws}/deploy-control.env" "export SKIP_DEPLOY=false" "S3 deploy-control"
echo "S3 ok: unresolved-deployment fallback requires three consecutive proceeds"

# S4 — spent approval on a skip is a RED build, not a green no-op ([D24]).
new_workspace s4
printf 'OK\n' > "${ws}/list.plan"
printf 'SKIP\tnewer0000feed0000cafe0000feed0000cafe000\tstale\n' > "${ws}/detector.plan"
printf 'APPROVED\n' > "${ws}/describe.plan"
if run_script build ""; then
  fail "S4: approved skip must exit non-zero (log: ${ws}/log)"
fi
assert_contains "${ws}/log" "manual approval was spent on a skipped deploy" "S4 red message"
assert_calls "${ws}/describe.calls" 1 "S4 approval lookup"
echo "S4 ok: approved stale skip exits red"

# S5 — unapproved (auto-triggered) skip keeps the clean exit-0 idiom ([I3])
# and still writes SKIP_DEPLOY=true for the downstream self-skips.
new_workspace s5
printf 'OK\n' > "${ws}/list.plan"
printf 'SKIP\tnewer0000feed0000cafe0000feed0000cafe000\tstale\n' > "${ws}/detector.plan"
printf 'NONE\n' > "${ws}/describe.plan"
run_script build "" || fail "S5: unapproved skip should exit 0 (log: ${ws}/log)"
assert_contains "${ws}/log" "Skipping deploy and promotion" "S5 skip message"
assert_contains "${ws}/deploy-control.env" "export SKIP_DEPLOY=true" "S5 deploy-control"
echo "S5 ok: unapproved skip stays a clean green no-op"

# S6 — the approval-state lookup fails closed: exactly four attempts, then a
# non-zero exit rather than green-skipping a possibly approved promotion
# ([D24]/[I4], CodeRabbit on PR #1291).
new_workspace s6
printf 'OK\n' > "${ws}/list.plan"
printf 'SKIP\tnewer0000feed0000cafe0000feed0000cafe000\tstale\n' > "${ws}/detector.plan"
printf 'FAIL\nFAIL\nFAIL\nFAIL\n' > "${ws}/describe.plan"
if run_script build ""; then
  fail "S6: unreadable approval state must exit non-zero (log: ${ws}/log)"
fi
assert_contains "${ws}/log" "could not read this build's approval state after 4 attempts" "S6 fail-closed message"
assert_calls "${ws}/describe.calls" 4 "S6 exactly four lookup attempts"
echo "S6 ok: unreadable approval state fails closed after four attempts"


# S7 — deployed-commit resolution succeeds and the [D24] advance path engages:
# the digest -> 40-hex tag mapping must work under the BUILDER'S awk (mawk has
# no interval expressions — /[0-9a-f]{40}/ silently matches nothing, which
# blinded the advance predicate from 2026-07-13 to 2026-07-18).
new_workspace s7
printf 'reg/api@sha256:feedfacefeedfacefeedfacefeedfacefeedface' > "${ws}/kubectl.out"
printf 'a16e341bcac25eb9a67773643bcac669f07d962f\tsha256:feedfacefeedfacefeedfacefeedfacefeedface\n' > "${ws}/artag.out"
printf 'OK\n' > "${ws}/list.plan"
printf 'PROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tadvances\n' > "${ws}/detector.plan"
run_script deploy fake-cluster || fail "S7: resolved-deployment advance should exit 0 (log: ${ws}/log)"
assert_contains "${ws}/log" "currently runs commit a16e341bcac25eb9a67773643bcac669f07d962f" "S7 resolution"
assert_calls "${ws}/detector.calls" 1 "S7 advance verdict accepted immediately (no confirmation rounds)"
assert_contains "${ws}/deploy-control.env" "export SKIP_DEPLOY=false" "S7 deploy-control"
echo "S7 ok: digest->tag resolution works under the builder awk and advance engages"

# S8 — a transient failure resolving the deploy trigger's id is retried like a
# builds-list failure, and the list runs only once the id is known.
new_workspace s8
printf 'FAIL\n' > "${ws}/trigger.plan"
printf 'OK\n' > "${ws}/list.plan"
printf 'PROCEED\tcafe0000feed0000cafe0000feed0000cafe0000\tnewest\n' > "${ws}/detector.plan"
run_script build "" || fail "S8: trigger-id retry should recover and exit 0 (log: ${ws}/log)"
assert_calls "${ws}/trigger.calls" 2 "S8 trigger describe retried"
assert_calls "${ws}/list.calls" 1 "S8 list waits for the trigger id"
assert_contains "${ws}/log" "not visible yet (attempt 1/8)" "S8 retry message"
echo "S8 ok: trigger-id resolution failures retry instead of failing closed"

echo "check-main-freshness orchestration tests passed"
