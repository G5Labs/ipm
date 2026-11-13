#!/usr/bin/env bash
# =============================================================================
# bootstrap-secrets.sh — Seed GCP Secret Manager secrets for G5
# =============================================================================
# Generates auto-generated secrets (JWT ES256 key pair, worker registration
# token placeholder, SCM encryption key) and prompts for external credentials
# (GitHub OAuth, LLM API keys). Stores everything in GCP Secret
# Manager.
#
# This script keeps secret material out of Terraform state.  Terraform creates
# the Secret Manager *containers* (google_secret_manager_secret); this script
# adds the first *version* with actual values.
#
# Prerequisites:
#   - gcloud CLI authenticated with secretmanager.versions.add permission
#   - openssl (for EC key generation)
#   - The Secret Manager secret containers must already exist (terraform apply)
#
# Usage:
#   ./scripts/bootstrap-secrets.sh                          # interactive
#   ./scripts/bootstrap-secrets.sh --project g5-dev-1ccc    # explicit project
#   ./scripts/bootstrap-secrets.sh --project g5-qa-1ccc \
#       --external-url https://qa.g5labs.ai \
#       --license-admin-secret-id g5-license-admin-token-qa
#   ./scripts/bootstrap-secrets.sh --force                  # overwrite existing
#
# Environment variables (skip prompts by setting these):
#   G5_GITHUB_CLIENT_ID, G5_GITHUB_CLIENT_SECRET
#   G5_LICENSE_ADMIN_TOKEN
#   ANTHROPIC_API_KEY, OPENAI_API_KEY, OPENROUTER_API_KEY
# =============================================================================
set -euo pipefail
set +x # Never include generated or supplied credentials in shell tracing.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/jwt-keys.sh
source "$SCRIPT_DIR/lib/jwt-keys.sh"
# shellcheck source=scripts/lib/secret-target.sh
source "$SCRIPT_DIR/lib/secret-target.sh"

# Colours
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'
log()  { echo -e "${GREEN}[✓]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
err()  { echo -e "${RED}[✗]${NC} $*" >&2; }
info() { echo -e "${BLUE}[→]${NC} $*"; }
step() { echo -e "\n${CYAN}━━━ $* ━━━${NC}"; }

# Defaults
PROJECT=""
PROJECT_EXPLICIT=false
EXTERNAL_URL=""
EXTERNAL_URL_EXPLICIT=false
LICENSE_ADMIN_SECRET_ID=""
FORCE=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --project|--external-url|--license-admin-secret-id)
      [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || { err "$1 requires a value"; exit 1; }
      case "$1" in
        --project) PROJECT="$2"; PROJECT_EXPLICIT=true ;;
        --external-url) EXTERNAL_URL="${2%/}"; EXTERNAL_URL_EXPLICIT=true ;;
        --license-admin-secret-id) LICENSE_ADMIN_SECRET_ID="$2" ;;
      esac
      shift 2 ;;
    --force)    FORCE=true;   shift   ;;
    --help|-h)
      echo "Usage: $0 [--project PROJECT_ID] [--external-url URL] [--license-admin-secret-id ID] [--force]"
      echo "Staging/prod require explicit project and HTTPS origin; their license secret is derived."
      echo "Legacy dev defaults remain. QA defaults to its QA origin. --force rotates existing keys."
      exit 0 ;;
    *) err "Unknown argument: $1"; exit 1 ;;
  esac
done

# Reject a malformed explicit URL before even querying the local gcloud config.
[[ -z "$EXTERNAL_URL" ]] || validate_secret_url
if [[ -z "$PROJECT" ]]; then
  PROJECT=$(gcloud config get-value project 2>/dev/null)
  if [[ -z "$PROJECT" ]]; then
    err "No project set. Use --project or 'gcloud config set project PROJECT_ID'"
    exit 1
  fi
fi
validate_secret_target bootstrap
LICENSE_ADMIN_SECRET_ID="${LICENSE_ADMIN_SECRET_ID:-g5-license-admin-token}"
[[ "$LICENSE_ADMIN_SECRET_ID" =~ ^[A-Za-z0-9_-]+$ ]] || { err "Invalid secret ID"; exit 1; }
readonly PROJECT EXTERNAL_URL LICENSE_ADMIN_SECRET_ID

info "Project: $PROJECT"
info "External URL: $EXTERNAL_URL"
info "Login callback URL: $EXTERNAL_URL/api/v1/auth/callback/github"

# Never combine a newly generated key with a previously seeded counterpart.
# Only metadata is read; repairing a partial pair requires an explicit full
# rotation (--force) or a separate operator-reviewed recovery of the missing key.
secret_version_count() {
  gcloud secrets versions list "$1" --project="$PROJECT" \
    --format="value(name)" 2>/dev/null | wc -l | tr -d ' '
}
for jwt_container in g5-jwt-secret g5-jwt-public-key; do
  gcloud secrets describe "$jwt_container" --project="$PROJECT" >/dev/null 2>&1 || {
    err "JWT secret container is missing or inaccessible; nothing written"
    exit 1
  }
done
jwt_private_count="$(secret_version_count g5-jwt-secret)"
jwt_public_count="$(secret_version_count g5-jwt-public-key)"
if [[ "$FORCE" != true ]] &&
  { [[ "$jwt_private_count" -gt 0 && "$jwt_public_count" -eq 0 ]] ||
    [[ "$jwt_private_count" -eq 0 && "$jwt_public_count" -gt 0 ]]; }; then
  err "Only one JWT key has versions; refusing to create a mismatched pair. Nothing written."
  err "Recover the missing key or explicitly review a full key rotation before using --force."
  exit 1
fi

# ---------------------------------------------------------------------------
# Helper: add a secret version (skip if version exists unless --force)
# ---------------------------------------------------------------------------
add_secret() {
  local secret_id="$1"
  local secret_value="$2"
  local description="${3:-}"

  if [[ -z "$secret_value" ]]; then
    secret_value="PLACEHOLDER"
    warn "  ○ $secret_id — no value provided, seeding placeholder (ESO requires a version)"
  fi

  # Check if secret container exists
  if ! gcloud secrets describe "$secret_id" --project="$PROJECT" &>/dev/null; then
    err "  ✗ $secret_id — secret container does not exist (run terraform apply first)"
    return 1
  fi

  # Check if a version already exists
  local version_count
  version_count=$(secret_version_count "$secret_id")

  if [[ "$version_count" -gt 0 ]] && [[ "$FORCE" != true ]]; then
    info "  ○ $secret_id — already has $version_count version(s), skipping (use --force to overwrite)"
    return 0
  fi

  printf '%s' "$secret_value" | gcloud secrets versions add "$secret_id" \
    --project="$PROJECT" --data-file=- >/dev/null 2>&1
  log "  ✓ $secret_id${description:+ — $description}"
}

# ---------------------------------------------------------------------------
# 1. Auto-generated secrets
# ---------------------------------------------------------------------------
step "Generating auto-generated secrets"

# JWT ES256 (ECDSA P-256) key pair
info "Generating ES256 key pair for JWT signing..."
JWT_PRIVATE_KEY=$(generate_es256_private_key)
JWT_PUBLIC_KEY=$(printf '%s\n' "$JWT_PRIVATE_KEY" | derive_es256_public_key)
if ! is_compatible_es256_private_key "$JWT_PRIVATE_KEY" || \
   ! es256_keypair_matches "$JWT_PRIVATE_KEY" "$JWT_PUBLIC_KEY"; then
  err "Failed to generate a valid PKCS#8 P-256 JWT key pair"
  exit 1
fi

# Worker registration token — placeholder until the environment bootstrap
# script mints a real token for the default org.
WORKER_REGISTRATION_TOKEN=""

# SCM encryption key — random 32-byte hex (64 chars for AES-256-GCM)
SCM_ENCRYPTION_KEY=$(openssl rand -hex 32)
G5_LICENSE_ADMIN_TOKEN="${G5_LICENSE_ADMIN_TOKEN:-$(openssl rand -hex 32)}"

step "Seeding auto-generated secrets"
add_secret "g5-jwt-secret"        "$JWT_PRIVATE_KEY"     "ES256 private key (PEM)"
add_secret "g5-jwt-public-key"    "$JWT_PUBLIC_KEY"      "ES256 public key (PEM)"
add_secret "g5-worker-registration-token" "$WORKER_REGISTRATION_TOKEN" "worker registration token (placeholder until org seed)"
add_secret "g5-scm-encryption-key" "$SCM_ENCRYPTION_KEY" "AES-256-GCM key (hex)"
add_secret "$LICENSE_ADMIN_SECRET_ID" "$G5_LICENSE_ADMIN_TOKEN" "self-hosted license admin token"

# ---------------------------------------------------------------------------
# 2. Database password — fetched from Cloud SQL via Terraform output
#    (The db_password secret version is managed by Terraform since it must
#    match the Cloud SQL user password set by random_password in the cloudsql
#    module.  Skip it here.)
# ---------------------------------------------------------------------------
info "g5-db-password is managed by Terraform (synced from Cloud SQL module)"

# ---------------------------------------------------------------------------
# 3. GitHub OAuth credentials
# ---------------------------------------------------------------------------
step "GitHub OAuth credentials"

if [[ -z "${G5_GITHUB_CLIENT_ID:-}" ]]; then
  echo ""
  info "Create a GitHub OAuth App at:"
  info "  https://github.com/organizations/G5Labs/settings/applications/new"
  info "  Login callback URL: $EXTERNAL_URL/api/v1/auth/callback/github"
  echo ""
  read -rp "GitHub OAuth Client ID (or Enter to skip): " G5_GITHUB_CLIENT_ID
  if [[ -n "$G5_GITHUB_CLIENT_ID" ]]; then
    read -rsp "GitHub OAuth Client Secret: " G5_GITHUB_CLIENT_SECRET
    echo ""
  fi
fi
add_secret "g5-github-oauth-client-id"     "${G5_GITHUB_CLIENT_ID:-}"
add_secret "g5-github-oauth-client-secret"  "${G5_GITHUB_CLIENT_SECRET:-}"

# ---------------------------------------------------------------------------
# 5. LLM API keys
# ---------------------------------------------------------------------------
step "LLM API keys"

if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
  read -rsp "Anthropic API key (or Enter to skip): " ANTHROPIC_API_KEY
  echo ""
fi
add_secret "g5-anthropic-api-key" "${ANTHROPIC_API_KEY:-}"

if [[ -z "${OPENAI_API_KEY:-}" ]]; then
  read -rsp "OpenAI API key (or Enter to skip): " OPENAI_API_KEY
  echo ""
fi
add_secret "g5-openai-api-key" "${OPENAI_API_KEY:-}"

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  read -rsp "OpenRouter API key (or Enter to skip): " OPENROUTER_API_KEY
  echo ""
fi
add_secret "g5-openrouter-api-key" "${OPENROUTER_API_KEY:-}"

# ---------------------------------------------------------------------------
# 6. Trigger ESO refresh
# ---------------------------------------------------------------------------
step "Summary"
echo ""
log "All secrets seeded in GCP Secret Manager (project: $PROJECT)"
echo ""
info "ESO will sync to K8s within its refresh interval (default: 1h)."
info "Use the $PROJECT cluster context when refreshing g5-api-secrets."
info "This script does not sync or restart workloads."
echo ""
