# Product defects and issues: IPM load and history

Defects and open issues found by the IPM Verus uplift at product revision `571e04a1`. Product repairs were not authorized, so each entry carries a repair proposal and no fix. Each proposal names the obligation, the affected writers and readers, the behavior to preserve and a check that distinguishes a fixed product from the current one.

Status values:
- **confirmed**: a governing source requires otherwise, and a native reproduction through the public command shows the violation.
- **confirmed, decision pending**: the violation is confirmed against the best-supported provisional answer of an open question.
- **suspected**: from code reading only.
- **withdrawn**: a governing source documents the behavior as intended.

`VV` = `.g5/Verus_verification`. Run IDs are directories under `VV/results/native/`.

DRAFT: updated after stage C round 1. Final native verdicts are added at handoff.

## Confirmed

### D1: A dependency's history record is lost when its parent's transaction rolls back
- Obligation: `history.survives-rollback` and `history.dependencies` (issue #366: "'install' (including as a dependency)", "if install rolls back we still need the history").
- Path: `load <dir>` → `LoadNewModule` `tstart` (`Utils/Module.cls:1195`) → `LoadDependencies` → `LoadModuleReference` creates the dependency's log inside the parent's transaction (`:65`, `:67` → `HistoryTemp.cls:21` twin insert). Its `%OnClose` persist (`HistoryTemp.cls:62-73`) also runs inside that transaction and deletes the IRISTEMP row. The parent's `trollback 1` loop (`Utils/Module.cls:1358-1362`) then removes the twin. The same happens in `-dev` when a dependency fails inside the second transaction.
- Evidence: B2 cases A1, A2, A3 (`20261113T182759Z-cases-a`, replay `20261113T184230Z-replay-fast`). Reproduction: `VV/native/repro/dep-history-lost.cos` → `20261113T184617Z-repro-dep-history-lost`. The ID is allocated and no row persists.
- Formal: model violations V1 and V2 (`case_c31_dep_record_lost`, `case_c33_dev_dep_record_lost`, `case_c45_*`, explorer `asbuilt-r3-dep`). Terminate variants are confirmed natively by replay `20261113T194122Z-replay-terminate-b`.
- Repair proposal: write a dependency's durable row outside the parent's transaction, and set its `Committed` from whether the dependency's writes were actually kept, not from the parent's final status. For example, the parent log keeps the IRISTEMP IDs of its dependency logs. After its own final `tcommit` or rollback at the entry `$TLEVEL`, the top-level `LoadNewModule` persists each nested log:
  - `Committed=0` when the parent's rollback undid the dependency.
  - `Committed=1` when the dependency's writes were committed and kept. In developer mode, dependencies stay when the parent fails after the second commit (case A5, B1-C38).
  - Writers: `HistoryTemp.Init`, `PersistToTwin`, `%OnClose`, `Utils.Module:LoadModuleReference`, `LoadNewModule` error path.
  - Readers: `history find` and `details`, `%IPM_General.History`, `Records`.
  - Preserve: a top-level failure reads `Committed=0`. A committed dependency of a successful parent reads `Committed=1`. ID order follows start order.
  - Distinguishing check: A1 expects one dependency row with `Committed=0`. A3 expects a dependency row with an error `Success`. A5/C38 expect the dependency row with `Committed=1`. A0 keeps two committed rows.
  - The model's repair variant is reported in `B/model-notes.md`.

### D2: One filter column with `_` silently drops every later filter, so a filtered `history delete` empties the namespace
- Obligation: `history.read.filter` ("Versions are filtered by part", "Filters work the same way for `find` and `delete`"); wiki/09 L112 advertises `Version_Major`. `history.delete` deletes only matching records.
- Path: `history delete -DVersion_Major=99` → `Main:History`. The delete gate passes because `filter` has subscripts (`Main.cls:4165`). Then `DeleteHistory` → `ConstructSQLWhere` `quit`s at the first name failing `^[a-zA-Z][a-zA-Z0-9]*$` (`General/History.cls:103-105`), leaving `WHERE 1 = 1`.
- Evidence: B2 E1, E2, E4, E8 (`20261113T182845Z-cases-ef`, replay). Reproductions: `VV/native/repro/history-delete-underscore.cos` → `20261113T184623Z-repro-history-delete-underscore`, which deleted all rows and backs them up and restores them. `repro/history-filter-widened.cos` → `20261113T184621Z-…`.
- Repair proposal: accept `_` in the name check (`^[a-zA-Z][a-zA-Z0-9_]*$`) and check names against the table's columns. Reject any invalid name with an error instead of `quit`.
  - Writers and readers: `ConstructSQLWhere`, `GetHistory`, `DeleteHistory`, `DeleteHistoryGlobally`, `Main:History`.
  - Preserve: E5 (no filter and no `-confirm` is refused). E6 (a single-package delete removes one row). The SQL-injection guard.
  - Distinguishing check: E4 deletes only rows with major version 99 (none). E2 lists 0 rows.

### D3: The retention purge deletes nothing and still stamps the day's purge as done
- Obligation: `history.retention` ("`config set HistoryRetain <days>` deletes records older than that many days").
- Path: `Shell` → `AutoPurgeOldHistory` (`Main.cls:862`). The filter value `"<='" _ cutoff _ "'"` (`General/History.cls:173`) is bound with its quotes after `<=` is stripped. Then `SetHistoryLastPurge` (`:176`) runs anyway.
- Evidence: B2 F1 (`20261113T182845Z-cases-ef`, replay). Reproduction: `repro/autopurge-noop.cos` → `20261113T184626Z-repro-autopurge-noop`. An adapter control without the quotes deletes exactly the two old rows.
- Repair proposal: `set filter("TimeStart") = "<=" _ cutoffDate`, and stamp `lastPurge` only after a successful delete.
  - Preserve: retention 0 (default) deletes nothing. At most one purge per day.
  - Distinguishing check: F1 deletes both 40-day rows and keeps the 1-day row.
  - Related: `history.retention` metadata notes the purge runs from `Shell` before every command and its error blocks the command (A2 OBS-H12, suspected).

### D4: `-dev`, `-DNoTransaction=1` and `-DNoJournal=1` loads refused by unmet system requirements leave the module installed and log `Committed=1` (decision pending: Q-17)
- Obligation: `load.refusal.requirements` (wiki/03 L260-261: requirements "must be validated before module installation"). The timing exception is open as `load.refusal.requirements.timing` (Q-17). Its provisional answer is that a refusal writes no module data.
- Path: `load -dev <dir>` → manifest `OBJ.Load` and module save, then the dev commit (`Utils/Module.cls:1309-1314`), then `CheckSystemRequirements` (`:1321`). Without a transaction, `SetCommitted` runs at `:1227` before any write.
- Evidence: B2 B2, B3, B4 (`20261113T182820Z-cases-b`, replay). Reproduction: `repro/sysreq-dev-manifest-left.cos` → `20261113T184630Z-repro-sysreq-dev-manifest-left` (`list-installed` shows the module).
- Repair proposal: check the requirements on the manifest object read by `GetModuleObjectFromPath` (`Utils/Module.cls:1164`), before the transaction starts and before `SetCommitted`.
  - Preserve: B0 (met requirements in dev mode install and commit). B1 (non-dev refusal rolls back). Dev mode keeps the entries of an installable module that fails later (#363).
  - Distinguishing check: B2, B3 and B4 leave no module record and log `Committed=0`.

### D5: `config delete HistoryRetain` restores the first value ever set, not the default 0
- Obligation: `history.retention` ("`0`, the default, disables it"); `settings` (`config delete` restores the default).
- Path: `config delete` → `ResetToDefault` reads `^IPM.settings(key,"default")` (`Repo/UniversalSettings.cls:101`). `SetValue` seeds that node with the first value set (`:178-179`), and `HistoryRetain` has no install-time default.
- Evidence: B2 F0. Reproduction: `repro/config-delete-retain.cos` → `20261113T184628Z-repro-config-delete-retain`. Also seen after the product's own `TestAutoPurgeOldHistory` (`20261113T182514Z-suite-test-CLI`, `HistoryRetain=30` afterwards).
- Repair proposal: give `HistoryRetain` an explicit factory default of 0 in `ResetToDefault`, as is done for `ModuleRoot`, or seed the default node at install time. Do not seed `"default"` from a user's first `SetValue`.
  - Preserve: settings that do seed their default at install.
  - Distinguishing check: F0 returns 0.

### D6: An Invoke on the Activate phase that returns an error leaves the phase open in a finalized record (decision pending: Q-14)
- Obligation: `history.phases` (wiki/09 L135-161: a phase with no elapsed time "never finished"), and `history.phases.returned-errors` (Q-14, provisional: every started phase ends with its status).
- Path: `Storage/Module.cls:669-671` `quit:$$$ISERR(tSC)` leaves the try block, and `$$$ThrowOnError(tSC)` (`:704`) throws before `FinalizePhase` (`:710`). The catch (`:695`) runs only for thrown errors.
- Evidence: reproduction `repro/activate-invoke-phase-open.cos` → B2 run listed in `index.jsonl` (`repro-activate-invoke-phase-open`). Invokes on the default Configure phase finalize Activate correctly (A1, A5).
- Repair proposal: call `pLog.FinalizePhase(tOnePhase, tSC)` whenever the phase ends with an error, whether returned or thrown, before `$$$ThrowOnError`.
  - Preserve: phase timings for successful phases.
  - Distinguishing check: the reproduction shows Activate with an end time and the Invoke's error.

### D9: A refused `load` records an empty package although the manifest named the module
- Obligation: `history.fields` `Package` (wiki/09 L69: empty only "if the action failed before a name was known"). This is a necessary implication (b), per round 1 disposition R1-ONT-03.
- Path: `Main.cls:2420` `LoadInit("")`. The name is filled in only by `pLog.SetName` (`Utils/Module.cls:1293`), which runs after the downgrade and update-required refusals (`:1213-1222`).
- Evidence: B2 g3, g6, g7 and R4 (`20261113T182832Z-cases-dg`, `20261113T184444Z-cases-hr`). `history find -DPackage=update-simple` shows 14 of the 20 rows whose command names the module.
- Repair proposal: call `pLog.SetName` and `SetVersion` right after `GetModuleObjectFromPath` succeeds (`Utils/Module.cls:1164-1166`).
  - Preserve: a malformed manifest still records an empty package (`TestDevMode`).
  - Distinguishing check: g3 records `Package=update-simple`.

### D10: The durable record of an action that never ended keeps only its start values: no package and no phases
- Obligation: `history.record` (I: "after it stops without ending, the log lists its record with `Success` 0, no end time and the phases it has started"). Also `history.fields` `Package`: the name was known and recorded in the in-flight log.
- Path: every setter after `Init` updates only the IRISTEMP row (`AbstractHistory.cls:86-173`). The twin is written at `Init` and at `%OnClose`. After SIGKILL no `%OnClose` runs. After `$SYSTEM.Process.Terminate` of a non-dev load, `%OnClose` runs before IRIS rolls the transaction back, so the twin update is undone and the IRISTEMP row is deleted (N-P10, C1).
- Evidence: B2 C1 (`20261113T184230Z-replay-fast`), C3 (`20261113T182927Z-cases-c-sigkill`), C4R (`20261113T183827Z-cases-c4r`). The durable row has `Package=""` and no phases after four phases ended and Activate started. Model: `kill_rec` after round 1 (B1).
- Repair proposal: at each phase boundary and commit at `$TLEVEL` 0, copy the name, version, `Committed` and phases to the durable row. Inside a transaction, keep them in IRISTEMP and copy them at the action's end. This shares the mechanism of D1, D12 and the Q-08 deviation (`design_simplification.md` S2).
  - Preserve: finalized rows stay immutable. `Success` stays 0 and `TimeEnd` empty for an unfinished record.
  - Distinguishing check: C4R and C3 list Initialize..Compile ended and Activate open, with the package set.

### D11: A manifest refused by validation under `-DNoJournal=1` keeps the module document
- Obligation: `load.refusal` (I: a refusal writes no module data, in every mode), `load.refusal.manifest`.
- Path: the no-transaction marker and `SetCommitted` (`Utils/Module.cls:1226-1228`), then `OBJ.Load` of the manifest (`:1251`). `StudioDocument/Module.cls` `ImportStream` saves the `.ZPM` document and rolls back its own nested transaction when validation refuses the module, but journaling is off, so the save is not undone (P-09, N-P07).
- Evidence: model case B1-C10. Native replay `20261113T193952Z-replay-return-b`: status error, `zpm_document_version=2.0.0` (before: none), `Committed=1`, no package. B1-C09 (`-DNoTransaction=1`) keeps nothing but shares D12 and D9.
- Repair proposal: validate the manifest before any write (`design_simplification.md` S1), or delete the saved document on refusal when no transaction is active.
  - Preserve: valid NoJournal loads (B4 neighbour).
  - Distinguishing check: B1-C10 leaves no `.ZPM` document and records `Committed=0` with the package set.

### D12: A NoTransaction or NoJournal load refused after the no-transaction marker reads `Committed=1` with nothing kept
- Obligation: `history.committed` (I: a refusal before anything is written is not committed, in any mode).
- Path: `SetCommitted` runs at `Utils/Module.cls:1226-1228`, before the manifest import (`:1251`), so any refusal after that point leaves `Committed=1`.
- Evidence: model V4 (`case_c08_committed_without_change`, `case_c09_committed_without_change`, explorer `asbuilt-commit-exact`). Native replay `20261113T193952Z-replay-return-b` (B1-C08: "No module definition found.", nothing written, `Committed=1`).
- Repair proposal: mark `Committed` only when the first write is kept, for example after `OBJ.Load` returns OK, or after validation (S1).
  - Preserve: B3 and B4 (requirements refusals that do keep the manifest read `Committed=1`, until D4 is decided). A0.
  - Distinguishing check: B1-C08 reads `Committed=0`.

### D13: A load that committed reports failure (`<INVALID OREF>`) when the temporary `.modules` repository could not be created
- Obligation: `load.outcome` (I: a failed load in normal mode keeps no module data, so the returned status and the namespace must agree).
- Path: `TempLocalRepoManager:%OnNew` fails and `%New` returns `""` (`General/TempLocalRepoManager.cls:63-68`). After `LoadNewModule` has committed, `tmpRepoMgr.CleanUp()` (`Main.cls:2529`) throws `<INVALID OREF>`.
- Evidence: model V6 (`case_c29_error_after_commit`). Native replay `20261113T193952Z-replay-return-b` (B1-C29: `ERROR #5002: <INVALID OREF>LoadInternal+83^%IPM.Main.1`, module and classes installed, `Committed=1`).
- Repair proposal: check the repository manager before use, and report its creation error before the transaction starts. Treat cleanup errors after the commit as warnings.
  - Preserve: the normal-mode rollback of real failures.
  - Distinguishing check: B1-C29 either refuses before writing or succeeds with a warning.
  - If maintainers consider an error after the final commit acceptable, record that as a Q under `load.outcome`.

## Deviations under open questions (decision pending)

| ID | Behavior | Governing question and provisional answer | Evidence |
|---|---|---|---|
| Q-05 | A `Shell("load …")` run inside a caller's transaction that the caller rolls back leaves no record, although the in-flight row read `Committed=1` and `Finalized=1` | `history.committed.nested` (record survives under a caller transaction, provisional, #366) | B2 D1, `repro/caller-tx-history-lost.cos` → `20261113T184632Z-…` |
| Q-08 | A SIGKILLed `-dev` load keeps its manifest and classes, but the durable record reads `Committed=0` (its missing package and phases are D10) | `history.record.abnormal-end` (provisional after round 1: `Committed` reflects kept changes) and `history.record` (started phases listed) | B2 C4R `20261113T183827Z-cases-c4r` |
| new Q (round 1) | `$SYSTEM.Process.Terminate` of a `-dev` load finalizes (locks) a record with no end time and an open phase | Q under `history.locked` (provisional: an unfinished record is not finalized) | B2 C2 `20261113T182903Z-cases-c-terminate` |

## Issues without a governing requirement

### I1 (issue, formerly D8): An unfinished phase is shown as `ERROR #00: (no error description)`
- Obligation: none beyond usability. wiki/09 L161 says how to read an open phase. Reclassified by round 1 disposition R1-MOD-12.
- Path: `General/History.cls:420` applies `GetErrorText` to the placeholder status 0.
- Evidence: B2 R2 (`20261113T184444Z-cases-hr`).
- Repair proposal: print "not finished" when the phase has no end time.
- Distinguishing check: R2 shows "not finished".

## Withdrawn

- D7 (`history details` prints the raw `%Status`): wiki/09 L133 documents it on purpose ("so the ObjectScript stack is visible"). Round 1 disposition R1-ONT-13.

## Suspected, not observed natively

- S-D14: a dependency whose own `<Defaults>` sets `NoJournal` escapes its parent's normal-mode rollback (`Utils/Module.cls:1166-1169`, `:1191-1193`; N-P07). It is then installed with no record, or IRIS raises `<ROLLFAIL>`. Obligation: `load.outcome`, `transaction.flag-sources`. Round 1 R1-MOD-05. Native case queued with B2.

From A2 observations. The listed obligations are in the ontology metadata.
- OBS-L4: `update -dev` from a registry ends in non-dev mode.
- OBS-L5: a dependency newly installed by `update` runs all its update steps.
- OBS-L7: archive installs delete the module's directory before any guard.
- OBS-L9: update steps are skipped silently when the update lock is not acquired in 3 s.
- OBS-L11: the lock file is written before the parent commits.
- OBS-L14: `PermitDowngrade` does not reach the downgrade refusal.
- OBS-L15: `LoadedResource` rows survive a rolled-back load.
- OBS-L17 and OBS-H8: uninstalling a required module reports success.
- OBS-H4: covered by D5.
- OBS-H10: global readers in namespaces without IPM.
- OBS-H12: a purge failure blocks every command.
- OBS-H13: full cleanup kills other namespaces' in-flight temp rows.
- OBS-H15: self-uninstall records success before running.
