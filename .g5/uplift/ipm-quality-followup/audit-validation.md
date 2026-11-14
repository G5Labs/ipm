# Independent validation of audit findings R01–R12

Each audit claim is checked against the original sources before any artifact is revised. A claim can be CONFIRMED, NARROWED (true in part, with the part that does not hold stated), or REFUTED. Paths are relative to `.g5/` unless they start with `src/`.

## R01: evaluator versus Verus clause

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

## R03: evidence completeness (first part)

| Claim | Source checked | Observed | Result |
|---|---|---|---|
| `parse` overwrites repeated measurements | `evaluate_replay.py:63-77` | two `status` lines for one case yield only the last value | CONFIRMED |
| changed inputs are recorded but not used in `complete` | `evaluate_replay.py:229-234, 277-278` | `changed_inputs` only feeds the output field `cases_with_changed_inputs_need_rerun` | CONFIRMED by reading |
| only observed or begun IDs are evaluated | `evaluate_replay.py:297-300` | `not_realized` lists only binding entries whose `realized` is not `yes`, so a realized case absent from the log has no entry | CONFIRMED by reading |
| repeated History rows for one slot | `native/scripts/B2Native/Replay.cls:301-327` | the driver reports the first row per slot and only counts the others in `history.S.x.rows`; the original evaluator ignores that field | CONFIRMED as an additional multiplicity gap at the driver level. No saved replay contains `x.rows` |

## R01 and R03: remaining subcases

| Claim | Result | Evidence |
|---|---|---|
| R01d all seven `MExp` kinds | CONFIRMED for kinds 0, 1, 2 and 6 (6 inherits the missing resources check). Kinds 3, 4 and 5 were transcribed faithfully | control family C in `Verus_verification/followup/results/20261114T002645Z-final-replay-controls/controls.json` |
| R01e write count and proxy | CONFIRMED. `kept` used the `LastUpdated` proxy and `unchanged` did not, so the two clauses read different projections of the same record | PROJECTION.md, control "kind 0 unchanged (present): record rewritten" |
| R01f unobserved slots, alternatives, missing values | CONFIRMED for missing values. Alternatives: no case has more than two, and `alts_ok` reads at most three, so no divergence. Unobserved model slots are never written by the model, so their clauses hold trivially | PROJECTION.md |
| R03d return code zero says nothing about completeness | NARROWED. The original evaluator did not rely on the return code alone: it required the runner's `process_outcome` COMPLETED (which needs `NATIVE-END`) and a `CASE-BEGIN` and `CASE-END` per case. It did not check `NATIVE-END` itself, the marker count or order, or that the script matched its receipt | `native/run-iris.sh:116-121`, `evaluate_replay.py:277-278` |
| R03e expectations revised after observation | CONFIRMED and quantified. `l1_permitted` in the final `cases.json` differs from the snapshot taken before the run in 27 of the 43 return-b cases, all 12 T/K cases and both approx-b cases (39 of the 55 final-run cases replayed against snapshot `fc7adec4`). c51-c52 and approx-c ran on a later snapshot and are unchanged. `l2_predicted` and `inputs` are unchanged in every final-run case, so re-evaluation without a new IRIS run is valid | `20261114T002646Z-final-replay-reeval/summary.json` (`l1_permitted_revised_after_run`) |
| R03f driver and result revision | CONFIRMED, with a further gap. The runner hashes the helper sources after the run ends (`run-iris.sh:129`), and only one replay run's helper hash (`approx-c`, `a3989fa8…`) matches a committed helper revision (`3b5e633a`). The helper sources used by return-b, terminate-b, sigkill-b and c51-c52 were never committed and cannot be recovered | `git log -- .g5/Verus_verification/native/scripts`; receipts |

## R02: Committed and clause identity

| Claim | Result | Evidence |
|---|---|---|
| R02a one any-slot `kept` for every row | CONFIRMED. For a dependency row the original evaluator requires `Committed` to equal "any module changed", where `contract.rs:318-321` uses only the dependency's own module. On a dependency that rolled back under a parent that kept its manifest, the original rejects the correct `Committed=0` and accepts the wrong `Committed=1`. No saved observation exercises it, because D1 removes those dependency rows, so it is latent in the corpus. It would mis-score a native check of the D1 repair | controls "Committed per slot" on B1-C33 |
| R02b qualified row requirement | NARROWED. For `ex = 2` the original's acceptance of either presence, and its skipping of row fields when the row is absent, matches `rec_exists_ok` and `rec_fields_ok` (`contract.rs:324-333`). Two real gaps remain: a missing row observation was scored as a violation instead of unknown, and the rendered text "required unless Q alternative; may be missing" does not record which open question produced it. No case in the final `cases.json` uses `ex = 2` (the provisional Q-06b makes every started row required) | controls "qualified row (ex=2)"; vocabulary scan of `cases.json` |
| R02c aggregation by one boolean | CONFIRMED. `native_matches_b1_l1_judgement` (`evaluate_replay.py:291`) compares only "violates or not". A control whose observation fails `L1-NAMESPACE` instead of the model's `H-COMMITTED[top]` is reported as a match | control "clause identity" |
| R02d individual failed clauses | Closed by controls for each clause family | control family C |

## Historical conclusions after re-evaluation

The corrected evaluator was run on all seven saved replay runs against the final `cases.json` (`10410864…`). In the five final runs, none of the 59 verdicts changed, and in every case the failed clauses equal the model's own `violated_clauses`. The weaknesses above could have accepted wrong observations, but the observations IPM actually produced did not hit them. In the superseded first attempt, B1-C19, B1-C20 and B1-C40 change to INDETERMINATE because `LastUpdated` was not yet observed. The original had reported B1-C19 and B1-C20 as Committed violations on that missing evidence.

## R04–R12

| Finding | Claim checked | Source checked | Result |
|---|---|---|---|
| R04 | history consumers are labeled unmodeled; the source owners are `GetHistory`, `DeleteHistory`, `AutoPurgeOldHistory`, `GlobalRecords` and `Main.History` | original `Verus_verification/coverage-map.md` rows `history.read*`, `history.delete*`, `history.retention`; `src/cls/IPM/General/History.cls:26-317`; `src/cls/IPM/Main.cls:4127-4176` | CONFIRMED. The original model has no reader command. `theorem_r2` covers edits of finalized rows only |
| R05 | one global transaction mode | `crates/ipm-load-history/src/model.rs` `lnm_begin` and `etx` read `c.txm` for every activation; dependencies are leaves (`lnm_deps`) | CONFIRMED. The product resolves the mode per activation (`src/cls/IPM/Utils/Module.cls:1165-1172`) |
| R05 | Q-22 and Q-23 affect different guarantees | ontology `transaction.flag-sources.dependency-defaults` and `.dependency-no-transaction` | CONFIRMED. Q-22 decides whether a dependency's changes may survive the parent's rollback (namespace and record survival). Q-23 decides whether the parent's transaction is suppressed (namespace) and what `Committed` a rolled-back dependency reads |
| R06 | `l1_alts_kill` returns `any_alt` for non-refusal opt-out process ends; developer-mode ends admit broad alternatives | `contract.rs:184-195` | CONFIRMED. `c.txm != 0` gives `any_alt`. Developer mode gives kinds 5 and 6, which accept a dependency installed while the parent's manifest is absent (`Verus_verification/followup/results/20261114T002830Z-final-prefix-cases`) |
| R06 | the repair relies on write ordering beyond the R-01 probe | `B/model-notes.md` section 5, `premises.md` R-01, N-P20 sub-cases | CONFIRMED and sharpened. Native N-P21 shows that a rollback does not undo a journaled write after a later unjournaled write to the same node (P-14). With P-06b this opens a Terminate window in repair 103 (`work/followup-findings.md` L-12). The direction of the hazard is opposite to the one the original text warned about |
| R07 | resources, phases and writes are folded; one foreground action | `model.rs:52-64`, `:91-110` | CONFIRMED. Inventory in `Verus_verification/followup/PROJECTIONS.md` |
| R08 | A inventory breadth versus formal depth | `A/intent-inventory.md`, `A/operation-inventory.md`, both coverage maps | CONFIRMED. No per-row disposition existed for the 163 partial or unestablished coverage rows. Ledger in `scope-ledger.md` |
| R09 | OBS-H11 and OBS-L16 have no entry in `bugs_issues.md` | `work/id-registry-summary.md` section 2 (built from the sources) | CONFIRMED. 18 of the 34 OBS rows are not mentioned in the original `bugs_issues.md`, and OBS-L3, OBS-L8 and OBS-H11 have no disposition anywhere |
| R10 | the introduction says none was modeled while S2 overlaps variant 103; S1 claims Q-17 moot; S5/S6 lack checks; S4 lacks a pair | original `design_simplification.md` (`git show ae619795:...`) | CONFIRMED on each point |
| R11 | no uncertainty file; the handoff collapses eleven Q IDs into one row | `handoff.md:55-67` | CONFIRMED. The collapsed row holds Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19 and Q-20. Q-06 appears in no handoff row |
| R12 | the round-three repairs were not independently re-reviewed | `handoff.md:134`, `uplift-plan.md` | CONFIRMED |
| R12 | the round-three report and the handoff differ on what was independently reproduced | `C/round-3/review.md:61,69,128`, `C/round-3/dispositions.md:3`, `handoff.md:128` | CONFIRMED. The round-three reviewer reproduced 260/0 on the pre-repair sources and the `d3334b16` replay verdicts. The handoff says that reviewer reproduced "263/0", which is the post-repair result that only the orchestrator ran |
| R12 (added check) | "nine product suites pass" | `B/native-report.md` section 3, suite `verdict.json` files | NARROWED. Eight of nine invocations PASS (61 active methods, 1,191 assertions). DependencyResolution had no effective test (its only method skipped though IPM printed "1 passed"), and InstallModule skipped one method by design. Analogous headline gaps: the handoff's "P-01..P-11 confirmed" omits P-13, and its "five with modifications" for round 1 is three in the dispositions |
