#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/self-hosted-azure-install.yml"
AZURE_DIR="$ROOT_DIR/scripts/self-hosted-azure"
WORKERHOST_SCRIPT="$ROOT_DIR/infra/self-hosted/scripts/g5-workerhost.sh"

die() {
  echo "azure microVM canary contract test failed: $*" >&2
  exit 1
}

require_text() {
  local file="$1"
  local text="$2"
  grep -Fq -- "$text" "$file" || die "$file missing contract text: $text"
}

reject_text() {
  local file="$1"
  local text="$2"
  if grep -Fq -- "$text" "$file"; then
    die "$file contains forbidden text: $text"
  fi
}

require_before() {
  local file="$1"
  local first="$2"
  local second="$3"
  local first_line second_line
  first_line="$(grep -nF -- "$first" "$file" | head -n 1 | cut -d: -f1)"
  second_line="$(grep -nF -- "$second" "$file" | head -n 1 | cut -d: -f1)"
  [ -n "$first_line" ] || die "$file missing ordered contract text: $first"
  [ -n "$second_line" ] || die "$file missing ordered contract text: $second"
  [ "$first_line" -lt "$second_line" ] \
    || die "$file must place '$first' before '$second'"
}

for script in \
  "$AZURE_DIR/bootstrap-ubuntu-host.sh" \
  "$AZURE_DIR/provision-control-tls.sh" \
  "$AZURE_DIR/install-control-plane.sh" \
  "$AZURE_DIR/install-worker-host.sh" \
  "$AZURE_DIR/verify-worker-hosts.sh"; do
  bash -n "$script"
done
node --check "$AZURE_DIR/login-smoke-common.js"

# G5Labs/G5-Lean#527 (operations g5.quality.delivery.pinned-third-party-tools):
# the root bootstrap installs the pinned, sha256-verified just release (the
# same version and digest as .github/actions/setup-just) and never pipes the
# mutable upstream installer into a shell; the workflow runs it as root.
reject_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'just.systems'
reject_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" '| bash'
require_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'JUST_VERSION=1.55.1'
require_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'JUST_SHA256_X86_64=b0ef600f0df20d5ae91ae931627c499fc52b477ffe5f5ea7b7b3ec616b16c778'
require_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'https://github.com/casey/just/releases/download/${JUST_VERSION}/${tarball}'
require_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'sha256sum --check --status'
require_before "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'sha256sum --check --status' 'tar -xzf "${work}/${tarball}"'
require_text "$AZURE_DIR/bootstrap-ubuntu-host.sh" 'install_pinned_just /usr/local/bin'
require_text "$WORKFLOW" 'sudo bash /tmp/bootstrap-ubuntu-host.sh'
node --check "$AZURE_DIR/github-login-smoke.js"
node --check "$AZURE_DIR/entra-login-smoke.js"
G5_ENTRA_SMOKE_SELF_TEST=1 node "$AZURE_DIR/entra-login-smoke.js"
G5_GITHUB_SMOKE_SELF_TEST=1 node "$AZURE_DIR/github-login-smoke.js"

require_text "$WORKFLOW" 'default: "Standard_D4s_v3"'
require_text "$WORKFLOW" 'microVM canary requires worker_hosts=1'
require_text "$WORKFLOW" '--assign-identity'
require_text "$WORKFLOW" '--role AcrPull'
require_text "$WORKFLOW" 'az provider register --namespace Microsoft.ContainerRegistry --wait'
require_text "$WORKFLOW" 'azure/login@7184910d9eb2b1c5e48f7073824a90609bb9b6d6'
reject_text "$WORKFLOW" 'azure/login@v2'
require_text "$WORKFLOW" 'G5D_ISOLATION_MODE=microvm'
require_text "$WORKFLOW" 'G5D_BASE_IMAGE=$task_image_q'
require_text "$WORKFLOW" 'G5D_HOST_BASE_IMAGE=$task_image_q'
require_text "$WORKFLOW" 'OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}'
require_text "$WORKFLOW" '-e OPENROUTER_API_KEY'
reject_text "$WORKFLOW" 'G5_REQUIRE_ROUTABLE_PREVIEW'
require_text "$WORKFLOW" '-e G5_VERIFY_CHAT -e G5_CHAT_MARKER'
require_text "$WORKFLOW" 'ANTHROPIC_AUTH_TOKEN=$provider_key'
require_text "$WORKFLOW" 'ANTHROPIC_BASE_URL=https://openrouter.ai/api'
reject_text "$WORKFLOW" 'ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}'
require_text "$WORKFLOW" 'G5_TLS_CERT_FILE=/var/lib/g5/tls/fullchain.pem'
require_text "$WORKFLOW" 'G5_TLS_CADDYFILE=/opt/g5/Caddyfile.tls docker compose --env-file /opt/g5/.env'
reject_text "$WORKFLOW" '"sudo docker compose --env-file /opt/g5/.env'
reject_text "$WORKFLOW" 'then docker compose --env-file /opt/g5/.env'
require_text "$WORKFLOW" 'G5_SYNC_QUESTION_V1_STAMPING=true'
require_text "$WORKFLOW" 'scripts/self-hosted-azure/compose.question-v1.yml'
require_text "$WORKFLOW" 'network private-dns record-set a add-record'
require_text "$WORKFLOW" 'Building rootfs template'
require_text "$WORKFLOW" 'cloned pristine block workload template'
require_text "$WORKFLOW" 'Creating microVM \(Cloud Hypervisor\)'
require_text "$WORKFLOW" 'g5d::guest'
reject_text "$WORKFLOW" 'Claude Code session started'
reject_text "$WORKFLOW" 'G5D_LLM_PROXY'
reject_text "$WORKFLOW" 'G5D_GUEST_ATTESTATION_ADVISORY'
require_text "$WORKFLOW" '  schedule:'
require_text "$WORKFLOW" "vars.G5_CANARY_ADMIN_API_URL || 'https://qa.g5labs.ai'"
require_text "$WORKFLOW" 'package-self-hosted-release-assets.sh'
reject_text "$WORKFLOW" 'g5-microvm-runtime-source.tar.gz'
reject_text "$WORKFLOW" 'G5_CLAUDE_CODE_FAKE=1'
reject_text "$AZURE_DIR/install-control-plane.sh" 'compose.azure-https.yml'
reject_text "$AZURE_DIR/install-control-plane.sh" 'Caddyfile.azure'

require_text "$AZURE_DIR/install-control-plane.sh" '--bundle-dir "$BUNDLE_DIR"'
require_text "$AZURE_DIR/install-control-plane.sh" 'G5_EXTRA_COMPOSE_FILE'
require_text "$AZURE_DIR/install-control-plane.sh" 'G5_TLS_CADDYFILE="${G5_TLS_CADDYFILE:-$INSTALL_DIR/Caddyfile.tls}"'
require_text "$AZURE_DIR/install-control-plane.sh" 'export G5_TLS_CADDYFILE'
require_text "$AZURE_DIR/install-control-plane.sh" 'release g5-api container is not running'
require_text "$AZURE_DIR/install-control-plane.sh" 'releaseApiImage'
reject_text "$AZURE_DIR/install-control-plane.sh" 'cargo build'
reject_text "$AZURE_DIR/install-control-plane.sh" 'g5-api-canary:'
require_text "$AZURE_DIR/install-control-plane.sh" "column_name = 'external_host_id'"
require_text "$AZURE_DIR/install-control-plane.sh" "pg_get_functiondef('authz.lookup_worker_host_for_auth(uuid)'::regprocedure)"
require_text "$AZURE_DIR/compose.question-v1.yml" 'G5_SYNC_QUESTION_V1_STAMPING:'
require_text "$AZURE_DIR/install-worker-host.sh" '--worker-host'
require_text "$AZURE_DIR/install-worker-host.sh" '--host-jwt "$G5_HOST_JWT"'
require_text "$AZURE_DIR/install-worker-host.sh" '--host-id "$G5D_HOST_ID"'
reject_text "$AZURE_DIR/install-worker-host.sh" 'G5D_REGISTRATION_TOKEN'
require_text "$AZURE_DIR/install-worker-host.sh" '--bundle-dir "$BUNDLE_DIR"'
require_text "$AZURE_DIR/install-worker-host.sh" 'g5-apt-install-with-retry'
require_text "$WORKFLOW" 'scripts/apt-install-with-retry.sh'
require_text "$AZURE_DIR/install-worker-host.sh" 'xfsprogs'
require_text "$AZURE_DIR/verify-worker-hosts.sh" "kind = 'self_hosted'"
require_text "$AZURE_DIR/verify-worker-hosts.sh" 'G5_TLS_CADDYFILE="${G5_TLS_CADDYFILE:-$INSTALL_DIR/Caddyfile.tls}"'
require_text "$AZURE_DIR/verify-worker-hosts.sh" 'export G5_TLS_CADDYFILE'
require_text "$AZURE_DIR/verify-worker-hosts.sh" "status = 'ready'"
require_text "$AZURE_DIR/verify-worker-hosts.sh" 'if [ "$count" -eq "$expected" ]; then'
require_text "$AZURE_DIR/verify-worker-hosts.sh" 'if [ "$count" -gt "$expected" ]; then'
require_text "$AZURE_DIR/verify-worker-hosts.sh" 'saw unexpected excess'
reject_text "$AZURE_DIR/verify-worker-hosts.sh" '"${count:-0}" -ge "$expected"'
require_text "$WORKERHOST_SCRIPT" 'export G5D_API_URL G5_HOST_JWT G5D_ORG_ID G5D_HOST_ID'
require_text "$WORKERHOST_SCRIPT" 'install-worker-service.py'
require_text "$WORKERHOST_SCRIPT" 'start_microvm_service'
require_text "$WORKERHOST_SCRIPT" 'stage-local-worker-runtime.sh'
require_text "$WORKERHOST_SCRIPT" 'G5D_ISOLATION_MODE=microvm'
require_text "$WORKERHOST_SCRIPT" 'mkfs.xfs -f -q -m reflink=1'
require_text "$WORKERHOST_SCRIPT" 'cp --reflink=always'
require_text "$ROOT_DIR/infra/self-hosted/scripts/install-worker-service.py" 'ExecStart=/usr/local/bin/g5d host'
require_text "$ROOT_DIR/infra/self-hosted/scripts/install-worker-service.py" 'LimitNOFILE=1048576'
require_text "$WORKERHOST_SCRIPT" 'resolve_kvm_gid() {'
require_text "$WORKERHOST_SCRIPT" 'kvm_gid="$(resolve_kvm_gid)"'
require_text "$WORKERHOST_SCRIPT" '--group-add="$kvm_gid"'
require_text "$WORKERHOST_SCRIPT" 'verify_microvm_stability'
require_text "$WORKERHOST_SCRIPT" 'systemctl show --property=NRestarts --value'
reject_text "$WORKERHOST_SCRIPT" '--privileged'
reject_text "$WORKERHOST_SCRIPT" 'G5_ACK_PROCESS_ISOLATION'
reject_text "$WORKERHOST_SCRIPT" 'G5D_WORKER_DISPLAY_NAME'
reject_text "$WORKERHOST_SCRIPT" 'G5D_REGISTRATION_TOKEN'
reject_text "$WORKFLOW" 'g5-azure-start-microvm-host'
reject_text "$WORKERHOST_SCRIPT" '-e G5D_DISPLAY_NAME='
require_text "$AZURE_DIR/login-smoke-common.js" 'agentProvider: "claude_code"'
require_text "$AZURE_DIR/login-smoke-common.js" 'Assistant response completed without required markers'
require_text "$AZURE_DIR/login-smoke-common.js" 'g5-openrouter-microvm-ok'
require_text "$AZURE_DIR/login-smoke-common.js" 'Do not read or modify files and do not invoke tools'
reject_text "$AZURE_DIR/login-smoke-common.js" 'Read /etc/g5-canary-marker'
require_text "$AZURE_DIR/login-smoke-common.js" '/config?orgId='
require_text "$AZURE_DIR/login-smoke-common.js" 'setSecrets: { ANTHROPIC_AUTH_TOKEN: openrouterKey }'
require_text "$AZURE_DIR/login-smoke-common.js" 'effectiveSecretKeys?.includes("ANTHROPIC_AUTH_TOKEN")'
require_text "$AZURE_DIR/login-smoke-common.js" 'runtimeConfigSecretKey: "ANTHROPIC_AUTH_TOKEN"'
require_text "$AZURE_DIR/login-smoke-common.js" 'ANTHROPIC_BASE_URL: "https://openrouter.ai/api"'
require_text "$AZURE_DIR/login-smoke-common.js" '/clone'
require_text "$AZURE_DIR/login-smoke-common.js" '/worker-hosts'
require_text "$AZURE_DIR/login-smoke-common.js" 'G5_PROVISION_HOST_OUTPUT'
require_text "$AZURE_DIR/login-smoke-common.js" 'G5_PROVISION_HOST_BASE_IMAGE'
require_text "$AZURE_DIR/login-smoke-common.js" 'docType: "design"'
require_text "$AZURE_DIR/login-smoke-common.js" 'filePath: `docs/${fork.id}/${docName}.md`'
require_text "$AZURE_DIR/login-smoke-common.js" 'const previewManifestPath = ".g5/workspace.yaml"'
require_text "$AZURE_DIR/login-smoke-common.js" 'const previewServerPath = "canary-preview-server.js"'
require_text "$AZURE_DIR/login-smoke-common.js" '`  command: \"node ${previewServerPath}\"`'
require_text "$AZURE_DIR/login-smoke-common.js" '/resume'
require_text "$AZURE_DIR/login-smoke-common.js" '/changes'
require_text "$AZURE_DIR/login-smoke-common.js" '/files/${filePath}?repoId='
require_text "$AZURE_DIR/login-smoke-common.js" '/preview/'
require_text "$AZURE_DIR/login-smoke-common.js" 'preview?.ready && preview.status === "running"'
reject_text "$AZURE_DIR/login-smoke-common.js" 'jobType: "sync"'
reject_text "$AZURE_DIR/login-smoke-common.js" 'jobType: "preview"'
reject_text "$AZURE_DIR/login-smoke-common.js" 'requiredWorkerId'
reject_text "$AZURE_DIR/login-smoke-common.js" '/drafts'
require_text "$AZURE_DIR/login-smoke-common.js" 'locator.isEnabled'
require_text "$AZURE_DIR/login-smoke-common.js" 'renderedText.includes(chatMarker)'
reject_text "$AZURE_DIR/login-smoke-common.js" 'blocked-known-defect'
reject_text "$AZURE_DIR/login-smoke-common.js" 'previewBlockerIssue'
require_before "$AZURE_DIR/login-smoke-common.js" '/preview/' '/sessions/start'
require_text "$AZURE_DIR/login-smoke-common.js" '/preview/cancel'

require_text "$AZURE_DIR/login-smoke-common.js" 'async function runLoginSmoke('
require_text "$AZURE_DIR/github-login-smoke.js" 'loginWithGitHub'
require_text "$AZURE_DIR/github-login-smoke.js" 'require("./login-smoke-common")'
require_text "$AZURE_DIR/entra-login-smoke.js" 'require("./login-smoke-common")'
require_text "$AZURE_DIR/entra-login-smoke.js" 'loginWithMicrosoft'
require_text "$AZURE_DIR/entra-login-smoke.js" 'continue with microsoft'
require_text "$AZURE_DIR/entra-login-smoke.js" 'chooseMicrosoftWorkAccountIfRequested'
require_text "$AZURE_DIR/entra-login-smoke.js" '/^Work or school account$/i'
require_text "$AZURE_DIR/entra-login-smoke.js" 'submitMicrosoftPasswordIfRequested'
require_text "$AZURE_DIR/entra-login-smoke.js" 'fillMicrosoftTotpIfRequested'
require_text "$AZURE_DIR/entra-login-smoke.js" 'isMicrosoftSecurityRegistrationUrl'
require_text "$AZURE_DIR/entra-login-smoke.js" 'AZURE_ENTRA_TEST_USER_TOTP_SECRET'

# Entra ID is the required canary login; GitHub login is optional beside it.
require_text "$WORKFLOW" 'name: Ensure Entra redirect URI'
require_before "$WORKFLOW" 'name: Azure login' 'name: Ensure Entra redirect URI'
require_before "$WORKFLOW" 'name: Ensure Entra redirect URI' 'name: Install control plane and verify clean install'
require_text "$WORKFLOW" 'APP_OBJECT_ID: ${{ secrets.AZURE_ENTRA_APP_OBJECT_ID }}'
require_text "$WORKFLOW" '/api/v1/auth/callback/microsoft'
require_text "$WORKFLOW" 'https://graph.microsoft.com/v1.0/applications/${APP_OBJECT_ID}'
require_text "$WORKFLOW" 'G5_MICROSOFT_CLIENT_ID: ${{ secrets.AZURE_ENTRA_CLIENT_ID }}'
require_text "$WORKFLOW" 'G5_MICROSOFT_CLIENT_SECRET: ${{ secrets.AZURE_ENTRA_CLIENT_SECRET }}'
require_text "$WORKFLOW" 'AZURE_ENTRA_TENANT_ID: ${{ secrets.AZURE_ENTRA_TENANT_ID }}'
require_text "$WORKFLOW" 'entra_tenant="${AZURE_ENTRA_TENANT_ID:-$AZURE_TENANT_ID}"'
require_text "$WORKFLOW" 'G5_MICROSOFT_CLIENT_ID=$G5_MICROSOFT_CLIENT_ID'
require_text "$WORKFLOW" 'G5_MICROSOFT_TENANT_ID=$entra_tenant'
reject_text "$WORKFLOW" 'G5_OAUTH_ENABLED_PROVIDERS'
reject_text "$WORKFLOW" 'G5_MICROSOFT_DEFAULT_ORG_ROLE_MAP'
require_text "$WORKFLOW" 'G5_ENTRA_EMAIL: ${{ secrets.AZURE_ENTRA_TEST_USER_EMAIL }}'
require_text "$WORKFLOW" 'G5_ENTRA_PASSWORD: ${{ secrets.AZURE_ENTRA_TEST_USER_PASSWORD }}'
require_text "$WORKFLOW" 'G5_ENTRA_TOTP_SECRET: ${{ secrets.AZURE_ENTRA_TEST_USER_TOTP_SECRET }}'
require_text "$WORKFLOW" '-e G5_ENTRA_EMAIL -e G5_ENTRA_PASSWORD -e G5_ENTRA_TOTP_SECRET'
require_text "$WORKFLOW" 'node /work/smoke/entra-login-smoke.js'
require_text "$WORKFLOW" '-v "$PWD/scripts/self-hosted-azure:/work/smoke:ro"'
reject_text "$WORKFLOW" '/work/github-login-smoke.js'
require_text "$WORKFLOW" 'name: Verify real Entra-authenticated chat executes in the ACR microVM'
require_before "$WORKFLOW" 'node /work/smoke/entra-login-smoke.js' 'node /work/smoke/github-login-smoke.js'
require_text "$WORKFLOW" 'name: Verify optional GitHub login beside Entra'
require_text "$WORKFLOW" 'G5_GITHUB_CLIENT_ID=${G5_GITHUB_CLIENT_ID:-}'
require_text "$WORKFLOW" 'SELF_HOSTED_GITHUB_CLIENT_ID is set without SELF_HOSTED_GITHUB_CLIENT_SECRET'
reject_text "$WORKFLOW" 'SELF_HOSTED_GITHUB_CLIENT_ID is required'
reject_text "$WORKFLOW" 'SELF_HOSTED_GITHUB_TEST_USER_EMAIL is required'
require_text "$AZURE_DIR/install-control-plane.sh" 'expected = ["microsoft"]'
require_text "$AZURE_DIR/install-control-plane.sh" 'expected = ["github", "microsoft"]'
reject_text "$AZURE_DIR/install-control-plane.sh" 'expected only github auth provider'
reject_text "$WORKFLOW" 'G5D_REGISTRATION_TOKEN'

echo "azure microVM canary contract test passed"

reject_text "$WORKFLOW" 'G5D_AGENT_CARRIER_LIVE'
reject_text "$WORKERHOST_SCRIPT" 'G5D_AGENT_CARRIER_LIVE'
