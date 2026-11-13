#!/usr/bin/env bash
# Execute one value-loop selector without allowing its expected non-zero status
# to abort the enclosing receipt loop. Infrastructure/receipt failures still
# propagate as non-zero exits.
set -uo pipefail

if [[ "$#" -lt 10 ]]; then
  echo "usage: $0 <results> <receipt-tool> <candidate-sha> <catalog-digest> <lane> <profile> <selector> <evidence-path> <evidence-ref> -- <command...>" >&2
  exit 2
fi

results="$1"
receipt_tool="$2"
candidate_sha="$3"
catalog_digest="$4"
lane="$5"
profile="$6"
selector="$7"
evidence_path="$8"
evidence_ref="$9"
shift 9
if [[ "${1:-}" != "--" ]]; then
  echo "value-loop selector command must follow --" >&2
  exit 2
fi
shift
if [[ "$#" -eq 0 ]]; then
  echo "value-loop selector command is empty" >&2
  exit 2
fi

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf 'command=' | tee -a "$evidence_path"
printf ' %q' "$@" | tee -a "$evidence_path"
printf '\n' | tee -a "$evidence_path"

if "$@" 2>&1 | tee -a "$evidence_path"; then
  command_status=0
else
  command_status=${PIPESTATUS[0]}
fi
ended_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
terminal_status=passed
if [[ "$command_status" -ne 0 ]]; then
  terminal_status=failed
fi

python3 "$receipt_tool" record \
  --results "$results" \
  --candidate-sha "$candidate_sha" \
  --catalog-digest "$catalog_digest" \
  --lane "$lane" \
  --profile "$profile" \
  --selector "$selector" \
  --started-at "$started_at" \
  --ended-at "$ended_at" \
  --status "$terminal_status" \
  --evidence-artifact "$evidence_ref" || exit $?

exit "$command_status"
