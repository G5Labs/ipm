# Checker repairs, round 1

Question: which gaps in the G arm's native evaluators let the wrong observations of `checker-calibration.md` through, what is the smallest repair in the artifact that owns each gap, and does each repaired checker reject its wrong observation while still giving every saved run its saved verdict? Product `571e04a1`. No IRIS session or container was used. Every evaluation below ran on a scratch copy of a saved run or of a calibration variant. `src/`, `tests/`, `playbook-uplift/`, `.g5/Verus_verification/consolidated/` and `.g5/uplift/consolidated/` (other than this file) were not changed.

A wrong observation counts as rejected when the repaired checker's verdict for the edited case becomes DEVIATES, INCOMPLETE, DISAGREES, VIOLATES or FAIL and the verdict names the edited field. A verdict that names only another field does not count.

## Results

| Accepted observation | Why it was accepted | Repair (owner, file:line) | Repaired verdict on the wrong observation | Neighbor |
|---|---|---|---|---|
| E-b1: A1 dependency row survives (`dep.hist.rows` 0→1) | It matches the contract (`expect: "1"`, Q-06 provisional). It refutes `predicted_as_built: "0"` (A2 OBS-L1/OBS-H5), but a refuted prediction only set `as_built_prediction` in `verdict.json` | Evaluator. A case that meets every contract expectation and refutes a prediction is INCOMPLETE and lists `as_built_refuted_fields` (`native/evaluate.py:143-144`, `:156-163`, `:323`) | A1 CONFORMS → INCOMPLETE `as_built_refuted=dep.hist.rows` | cases-a A1 stays DEVIATES `mismatch=dep.hist.rows` |
| E-b2: A0 parent slot carries the dependency's `Package`/`SourceName` | No case compared row `Package` | Expectation. A0–A5 gain `par.hist.row1.Package` and `dep.hist.row1.Package` harness guards set to the fixture names each case loads (`native/cases/expected-cases.json:43,52,138,165,223,246,316,339,411,432,494,515`). The dependency's guard has the same `when: dep.hist.rows=1` as its other row fields in A1–A3 | A0 CONFORMS → INCOMPLETE `harness_failed=par.hist.row1.Package` | cases-a and cases-b unchanged. Every observed A0–A5 slot row names its fixture |
| E-c1: B2 developer-mode manifest reported rolled back (`mod.exists` 1→0, `hist.row1.Committed` 1→0) | As E-b1. Both edited fields match the contract and refute `predicted_as_built: "1"` | Same evaluator rule as E-b1 | B2 CONFORMS → INCOMPLETE `as_built_refuted=mod.exists,hist.row1.Committed` | cases-b B2 stays DEVIATES `mismatch=mod.exists,hist.row1.Committed` |
| E-d: A0 `load.tlevel` 0→1 | 23 cases observe `load.tlevel`/`update.tlevel` without an expectation, and unlisted fields were never compared | Evaluator. In a non-premise case, every unlisted `<label>.tlevel` written by `B2Native.Util.Run` next to `<label>.ok` is expected to be `0`. A case that lists a non-zero level for itself (D0, D1) is left to its own expectations (`native/evaluate.py:80-89`, `:122-125`) | A0 CONFORMS → DEVIATES `mismatch=load.tlevel` | Every observed unlisted `*.tlevel` in the saved runs is `0`. No verdict changed |
| E-d2: A1 second, conflicting `dep.hist.rows` line | `parse_log` kept the last value and only listed the duplicate | Evaluator. A field observed twice with different values makes its case INCOMPLETE with `conflicting_observations` (`native/evaluate.py:59-63`, `:77`, `:156`, `:246`, `:294`) | A1 CONFORMS → INCOMPLETE `conflicting=dep.hist.rows` (also `as_built_refuted=dep.hist.rows`, `missing=dep.hist.row1.Package`) | No saved run has a duplicate. An identical repeated line still passes (extra check X-dup-identical) |
| Saved controls `B2:mod.exists=0` (cases-b and replay-fast) and `M5:d.exists=0` (stagec2) recorded `detected: true` | Any change counted as detection, including a field moving from mismatch to match | Evaluator: a control is detected only when the mutated value is flagged by a contract mismatch or a refuted prediction, and it records `effect` (`native/evaluate.py:259-275`). Records: the three entries now read `detected: false`, `effect: "accepts mutated value"`, `detected_as_first_recorded: true`, plus a `correction` note (`results/native/20261113T182820Z-cases-b/verdict.json`, `20261113T184230Z-replay-fast/verdict.json`, `20261113T204405Z-stagec2/verdict.json`) | With the repaired evaluator the same controls are detected for the right reason: the mutated value refutes `predicted_as_built: "1"` | No other field of the three records changed |
| R-d: B1-C26 module already installed before the command | `before.*` was never compared with `inputs.installed_before` | Evaluator. `precondition_bad` compares `before.modules.<slot>.manifest_present`, `version` and `developer_mode` with the case's `installed_before` for `top` and each dependency, and a mismatch is a setup failure (`native/evaluate_replay.py:90-116`, `:282`) | B1-C26 AGREES/PERMITTED → INCOMPLETE/INCOMPLETE `setup_failed=before.modules.top.manifest_present` | In all 7 replay runs every case's before-state matches its inputs. No verdict changed |
| S-d: no IPM "Test Results" block, extractor reads a stale TestInstance | The IPM summary check ran only when the line existed | Evaluator. PASS needs IPM's `Methods:` line (`native/evaluate_suite.py:34-35`) | PASS → INCOMPLETE | All 8 suite runs have the line |
| S-d2: extractor lists 1 of the 2 methods IPM ran | IPM's total was never compared with the extracted count | Evaluator. PASS needs IPM's total to equal the number of extracted methods (`native/evaluate_suite.py:34-35`) | PASS 1/1 → INCOMPLETE | In all 8 suite runs the totals match, skipped methods included (InstallModule 7/7, DependencyResolution 1/1) |

Now rejected: E-b1, E-b2, E-c1, E-d, E-d2, R-d, S-d, S-d2. Still accepted: none of the calibrated G-arm observations. E-c2 and E-c3 were not accepted before the repair either. Their status is under "Not repaired".

## Sources for the new expectations

- Slot `Package` guards (E-b2). The value is the fixture each case loads, fixed before the run. Parent and dependency names come from `native/make_fixtures.py:84-87` and `CaseA0`…`CaseA5` in `native/scripts/B2Native/Cases.cls`. They are the same names the `CMD` lines show. `Util.cls` `HistFor` selects a slot's rows `WHERE Package = ?`, so a different name in a slot means the driver read the wrong row. The guards are therefore `harness: true` and a failure makes the case INCOMPLETE, not DEVIATES. A0's claim ("each module has its own finalized, committed record") and INT-HIST-08 state the intent. The note in `expected-cases.json` records that the guards were added after the runs.
- `tlevel = 0` (E-d). The value comes from the driver and the model, not from the observation. `Cases.cls` `Go` rolls back after every case, so each case starts at `$TLEVEL` 0. The ontology entry `transactions-and-dev-mode#transaction` says a package change runs as one transaction. `theorem_tlevel` (`crates/ipm-load-history/src/proofs.rs:521-527`) proves that a load ends at its entry level. D0 and D1 hold a caller transaction and list `load.tlevel: "1"`, and the rule leaves them alone. M5 and M5N already listed `load.tlevel: "0"` (`Utils/Module.cls:1358-1362`).
- As-built refutation (E-b1, E-c1). Nothing new is expected. The predictions were written before the runs. The rule only stops a contract match that contradicts them from reading CONFORMS.

## Neighbors (saved runs)

`rerun.py` reproduced 61 saved evaluations of 41 runs: 37 by `evaluate.py`, 16 by `evaluate_replay.py` and 8 by `evaluate_suite.py`. Each was run twice on identical copies, once with the evaluators and expectations as they were before the repair (`old`) and once with the repaired ones (`new`). Four replay re-evaluations against cases.json `d3334b16` were skipped, because that version of `cases/cases.json` is not kept in the repository.

- Old evaluator vs saved verdict: identical for 58 of 61. The three differences come from expectations that changed after the evaluation, not from the repair. `cases-c-terminate/verdict.json` (C1) and `cases-c-sigkill/verdict.json` (C3) were evaluated at 18:41 against `expected-cases.json` `3ca2500d`, and their saved `verdict-reeval-3fdc3831.json` match the old evaluator exactly. `replay-fast/verdict-reeval-e543ed6b.json` (C1) used an older version, `e543ed6b`.
- Old vs new: every case verdict is unchanged in 60 of 61 evaluations. Every replay and suite verdict is unchanged, and so is every premise and every case in cases-a, cases-b, stagec2 and replay-return-b.
- Intended change: `20261113T204152Z-stagec/verdict.json` S4 goes from CONFORMS to INCOMPLETE `as_built_refuted=load.ok,mod.exists,hist.row1.Committed`, and the run's substantive verdict goes from EVALUATED to PARTIAL. That real CONFORMS was a vacuous pass. The relative path did not exist where IRIS resolves it, so the refusal had another cause. The later re-evaluation `verdict-reeval-31de3da7.json` already gives S4 INCOMPLETE (harness guard `rel.exists`), and the rebound case S4R in stagec2 DEVIATES as the prediction said. The saved stagec `verdict.json` was not rewritten.
- Unchanged verdicts with new detail: C2 in `replay-fast/verdict.json` and S3 in stagec stay DEVIATES and now also list their refuted predictions (C2: `hist.row1.Version, Finalized, Package, Committed`; S3: `b.exists, b.doc.version`). Unlisted `*.tlevel` fields move from `unexpected_observations` into `fields` with `implicit: true`.
- Controls on re-evaluation: 55 of the 57 replayed `--mutate` controls keep their `detected` value. Most mismatch-to-match controls are still detected, because the mutated value refutes a prediction. Two now read `detected: false`. `C2:hist.row1.Finalized=0` (cases-c-terminate, DEVIATES→CONFORMS) reads `accepts mutated value (verdict depends on this field)` and `C4R:hist.row1.Committed=1` (cases-c4r) reads `accepts mutated value`. Neither field has a prediction.

## Wrong-observation variants

The 21 calibration variants (`checker-calibration/g-arm/Verus_verification/results/native/*__*`, each with its `EDITS.json`) were copied without their verdicts and run with the calibration's own commands. The old tree reproduced every calibration result. The new results are in the table above. The neighbor variants E-N-cases-a, E-N-cases-b, E-N-stagec2, R-N-return-b (43 cases) and S-N-LoadModuleWithDeps give the same verdicts in both trees. Variants that were already rejected stay rejected for the same reason: E-a (A0 DEVIATES `dep.cls.compiled`), R-a, R-b1, R-b2, R-c and S-d-control. No run crashed.

Extra checks with the repaired evaluator, on scratch copies:

- X-b2-whole-row: the A0 parent slot holds the dependency's whole row (ID 12, `Package`, `SourceName`), which is the driver fault E-b2 stands for. A0 INCOMPLETE `harness_failed=par.hist.row1.Package`.
- X-dup-identical: `OBS A1 dep.hist.rows=0` repeated with the same value. A1 stays DEVIATES `mismatch=dep.hist.rows`, and the duplicate is only listed.
- `--mutate` controls of the `tlevel` rule. Each of `S3:update.tlevel=1`, `S3:setup.tlevel=1`, `H2:set.tlevel=1` (stagec), `G:g3.tlevel=1` (cases-dg), `B0:load.tlevel=1`, `B2:uninstall.tlevel=1` (cases-b) and `M5:load.tlevel=1` (stagec2) moves its field from match to mismatch and is detected. `D0:load.tlevel=0` turns D0 from CONFORMS to DEVIATES against its own listed `1`, so the rule does not override D0.

## Not repaired

- E-c2 (M5, the dependency's unjournaled module, class and document reported rolled back). It stays DEVIATES because of the unedited `d.hist.rows`, as before. The edited fields are now named in `as_built_refuted=d.exists,d.cls.compiled,d.doc.version`. Nothing cross-checks the observation against premises N-P07 and N-P20A directly. That would need a new premise-to-case link, which no expectation form provides.
- E-c3 (A5, everything reported rolled back). It stays INCOMPLETE, flagged as missing evidence. Its missing list gains `dep.hist.row1.Package`, because the new guard follows A5's unguarded dependency row fields. The `when` guards that the calibration proposed would change how A5 reports a rollback, which none of the listed observations needed.
- Row `Package` in single-module cases (B0–B4, D0, D1 `inflight`, G, H1, S1–S4R) and in M5/M5N. These slots are read with `HistSince`, so a wrong name there is a product fault, not the slot mix-up of E-b2. Several refused loads record an empty `Package` (g3, g6, g7, S1, S2, S2B, S2N, S4, S4N1), so an expectation would first need a decision on what such a row should name. M5 and M5N are evaluated from their run's pre-run snapshot, so a guard there would apply only under `--use-current`.
- S-d TestInstance check. Matching `UTINSTANCE <ns> <n>` with IPM's `Test Run #<n>` is not needed to reject S-d, and the `suite-test-*` logs print two `UTINSTANCE` lines.
- `followup/replay/evaluate_replay2.py` also accepts R-d. It lies outside the files this task names.
- Saved control records of the same shape as the two named controls (field mismatch→match, verdict unchanged, recorded `detected: true`): `B3:hist.row1.Committed=0` (cases-b), `F1:old.final.exists=0` (cases-ef, replay-fast), `C2:hist.row1.Finalized=0` (cases-c-terminate), `S1:load.ok=1`, `S2:mod.exists=0`, `H2:find.shown=5` (stagec) and `S4R:load.ok=0` (stagec2). Only the two named controls were corrected in the records. Controls that moved a verdict out of DEVIATES (A1, D1, E4, E1, F0, C4R, H1, X1) show that the deviation depends on that field, which is their purpose.
- The saved `verdict.json` files were not regenerated. A rewrite would replace the expectation hashes and `written_before_run` values recorded at evaluation time. The only record edits are the three control entries above.
- `evaluate_replay.py`'s own `--mutate` controls still count any change as detection. None of the calibrated observations depends on them.

## Commands

All scratch work is under `/private/tmp/claude-501/-Users-ziniuw--claude-spares-spare-GQFuCD/71f136fd-4397-41d7-b437-1517c7d9c194/scratchpad/checker-repairs` (below, `<S>`). Every Python run used `python3 -I`.

1. Before any edit, copy `native/evaluate.py`, `evaluate_replay.py`, `evaluate_suite.py` and `native/cases/*.json` to `<S>/orig-native/`, copy the three verdicts that hold the controls to `<S>/orig-verdict-*.json`, and record hashes in `<S>/sha-before.txt` (`shasum -a 256 …`).
2. Survey of saved runs (inline `python3 -I` scripts): the evaluator and controls of every `verdict*.json`, every observed `*.hist.row<N>.Package` and `*.tlevel` against the expectations, every replay case's `before.modules.*` against `inputs.installed_before`, and every suite run's IPM summary.
3. Edits to `native/evaluate.py`, `native/evaluate_suite.py` and `native/evaluate_replay.py`. Edit of `native/cases/expected-cases.json` with `json.dumps(indent=1)`, which reproduces the original file byte for byte before the edit.
4. `cd <S> && python3 -I rerun.py`. This builds `<S>/old` and `<S>/new`, copies every saved run with an evaluator verdict and the 21 calibration variants, and runs each evaluation in both trees. The commands have these forms:
   - `cd <tree>/Verus_verification && python3 -I native/evaluate.py results/native/<run> native/cases/<expected>.json [--mutate <each saved control>] [--use-current]`
   - `cd <tree>/Verus_verification/native && python3 -I evaluate_replay.py ../results/native/<run> [--mutate <each saved control>] [--cases cases/b1-cases-05842975.json | ../cases/cases.json]`
   - `cd <tree>/Verus_verification && python3 -I native/evaluate_suite.py results/native/<run> <TestCase>`
   - Variants: the calibration's commands, for example `cd <tree>/Verus_verification && python3 -I native/evaluate.py results/native/20261113T182759Z-cases-a__E-b2-A0-dep-row-under-parent native/cases/expected-cases.json`.
   Output: `<S>/results.json`, with each verdict and stdout copied to `<S>/out/{old,new}/<run>/`.
5. `cd <S> && python3 -I analyze.py > analysis.txt`
6. `cd <S> && python3 -I extra_variants.py` (X-b2-whole-row and X-dup-identical, output `<S>/extra-results.json`).
7. `tlevel` controls, in `<S>/new/Verus_verification`:
   - `python3 -I native/evaluate.py results/native/20261113T204152Z-stagec native/cases/expected-stagec.json --mutate S3:update.tlevel=1 --mutate S3:setup.tlevel=1 --mutate H2:set.tlevel=1`
   - `… 20261113T182832Z-cases-dg native/cases/expected-cases.json --mutate D0:load.tlevel=0 --mutate G:g3.tlevel=1`
   - `… 20261113T182820Z-cases-b native/cases/expected-cases.json --mutate B2:uninstall.tlevel=1 --mutate B0:load.tlevel=1`
   - `… 20261113T204405Z-stagec2 native/cases/expected-stagec.json --mutate M5:load.tlevel=1`
8. Correction of the three control entries (inline `python3 -I` script, run in `.g5/Verus_verification/results/native`). Before writing, it checked that `json.dumps(indent=2)` reproduces each file byte for byte and that each entry was a DEVIATES→DEVIATES, mismatch→match control recorded as detected.
9. After all edits, steps 4–6 were run again. The results above come from that final run.

## Files changed

| File | sha256 before | sha256 after |
|---|---|---|
| `.g5/Verus_verification/native/evaluate.py` | `335ec47f…` | `4ea6e7b9…` |
| `.g5/Verus_verification/native/evaluate_replay.py` | `3d9c2f19…` | `27bb1d59…` |
| `.g5/Verus_verification/native/evaluate_suite.py` | `2856ec8d…` | `88d97819…` |
| `.g5/Verus_verification/native/cases/expected-cases.json` | `3fdc3831…` | `8cfdff56…` |
| `.g5/Verus_verification/results/native/20261113T182820Z-cases-b/verdict.json` | `2631d32e…` | `0fc71519…` |
| `.g5/Verus_verification/results/native/20261113T184230Z-replay-fast/verdict.json` | `8e3a5944…` | `aeeb3b2f…` |
| `.g5/Verus_verification/results/native/20261113T204405Z-stagec2/verdict.json` | `2c79d5fe…` | `e62f5d73…` |

A new `--use-current` re-evaluation against the repaired `expected-cases.json` is named `verdict-reeval-8cfdff56.json`.
