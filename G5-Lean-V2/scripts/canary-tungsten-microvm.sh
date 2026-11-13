#!/usr/bin/env bash
# =============================================================================
# canary-tungsten-microvm.sh - standalone canary for the workspace VM path
# =============================================================================
# Exercises, end-to-end against a live G5 environment (default dev.g5labs.ai),
# the full chain validated on 2026-06-18 (infra ontology g5/infra D17;
# g5-api worker_hosts D10/D42):
#
#   1. Resolve the canary org (never created here - a missing org FAILS;
#      the org, its Base fixture, and the API key are operator-provisioned).
#   2. Provision a workspace host via the API (POST .../worker-hosts), the
#      only host-provisioning path in G5-Lean. Wait for `ready`.
#      Runs against an ordered candidate-region list (default us-central1 then
#      us-east1): a classified region-wide GCE stockout falls back to the next
#      region instead of going red (ops_scripts D47).
#   3. Set the org LLM runtime config (OpenRouter/Anthropic env) from a secret,
#      only if not already present.
#   4. Stage `chat`: fork Base, explicitly resume its workspace VM on the new
#      host, then run a Maven command via a chat task. The
#      compliance code is NOT seeded here, so the task is the dependency-free
#      `mvn -version` (proves Maven + Java run inside the microVM).
#      Stage `preview`: fork the org's Base, explicitly resume its workspace VM,
#      start the repo-committed preview profile, assert stable proxy routing,
#      then delete the fork.
#      Stages are selected by the caller via G5_CANARY_STAGES because they
#      target DIFFERENT orgs (tungsten-canary = chat; the preview org = PNC).
#   5. Tear everything down (forks, preview and workspace host), always (trap).
#
# Exit 0 = canary passed; non-zero = failed (with the failing stage logged).
#
# Auth (one of, in preference order):
#   - G5_CANARY_API_KEY  - an org-scoped G5 API key (g5k_*) + G5_CANARY_ORG_ID.
#     PREFERRED for CI: static bearer, no expiry, least-privilege scopes, org-
#     scoped, revocable. Mint once: POST /orgs/<org>/api-keys with scopes
#     [workers:admin, workspaces:read, workspaces:write, sessions:read,
#     sessions:write, tasks:read] and no expiresInSecs. Cannot create an org
#     (org-scoped), hence G5_CANARY_ORG_ID is required.
#   - G5_CANARY_REFRESH_TOKEN / _FILE (default /tmp/g5_refresh.dev) - a user
#     refresh token; fallback for local dev runs.
#   Supply credentials explicitly for an operator run. This script is not
#   invoked by the QA deploy pipeline.
#   Preview requires G5_CANARY_PREVIEW_HOST_SUFFIX, the deployment's bare
#   preview hostname (for example preview.lean.g5labs.ai).
#
# LLM secret (step 3, only when the org has no config yet): from EITHER
#   - G5_CANARY_LLM_SECRET_FILE      - key=value file (default ~/Desktop/open_router.secret), OR
#   - the caller environment: ANTHROPIC_AUTH_TOKEN (required) +
#     optional ANTHROPIC_BASE_URL / ANTHROPIC_MODEL / CLAUDE_CODE_SUBAGENT_MODEL.
#
# Deps: bash, curl, jq.
# =============================================================================
set -euo pipefail

API_URL="${G5_CANARY_API_URL:-https://dev.g5labs.ai}"
# Dedicated canary org - deliberately NOT the real 'tungsten' org (in prod
# that is a live customer tenant). Override per env if needed. Standing
# infrastructure: the canary resolves it and FAILS if absent, never creates it.
ORG_SLUG="${G5_CANARY_ORG_SLUG:-tungsten-canary}"
# c3-standard-4 (Sapphire Rapids, 4 vCPU / 16 GB — same shape as c2-standard-4)
# routes the canary host onto the abundant C3 capacity pool instead of the
# region-capped C2 pool. us-central1 C2_CPUS is a hard 14/project-region and the
# standing dev worker hosts already fill it, so a c2-standard-4 canary host
# repeatedly failed to provision with QUOTA_EXCEEDED. The API path fully supports
# C3 (gVNIC + pd-balanced in backend/g5-api/src/cloud/gcp.rs; allowlisted in g5_common).
INSTANCE_TYPE="${G5_CANARY_INSTANCE_TYPE:-c3-standard-4}"
# Ordered candidate regions for the worker-host provision (ops_scripts D47).
# `zone_capacity_exhausted` means g5-api's in-region multi-zone fallback already
# exhausted EVERY candidate zone, so a region-wide stockout dead-ends the row
# (G5#1880) no matter how long the canary waits - the only escape is another
# region. On that classified stockout the canary deletes the dead host row and
# reruns the whole provision phase in the next region. G5_CANARY_REGIONS is
# comma- or space-separated; the legacy G5_CANARY_REGION stays honored as an
# explicit single-region pin (no fallback) when G5_CANARY_REGIONS is unset.
if [ -n "${G5_CANARY_REGIONS:-}" ]; then
  REGIONS="$(echo "$G5_CANARY_REGIONS" | tr ',' ' ')"
elif [ -n "${G5_CANARY_REGION:-}" ]; then
  REGIONS="$G5_CANARY_REGION"
else
  REGIONS="us-central1 us-east1"
fi
read -r -a REGION_LIST <<< "$REGIONS"
[ "${#REGION_LIST[@]}" -ge 1 ] || { echo "FATAL: G5_CANARY_REGIONS resolved to an empty region list" >&2; exit 2; }
# Current-attempt region: reassigned by the per-region provision loop below;
# the provision payload, logs, and Slack text always name the region in play.
REGION="${REGION_LIST[0]}"
# Digest-pinned g5d-tungsten microVM base (JDK17 + Maven), supplied by the
# caller. Do NOT carry a
# hardcoded digest here. Tag-only/non-digest refs are rejected 422 by the API.
BASE_IMAGE_REF="${G5_CANARY_BASE_IMAGE_REF:-}"
LLM_SECRET_FILE="${G5_CANARY_LLM_SECRET_FILE:-$HOME/Desktop/open_router.secret}"
REFRESH_TOKEN_FILE="${G5_CANARY_REFRESH_TOKEN_FILE:-/tmp/g5_refresh.dev}"
READY_TIMEOUT_SECS="${G5_CANARY_READY_TIMEOUT_SECS:-600}"
CHAT_TIMEOUT_SECS="${G5_CANARY_CHAT_TIMEOUT_SECS:-420}"
# Gating (default ON): a post-deploy canary against a rolled g5d-tungsten image
# MUST see bare `mvn` on PATH and a writable HOME, else the PR #494 fixes
# regressed. Set these to 0 for a PRE-rollout run (before the fixed image ships),
# where Maven still resolves only via its full path - then they are warnings.
REQUIRE_MVN_ON_PATH="${G5_CANARY_REQUIRE_MVN_ON_PATH:-1}"
REQUIRE_HOME_WRITABLE="${G5_CANARY_REQUIRE_HOME_WRITABLE:-1}"
CHAT_PROFILE="${G5_CANARY_CHAT_PROFILE:-tungsten}"
case "$CHAT_PROFILE" in
  tungsten|g5dev) ;;
  *) echo "FATAL: unsupported G5_CANARY_CHAT_PROFILE='$CHAT_PROFILE'" >&2; exit 2 ;;
esac
# Stage selection (ops_scripts D33): the chat exercise and the preview
# preview stage run against DIFFERENT orgs, so each invocation selects its
# stage. `chat` = the Maven-in-workspace-VM task (tungsten-canary org,
# never previews). `preview` = the stable proxy +
# real-work stage (a dedicated preview org whose Base repo carries the
# fixture; currently the PNC org). Comma-combinable for local full runs.
STAGES="${G5_CANARY_STAGES:-chat}"
RUN_CHAT=0; RUN_PREVIEW=0
case ",$STAGES," in *",chat,"*) RUN_CHAT=1 ;; esac
case ",$STAGES," in *",preview,"*) RUN_PREVIEW=1 ;; esac
[ "$RUN_CHAT" = 1 ] || [ "$RUN_PREVIEW" = 1 ] || { echo "FATAL: G5_CANARY_STAGES='$STAGES' selects no stage (use chat, preview, or chat,preview)" >&2; exit 2; }

for dep in curl jq; do command -v "$dep" >/dev/null 2>&1 || { echo "FATAL: missing dependency '$dep'" >&2; exit 2; }; done
# `org-default` skips the pinned-image path entirely: the provision POST omits
# baseImageRef so the org's default base image is used (the preview stage runs
# on the image the org's real workloads use).
[ -n "$BASE_IMAGE_REF" ] || { echo "FATAL: base image ref unresolved - set G5_CANARY_BASE_IMAGE_REF (or 'org-default')" >&2; exit 2; }

log()  { echo "[canary $(date -u +%H:%M:%S)] $*"; }
fail() { echo "[canary FAIL] $*" >&2; exit 1; }

# Validate before any API calls or billable worker-host provisioning. Keep this
# bare DNS hostname check aligned with run_release_checks.py.
PREVIEW_HOST_SUFFIX="${G5_CANARY_PREVIEW_HOST_SUFFIX:-}"
if [ "$RUN_PREVIEW" = 1 ]; then
  preview_label='[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?'
  [[ ${#PREVIEW_HOST_SUFFIX} -le 253 && "$PREVIEW_HOST_SUFFIX" =~ ^${preview_label}(\.${preview_label})*$ ]] \
    || fail "preview stage requires G5_CANARY_PREVIEW_HOST_SUFFIX as a bare hostname (no scheme, path, port or userinfo)"
fi

# -- Classified provisioning-failure reporting (ops_scripts D43) ---------------
# When cloud bring-up fails for a known cause, g5-api persists
# worker_hosts.provision_error_code and the worker-hosts list returns it as
# provisionErrorCode. Surfacing it turns "stuck in provisioning" into an
# actionable verdict: infra flake vs image/API regression.
provision_error_hint() {
  case "$1" in
    zone_capacity_exhausted)
      echo "GCE capacity stockout - every candidate zone rejected the instance insert; infra flake, NOT an image/API regression" ;;
    *)
      echo "unrecognized provision error code (see g5-api worker_hosts classification)" ;;
  esac
}
# Best-effort Slack post for a CLASSIFIED provisioning failure only - the
# generic Cloud Build failure notifier already announces unclassified reds.
# Bounded, never fatal, never carries bearers/keys/response bodies (D21);
# silently skipped when the caller provides no G5_CANARY_SLACK_WEBHOOK.
notify_slack_provision_failure() {
  [ -n "${G5_CANARY_SLACK_WEBHOOK:-}" ] || return 0
  [ -n "${host_err_code:-}" ] || return 0
  local text payload
  text=":warning: *Canary worker-host provisioning failed: \`${host_err_code}\`*
$(provision_error_hint "$host_err_code")
• target: ${API_URL} (org ${ORG_SLUG}, stages ${STAGES})
• host: ${HOST_ID:-unknown} (${INSTANCE_TYPE}, ${REGION})${REGIONS_TRIED:+
• regions attempted: ${REGIONS_TRIED}}${G5_CANARY_BUILD_LOG_URL:+
• build: ${G5_CANARY_BUILD_LOG_URL}}"
  payload="$(jq -n --arg t "$text" '{text: $t}')"
  if curl -sS -o /dev/null --max-time 10 -X POST -H 'Content-Type: application/json' \
    -d "$payload" "$G5_CANARY_SLACK_WEBHOOK" 2>/dev/null; then
    log "posted classified provisioning-failure Slack notification ($host_err_code)"
  else
    log "WARN: Slack provisioning-failure notification failed (non-fatal)"
  fi
}

# -- Transient-tolerant API curl (ops_scripts ontology D32) --------------------
# The canary runs immediately after a rolling deploy: the public LB can serve
# 502/503/504 while pods rotate, and one such response must not fail a
# successful deploy (the 2026-07-13 prod promote went red exactly this way).
# Behaves like `curl -fsS` (body on stdout, nonzero exit on HTTP error) but
# retries transient gateway statuses and transport-level failures a bounded
# number of times. Retry logging prints ONLY status + URL - never headers or
# bodies - so the bearer cannot leak (D21).
#
# Scope: read-only GETs, the auth-refresh mint, and the idempotent org-config
# PUT. Resource-creating POSTs (org create, worker-host provision, session
# start) deliberately stay single-shot `curl -fsS`: a gateway error can land
# after the backend processed the request, and a blind retry could
# double-provision a billable worker host whose duplicate hostId the teardown
# trap never captured - a stranded-host leak.
#
# Budget: 30 x 10s = ~5 min. A cross-zone edge-pod reschedule leaves the
# external LB 502ing until GCLB NEG reprogramming completes (1-3+ min), which
# outlasted the old 6 x 10s budget (2026-08-01 dev deploy).
API_RETRY_ATTEMPTS="${G5_CANARY_API_RETRY_ATTEMPTS:-30}"
API_RETRY_SLEEP_SECS="${G5_CANARY_API_RETRY_SLEEP_SECS:-10}"
# Explicit network budget on every API call: a stalled request must return as
# a transport failure (000) and consume retry budget, not hang it (D32).
API_CONNECT_TIMEOUT_SECS="${G5_CANARY_API_CONNECT_TIMEOUT_SECS:-10}"
API_MAX_TIME_SECS="${G5_CANARY_API_MAX_TIME_SECS:-60}"
# Aggregate wall-clock deadline per api_curl call: with fully-stalled
# transfers the attempt bound alone allows 30x(60+10)s ~ 35 min; whichever
# of attempts or deadline exhausts first ends the call (D32).
API_RETRY_DEADLINE_SECS="${G5_CANARY_API_RETRY_DEADLINE_SECS:-600}"
api_curl() {
  local url="" arg attempt=1 code body deadline
  for arg in "$@"; do case "$arg" in http://*|https://*) url="$arg" ;; esac; done
  deadline=$(( $(date +%s) + API_RETRY_DEADLINE_SECS ))
  body="$(mktemp)"
  while :; do
    # `|| code=000`: a transport-level failure (connect refused/reset, DNS,
    # timeout) is retried like a gateway error rather than tripping `set -e`.
    code="$(curl -sS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
      --max-time "$API_MAX_TIME_SECS" -o "$body" -w '%{http_code}' "$@")" || code="000"
    case "$code" in
      2??|3??)
        cat "$body"; rm -f "$body"; return 0 ;;
      502|503|504|000)
        if [ "$attempt" -ge "$API_RETRY_ATTEMPTS" ] || [ "$(date +%s)" -ge "$deadline" ]; then
          echo "[canary] API call still failing after $attempt attempts (last http=$code, deadline ${API_RETRY_DEADLINE_SECS}s): $url" >&2
          rm -f "$body"; return 22
        fi
        echo "[canary] transient http=$code from $url (attempt $attempt/$API_RETRY_ATTEMPTS) - retrying in ${API_RETRY_SLEEP_SECS}s..." >&2
        attempt=$((attempt + 1))
        sleep "$API_RETRY_SLEEP_SECS" ;;
      *)
        # Non-transient HTTP error: fail immediately with the response body on
        # stderr (same abort semantics the old `curl -fsS` gave `set -e`).
        echo "[canary] API call failed (http=$code): $url" >&2
        cat "$body" >&2 || true
        rm -f "$body"; return 22 ;;
    esac
  done
}

# -- Auth ----------------------------------------------------------------------
# Prefer an org-scoped G5 API key (g5k_*) - a static bearer with no expiry and
# least-privilege scopes (workers:admin, workspaces:read/write, sessions:read/
# write, tasks:read for the preview proxy); the middleware accepts it as a user
# context. Fall back to a refresh token (local dev runs). The API-key
# path is org-scoped and CANNOT create an org - it reuses an existing one
# (G5_CANARY_ORG_ID, or resolved by slug).
API_KEY="${G5_CANARY_API_KEY:-}"
ORG_ID="${G5_CANARY_ORG_ID:-}"
RT="${G5_CANARY_REFRESH_TOKEN:-}"
if [ -z "$RT" ] && [ -f "$REFRESH_TOKEN_FILE" ]; then RT="$(tr -d '[:space:]' < "$REFRESH_TOKEN_FILE")"; fi
if [ -z "$API_KEY" ] && [ -z "$RT" ]; then
  fail "no credential: set G5_CANARY_API_KEY (g5k_*, preferred) or G5_CANARY_REFRESH_TOKEN[_FILE]"
fi

# -- Stable preview proxy assertion -------------------------------------------
# Reads one preview's per-boot nonce through the authenticated proxy. Every
# fetch traverses the workspace VM's HTTP stream. The nonce must be non-empty
# and identical across five spaced fetches.
#
# API keys cannot mint browser handoffs (#661). Both keys and refresh-token
# JWTs authenticate as Bearer directly on the task's isolated preview origin.
# The canary tests cover the API-key path; the refresh-token JWT path is untested.
# The caller supplies its deployment's suffix; no redirect or cookie is used.
# Browser handoff/exchange/cookie coverage remains in api_task_lifecycle and e2e.
assert_preview_stable() {
  local stable i n code detail url="https://$ISO_TASK_ID.$PREVIEW_HOST_SUFFIX/nonce.txt"
  PREVIEW_BODY_FILE="$(mktemp)"
  stable=""
  for i in 1 2 3 4 5; do
    code="$(curl -sS --max-time 20 -o "$PREVIEW_BODY_FILE" -w '%{http_code}' \
      "$url" "${AUTH[@]}")" || code="000"
    if [ "$code" != 200 ]; then
      # build_error_response wraps these fields in .error; never log the raw
      # body, message, request metadata or an HTML proxy error page.
      detail="$(jq -r '
        .error | select(type == "object") |
        [("code=" + (.code | select(type == "string"))),
         (.details.missing_scopes | select(type == "array") |
          map(select(type == "string")) | select(length > 0) |
          "missing_scopes=" + join(","))] |
        join(" ") | gsub("[[:cntrl:]]"; " ") | .[:200]
      ' "$PREVIEW_BODY_FILE" 2>/dev/null || true)"
      fail "preview stage: nonce fetch $i HTTP $code${detail:+; $detail} ($url)"
    fi
    n="$(tr -d '[:space:]' < "$PREVIEW_BODY_FILE")"
    [ -n "$n" ] || fail "preview stage: preview returned an empty nonce on fetch $i ($url)"
    if [ -z "$stable" ]; then
      stable="$n"
    elif [ "$n" != "$stable" ]; then
      fail "preview stage: nonce changed between fetches ('$stable' -> '$n')"
    fi
    sleep 2
  done
  rm -f "$PREVIEW_BODY_FILE"
  PREVIEW_BODY_FILE=""
  NONCE_A="$stable"
  log "preview proxy stable nonce: $NONCE_A"
}

# mint_jwt [orgId] - sets $JWT. With a static API key it's a no-op (the key is
# the bearer and does not expire); with a refresh token it mints an org-scoped
# JWT (the worker-host/config endpoints are audited for org scope before the
# handler runs). All callers re-derive AUTH from $JWT, so both paths are uniform.
mint_jwt() {
  if [ -n "$API_KEY" ]; then JWT="$API_KEY"; return 0; fi
  local body; body="$(jq -nc --arg rt "$RT" --arg org "${1:-}" \
    '{refreshToken:$rt} + (if $org=="" then {} else {orgId:$org} end)')"
  JWT="$(api_curl -X POST "$API_URL/api/v1/auth/refresh" \
    -H 'Content-Type: application/json' -d "$body" | jq -r '.data.jwt // .jwt // empty')"
  [ -n "$JWT" ] || fail "auth/refresh did not return a JWT (token expired or wrong env?)"
}
mint_jwt
AUTH=(-H "Authorization: Bearer $JWT")
log "authenticated against $API_URL ($([ -n "$API_KEY" ] && echo 'API key' || echo 'refresh token'))"

# -- Step 1: resolve the canary org --------------------------------------------
# Use an explicit G5_CANARY_ORG_ID if given, else resolve by slug (works in both
# auth modes). The canary NEVER creates or writes standing state - a missing org
# is a missing prerequisite and FAILS (ops_scripts D33: the org, its Base
# fixture, and the API key are operator-provisioned standing infrastructure).
if [ -z "$ORG_ID" ]; then
  ORG_ID="$(api_curl "$API_URL/api/v1/orgs" "${AUTH[@]}" \
    | jq -r --arg s "$ORG_SLUG" '.data[] | select(.orgSlug == $s) | .orgId' | head -1)"
fi
[ -n "$ORG_ID" ] || fail "canary org '$ORG_SLUG' not found - provision the org (and its Base fixture + API key) before enabling the canary; set G5_CANARY_ORG_ID to disambiguate"
log "org '$ORG_SLUG': $ORG_ID"

# Re-mint the JWT scoped to the tungsten org so the org-scoped config/worker-host
# endpoints (audited for org/workspace scopes) accept the calls regardless of
# which org was active on the bare refresh.
mint_jwt "$ORG_ID"
AUTH=(-H "Authorization: Bearer $JWT")

# Resolve the source workspace that each stage forks. A pre-provisioned fixture
# workspace may override Base for a preview-only run. Otherwise, the
# org/workspaces LIST response omits isBase, so pick the
# canonical cascade-named "Base" workspace then confirm via the workspace
# detail endpoint (which does carry isBase).
if [ -n "${G5_CANARY_FIXTURE_WS_ID:-}" ]; then
  WS_ID="$G5_CANARY_FIXTURE_WS_ID"
  WS_NAME="$(api_curl "$API_URL/api/v1/workspaces/$WS_ID" "${AUTH[@]}" | jq -r '.data.name // empty')"
  [ -n "$WS_NAME" ] || fail "fixture workspace $WS_ID (G5_CANARY_FIXTURE_WS_ID) not found in target env"
  log "fixture workspace (override): $WS_ID ($WS_NAME)"
else
  WS_ID="$(api_curl "$API_URL/api/v1/orgs/$ORG_ID/workspaces" "${AUTH[@]}" \
    | jq -r '.data[] | select(.name == "Base") | .id' | head -1)"
  [ -n "$WS_ID" ] || fail "no 'Base' workspace found in org $ORG_ID"
  IS_BASE="$(api_curl "$API_URL/api/v1/workspaces/$WS_ID" "${AUTH[@]}" | jq -r '.data.isBase // .data.is_base // false')"
  [ "$IS_BASE" = "true" ] || fail "workspace $WS_ID (name=Base) is not flagged isBase in detail"
  log "base workspace: $WS_ID"
fi

# -- Step 3: set org LLM runtime config if absent (chat stage only) -----------
# (Done before provisioning so the worker's first task already sees the keys.)
if [ "$RUN_CHAT" = 1 ]; then
HAS_KEY="$(api_curl "$API_URL/api/v1/orgs/$ORG_ID/config" "${AUTH[@]}" \
  | jq -r '([.data.effectiveSecretKeys // [], .data.secretKeys // []] | add | index("ANTHROPIC_AUTH_TOKEN")) // empty')"
if [ -n "$HAS_KEY" ]; then
  log "org LLM config already present (ANTHROPIC_AUTH_TOKEN set) - leaving as-is"
else
  # Source the LLM key from a key=value file (local runs) OR directly from env
  # (Cloud Build injects the Secret Manager value as ANTHROPIC_AUTH_TOKEN - it has
  # no desktop file). At least one must supply ANTHROPIC_AUTH_TOKEN.
  if [ -f "$LLM_SECRET_FILE" ]; then
    log "setting org LLM runtime config from $LLM_SECRET_FILE..."
    CONFIG_BODY="$(jq -Rn --rawfile raw "$LLM_SECRET_FILE" '
      ($raw | split("\n") | map(select(index("=")) | (index("=")) as $i |
        {key: (.[:$i] | gsub("^\\s+|\\s+$";"")), val: (.[$i+1:] | gsub("^\\s+|\\s+$";""))})
        | map(select(.key != "" and .val != ""))) as $kv
      | {
          settings:   ($kv | map(select(.key != "ANTHROPIC_AUTH_TOKEN")) | map({(.key): .val}) | add // {}),
          setSecrets: ($kv | map(select(.key == "ANTHROPIC_AUTH_TOKEN")) | map({(.key): .val}) | add // {}),
          removeSecretKeys: []
        }')"
  elif [ -n "${ANTHROPIC_AUTH_TOKEN:-}" ]; then
    log "setting org LLM runtime config from env (ANTHROPIC_AUTH_TOKEN)..."
    CONFIG_BODY="$(jq -n \
      --arg tok "$ANTHROPIC_AUTH_TOKEN" \
      --arg url "${ANTHROPIC_BASE_URL:-https://openrouter.ai/api}" \
      --arg model "${ANTHROPIC_MODEL:-anthropic/claude-opus-4.7}" \
      --arg sub "${CLAUDE_CODE_SUBAGENT_MODEL:-${ANTHROPIC_MODEL:-anthropic/claude-opus-4.7}}" \
      '{settings:{ANTHROPIC_BASE_URL:$url, ANTHROPIC_MODEL:$model, CLAUDE_CODE_SUBAGENT_MODEL:$sub}, setSecrets:{ANTHROPIC_AUTH_TOKEN:$tok}, removeSecretKeys:[]}')"
  else
    fail "LLM config absent: provide G5_CANARY_LLM_SECRET_FILE (key=value) or set ANTHROPIC_AUTH_TOKEN (+ optional ANTHROPIC_BASE_URL/ANTHROPIC_MODEL/CLAUDE_CODE_SUBAGENT_MODEL) in the env"
  fi
  echo "$CONFIG_BODY" | jq -e '.setSecrets.ANTHROPIC_AUTH_TOKEN | length > 0' >/dev/null \
    || fail "resolved LLM config has no ANTHROPIC_AUTH_TOKEN"
  api_curl -X PUT "$API_URL/api/v1/orgs/$ORG_ID/config" "${AUTH[@]}" \
    -H 'Content-Type: application/json' -d "$CONFIG_BODY" >/dev/null
  log "org LLM config set (settings: $(echo "$CONFIG_BODY" | jq -rc '.settings|keys'))"
fi

fi

# -- Step 2: provision a workspace host via the API ---------------------------
HOST_ID=""
# Set before the trap installs: the teardown reads them on every exit path.
CHAT_WS_ID=""; ISO_WS_ID=""; ISO_TASK_ID=""
PREVIEW_BODY_FILE=""
# Best-effort worker-host DELETE with one re-auth retry on 401/403, shared by
# the teardown trap and the D47 stockout-fallback path. Emits the final HTTP
# status on stdout; never fails the caller (a flaky/expired auth/refresh must
# not strand a billable host or abort a fallback hop). Every call carries the
# explicit D32 network budget: on the fallback path a stalled DELETE must
# consume bounded time, not block the hop to the next region. Callers invoke
# this in a $(...) subshell, so a JWT refreshed here is LOST to the caller -
# any caller that issues API calls afterwards must re-mint first.
delete_worker_host_row() { # $1=hostId
  local hid="$1" code
  code="$(curl -fsS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
    --max-time "$API_MAX_TIME_SECS" -o /dev/null -w '%{http_code}' -X DELETE \
    "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts/$hid" -H "Authorization: Bearer ${JWT:-}" || true)"
  if [ "$code" = "401" ] || [ "$code" = "403" ]; then
    JWT="$(curl -fsS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
      --max-time "$API_MAX_TIME_SECS" -X POST "$API_URL/api/v1/auth/refresh" \
      -H 'Content-Type: application/json' \
      -d "$(jq -nc --arg rt "$RT" --arg org "$ORG_ID" '{refreshToken:$rt, orgId:$org}')" \
      | jq -r '.data.jwt // empty' 2>/dev/null || true)"
    code="$(curl -fsS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
      --max-time "$API_MAX_TIME_SECS" -o /dev/null -w '%{http_code}' -X DELETE \
      "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts/$hid" -H "Authorization: Bearer ${JWT:-}" || true)"
  fi
  echo "$code"
}
# These IDs are captured only from forks created by this run. Force deletion
# intentionally discards their disposable runtime changes; never pass the
# standing fixture. Reuse the fork UUID as the operation ID so retries join the
# same lifecycle stop instead of repeatedly replacing its generation.
delete_canary_workspace() { # $1=workspaceId
  local workspace_id="$1" body code attempt=1 deadline
  body="$(mktemp)"
  deadline=$(( $(date +%s) + API_RETRY_DEADLINE_SECS ))
  while :; do
    code="$(curl -sS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
      --max-time "$API_MAX_TIME_SECS" -o "$body" -w '%{http_code}' \
      -X POST "$API_URL/api/v1/workspaces/$workspace_id/delete" \
      -H "Authorization: Bearer ${JWT:-}" -H 'Content-Type: application/json' \
      -d "$(jq -nc --arg id "$workspace_id" '{operationId:$id, force:true}')")" || code="000"
    case "$code" in
      204|404) rm -f "$body"; return 0 ;;
      409)
        if ! jq -e '.error.code == "WORKSPACE_STOPPING"' "$body" >/dev/null 2>&1; then
          break
        fi ;;
      502|503|504|000) ;;
      *) break ;;
    esac
    if [ "$attempt" -ge "$API_RETRY_ATTEMPTS" ] || [ "$(date +%s)" -ge "$deadline" ]; then
      break
    fi
    attempt=$((attempt + 1))
    sleep "$API_RETRY_SLEEP_SECS"
  done
  rm -f "$body"
  log "WARN: canary workspace $workspace_id cleanup failed (http=$code, attempts=$attempt)"
  return 1
}
teardown() {
  # -- Step 5: always tear everything down (best-effort) ----------------------
  rm -f "${PREVIEW_BODY_FILE:-}" 2>/dev/null || true
  # Cancel the active workspace preview before deleting its workspace.
  if [ -n "${ISO_TASK_ID:-}" ] && [ -n "${ISO_WS_ID:-}" ]; then
    curl -fsS -o /dev/null -X POST "$API_URL/api/v1/workspaces/$ISO_WS_ID/preview/cancel" \
      -H "Authorization: Bearer ${JWT:-}" 2>/dev/null || true
  fi
  if [ -n "${ISO_WS_ID:-}" ]; then
    log "deleting forked canary workspace $ISO_WS_ID..."
    delete_canary_workspace "$ISO_WS_ID" \
      || log "WARN: forked workspace $ISO_WS_ID delete failed - remove it manually"
  fi
  if [ -n "${CHAT_WS_ID:-}" ]; then
    log "deleting forked chat workspace $CHAT_WS_ID..."
    delete_canary_workspace "$CHAT_WS_ID" \
      || log "WARN: forked chat workspace $CHAT_WS_ID delete failed - remove it manually"
  fi
  [ -n "$HOST_ID" ] || return 0
  log "tearing down worker host $HOST_ID..."
  local code
  code="$(delete_worker_host_row "$HOST_ID")"
  log "teardown DELETE http=$code (204=deleted)"
}
trap teardown EXIT

log "provisioning workspace host (API path) instance=$INSTANCE_TYPE base=$(echo "$BASE_IMAGE_REF" | sed 's/.*@/@/') regions=$REGIONS..."

# -- Provision POST: bounded adopt-or-retry (ops_scripts D32) ------------------
# A blind re-POST after a gateway error could double-provision a billable host
# (the API commits the host row BEFORE responding, so a lost response leaves a
# live row whose hostId we never captured). Instead of staying single-shot -
# which turned a one-shot 502 into a red canary on the 2026-08-12 dev deploy -
# snapshot the org's pre-existing hostIds, and on a transient gateway status
# check the list for a NEW hostId first: exactly one means the POST landed and
# we ADOPT it (the teardown trap reaps it like a normal provision); none means
# the POST never landed and re-issuing is safe; more than one is ambiguous and
# fails without adopting. Logging prints only statuses, attempts, and hostIds -
# never bodies or headers (D21).
PROVISION_RETRY_ATTEMPTS="${G5_CANARY_PROVISION_RETRY_ATTEMPTS:-5}"
# Aggregate deadline for ONE region's provision phase (POST attempts, adoption
# polling, stray sweep); exceeding it fails loudly rather than letting nested
# retry budgets multiply toward the 3h build timeout (D32). Reset on every
# region hop (D47) so each candidate region gets the full budget.
PROVISION_DEADLINE_SECS="${G5_CANARY_PROVISION_DEADLINE_SECS:-1200}"

new_host_ids() {
  # NB: bind the id to $h before index() — index(f) evaluates f against its
  # OWN input ($known), so `index(.)` self-matches and returns 0 for every id.
  api_curl "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts" "${AUTH[@]}" \
    | jq -r --arg pre "$PRE_HOST_IDS" '
        ($pre | split(" ")) as $known
        | (.data.hosts? // .data)[]? | (.hostId // "") as $h | $h
        | select($h != "") | select(($known | index($h)) | not)'
}

# -- Per-region provision loop (ops_scripts D47) -------------------------------
# Run the full provision phase (adopt-or-retry POST, stray sweep, ready wait)
# against each candidate region in order. Only the CLASSIFIED region-wide
# stockout (provisionErrorCode=zone_capacity_exhausted) advances to the next
# region - every other failure keeps its immediate-abort semantics, because a
# real image/API regression must stay red rather than burn a second region
# proving itself.
HOST_ID=""
REGIONS_TRIED=""
host_status=""; host_err_code=""
for region_idx in "${!REGION_LIST[@]}"; do
  REGION="${REGION_LIST[$region_idx]}"
  regions_left=$(( ${#REGION_LIST[@]} - region_idx - 1 ))
  REGIONS_TRIED="${REGIONS_TRIED:+$REGIONS_TRIED }$REGION"

  # One workspace VM at a time is enough: the chat workspace is deleted before
  # a combined run starts the preview workspace. Validated 1..vCPUs at the
  # provision POST (worker_hosts D53/D54).
  PROVISION_PAYLOAD="$(jq -n --arg r "$REGION" --arg it "$INSTANCE_TYPE" --arg img "$BASE_IMAGE_REF" \
        '{region:$r, instanceType:$it, workerProfile:"fat-default", workspaceVmLimit:1}
         + (if $img == "org-default" then {} else {baseImageRef:$img} end)')"
  PROVISION_DEADLINE=$(( $(date +%s) + PROVISION_DEADLINE_SECS ))
  # Fresh pre-existing snapshot per region attempt: a dead host row from a
  # previous region's stockout must never be adopted as "new" here.
  PRE_HOST_IDS="$(api_curl "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts" "${AUTH[@]}" \
    | jq -r '(.data.hosts? // .data)[]?.hostId // empty' | sort | tr '\n' ' ')"

  HOST_ID=""
  prov_attempt=1
  while :; do
    [ "$(date +%s)" -lt "$PROVISION_DEADLINE" ] \
      || fail "provision phase exceeded its ${PROVISION_DEADLINE_SECS}s deadline (attempt $prov_attempt, region $REGION) - check for stray hosts from this run and reap manually"
    prov_body="$(mktemp)"
    prov_code="$(curl -sS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
      --max-time "$API_MAX_TIME_SECS" -o "$prov_body" -w '%{http_code}' \
      -X POST "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts" "${AUTH[@]}" \
      -H 'Content-Type: application/json' -d "$PROVISION_PAYLOAD")" || prov_code="000"
    case "$prov_code" in
      2??)
        PROV="$(cat "$prov_body")"; rm -f "$prov_body"
        HOST_ID="$(echo "$PROV" | jq -r '.data.hostId // empty')"
        # Never print $PROV: the provision response carries the minted hostJwt.
        [ -n "$HOST_ID" ] || fail "worker-host provision returned no hostId (http=$prov_code)"
        if [ "$prov_attempt" -gt 1 ]; then
          # Late-commit sweep: a slow backend can commit an earlier attempt's
          # row AFTER that attempt's adoption window expired, which this
          # successful POST would otherwise strand as a billable orphan. Any
          # new host that is not the one we just created is such a stray:
          # delete it best-effort. Attempt EVERY stray before failing so no
          # stray is left undeleted AND unreported; collected failures fail
          # the canary loudly - a stray can never stay silently billable.
          # Capture enumeration separately: $(...) inside a for-list swallows a
          # nonzero status under set -e, and a failed listing must not read as
          # "no strays".
          stray_list="$(new_host_ids)" \
            || fail "could not enumerate worker hosts for the stray sweep - reap any stray host from this run manually"
          stray_failures=""
          for stray in $stray_list; do
            [ "$stray" = "$HOST_ID" ] && continue
            log "deleting stray host $stray left by an earlier lost-response provision attempt..."
            stray_code="$(curl -sS --connect-timeout "$API_CONNECT_TIMEOUT_SECS" \
              --max-time "$API_MAX_TIME_SECS" -o /dev/null -w '%{http_code}' -X DELETE \
              "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts/$stray" "${AUTH[@]}")" || stray_code="000"
            case "$stray_code" in
              2??) log "stray host $stray deleted (http=$stray_code)" ;;
              *) log "WARN: stray host $stray delete failed (http=$stray_code)"
                 stray_failures="$stray_failures $stray(http=$stray_code)" ;;
            esac
          done
          [ -z "$stray_failures" ] || fail "stray hosts from earlier provision attempts could not be deleted - reap manually:$stray_failures"
        fi
        log "provisioned host record $HOST_ID (status=$(echo "$PROV" | jq -r '.data.status'), region $REGION) - waiting for ready..."
        break ;;
      502|503|504|000)
        rm -f "$prov_body"
        log "transient http=$prov_code from provision POST (attempt $prov_attempt/$PROVISION_RETRY_ATTEMPTS) - checking whether the request landed..."
        # Adoption window: the host row is committed before the API responds, so
        # a processed-but-lost POST shows up in the list within seconds.
        new_hosts=""
        for _ in 1 2 3; do
          sleep 5
          new_hosts="$(new_host_ids)"
          [ -n "$new_hosts" ] && break
        done
        if [ -n "$new_hosts" ]; then
          if [ "$(echo "$new_hosts" | wc -l | tr -d ' ')" -gt 1 ]; then
            fail "provision retry found MULTIPLE unattributed new worker hosts - not adopting; reap manually: $(echo "$new_hosts" | tr '\n' ' ')"
          fi
          HOST_ID="$new_hosts"
          log "adopted host record $HOST_ID created by the lost-response provision POST - waiting for ready..."
          break
        fi
        if [ "$prov_attempt" -ge "$PROVISION_RETRY_ATTEMPTS" ]; then
          fail "worker-host provision POST still failing after $prov_attempt attempts (last http=$prov_code)"
        fi
        prov_attempt=$((prov_attempt + 1))
        log "no new host appeared - the POST never landed; retrying provision in ${API_RETRY_SLEEP_SECS}s..."
        sleep "$API_RETRY_SLEEP_SECS"
        mint_jwt "$ORG_ID"; AUTH=(-H "Authorization: Bearer $JWT") ;;
      *)
        echo "[canary] worker-host provision failed (http=$prov_code):" >&2
        cat "$prov_body" >&2 || true
        rm -f "$prov_body"
        fail "worker-host provision returned non-transient http=$prov_code" ;;
    esac
  done

  deadline=$(( $(date +%s) + READY_TIMEOUT_SECS ))
  host_status="provisioning"; host_err_code=""
  region_stockout=0
  while [ "$(date +%s)" -lt "$deadline" ]; do
    sleep 15
    mint_jwt "$ORG_ID"; AUTH=(-H "Authorization: Bearer $JWT")
    host_row="$(api_curl "$API_URL/api/v1/orgs/$ORG_ID/worker-hosts" "${AUTH[@]}" \
      | jq -c --arg h "$HOST_ID" '[(.data.hosts? // .data)[]? | select(.hostId == $h)] | first // {}')"
    host_status="$(echo "$host_row" | jq -r '.status // empty')"
    host_err_code="$(echo "$host_row" | jq -r '.provisionErrorCode // empty')"
    log "  host status=$host_status${host_err_code:+ provisionErrorCode=$host_err_code}"
    # D47: the classified stockout is terminal for this attempt - g5-api only
    # persists it after EVERY zone in $REGION rejected the insert, and the row
    # then sits in `provisioning` until the reconciler flips it (the G5#1880
    # dead-end) - so stop waiting immediately, whatever the row status.
    if [ "$host_err_code" = "zone_capacity_exhausted" ]; then
      region_stockout=1
      break
    fi
    case "$host_status" in
      ready) break ;;
      terminated|failed)
        notify_slack_provision_failure
        fail "worker host reached terminal status '$host_status' before ready${host_err_code:+ - provisionErrorCode=$host_err_code: $(provision_error_hint "$host_err_code")}"
        ;;
    esac
  done

  if [ "$region_stockout" = 1 ]; then
    if [ "$regions_left" -gt 0 ]; then
      next_region="${REGION_LIST[$((region_idx + 1))]}"
      log "region $REGION stocked out (provisionErrorCode=$host_err_code) - falling back to $next_region"
      # Delete the dead row now (no VM exists behind it - the insert was
      # rejected everywhere) and clear HOST_ID so the teardown trap only ever
      # reaps the CURRENT attempt's host. A failed delete is loud but does not
      # block the fallback: the stuck row is unbillable and operator-reapable.
      dead_code="$(delete_worker_host_row "$HOST_ID")"
      case "$dead_code" in
        2??) log "deleted dead host row $HOST_ID (http=$dead_code)" ;;
        *)   log "WARN: dead host row $HOST_ID delete failed (http=$dead_code) - reap manually" ;;
      esac
      HOST_ID=""
      # A JWT the helper refreshed inside its $(...) subshell is lost here:
      # re-mint before the next region's snapshot so a stale bearer cannot
      # abort the fallback (no-op in API-key mode).
      mint_jwt "$ORG_ID"; AUTH=(-H "Authorization: Bearer $JWT")
      continue
    fi
    notify_slack_provision_failure
    fail "worker host provisioning stocked out in every candidate region ($REGIONS_TRIED) - provisionErrorCode=$host_err_code: $(provision_error_hint "$host_err_code")"
  fi

  if [ "$host_status" != "ready" ]; then
    notify_slack_provision_failure
    fail "worker host did not reach 'ready' within ${READY_TIMEOUT_SECS}s (last=$host_status)${host_err_code:+ - provisionErrorCode=$host_err_code: $(provision_error_hint "$host_err_code")}"
  fi
  break
done
log "workspace host READY (region $REGION)"

fork_workspace() { # $1=name prefix; sets FORKED_WORKSPACE_ID/FORKED_WORKSPACE_NAME
  local prefix="$1" response
  FORKED_WORKSPACE_NAME="${prefix}-$(date +%Y%m%d%H%M%S)-$$"
  response="$(curl -fsS -X POST "$API_URL/api/v1/workspaces/$WS_ID/clone" "${AUTH[@]}" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --arg n "$FORKED_WORKSPACE_NAME" --arg k "canary:$FORKED_WORKSPACE_NAME" \
      '{name:$n, idempotencyKey:$k}')")" \
    || fail "forking source workspace $WS_ID into '$FORKED_WORKSPACE_NAME' failed"
  FORKED_WORKSPACE_ID="$(echo "$response" | jq -r '.data.workspace.id // .data.id // empty')"
  if [ -z "$FORKED_WORKSPACE_ID" ]; then
    FORKED_WORKSPACE_ID="$(api_curl "$API_URL/api/v1/orgs/$ORG_ID/workspaces" "${AUTH[@]}" \
      | jq -r --arg n "$FORKED_WORKSPACE_NAME" '[.data[]? | select(.name == $n)] | first | .id // empty')"
    fail "workspace clone returned an unexpected shape (fork ${FORKED_WORKSPACE_ID:-not created}; teardown will remove it if found): $(echo "$response" | head -c 300)"
  fi
}

resume_workspace_on_canary_host() { # $1=workspace id $2=stage label
  local workspace_id="$1" label="$2" deadline vm status assigned_host last_error
  api_curl -X POST "$API_URL/api/v1/workspaces/$workspace_id/resume" "${AUTH[@]}" \
    -H 'Content-Type: application/json' -d '{}' >/dev/null
  deadline=$(( $(date +%s) + READY_TIMEOUT_SECS ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    sleep 8
    mint_jwt "$ORG_ID"; AUTH=(-H "Authorization: Bearer $JWT")
    vm="$(api_curl "$API_URL/api/v1/workspaces/$workspace_id/vm" "${AUTH[@]}")"
    status="$(echo "$vm" | jq -r '.data.status // empty')"
    assigned_host="$(echo "$vm" | jq -r '.data.hostId // .data.host_id // empty')"
    log "  $label workspace VM status=$status${assigned_host:+ host=$assigned_host}"
    if [ "$status" = "running" ]; then
      [ "$assigned_host" = "$HOST_ID" ] \
        || fail "$label workspace VM ran on host ${assigned_host:-unknown}, not canary host $HOST_ID"
      return 0
    fi
    if [ "$status" = "failed" ]; then
      last_error="$(echo "$vm" | jq -c '.data.lastError // .data.last_error // null')"
      fail "$label workspace VM failed to start on canary host $HOST_ID: $last_error"
    fi
  done
  fail "$label workspace VM did not reach running on canary host $HOST_ID within ${READY_TIMEOUT_SECS}s"
}

SESSION_ID=""
# -- Step 4: run `mvn -version` inside the org via a chat task (chat stage) ----
if [ "$RUN_CHAT" = 1 ]; then
fork_workspace "${G5_CANARY_CHAT_FORK_PREFIX:-canary-chat}"
CHAT_WS_ID="$FORKED_WORKSPACE_ID"
log "chat stage: forked source into '$FORKED_WORKSPACE_NAME' = $CHAT_WS_ID"
resume_workspace_on_canary_host "$CHAT_WS_ID" "chat"
# Also exercises the HOME-writability fix: `mvn -version` alone does not touch
# HOME, so a regression in the /home/ubuntu ownership fix would otherwise pass.
if [ "$CHAT_PROFILE" = "g5dev" ]; then
PROMPT='Run every numbered item as its own Bash tool call from the checked-out G5 repository; stop on its first failure and report exact output.
1) `docker version`
2) `psql --version && test -f /usr/share/postgresql/*/extension/vector.control`
3) `helm version --short`
4) `cargo fmt --version`
5) `cargo clippy --version`
6) `test -w "${CARGO_HOME:-$HOME/.cargo}" && touch "${CARGO_HOME:-$HOME/.cargo}/.g5-canary" && rm "${CARGO_HOME:-$HOME/.cargo}/.g5-canary"`
7) `cd frontend && g5-dev-frontend-bootstrap`
8) `cd frontend && npx playwright --version`
9) `cd frontend && npm run lint`
10) `cd frontend && npm run build`
11) `cd frontend && npx vitest run`
12) `cd . && SQLX_OFFLINE=true cargo check -p g5d --no-default-features`
Do not inspect /dev/kvm, run nested VMs, or compile g5d default features. Cloud Hypervisor/VM execution is intentionally host-only and unsupported inside this guest.'
TITLE="canary g5dev supported surfaces"
else
PROMPT='Do exactly these two checks inside this microVM and nothing else (the repo is intentionally empty).
1) Run `mvn -version`. If the shell reports "mvn: command not found", Maven is installed but not on PATH - locate it (try /usr/share/maven/bin/mvn) and run that full path with -version. Paste the full version output verbatim. Then on its own line print exactly MVN_ON_PATH=yes if the bare `mvn` worked, else MVN_ON_PATH=no.
2) Check HOME is writable by Maven by running: `mkdir -p "$HOME/.m2" && touch "$HOME/.m2/.canary" && rm -f "$HOME/.m2/.canary" && echo HOME_WRITABLE=yes || echo HOME_WRITABLE=no`. Print that result line verbatim.'
TITLE="canary mvn -version"
fi
log "starting $CHAT_PROFILE chat capability task in workspace VM $CHAT_WS_ID..."
# The disposable canary session runs unattended. Select its tool-approval mode
# explicitly; current Lean rejects an unset mode rather than assuming a default.
START="$(curl -fsS -X POST "$API_URL/api/v1/workspaces/$CHAT_WS_ID/sessions/start" "${AUTH[@]}" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg c "$PROMPT" --arg t "$TITLE" \
        '{content:$c, title:$t, agentProvider:"claude_code", permissionMode:"bypassPermissions"}')")"
SESSION_ID="$(echo "$START" | jq -r '.data.session.id // empty')"
[ -n "$SESSION_ID" ] || fail "chat start returned no session id: $START"
log "chat session $SESSION_ID - waiting for completion..."

deadline=$(( $(date +%s) + CHAT_TIMEOUT_SECS ))
msg_status="pending"; assistant=""
while [ "$(date +%s)" -lt "$deadline" ]; do
  sleep 12
  mint_jwt "$ORG_ID"; AUTH=(-H "Authorization: Bearer $JWT")
  MSGS="$(api_curl "$API_URL/api/v1/sessions/$SESSION_ID/messages" "${AUTH[@]}")"
  msg_status="$(echo "$MSGS" | jq -r '[.data.messages[]? // .data[]? | select(.role=="assistant")] | last | .status // "none"')"
  log "  assistant status=$msg_status"
  case "$msg_status" in
    complete|completed) assistant="$(echo "$MSGS" | jq -r '[.data.messages[]? // .data[]? | select(.role=="assistant")] | last | .content // ""')"; break ;;
    error|failed|stopped)
      assistant="$(echo "$MSGS" | jq -r '[.data.messages[]? // .data[]? | select(.role=="assistant")] | last | .content // ""')"
      echo "--- assistant message ---"; echo "$assistant"
      echo "--- timeline tail ---"
      api_curl "$API_URL/api/v1/sessions/$SESSION_ID/timeline" "${AUTH[@]}" \
        | jq -r '.data.entries[]? | "[\(.entryType)] \(.content)"' | tail -8
      fail "chat task ended in '$msg_status'"
      ;;
  esac
done
[ "$msg_status" = "complete" ] || [ "$msg_status" = "completed" ] || fail "chat did not complete within ${CHAT_TIMEOUT_SECS}s (last=$msg_status)"

# Execution-trace guard (FATAL): prove the agent actually RAN a shell in the
# microVM rather than answering `mvn -version` from model knowledge. A pure-prose
# turn has zero Bash tool calls in the timeline; require at least one.
TIMELINE="$(api_curl "$API_URL/api/v1/sessions/$SESSION_ID/timeline" "${AUTH[@]}")"
tool_calls="$(echo "$TIMELINE" | jq -r '[.data.entries[]? | select(.entryType=="tool_call")] | length')"
log "timeline tool_call (Bash) entries: ${tool_calls:-0}"
[ "${tool_calls:-0}" -ge 1 ] || { echo "$assistant"; fail "assistant produced no tool execution in the timeline - 'Apache Maven' is unverified prose, not a live microVM run"; }

if [ "$CHAT_PROFILE" = "g5dev" ]; then
  tool_results="$(echo "$TIMELINE" | jq -r '[.data.entries[]? | select(.entryType=="tool_result")] | length')"
  timeline_errors="$(echo "$TIMELINE" | jq -r '[.data.entries[]? | select(.entryType=="error")] | length')"
  [ "${tool_calls:-0}" -ge 12 ] \
    || fail "g5dev canary executed only ${tool_calls:-0} of 12 required Bash calls"
  [ "${tool_results:-0}" -ge 12 ] \
    || fail "g5dev canary recorded only ${tool_results:-0} of 12 required tool results"
  [ "${timeline_errors:-0}" -eq 0 ] \
    || fail "g5dev canary timeline contains $timeline_errors error entries"
  log "g5dev supported-surface commands completed in the workspace VM; nested VM/Cloud Hypervisor remains host-only"
else
# Primary assertion (FATAL): Maven actually ran inside the microVM - with a tool
# call proven above, this version banner is real microVM execution, not prose.
if echo "$assistant" | grep -qiE "Apache Maven [0-9]"; then
  log "Maven ran in the microVM: $(echo "$assistant" | grep -oiE 'Apache Maven [0-9.]+' | head -1)"
else
  echo "--- assistant message ---"; echo "$assistant"
  fail "no 'Apache Maven <version>' in output - Maven did not run in the microVM (worker/microVM/agent health)"
fi

# #494 PATH fix: bare `mvn` resolves only on a g5d-tungsten image rebuilt with the
# Dockerfile symlink. FATAL by default (a rolled image must have it); downgrade to
# a warning with G5_CANARY_REQUIRE_MVN_ON_PATH=0 for a pre-rollout run.
if echo "$assistant" | grep -qiE "MVN_ON_PATH=yes"; then
  log "PATH fix OK - bare 'mvn' is on the guest PATH (#494 deployed in this worker image)"
elif echo "$assistant" | grep -qiE "MVN_ON_PATH=no|command not found"; then
  if [ "$REQUIRE_MVN_ON_PATH" = "1" ]; then
    fail "'mvn' is NOT on the guest PATH - the #494 Dockerfile PATH fix is missing in this worker image (set G5_CANARY_REQUIRE_MVN_ON_PATH=0 for a pre-rollout run)"
  fi
  log "WARN (pre-rollout): 'mvn' not on PATH - #494 PATH fix not in this image yet (Maven ran via full path)"
elif [ "$REQUIRE_MVN_ON_PATH" = "1" ]; then
  fail "chat omitted required MVN_ON_PATH=yes evidence"
fi

# #494 HOME-ownership fix: same gating policy (FATAL by default).
if echo "$assistant" | grep -qiE "HOME_WRITABLE=yes"; then
  log "HOME fix OK - the agent can write \$HOME/.m2 (#494 deployed in this worker image)"
elif echo "$assistant" | grep -qiE "HOME_WRITABLE=no|Permission denied"; then
  if [ "$REQUIRE_HOME_WRITABLE" = "1" ]; then
    fail "\$HOME (/home/ubuntu) is NOT writable - the #494 Dockerfile HOME fix is missing in this worker image (set G5_CANARY_REQUIRE_HOME_WRITABLE=0 for a pre-rollout run)"
  fi
  log "WARN (pre-rollout): \$HOME not writable - #494 HOME fix not in this image yet"
elif [ "$REQUIRE_HOME_WRITABLE" = "1" ]; then
  fail "chat omitted required HOME_WRITABLE=yes evidence"
fi
fi
fi

if [ -n "$CHAT_WS_ID" ]; then
  log "deleting completed chat workspace $CHAT_WS_ID..."
  delete_canary_workspace "$CHAT_WS_ID" \
    || fail "completed chat workspace $CHAT_WS_ID could not be deleted"
  CHAT_WS_ID=""
fi

# -- Step 4b: workspace preview stage -----------------------------------------
# Fork the source workspace, explicitly resume its VM on the provisioned host,
# start the one active preview service, assert stable authenticated proxy
# routing, and delete the fork and host in the teardown trap. The
# fork inherits Base's current code and preview profiles. Runs ONLY when the
# `preview` stage is selected, against the dedicated preview org (currently
# PNC) — never against the tungsten org. The canary itself NEVER commits code
# into any org. HARD PREREQUISITE: the fixture (a Base code branch carrying
# the preview profile) must exist - a missing fixture FAILS the deploy,
# exactly like a missing org. Fixture ownership: the preview org commits
# canary/IsolationCanaryApp.java + the profile through its own repo flow.
if [ "$RUN_PREVIEW" = 1 ]; then
FORK_PREFIX="${G5_CANARY_FORK_PREFIX:-canary-isolation}"
PREVIEW_PROFILE="${G5_CANARY_PREVIEW_PROFILE:-isolation-canary}"
PREVIEW_READY_TIMEOUT_SECS="${G5_CANARY_PREVIEW_READY_TIMEOUT_SECS:-300}"

# The fixture check runs BEFORE forking: some code branch of Base must carry
# the preview profile (the fork inherits it). Docs branches are skipped.
ISO_PROFILE_REPO_ID=""
while read -r repo; do
  [ -n "$repo" ] || continue
  if api_curl "$API_URL/api/v1/workspaces/$WS_ID/repos/$repo/preview-profiles" "${AUTH[@]}" 2>/dev/null \
    | jq -e --arg p "$PREVIEW_PROFILE" '[.data[]? | select(.name == $p)] | length > 0' >/dev/null; then
    ISO_PROFILE_REPO_ID="$repo"
    break
  fi
done <<EOF_BRANCHES
$(api_curl "$API_URL/api/v1/workspaces/$WS_ID" "${AUTH[@]}" \
  | jq -r '.data.branches[]? | select(.repoName != "docs") | .repoId // empty')
EOF_BRANCHES

if [ -z "$ISO_PROFILE_REPO_ID" ]; then
  fail "preview stage prerequisite missing: no code branch of the source workspace carries preview profile '$PREVIEW_PROFILE'. The org must commit the fixture and profile through its own repo flow first"
else
  fork_workspace "$FORK_PREFIX"
  ISO_WS_ID="$FORKED_WORKSPACE_ID"
  log "preview stage: forked source into '$FORKED_WORKSPACE_NAME' = $ISO_WS_ID"
  resume_workspace_on_canary_host "$ISO_WS_ID" "preview"

  PREVIEW_RESP="$(curl -fsS -X POST "$API_URL/api/v1/workspaces/$ISO_WS_ID/preview" "${AUTH[@]}" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n --arg repo "$ISO_PROFILE_REPO_ID" --arg p "$PREVIEW_PROFILE" \
      --arg k "canary-preview:$FORKED_WORKSPACE_NAME" \
      '{repoId:$repo, profile:$p, idempotencyKey:$k}')")" \
    || fail "preview stage: start rejected despite profile '$PREVIEW_PROFILE' on source repository $ISO_PROFILE_REPO_ID"
  ISO_TASK_ID="$(echo "$PREVIEW_RESP" | jq -r '.data.taskId // .data.task_id // empty')"
  [ -n "$ISO_TASK_ID" ] || fail "preview stage: start response returned no task id: $(echo "$PREVIEW_RESP" | head -c 300)"
  log "preview task $ISO_TASK_ID started; waiting for ready (${PREVIEW_READY_TIMEOUT_SECS}s)..."

  preview_ready=false
  preview_deadline=$(( $(date +%s) + PREVIEW_READY_TIMEOUT_SECS ))
  while [ "$(date +%s)" -lt "$preview_deadline" ]; do
    sleep 10
    mint_jwt "$ORG_ID"; AUTH=(-H "Authorization: Bearer $JWT")
    PREVIEW_STATUS="$(api_curl "$API_URL/api/v1/workspaces/$ISO_WS_ID/preview" "${AUTH[@]}")"
    preview_task="$(echo "$PREVIEW_STATUS" | jq -r '.data.taskId // .data.task_id // empty')"
    preview_status="$(echo "$PREVIEW_STATUS" | jq -r '.data.status // "missing"')"
    preview_ready="$(echo "$PREVIEW_STATUS" | jq -r '.data.ready // false')"
    log "  preview task=$preview_task status=$preview_status ready=$preview_ready"
    [ "$preview_task" = "$ISO_TASK_ID" ] \
      || fail "preview stage: current task changed from $ISO_TASK_ID to ${preview_task:-none}"
    [ "$preview_ready" = "true" ] && break
    case "$preview_status" in failed|cancelled|succeeded) fail "preview task $ISO_TASK_ID became $preview_status before readiness" ;; esac
  done
  [ "$preview_ready" = "true" ] || fail "preview task $ISO_TASK_ID was not ready within ${PREVIEW_READY_TIMEOUT_SECS}s"

  assert_preview_stable
fi

fi

log "CANARY PASSED (org=$ORG_ID host=$HOST_ID stages=$STAGES${SESSION_ID:+ session=$SESSION_ID})"
# teardown runs via trap on EXIT
