#!/usr/bin/env bash
# R12a: rerun the round-three repairs' checks on the final inputs (original commands, follow-up receipts).
set -u
cd "$(dirname "$0")/../.." || exit 1
v() { python3 -I -B followup/bin/verus_check.py verus "$@" 2>&1 | tail -1; }
x() { python3 -I -B followup/bin/verus_check.py explore "$@" 2>&1 | tail -1; }
v r12-asbuilt-proofs "verified 263, errors 0"
v r12-control-13-verus "verification failure (detected)" --features control-13
v r12-control-12-verus "verification failure (detected)" --features control-12
v r12-control-01-verus "failure of lemma_lnm0_b" --features control-01
v r12-sanity-false "failure of sanity_false_claim" --features sanity-false
for r in asbuilt-invariants-default asbuilt-invariants-notx asbuilt-invariants-nojournal asbuilt-l1-normal-conforming-domain asbuilt-l1-ns-all-conforming-domain repair-103-dep-clauses repair-103-invariants; do x $r pass; done
for r in asbuilt-dep-clauses asbuilt-r3-dep asbuilt-l1-end asbuilt-commit-exact control-12-dev-phases-in-transaction control-13-notransaction-ignored control-01-no-setcommitted-c1; do x $r violation; done
echo R12A-BATCH-DONE
