# Product defects and issues: IPM load and history

Defects and open issues found by the IPM Verus uplift at product revision `571e04a1`. Product repairs were not authorized, so each entry carries a repair proposal and no fix. Each proposal names the obligation, the affected writers and readers, the behavior to preserve and a check that distinguishes a fixed product from the current one.

Status values:
- **confirmed**: a governing source requires otherwise, and a native reproduction through the public command shows the violation.
- **confirmed, decision pending**: the violation is confirmed against the best-supported provisional answer of an open question.
- **suspected**: from code reading only.
- **withdrawn**: a governing source documents the behavior as intended.

`VV` = `.g5/Verus_verification`. Run IDs are directories under `VV/results/native/`.

DRAFT: formal (B1) cross-references and phase 2 native results are added at handoff.

## Confirmed

### D1: A dependency's history record is lost when its parent's transaction rolls back
- Obligation: `history.survives-rollback` and `history.dependencies` (issue #366: "'install' (including as a dependency)", "if install rolls back we still need the history").
- Path: `load <dir>` → `LoadNewModule` `tstart` (`Utils/Module.cls:1195`) → `LoadDependencies` → `LoadModuleReference` creates the dependency's log inside the parent's transaction (`:65`, `:67` → `HistoryTemp.cls:21` twin insert). Its `%OnClose` persist (`HistoryTemp.cls:62-73`) also runs inside that transaction and deletes the IRISTEMP row. The parent's `trollback 1` loop (`Utils/Module.cls:1358-1362`) then removes the twin. The same happens in `-dev` when a dependency fails inside the second transaction.
- Evidence: B2 cases A1, A2, A3 (`20261113T182759Z-cases-a`, replay `20261113T184230Z-replay-fast`). Reproduction: `VV/native/repro/dep-history-lost.cos` → `20261113T184617Z-repro-dep-history-lost`. The ID is allocated and no row persists.
- Repair proposal: write the durable copy of a nested log only once the top-level action's own commit or rollback has settled. For example, the parent log keeps the IRISTEMP IDs of its dependency logs, and the top-level `LoadNewModule`, after its final `tcommit` or rollback at the entry `$TLEVEL`, persists each nested log with `Committed` set from the parent's outcome.
  - Writers: `HistoryTemp.Init`, `PersistToTwin`, `%OnClose`, `Utils.Module:LoadModuleReference`, `LoadNewModule` error path.
  - Readers: `history find` and `details`, `%IPM_General.History`, `Records`.
  - Preserve: a top-level failure reads `Committed=0`. A committed dependency of a successful parent reads `Committed=1`. ID order follows start order.
  - Distinguishing check: A1 expects one dependency row with `Committed=0`. A3 expects a dependency row with an error `Success`. A0 keeps two committed rows.

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

### D6: An Invoke on the Activate phase that returns an error leaves the phase open in a finalized record
- Obligation: `history.phases` (wiki/09 L135-161: a phase with no elapsed time "never finished"), and `history.phases.returned-errors` (Q-14, provisional: every started phase ends with its status).
- Path: `Storage/Module.cls:669-671` `quit:$$$ISERR(tSC)` leaves the try block, and `$$$ThrowOnError(tSC)` (`:704`) throws before `FinalizePhase` (`:710`). The catch (`:695`) runs only for thrown errors.
- Evidence: reproduction `repro/activate-invoke-phase-open.cos` → B2 run listed in `index.jsonl` (`repro-activate-invoke-phase-open`). Invokes on the default Configure phase finalize Activate correctly (A1, A5).
- Repair proposal: call `pLog.FinalizePhase(tOnePhase, tSC)` whenever the phase ends with an error, whether returned or thrown, before `$$$ThrowOnError`.
  - Preserve: phase timings for successful phases.
  - Distinguishing check: the reproduction shows Activate with an end time and the Invoke's error.

### D8: An unfinished phase is shown as `ERROR #00: (no error description)`
- Obligation: wiki/09 L161 (an open phase is read as never finished).
- Path: `General/History.cls:420` applies `GetErrorText` to the placeholder status 0.
- Evidence: B2 R2 (`20261113T184444Z-cases-hr`).
- Repair proposal: print "not finished" when the phase has no end time.
- Distinguishing check: R2 shows "not finished".

### D9: A refused `load` records an empty package although the manifest named the module
- Obligation: `history.fields` `Package` (wiki/09 L69: empty only "if the action failed before a name was known"). This is a necessary implication (b), per round 1 disposition R1-ONT-03.
- Path: `Main.cls:2420` `LoadInit("")`. The name is filled in only by `pLog.SetName` (`Utils/Module.cls:1293`), which runs after the downgrade and update-required refusals (`:1213-1222`).
- Evidence: B2 g3, g6, g7 and R4 (`20261113T182832Z-cases-dg`, `20261113T184444Z-cases-hr`). `history find -DPackage=update-simple` shows 14 of the 20 rows whose command names the module.
- Repair proposal: call `pLog.SetName` and `SetVersion` right after `GetModuleObjectFromPath` succeeds (`Utils/Module.cls:1164-1166`).
  - Preserve: a malformed manifest still records an empty package (`TestDevMode`).
  - Distinguishing check: g3 records `Package=update-simple`.

## Deviations under open questions (decision pending)

| ID | Behavior | Governing question and provisional answer | Evidence |
|---|---|---|---|
| Q-05 | A `Shell("load …")` run inside a caller's transaction that the caller rolls back leaves no record, although the in-flight row read `Committed=1` and `Finalized=1` | `history.committed.nested` (record survives under a caller transaction, provisional, #366) | B2 D1, `repro/caller-tx-history-lost.cos` → `20261113T184632Z-…` |
| Q-08 | A SIGKILLed `-dev` load keeps its manifest and classes, but the durable record reads `Committed=0`, `Package=""`, no phases | `history.record.abnormal-end` (provisional after round 1: `Committed` reflects kept changes) and `history.record` (started phases listed) | B2 C4R `20261113T183827Z-cases-c4r` |
| new Q (round 1) | `$SYSTEM.Process.Terminate` of a `-dev` load finalizes (locks) a record with no end time and an open phase | Q under `history.locked` (provisional: an unfinished record is not finalized) | B2 C2 `20261113T182903Z-cases-c-terminate` |
| Q-08 | `Terminate` of a non-dev load: `%OnClose` runs before IRIS rolls back, so the twin update is undone and the IRISTEMP row with the real values is deleted. The durable row has `Package=""` and no phases | `history.record` (started phases listed) | B2 C1 |

## Withdrawn

- D7 (`history details` prints the raw `%Status`): wiki/09 L133 documents it on purpose ("so the ObjectScript stack is visible"). Round 1 disposition R1-ONT-13.

## Suspected, not observed natively

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
