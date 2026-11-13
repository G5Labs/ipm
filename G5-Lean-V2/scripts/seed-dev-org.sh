#!/usr/bin/env bash
# =============================================================================
# seed-dev-org.sh — Seed the "G5 Labs" org in the dev database
# =============================================================================
# Hits the E2E test-token endpoint which cascade-creates:
#   user (demo@g5labs.ai) → org ("G5 Labs" / g5labs) → workspaces (base + demo)
#   → docs repo → workspace membership links
# then seeds the OAuth stub's dev personas (display names, avatars, org roles)
# and makes Alex (demo@g5labs.ai) an administrator of the platform operator org,
# as the platform seed makes the deployment's first operator in prod.
#
# Outputs the base workspace UUID on stdout (for use by callers).
# Human-readable status messages go to stderr. Exits nonzero, without printing
# the UUID, when the org, a dev persona or the operator cannot be seeded and
# verified.
#
# Requires: curl, jq, running g5-api with G5_E2E_AUTH=true
# =============================================================================
set -euo pipefail

# ── Resolve API URL (handles devcontainer Docker-in-Docker) ──────────────────
# The stack identity the Justfile exports (see dev-stack-up): a preview of
# this stack beside the developer's own stack seeds its own API and database.
STACK="${G5_COMPOSE_PROJECT:-g5-dev}"
API_PORT="${G5_PORT_API:-7500}"
API_URL="${1:-http://localhost:${API_PORT}}"
PG_CONTAINER="${PG_CONTAINER:-${STACK}-postgres}"
DB_NAME="${DB_NAME:-g5_dev}"
REPOS_VOLUME="${REPOS_VOLUME:-${G5_COMPOSE_VOLUMES:-g5-lean}_api_repos}"

if [ "$API_URL" = "auto" ] || ! curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
  if curl -sf "http://localhost:${API_PORT}/healthz" > /dev/null 2>&1; then
    API_URL="http://localhost:${API_PORT}"
  else
    API_IP=$(docker inspect "${STACK}-api" --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
    if [ -n "$API_IP" ] && curl -sf "http://${API_IP}:7500/healthz" > /dev/null 2>&1; then
      API_URL="http://${API_IP}:7500"
    else
      echo "  ✗ Cannot reach g5-api — dev org not seeded" >&2
      exit 1
    fi
  fi
fi

# ── Hit test-token to create user + org cascade ─────────────────────────────
RESP=$(curl -sf "${API_URL}/api/v1/auth/test-token" \
  -H "Content-Type: application/json" \
  -d '{"email":"demo@g5labs.ai"}' 2>/dev/null || echo "")

if [ -z "$RESP" ]; then
  echo "  ✗ test-token endpoint unavailable — dev org not seeded" >&2
  exit 1
fi

TOKEN=$(echo "$RESP" | jq -r '.data.jwt // empty' 2>/dev/null)
if [ -z "$TOKEN" ]; then
  echo "  ✗ No JWT in test-token response — dev org not seeded" >&2
  exit 1
fi

# Fetch org + workspace IDs
ORGS_RESP=$(curl -sf "${API_URL}/api/v1/orgs" \
  -H "Authorization: Bearer ${TOKEN}" 2>/dev/null || echo "")
ORG_SLUG=$(echo "$ORGS_RESP" | jq -r '.data[0].orgSlug // empty' 2>/dev/null)
ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)

if [ -n "$ORG_SLUG" ]; then
  echo "  ✓ Org '${ORG_SLUG}' seeded (user: demo@g5labs.ai)" >&2
else
  echo "  ⚠ Org seed completed but could not verify" >&2
fi

# Get the base workspace ID
WS_ID=""
if [ -n "$ORG_ID" ]; then
  WS_RESP=$(curl -sf "${API_URL}/api/v1/orgs/${ORG_ID}/workspaces" \
    -H "Authorization: Bearer ${TOKEN}" 2>/dev/null || echo "")
  WS_ID=$(echo "$WS_RESP" | jq -r '.data[] | select(.name == "base" or .name == "Base") | .id // empty' 2>/dev/null | head -1)
  [ -z "$WS_ID" ] && WS_ID=$(echo "$WS_RESP" | jq -r '.data[0].id // empty' 2>/dev/null)
  if [ -n "$WS_ID" ]; then
    echo "  ✓ Base workspace: ${WS_ID}" >&2
  fi
fi

# Get the docs repo ID
DOCS_REPO_ID=""
if [ -n "$ORG_ID" ]; then
  REPOS_RESP=$(curl -sf "${API_URL}/api/v1/orgs/${ORG_ID}/repos" \
    -H "Authorization: Bearer ${TOKEN}" 2>/dev/null || echo "")
  DOCS_REPO_ID=$(echo "$REPOS_RESP" | jq -r '.data[] | select(.slug == "docs") | .id // empty' 2>/dev/null | head -1)
  if [ -n "$DOCS_REPO_ID" ]; then
    echo "  ✓ Docs repo: ${DOCS_REPO_ID}" >&2
  fi
fi

# ── Verify docs authority created by the canonical test-token path ───────────
# The endpoint finalizes the initial docs candidate through the same accepted-
# cut operation used in production. This script deliberately does not create a
# replacement repository, mutate refs, or patch authority rows.
if [ -z "$WS_ID" ] || [ -z "$DOCS_REPO_ID" ]; then
  echo "  ✗ Cannot verify Base docs authority without workspace and repo IDs" >&2
  exit 1
fi

DOCS_STATE=$(curl -fsS "${API_URL}/api/v1/workspaces/${WS_ID}/branches" \
  -H "Authorization: Bearer ${TOKEN}")
DOCS_HEAD=$(printf '%s' "$DOCS_STATE" | jq -er --arg repo "$DOCS_REPO_ID" \
  '.data[] | select(.repoId == $repo and .materializationStatus == "materialized") | .headCommit')
[[ "$DOCS_HEAD" =~ ^[0-9a-f]{40}$ ]] || { echo 'Base docs Git ref is unavailable' >&2; exit 1; }
echo "  ✓ Base docs Git ref materialized: ${DOCS_HEAD}" >&2

# ── Seed the OAuth stub's dev personas ───────────────────────────────────────
# The stub's roster (OAUTH_USER_* and OAUTH_EXTRA_USERS in
# docker-compose.dev.yml) defines the local personas; the frontend Dev Login
# buttons (DEV_LOGIN_USERS in frontend/src/App.tsx) mirror it. Each persona gets
# the test-token membership cascade, the stub's display name and avatar, and
# its declared `role` in the fixture orgs test-token provisions. Test-token
# makes a new membership Admin. Role convergence uses the same guarded member
# API as ordinary management, including on retries; no direct SQL can demote
# the final administrator. Provision the whole roster before changing roles,
# then apply Admin roles before reductions. Each update selects a current
# fixture Admin; reductions use one the roster leaves Admin for later updates
# and reruns. The member API rechecks that authority at the actual mutation.
persona_fail() {
  echo "  ✗ $1" >&2
  exit 1
}

OAUTH_STUB_URL="${OAUTH_STUB_URL:-http://localhost:${G5_PORT_OAUTH_STUB:-9876}}"
if ! curl -sf "${OAUTH_STUB_URL}/healthz" > /dev/null 2>&1; then
  STUB_IP=$(docker inspect "${STACK}-oauth-stub" --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
  [ -n "$STUB_IP" ] && OAUTH_STUB_URL="http://${STUB_IP}:9876"
fi
ROSTER=$(curl -sf "${OAUTH_STUB_URL}/__e2e/users" 2>/dev/null) \
  || persona_fail "OAuth stub roster unavailable at ${OAUTH_STUB_URL}; dev personas not seeded"
# Unit-separator fields: tab is IFS whitespace and would collapse an empty role.
PERSONAS=$(printf '%s' "$ROSTER" \
  | jq -er '.users | if length > 0 then sort_by((.role // "admin") != "admin")[] else error("empty roster") end
      | [(.email | ascii_downcase), .name, .avatar_url, (.role // "admin" | if . == "admin" or . == "member" or . == "viewer" then . else error("unsupported role") end)] | join("\u001f")' 2>/dev/null) \
  || persona_fail "OAuth stub roster is not a non-empty user list; dev personas not seeded"

E2E_OPERATOR_EMAIL="e2e-operator@g5labs.ai"
operator_fail() {
  echo "  ✗ $1" >&2
  exit 1
}
OP_RESP=$(curl -sf "${API_URL}/api/v1/auth/test-token" \
  -H "Content-Type: application/json" \
  -d "$(jq -nc --arg email "$E2E_OPERATOR_EMAIL" '{email: $email}')" 2>/dev/null || echo "")
OP_TOKEN=$(printf '%s' "$OP_RESP" | jq -r '.data.jwt // empty' 2>/dev/null || echo "")
[ -n "$OP_TOKEN" ] || operator_fail "Could not mint a test token for ${E2E_OPERATOR_EMAIL}; operator org not seeded"

declare -A FIXTURE_TOKENS=() FIXTURE_REFRESH_TOKENS=() PERSONA_IDS=() PERSONA_ORGS=() PERSONA_ROLES=()
declare -a FIXTURE_ACTORS=("$E2E_OPERATOR_EMAIL" "demo@g5labs.ai")
FIXTURE_TOKENS["$E2E_OPERATOR_EMAIL"]="$OP_TOKEN"
FIXTURE_TOKENS["demo@g5labs.ai"]="$TOKEN"
FIXTURE_REFRESH_TOKENS["$E2E_OPERATOR_EMAIL"]=$(printf '%s' "$OP_RESP" | jq -er '.data.refreshToken') \
  || operator_fail "Missing fixture operator refresh token"
FIXTURE_REFRESH_TOKENS["demo@g5labs.ai"]=$(printf '%s' "$RESP" | jq -er '.data.refreshToken') \
  || persona_fail "Missing initial fixture refresh token"

while IFS=$'\x1f' read -r P_EMAIL P_NAME P_AVATAR P_ROLE; do
  P_RESP=$(curl -sf "${API_URL}/api/v1/auth/test-token" \
    -H "Content-Type: application/json" \
    -d "$(jq -nc --arg email "$P_EMAIL" '{email: $email}')" 2>/dev/null || echo "")
  P_TOKEN=$(printf '%s' "$P_RESP" | jq -r '.data.jwt // empty' 2>/dev/null || echo "")
  [ -n "$P_TOKEN" ] || persona_fail "Could not mint a test token for dev persona ${P_EMAIL}"

  curl -sf -X PATCH "${API_URL}/api/v1/users/me" \
    -H "Authorization: Bearer ${P_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$(jq -nc --arg name "$P_NAME" --arg avatar "$P_AVATAR" '{displayName: $name, avatarUrl: $avatar}')" \
    > /dev/null 2>&1 \
    || persona_fail "Could not set the profile of dev persona ${P_EMAIL}"

  PROFILE=$(curl -sf "${API_URL}/api/v1/users/me" -H "Authorization: Bearer ${P_TOKEN}") \
    || persona_fail "Could not read persona ${P_EMAIL}"
  P_USER_ID=$(printf '%s' "$PROFILE" | jq -er '.data.id') \
    || persona_fail "Missing persona user id for ${P_EMAIL}"
  P_ORGS=$(printf '%s' "$PROFILE" | jq -er '[.data.orgs[] | select(.orgSlug == "g5labs" or .orgSlug == "g5labs-member") | .orgId] | if length == 2 then .[] else error("missing fixture org") end') \
    || persona_fail "Missing fixture organizations for ${P_EMAIL}"
  if [ -z "${FIXTURE_TOKENS[$P_EMAIL]+present}" ]; then
    FIXTURE_ACTORS+=("$P_EMAIL")
  fi
  FIXTURE_TOKENS["$P_EMAIL"]="$P_TOKEN"
  FIXTURE_REFRESH_TOKENS["$P_EMAIL"]=$(printf '%s' "$P_RESP" | jq -er '.data.refreshToken') \
    || persona_fail "Missing refresh token for dev persona ${P_EMAIL}"
  PERSONA_IDS["$P_EMAIL"]="$P_USER_ID"
  PERSONA_ORGS["$P_EMAIL"]="$P_ORGS"
  PERSONA_ROLES["$P_EMAIL"]="$P_ROLE"
done <<< "$PERSONAS"

# Refresh scopes for the actual target organization. A token issued while a
# persona was Member cannot administer it merely because the role was promoted.
fixture_token_for_org() {
  local email="$1" org_id="$2"
  curl -sf "${API_URL}/api/v1/auth/refresh" \
    -H "Content-Type: application/json" \
    -d "$(jq -nc --arg refresh "${FIXTURE_REFRESH_TOKENS[$email]}" --arg org "$org_id" \
      '{refreshToken: $refresh, orgId: $org}')" \
    | jq -er '.data.jwt | select(type == "string" and length > 0)'
}

# /users/me lists only active memberships. Select among the known fixture
# identities, never mint an arbitrary replacement or repair roles with SQL.
# An identity absent from the roster keeps its current role. The following
# PATCH remains authoritative if membership changes after this read.
fixture_admin_token() {
  local org_id="$1" target_role="$2" email token profile
  for email in "${FIXTURE_ACTORS[@]}"; do
    if [ "$target_role" != "admin" ] && [ "${PERSONA_ROLES[$email]:-admin}" != "admin" ]; then
      continue
    fi
    token="${FIXTURE_TOKENS[$email]}"
    profile=$(curl -sf "${API_URL}/api/v1/users/me" -H "Authorization: Bearer ${token}") || continue
    if printf '%s' "$profile" | jq -e --arg org "$org_id" \
      'any(.data.orgs[]?; .orgId == $org and .role == "admin")' > /dev/null; then
      token=$(fixture_token_for_org "$email" "$org_id") || continue
      printf '%s' "$token"
      return 0
    fi
  done
  return 1
}

while IFS=$'\x1f' read -r P_EMAIL P_NAME P_AVATAR P_ROLE; do
  P_TOKEN="${FIXTURE_TOKENS[$P_EMAIL]}"
  P_USER_ID="${PERSONA_IDS[$P_EMAIL]}"
  P_ORGS="${PERSONA_ORGS[$P_EMAIL]}"
  while IFS= read -r P_ORG_ID; do
    ROLE_TOKEN=$(fixture_admin_token "$P_ORG_ID" "$P_ROLE") \
      || persona_fail "No authorized fixture Admin available for role '${P_ROLE}' in ${P_ORG_ID}; dev personas not seeded"
    curl -sf -X PATCH "${API_URL}/api/v1/orgs/${P_ORG_ID}/members/${P_USER_ID}" \
      -H "Authorization: Bearer ${ROLE_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "$(jq -nc --arg role "$P_ROLE" '{role: $role}')" > /dev/null 2>&1 \
      || persona_fail "Could not apply role '${P_ROLE}' to dev persona ${P_EMAIL}"
  done <<< "$P_ORGS"

  # Verify through the API what a sign-in will see: the profile and the role in
  # both fixture orgs (also catches an update that matched no membership).
  curl -sf "${API_URL}/api/v1/users/me" -H "Authorization: Bearer ${P_TOKEN}" 2>/dev/null \
    | jq -e --arg name "$P_NAME" --arg role "$P_ROLE" '
        .data.displayName == $name
        and ([.data.orgs[] | select(.orgSlug == "g5labs" or .orgSlug == "g5labs-member") | .role]
             | length == 2 and all(. == $role))' > /dev/null 2>&1 \
    || persona_fail "Dev persona ${P_EMAIL} did not verify as ${P_NAME} (${P_ROLE}) in g5labs and g5labs-member"
  echo "  ✓ Dev persona ${P_NAME} <${P_EMAIL}> seeded (${P_ROLE})" >&2
done <<< "$PERSONAS"

# ── Make Alex a platform operator, as the platform seed does in prod ─────────
# Prod's seed-prod-org.sh gives DEFAULT_ADMIN_EMAIL an administrator invitation
# to the operator org (g5.organization.platform-operator.bootstrap). Here the
# test-token fixture operator persona provisions that org on its first mint,
# then adds DEV_OPERATOR_EMAIL through the ordinary member API; the person
# already exists, so the membership is immediate. A re-run restores the
# administrator role if it was changed.
DEV_OPERATOR_EMAIL="${DEV_OPERATOR_EMAIL:-demo@g5labs.ai}"
OPERATOR_ORG_ID=$(curl -sf "${API_URL}/api/v1/orgs" -H "Authorization: Bearer ${OP_TOKEN}" 2>/dev/null \
  | jq -r '.data[] | select(.orgSlug == "operator") | .orgId' 2>/dev/null | head -1 || echo "")
[ -n "$OPERATOR_ORG_ID" ] || operator_fail "Operator org not found for ${E2E_OPERATOR_EMAIL}"
OP_TOKEN=$(fixture_token_for_org "$E2E_OPERATOR_EMAIL" "$OPERATOR_ORG_ID") \
  || operator_fail "Could not refresh the fixture operator session for the operator org"

MEMBER=$(curl -sf -X POST "${API_URL}/api/v1/orgs/${OPERATOR_ORG_ID}/members" \
  -H "Authorization: Bearer ${OP_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "$(jq -nc --arg email "$DEV_OPERATOR_EMAIL" '{email: $email, role: "admin"}')" 2>/dev/null \
  | jq -c '.data.membership // empty' 2>/dev/null || echo "")
[ -n "$MEMBER" ] || operator_fail "Could not add ${DEV_OPERATOR_EMAIL} to the operator org"
if [ "$(printf '%s' "$MEMBER" | jq -r '.role')" != "admin" ]; then
  curl -sf -X PATCH "${API_URL}/api/v1/orgs/${OPERATOR_ORG_ID}/members/$(printf '%s' "$MEMBER" | jq -r '.userId')" \
    -H "Authorization: Bearer ${OP_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"role":"admin"}' > /dev/null 2>&1 \
    || operator_fail "Could not restore ${DEV_OPERATOR_EMAIL}'s operator administrator role"
fi

# Verify as the operator what a sign-in will see.
DEV_OP_TOKEN=$(curl -sf "${API_URL}/api/v1/auth/test-token" \
  -H "Content-Type: application/json" \
  -d "$(jq -nc --arg email "$DEV_OPERATOR_EMAIL" '{email: $email}')" 2>/dev/null \
  | jq -r '.data.jwt // empty' 2>/dev/null || echo "")
curl -sf "${API_URL}/api/v1/users/me" -H "Authorization: Bearer ${DEV_OP_TOKEN}" 2>/dev/null \
  | jq -e '[.data.orgs[] | select(.orgSlug == "operator") | .role] == ["admin"]' > /dev/null 2>&1 \
  || operator_fail "${DEV_OPERATOR_EMAIL} did not verify as an operator administrator"
echo "  ✓ ${DEV_OPERATOR_EMAIL} is a platform operator (admin of /operator)" >&2

# Output workspace ID on stdout
echo "${WS_ID}"
