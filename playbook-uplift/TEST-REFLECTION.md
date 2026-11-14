# Test reflection

What the proofs and model checks do not cover, and the test that covers (or would cover) each gap. Rewritten
2026-11-13 after the last full rerun; the open items are collected in `QUESTIONS.md` Part B.

## A. Run here, on IRIS

| What | Result |
|---|---|
| Environment probes (`Uplift.Probe.Env`, mapping and sync environment probes) | Model differentials 0 mismatches; every environment mutant contradicted |
| IPM load/history scenarios (`Uplift.Probe.Scenario`, `Extra`, crash scripts), repeated on IRIS 2025.3 | 47 replayed through the fine L2, 0 mismatches |
| Candidate native checks for modules 03–09 (`validation/iris/{inst,repo,nsp}`) | 63 of 65 run; 2 not checkable (mirror; patched document processor, later done with probe patch p8b) |
| Native gap lanes n1 and n2 (`validation/native-gaps-n1.md`, `-n2.md`) | Restarts (`docker restart`, `iris force`), history delete of a running record, phase-level Invoke failures, archive and URL loads, halts inside update steps, self-uninstall and full cleanup, probe patches p8a/p8b, dependency chains and upgrades, IPM's whole suite, the ZPM 0.7.4 upgrade path |
| Write sets (`Uplift.Probe.WriteSet`) | 46 commands, every global in the namespace's databases, IRISSYS, IRISTEMP, IRISSECURITY, IRISLOCALDATA, IRISAUDIT |
| IPM unit suite | 33 classes, 152 of 152 methods pass |
| IPM integration suite, one class per call | 52 classes: 49 pass, 3 fail (classified below) |
| IPM integration suite in CI form (`verify zpm -v -only`) | 263 methods, 262 pass; the failure is F-RES-7 |
| `Test.PM.Migration` on the 0.7.4 upgrade path | 5 methods, 24 assertions pass |
| Totals in `evidence/` | 355 IRIS observation records; 113 IPM test records (109 pass, 4 fail) |

The four failing test records and their classification (`native-gaps-n1.md`):
- `ProcessPythonWheel`: product defect F-RES-7 (tar names of 100+ characters; ARM64 wheel name is exactly 100).
- `Module` and `Module(verbose,no-quiet)`: test-order dependency in the test (needs an interoperability-enabled
  namespace, which only `Health` sets up); passes in the full run.
- `TestOutputFormat`: fails only when this harness nests `-quiet`; exposes F-RES-8. Passes with `-v`.

## B. Negative controls

- Verus: 195 controls across the 8 proof files (implementation, premise-drop and L1-vacuity mutants, plus the
  reviewers' mutants), each killed in the expected lemma, located on the primary error line.
- Explorers: code mutants each rejected by an L1 rule; drop-premise explorations fail as expected.
- Environment model: every mutant contradicted by an IRIS probe.
- Ontology gate self-test: injected violations each caught, compared against the clean baseline (parent, grade,
  definition order, missing cell, P with F, premise not named, and more).
- Semantics gate: refuses unclassified globals, unlabelled failed runs, and verdicts without matching evidence.

## C. Premises and bounds, each mapped to a test

| Premise or bound | Covering test | Status |
|---|---|---|
| P1 no caller transaction | `ipm-load-inside-caller-transaction-rolled-back` | Run; defect F-HIST-3 |
| P2 a crash rolls back | `ipm-halt-txn`, `ipm-halt-txn-operator-commits` | Run; both branches; F-HIST-4 |
| P3 commit window | probe patch p8a (`nc-gap-P8-commit-window-after-commit`, control) | Run with patch; F-HIST-7 |
| PC developer crash window | `ipm-halt-dev-crash-in-dependency` | Run; F-HIST-2 |
| PD dependency rollback | `ipm-dep-parent-fails`, `nc-gap-P9-*-parent-fails-txn` | Run; F-HIST-1 |
| PI instance crash | `nc-gap-P1-*` (docker restart, `iris force`) | Run; F-HIST-8. `iris force` for txn mode only; graceful stop not tried |
| PM nothing fails after the final commit | `ipm-post-commit-message-fails`, `r4-hist-post-commit-throw-r4h-msg2-*` | **False**: F-LOAD-4 |
| Bound: one dependency level, absent pre-state | proofs lifted to any number of dependencies and any prior state; `nc-gap-P9-*` chains and upgrades | Explorer still bounded at 0–3 dependencies |
| Archive and URL loads | `nc-gap-P4-*`, `nc-gap-P5-*` | Run; F-LOAD-1..3 |
| Phase record for a failing phase-level Invoke | `nc-gap-P3-*` | Run; F-HIST-9 |
| `history delete` of a running record | `nc-gap-P2-*`, `r4-hist-delete-*` | Run during Activate and Compile, and on dependency rows; intended (DEC-HIST-6) |
| Kill between an update step and its record | `nc-gap-P6-*-halt-in-step`, `r4-inst-u22-*` | Run; the step reruns (at least once, intended) |
| Update lock mutual exclusion | `nc-inst-3-update-lock-busy` | Run for a busy lock; mutual exclusion not proved |

## D. Round 4 runs (resolving decisions) and IPM's CI

- New IRIS runs per research lane (logs `validation/logs/a1-*` to `a4-*`, observations `r4-*`; invalid first runs
  listed in `validation/logs/SUPERSEDED.tsv`): post-commit failure (F-LOAD-4), graceful `iris stop`, `history delete`
  in Compile and of dependency rows, `update -dev` from ORAS, a kill during the step-record save, scoped needs and
  uninstall, full cleanup with a mapped namespace, simulated legacy records reached by `enable -map`, the
  unmap-then-install route, the collation slot of F-NSP-8, IRIS wallet availability.
- IPM's CI at this commit (run 42293577988, 8 IRIS images, `validation/ci/ci-571e04a1-extract.txt`): the migration
  test has 0 skipped assertions in the span checked; `TestPackageWithPythonDeps` passes on amd64 (F-RES-7 is
  ARM64-only); `InstallApplication#TestWSGIApp` and `DependencyResolution#TestEverything` are skipped in every run;
  `InstallModule#TestUninstallDependencyDoesError` is skipped because blocking "does not yet exist".

## E. Not run

- Mirrors (Community Edition refuses mirroring).
- The defects on amd64 or IRIS versions other than 2026.1 (load/history also on 2025.3); IPM's CI covers only its tests.
- A remote (HTTP) registry for F-INST-5; real ZPM 0.7.4 for F-NSP-10.

## F. Weak IPM tests (they pass but assert less than their names)

| Test | Weakness |
|---|---|
| `Update#TestUpdateAfterInterruptedUpdate` | Kills at an uncontrolled point; asserts only that a retry returns OK (F-INST-3) |
| `Update#SingleFailedUpdateTest`, `AssertUpdateStepsExist` | Status only; rows exist, not that steps ran (F-INST-3) |
| `Update#TestUpdateToSameVersionTwiceNoMirror` | Returns early on a mirror member without a skip mark (F-INST-2) |
| `DependencyResolution#TestEverything` | Asserts nothing (F-DEP-6) |
| `LockFile#Test05`, `#Test08` | Swallows exceptions; empty (F-DEP-6) |
| `InstallApplication#TestWSGIApp` | Skips itself on container builds (`$zversion` word count); skipped in all 16 CI runs |
| `Module` | Depends on `Health` running first |
| `PublishModuleWithDeps` L25-L26 | Asserts the blocked-uninstall success of F-UNIN-6 |
