#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <g5d-binary> [ldd-report-path]" >&2
  exit 2
fi

binary="$1"
report="${2:-}"

if [[ ! -x "${binary}" ]]; then
  echo "ERROR: ${binary} is not executable" >&2
  exit 2
fi

if ! command -v ldd >/dev/null 2>&1; then
  echo "Skipping g5d dynamic dependency check: ldd not available"
  exit 0
fi

if [[ -n "${report}" ]]; then
  ldd "${binary}" | tee "${report}"
  report_to_scan="${report}"
else
  report_to_scan="$(mktemp)"
  trap 'rm -f "${report_to_scan}"' EXIT
  ldd "${binary}" | tee "${report_to_scan}"
fi

if grep -E 'libkrunfw\.so|libkrun\.so|not found' "${report_to_scan}" >/dev/null; then
  echo "ERROR: ${binary} has a missing or forbidden legacy dynamic dependency" >&2
  exit 2
fi
