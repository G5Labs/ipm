#!/usr/bin/env sh
# =============================================================================
# check-claude-cli-flags.sh — prove an installed Claude Code CLI carries the
# flag surface the G5 provider runner passes (docs/lean-provider-cli-contract.md).
#
#   check-claude-cli-flags.sh <claude-binary> [expected-version]
#
# Every place that installs the pinned CLI runs this after its `--version`
# smoke: infra/docker/Dockerfile.g5d{,-prebuilt}, the Packer worker image
# (infra/packer/g5-worker/scripts/40-install-claude.sh) and the local guest
# runtime staging (scripts/stage-local-worker-runtime.sh). It fails closed so a
# pin bump to a release that dropped or renamed a runner flag breaks the image
# build instead of the first chat turn.
#
# POSIX sh only: it runs inside Dockerfile RUN steps and minimal containers.
# =============================================================================
set -eu

CLAUDE_BIN="${1:?usage: $0 <claude-binary> [expected-version]}"
EXPECTED_VERSION="${2:-}"

fail() {
    echo "check-claude-cli-flags: FAIL: $*" >&2
    exit 1
}

[ -x "$CLAUDE_BIN" ] || fail "$CLAUDE_BIN is not executable"

# 1. `--version` prints "<semver> (Claude Code)"; compare the semver with the
#    pin when one is given.
version_out="$("$CLAUDE_BIN" --version 2>&1)" || fail "'$CLAUDE_BIN --version' exited non-zero: $version_out"
installed_version="${version_out%% *}"
case "$installed_version" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *) fail "'$CLAUDE_BIN --version' printed '$version_out', not '<semver> (Claude Code)'" ;;
esac
if [ -n "$EXPECTED_VERSION" ] && [ "$installed_version" != "$EXPECTED_VERSION" ]; then
    fail "installed claude $installed_version does not match the pin $EXPECTED_VERSION"
fi

# 2. Every flag the runner passes must be advertised by `--help`. The list is
#    the verified flag table in docs/lean-provider-cli-contract.md; keep both in
#    step when the runner's argv (design v6 §8.3) changes.
help_out="$("$CLAUDE_BIN" --help 2>&1)" || fail "'$CLAUDE_BIN --help' exited non-zero"
missing=""
for flag in \
    --print \
    --output-format \
    --input-format \
    --verbose \
    --include-partial-messages \
    --resume \
    --session-id \
    --permission-mode \
    --permission-prompts \
    --permission-prompt-tool \
    --mcp-config \
    --strict-mcp-config \
    --settings \
    --add-dir \
    --setting-sources \
    --disallowedTools \
    --append-system-prompt \
    --system-prompt \
    --tools \
    --allowedTools \
    --effort \
    --model \
    ; do
    printf '%s\n' "$help_out" | grep -q -- "$flag" || missing="$missing $flag"
done
# The choices the runner relies on: stream-json for both formats, host for
# permission prompts, bypassPermissions as a permission mode.
printf '%s\n' "$help_out" | grep -q -- '"stream-json"' || missing="$missing stream-json"
printf '%s\n' "$help_out" | grep -q -- '"host"' || missing="$missing permission-prompts=host"
printf '%s\n' "$help_out" | grep -q -- '"bypassPermissions"' || missing="$missing permission-mode=bypassPermissions"
printf '%s\n' "$help_out" | grep -q -- '"dontAsk"' || missing="$missing permission-mode=dontAsk"
# Help wraps this option across lines. Only its choices establish that
# `none` is accepted; an unrelated option's `none` must not satisfy the gate.
# The real CLI indents headings two spaces; the fake's help has no indentation.
permission_prompts_choices="$(printf '%s\n' "$help_out" | awk '
    /^(  )?--permission-prompts[[:space:]]/ { in_option = 1 }
    in_option && /^(  )?--/ && $1 != "--permission-prompts" { exit }
    in_option { printf "%s ", $0 }
')"
printf '%s\n' "$permission_prompts_choices" | grep -Eq -- '\(choices:[^)]*"none"' \
    || missing="$missing permission-prompts=none"
[ -z "$missing" ] || fail "claude $installed_version --help does not advertise:$missing"

# 3. `--max-turns <turns>` is parsed but hidden from `--help` on 2.1.270. The
#    option parser validates its argument before `--version` short-circuits, so
#    a non-numeric value proves the option is defined (an unknown option would
#    be ignored and the version printed instead).
probe_out="$("$CLAUDE_BIN" --max-turns not-a-number --version 2>&1 || true)"
printf '%s\n' "$probe_out" | grep -q -- '--max-turns' \
    || fail "claude $installed_version does not recognise --max-turns (probe output: $probe_out)"

echo "check-claude-cli-flags: OK: claude $installed_version at $CLAUDE_BIN carries the runner flag surface"
