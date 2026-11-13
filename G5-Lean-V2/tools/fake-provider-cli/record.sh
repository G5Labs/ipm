#!/usr/bin/env bash
# =============================================================================
# record.sh — run the real pinned provider CLIs once per scripted scenario and
# save their frames as fixtures under tests/fixtures/provider-frames/.
#
#   tools/fake-provider-cli/record.sh [claude|codex|all]      (default: all)
#
# The fixtures are what fake-claude / fake-codex replay (G5_FAKE_SCENARIO), so
# they are recorded with the exact argv the provider runner uses (design v6
# §8.3, docs/lean-provider-cli-contract.md) against the `fake-g5-tools` MCP
# stub, which exposes an `echo` tool for ordinary MCP transport capture.
# Native questions and permissions are covered by the agent-turn stdio fixtures, not this MCP recorder.
#
# Requirements:
#   * network access to npm when the CLI on PATH is not the pinned release
#     (the script installs @anthropic-ai/claude-code@<pin> / @openai/codex@<pin>
#     into a temporary prefix); G5_RECORD_CLAUDE_PATH / G5_RECORD_CODEX_PATH
#     override the binaries;
#   * Claude auth: a logged-in Claude Code (the default CLAUDE_CONFIG_DIR is
#     used unless G5_RECORD_CLAUDE_CONFIG_DIR is set) or ANTHROPIC_API_KEY /
#     CLAUDE_CODE_OAUTH_TOKEN in the environment;
#   * Codex auth: ${CODEX_HOME:-$HOME/.codex}/auth.json (copied into a
#     throwaway CODEX_HOME) or OPENAI_API_KEY / CODEX_API_KEY;
#   * cargo (builds fake-g5-tools), node/npm, python3, jq.
#
# Every scenario runs in a fresh temporary working directory. Before a fixture
# is written the session id becomes {{session_id}}, the working directory
# {{cwd}}, and machine-specific init fields are emptied (see sanitize()).
# Model names, usage, costs and timings are kept as recorded.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_ROOT="${G5_RECORD_OUT_DIR:-$REPO_ROOT/tests/fixtures/provider-frames}"
WHICH="${1:-all}"
CLAUDE_MODEL="${G5_RECORD_CLAUDE_MODEL:-haiku}"
CODEX_MODEL="${G5_RECORD_CODEX_MODEL:-}"

CLAUDE_PIN="$(tr -d '[:space:]' < "$REPO_ROOT/scripts/claude-code-cli-version")"
CODEX_PIN="$(tr -d '[:space:]' < "$REPO_ROOT/scripts/codex-cli-version")"
export G5_RECORD_CLAUDE_PIN="$CLAUDE_PIN" G5_RECORD_CODEX_PIN="$CODEX_PIN"

# Native stdin/replay probes use the actual pinned CLI and a controlled local
# model endpoint; unlike the ordinary recordings they need no provider auth.
if [ "$WHICH" = "claude-steering" ]; then
    shift
    exec python3 "$SCRIPT_DIR/record-steering.py" \
        --claude "${G5_RECORD_CLAUDE_PATH:-$(command -v claude)}" \
        --pin "$CLAUDE_PIN" --out "$OUT_ROOT/claude" "$@"
fi

info() { printf '[record] %s\n' "$*" >&2; }
fail() { printf '[record] FATAL: %s\n' "$*" >&2; exit 1; }

for tool in cargo npm python3 jq; do
    command -v "$tool" >/dev/null || fail "$tool is required"
done

WORK="$(mktemp -d -t g5-record-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT_ROOT/claude" "$OUT_ROOT/codex"

# ---------------------------------------------------------------------------
# Build the MCP stub the CLIs will talk to.
# ---------------------------------------------------------------------------
info "building fake-g5-tools"
(cd "$REPO_ROOT" && SQLX_OFFLINE=true cargo build -p fake-provider-cli --bin fake-g5-tools --offline >/dev/null)
STUB="$REPO_ROOT/target/debug/fake-g5-tools"
[ -x "$STUB" ] || fail "fake-g5-tools was not built at $STUB"

# ---------------------------------------------------------------------------
# Resolve the pinned CLIs.
# ---------------------------------------------------------------------------
resolve_cli() {
    # resolve_cli <name> <pin> <npm package> <override env value> <version-prefix-regex>
    local name="$1" pin="$2" package="$3" override="$4" regex="$5" candidate version
    if [ -n "$override" ]; then
        [ -x "$override" ] || fail "$name override $override is not executable"
        echo "$override"
        return
    fi
    candidate="$(command -v "$name" || true)"
    if [ -n "$candidate" ]; then
        version="$("$candidate" --version 2>/dev/null | sed -nE "s/$regex/\1/p" | head -1)"
        if [ "$version" = "$pin" ]; then
            echo "$candidate"
            return
        fi
        info "$name on PATH is ${version:-unknown}, pin is $pin; installing the pinned release"
    fi
    local prefix="$WORK/npm-$name"
    mkdir -p "$prefix"
    npm install --global --prefix "$prefix" --no-audit --no-fund "$package@$pin" >/dev/null 2>"$WORK/npm-$name.err" \
        || { cat "$WORK/npm-$name.err" >&2; fail "npm install $package@$pin failed"; }
    candidate="$prefix/bin/$name"
    [ -x "$candidate" ] || fail "$candidate missing after npm install"
    version="$("$candidate" --version 2>/dev/null | sed -nE "s/$regex/\1/p" | head -1)"
    [ "$version" = "$pin" ] || fail "installed $name reports ${version:-unknown}, expected $pin"
    echo "$candidate"
}

# ---------------------------------------------------------------------------
# Sanitisation: <raw frames> <session id> <cwd> -> stdout
# ---------------------------------------------------------------------------
SANITIZE_PY="$WORK/sanitize.py"
cat > "$SANITIZE_PY" <<'PY'
import json, sys
session_id, cwd, real_cwd = sys.argv[1], sys.argv[2], sys.argv[3]

def scrub(value):
    if isinstance(value, str):
        for needle in sorted({cwd, real_cwd}, key=len, reverse=True):
            value = value.replace(needle, "{{cwd}}")
        if session_id:
            value = value.replace(session_id, "{{session_id}}")
        return value
    if isinstance(value, list):
        return [scrub(v) for v in value]
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items()}
    return value

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        frame = json.loads(line)
    except json.JSONDecodeError:
        # Non-JSON stdout lines are not frames; keep them out of the fixture.
        sys.stderr.write(f"[record] dropping non-JSON stdout line: {line[:120]}\n")
        continue
    frame = scrub(frame)
    if isinstance(frame, dict) and frame.get("type") == "system" and frame.get("subtype") == "init":
        # Machine-specific: memory dir under CLAUDE_CONFIG_DIR, the desktop
        # messaging socket, and whatever user-level skills/agents/commands/
        # plugins the recording account has installed.
        for key in ("memory_paths", "messaging_socket_path"):
            frame.pop(key, None)
        for key in ("skills", "agents", "slash_commands", "plugins"):
            if key in frame:
                frame[key] = []
    print(json.dumps(frame, separators=(",", ":")))
PY

sanitize() {
    local session_id="$1" cwd="$2" real_cwd
    real_cwd="$(cd "$cwd" && pwd -P)"
    python3 "$SANITIZE_PY" "$session_id" "$cwd" "$real_cwd"
}

RECORDING_JSON="$WORK/recording.json"
echo '{"recorded_at": "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'", "scenarios": []}' > "$RECORDING_JSON"

note_scenario() {
    # note_scenario <provider> <name> <cli version> <prompt> <exit code> <stderr file> <argv...>
    local provider="$1" name="$2" version="$3" prompt="$4" code="$5" stderr_file="$6"
    shift 6
    local argv_json
    argv_json="$(printf '%s\n' "$@" | jq -R . | jq -s .)"
    jq --arg provider "$provider" --arg name "$name" --arg version "$version" --arg prompt "$prompt" \
       --argjson code "$code" --rawfile stderr "$stderr_file" --argjson argv "$argv_json" \
       '.scenarios += [{provider: $provider, name: $name, cli_version: $version, prompt: $prompt, exit_code: $code, stderr: ($stderr | .[0:2000]), argv: $argv}]' \
       "$RECORDING_JSON" > "$RECORDING_JSON.tmp" && mv "$RECORDING_JSON.tmp" "$RECORDING_JSON"
}

# ---------------------------------------------------------------------------
# Claude
# ---------------------------------------------------------------------------
record_claude() {
    local claude
    claude="$(resolve_cli claude "$CLAUDE_PIN" @anthropic-ai/claude-code "${G5_RECORD_CLAUDE_PATH:-}" '^([0-9]+\.[0-9]+\.[0-9]+).*$')"
    info "claude: $claude ($("$claude" --version))"
    sh "$REPO_ROOT/scripts/check-claude-cli-flags.sh" "$claude" "$CLAUDE_PIN" >&2

    local config_dir="${G5_RECORD_CLAUDE_CONFIG_DIR:-${CLAUDE_CONFIG_DIR:-}}"
    local base_env=(env -u CLAUDECODE -u CLAUDE_CODE_ENTRYPOINT -u CLAUDE_CODE_SESSION_ID -u CLAUDE_CODE_HOST_SESSION_ID
                    MCP_TOOL_TIMEOUT=120000)
    if [ -n "$config_dir" ]; then
        base_env+=("CLAUDE_CONFIG_DIR=$config_dir")
    fi

    # run_claude <name> <cwd> <session id> <resume id or ''> <permission mode> <prompt>
    run_claude() {
        local name="$1" cwd="$2" session_id="$3" resume="$4" mode="$5" prompt="$6"
        local mcp_json="$cwd/.mcp.json" log="$WORK/$name.mcp-requests.jsonl" stderr_file="$WORK/$name.stderr"
        jq -n --arg cmd "$STUB" '{mcpServers: {"g5-tools": {command: $cmd, args: [], cwd: "."}}}' > "$mcp_json"
        rm -f "$log"
        local argv=(-p --output-format stream-json --input-format stream-json --verbose --include-partial-messages)
        if [ -n "$resume" ]; then
            argv+=(--resume "$resume")
        else
            argv+=(--session-id "$session_id")
        fi
        argv+=(--permission-mode "$mode" --permission-prompts host)
        argv+=(--mcp-config "$mcp_json" --strict-mcp-config --setting-sources project
               --model "$CLAUDE_MODEL" --max-turns 8
               --append-system-prompt "You are being recorded for a test fixture. Follow the instruction literally and briefly.")
        info "claude/$name: $prompt"
        local code=0
        printf '{"type":"user","message":{"role":"user","content":[{"type":"text","text":%s}]}}\n' "$(jq -Rn --arg p "$prompt" '$p')" \
            | (cd "$cwd" && "${base_env[@]}" G5_FAKE_MCP_LOG="$log" "$claude" "${argv[@]}") \
            > "$WORK/$name.raw" 2>"$stderr_file" || code=$?
        [ -s "$WORK/$name.raw" ] || { cat "$stderr_file" >&2; fail "claude/$name produced no frames (exit $code)"; }
        sanitize "$session_id" "$cwd" < "$WORK/$name.raw" > "$OUT_ROOT/claude/$name.jsonl"
        if [ -s "$log" ]; then
            sanitize "$session_id" "$cwd" < "$log" > "$OUT_ROOT/claude/$name.mcp-requests.jsonl"
        fi
        note_scenario claude "$name" "$CLAUDE_PIN" "$prompt" "$code" "$stderr_file" "${argv[@]}"
        info "claude/$name: exit $code, $(wc -l < "$OUT_ROOT/claude/$name.jsonl" | tr -d ' ') frames"
    }

    local text_cwd="$WORK/claude-text" text_session
    mkdir -p "$text_cwd"
    text_session="$(python3 -c 'import uuid; print(uuid.uuid4())')"
    run_claude text-only "$text_cwd" "$text_session" "" bypassPermissions \
        "Reply with exactly the word OK and nothing else."
    # Same cwd and config dir: the transcript from text-only is what --resume loads.
    run_claude resume "$text_cwd" "$text_session" "$text_session" bypassPermissions \
        "Reply with exactly the word AGAIN and nothing else."

    local write_cwd="$WORK/claude-write"
    mkdir -p "$write_cwd"
    run_claude write-file "$write_cwd" "$(python3 -c 'import uuid; print(uuid.uuid4())')" "" bypassPermissions \
        "Use the Write tool to create a file named hello.txt in the current directory whose entire content is the single line hello. Then reply with exactly DONE."

    local ask_cwd="$WORK/claude-echo"
    mkdir -p "$ask_cwd"
    run_claude mcp-echo "$ask_cwd" "$(python3 -c 'import uuid; print(uuid.uuid4())')" "" bypassPermissions \
        "Call the echo tool of the g5-tools MCP server once with text \"yes\". Then reply with exactly the text it returned and nothing else."


}

# ---------------------------------------------------------------------------
# Codex
# ---------------------------------------------------------------------------
record_codex() {
    local codex
    codex="$(resolve_cli codex "$CODEX_PIN" @openai/codex "${G5_RECORD_CODEX_PATH:-}" '^codex-cli ([0-9]+\.[0-9]+\.[0-9]+).*$')"
    info "codex: $codex ($("$codex" --version))"

    local home="$WORK/codex-home" auth_src="${CODEX_HOME:-$HOME/.codex}/auth.json"
    mkdir -p "$home"
    if [ -f "$auth_src" ]; then
        cp "$auth_src" "$home/auth.json"
    elif [ -z "${OPENAI_API_KEY:-}${CODEX_API_KEY:-}" ]; then
        fail "no Codex credentials: $auth_src is missing and OPENAI_API_KEY/CODEX_API_KEY are unset"
    fi

    # run_codex <name> <cwd> <resume id or ''> <prompt> [allow-empty 0|1] [approve-mcp 0|1]
    run_codex() {
        local name="$1" cwd="$2" resume="$3" prompt="$4" allow_empty="${5:-0}" approve_mcp="${6:-1}"
        local log="$WORK/$name.mcp-requests.jsonl" stderr_file="$WORK/$name.stderr" last="$WORK/$name.last"
        # Codex passes only its own env to MCP servers, so the stub's knobs go
        # through [mcp_servers.<name>.env]. Under approval_policy = "never" an
        # MCP tool call fails ("requires approval") unless the tool carries
        # approval_mode = "approve", which is what the worker renders for every
        # required g5-tools tool (backend/g5d-agent/src/codex.rs).
        cat > "$home/config.toml" <<TOML
approval_policy = "never"
sandbox_mode = "workspace-write"

[mcp_servers.g5-tools]
command = "$STUB"
args = []
cwd = "$cwd"
startup_timeout_sec = 60
tool_timeout_sec = 120

[mcp_servers.g5-tools.env]
G5_FAKE_MCP_LOG = "$log"
TOML
        if [ "$approve_mcp" = "1" ]; then
            cat >> "$home/config.toml" <<TOML

[mcp_servers.g5-tools.tools.echo]
approval_mode = "approve"
TOML
        fi
        rm -f "$log" "$last"
        local argv=(exec)
        if [ -n "$resume" ]; then
            argv+=(resume "$resume")
        fi
        argv+=(--json --skip-git-repo-check --output-last-message "$last")
        if [ -n "$CODEX_MODEL" ]; then
            argv+=(--model "$CODEX_MODEL")
        fi
        argv+=(-c 'approval_policy="never"' -c 'sandbox_mode="workspace-write"')
        info "codex/$name: $prompt"
        local code=0
        printf '%s\n' "$prompt" \
            | (cd "$cwd" && env CODEX_HOME="$home" G5_FAKE_MCP_LOG="$log" "$codex" "${argv[@]}") \
            > "$WORK/$name.raw" 2>"$stderr_file" || code=$?
        if [ ! -s "$WORK/$name.raw" ] && [ "$allow_empty" != "1" ]; then
            cat "$stderr_file" >&2
            fail "codex/$name produced no frames (exit $code)"
        fi
        local thread_id
        thread_id="$(jq -r 'select(.type == "thread.started") | .thread_id' "$WORK/$name.raw" | head -1)"
        [ -n "$thread_id" ] || thread_id="$resume"
        if [ -s "$WORK/$name.raw" ]; then
            sanitize "$thread_id" "$cwd" < "$WORK/$name.raw" > "$OUT_ROOT/codex/$name.jsonl"
        else
            # No frames: the manifest entry (exit code, stderr) is the fixture.
            rm -f "$OUT_ROOT/codex/$name.jsonl"
        fi
        if [ -s "$log" ]; then
            sanitize "$thread_id" "$cwd" < "$log" > "$OUT_ROOT/codex/$name.mcp-requests.jsonl"
        fi
        note_scenario codex "$name" "$CODEX_PIN" "$prompt" "$code" "$stderr_file" "${argv[@]}"
        local frame_count=0
        [ -f "$OUT_ROOT/codex/$name.jsonl" ] && frame_count="$(wc -l < "$OUT_ROOT/codex/$name.jsonl" | tr -d ' ')"
        info "codex/$name: exit $code, $frame_count frames, thread $thread_id"
        printf '%s' "$thread_id"
    }

    local text_cwd="$WORK/codex-text" text_thread
    mkdir -p "$text_cwd"
    text_thread="$(run_codex text-only "$text_cwd" "" "Reply with exactly the word OK and nothing else.")"
    run_codex resume "$text_cwd" "$text_thread" "Reply with exactly the word AGAIN and nothing else." >/dev/null
    run_codex resume-missing "$text_cwd" "00000000-0000-7000-8000-000000000000" "Reply with exactly the word OK." 1 >/dev/null

    local write_cwd="$WORK/codex-write"
    mkdir -p "$write_cwd"
    run_codex write-file "$write_cwd" "" \
        "Create a file named hello.txt in the current directory whose entire content is the single line hello. Then reply with exactly DONE." >/dev/null

    local ask_cwd="$WORK/codex-echo"
    mkdir -p "$ask_cwd"
    run_codex mcp-echo "$ask_cwd" "" \
        "Call the echo tool of the g5-tools MCP server once with text \"yes\". Then reply with exactly the text it returned and nothing else." >/dev/null
    # Without approval_mode = "approve" the same call fails under
    # approval_policy = "never": the negative shape the runner must map.
    local unapproved_cwd="$WORK/codex-unapproved"
    mkdir -p "$unapproved_cwd"
    run_codex mcp-echo-unapproved "$unapproved_cwd" "" \
        "Call the echo tool of the g5-tools MCP server once with text \"yes\". Then reply with exactly the text it returned and nothing else." 0 0 >/dev/null
}

case "$WHICH" in
    claude) record_claude ;;
    codex) record_codex ;;
    all) record_claude; record_codex ;;
    *) fail "usage: $0 [claude|codex|all]" ;;
esac

# Merge into the committed recording manifest (one entry per provider/name;
# entries for providers that were not re-recorded are kept).
MANIFEST="$OUT_ROOT/recording.json"
python3 - "$MANIFEST" "$RECORDING_JSON" <<'PY'
import json, os, sys
manifest_path, new_path = sys.argv[1], sys.argv[2]
new = json.load(open(new_path))
old = json.load(open(manifest_path)) if os.path.exists(manifest_path) else {"scenarios": []}
fresh = {(s["provider"], s["name"]) for s in new["scenarios"]}
kept = [s for s in old.get("scenarios", []) if (s["provider"], s["name"]) not in fresh]
merged = {
    "recorded_at": new["recorded_at"],
    "claude_code_version": os.environ.get("G5_RECORD_CLAUDE_PIN", ""),
    "codex_version": os.environ.get("G5_RECORD_CODEX_PIN", ""),
    "scenarios": sorted(kept + new["scenarios"], key=lambda s: (s["provider"], s["name"])),
}
if not merged["claude_code_version"]:
    merged["claude_code_version"] = old.get("claude_code_version", "")
if not merged["codex_version"]:
    merged["codex_version"] = old.get("codex_version", "")
with open(manifest_path, "w") as handle:
    json.dump(merged, handle, indent=2)
    handle.write("\n")
PY
info "fixtures written under $OUT_ROOT; manifest at $MANIFEST"
