#!/usr/bin/env bash
# Final-input batch for the follow-up: rerun every follow-up check with receipts. Native runs are not
# repeated here (their observations are archived); their verdicts are re-derived where an evaluator changed.
set -u
cd "$(dirname "$0")/../.." || exit 1
R() { python3 -I -B followup/bin/receipt.py "$@" 2>&1 | tail -1; }
N=followup/results/native
R final-replay-controls "exit 0; A 175, B 59, C 45, D 14 pass" --inputs followup/replay native/evaluate_replay.py cases/cases.json native/cases -- python3 -I -B followup/replay/run_controls.py {OUT}
R final-replay-reeval "final runs: 0 verdicts changed, 59 clause sets equal to the model" --inputs followup/replay cases/cases.json -- python3 -I -B followup/replay/reevaluate_all.py {OUT}
R final-replay-rerun-compare "identical 45/45; verdicts 23 PERMITTED, 20+2 VIOLATES" --inputs followup/replay cases/cases.json $N/20261114T000144Z-replay-return-rerun $N/20261114T000251Z-replay-c51-c52-rerun -- bash -c "python3 -I -B followup/replay/compare_reruns.py {OUT} results/native/20261113T193952Z-replay-return-b $N/20261114T000144Z-replay-return-rerun results/native/20261113T205137Z-replay-c51-c52 $N/20261114T000251Z-replay-c51-c52-rerun | tail -1 && python3 -I -B followup/replay/evaluate_replay2.py $N/20261114T000144Z-replay-return-rerun --cases cases/cases.json --out {OUT} | tail -2 && python3 -I -B followup/replay/evaluate_replay2.py $N/20261114T000251Z-replay-c51-c52-rerun --cases cases/cases.json --out {OUT} | tail -2"
R final-history-readers-model "valid neighbor 0 violations; archived correspondence holds" --inputs followup/models/history_readers.py -- python3 -I -B followup/models/history_readers.py {OUT}
R final-loadtx-model "12 correspondence rows hold" --inputs followup/models/loadtx.py -- python3 -I -B followup/models/loadtx.py {OUT}
R final-prefix-cases "exit 0" --inputs followup/models followup/replay/l1sem.py -- python3 -I -B followup/models/prefix_cases.py {OUT}
echo FINAL-BATCH-DONE
