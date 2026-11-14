# Independent review record (R12e)

Two fresh reviewers, each given the code and evidence rather than the author's conclusions, checked this follow-up's material corrections. Each was told to derive its own expectations from the original contract, the original evaluator, the audit and the product source before reading the follow-up's claims. Neither reviewer wrote any of the reviewed artifacts. Neither ran IRIS or touched a container: native evidence was reviewed from saved files. Each reviewer's own record is the file named below, with its scratch scripts and outputs under `scratch/`.

| Review | File | Covered | Not covered |
|---|---|---|---|
| Technical | `review-technical.md` | Corrected evaluator against `contract.rs` (line by line), its projection against `model.rs` and the driver, a rerun of its 289 controls, 15 single-check deletions (13 caught), adversarial observations; the re-evaluation claim with 5 cases checked by hand from `raw.log`; the load model's mode resolution, nesting reduction, P-14, PREFIX independence, the counts for COMMITTED 30, REPAIR-103 818/27/60 and GUARDED 141, and the Terminate window; the history-reader model against `ConstructSQLWhere` including key collation, and N-R04-SEL; native evidence integrity (expectations committed before the runs, snapshots against receipts, runner names); the R12a reruns and the 2322 = 2059 + 263 explanation | the replay re-executions `followup/results/native/20261114T000144Z-*` and `000251Z-*` and their comparison (they appeared during the review); the owner files |
| Owner files | `review-owner-files.md` | see that file | see that file |

## Dispositions of the technical review's findings

| Finding | Severity | Disposition | Where |
|---|---|---|---|
| T3-M1: the load model did not apply P-06c (innermost frame first) to JournalManager objects at a Terminate. With the right order a NoJournal dependency's row is written journaled and rolled back (384 hidden REC-EXISTS violations). The before-image mutant's apparent detection came from this | material | accepted and repaired. Terminate now unwinds JournalManagers and logs innermost first. Native N-P19 added as a twelfth correspondence row and holds. As-built REC-EXISTS 9,942. M-BEFORE-IMAGE reported as equivalent in this model (refuted natively by N-P21). Every other count unchanged | `followup/models/loadtx.py`, `MODELS.md` "Revisions", receipt `followup/results/20261114T002647Z-final-loadtx-model` |
| T1-m1: any status other than "OK" counted as an error, so a malformed status could pass `L1-STATUS` | minor | accepted and repaired: only `OK` and `ERR` are accepted. A new control holds | `followup/replay/l1sem.py` `st_ok`, control "status: value outside the driver vocabulary" |
| T1-m2: no control caught the removal of the BEGIN/END check or of `res` from the unchanged comparison | minor | accepted: two controls added ("case began twice", "resources appeared without a module record") | `followup/replay/run_controls.py`, receipt `followup/results/20261114T002645Z-final-replay-controls` (A 175, B 59, C 45, D 14) |
| T1-m3: the evaluator checked only `input.cos` against the receipt, not the binding and cases snapshot | minor | accepted: it now recomputes the original runner's `expectations_snapshot_sha256` over `inputs/` when the receipt has one. A control holds | `evaluate_replay2.py` `load_run` |
| T3-m: PREFIX reads its durability points from the model's own commit steps, so a model that never commits would show PREFIX 0 | minor | accepted as a documented limit. COMMITTED and RETRY catch that case | `MODELS.md` "What PREFIX does and does not check" |
| T3-m: the Terminate window depends on the follow-up's repair variant finalizing dependency rows at a Terminate close, which the crate's variant 103 does not do | minor | accepted: stated in `MODELS.md`, `design_simplification.md` S2, `bugs_issues.md` D1 and `open_questions.md` Q-30 | as listed |
| T3-m: end-state counts include duplicate cut points (the 60 window end states are 30 distinct) | minor | accepted: stated wherever the window is cited | as above |
| T3-m: MODELS.md said the model takes about 10 minutes | minor | corrected (about 2 minutes) | `MODELS.md` |
| T3-m: MODELS.md quoted PREFIX 2,216 and 5,228 from a superseded run | minor | already corrected during the review (2,440 and 5,780) | `MODELS.md` |
| T5-m: `run-native.sh exec` did not check the container label, and `down` removed the network without an ownership check | minor | accepted and repaired: `exec` refuses an unlabeled container, `down` removes the network only when nothing is attached | `followup/native/run-native.sh` |

Every follow-up check was rerun on the final inputs after these repairs (`followup/bin/final-batch.sh`, receipts `followup/results/20261114T00264*Z-final-*` and `20261114T002830Z-final-prefix-cases`). The repairs themselves were not re-reviewed by a third reviewer. They were validated by the reruns above, and the material one by the new native correspondence row N-P19.

## Dispositions of the owner-file review's findings

See the section appended below once that review completes.
