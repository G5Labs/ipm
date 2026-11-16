#!/bin/bash
# Full rerun from clean (playbook principle 10 and gate 3.7): every proof, every control, every model check,
# evidence collection and every gate. Uses the recorded IRIS logs (rerunning IRIS itself is per-lane, see README).
# Usage: scripts/full_rerun.sh   (writes validation/logs/49-full-rerun.log; exits non-zero on any failure)
set -u
cd "$(dirname "$0")/.."
LOG=validation/logs/49-full-rerun.log
fail=0
step() { echo "== $*" | tee -a "$LOG"; "$@" >> "$LOG" 2>&1; rc=$?; echo "   exit $rc" | tee -a "$LOG"; [ $rc -eq 0 ] || fail=1; }
: > "$LOG"
echo "started $(date -u +%FT%TZ)" >> "$LOG"
find models -name __pycache__ -type d -prune -exec rm -rf {} +
rm -f evidence/proofs-*.json models/verus/results/*.json
step python3 -I scripts/collect_evidence.py
step python3 -I models/verus/controls.py
step python3 -I scripts/record_proofs.py
step python3 -I models/explorer/explore.py
step python3 -I models/explorer/inst/check.py
step python3 -I models/explorer/repo/check.py
step python3 -I models/explorer/nsp/check.py
step python3 -I scripts/ontology_index.py
step python3 -I scripts/ontology_index.py --check
step python3 -I scripts/ontology_index.py --self-test
step python3 -I scripts/semantics_gate.py
step python3 -I scripts/semantics_gate.py --check
echo "finished $(date -u +%FT%TZ) fail=$fail" | tee -a "$LOG"
exit $fail
