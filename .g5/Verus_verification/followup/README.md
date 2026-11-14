# Follow-up verification: runbook

This directory holds the follow-up review's corrected verification foundation. The original foundation in `..` (crates, native runner, results) is unchanged. Its runbook is `../README.md`. The review record (closure ledger, audit validation, scope ledger, independent review, handoff) is in `.g5/uplift/ipm-quality-followup/`.

```
 followup/
   replay/     corrected replay evaluator (l1sem.py, evaluate_replay2.py), controls, re-evaluation, PROJECTION.md
   models/     loadtx.py (R05/R06), history_readers.py (R04), prefix_cases.py (R06d), MODELS.md
   native/     run-native.sh, FUNative helper class, fixtures, session generator, expectations, evaluator
   bin/        receipt.py (any check), verus_check.py (original Verus/explorer checks, follow-up receipts), r12a-batch.sh
   results/    one directory per run: command.txt, raw.log, receipt.json, outputs; native/ for IRIS runs; index.jsonl
   PROJECTIONS.md   what each model abstraction keeps and what survives it (R07)
```

## Pins and environment

| Item | Value |
|---|---|
| Product | `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, read-only |
| Original submission | `ae61979578ecbc23d47b3af44c076c105663f38c` |
| Verus (original checks) | `0.2026.09.27.3cf1832`, Rust `1.98.1`, as in `../README.md`. `verus_check.py` builds into `$IPM_UPLIFT_WORK/verus-target`, so nothing in the repository changes |
| Python | 3, run every script with `python3 -I -B` from `.g5/Verus_verification` |
| IRIS | the original product image `ipm-verus-uplift-20261113:571e04a1` (IRIS 2026.1), used read-only |
| Container names | `IRIS_CONTAINER`, `IRIS_NETWORK`, `IPM_UPLIFT_WORK` from the environment. `run-native.sh` has no defaults, labels its container `ipm-quality-followup=1`, and refuses to replace a container without that label. It starts no ORAS registry and disables the remote registry repository inside each session |

## 1. Corrected replay evaluation (R01–R03)

```sh
python3 -I -B followup/bin/receipt.py replay-controls "exit 0; counts A/B/C/D all pass" --inputs followup/replay native/evaluate_replay.py cases/cases.json -- python3 -I -B followup/replay/run_controls.py {OUT}
python3 -I -B followup/bin/receipt.py replay-reeval "final runs: 0 verdicts changed" --inputs followup/replay cases/cases.json -- python3 -I -B followup/replay/reevaluate_all.py {OUT}
```

Expected: controls A 175, B 59, C 45, D 14, all pass (`results/20261114T002645Z-final-replay-controls/controls.json`). Re-evaluation: in the five final runs, 59 cases, 0 verdicts changed, and every case's failed clauses equal the model's (`results/20261114T002646Z-final-replay-reeval/delta.md`). To evaluate one run by hand:

```sh
python3 -I -B followup/replay/evaluate_replay2.py results/native/20261113T193952Z-replay-return-b --cases cases/cases.json --out /tmp/eval
```

A distinguishing control on the same run (resources removed from B1-C01's observation; the original evaluator still permits it):

```sh
python3 -I -B followup/replay/evaluate_replay2.py results/native/20261113T193952Z-replay-return-b --cases cases/cases.json --out /tmp/eval --mutate B1-C01:modules.top.resources_loaded=0
```

Expected: `B1-C01 … l1=VIOLATES L1-NAMESPACE`.

Re-execution with the committed driver (R03f). The archived scripts of the two return-path runs are `native/sessions/replay-return-b.cos` and `replay-c51-c52.cos`. `NATIVE_ORIGINAL=1 ./run-native.sh exec …` stages the committed original driver and fixtures and records their hash at start. `replay/compare_reruns.py` compares the fresh observations with the archived ones field by field. Expected: identical 45/45 (`results/20261114T002647Z-final-replay-rerun-compare`).

## 2. Models (R04–R06)

See `models/MODELS.md` for bounds, clauses, variants and results.

```sh
python3 -I -B followup/bin/receipt.py history-readers-model "valid neighbor 0" --inputs followup/models/history_readers.py -- python3 -I -B followup/models/history_readers.py {OUT}
python3 -I -B followup/bin/receipt.py prefix-cases "exit 0" --inputs followup/models -- python3 -I -B followup/models/prefix_cases.py {OUT}
python3 -I -B followup/bin/receipt.py loadtx-model "11 correspondence rows hold" --inputs followup/models/loadtx.py -- python3 -I -B followup/models/loadtx.py {OUT}
```

The last one takes about 10 minutes. Expected results are in `models/MODELS.md` and in the current receipts `results/20261114T002647Z-final-history-readers-model`, `results/20261114T002830Z-final-prefix-cases` and `results/20261114T002647Z-final-loadtx-model`.

## 3. Native cases (R04, R05, R06e)

```sh
cd followup/native
python3 -I -B make_fixtures.py && python3 -I -B make_sessions.py      # fixtures, sessions/*.cos, cases/expected-*.json
./run-native.sh up                                                     # expect: "up: $IRIS_CONTAINER healthy on $IRIS_NETWORK"
./run-native.sh exec r05-modes-nesting sessions/r05.cos
python3 -I -B evaluate_native.py ../results/native/<run-id printed above> r05
./run-native.sh down
```

Expected: ten R05 cases CONFORM (`results/native/20261113T234817Z-r05-modes-nesting/verdict.json`). The session loads only test fixtures. `p21.cos` (rollback ordering probe on scratch globals and one probe history row it deletes) and `r04.cos` (history readers on seeded test rows, which it removes) run the same way. Expected: N-P21A and N-P21R DEVIATE from the before-image prediction recorded before the run, and the others CONFORM (`results/native/20261113T234716Z-p21-ordering-r2`). All seven R04 cases CONFORM (`results/native/20261113T234810Z-r04-history-readers`). Expectations are snapshotted into each run's `inputs/` and were committed before the first run (git `104fa5e2`).

## 4. Round-three checks on the final inputs (R12a)

```sh
./followup/bin/r12a-batch.sh
```

Expected: the 19 lines in `results/` from `20261113T232529Z-r12-asbuilt-proofs` to `20261113T233034Z-control-01-no-setcommitted-c1` all match their expectations. On a cold build the as-built line reads `verified 2322`, because the parser adds vstd's 2059 obligations. The crate's own summary line in `raw.log` reads `263 verified, 0 errors`, and `inputs.sha256` equals the original final receipt `../results/verus/20261113T222933Z-asbuilt-proofs-r3` on every proof input.

## 5. One end-to-end trace and its distinguishing control (R12b)

Defect D1, a dependency's record lost when its parent rolls back, from intent to the corrected native verdict:

```
 intent    ontology history.survives-rollback, history.dependencies (issue #366: "if install rolls back we still need the history")
   │
 L1        contract.rs dep_rec: an attempted dependency's row must exist (ex = 1), Committed = its own changes kept
   │
 L2        model.rs: the dependency row is inserted inside the parent's transaction; the parent's rollback removes it
   │       proof cases.rs:193 case_c31_dep_record_lost; explorer asbuilt-r3-dep VIOLATION
   │       (rerun on final inputs: followup/results/20261113T233033Z-asbuilt-r3-dep)
   │
 case      cases/cases.json B1-C31: l1_vs_l2.violated_clauses = [H-RECORD-EXISTS[dep1]]
   │
 L3        results/native/20261113T193952Z-replay-return-b raw.log: OBS B1-C31 status=ERR,
   │       history.dep1.row=0, history_temp.dep1.row=0
   │
 verdict   followup/results/20261114T002646Z-final-replay-reeval: B1-C31 l1=VIOLATES H-RECORD-EXISTS[dep1],
           same clauses as the model
```

Distinguishing controls:

- In the corrected evaluator, the control "record exists: dependency row restored as the contract requires (valid neighbor)" restores the row on the same observation and gets PERMITTED. The control "Committed per slot" shows that a restored row must read `Committed=0`, because the dependency itself was rolled back. The original evaluator gets that one backwards (R02a).
- In the original model, repair variant 103 passes the dependency clauses (`followup/results/20261113T233031Z-repair-103-dep-clauses`).
- In the follow-up model, the same repair fails them when a dependency declares its own NoJournal, or when a Terminate falls between its `Committed` mark and the commit (`models/MODELS.md`).

## Results index

`results/index.jsonl` marks superseded runs. Native runs keep a failed first attempt: `results/native/20261113T234651Z-p21-ordering` (a syntax error in one case of the driver; superseded by `-r2`).
