#!/usr/bin/env sh
# Offline, fail-closed gate for the ordinary and formal native Codex App Server launch.
set -eu
CODEX_BIN="${1:?usage: check-codex-cli-flags.sh <codex> [expected-version]}"
EXPECTED="${2:-}"
version="$("$CODEX_BIN" --version)"
[ -z "$EXPECTED" ] || [ "$version" = "codex-cli $EXPECTED" ] || { echo "Codex pin mismatch: $version" >&2; exit 1; }
app_help="$("$CODEX_BIN" app-server --help)"
# G5 launches only App Server; these are the flags its ordinary and formal
# launches pass. Manual `codex exec` use is outside this contract.
for flag in --listen --strict-config --config --disable --print-startup-config; do
    printf '%s\n' "$app_help" | grep -q -- "$flag" || { echo "Codex App Server lacks $flag" >&2; exit 1; }
done
# Global `features list` validates each real feature name without a model call.
"$CODEX_BIN" \
    --disable shell_tool --disable shell_snapshot --disable shell_snapshot_v2 --disable undo --disable unified_exec --disable multi_agent \
    --disable multi_agent_v2 --disable multi_agent_mode --disable agent_message_board \
    --disable apps --disable plugins --disable remote_plugin --disable plugin_hooks \
    --disable hooks --disable browser_use --disable browser_use_external \
    --disable computer_use --disable view_image --disable image_generation \
    --disable imagegenext --disable in_app_browser --disable skill_search --disable tool_search \
    --disable skill_mcp_dependency_install --disable memories --disable code_mode \
    --disable code_mode_host --disable goals features list >/dev/null
# Help short-circuits config loading, and even --strict-config permits unknown
# nested permission keys. --print-startup-config loads and validates the formal
# launch configuration, then exits without serving or making a model request.
# Invalid recognized values must fail that load; an ignored key would instead
# print the startup configuration and fail this gate.
probe_home="$(mktemp -d /tmp/g5-codex-config.XXXXXX)"
trap 'rm -rf "$probe_home"' EXIT
printf 'Configuration parsing probe.\n' > "$probe_home/instructions.txt"
config_probe() {
    env -i PATH="$PATH" HOME="$probe_home" CODEX_HOME="$probe_home" \
        "$CODEX_BIN" app-server --strict-config --print-startup-config \
        -c 'default_permissions="g5_probe"' \
        -c 'permissions.g5_probe.extends=":read-only"' \
        -c 'permissions.g5_probe.filesystem={":root"="deny"}' \
        -c "model_instructions_file=\"$probe_home/instructions.txt\"" \
        -c 'approval_policy="never"' \
        -c 'project_doc_max_bytes=0' \
        -c 'developer_instructions=""' \
        -c 'include_apps_instructions=false' \
        -c 'include_collaboration_mode_instructions=false' \
        -c 'skills.bundled.enabled=false' \
        -c 'skills.include_instructions=false' \
        -c 'tools.update_plan.enabled=false' \
        -c 'tools.experimental_request_user_input.enabled=false' \
        -c 'web_search="disabled"' \
        -c 'mcp_servers.formal_review.command="/bin/false"' \
        -c 'mcp_servers.formal_review.required=true' \
        -c 'mcp_servers.formal_review.omit_tools_from=["deferred"]' \
        -c 'mcp_servers.formal_review.tools.read_review.approval_mode="approve"' \
        -c 'mcp_servers.formal_review.tools.write_report.approval_mode="approve"' \
        --disable shell_tool --disable multi_agent --disable hooks --disable memories \
        "$@" </dev/null >"$probe_home/result" 2>&1
}
config_probe || { cat "$probe_home/result" >&2; echo 'Codex rejected the formal launch configuration' >&2; exit 1; }
grep -q "\"appServerVersion\":\"${version#codex-cli }\"" "$probe_home/result" || {
    cat "$probe_home/result" >&2; echo 'Codex startup configuration probe did not report the pinned App Server' >&2; exit 1;
}
for invalid in \
    'g5_nonexistent.key=1' \
    'approval_policy="g5_invalid"' \
    'default_permissions="g5_missing"' \
    'permissions.g5_probe.extends="g5_missing"' \
    'permissions.g5_probe.filesystem={":root"="g5_invalid"}' \
    'model_instructions_file=7' \
    'project_doc_max_bytes="g5_invalid"' \
    'developer_instructions=7' \
    'include_apps_instructions=7' \
    'include_collaboration_mode_instructions=7' \
    'skills.bundled.enabled=7' \
    'skills.include_instructions=7' \
    'tools.update_plan.enabled=7' \
    'tools.experimental_request_user_input.enabled=7' \
    'web_search="g5_invalid"' \
    'mcp_servers.formal_review.required="g5_invalid"' \
    'mcp_servers.formal_review.omit_tools_from=["g5_invalid"]' \
    'mcp_servers.formal_review.tools.read_review.approval_mode="g5_invalid"' \
    'mcp_servers.formal_review.tools.write_report.approval_mode="g5_invalid"'; do
    config_probe -c "$invalid" && { echo "Codex accepted invalid config: $invalid" >&2; exit 1; }
    grep -q '"reason":"configLoad"' "$probe_home/result" || {
        echo "Codex did not reject confinement config: $invalid" >&2; cat "$probe_home/result" >&2; exit 1
    }
done
printf 'Codex CLI launch surface verified: %s\n' "$version"
