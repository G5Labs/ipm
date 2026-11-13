#!/usr/bin/env bash
set -euo pipefail

MODE="single-api"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:?--mode requires a value}"
      shift 2
      ;;
    --help|-h)
      cat <<'USAGE'
Usage: ./scripts/provided-agents-manual-e2e.sh [--mode single-api]

Prints a manual Claude Code/Codex fake-binary E2E checklist and worker command
blocks. It does not print bearer tokens or provider secrets.
USAGE
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ "$MODE" != "single-api" ]]; then
  echo "Mode '$MODE' is not implemented yet. Use --mode single-api." >&2
  exit 2
fi

API_URL="${G5_API_URL:-http://localhost:7500}"
BROWSER_URL="${G5_BROWSER_URL:-http://localhost:3000}"
ORG_ID="${G5_ORG_ID:-<org-uuid>}"
WORKSPACE_ID="${G5_WORKSPACE_ID:-<workspace-uuid>}"

cat <<EOF2
=== G5 Provided Agents manual E2E helper ===

Mode:        ${MODE}
Browser URL: ${BROWSER_URL}
API URL:     ${API_URL}
Org ID:      ${ORG_ID}
Workspace ID:${WORKSPACE_ID}

This helper is intentionally print-only. Start/reuse the dev stack using the
normal repo workflow, then run the command blocks below in a temporary worker
directory. No bearer tokens or provider secrets are printed.

Common setup (start in the G5-Lean checkout):

  cargo build -p g5d -p fake-provider-cli --bins
  export G5D_BIN="\$PWD/target/debug/g5d"
  export G5_CLAUDE_PATH="\$PWD/target/debug/fake-claude"
  export G5_CODEX_PATH="\$PWD/target/debug/fake-codex"
  # Use non-production test credentials in Runtime Config for both providers.

  mkdir -p /tmp/g5-provided-agents-manual
  cd /tmp/g5-provided-agents-manual
  export G5_API_URL='${API_URL}'
  export G5D_AUTH_STATE="\$PWD/.g5-worker-auth.json"
  cat > providers.toml <<'PROVIDERS'
[claude_code]
enabled = true

[codex]
enabled = true
PROVIDERS

Org-scoped primary worker flow:

  export G5_ORG_ID='${ORG_ID}'
  "\$G5D_BIN" --api-url "\$G5_API_URL" auth login --org-id "\$G5_ORG_ID"
  "\$G5D_BIN" --api-url "\$G5_API_URL" worker \\
    --org "\$G5_ORG_ID" \\
    --workspace-path "\$PWD" \\
    --provider-configs providers.toml

Workspace-scoped worker flow:

  export G5_WORKSPACE_ID='${WORKSPACE_ID}'
  "\$G5D_BIN" --api-url "\$G5_API_URL" auth login --workspace-id "\$G5_WORKSPACE_ID"
  "\$G5D_BIN" --api-url "\$G5_API_URL" worker \\
    --workspace "\$G5_WORKSPACE_ID" \\
    --workspace-path "\$PWD" \\
    --provider-configs providers.toml

Manual checklist:

  1. Worker install UI copy shows org/workspace setup clearly.
  2. Worker display name is presentation-only; UUID/id remains subtext.
  3. Claude Code and Codex are the only provider choices.
  4. Send 'hello' with each provider and verify its fake CLI greeting.
  5. Send a follow-up and reload; the transcript and provider stay the same.
  6. Stop the worker; the bound session shows that worker is unavailable.
  7. Restart the same worker from the same checkout/context; its identity and pinned session recover.
  8. '--reset-worker-identity' creates a new identity and does not recover the old pinned session.
  9. 'g5d auth status --global' / 'logout --global' behave as expected.

Critical recovery path to verify:
  worker loss -> errors visible -> a different worker does not recover the old session ->
  original same-place/same-scope worker restart reuses "\$PWD/.g5/worker_identity.json" ->
  same worker-bound session sends hello -> normal completion.
EOF2
