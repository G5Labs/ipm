# Product defects and issues: IPM load and history

Defects and open issues found by the IPM Verus uplift at product revision `571e04a1`. Product repairs were not authorized, so each entry carries a repair proposal and no fix. Each proposal names the obligation, the affected writers and readers, the behavior to preserve and a check that distinguishes a fixed product from the current one.

Status values:
- **confirmed**: a governing source requires otherwise, and a native reproduction through the public command shows the violation.
- **confirmed, decision pending**: the violation is confirmed against the best-supported provisional answer of an open question.
- **suspected**: from code reading only.
- **withdrawn**: a governing source documents the behavior as intended.

`VV` = `.g5/Verus_verification`. Run IDs are directories under `VV/results/native/`.

Updated after stage C round 1 and B2 phase 2.

## Confirmed

### D1: A dependency's history record is lost when its parent's transaction rolls back
- Obligation: `history.survives-rollback` and `history.dependencies` (issue #366: "'install' (including as a dependency)", "if install rolls back we still need the history").
- Path: `load <dir>` → `LoadNewModule` `tstart` (`Utils/Module.cls:1195`) → `LoadDependencies` → `LoadModuleReference` creates the dependency's log inside the parent's transaction (`:65`, `:67` → `HistoryTemp.cls:21` twin insert). Its `%OnClose` persist (`HistoryTemp.cls:62-73`) also runs inside that transaction and deletes the IRISTEMP row. The parent's `trollback 1` loop (`Utils/Module.cls:1358-1362`) then removes the twin. The same happens in `-dev` when a dependency fails inside the second transaction.
- Evidence: B2 cases A1, A2, A3 (`20261113T182759Z-cases-a`, replay `20261113T184230Z-replay-fast`). Reproduction: `VV/native/repro/dep-history-lost.cos` → `20261113T184617Z-repro-dep-history-lost`. The ID is allocated and no row persists.
- Formal: model violations V1 and V2 (`case_c31_dep_record_lost`, `case_c33_dev_dep_record_lost`, `case_c45_*`, explorer `asbuilt-r3-dep`). Terminate variants are confirmed natively by replay `20261113T194122Z-replay-terminate-b`.
- Repair proposal: write the dependency's durable History row in a way no rollback undoes, and set its `Committed` from whether the dependency's writes were actually kept.
  - Mechanism: insert and update the row with process journaling off for the duration of that write only. That is, scope `%IPM.Utils.JournalManager(0)` around the `History` save in `HistoryTemp.Init` and `PersistToTwin`. N-P07 shows unjournaled sets survive `TROLLBACK` and the journaling flag is restored. Whether this holds inside an open transaction, mixed with journaled updates of the same row and through SIGKILL, is premise R-01, checked natively by B2 (see `premises.md`).
  - `Committed`: the parent records, for each dependency log, whether its writes were later undone, and the parent's rollback path updates those rows (`Committed=0`). In developer mode, dependencies stay when the parent fails after the second commit (case A5, B1-C38), so they keep `Committed=1`.
  - A deferral that writes nested rows only after the top-level action settles is not enough, because a process end inside the parent then leaves no dependency row (B1-C45K/T).
  - Writers: `HistoryTemp.Init`, `PersistToTwin`, `%OnClose`, `Utils.Module:LoadModuleReference`, `LoadNewModule` error path.
  - Readers: `history find` and `details`, `%IPM_General.History`, `Records`.
  - Preserve: a top-level failure reads `Committed=0`. A committed dependency of a successful parent reads `Committed=1`. ID order follows start order. Finalized rows stay immutable.
  - Distinguishing checks:
    - A1 expects one dependency row with `Committed=0`.
    - A3 expects a dependency row with an error `Success`.
    - A5/C38 expect the dependency row with `Committed=1`.
    - B1-C45K and C45T (process end inside the parent) expect the dependency row to survive.
    - A0 keeps two committed rows.
  - The model's repair variant 103 assumes this mechanism (premise R-01). Its result is in `B/model-notes.md`.

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

### D6: An Invoke that returns an error leaves its phase open in a finalized record (decision pending: Q-14)
- Obligation: `history.phases` (wiki/09 L135-161: a phase with no elapsed time "never finished"), and `history.phases.returned-errors` (Q-14, provisional: every started phase ends with its status).
- Path: in `ExecutePhases`, the before-phase and after-phase Invoke loops exit with `quit:$$$ISERR(tSC)` (`Storage/Module.cls:608-610` and `:669-671`), which leaves the try block. Then `$$$ThrowOnError(tSC)` (`:704`) throws before `FinalizePhase` (`:710`), and the catch (`:695`) runs only for thrown errors. This applies to every phase with an Invoke, not only Activate.
- Evidence: reproduction `repro/activate-invoke-phase-open.cos` (an Invoke bound to Activate) → B2 run listed in `index.jsonl` (`repro-activate-invoke-phase-open`). Invokes on the default Configure phase finalize Activate correctly (A1, A5). Other phases were not reproduced natively.
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
- Repair proposal: at each `SetName`, `SetVersion`, phase boundary and commit, copy the name, version, `Committed` and phases to the durable row. Use the same rollback-proof write as D1 (journaling off around the History save, premise R-01), so the copy survives the load's own rollback and a process end at any `$TLEVEL`. Copying only at `$TLEVEL` 0 would not help a normal-mode load, whose phases all run at `$TLEVEL` 1. This shares the mechanism of D1, D12 and the Q-08 deviation (`design_simplification.md` S2).
  - Preserve: finalized rows stay immutable. `Success` stays 0 and `TimeEnd` empty for an unfinished record. A rolled-back normal-mode load reads `Committed=0`.
  - Distinguishing check: C4R and C3 list Initialize..Compile ended and Activate open, with the package set. If R-01 is refuted, C3 (normal mode) stays failing and only developer-mode kills (C4R) are repairable this way.

### D11: Manifest refusals under `-DNoTransaction=1` or `-DNoJournal=1` keep module data
- Obligation: `load.refusal` (I: a refusal writes no module data, in every mode), `load.refusal.manifest`.
- Path: the no-transaction marker and `SetCommitted` (`Utils/Module.cls:1226-1228`), then `OBJ.Load` of the manifest (`:1251`). `StudioDocument/Module.cls` `ImportStream` saves the `.ZPM` document and rolls back its own nested transaction when validation refuses the module, but journaling is off, so the save is not undone (P-09, N-P07).
- Path (late checks): with `-DNoTransaction=1`, a manifest with several documents or no module definition is refused after `OBJ.Load` has saved its documents (`Utils/Module.cls:1251-1260`).
- Evidence:
  - NoJournal validation refusal: model case B1-C10, native replay `20261113T193952Z-replay-return-b`. Status error, `zpm_document_version=2.0.0` (before: none), `Committed=1`, no package.
  - NoTransaction late checks: stage C cases S2 and S2B (`20261113T204152Z-stagec`) and model case B1-C52 (`20261113T205137Z-replay-c51-c52`, l2 agrees). The module record, `.ZPM` document and extra class are kept, with `Committed=1`.
  - B1-C09 (NoTransaction validation refusal) keeps nothing but shares D12 and D9.
- Repair proposal: validate the manifest before any write (`design_simplification.md` S1), or delete the saved document on refusal when no transaction is active.
  - Preserve: valid NoJournal loads (B4 neighbour).
  - Distinguishing check: B1-C10, B1-C52, S2 and S2B leave no module data and record `Committed=0`.

### D12: A NoTransaction or NoJournal load refused after the no-transaction marker reads `Committed=1` with nothing kept
- Obligation: `history.committed` (I: a refusal before anything is written is not committed, in any mode).
- Path: `SetCommitted` runs at `Utils/Module.cls:1226-1228`, before the manifest import (`:1251`), so any refusal after that point leaves `Committed=1`.
- Evidence: model V4 (`case_c08_committed_without_change`, `case_c09_committed_without_change`, explorer `asbuilt-commit-exact`). Native replay `20261113T193952Z-replay-return-b` (B1-C08: "No module definition found.", nothing written, `Committed=1`).
- Repair proposal: validate the manifest before the no-transaction marker (`design_simplification.md` S1), so that no refusal follows it. Moving the marker after `OBJ.Load` is not a safe fix on its own: until D11 is fixed, a NoJournal validation refusal (B1-C10) keeps the `.ZPM` document and would then read `Committed=0`, which breaks the proved rule "changed implies `Committed=1`" (`theorem_commit_sound`).
  - It changes a decision: the D block `history.committed.marking` (PR #1322: mark "before writing when there is no transaction") would move to "after validation". That needs maintainer agreement.
  - Preserve: changed implies `Committed=1` (B1-C10, B1-C52, S2 until D11 is fixed). B3 and B4 (requirements refusals that keep the manifest read `Committed=1`, until D4 is decided). A0.
  - Distinguishing check: B1-C08 reads `Committed=0`.

### D13: A load that committed reports failure (`<INVALID OREF>`) when the temporary `.modules` repository could not be created
- Obligation: `load.outcome` (I: a failed load in normal mode keeps no module data, so the returned status and the namespace must agree).
- Path: `TempLocalRepoManager:%OnNew` fails and `%New` returns `""` (`General/TempLocalRepoManager.cls:63-68`). After `LoadNewModule` has committed, `tmpRepoMgr.CleanUp()` (`Main.cls:2529`) throws `<INVALID OREF>`.
- Evidence: model V6 (`case_c29_error_after_commit`). Native replay `20261113T193952Z-replay-return-b` (B1-C29: `ERROR #5002: <INVALID OREF>LoadInternal+83^%IPM.Main.1`, module and classes installed, `Committed=1`).
- Repair proposal: check the repository manager before use, and report its creation error before the transaction starts. Treat cleanup errors after the commit as warnings.
  - Preserve: the normal-mode rollback of real failures.
  - Distinguishing check: B1-C29 either refuses before writing or succeeds with a warning.
  - If maintainers consider an error after the final commit acceptable, record that as a Q under `load.outcome`.

### D14: A dependency whose own `<Defaults>` sets `NoJournal` escapes its parent's normal-mode rollback (decision pending: Q-22)
- Obligation: two documented rules conflict, so the precedence is the open question Q-22 under `transaction.flag-sources` (round 2 disposition R2-03).
  - `load.outcome`, normal mode: "The whole load, including dependencies loaded for it, is rolled back".
  - `transaction.flag-sources`, with wiki/00 L175: `<Defaults>` are "applied whenever this module is installed". wiki/00 L149: NoTransaction is for modules that "conflict with an outer wrapping transaction". wiki/00 L155: NoJournal changes "cannot be rolled back".
  - Whichever way Q-22 is decided, the dependency's lost history record is D1.
- Path: `LoadNewModule` for the dependency merges its `<Defaults>` under the parent's parameters (`Utils/Module.cls:1166-1169`). Because the parent never set `NoJournal`, the dependency's value applies and creates `JournalManager(0)` inside the parent's open transaction (`:1191-1193`). Its unjournaled writes are not undone by the parent's `trollback` (N-P07).
- Evidence: stage C case M5 (`20261113T204405Z-stagec2`), with neighbour M5N. The parent is rolled back, and the dependency stays installed with its class and `.ZPM` document. There is no dependency history row: the row was journaled and rolled back. No `<ROLLFAIL>`, `$TLEVEL` 0. Not representable in the model (open obligation in `B/model-notes.md`).
- Options, depending on Q-22:
  - If the parent's rollback rule wins: a nested load inherits its parent's transaction mode, so `NoTransaction` and `NoJournal` from a dependency's `<Defaults>` are ignored while a parent transaction is open, or such a dependency is refused. Preserve: `<Defaults>` transaction flags for a top-level load. Distinguishing check: M5 leaves the dependency uninstalled.
  - If the dependency's `<Defaults>` win: document that the parent's rollback cannot undo such a dependency, and keep its history record (D1).

### D15: `load` refuses `.tar.gz` archives
- Obligation: `load.path.archive` (help `Main.cls:280-282` example `load C:\module\root\path\module-0.0.1.tar.gz`; CHANGELOG 0.10.0 #474).
- Path: `extension = $piece(path,".",*)` yields `gz`, so `extension="tar.gz"` never matches (`Main.cls:2466-2467`). The path is then treated as a directory. This regressed in `f83b207e` (#887), which replaced the two-piece check.
- Evidence: stage C S1, with neighbour S1N (`.tgz`) (`20261113T204152Z-stagec`). Refused with "File not found to obtain module object: module.xml".
- Repair proposal: test the last piece for `tgz` and the last two pieces for `tar.gz`.
  - Preserve: `.tgz` and folder loads.
  - Distinguishing check: S1 loads.

### D16: `update -path` with another module's archive crashes instead of giving the mismatch refusal
- Obligation: `update.refusal` (an `update -path` archive must contain the named module), `history.committed` (no false commit).
- Path: `LoadNewModule` opens the installed module by the archive's module name (`Utils/Module.cls:1207`, `""` when not installed) and dereferences it at `:1232` while that module is not installed, before the mismatch guard (`:1269-1275`) can run.
- Evidence: stage C S3 and S3N1 (`20261113T204152Z-stagec`). `<INVALID OREF>LoadNewModule+67`. Under `-DNoTransaction=1` the log reads `Committed=1`, and the archive is left extracted under `<ModuleRoot>`. A2 OBS-L6's prediction ("B's manifest stays") is refuted.
- Repair proposal: run the mismatch check on the manifest object before the version guards (S1 in `design_simplification.md`).
  - Distinguishing check: S3 refuses with the mismatch message and `Committed=0`.

### D17: `load` accepts an existing relative path
- Obligation: `load.refusal.path` (CHANGELOG 0.10.4 #706: "`load` now only accepts absolute paths").
- Path: `Main.cls` refuses a relative path only when it does not exist, is not absolute and is not a URL (`Main.cls:2447-2452`). A relative path that `%File` resolves against the process directory passes.
- Evidence: stage C S4R (`20261113T204405Z-stagec2`), with neighbours S4N1 and S4N2. It loaded, with `Committed=1`.
- Repair proposal: refuse any non-absolute, non-URL path before the existence check.
  - Distinguishing check: S4R is refused.

### D18: Namespace default modifiers become `history` filters (decision pending: Q-18)
- Obligation: `history.read.filter.default-modifiers` (Q-18, provisional: no), `history.read.find` (the log lists records). Also see the round 1 note that query errors must not look like an empty log.
- Path: defaults are merged into every command's data (`Main.cls:1067-1069`). Non-dotted `-D` names are duplicated under `data("zpm",…)` (`CLI.cls:815-819`), which `History` uses as its filter (`Main.cls:4136`). `GetHistory` ignores the SQL error for the unknown column.
- Evidence: stage C H2 (`20261113T204152Z-stagec`). After the product's own help example `default-modifiers -set -DNoTransaction=1 -DUpdateSnapshots=1`, `history find` shows 0 rows with an OK status and `history delete -confirm` fails with SQLCODE -29.
- Repair proposal: `history` reads only its own command-line filters, and query errors are reported.
  - Distinguishing check: H2 lists the 5 rows.

### D19: "Namespace" default modifiers are shared by every namespace
- Obligation: `settings.default-modifiers` ("adds to every command in the current namespace"; CLI description).
- Path: `$$$ZPMDefaultModifiers` is `^%IPM.DefaultModifiers` (`inc/IPM/Common.inc:31`), a `^%` global mapped to IRISSYS.
- Evidence: stage C X1 (`20261113T204152Z-stagec`). Defaults set in USER are visible in USER-VERIFY. Consequence for loading: `-DNoTransaction=1` set in one namespace makes every load in every namespace non-transactional.
- Repair proposal: store default modifiers in a namespace global, or subscript the global by namespace.
  - Distinguishing check: X1 shows nothing in USER-VERIFY.

### D20: The `GlobalRecords` query returns the current namespace's rows once per namespace
- Obligation: `history.read.api` (`GlobalRecords` reads all namespaces; INT-HIST-11, issue #366 "cross-namespace class query").
- Path: `GlobalRecordsFetch` (`General/History.cls:253-281`) fetches result sets prepared for other namespaces without switching `$namespace`.
- Evidence: stage C H10 (`20261113T204152Z-stagec`). `history find -globally` is correct. `SELECT * FROM %IPM_General.History_GlobalRecords(0,0)` returns 458 rows (twice USER's 229) and none of USER-VERIFY's 173.
- Repair proposal: switch to each result set's namespace while fetching, as `DisplayArray` does (`:528`).
  - Distinguishing check: H10 returns 229 + 173 rows.

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

### I2: A load ended mid-way leaves its temporary `.modules` repository registered at first priority
- Obligation: none directly. `load.dependencies.local-repo` says IPM removes the temporary repository "when loading finishes, even after an error", and no source covers a process end.
- Path: the repository is created before the transaction and removed only by the explicit `CleanUp()` (`Main.cls:2527-2529`). A process end inside the load skips it.
- Evidence: after B2's Terminate and SIGKILL replays, six `ipm-temp-modules-N` repositories with `OverriddenSortOrder=-1000` still pointed at the cases' `.modules` folders (`20261113T210229Z-temp-repos-leftover`). B2 recorded and deleted them.
- Consequence: later installs in the namespace resolve dependencies from those folders first.
- Proposal: register the temporary repository in a form that does not outlive the process, or remove stale `ipm-temp-modules-*` entries at the start of the next load.

## Withdrawn

- D7 (`history details` prints the raw `%Status`): wiki/09 L133 documents it on purpose ("so the ObjectScript stack is visible"). Round 1 disposition R1-ONT-13.

## Suspected, not observed natively


From A2 observations. The listed obligations are in the ontology metadata.
- OBS-L13: a git URL load ignores the clone's exit code (a failed clone surfaces only as "File not found to obtain module object") and never removes the clone folder (`Main.cls:2351-2372`). The leftover folder falls under Q-04 (`transaction.rollback-scope`). Git URL loads have no native or formal evidence.
- OBS-L4: `update -dev` from a registry ends in non-dev mode.
- OBS-L5: a dependency newly installed by `update` runs all its update steps.
- OBS-L7: archive installs delete the module's directory before any guard.
- OBS-L9: update steps are skipped silently when the update lock is not acquired in 3 s.
- OBS-L11: the lock file is written before the parent commits.
- OBS-L14: `PermitDowngrade` does not reach the downgrade refusal.
- OBS-L15: `LoadedResource` rows survive a rolled-back load.
- OBS-L17 and OBS-H8: uninstalling a required module reports success.
- OBS-H4: covered by D5.
- OBS-H10, remaining part: the "IPM is not enabled" message is unreachable (the wrong-rows part is D20).
- OBS-H12: a purge failure blocks every command.
- OBS-H13: full cleanup kills other namespaces' in-flight temp rows.
- OBS-H15: self-uninstall records success before running.
