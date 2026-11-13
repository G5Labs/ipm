#!/usr/bin/env bash
set -euo pipefail

generation_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
catalog="$generation_dir/tests/e2e/value_loop_matrix.tsv"
receipt_tool="$generation_dir/scripts/value_loop_receipt.py"
reuse_stack="${1:-auto}"

candidate_sha="${G5_VALUE_LOOP_CANDIDATE_SHA:-${COMMIT_SHA:-}}"
if [[ -z "$candidate_sha" ]]; then
  candidate_sha="$(git -C "$generation_dir" rev-parse HEAD)"
fi
if [[ ! "$candidate_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "value-loop candidate SHA must be one full lowercase 40-character Git SHA: $candidate_sha" >&2
  exit 1
fi

run_id="$(date -u +%Y%m%dT%H%M%SZ)"
artifact_root="${G5_VALUE_LOOP_ARTIFACT_DIR:-$generation_dir/.tmp/value-loop-matrix/$candidate_sha/$run_id}"
results="$artifact_root/selector-results.jsonl"
receipt="$artifact_root/value-loop-receipt.json"
logs_dir="$artifact_root/logs"
if [[ -e "$results" || -e "$receipt" ]]; then
  echo "value-loop artifact directory is not empty: $artifact_root" >&2
  exit 1
fi
mkdir -p "$logs_dir"

catalog_digest="$(python3 "$receipt_tool" digest --catalog "$catalog")"
execution_plan="$artifact_root/execution-plan.tsv"
python3 "$receipt_tool" plan --catalog "$catalog" >"$execution_plan"

slug_for() {
  printf '%s' "$1-$2" | tr -cs 'A-Za-z0-9._-' '-'
}

overall_status=0
selector_ordinal=0
while IFS=$'\t' read -r profile lane selector loop_ids source; do
  [[ -n "$profile" && -n "$lane" && -n "$selector" && -n "$loop_ids" && -n "$source" ]] || {
    echo "invalid value-loop execution-plan row" >&2
    exit 1
  }
  selector_ordinal=$((selector_ordinal + 1))
  slug="$(printf '%02d-%s' "$selector_ordinal" "$(slug_for "$profile" "$selector")")"
  evidence_ref="logs/$slug.log"
  evidence_path="$artifact_root/$evidence_ref"
  started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  command=()
  case "$profile" in
    api-integration)
      command=(just ci-docker-5 "test(${selector})" "$reuse_stack")
      ;;
    rust-e2e-broad)
      command=(just ci-docker-6 "$selector" false "$reuse_stack")
      ;;
    rust-e2e-live-microvm)
      command=(env G5_E2E_REQUIRE_LIVE_PROVIDED_AGENT=1 G5_FAKE_BUDGET_RESUME_E2E=1 just ci-docker-6 "$selector" false "$reuse_stack")
      ;;
    playwright-workspace-vm)
      command=(just ci-docker-7 "$selector" "$reuse_stack")
      ;;
    *)
      echo "unknown value-loop execution profile: $profile" >&2
      exit 1
      ;;
  esac

  {
    echo "candidate_sha=$candidate_sha"
    echo "catalog_digest=$catalog_digest"
    echo "profile=$profile"
    echo "lane=$lane"
    echo "selector=$selector"
    echo "loop_ids=$loop_ids"
    echo "source=$source"
    printf 'command='
    printf ' %q' "${command[@]}"
    printf '\n'
  } | tee "$evidence_path"

  set +e
  "${command[@]}" 2>&1 | tee -a "$evidence_path"
  command_status=${PIPESTATUS[0]}
  set -e
  ended_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  terminal_status="passed"
  if [[ "$command_status" -ne 0 ]]; then
    terminal_status="failed"
    overall_status=1
  else
    reuse_stack="true"
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
    --evidence-artifact "$evidence_ref"
done <"$execution_plan"

set +e
python3 "$receipt_tool" finalize \
  --catalog "$catalog" \
  --results "$results" \
  --candidate-sha "$candidate_sha" \
  --output "$receipt"
receipt_status=$?
set -e
if [[ "$receipt_status" -ne 0 ]]; then
  overall_status=1
fi

echo "[value-loop] receipt: $receipt"
if [[ "$overall_status" -ne 0 ]]; then
  echo "[value-loop] acceptance failed; inspect the machine-readable receipt and selector logs" >&2
  exit "$overall_status"
fi
echo "[value-loop] ${selector_ordinal}/${selector_ordinal} retained catalog selectors passed at $candidate_sha"
