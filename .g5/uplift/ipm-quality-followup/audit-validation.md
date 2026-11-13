# Independent validation of audit findings R01–R12

Each audit claim is checked against the original sources before any artifact is revised. A claim can be CONFIRMED, NARROWED (true in part, with the part that does not hold stated), or REFUTED. Paths are relative to `.g5/` unless they start with `src/`.

## R01 — evaluator versus Verus clause

Probe: `Verus_verification/followup/probes/audit_r01_r03_probe.py`, run with

```sh
cd .g5/Verus_verification
python3 -I followup/probes/audit_r01_r03_probe.py native/evaluate_replay.py results/native/20261113T193952Z-replay-return-b cases/cases.json
```

It loads the original evaluator, parses the saved `raw.log` and evaluates in memory. No product code runs and no file is written.

| Claim | Source checked | Observed | Result |
|---|---|---|---|
| `slot_installed` accepts a module without resources; `installed_ok` requires `m.res` | `native/evaluate_replay.py:89-96`; `crates/ipm-load-history/src/contract.rs:291` | B1-C01 with `modules.top.resources_loaded=0` still gives `module_alt_ok = True` and no L1 violation | CONFIRMED |
| the manifest-only rule omits the document-version check | `evaluate_replay.py:109-114`; `contract.rs:296` (`m.doc == e.ver`) | a synthetic observation with `zpm_document_version=9.9.9` passes `manifest_only 2.0.0` | CONFIRMED |
| `slot_unchanged({}, 'top')` returns true | `evaluate_replay.py:80-86` | `True` | CONFIRMED. In addition, `unchanged` in Verus is `m == i` over all six `Mod` fields including `wr` (`model.rs:52-64`); the Python comparison omits the `LastUpdated` proxy that the same evaluator uses for `kept` (`evaluate_replay.py:175`) |

## R03 — evidence completeness (first part)

| Claim | Source checked | Observed | Result |
|---|---|---|---|
| `parse` overwrites repeated measurements | `evaluate_replay.py:63-77` | two `status` lines for one case yield only the last value | CONFIRMED |
| changed inputs are recorded but not used in `complete` | `evaluate_replay.py:229-234, 277-278` | `changed_inputs` only feeds the output field `cases_with_changed_inputs_need_rerun` | CONFIRMED by reading |
| only observed or begun IDs are evaluated | `evaluate_replay.py:297-300` | `not_realized` lists only binding entries whose `realized` is not `yes`, so a realized case absent from the log has no entry | CONFIRMED by reading |
| repeated History rows for one slot | `native/scripts/B2Native/Replay.cls:301-327` | the driver reports the first row per slot and only counts the others in `history.S.x.rows`; the original evaluator ignores that field | CONFIRMED as an additional multiplicity gap at the driver level. No saved replay contains `x.rows` |

## R01 and R03 — remaining subcases

| Claim | Result | Evidence |
|---|---|---|
| R01d all seven `MExp` kinds | CONFIRMED for kinds 0, 1, 2 and 6 (6 inherits the missing resources check). Kinds 3, 4 and 5 were transcribed faithfully | control family C in `Verus_verification/followup/results/20261113T232331Z-replay-controls/controls.json` |
| R01e write count and proxy | CONFIRMED. `kept` used the `LastUpdated` proxy and `unchanged` did not, so the two clauses read different projections of the same record | PROJECTION.md, control "kind 0 unchanged (present): record rewritten" |
| R01f unobserved slots, alternatives, missing values | CONFIRMED for missing values. Alternatives: no case has more than two, and `alts_ok` reads at most three, so no divergence. Unobserved model slots are never written by the model, so their clauses hold trivially | PROJECTION.md |
| R03d return code zero says nothing about completeness | NARROWED. The original evaluator did not rely on the return code alone: it required the runner's `process_outcome` COMPLETED (which needs `NATIVE-END`) and a `CASE-BEGIN` and `CASE-END` per case. It did not check `NATIVE-END` itself, the marker count or order, or that the script matched its receipt | `native/run-iris.sh:116-121`, `evaluate_replay.py:277-278` |
| R03e expectations revised after observation | CONFIRMED and quantified. `l1_permitted` in the final `cases.json` differs from the snapshot taken before the run in 27 of the 43 return-b cases, all 12 T/K cases and both approx-b cases (39 of the 55 final-run cases replayed against snapshot `fc7adec4`). c51-c52 and approx-c ran on a later snapshot and are unchanged. `l2_predicted` and `inputs` are unchanged in every final-run case, so re-evaluation without a new IRIS run is valid | `20261113T232332Z-replay-reeval/summary.json` (`l1_permitted_revised_after_run`) |
| R03f driver and result revision | CONFIRMED, with a further gap. The runner hashes the helper sources after the run ends (`run-iris.sh:129`), and only one replay run's helper hash (`approx-c`, `a3989fa8…`) matches a committed helper revision (`3b5e633a`). The helper sources used by return-b, terminate-b, sigkill-b and c51-c52 were never committed and cannot be recovered | `git log -- .g5/Verus_verification/native/scripts`; receipts |

## R02 — Committed and clause identity

| Claim | Result | Evidence |
|---|---|---|
| R02a one any-slot `kept` for every row | CONFIRMED. For a dependency row the original evaluator requires `Committed` to equal "any module changed", where `contract.rs:318-321` uses only the dependency's own module. On a dependency that rolled back under a parent that kept its manifest, the original rejects the correct `Committed=0` and accepts the wrong `Committed=1`. No saved observation exercises it, because D1 removes those dependency rows, so it is latent in the corpus. It would mis-score a native check of the D1 repair | controls "Committed per slot" on B1-C33 |
| R02b qualified row requirement | NARROWED. For `ex = 2` the original's acceptance of either presence, and its skipping of row fields when the row is absent, matches `rec_exists_ok` and `rec_fields_ok` (`contract.rs:324-333`). Two real gaps remain: a missing row observation was scored as a violation instead of unknown, and the rendered text "required unless Q alternative; may be missing" does not record which open question produced it. No case in the final `cases.json` uses `ex = 2` (the provisional Q-06b makes every started row required) | controls "qualified row (ex=2)"; vocabulary scan of `cases.json` |
| R02c aggregation by one boolean | CONFIRMED. `native_matches_b1_l1_judgement` (`evaluate_replay.py:291`) compares only "violates or not". A control whose observation fails `L1-NAMESPACE` instead of the model's `H-COMMITTED[top]` is reported as a match | control "clause identity" |
| R02d individual failed clauses | Closed by controls for each clause family | control family C |

## Historical conclusions after re-evaluation

The corrected evaluator was run on all seven saved replay runs against the final `cases.json` (`10410864…`). In the five final runs, none of the 59 verdicts changed, and in every case the failed clauses equal the model's own `violated_clauses`. The weaknesses above could have accepted wrong observations, but the observations IPM actually produced did not hit them. In the superseded first attempt, B1-C19, B1-C20 and B1-C40 change to INDETERMINATE because `LastUpdated` was not yet observed. The original had reported B1-C19 and B1-C20 as Committed violations on that missing evidence.

The remaining rows are filled in as each finding is worked.
