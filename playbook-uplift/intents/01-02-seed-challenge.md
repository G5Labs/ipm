# Seed challenge: package loading and transaction history

Seed: `docs/ipm-uplift/index.md`, "Worked example — what does a failed load leave behind?". Each row was re-derived
from source and then checked on IRIS 2026.1 (build 234U) with IPM 571e04a1 installed from source. Observation names
refer to `evidence/iris-observations.json`; scenario code is `validation/iris/Uplift/Probe/Scenario.cls`.

## Seed rows

| Seed claim | Verdict | Evidence |
|---|---|---|
| A developer-mode load refused before its first commit rolls back; `Committed = 0`. | **Confirmed.** The native check also read the installed module back: 2.0.0 stays. | O:ipm-dev-downgrade-refused; T:Test.PM.Integration.History (TestDevModeRollbackNotCommitted); P:witness_refusal_is_clean |
| A developer-mode load commits the manifest, then Activate fails; `Committed = 1`. | **Confirmed for Activate failure, contradicted for a crash in the same window.** If the process ends while the dependencies of that load are loading, the manifest stays but the record says `Committed = 0`, package empty (F-HIST-2). A missing dependency after the commit also gives `Committed = 1` (refinement: the window is "after the first commit", not "Activate"). | O:ipm-dev-activate-fails; O:ipm-dev-missing-dependency; O:ipm-halt-dev-crash-in-dependency; P:witness_dev_crash_commit_flag |
| `-DNoTransaction=1` reaches the write phase, then Activate fails; `Committed = 1`. Early refusal is separate. | **Confirmed**, also for `-DNoJournal=1` and for a crash during Activate (row has no end time). The durability read shows the module record and compiled class kept. | O:ipm-notxn-activate-fails; O:ipm-nojournal-activate-fails; O:ipm-halt-notxn; O:ipm-terminate-notxn |
| "A native check of durability should also read the affected installed module or resource after the failure." | **Done** for every scenario: module records, developer-mode flag and compiled classes are read back. | `Observe` in Scenario.cls |
| For uninstall, the flag reflects success; extending the load rule would invent a guarantee. | **Confirmed by inspection** (AbstractHistory.cls#L43; module 05). Not exercised natively here. | I |

## Beyond the seed

| New claim | Status |
|---|---|
| The history design rests on IRIS facts: IRISTEMP is not rolled back; `%OnClose` runs at method return and, on a crash, before the rollback; re-saving a rolled-back object re-creates it. | Observed (9 environment probes) and modelled; 9 environment mutants each contradicted by a probe. |
| Dependency loads write their own records, which vanish when the parent rolls back. | Observed (F-HIST-1); contradicts the class comment's intent. DEC-HIST-1. |
| A load inside a caller transaction loses its record if the caller rolls back. | Observed (F-HIST-3). DEC-HIST-4. |
| An interactive operator can commit a half-done load at the crash prompt; history says `Committed = 0`. | Observed (F-HIST-4). DEC-LOAD-1. |
| History auto-purge never deletes (quoted date). | Observed with cause (F-HIST-6). |
| A transactional load is atomic, including its dependencies and compiled classes, for failure and crash (rollback). | Proved (L2 refines L1 in transactional mode under the built-in premises P1–P3 only; PC and PD are not needed), model-checked, observed. |
| A plain load over a developer-mode install keeps developer mode. | Observed. DEC-LOAD-2. |
| Finalized records resist SQL and object edits, and can still be deleted. | Observed on IPM's own class. |

## Native checks run (beyond IPM's own tests)

38 observations: 9 IRIS environment probes and 29 IPM scenarios, including 9 crash scenarios (halt in Activate in each
mode, terminate from another process in each mode, halt inside a dependency in two modes, operator commit at the
prompt). Logs: `validation/logs/2*.log` and `3*.log`.
