#!/usr/bin/env bash
# Final-candidate batch: rerun every explorer run and Verus control on the current inputs.
cd "$(dirname "$0")/.." || exit 1
C="final candidate rerun (orchestrator, after round 3)"
ex() { python3 -I scripts/receipt.py explore "$1" "$2" "$3" "$C" 2>&1 | tail -1; }
for r in asbuilt-invariants-default asbuilt-invariants-notx asbuilt-invariants-nojournal asbuilt-r2-with-foreign-edits asbuilt-l1-normal-conforming-domain asbuilt-l1-ns-all-conforming-domain; do ex $r as-built pass; done
for r in asbuilt-r3-dep asbuilt-l1-end asbuilt-commit-exact asbuilt-dep-clauses; do ex $r as-built violation; done
for r in control-01-no-setcommitted-c1 control-02-no-setcommitted-c3 control-03-no-setcommitted-notx control-04-no-rollback-loop control-04b-no-rollback-loop-l1 control-06-iristemp-rolled-back control-07-no-finalized-flag control-08-no-downgrade-guard control-09-no-update-guard control-10-no-trigger control-11-death-keeps-transaction control-12-dev-phases-in-transaction control-13-notransaction-ignored; do ex $r ${r%%-[a-z]*} violation; done
ex control-05-finalize-no-isok control-05 "survive (pass)"
ex control-05b-finalize-no-isok-l1 control-05 "survive (pass)"
ex control-05c-finalize-no-isok-all-callers control-05 pass
ex diagnostic-11b-death-keeps-transaction-r3top control-11 pass
ex repair-103-dep-clauses repair-103 pass
ex repair-103-invariants repair-103 pass
for n in 01 02 03 04 06 07 08 09 10 11 12 13; do
  python3 -I scripts/receipt.py verus final-r3-control-$n-verus control-$n "verification failure (detected)" "$C" --features control-$n 2>&1 | tail -1
done
python3 -I scripts/receipt.py verus final-r3-control-05-verus control-05 "verified, 0 errors (survives: equivalent on the load path)" "$C" --features control-05 2>&1 | tail -1
python3 -I scripts/receipt.py verus final-r3-sanity-false sanity "failure of sanity_false_claim" "$C" --features sanity-false 2>&1 | tail -1
echo BATCH-DONE
