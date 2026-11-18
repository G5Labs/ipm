# Iteration 74 predictions (written 2026-11-18T04:04Z, before reading the deciding code)

Deciding code not yet read: `Main.cls` Install / Update / Uninstall / RunOnePhase / Reload / List / Sync and their
`HistoryTemp` init and finalize calls, `General/AbstractHistory.cls`, `General/History*.cls`, `Utils/Module.cls`
SetCommitted sites, `Storage/Module.cls` Uninstall and ExecutePhases.

Read before writing this file: the ontology page `features/history.g5.md`, census records for `features/history`,
iteration72 handoff, `consolidated/verus/history.rs`, the head and L2 of `playbook-uplift/models/verus/uninstall.rs`,
the header of `update_steps.rs`, `consolidated/iter22/reinstall.rs` and the `ipm-load-history` BINDINGS.md.

## Exposure

These sources already state code facts, so the predictions marked (E) are not blind:

- E1. Census gap of history.actions#2: `InstallInit` (Main.cls:2565-2566) precedes the install name check.
- E2. history.actions#7: update with -PermitDowngrade or of a module that is not installed throws before
  `HistoryTemp.UpdateInit` (Main.cls:4415-4423, init at 4432).
- E3. history.actions#6 / reinstall.rs: reinstall of a name that is not installed throws before any row
  (Main.cls:2685-2687).
- E4. history.committed.marking#4, review-71 A3: uninstall never calls SetCommitted. `Finalize` sets
  `Committed = Committed || ISOK(status)` (AbstractHistory.cls:131).
- E5. marking#3: SetCommitted sites in LoadNewModule are Utils/Module.cls:1227, 1313, 1350.
- E6. uninstall.rs `storage_uninstall` pushes a failed row with no module for a name that is not found, and a row
  with `ok = Clean succeeded` after Clean for a found module. The model writes rows only at the end.
  `dep_step` pushes rows for dependencies cleaned under `-r`. `clean_cmd` is commented "no log" but reuses
  `exec_clean`, whose dep_step pushes rows (iteration70 finding 17).
- E7. ipm-load-history BINDINGS: Load creates the IRISTEMP row and its History twin first (LoadInit), refusals
  skip to Finalize; the History row is persisted in `%OnClose`.

## Predictions (reader: the `%IPM.General.History` row, as `history find` / `details` show it)

Fields: Action, Package, Version, Success, Committed, TimeEnd, Phases.

| # | Case | Neighbour | Predicted rows / fields | Expected difference | Source |
|---|---|---|---|---|---|
| P1 | `install m` of a module not installed, found in a repo, succeeds | P2 | 1 row: Action install, Package m, Version found, Success 1, Committed 1, TimeEnd set, Phases Initialize..Activate (5, all OK) | — | ontology history.phases B1, committed |
| P2 | `install m` when m is already installed at that version | P1 | 1 row, Action install, Success 1. IPM may skip the load ("already installed"); then Phases empty and Committed 1 only through the ISOK term. If it reinstalls, same as P1 | Phases empty vs five | ontology silent; guess |
| P3 | `install` of a bad / unknown name (not in any repo) | P1 | 1 row (E1): Action install, Package = typed name (or empty if refused before SetName), Version `0.0.0-0`, Success error, Committed 0, TimeEnd set, no phases | Row count 1 in both; Success/Committed differ; Phases 0 vs 5 | E1, ontology actions B1 |
| P3b | `install` with no name | P1 | 1 row (E1), Package empty, Success error, Committed 0 | same as P3 | E1 |
| P4 | `update m` whose ApplyUpdateSteps fails after the LoadNewModule commit | P5 | 1 row, Action update, Success error, Committed 1 (set at the commit; E5), Phases Initialize..Activate OK + ApplyUpdateSteps error | Success differs, Committed both 1, last phase status differs | ontology committed#3 gap, phases B1 |
| P5 | `update m` success | P4 | 1 row Action update, Success 1, Committed 1, Phases 5 + ApplyUpdateSteps OK (6 entries) | | ontology phases B1 |
| P6 | `update m` where m is not installed | P5 | 0 rows (E2): throws before UpdateInit | row count 0 vs 1 | E2 (ontology record WA says "record them": as-built diverges) |
| P7 | `uninstall m` where m is not installed | P8 | 1 row Action uninstall, Package m, Success error, Committed 0, Phases empty | count 1 both; Success/Committed differ | E6, ontology actions B1 |
| P8 | `uninstall m` success | P7, P9 | 1 row Action uninstall, Package m, Version installed, Success 1, Committed 1 (ISOK term, E4), Phases [Clean OK] | | ontology phases B1 |
| P9 | `uninstall m` that fails mid-Clean after deleting a resource | P8 | 1 row Success error, Committed 0 (E4) although a resource is gone, Phases [Clean error] | Committed 0 vs 1 with kept change | E4, ontology committed.uninstall |
| P10 | `uninstall` with no name | P7 | 0 rows: Main.cls refuses before Storage.Module:Uninstall creates the log. The ontology working assumption says 1 row | 0 vs 1 | guess from E6 (log is created in Storage/Module Uninstall) |
| P11 | `m clean` (m with a dependency d) | `uninstall m` | 0 rows for m. For d: I predict 0 rows, because the History log of Uninstall belongs to Storage.Module:Uninstall, not to %Clean; the HRow pushes in uninstall.rs dep_step model something else (or a row written only when an uninstall log exists). Low confidence | 0 vs 1 | ontology actions B3; finding 17 open |
| P12 | `reload m` | load | 0 rows | 0 vs 1 | ontology actions B3 |
| P13 | `list` | load | 0 rows | 0 vs 1 | ontology actions B3 |
| P14 | install whose Compile fails | P1 | 1 row Success error, Committed 0 (non-dev rollback), Phases Initialize, Reload, Validate (OK), Compile (error, TimeEnd set); no Activate | 4 entries vs 5 | ontology phases B2 |
| P15 | Row creation timing for install/update/uninstall | P3/P9 | Created at start with Success 0, no TimeEnd, filled at end (TimeEnd set) | | ontology record |
| P16 | `reinstall m` with m not installed | reinstall success | 0 rows (E3) | | E3 |

Uninstall row placement (P15 for uninstall) is the least certain: uninstall.rs writes rows only at the end, the
ontology says at start. I predict the product creates the row at start in Storage.Module:Uninstall (UninstallInit)
and the model's end-only placement is a simplification.

## Results (appended after reading the code and the exploration runs, 2026-11-18T04:30Z)

Code read: Main.cls Install/Reinstall/Uninstall/RunOnePhase/Update/Load, AbstractHistory.cls, HistoryTemp.cls,
Storage/Module.cls ExecutePhases and Uninstall, Lifecycle/Base.cls GetCompletePhases and %Clean, Lifecycle/Module.cls
%Clean, Utils/Module.cls LoadModuleReference/LoadModuleFromDirectory/LoadNewModule/LoadDependencies. Observations
from the scratch container `ipmcov20261117-iter74-explore`, then the committed fresh run (`native/<stamp>`).

| # | Verdict | Observed |
|---|---|---|
| P1 | held | 1 row, install, Success, Committed, five phases each ended OK |
| P2 | missed | No skip: a second `install h74a` reinstalls and writes the same row as P1 (five phases) |
| P3 | held | 1 row, Package `h74nosuch`, version `0.0.0-0`, error, Committed 0, no phases |
| P3b | missed at the CLI, held at the API | `install` with no name is refused by the command parser (`module` is required), so no row. Calling `##class(%IPM.Main).Install` directly with an empty name writes a row with an empty Package |
| P4 | missed | With transactions, an update failing in ApplyUpdateSteps is rolled back: Committed 0, the module stays at 1.0.0 (the census gap's contrast "reads Committed=1 with the steps kept" was wrong for the default mode). With `-DNoTransaction=1` the same failure reads Committed 1 and 1.1.0 is kept. Phases: six, ApplyUpdateSteps ended with an error |
| P5 | held | Six phases, last ApplyUpdateSteps |
| P6 | held | No row for an update of a module that is not installed, nor for `-DPermitDowngrade=1` |
| P7 | held | 1 row, uninstall, Package typed, placeholder version, error, Committed 0, no phases |
| P8 | held | [Clean OK], Committed 1 |
| P9 | held | Clean fails in a resource processor after the FileCopy target was deleted: error, Committed 0, Clean ended with the error, files gone, class and module record still there |
| P10 | held | No row |
| P11 | missed | `h74m clean` writes one row: `uninstall h74d`, Success, Committed, [Clean OK], and h74d's record is deleted. Nothing for h74m |
| P12, P13 | held | `list`, `h74a reload`, `h74a compile`, `reload h74a`, `sync`: no row |
| P14 | held | Initialize, Reload, Validate OK, Compile ended with an error, no Activate, Committed 0 |
| P15 | held | While running, the permanent row exists with Success 0, no end time and not finalized (install, update, uninstall). Not predicted: it lists no phases while running; only the IRISTEMP copy does (5, 6 and 1 phases at the observation points). This matches the already refuted history.record#6 |
| P16 | held | No row |

Not predicted by either source:
- `<module> test` writes a `load` row for a test-scoped dependency it installs (t1). The history.actions metadata already
  noted this ("Observed: ... recorded as `load`"), but the statement says phase commands are not recorded.
- `uninstall h74y -r` whose dependency's undo hook fails: h74y's row is created first (lower ID) and h74z's second,
  both failed and uncommitted, while h74y's class is already deleted (n5). uninstall.rs appends the top row last.
- A downgrade refusal records the requested version (from the search result), Committed 0 even with
  `-DNoTransaction=1`, because the guard runs before the no-transaction commit mark (i6, i7).
