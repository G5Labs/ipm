#!/usr/bin/env bash
set -euo pipefail

artifact_root="${G5_CI_ARTIFACT_ROOT:-/workspace}"
validation_json="${artifact_root}/ontology-v4-validation.json"
lint_json="${artifact_root}/ontology-v4-lint-advisory.json"

# G5 Lean intentionally omits the original repository's ontology sources.
# Keep the parser/resolver/CLI unit tests in the calling Cloud Build step, and
# skip only the corpus boundary when there is no corpus to validate. An
# existing (including empty) directory still reaches the fail-closed g5d CLI.
if [[ ! -d .semlift/specs ]]; then
  echo "SKIP: ontology corpus validation; .semlift/specs does not exist"
  exit 0
fi

mkdir -p "${artifact_root}"

# stdout goes to JSON artifacts, so echo a failed report to the build log or
# the red build is undiagnosable.
set +e
cargo run -p g5d -- --workspace . --json spec validate --all \
  > "${validation_json}"
validate_rc=$?
set -e
if [[ "${validate_rc}" -ne 0 ]]; then
  echo "ontology-v4 corpus validation failed (exit ${validate_rc}); JSON report:"
  cat "${validation_json}"
  exit "${validate_rc}"
fi

set +e
cargo run -p g5d -- --workspace . --json spec validate --all --ontology-lint advisory \
  > "${lint_json}"
lint_rc=$?
set -e
if [[ "${lint_rc}" -ne 0 ]]; then
  echo "ontology-v4 advisory lint run failed (exit ${lint_rc}); JSON report:"
  cat "${lint_json}"
  exit "${lint_rc}"
fi

python3 - "${validation_json}" "${lint_json}" <<'PY'
import json
from pathlib import Path
import sys

report = json.loads(Path(sys.argv[1]).read_text())
lint_report = json.loads(Path(sys.argv[2]).read_text())
ontology_lint = lint_report.get("ontologyLint", {})
print(
    "ontology-v4 corpus validation: "
    f"valid={report['valid']} "
    f"checked={report['checked']} "
    f"issues={len(report['issues'])}"
)
print(
    "ontology-v4 advisory lint: "
    f"valid={ontology_lint.get('valid')} "
    f"diagnostics={len(ontology_lint.get('diagnostics', []))}"
)
PY
