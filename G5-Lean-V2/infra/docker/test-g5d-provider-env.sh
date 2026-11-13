#!/usr/bin/env sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
GENERATION_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

fail() {
  echo "test-g5d-provider-env: $*" >&2
  exit 1
}

assert_contains() {
  file="$1"
  needle="$2"
  grep -F "$needle" "$file" >/dev/null || fail "expected $file to contain $needle"
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT INT TERM

(
  unset G5D_PROVIDER_CONFIGS
  unset G5D_GENERATED_PROVIDER_CONFIGS
  unset G5_CLAUDE_PATH
  export G5D_GENERATED_PROVIDER_CONFIGS="$tmpdir/providers.toml"
  . "$SCRIPT_DIR/g5d-provider-env.sh"
  g5d_setup_provider_env

  [ "$G5D_PROVIDER_CONFIGS" = "$tmpdir/providers.toml" ] || fail "generated provider config path was not exported"
  [ -z "${G5_CLAUDE_CODE_SIDECAR:-}" ] || fail "the Node sidecar pin must not be exported any more"
  assert_contains "$G5D_PROVIDER_CONFIGS" "[claude_code]"
  assert_contains "$G5D_PROVIDER_CONFIGS" "[codex]"
  [ "$(grep -c '^\[' "$G5D_PROVIDER_CONFIGS")" = 2 ] || fail "expected exactly two provider sections"
)

(
  export G5D_PROVIDER_CONFIGS="$tmpdir/custom-providers.toml"
  export G5D_GENERATED_PROVIDER_CONFIGS="$tmpdir/should-not-exist.toml"
  . "$SCRIPT_DIR/g5d-provider-env.sh"
  g5d_setup_provider_env

  [ "$G5D_PROVIDER_CONFIGS" = "$tmpdir/custom-providers.toml" ] || fail "existing provider config path was overwritten"
  [ ! -e "$tmpdir/should-not-exist.toml" ] || fail "generated config was written despite existing G5D_PROVIDER_CONFIGS"
)

(
  unset G5D_PROVIDER_CONFIGS
  export OPENROUTER_API_KEY=openrouter-secret
  export ANTHROPIC_API_KEY=anthropic-secret
  unset ANTHROPIC_AUTH_TOKEN
  unset ANTHROPIC_BASE_URL
  unset OPENROUTER_BASE_URL
  unset G5D_LLM_PROXY
  unset G5D_LLM_PROVIDER
  unset G5D_LLM_MODEL
  unset ANTHROPIC_MODEL
  unset CLAUDE_CODE_SUBAGENT_MODEL
  . "$SCRIPT_DIR/g5d-provider-env.sh"
  g5d_setup_openrouter_claude_env

  [ "$ANTHROPIC_API_KEY" = "anthropic-secret" ] || fail "OpenRouter setup overrode direct ANTHROPIC_API_KEY without explicit provider"
  [ "${ANTHROPIC_AUTH_TOKEN+x}" != x ] || fail "OpenRouter setup derived ANTHROPIC_AUTH_TOKEN despite direct Anthropic credential"
  [ "${ANTHROPIC_BASE_URL+x}" != x ] || fail "OpenRouter setup derived ANTHROPIC_BASE_URL despite direct Anthropic credential"
  [ "${G5D_LLM_PROXY+x}" != x ] || fail "OpenRouter setup enabled proxy despite direct Anthropic credential"
)

(
  unset G5D_PROVIDER_CONFIGS
  export OPENROUTER_API_KEY=openrouter-secret
  export ANTHROPIC_API_KEY=anthropic-secret
  unset ANTHROPIC_AUTH_TOKEN
  unset ANTHROPIC_BASE_URL
  unset OPENROUTER_BASE_URL
  unset G5D_LLM_PROXY
  export G5D_LLM_PROVIDER=openrouter
  export G5D_LLM_MODEL=anthropic/claude-sonnet-4.6
  unset ANTHROPIC_MODEL
  unset CLAUDE_CODE_SUBAGENT_MODEL
  . "$SCRIPT_DIR/g5d-provider-env.sh"
  g5d_setup_openrouter_claude_env

  [ "${ANTHROPIC_API_KEY+x}" = x ] || fail "ANTHROPIC_API_KEY was not exported"
  [ "$ANTHROPIC_API_KEY" = "" ] || fail "OpenRouter setup did not blank ANTHROPIC_API_KEY"
  [ "$ANTHROPIC_AUTH_TOKEN" = "$OPENROUTER_API_KEY" ] || fail "OpenRouter setup did not derive ANTHROPIC_AUTH_TOKEN"
  [ "$ANTHROPIC_BASE_URL" = "https://openrouter.ai/api" ] || fail "OpenRouter setup did not default ANTHROPIC_BASE_URL"
  [ "${G5D_LLM_PROXY+x}" != x ] || fail "OpenRouter setup enabled the retired proxy"
  [ "$ANTHROPIC_MODEL" = "$G5D_LLM_MODEL" ] || fail "OpenRouter setup did not derive ANTHROPIC_MODEL"
  [ "$CLAUDE_CODE_SUBAGENT_MODEL" = "$G5D_LLM_MODEL" ] || fail "OpenRouter setup did not derive CLAUDE_CODE_SUBAGENT_MODEL"
)

(
  # A preset G5_CLAUDE_PATH is never downgraded, even when the pinned CLI exists.
  export G5_CLAUDE_PATH="$tmpdir/custom-claude"
  . "$SCRIPT_DIR/g5d-provider-env.sh"
  g5d_setup_claude_code_env
  [ "$G5_CLAUDE_PATH" = "$tmpdir/custom-claude" ] || fail "preset G5_CLAUDE_PATH was overwritten"
)

(
  # Without a preset, the pinned CLI at /usr/local/bin/claude wins when it
  # exists; on this host it only resolves when that path is executable, and
  # the SDK fallback only when its native binary exists, so assert the branch
  # that applies here rather than the image layout.
  unset G5_CLAUDE_PATH
  . "$SCRIPT_DIR/g5d-provider-env.sh"
  g5d_setup_claude_code_env
  if [ -x /usr/local/bin/claude ]; then
    [ "${G5_CLAUDE_PATH:-}" = "/usr/local/bin/claude" ] || fail "pinned /usr/local/bin/claude was not preferred (got '${G5_CLAUDE_PATH:-}')"
  elif [ -z "${G5_CLAUDE_PATH:-}" ] || [ -x "$G5_CLAUDE_PATH" ]; then
    :
  else
    fail "G5_CLAUDE_PATH resolved to a non-executable path '$G5_CLAUDE_PATH'"
  fi
)

for dockerfile in "$GENERATION_DIR/infra/docker/Dockerfile.g5d" "$GENERATION_DIR/infra/docker/Dockerfile.g5d-prebuilt"; do
  grep -q '^COPY scripts/claude-code-cli-version ' "$dockerfile" || fail "$dockerfile does not install the pinned Claude Code CLI from scripts/claude-code-cli-version"
  grep -q 'check-claude-cli-flags.sh /usr/local/bin/claude' "$dockerfile" || fail "$dockerfile does not gate the Claude Code CLI on scripts/check-claude-cli-flags.sh"
  grep -q '^COPY scripts/codex-cli-version ' "$dockerfile" || fail "$dockerfile does not install the pinned Codex CLI"
  grep -q 'check-codex-cli-flags.sh /usr/local/bin/codex' "$dockerfile" || fail "$dockerfile does not gate the Codex CLI"
done
