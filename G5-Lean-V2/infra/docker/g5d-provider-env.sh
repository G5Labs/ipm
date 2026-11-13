#!/usr/bin/env sh

g5d_write_default_provider_config() {
  if [ -n "${G5D_PROVIDER_CONFIGS:-}" ]; then
    return 0
  fi

  provider_config="${G5D_GENERATED_PROVIDER_CONFIGS:-/workspace/.g5/providers.toml}"
  mkdir -p "$(dirname "$provider_config")"

  cat > "$provider_config" <<EOF
[claude_code]
enabled = true

opus_model_env = "G5_CLAUDE_CODE_OPUS_MODEL"
sonnet_model_env = "G5_CLAUDE_CODE_SONNET_MODEL"
haiku_model_env = "G5_CLAUDE_CODE_HAIKU_MODEL"
anthropic_model_env = "G5_CLAUDE_CODE_ANTHROPIC_MODEL"
subagent_model_env = "G5_CLAUDE_CODE_SUBAGENT_MODEL"

# Codex is a first-class provided agent (worker provided_agent_runtime D31A).
# Declaring it here — rather than via G5D_CODEX_ENABLED — is required because
# the env auto-config path is skipped whenever G5D_PROVIDER_CONFIGS is set, so
# an env flag alone would never advertise Codex on Docker-packaged workers. The
# entry is readiness-gated: a worker image without the @openai/codex CLI fails
# readiness closed and advertises no agent_provider:codex capability.
[codex]
enabled = true
EOF
  export G5D_PROVIDER_CONFIGS="$provider_config"
}

g5d_setup_codex_env() {
  # Point the Codex provider at the CLI baked into the worker image so
  # readiness resolves deterministically instead of relying on the runtime
  # PATH. A non-empty preset (custom image) is preserved verbatim, even when
  # its target is missing; Codex readiness (worker check_readiness validates
  # the codex CLI) then fails closed and advertises no agent_provider:codex
  # capability. Under `G5_CODEX_FAKE=1` the worker prefers `fake-codex` beside
  # `g5d` over this pin.
  if [ -z "${G5_CODEX_PATH:-}" ] && [ -x /usr/local/bin/codex ]; then
    export G5_CODEX_PATH="/usr/local/bin/codex"
  fi
}

g5d_setup_openrouter_claude_env() {
  if [ -z "${OPENROUTER_API_KEY:-}" ]; then
    return 0
  fi

  g5_llm_provider="$(printf '%s' "${G5D_LLM_PROVIDER:-}" | tr '[:upper:]' '[:lower:]')"
  if [ "$g5_llm_provider" != "openrouter" ] && { [ -n "${ANTHROPIC_API_KEY:-}" ] || [ -n "${ANTHROPIC_AUTH_TOKEN:-}" ]; }; then
    return 0
  fi

  # Claude Code connects directly using its native bearer-token contract.
  # Clear a competing API key so the selected OpenRouter account wins.
  export ANTHROPIC_API_KEY=""
  export ANTHROPIC_AUTH_TOKEN="${ANTHROPIC_AUTH_TOKEN:-$OPENROUTER_API_KEY}"
  # Claude Code's Anthropic client appends its own /v1 paths. Use OpenRouter's
  # API root here so Claude's /v1/messages lands at /api/v1/messages, not
  # /api/v1/v1/messages.
  export ANTHROPIC_BASE_URL="${ANTHROPIC_BASE_URL:-${OPENROUTER_BASE_URL:-https://openrouter.ai/api}}"

  if [ -n "${G5D_LLM_MODEL:-}" ]; then
    export ANTHROPIC_MODEL="${ANTHROPIC_MODEL:-$G5D_LLM_MODEL}"
    export CLAUDE_CODE_SUBAGENT_MODEL="${CLAUDE_CODE_SUBAGENT_MODEL:-$G5D_LLM_MODEL}"
    export G5_CLAUDE_CODE_ANTHROPIC_MODEL="${G5_CLAUDE_CODE_ANTHROPIC_MODEL:-$G5D_LLM_MODEL}"
    export G5_CLAUDE_CODE_SUBAGENT_MODEL="${G5_CLAUDE_CODE_SUBAGENT_MODEL:-$G5D_LLM_MODEL}"
  fi
}

g5d_setup_claude_code_env() {
  # The pinned Claude Code CLI (scripts/claude-code-cli-version) is installed
  # at /usr/local/bin/claude by Dockerfile.g5d{,-prebuilt} with its runner flag
  # surface verified at build time; it is what the Phase 3 provider runner
  # spawns and what the microVM guest already pins as /opt/g5/bin/claude
  # (g5d-host GUEST_CLAUDE_PATH), so the Docker worker resolves the same
  # binary. An operator preset is preserved verbatim. Under
  # `G5_CLAUDE_CODE_FAKE=1` the worker prefers `fake-claude` beside `g5d`
  # over this pin.
  if [ -z "${G5_CLAUDE_PATH:-}" ] && [ -x /usr/local/bin/claude ]; then
    export G5_CLAUDE_PATH="/usr/local/bin/claude"
  fi
}

g5d_setup_provider_env() {
  g5d_setup_openrouter_claude_env
  g5d_setup_claude_code_env
  g5d_setup_codex_env
  g5d_write_default_provider_config
}
