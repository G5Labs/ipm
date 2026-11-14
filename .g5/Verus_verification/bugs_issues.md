# Product findings: IPM load and history

Findings about InterSystems Package Manager (IPM) at product revision `571e04a1`, from the IPM Verus uplift and its follow-up review. This file replaces the uplift's original `bugs_issues.md`, which stays in git at commit `ae619795`. It reconciles every finding ID the uplift produced (A2 observations, defects, issues, model counterexamples, native cases) with the follow-up's new evidence, one entry per underlying cause.

No product file was changed and no repair was implemented. Each finding carries a proposed direction for the maintainers.

## How to read this file

### Classes

Each finding has exactly one class.

| Class | Meaning |
|---|---|
| supported | A governing source requires other behavior. A native run through the public command, or a reproduction script that uses only the product and a fixture, shows the violation. |
| conditional | The verdict depends on an open maintainer question (a Q block in the ontology). The entry names the Q and says what the finding becomes under each option. Nine are defects under the question's provisional answer. Two (D14, OBS-H7) are defects only under an alternative. |
| issue | Observed or code-read behavior worth a maintainer's attention, for which no source states a requirement under the current provisional answers. |
| suspected | Read from the code and not observed natively. A governing source would be violated if the reading is right. A suspected finding whose verdict also depends on a Q stays suspected and names its Q. |
| withdrawn | A governing source documents the behavior as intended. Kept with the reason. |
| out of scope | Outside the product surface this review covers. Kept with the reason. |

### IDs

- D1 to D20 and I1, I2 keep the meaning they had in the original file. D7 stays withdrawn and D8 is I1.
- New defects are D21 to D30 and new issues are I3 to I8.
- D25, D26 and D27 are the three rows of the original table "Deviations under open questions", which had only Q labels.
- Suspected findings taken from the A2 operation inventory keep their A2 ID (OBS-L*, OBS-H*). D23, D29 and D30 are suspected findings with no A2 ID.
- Every other ID that refers to the same cause is listed under "Aliases" and in Appendix A.

### Fields

Each finding gives:

- Expected and Observed, one or two sentences each.
- Governing source: an ontology block by ID, a document line, or a code comment.
- Evidence: run directories, case IDs and model receipts.
- Impact for users.
- Decision: the maintainer question, if any.
- Missing evidence: what was not run and why.
- Direction: a proposed repair, not implemented. The design proposals S1 to S9 it mentions are in `design_simplification.md` next to this file.
- Aliases.

### Paths and labels

- Product paths are relative to `src/cls/IPM/` at `571e04a1`, unless they start with `src/inc/` or `tests/`.
- `VV` is `.g5/Verus_verification`. `RN` is `VV/results/native/` (uplift native runs). `RV` is `VV/results/verus/`. `FR` is `VV/followup/results/` (follow-up models and re-evaluation) and `FRN` is `FR/native/` (follow-up native runs).
- An ontology citation such as `history.read.filter` (`features/history.g5.md:204`) gives the block ID and the line of its tag under `.g5/ontology/`. `wiki/09` is page 09 of the IPM wiki copy in `../ipm-wiki`.
- Case labels: B1-Cnn are model cases replayed natively. Letters with digits (A1, E4, S3, …) are uplift native cases. N-P* are platform premise checks. N-R04-*, N-R05-* and N-P21* are follow-up native cases. The native case "D1" is a load inside a caller's transaction and is not defect D1.
- Follow-up brief items (L-n, H-n, E-n) are in `.g5/uplift/ipm-quality-followup/work/followup-findings.md`.

### Destructive behaviors

For history deletion that widens, the retention purge, record loss and process termination, this file describes the trigger class and cites the archived evidence. It gives no command that deletes data or ends a process. The follow-up did not re-create these outcomes natively. It observed the selection read-only and used models for the effect.

## Summary

| Class | Count | Findings |
|---|---|---|
| Supported defects | 15 | D1, D2, D3, D5, D9, D10, D11, D12, D13, D15, D16, D17, D19, D20, D21 |
| Decision-dependent (conditional) defects | 11 | Defects under the provisional answer: D4 (Q-17), D6 (Q-14), D18 (Q-18), D22 (Q-22), D24 (Q-10), D25 (Q-05/Q-06b), D26 (Q-08), D27 (Q-21), D28 (Q-23). Defects only under an alternative: D14 (Q-22), OBS-H7 (Q-15) |
| Issues without a governing requirement | 8 | I1 to I8 |
| Suspected (code reading only) | 13 | OBS-L3, OBS-L4, OBS-L5, OBS-L9, OBS-L16, OBS-L17, OBS-H10 (remainder), OBS-H12, OBS-H13, OBS-H15, D23, D29, D30 |
| Withdrawn or refuted | 2 | D7, OBS-H9. Three refuted predictions (OBS-L6, the Terminate part of OBS-H6, A1 INT-HIST-03 (d)) are kept as contrary evidence under D16 and D10 |
| Out of scope | 1 | OBS-L15 |

Total: 50 findings. Appendix A accounts for every ID in the families the follow-up was asked to reconcile.

| ID | Class | Finding | What users see | Q |
|---|---|---|---|---|
| D1 | supported | A dependency's history record is lost when its parent's transaction rolls back | Lost history | |
| D2 | supported | A filter column with `_` drops every later filter, so find widens and a filtered delete can remove every record | Data loss | |
| D3 | supported | The retention purge deletes nothing and marks the day's purge done | Broken feature | |
| D4 | conditional | Refused `-dev`, NoTransaction or NoJournal loads keep the module and read committed when system requirements fail | Wrong installed state | Q-17 |
| D5 | supported | `config delete HistoryRetain` restores the first value ever set | Wrong setting | |
| D6 | conditional | An Invoke that returns an error leaves its phase open | Wrong history content | Q-14 |
| D9 | supported | A refused load records an empty package | Wrong history content | |
| D10 | supported | The durable record of an action that never ended keeps only its start values | Wrong history content | |
| D11 | supported | Manifest refusals under NoTransaction or NoJournal keep module data | Wrong installed state | |
| D12 | supported | Refusals after the no-transaction marker read `Committed=1` with nothing kept | Wrong history content | |
| D13 | supported | A load that committed reports `<INVALID OREF>` when the `.modules` repository cannot be created | Status contradicts the namespace | |
| D14 | conditional | A dependency's own `NoJournal` default escapes its parent's rollback | Wrong installed state | Q-22 |
| D15 | supported | `load` refuses `.tar.gz` archives | Broken feature | |
| D16 | supported | `update -path` with another module's archive crashes instead of refusing | Misleading error, false commit | |
| D17 | supported | `load` accepts an existing relative path | Wrong module loaded | |
| D18 | conditional | Namespace default modifiers become `history` filters | History looks empty | Q-18 |
| D19 | supported | "Namespace" default modifiers apply to every namespace | Wrong installed state | |
| D20 | supported | `GlobalRecords` returns the current namespace's rows once per namespace | Wrong API result | |
| D21 | supported | A failed load can leave an installed module without its dependency | Wrong installed state | (Q-22 shapes the repair) |
| D22 | conditional | The top record reads `Committed=0` although a dependency's changes were kept | Wrong history content | Q-22 |
| D23 | suspected | The global history delete hides every error, not only permission errors | Misleading output | |
| D24 | conditional | `history find` shows an empty log with an OK status when its query fails | History looks empty | Q-10 |
| D25 | conditional | A caller's rollback removes the record of a load it wrapped | Lost history | Q-05/Q-06b |
| D26 | conditional | An unfinished record reads `Committed=0` although changes were kept | Wrong history content | Q-08 |
| D27 | conditional | A Terminate finalizes (locks) the record of an action that never ended | Wrong history content | Q-21 |
| D28 | conditional | A dependency's own `NoTransaction` default does not keep it out of the parent's transaction | Flag has no effect | Q-23 |
| D29 | suspected | Refused `update`, `reinstall` and `uninstall` attempts leave no record | Missing history | Q-09 |
| D30 | suspected | `uninstall -purge` sets a key nothing reads | Flag has no effect | |
| I1 | issue | An unfinished phase is shown as `ERROR #00` | Misleading output | |
| I2 | issue | A load ended mid-way leaves its temporary repository registered first | Wrong dependency source later | |
| I3 | issue | A dependency in developer mode is silently replaced and leaves developer mode | Developer setup lost | (new Q or Q-03) |
| I4 | issue | The history class queries return no ID and no namespace | API rows cannot be attributed | |
| I5 | issue | Compile ignores a resource processor's error status | Failure reported as success | |
| I6 | issue | Files a load writes outside the database outlive its rollback | Files disagree with the namespace | (Q-04) |
| I7 | issue | `PermitDowngrade` reaches the dependency graph and one guard but not the downgrade refusal | Inconsistent flag | |
| I8 | issue | `-DNoLock=1` turns compile locking on | Flag inverted | |
| OBS-L3 | suspected | A plain load does not save `Root` and `Repository`, so the first lock file goes to the wrong folder | Misplaced file | |
| OBS-L4 | suspected | `update -dev` from a registry ends in normal mode | Wrong installed state | |
| OBS-L5 | suspected | A dependency first installed by `update` runs all its update steps | Unneeded migrations | |
| OBS-L9 | suspected | Update steps are skipped silently when the update lock times out | Failure reported as success | |
| OBS-L16 | suspected | A failed legacy XML install reads `Committed=0` although it kept changes | Wrong history content | |
| OBS-L17 | suspected | Uninstalling a module others require reports success | Failure reported as success | |
| OBS-H7 | conditional | Dependency records are labeled `load`, keep a source and list it as `n/a` | Source hidden | Q-15 |
| OBS-H10 (remainder) | suspected | Namespaces without IPM are not reported as such by `-globally` | Misleading output | |
| OBS-H12 | suspected | A purge failure stops the user's command | Command blocked | Q-13 |
| OBS-H13 | suspected | IPM self-uninstall with full cleanup deletes other namespaces' in-progress records | Lost history | Q-16 |
| OBS-H15 | suspected | Self-uninstall records success before it runs and exits the shell even when skipped | Failure reported as success | |
| D7 | withdrawn | `history details` prints the raw `%Status` | Documented | |
| OBS-H9 | withdrawn | An unfiltered global delete needs no confirmation | Documented | |
| OBS-L15 | out of scope | `LoadedResource` rows survive a rolled-back load | Excluded API | |

## Supported defects

### D1: A dependency's history record is lost when its parent's transaction rolls back

- Expected: a dependency load gets its own record. The record survives any rollback that undoes the dependency, reading `Committed=0`.
- Observed: the record's ID is allocated and no row persists after the parent's rollback. This holds in normal mode when the parent or the dependency fails, in developer mode when a dependency fails inside the second transaction, and at a process end inside the parent's transaction. A failed dependency load leaves no trace at all.
- Governing source: `history.survives-rollback` (`features/history.g5.md:67`, "This includes a parent load's rollback of a dependency it loaded"), `history.dependencies` (`:171`), `history.committed` (`:125`, "A dependency undone that way reads uncommitted"). Issue #366: "'install' (including as a dependency)" and "if install rolls back we still need the history". Round 1 disposition R1-ONT-01.
- Mechanism: `LoadModuleReference` creates the dependency's log inside the parent's transaction (`Utils/Module.cls:64-68` → `General/HistoryTemp.cls:21`, the durable twin insert). Its `%OnClose` persist (`HistoryTemp.cls:62-73`) runs inside the same transaction and deletes the IRISTEMP row. The parent's rollback loop (`Utils/Module.cls:1358-1362`) then removes the twin.
- Evidence:
  - Native A1, A2, A3 (`RN/20261113T182759Z-cases-a`, replay `RN/20261113T184230Z-replay-fast`) and reproduction `RN/20261113T184617Z-repro-dep-history-lost`.
  - B1-C31, C32, C33, C36, C37 (`RN/20261113T193952Z-replay-return-b`), B1-C45T, C47T (`RN/20261113T194122Z-replay-terminate-b`), B1-C45K, C47K (`RN/20261113T194155Z-replay-sigkill-b`).
  - M5N and the dependency row of M5 (`RN/20261113T204405Z-stagec2`). Follow-up N-R05-Q23F and N-R05-NESTF (`FRN/20261113T234817Z-r05-modes-nesting`).
  - Model V1 and V2: lemmas `case_c31_dep_record_lost`, `case_c33_dev_dep_record_lost`, `case_c45_kill_dep_record_lost`, `case_c45_terminate_dep_record_lost` (`RV/20261113T222933Z-asbuilt-proofs-r3`, rerun `FR/20261113T232529Z-r12-asbuilt-proofs`: 263 verified, 0 errors). Explorer violations rerun on the final inputs: `FR/20261113T233033Z-asbuilt-r3-dep`, `FR/20261113T233032Z-asbuilt-dep-clauses`.
  - The corrected replay evaluator changed no verdict (`FR/20261114T002646Z-final-replay-reeval/delta.md`).
- Impact: after a failed install or load, history does not show which dependencies were loaded or which one failed. The audit trail issue #366 asked for is lost exactly when it is needed.
- Decision: none for the defect. Q-06 (`history.dependencies.parent-rollback`, `:176`) decides which `Success` the surviving record shows. It could not be observed because the record is lost.
- Missing evidence: every dependency case used a `.modules` filesystem repository. Remote and ORAS registries as the dependency source were not run (follow-up R05d deferred, needs registry services). Dependencies of `install` and `update` were not run natively.
- Direction: write the dependency's durable row so that no rollback removes it. Set `Committed=1` on it only when the dependency's writes are durable. This is the round 3 mechanism (disposition R3-02) with the follow-up's corrections.
  1. Insert the row with process journaling off for that save only (`%IPM.Utils.JournalManager(0)` around the `History` save in `HistoryTemp.Init`), with `Committed=0`. Premise R-01 is confirmed natively: such a row survives a full rollback, `TROLLBACK 1` and SIGKILL, with no `<ROLLFAIL>` (N-P20, `RN/20261113T214135Z-premise-p20-r01`).
  2. Write the facts that must survive (package, version, phases, `Success`, `TimeEnd`) with unjournaled updates as they become known.
  3. Set `Committed=1` with a journaled update as the row's last write before the commit that returns the load to its entry `$TLEVEL` (C2 in developer mode, C3 in normal mode). When no IPM transaction is open, write it at the dependency's close. A rollback then undoes the journaled update and the row reads `Committed=0` again (N-P20C).
  - Constraints found by the follow-up:
    - No unjournaled write may touch the row between the journaled mark and the commit. A rollback restores a journaled write only if the node still holds the value that write set (premise P-14). Natively, an unjournaled insert, a journaled `Committed=1` and an unjournaled `Finalized=1` left both values at 1 after the rollback (N-P21A and N-P21R in `FRN/20261113T234716Z-p21-ordering-r2`; the first run `FRN/20261113T234651Z-p21-ordering` is INCOMPLETE for N-P21B).
    - Finalize dependency rows at the top-level close only when no transaction of the load is open. A Terminate between the mark and the commit runs the top-level `%OnClose` before IRIS rolls back (P-06b). The repair design finalizes the dependency rows at the top-level close with unjournaled writes (the crate's variant 103 models the mark as atomic with the commit and cannot show this), so the mark survives the rollback and a rolled-back dependency reads `Committed=1` (60 end states, 30 of them distinct, in a window two statements wide). The guarded variant removes them (`FR/20261114T002647Z-final-loadtx-model`, variants REPAIR-103 and REPAIR-103-GUARDED). The mutant that writes the rows unjournaled between the mark and the commit is detected.
    - A dependency whose own `<Defaults>` set `NoJournal` keeps its writes when the parent rolls back (D14). If Q-22 keeps that behavior, its row must read `Committed=1`, so mark it durably at the dependency's own close instead of at the parent's commit. Variant 103 leaves it at 0 (818 dependency-clause violations: 27 at returned ends, 791 at process ends). The original handoff's claim that variant 103 "meets every dependency clause in default and developer modes" holds only when every activation of a load uses the same transaction mode.
    - Do not keep the current `Committed` logic with a surviving row. `Finalize` sets `Committed` from "committed or OK" and the no-transaction marker sets it before writing, so rolled-back dependencies would read 1 (2,712 clause violations, variant REPAIR-NAIVE).
    - Never a journaled insert followed by unjournaled updates (N-P20C2: the unjournaled value survives the rollback).
    - A write-then-correct policy and a deferral of dependency rows to the top-level close both fail at process ends (round 3 variant 16; B1-C45K and B1-C45T).
  - Writers: `HistoryTemp.Init`, `PersistToTwin`, `%OnClose`, `Utils.Module:LoadModuleReference`, the `LoadNewModule` error path. Readers: `history find` and `details`, `%IPM_General.History`, `Records`.
  - Preserve: a top-level failure reads `Committed=0`. A committed dependency of a successful parent reads `Committed=1`. ID order follows start order. Finalized rows stay immutable.
  - Distinguishing checks: A1 expects one dependency row with `Committed=0`. A3 expects a dependency row with an error `Success`. A5 and B1-C38 expect the dependency row with `Committed=1`. B1-C45K and C45T expect the dependency row to survive with `Committed=0`. A0 keeps two committed rows. The N-P21R write order must not occur on any row.
  - Design: S2 and S3, with S8 for the per-dependency mode.
- Aliases: OBS-L1, OBS-H5, V1, V2, B1-C31, B1-C32, B1-C33, B1-C36, B1-C37, B1-C45K, B1-C45T, B1-C47K, B1-C47T (dependency part), native A1, A2, A3, M5N, the dependency row of M5, N-R05-Q23F (record part), N-R05-NESTF (record part).

### D2: A filter column with `_` drops every later filter, so `history find` widens and a filtered `history delete` can remove every record

- Expected: several filters must all match. Versions are filtered by part, such as `Version_Major`. Filters work the same way for `find` and `delete`. `history delete` deletes the records that match its filters.
- Observed: `ConstructSQLWhere` walks the filter keys in collation order and stops at the first key whose name fails `^[a-zA-Z][a-zA-Z0-9]*$` (`General/History.cls:103-105`). That key and every later key are dropped. The delete gate in `Main:History` passes because a filter was given (`Main.cls:4165`). A filtered `find` then returns rows the filter excludes. A filtered delete whose first key is such a column runs with no condition.
- Trigger class: any filter on a column whose name contains `_`, which includes every version-part column that `history schema` lists and wiki/09 L112 recommends, when it sorts before the other keys. Uppercase names sort before the underscore, so a `Package` key keeps its condition (N-R04-SEL2) while a lowercase `package` key is dropped (N-R04-SEL).
- Governing source: `history.read.filter` (`features/history.g5.md:204`), `history.delete` (`:253`), wiki/09 L112.
- Evidence:
  - Archived E1, E2, E8 and E4 (`RN/20261113T182845Z-cases-ef`, replay `RN/20261113T184230Z-replay-fast`). Reproductions X2 (`RN/20261113T184621Z-repro-history-filter-widened`) and X3 (`RN/20261113T184623Z-repro-history-delete-underscore`, final `RN/20261113T211000Z-repro-history-delete-underscore`). The X3 script backs up and restores the history it deletes.
  - Follow-up, read-only: N-R04-SEL returned all 9 rows where the contract selects 1 and the deliberately wrong model "skip invalid keys" predicted 2. N-R04-SEL2 returned 2 rows where the contract selects 1 (`FRN/20261113T234810Z-r04-history-readers`).
  - Model `FR/20261114T002647Z-final-history-readers-model`: 24 local and 12 global delete commands remove rows they should keep. All 69 plain-column commands match the contract.
- Impact: data loss. A user who follows the documented advice to filter by version part can delete a namespace's whole history without `-confirm`. With `-globally` the same holds for every namespace.
- Decision: none for the defect. Q-10 (`history.read.filter.invalid-column`, `:214`) decides how an unknown column is treated. Every option it lists rejects silent dropping.
- Missing evidence: the widening delete was not re-created in the follow-up, by its ground rules. The global variant rests on the model and the code.
- Direction: accept `_` in names and check each name against the table's columns, as `history schema` lists them. Reject any other name with an error instead of quitting the loop. Apply this in `GetHistory`, `DeleteHistory` and `DeleteHistoryGlobally`.
  - Preserve: E5 (no filter and no `-confirm` is refused), E6 (a single-package delete removes one row), the SQL-injection guard on column names.
  - Distinguishing checks: N-R04-SEL returns 1 row. E2 lists 0 rows. E4 deletes nothing.
  - Design: S4.
- Aliases: OBS-H1, E1, E2, E4, E8, X2, X3, N-R04-SEL, N-R04-SEL2, H-1.

### D3: The retention purge deletes nothing and still marks the day's purge as done

- Expected: with `HistoryRetain` set to N days, IPM deletes records that started more than N days ago, on any IPM command at most once per calendar day, in every namespace.
- Observed: `AutoPurgeOldHistory` builds the filter value `"<='" _ cutoff _ "'"` (`General/History.cls:173`). `ConstructSQLWhere` strips the `<=` and binds the rest, quotes included (`:107-109`), so no row matches. `SetHistoryLastPurge` (`:176`) then stamps today, so the purge does not retry that day.
- Governing source: `history.retention` (`features/history.g5.md:299`).
- Evidence: F1 (`RN/20261113T182845Z-cases-ef`, replay) and reproduction `RN/20261113T184626Z-repro-autopurge-noop`, where an adapter control without the quotes deleted exactly the two old rows. Follow-up read-only N-R04-CUTOFF: the cutoff is `2026-10-14 00:00:00`, the quoted comparison selects 0 rows and the unquoted one selects the 40-day-old row (`FRN/20261113T234810Z-r04-history-readers`). Model variant M-NO-QUOTES (`FR/20261114T002647Z-final-history-readers-model`).
- Impact: retention never works, so history grows without bound although the user configured a limit. The setting is instance-wide (`%SYS` `^IPM.settings`) and the purge runs before every shell command in every namespace.
- Decision: none for the defect. Once it is fixed, Q-20 (`history.delete.running-records`, `:276`) decides whether the purge may remove the record of an action that started before the cutoff and is still running. The provisional answer allows it. The M-NO-QUOTES model shows the fixed purge does remove such a record. Purge failures are OBS-H12 (Q-13).
- Missing evidence: a purge that deletes was not re-created in the follow-up.
- Direction: bind `"<="_cutoffDate` without quotes. Stamp `lastPurge` only after the delete succeeded. If Q-20 takes its alternative, skip unfinalized rows.
  - Preserve: retention 0, the default, deletes nothing. At most one purge per day.
  - Distinguishing checks: F1 deletes both 40-day-old rows and keeps the 1-day-old row. N-R04-CUTOFF selects 1 row.
  - Design: S4. Related: D5 (a reset can leave retention on) and D23 (the purge runs through `DeleteHistoryGlobally`, which hides errors).
- Aliases: OBS-H3, F1, N-R04-CUTOFF, H-2, H-7 (the running-record consequence).

### D5: `config delete HistoryRetain` restores the first value ever set, not the default 0

- Expected: `config delete` restores a setting's default and the default of `HistoryRetain` is 0.
- Observed: `ResetToDefault` reads the stored default node (`Repo/UniversalSettings.cls:101`). `SetValue` seeds that node with the first value ever set (`:178-179`) and `HistoryRetain` has no install-time default.
- Governing source: `settings` (`concepts/settings.g5.md:13`), `history.retention` (`features/history.g5.md:299`).
- Evidence: F0 and reproduction `RN/20261113T184628Z-repro-config-delete-retain`. Also seen after the product's own `TestAutoPurgeOldHistory`, which ends with `HistoryRetain=30` (`RN/20261113T182514Z-suite-test-CLI`).
- Impact: a user who deletes the setting to turn retention off leaves it on for the whole instance. D3 hides this today. Once D3 is fixed, records would be purged by a setting the user believes is gone.
- Decision: none.
- Missing evidence: other settings without an install-time default were not checked natively.
- Direction: give `HistoryRetain` an explicit factory default of 0 in `ResetToDefault`, as `ModuleRoot` has (`:97-99`), or seed the default node at install. Do not seed it from a user's first `SetValue`.
  - Preserve: settings that do seed their default at install.
  - Distinguishing check: F0 returns 0.
  - Design: S6.
- Aliases: OBS-H4.

### D9: A refused load records an empty package although the manifest named the module

- Expected: `Package` is empty only if the action failed before any name was known. A refusal after IPM read the manifest records the name.
- Observed: `Main.cls:2420` starts the log with `LoadInit("")`. The name is set only by `pLog.SetName` (`Utils/Module.cls:1293`), which runs after the downgrade and update-required refusals (`:1213-1222`) and after the manifest checks (`:1251-1264`). The refusal message itself names the module.
- Governing source: `history.fields` (`features/history.g5.md:79`), wiki/09 L69. Round 1 disposition R1-ONT-03.
- Evidence: g3, g6, g7 (`RN/20261113T182832Z-cases-dg`, replay) and R4 (`RN/20261113T184444Z-cases-hr`: a package filter finds 14 of the 20 rows whose command names the module). B1-C09 to C15 and C18 (`RN/20261113T193952Z-replay-return-b`), B1-C51 and C52 (`RN/20261113T205137Z-replay-c51-c52`). Model V7. The g-cases' CONFORMS verdict predates the package expectation that R1-ONT-03 added.
- Impact: filtering history by package misses refused attempts, so a user cannot see that a downgrade or update of that module was refused.
- Decision: none.
- Missing evidence: none material.
- Direction: call `pLog.SetName` and `SetVersion` right after `GetModuleObjectFromPath` succeeds (`Utils/Module.cls:1164`).
  - Preserve: a malformed manifest still records an empty package (`TestDevMode`).
  - Distinguishing check: g3 records the module name. B1-C11 sets the package.
  - Design: S1.
- Aliases: V7, B1-C09 (package part), B1-C10 (package part), B1-C11, B1-C12, B1-C13, B1-C14, B1-C15, B1-C18, B1-C51, B1-C52 (package part), g3, g6, g7, R4.

### D10: The durable record of an action that never ended keeps only its start values

- Expected: after an action stops without ending, the log lists its record with `Success` 0, no end time, the phases it started and the package it had named.
- Observed: every setter after `Init` updates only the IRISTEMP row (`General/AbstractHistory.cls:86-173`). The durable twin is written at `Init` (`HistoryTemp.cls:21`) and at `%OnClose` (`:62-73`). After a SIGKILL no `%OnClose` runs, so the twin keeps an empty package, the placeholder version and no phases. The IRISTEMP row is orphaned. After a Terminate of a normal-mode load, `%OnClose` runs before IRIS rolls the transaction back, so its twin update is undone and the IRISTEMP row is deleted.
- Contrary evidence: A2 OBS-H6 predicted the start-value row for a Terminate too. Premise N-P10 refuted that, because `%OnClose` does run at a Terminate. For a developer-mode load at `$TLEVEL` 0, a Terminate writes the real values and finalizes them (C2, see D27). The ontology renamed OBS-H6 to D10 (`features/history.g5.md:362`).
- Governing source: `history.record` (`features/history.g5.md:41`), `history.fields` (`:79`). Round 1 disposition R1-MOD-03.
- Evidence: C1 (`RN/20261113T182903Z-cases-c-terminate`, replay), C3 (`RN/20261113T182927Z-cases-c-sigkill`), C4R (`RN/20261113T183827Z-cases-c4r`). B1-C44T, C45T, C47T (`RN/20261113T194122Z-replay-terminate-b`), B1-C44K, C45K, C46K, C47K, C48K (`RN/20261113T194155Z-replay-sigkill-b`), B1-C49K approximate (`RN/20261113T205218Z-replay-approx-c`). Model V11; `repair103_c45` shows the D1 repair leaves this defect in place.
- Impact: after a crash, history cannot tell which module or which phase was running.
- Decision: none.
- Missing evidence: IRISTEMP at an instance restart (premise P-12) was not run. No new process end was run in the follow-up.
- Direction: use D1's mechanism for every log. At each `SetName`, `SetVersion` and phase boundary, write the name, version and phases to the durable row with an unjournaled update, so the copy survives the load's own rollback and a process end at any `$TLEVEL`. Write `Committed` with a journaled update as the row's last write before each commit.
  - The ordering constraints under D1 apply to the top-level row too. The follow-up model checked the Terminate window between the mark and the commit only for dependency rows (L-12), so the top-level close needs the same guard and was not modeled.
  - Preserve: finalized rows stay immutable. `Success` stays 0 and `TimeEnd` empty for an unfinished record. A rolled-back normal-mode load reads `Committed=0`.
  - Distinguishing checks: C3 lists Initialize to Compile ended and Activate open, with the package set and `Committed=0`. C4R shows the same phases with `Committed=1`.
  - Design: S2.
- Aliases: OBS-H6 (renamed), V11, C1, C3, C4R (phases and package), B1-C44K, B1-C44T, B1-C45K, B1-C45T, B1-C46K, B1-C47K, B1-C47T, B1-C48K, B1-C49K (top-record fields).

### D11: Manifest refusals under `-DNoTransaction=1` or `-DNoJournal=1` keep module data

- Expected: a refusal happens before IPM writes any module data, so in every mode the namespace stays as it was. IPM refuses a manifest with more than one document or no module definition before it touches any resource.
- Observed: under NoJournal, `StudioDocument/Module.cls` `ImportStream` saves the `.ZPM` document and rolls back its own nested transaction when validation refuses the module, but the save was not journaled, so it stays (B1-C10). Under NoTransaction, a manifest with several documents or no module definition is refused after `$system.OBJ.Load` has saved its documents (`Utils/Module.cls:1251-1260`), which keeps the module record, the `.ZPM` document and the extra class.
- Governing source: `load.refusal` (`features/load.g5.md:83`), `load.refusal.manifest` (`:99`), wiki/09 L133 ("rejected before any resource was touched").
- Evidence: B1-C10 (`RN/20261113T193952Z-replay-return-b`: document version 2.0.0 kept, `Committed=1`), B1-C52 (`RN/20261113T205137Z-replay-c51-c52`), S2 and S2B (`RN/20261113T204152Z-stagec`), with neighbour S2N conforming. Model V12, lemma `case_c52_multiple_documents_kept`.
- Impact: a refused load leaves a partial module record or a stray class. Its record reads committed, so a user may believe nothing was installed.
- Decision: none.
- Missing evidence: other validation refusals under NoJournal were not enumerated.
- Direction: validate the manifest before any write, or delete the saved documents on refusal when no transaction is active.
  - Preserve: valid NoJournal loads (B1-C04).
  - Distinguishing checks: B1-C10, B1-C52, S2 and S2B leave no module data and read `Committed=0`.
  - Design: S1.
- Aliases: V12, B1-C10 (namespace part), B1-C52 (namespace part), S2, S2B.

### D12: A NoTransaction or NoJournal load refused after the no-transaction marker reads `Committed=1` with nothing kept

- Expected: a refusal before anything is written is not committed, in any mode.
- Observed: `SetCommitted` runs at `Utils/Module.cls:1226-1228`, before the manifest import (`:1251`), so any refusal after that point reads `Committed=1`.
- Governing source: `history.committed` (`features/history.g5.md:125`).
- Evidence: B1-C08 ("No module definition found.", nothing written) and B1-C09 (`RN/20261113T193952Z-replay-return-b`). Model V4, lemmas `case_c08_committed_without_change` and `case_c09_committed_without_change`, explorer `FR/20261113T233033Z-asbuilt-commit-exact`.
- Impact: the wiki's recipe for "failed but committed" records (wiki/09 L218) returns false positives and a user may try to undo changes that do not exist.
- Decision: none for the defect. The repair changes the D block `history.committed.marking` (`:136`, PR #1322: mark "before writing when there is no transaction"), which needs maintainer agreement.
- Missing evidence: none material.
- Direction: validate the manifest before the no-transaction marker, so that no refusal follows it. Moving the marker after `OBJ.Load` alone is not safe until D11 is fixed: B1-C10 keeps the `.ZPM` document and would then read `Committed=0`, which breaks the proved rule "changed implies `Committed=1`" (`theorem_commit_sound`).
  - Preserve: changed implies `Committed=1` (B1-C10, B1-C52 and S2 while D11 stands). B3 and B4 while D4 is undecided. A0.
  - Distinguishing check: B1-C08 reads `Committed=0`.
  - Design: S1 and S3.
- Aliases: V4, B1-C08, B1-C09 (`Committed` part).

### D13: A load that committed reports `<INVALID OREF>` when the temporary `.modules` repository could not be created

- Expected: the returned status and the namespace agree. A failed normal-mode load keeps no module data, so an error must mean nothing was kept.
- Observed: when `TempLocalRepoManager:%OnNew` fails, `%New` returns `""` (`General/TempLocalRepoManager.cls:63-68`). After `LoadNewModule` has committed, `tmpRepoMgr.CleanUp()` (`Main.cls:2530`) throws `<INVALID OREF>`.
- Trigger class: a module whose `.modules` folder holds a manifest the filesystem repository cache rejects.
- Governing source: `load.outcome` (`features/load.g5.md:145`), `load.dependencies.local-repo` (`:189`).
- Evidence: B1-C29 (`RN/20261113T193952Z-replay-return-b`: `ERROR #5002: <INVALID OREF>LoadInternal+83^%IPM.Main.1`, module and classes installed, `Committed=1`). Model V6, lemma `case_c29_error_after_commit`.
- Impact: automation sees a failure and may retry or roll back by other means while the module is installed. The record reads failed but committed.
- Decision: none. If maintainers accept an error after the final commit, that becomes a Q under `load.outcome`.
- Missing evidence: the same pattern exists for archive installs from a repository (`Utils/Module.cls:256-259`) and was not run.
- Direction: check the repository manager before use and report its creation error before the transaction starts. Treat cleanup errors after the commit as warnings.
  - Preserve: the normal-mode rollback of real failures.
  - Distinguishing check: B1-C29 either refuses before writing or succeeds with a warning.
  - Design: S7.
- Aliases: OBS-L12, V6, B1-C29.

### D15: `load` refuses `.tar.gz` archives

- Expected: the path may be a `.tgz` or `.tar.gz` archive, as the help example `load C:\module\root\path\module-0.0.1.tar.gz` shows (`Main.cls:282`, CHANGELOG 0.10.0 #474).
- Observed: `$piece(path,".",*)` yields `gz`, so `extension="tar.gz"` never matches (`Main.cls:2466-2467`). The path is treated as a folder and the load fails with "File not found to obtain module object: module.xml". This regressed in `f83b207e` (#887), which replaced a two-piece check.
- Governing source: `load.path` (`features/load.g5.md:42`).
- Evidence: S1, with neighbour S1N (`.tgz`) conforming (`RN/20261113T204152Z-stagec`).
- Impact: a documented archive format does not load.
- Decision: none.
- Missing evidence: none material.
- Direction: test the last piece for `tgz` and the last two pieces for `tar.gz`.
  - Preserve: `.tgz` and folder loads.
  - Distinguishing check: S1 loads.
- Aliases: OBS-L2, S1.

### D16: `update -path` with another module's archive crashes instead of giving the mismatch refusal

- Expected: IPM refuses an update whose `-path` archive holds a different module, keeps no module data and records the refusal as not committed.
- Observed: `LoadNewModule` opens the installed module by the archive's module name (`Utils/Module.cls:1207`, `""` when that module is not installed) and dereferences it at `:1232`, before the mismatch check (`:1269-1275`) can run. The update fails with `<INVALID OREF>LoadNewModule+67`. Under NoTransaction the record reads `Committed=1` and the archive is left extracted under the `ModuleRoot` folder.
- Contrary evidence: A2 OBS-L6 predicted that the check runs after the manifest was saved and that the other module's manifest stays. S3 refuted that, because the load crashes before the manifest import.
- Governing source: `update.refusal` (`features/update.g5.md:20`), `history.committed` (`features/history.g5.md:125`).
- Evidence: S3 and S3N1 (`RN/20261113T204152Z-stagec`). S3N2, an update from the module's own archive, conforms.
- Impact: an internal error instead of the documented message, a false `Committed=1` under NoTransaction, and a leftover extracted folder (I6).
- Decision: none.
- Missing evidence: the case where the other module is also installed, so that the check is reachable, was not run.
- Direction: run the mismatch check on the manifest object before the version checks.
  - Distinguishing check: S3 refuses with the mismatch message and `Committed=0`.
  - Design: S1.
- Aliases: OBS-L6 (prediction refuted), S3, S3N1 (message part).

### D17: `load` accepts an existing relative path

- Expected: `load` accepts only absolute paths and git URLs (CHANGELOG 0.10.4 #706: "load now only accepts absolute paths").
- Observed: `Main.cls:2447-2452` refuses a path only when it does not exist, is not absolute and is not a URL. A relative path that `%File` resolves against the process's current directory passes.
- Governing source: `load.refusal.path` (`features/load.g5.md:94`).
- Evidence: S4R, with neighbours S4N1 and S4N2 conforming (`RN/20261113T204405Z-stagec2`). The first binding S4 is INCOMPLETE (the path did not exist for `%File`).
- Impact: the same command loads different modules depending on the process's directory.
- Decision: none.
- Missing evidence: none material.
- Direction: refuse any path that is not absolute and not a URL before the existence check.
  - Distinguishing check: S4R is refused.
- Aliases: S4R, S4.

### D19: "Namespace" default modifiers are shared by every namespace

- Expected: `default-modifiers` stores flags that IPM adds to every command in the current namespace.
- Observed: `$$$ZPMDefaultModifiers` is `^%IPM.DefaultModifiers` (`src/inc/IPM/Common.inc:31`), a `^%` global stored in IRISSYS and visible from every namespace.
- Governing source: `settings.default-modifiers` (`concepts/settings.g5.md:30`) and the command description in `Main.cls`.
- Evidence: X1 (`RN/20261113T204152Z-stagec`): defaults set in USER are visible in USER-VERIFY.
- Impact: storing `-DNoTransaction=1` in one namespace makes every load in every namespace non-transactional and stored defaults become history filters everywhere (D18).
- Decision: none.
- Missing evidence: none material.
- Direction: store default modifiers in a namespace global, or subscript the global by namespace, with a migration for existing values.
  - Distinguishing check: X1 shows nothing in USER-VERIFY.
  - Design: S9.
- Aliases: OBS-X1, X1, A2 premise P-10 (now observed).

### D20: The `GlobalRecords` query returns the current namespace's rows once per namespace

- Expected: the class query `GlobalRecords` returns the records of all namespaces.
- Observed: `GlobalRecordsFetch` (`General/History.cls:253-281`) fetches result sets prepared for other namespaces without switching `$namespace`, unlike `DisplayArray` (`:528`).
- Governing source: `history.read.api` (`features/history.g5.md:246`). Issue #366 asked for a "cross-namespace class query".
- Evidence: H10 (`RN/20261113T204152Z-stagec`): `history find -globally` is correct, while the query returned 458 rows, twice USER's 229, with none of USER-VERIFY's 173. Model `FR/20261114T002647Z-final-history-readers-model` (`globalrecords:ROWS`).
- Impact: programs that use the documented API get duplicated rows of one namespace and miss the others.
- Decision: none.
- Missing evidence: the follow-up container has no second IPM namespace, so the follow-up relies on H10 and the model.
- Direction: switch to each result set's namespace while fetching, as `DisplayArray` does. Add a namespace column (I4).
  - Distinguishing check: H10 returns 229 + 173 rows.
  - Design: S9.
- Aliases: OBS-H10 (wrong-rows part), H10.

### D21 (new): A failed normal-mode load can leave an installed module without its dependency

- Expected: under Q-22's alternative, a failed normal-mode load keeps no module data. Under Q-22's provisional answer, a dependency whose own `<Defaults>` set `NoJournal` keeps its changes, but an installed module still has its declared dependencies: IPM loads dependencies before the module and refuses to uninstall a module that installed modules depend on.
- Observed: in a chain m → d1 → d2 where d1's own `<Defaults>` set `NoJournal`, the top load's `LoadDependencies` loads the whole closure in dependency order with m's parameters. d2 is loaded before d1 inside m's transaction and does not inherit d1's `NoJournal`. When m fails, d2 is rolled back and d1 stays installed with its class. Neither dependency has a record (D1).
- Mechanism: a module's own defaults are merged under the parameters it receives (`Utils/Module.cls:1164-1169`), the top load passes its parameters to `LoadDependencies` (`:1328`, `:1368-1413`) and an installed version that satisfies the requirement is skipped (`Storage/Module.cls:1139-1154`). Follow-up L-3.
- Governing source: `load.outcome` (`features/load.g5.md:145`) under Q-22's alternative. Under the provisional answer, a necessary implication of `load.dependencies` (`:179`) and `uninstall.dependents` (`features/uninstall.g5.md:20`).
- Evidence: N-R05-NESTF (expectation written before the run) and its success neighbour N-R05-NESTS (`FRN/20261113T234817Z-r05-modes-nesting`). The loadtx model relies on this reduction, argued from the source in its docstring (`VV/followup/models/loadtx.py`, receipt `FR/20261114T002647Z-final-loadtx-model`).
- Impact: d1 is listed as installed while its dependency is missing, so its code can fail at run time. The protection that `uninstall.dependents` gives is bypassed.
- Decision: the violation holds under both options of Q-22 (`transaction.flag-sources.dependency-defaults`). Only the repair depends on it.
- Missing evidence: deeper chains, a developer-mode parent and remote dependency sources were not run.
- Direction: decide one transaction plan per load.
  - If Q-22 takes its alternative (the parent's transaction governs), nested loads inherit the parent's mode, which fixes D14 and D21 together.
  - If Q-22 keeps the provisional answer, load a `NoJournal` dependency's own dependencies with the same `NoJournal`, or refuse the combination, so that what escapes the rollback is closed under dependencies.
  - Distinguishing check: N-R05-NESTF leaves neither d1 nor d2 (alternative), or both (provisional).
  - Design: S8.
- Aliases: L-4, N-R05-NESTF (namespace part).

## Decision-dependent (conditional) defects

### D4 (Q-17): Refused `-dev`, NoTransaction or NoJournal loads keep the module and read committed when system requirements fail

- Expected under the provisional answer: IPM checks system requirements before it writes anything, in every mode, so the refusal writes no module data and reads uncommitted.
- Observed: the manifest is imported and saved. In developer mode the first commit follows (`Utils/Module.cls:1309-1314`). Only then does `CheckSystemRequirements` run (`:1321`). Without a transaction, `SetCommitted` runs at `:1227` before any write. The module stays listed as installed with `Committed=1`.
- Governing source: `load.refusal.requirements` (`features/load.g5.md:124`) and the open question `load.refusal.requirements.timing` (`:129`). wiki/03 L261: requirements "must be validated before module installation".
- Evidence: B2, B3, B4 (`RN/20261113T182820Z-cases-b`, replay) and reproduction `RN/20261113T184630Z-repro-sysreq-dev-manifest-left`. B1-C23 and C24 (`RN/20261113T193952Z-replay-return-b`). Model V3, lemma `case_c23_sysreq_dev_keeps_manifest`. Neighbours B0 and B1 conform. A2 OBS-L10 predicted it from the code and called it consistent with the comment at `:1310`.
- Impact: a module that cannot run in this namespace appears installed and has to be uninstalled by hand.
- Decision: Q-17. Provisional answer (check before writing): a defect. Alternative (a refused developer-mode or NoTransaction load keeps its manifest, as developer mode keeps what it did, #363): intended. `Committed=1` is then correct.
- Missing evidence: requirement kinds other than `SYSNamespace` were not run.
- Direction (provisional answer): check the requirements on the manifest object that `GetModuleObjectFromPath` returns (`:1164`), before the transaction starts and before `SetCommitted`.
  - Preserve: B0 (met requirements in developer mode install and commit). B1 (a normal-mode refusal rolls back). Developer mode keeps the entries of an installable module that fails later (#363).
  - Distinguishing check: B2, B3 and B4 leave no module record and read `Committed=0`.
  - Design: S1, which chooses the provisional answer.
- Aliases: OBS-L10, V3, B2, B3, B4, B1-C23, B1-C24.

### D6 (Q-14): An Invoke that returns an error leaves its phase open in a finalized record

- Expected under the provisional answer: every phase that starts records its end and status, including when an `<Invoke>` returns an error status.
- Observed: the before-phase and after-phase Invoke loops in `ExecutePhases` exit with `quit:$$$ISERR(tSC)` (`Storage/Module.cls:608-610` and `:669-671`). That leaves the try block without entering the catch (`:692-697`). `$$$ThrowOnError(tSC)` (`:707`) then throws before `FinalizePhase` (`:710`). This applies to every phase with an Invoke, not only Activate.
- Governing source: `history.phases` (`features/history.g5.md:107`, wiki/09 L161: a phase with no end time "never finished") and the open question `history.phases.returned-errors` (`:115`).
- Evidence: reproduction `RN/20261113T184634Z-repro-activate-invoke-phase-open` (an Invoke bound to Activate). B1-C28 (`RN/20261113T193952Z-replay-return-b`). Model V5, lemma `case_c28_phase_left_open`. Invokes on the default Configure phase, whose errors are thrown, finalize Activate correctly (A1, A5).
- Impact: an ordinary error looks like a hang or a crash in history.
- Decision: Q-14. Provisional answer: a defect. Alternative (accept open phases for returned errors and document it): documented behavior. I1's display then matters more.
- Missing evidence: phases other than Activate were not reproduced natively. The code shows the same exit for each.
- Direction (provisional answer): call `pLog.FinalizePhase(tOnePhase, tSC)` whenever the phase ends with an error, returned or thrown, before `$$$ThrowOnError`.
  - Preserve: phase timings of successful phases.
  - Distinguishing check: the reproduction shows Activate with an end time and the Invoke's error.
  - Design: S5.
- Aliases: OBS-H14, V5, B1-C28.

### D14 (Q-22): A dependency whose own `<Defaults>` set `NoJournal` escapes its parent's normal-mode rollback

- Expected: two documented rules conflict. `load.outcome` says a failed normal-mode load rolls back the whole load, "including dependencies loaded for it". `transaction.flag-sources` and wiki/00 L175 say `<Defaults>` are "applied whenever this module is installed" and wiki/00 L155 says `NoJournal` changes "cannot be rolled back".
- Observed: the dependency's `LoadNewModule` merges its own `<Defaults>` under the parameters it receives (`Utils/Module.cls:1164-1169`). The parent never set `NoJournal`, so the dependency's value applies and creates `JournalManager(0)` inside the parent's open transaction (`:1191-1193`). Its unjournaled writes survive the parent's rollback (N-P07). The parent is rolled back and the dependency stays installed with its class and `.ZPM` document, with no `<ROLLFAIL>` and `$TLEVEL` 0.
- Governing source: `load.outcome` (`features/load.g5.md:145`), `transaction.flag-sources` (`concepts/transactions-and-dev-mode.g5.md:43`) and the open question `transaction.flag-sources.dependency-defaults` (`:50`).
- Evidence: M5 with neighbour M5N (`RN/20261113T204405Z-stagec2`). Follow-up N-R05-NESTF and N-R05-PARDEF (`FRN/20261113T234817Z-r05-modes-nesting`). In the loadtx model the as-built product violates the effect-prefix clause in 2,440 end states under the alternative and in none under the provisional answer. A model with one global mode cannot produce M5 (`FR/20261114T002647Z-final-loadtx-model`). Command-line flags override a parent's defaults, which override the dependency's own (L-1).
- Impact: under the alternative, a load reported as rolled back leaves a dependency installed.
- Decision: Q-22. Provisional answer (a dependency's own defaults apply): intended behavior. What remains are D1 (the lost record), D21 (the broken dependency closure) and D22 (the top record's `Committed`). Alternative (the parent's transaction governs): a defect, to be repaired by making a nested load inherit its parent's mode or refusing it.
- Missing evidence: none for the behavior. The precedence is a maintainer choice.
- Direction: under the alternative, ignore `NoTransaction` and `NoJournal` from a dependency's `<Defaults>` while a parent transaction is open, or refuse such a dependency. Under the provisional answer, document that the parent's rollback cannot undo such a dependency and fix D1, D21 and D22.
  - Distinguishing check under the alternative: M5 leaves the dependency uninstalled.
  - Design: S8.
- Aliases: M5 (namespace part), round 1 finding R1-MOD-05.

### D18 (Q-18): Namespace default modifiers become `history` filters

- Expected under the provisional answer: default modifiers apply to package changes and are not `history` filters. `history find` lists records.
- Observed: defaults are merged into every command's data (`Main.cls:1067-1069`). Non-dotted `-D` names are duplicated under `data("zpm",…)` (`CLI.cls:815-819`), which `History` uses as its filter (`Main.cls:4136`). After the product's own help example for `default-modifiers` (`Main.cls:628-629`), which stores `NoTransaction` and `UpdateSnapshots`, `history find` shows no rows with an OK status (D24) and an unfiltered confirmed delete fails with SQLCODE -29.
- Governing source: `history.read.find` (`features/history.g5.md:196`) and the open question `history.read.filter.default-modifiers` (`:223`).
- Evidence: H2 (`RN/20261113T204152Z-stagec`; defaults and history restored afterwards). Model `FR/20261114T002647Z-final-history-readers-model` (STATUS violations with a stored default).
- Impact: after following the documented example, history looks empty. With D19 this happens in every namespace.
- Decision: Q-18. Provisional answer: a defect. Alternative (apply defaults as filters and document it): documented behavior, though D24 still hides the resulting error.
- Missing evidence: none material.
- Direction (provisional answer): build the `history` filter only from the command line's own `-D` values.
  - Distinguishing check: H2 lists the 5 rows.
  - Design: S4.
- Aliases: OBS-H2, H2 (filter part).

### D22 (new, Q-22): The top record reads `Committed=0` although a dependency's `NoJournal` changes were kept

- Expected under Q-22's provisional answer: `Committed` shows whether the load kept any changes and a dependency with its own `NoJournal` keeps its changes when the parent rolls back.
- Observed: the parent fails and rolls back while the dependency stays installed. The top record reads `Committed=0`.
- Governing source: `history.committed` (`features/history.g5.md:125`) with `transaction.flag-sources.dependency-defaults` (`concepts/transactions-and-dev-mode.g5.md:50`). This reading counts a dependency's kept changes as changes of the load, as `load.outcome` and the model's contract do. If maintainers read `Committed` strictly per action, D22 reduces to the dependency's own record, which is D1.
- Evidence: M5 (`RN/20261113T204405Z-stagec2`: parent `Committed=0`, dependency installed) and N-R05-NESTF (`FRN/20261113T234817Z-r05-modes-nesting`: top `Committed=0`, d1 installed). Loadtx model: 30 end states, all with a dependency's own `NoJournal` (`FR/20261114T002647Z-final-loadtx-model`).
- Impact: the wiki's recipe for "failed but committed" records misses loads that changed the namespace.
- Decision: Q-22. Provisional answer: a defect. Alternative: once D14 is fixed nothing is kept and `Committed=0` is correct.
- Missing evidence: a developer-mode parent was not run.
- Direction (provisional answer): when an activation of the load writes outside the parent's transaction, mark the top record durably as committed at that point, with the same write rules as D1.
  - Distinguishing check: M5's parent record reads `Committed=1`.
  - Design: S3 and S8.
- Aliases: L-8, M5 (parent record part), N-R05-NESTF (top record part).

### D24 (Q-10): `history find` shows an empty log with an OK status when its query fails

- Expected under the provisional answer: when `find` cannot run its query, it reports the error instead of showing an empty log.
- Observed: `GetHistory` returns the result set of `%ExecDirect` without checking `%SQLCODE` (`General/History.cls:146-147`), unlike `DeleteHistory` (`:157`). A filter on an unknown column, or a stored default modifier (D18), makes `find` print nothing with an OK status.
- Governing source: the open question `history.read.filter.invalid-column` (`features/history.g5.md:214`), extended by round 1 finding R1-ONT-10.
- Evidence: E3 (`RN/20261113T182845Z-cases-ef`; its CONFORMS verdict used the phase 1 expectation "error also acceptable"), the find part of H2 (`RN/20261113T204152Z-stagec`), and follow-up N-R04-UNKNOWN (`FRN/20261113T234810Z-r04-history-readers`: SQLCODE -29 inside the result set, no exception). Model `FR/20261114T002647Z-final-history-readers-model`: `find:STATUS` in 104 commands and `find-global:STATUS` in 52. E7, a delete with an unknown column, is refused correctly.
- Impact: a typing error in a filter column looks like an empty history.
- Decision: Q-10. The provisional answer and both listed alternatives (refuse an unknown column with an error, or ignore it with a warning) all report the problem, so the finding is a defect under every listed option. Only a maintainer choice to accept a silent empty listing would change that.
- Missing evidence: none material.
- Direction: check `%SQLCODE` in `GetHistory` and in each namespace of `GetHistoryGlobally` and report the error.
  - Distinguishing check: E3 reports an error.
  - Design: S4.
- Aliases: E3, H-3, N-R04-UNKNOWN, H2 (find part).

### D25 (Q-05/Q-06b): A caller's rollback removes the record of a load it wrapped

- Expected under the provisional answer: the record of a load run inside a caller's transaction survives the caller's rollback. `Committed` is exact only for actions started outside a caller transaction.
- Observed: the record's insert, every IPM commit and the final persist nest in the caller's transaction, so the caller's rollback removes the record. In flight the record read `Committed=1` and `Finalized=1`.
- Governing source: the open question `history.committed.nested` (`features/history.g5.md:151`), issue #366 ("if install rolls back we still need the history").
- Evidence: native case D1 (`RN/20261113T182832Z-cases-dg`, replay) and reproduction `RN/20261113T184632Z-repro-caller-tx-history-lost`. B1-C42, C43 (`RN/20261113T193952Z-replay-return-b`), B1-C50T (`RN/20261113T194122Z-replay-terminate-b`), B1-C50K (`RN/20261113T194155Z-replay-sigkill-b`). Model V8, lemma `case_c42_caller_rollback_record_lost`. Neighbour D0 conforms. B1-C41 (`Committed=1` while nothing is durable yet) is permitted under the provisional answer.
- Impact: automation that wraps IPM in its own transaction loses the audit trail when it rolls back. The same happens when a module's Invoke runs a load inside another module's normal-mode load (OBS-H16).
- Decision: Q-05 and its model parameter Q-06b. Provisional answer: a defect. Alternative "caller-held transactions are unsupported, so the record may be lost": a documented limitation.
- Missing evidence: none for the behavior.
- Direction (provisional answer): write the top-level row with D1's mechanism, so its unjournaled insert survives the caller's rollback (R-01 was confirmed at `$TLEVEL` 1 and 2). Document what `Committed` means under a caller transaction.
  - Distinguishing check: native case D1 keeps one record.
  - Design: S2.
- Aliases: the original deviation row "Q-05", OBS-H16, V8, native case D1, B1-C42, B1-C43, B1-C50K, B1-C50T.

### D26 (Q-08): The record of an action that never ended reads `Committed=0` although its changes were kept

- Expected under the provisional answer: `Committed` reflects the changes kept, even on an unfinished record.
- Observed: `SetCommitted` writes only the IRISTEMP row (`General/AbstractHistory.cls:120-125`) and the durable twin keeps `Committed=0` until `%OnClose` (N-P14). After a SIGKILL of a developer-mode load after its commits, the manifest and classes stay and the durable record reads `Committed=0`.
- Governing source: the open question `history.record.abnormal-end` (`features/history.g5.md:58`), issue #1237 ("should reflect whether the action's changes were actually retained").
- Evidence: C4R (`RN/20261113T183827Z-cases-c4r`), B1-C46K and C48K (`RN/20261113T194155Z-replay-sigkill-b`). B1-C49K and C49T approximate (`RN/20261113T205218Z-replay-approx-c`): a process end between the C3 commit and its `SetCommitted` (`Utils/Module.cls:1348-1350`) leaves an installed module whose record reads `Committed=0`. Model V9, lemma `case_c46_dev_end_inside_activate`.
- Impact: the wiki's recipe for "failed but committed" records misses crashed loads that kept changes.
- Decision: Q-08. Provisional answer: a defect. Alternative (an empty end time means the outcome is unknown, so readers must not trust `Committed=0`): a documented limitation.
- Missing evidence: B1-C49T and C49K stay APPROXIMATE (a debugger stall). An exact witness needs a hook between the commit and `SetCommitted`, which the product does not have. No new process end was run in the follow-up.
- Direction (provisional answer): D10's mechanism, with the journaled `Committed=1` written before each commit, which also closes the window between the commit and the mark.
  - Distinguishing check: C4R reads `Committed=1`.
  - Design: S2.
- Aliases: the original deviation row "Q-08", V9, C4R (`Committed` part), B1-C46K, B1-C48K, B1-C49K, B1-C49T (`Committed` part).

### D27 (Q-21): A Terminate finalizes (locks) the record of an action that never ended

- Expected under the provisional answer: a record without an end time stays unfinalized.
- Observed: at a Terminate or `HALT`, `%OnClose` calls `PersistToTwin`, which sets `Finalized=1` regardless of the end time (`General/HistoryTemp.cls:53`). A developer-mode load ended in Activate at `$TLEVEL` 0 gets a finalized record with `Success` 0, no end time and Activate open.
- Governing source: `history.locked` (`features/history.g5.md:285`) and the open question `history.locked.unfinished` (`:290`).
- Evidence: C2 (`RN/20261113T182903Z-cases-c-terminate`, replay), with reader case R2. B1-C46T and C48T (`RN/20261113T194122Z-replay-terminate-b`), B1-C49T approximate. Model V10.
- Impact: the record can never be corrected, because finalized records reject edits.
- Decision: Q-21 (the model calls it Q-FIN and the original file called it "new Q (round 1)"). Provisional answer: a defect. Alternative (finalize whenever IPM releases the record): intended.
- Missing evidence: none material.
- Direction (provisional answer): `PersistToTwin` sets `Finalized` only when `Finalize` has run.
  - Distinguishing check: C2's record is unfinalized.
  - Design: S2.
- Aliases: the original deviation row "new Q (round 1)", V10, C2, B1-C46T, B1-C48T, B1-C49T (`Finalized` part).

### D28 (Q-23): A dependency's own `<Defaults>` `NoTransaction` does not keep it out of its parent's transaction

- Expected under the provisional answer: a module that declares `NoTransaction` does not run inside a transaction opened for its parent, so a failure may leave it partly installed.
- Observed: the dependency only skips its own `tstart` (`Utils/Module.cls:1172`, `:1194-1196`). It stays inside the parent's transaction, is undone with it (`:1358-1362`) and loses its record (D1). On success it is installed with `Committed=1`.
- Governing source: the open question `transaction.flag-sources.dependency-no-transaction` (`concepts/transactions-and-dev-mode.g5.md:59`), wiki/00 L149 (`NoTransaction` is for modules that "conflict with an outer wrapping transaction").
- Evidence: first native observation, follow-up N-R05-Q23F and its success neighbour N-R05-Q23S (`FRN/20261113T234817Z-r05-modes-nesting`, expectations written before the run). Loadtx model: 5,780 end states violate the effect-prefix clause under the provisional answer and none under the alternative (`FR/20261114T002647Z-final-loadtx-model`).
- Impact: under the provisional answer, a module that cannot live inside an outer transaction still runs inside one.
- Decision: Q-23. Provisional answer: a defect. Alternative (the flag affects only the dependency's own load and users pass `-DNoTransaction=1` for the whole command): intended.
- Missing evidence: a dependency with both `NoTransaction` and `NoJournal`, and a developer-mode parent, were not run.
- Direction (provisional answer): resolve each load's transaction plan before the first `tstart`, so a dependency's `NoTransaction` suppresses the parent's transaction or is refused with a clear message.
  - Distinguishing check: N-R05-Q23F keeps the dependency installed after the parent fails.
  - Design: S8.
- Aliases: L-2, N-R05-Q23F (namespace part).

### OBS-H7 (Q-15): Dependency records are labeled `load`, keep a source and list it as `n/a`

- Expected: under the provisional answer of Q-15, dependencies of `load` and `update` keep the `load` label and their stored source. Listings show a source only for installs. The product does exactly this.
- Observed: only `cmd="install"` selects `InstallInit` (`Utils/Module.cls:63-68`), so dependencies of `load`, `update`, `test` and `verify` read `load`. They carry the repository's name and the listing shows `n/a` for every action other than install (`General/History.cls:461-463`). A top-level `update` from a registry stores a source that the listing also hides.
- Governing source: the open question `history.fields.dependency-source` (`features/history.g5.md:95`). The `history.fields` bullet (`:79`) says sources are empty for load, which the provisional answer of Q-15 overrides for dependency records.
- Evidence: A0 (`RN/20261113T182759Z-cases-a`: dependency record `Action=load`, `SourceName=ipm-temp-modules-1`).
- Impact: users cannot see in the listing where a dependency came from.
- Decision: Q-15. Provisional answer: not a defect. Either alternative (label such records `install` with their source, or show any stored source in listings) makes the current behavior a defect in labeling or display.
- Missing evidence: registry and ORAS sources were not run.
- Direction: none until Q-15 is decided.
- Aliases: none.

## Issues without a governing requirement

### I1: An unfinished phase is shown as `ERROR #00: (no error description)`

- Expected: no source requires a display. wiki/09 L161 explains how to read a phase with no end time.
- Observed: `General/History.cls:420` applies `GetErrorText` to the placeholder status 0 of an open phase.
- Evidence: reader case R2 (`RN/20261113T184444Z-cases-hr`).
- Impact: an open phase reads like an error with no description.
- Decision: none. Round 1 disposition R1-MOD-12 moved it from defect D8 to this issue.
- Missing evidence: none.
- Direction: print "not finished" when the phase has no end time. Distinguishing check: R2 shows "not finished".
- Aliases: D8, R2.

### I2: A load ended mid-way leaves its temporary `.modules` repository registered at first priority

- Expected: `load.dependencies.local-repo` (`features/load.g5.md:189`) says IPM removes the temporary repository when loading finishes, even after an error. No source covers a process end.
- Observed: the repository is created before the transaction and removed only by the explicit `CleanUp()` (`Main.cls:2528-2530`), which a process end skips.
- Evidence: after the uplift's Terminate and SIGKILL replays, six `ipm-temp-modules-N` repositories with `OverriddenSortOrder=-1000` still pointed at the cases' `.modules` folders (`RN/20261113T210229Z-temp-repos-leftover`). They were recorded and then removed.
- Impact: later installs in the namespace resolve dependencies from those folders first.
- Decision: none, unless maintainers extend `load.dependencies.local-repo` to process ends.
- Missing evidence: none.
- Direction: register the temporary repository in a form that does not outlive the process, or remove stale `ipm-temp-modules-*` entries at the start of the next load. Design: S7.
- Aliases: none.

### I3 (new): A dependency installed in developer mode is silently replaced and leaves developer mode

- Expected: no source found. `dev-mode.overwrite-guard` (`concepts/transactions-and-dev-mode.g5.md:100`) protects a developer-mode module from a normal-mode install. Whether it covers `load` is open (Q-03).
- Observed: when a load needs another version of a dependency that is installed in developer mode, the dependency is replaced and its `DeveloperMode` cleared, with an OK status. Dependencies get `DeveloperMode` set to 0 (`Utils/Module.cls:309-311`). The guard at `:109` needs `params("Install")`, which `load` never sets (`Main.cls:2457-2460`) and `install` removes before the top load's activation (`Utils/Module.cls:318-319`). So the guard never protects a dependency. At the required version the dependency is skipped and keeps developer mode.
- Evidence: N-R05-DEVUP and the neighbour N-R05-DEVSAME (`FRN/20261113T234817Z-r05-modes-nesting`).
- Impact: a module a developer is changing in this namespace is replaced by a packaged version and loses its developer setup without a warning.
- Decision: a new question, or an extension of Q-03 (`dev-mode.overwrite-guard.force`): should the guard protect a developer-mode module from replacement as a dependency?
- Missing evidence: the `install` variant is code-read only. Remote sources were not run.
- Direction: apply the guard, or at least a warning, when a dependency load would replace a developer-mode module. Distinguishing check: N-R05-DEVUP is refused or warns. Design: S8 (the rule belongs in the load plan).
- Aliases: L-5, N-R05-DEVUP.

### I4 (new): The history class queries return no ID and no namespace

- Expected: no source states the columns. `history.namespace` (`features/history.g5.md:184`) says record IDs are unique only within a namespace.
- Observed: `Records` declares `CONTAINID = 1` while its first column is `Action` (`General/History.cls:26-28`). `GlobalRecords` has neither a namespace nor an ID column (`:233-235`).
- Evidence: static reading. The history readers model notes it.
- Impact: a program cannot open a row's details from the query result. Even with D20 fixed it cannot tell which namespace a `GlobalRecords` row came from.
- Decision: none.
- Missing evidence: a client that honors `CONTAINID` was not run.
- Direction: add the ID and a namespace column and point `CONTAINID` at the ID column. Design: S9.
- Aliases: H-5, OBS-H11.

### I5: Compile ignores a resource processor's error status

- Expected: no source states it for resource processors. For `<Invoke>`, `module.invoke` (`concepts/modules.g5.md:65`) says an error fails the phase.
- Observed: `%Compile` calls `do tResource.Processor.OnPhase("Compile",.pParams,.tHandled)` and discards the returned status (`Lifecycle/Base.cls:1081`).
- Evidence: code reading only.
- Impact: a processor's compile failure is reported as Compile SUCCESS.
- Decision: none.
- Missing evidence: not run.
- Direction: check the returned status and fail the phase on error, as other processor hooks do (`Storage/Module.cls:683-684`).
- Aliases: OBS-L8.

### I6: Files a load writes outside the database outlive its rollback

- Expected: under the provisional answer of Q-04 (`transaction.rollback-scope`, `concepts/transactions-and-dev-mode.g5.md:24`), a rollback promises only database writes. Files, the git clone folder and IRISTEMP data stay. So no source requires the cleanup below.
- Observed (code):
  - Archive loads delete and replace `<ModuleRoot>/<name>/<version>` before any check (`Main.cls:2507-2513`; `Utils/Module.cls:236-242` for repository installs). A same-version archive load that then fails rolls the database back and leaves the failed archive's files in the installed module's folder.
  - The lock file is written at the end of `LoadDependencies` (`Utils/Module.cls:1416-1422`), before the module's phases and commit, so it survives a rollback.
  - Git URL loads clone into `mgr/Temp/<timestamp>/` and never remove it (`Main.cls:2351-2372`). `RunCommandViaZF` returns OK whatever git returns (`Utils/Module.cls:2387-2416`), so a failed clone surfaces later as "File not found to obtain module object".
- Evidence: code reading. S3 observed an archive left extracted after a failed update (D16).
- Impact: the installed module's files can disagree with its database state. A lock file can describe an install that was rolled back. A failed clone gives a misleading error.
- Decision: Q-04. Under the alternative (restore the whole namespace), each item becomes a suspected defect.
- Missing evidence: none of these paths was run natively. Git URL loads have no native or formal evidence.
- Direction: unpack into a temporary folder and move it into place only after the commit. Write the lock file after the commit. Remove the clone folder when the load ends and fail on a non-zero git exit code.
- Aliases: OBS-L7, OBS-L11, OBS-L13.

### I7: `PermitDowngrade` reaches the dependency graph and one guard but not the downgrade refusal

- Expected: `load.refusal.downgrade` (`features/load.g5.md:104`) says nothing lifts the downgrade refusal. `PermitDowngrade` is undocumented except in `update`'s refusal (A3 disagreement 1, CAT-28).
- Observed: `BuildDependencyGraph` honors `PermitDowngrade` (`Storage/Module.cls:1157`) and `LoadModuleReference` skips its developer-mode overwrite guard when it is set (`Utils/Module.cls:103`). `LoadNewModule` refuses every downgrade (`:1213-1217`), so a dependency downgrade the graph accepted fails later with "Cannot downgrade".
- Evidence: code reading.
- Impact: the flag cannot permit the downgrade it names, yet it silently disables the developer-mode guard.
- Decision: none, unless maintainers document the flag.
- Missing evidence: not run.
- Direction: remove the flag, or document it and make the graph, the overwrite guard and the downgrade refusal agree.
- Aliases: OBS-L14.

### I8 (new): `-DNoLock=1` turns compile locking on

- Expected: no source defines `NoLock` (wiki/02 L447 lists it without a meaning, A1 conflict 30).
- Observed: the code sets `tNoCompileLock = '$get(pParams("NoLock"))` and adds the `-l` (no lock) compile qualifier only when `tNoCompileLock` is true (`Lifecycle/Base.cls:1194-1198`). Without the flag compiles run unlocked and with it they lock.
- Evidence: code reading.
- Impact: a user who sets `NoLock` to avoid locks gets them.
- Decision: none.
- Missing evidence: not run.
- Direction: invert the test, or document the actual meaning.
- Aliases: CAT-28 note, A1 conflict 30.

## Suspected (code reading only)

Each entry gives the reading, the governing source it would violate, and why it was not run. None was observed natively.

### OBS-L3: A plain load does not save `Root` and `Repository`, so the first lock file goes to the wrong folder

- Expected: `-create-lockfile` writes `module-lock.json` in the module root (`deps.lockfile`, `features/dependencies.g5.md:34`). `load.core` (`features/load.g5.md:56`) says install records where a module came from.
- Observed (code): the save at `Utils/Module.cls:1298-1304` runs only when `params("DeveloperMode")` is defined, which a plain load leaves undefined. The in-memory `Repository` (`:1296`) is discarded by `%Reload` (`:1332`). The lock file is written before that reload, from an empty `Root` (`General/LockFile.cls:82`), so on a first plain load it lands relative to the process's directory. A module first installed from a registry and then loaded from a folder keeps the old repository name.
- Impact: a misplaced lock file and a stale repository in `list-installed`.
- Missing evidence: not run (no case in scope covered lock files).
- Direction: save `Root` and `Repository` on every load path before the lock file is written.

### OBS-L4: `update -dev` from a registry ends in normal mode

- Expected: developer mode applies to `update` (`dev-mode`, `concepts/transactions-and-dev-mode.g5.md:70`). `update.dev-mode` (`features/update.g5.md:32`) excludes only dependencies.
- Observed (code): `Main:Install` sets `DeveloperMode` but no `Install` flag for an update (`Main.cls:2623-2630`) and `LoadModuleFromDirectory` then replaces it with `pOverrideDeveloperMode`, 0 by default (`Utils/Module.cls:258`, `:309-311`).
- Impact: the updated module leaves developer mode.
- Missing evidence: needs a registry, not run.
- Direction: pass the requested developer mode to the top-level module of an update.

### OBS-L5: A dependency first installed by `update` runs all its update steps

- Expected: `update` runs the steps needed to move from the current version to the new one (`update.steps`, `features/update.g5.md:42`). A module with no current version needs none and a normal install seeds them as done.
- Observed (code): `Update=1` propagates to dependency loads (`Utils/Module.cls:316-317`), which then run `ApplyUpdateSteps` (`:1344-1345`). `%Activate` skips seeding when `Update` is set (`Lifecycle/Module.cls:122`) and `HandleUpdateStepsFromList` runs every step from the first one not yet run (`Storage/Module.cls:186-195`).
- Impact: migration steps meant for upgrades run on a fresh install. Steps must be safe to rerun, which limits the harm.
- Missing evidence: needs a registry and update packages, not run.
- Direction: seed the steps of a dependency that was not installed before the update.

### OBS-L9: Update steps are skipped silently when the update lock times out

- Expected: `update.steps` requires the steps to run.
- Observed (code): the method takes `lock +^IPM.UpdateLock(name):3` (`src/inc/IPM/Common.inc:105`). On timeout it returns its OK status without running any step (`Storage/Module.cls:105-108`). No human source covers the timeout (A1 conflict 24).
- Impact: an update and its history record both report success while its steps did not run.
- Missing evidence: needs two concurrent processes, not run.
- Direction: return an error on timeout, or wait and report.

### OBS-L16: A failed legacy XML install reads `Committed=0` although it kept changes

- Expected: `Committed` shows whether changes were kept, including writes outside a transaction (`history.committed`, `features/history.g5.md:125`; `General/AbstractHistory.cls:41-42`).
- Observed (code): the legacy XML payload path (`Utils/Module.cls:175-184`) has no `SetCommitted` and `Finalize` sets `Committed` only from a successful status. This is the one caller that relies on `Finalize`'s "or OK" (model control 05 notes, round 1 finding R1-MOD-07).
- Impact: a failed legacy install that wrote classes reads uncommitted.
- Missing evidence: needs a registry serving a legacy XML package, not run.
- Direction: mark `Committed` before the legacy path writes (S3).

### OBS-L17: Uninstalling a module others require reports success

- Expected: IPM prevents uninstalling a module that installed modules depend on, unless `-force` (`uninstall.dependents`, `features/uninstall.g5.md:20`). `-recurse` keeps dependencies that others need (`uninstall.recurse`, `:25`). For uninstall, `Committed` shows success (`history.committed.uninstall`, `features/history.g5.md:141`).
- Observed (code): uninstall runs Clean with level 1 (`Storage/Module.cls:742`). `CheckBeforeClean` marks the module skipped with an OK status (`Lifecycle/Base.cls:290-294`) and `%Clean` returns OK (`Lifecycle/Module.cls:12-15`). The module stays while the command succeeds and the record reads success with `Committed=1`. A recursive uninstall finalizes each skipped dependency as a successful uninstall (`Lifecycle/Base.cls:471-484`). The maintainer test for the refusal is skipped as "does not yet exist" (`tests/integration_tests/Test/PM/Integration/InstallModule.cls:135`).
- Impact: the user and history both report an uninstall that did not happen.
- Missing evidence: uninstall paths were not run natively.
- Direction: return an error when Clean is skipped for dependents and record the skip as a failure or a distinct outcome.
- Aliases: OBS-H8.

### OBS-H10 (remainder): Namespaces without IPM are not reported as such by `-globally`

- Expected: with `-globally`, namespaces without IPM add no records (`history.read.global`, `features/history.g5.md:237`).
- Observed (code): `GetHistoryGlobally` skips a namespace only when `GetHistory` returns `$$$NULLOREF` (`General/History.cls:309-311`), but `%ExecDirect` returns an error result set instead. The message "IPM is not enabled for this namespace" (`:496-499`) is unreachable and such namespaces appear as empty groups.
- Impact: misleading output for instances with namespaces without IPM.
- Missing evidence: the follow-up container has no namespace without IPM that `-globally` would visit with rows elsewhere. Not run.
- Direction: skip namespaces that lack the class, as `DeleteHistoryGlobally` does (`:212`).

### OBS-H12 (Q-13): A purge failure stops the user's command

- Expected: under the provisional answer of `history.retention.effect` (`features/history.g5.md:307`), the user's command still runs.
- Observed (code): `Shell` returns the purge's error before running the command (`Main.cls:862-865`). `ShellInternal` and the API writers never purge. Because `DeleteHistoryGlobally` swallows per-namespace errors (D23), a blocking failure has to come from the namespace list query (`General/History.cls:206-207`) or the stamp.
- Impact: every IPM command fails while the purge fails.
- Decision: Q-13. Provisional answer: a defect, as a proposed change. Alternative: report the error and stop.
- Missing evidence: a purge failure was not run.
- Direction: log the purge error and continue with the command.

### OBS-H13 (Q-16): IPM self-uninstall with full cleanup deletes other namespaces' in-progress records

- Expected: under the provisional answer of `history.keep-on-ipm-uninstall.other-namespaces` (`features/history.g5.md:321`), uninstalling IPM in one namespace does not touch other namespaces' records.
- Observed (code): full cleanup kills `^IRIS.Temp.HistoryTempD` (`Lifecycle/Module.cls:51`). That global is shared by every namespace (N-P06, N-P17 confirmed natively), so in-progress records of loads running elsewhere lose their IRISTEMP copy and the ID counter restarts. A live log in another process may then reload a different log's row that reuses its ID.
- Trigger class: a full-cleanup self-uninstall of IPM in one namespace while a load runs in another.
- Impact: lost or mixed-up history for loads running elsewhere.
- Decision: Q-16. Provisional answer: a defect. Alternative: accept it, because removing IPM is rare.
- Missing evidence: concurrent loads were not run.
- Direction: delete only this namespace's IRISTEMP rows.

### OBS-H15: Self-uninstall records success before it runs and exits the shell even when skipped

- Expected: `Success` is the action's status (`history.fields`, `features/history.g5.md:79`). For uninstall, `Committed` shows success (`:141`). History survives IPM's own uninstall (`history.keep-on-ipm-uninstall`, `:316`).
- Observed (code): `Finalize($$$OK)` and `PersistToTwin` run before `%Clean` (`Storage/Module.cls:630-643`). The self-uninstall flag is set before the uninstall call (`Main.cls:2746-2749`), so the shell prints that IPM was uninstalled and exits (`Main.cls:1152-1156`) even when Clean is skipped because another module requires IPM.
- Impact: history and the shell report an uninstall of IPM that did not happen.
- Missing evidence: self-uninstall was not run. Only its persist step was probed: a second `PersistToTwin` on a finalized row fails with SQLCODE -132 and changes nothing. `%OnClose` still deletes the IRISTEMP row without raising (N-P16).
- Direction: finalize after Clean, using an explicit persist that does not depend on the classes Clean removes. Set the exit flag only on success.

### D23 (new): The global history delete hides every error, not only permission errors

- Expected: `-verbose` shows which namespaces were skipped for lack of permission (`history.delete`, `features/history.g5.md:253`). The code comment says the catch is for permission errors (`General/History.cls:213`, `:221`). A local delete with an unknown column is refused with SQLCODE -29 (E7).
- Observed (code and model): `DeleteHistoryGlobally` catches every exception in each namespace and continues (`General/History.cls:214-225`), so a global delete with an unusable column reports "Deleted 0 record(s)" without an error. The purge goes through the same method.
- Evidence: history readers model, `delete-global:STATUS` in 52 commands (`FR/20261114T002647Z-final-history-readers-model`).
- Impact: a user believes no record matched while the delete never ran.
- Missing evidence: not run natively. The follow-up container has no second IPM namespace and its ground rules exclude deletes beyond test rows.
- Direction: catch only permission errors and report every other error. Design: S9.
- Aliases: H-4.

### D29 (new, Q-09): Refused `update`, `reinstall` and `uninstall` attempts leave no record

- Expected: under the provisional answer of `history.record.unlogged-refusals` (`features/history.g5.md:49`), IPM records these refused attempts, as `history.actions` (`:32`) records refused loads and installs.
- Observed (code): `update` refuses `PermitDowngrade` and a module that is not installed (`Main.cls:4415-4424`) before it creates its log (`:4438`). `reinstall` refuses a module that is not installed (`Main.cls:2686-2687`) and `uninstall` refuses a missing name (`Main.cls:2710-2711`). Both happen before any log exists.
- Impact: refused updates leave no trace in history.
- Decision: Q-09. Provisional answer: a defect. Alternative (argument refusals are not attempts): intended.
- Missing evidence: not run.
- Direction (provisional answer): create the log before the argument checks, as `load` and `install` do.

### D30 (new): `uninstall -purge` sets a key nothing reads

- Expected: `-purge` deletes the data in the module's tables during uninstall (`uninstall.purge`, `features/uninstall.g5.md:40`; help `Main.cls:505`).
- Observed (code): the modifier sets `data("Purge")` (`Main.cls:505`), while the only reader takes `pParams("Configure","Purge")` (`Lifecycle/Base.cls:554`) and passes it to the installer class (`:561`). A1 conflict 28.
- Impact: the module's table data is kept although the user asked for it to be purged.
- Missing evidence: not run.
- Direction: map `-purge` to `Configure.Purge`, or read `Purge` in `%Unconfigure`.

## Withdrawn, refuted and out of scope

- D7, withdrawn: `history details` prints the raw `%Status`. wiki/09 L133 documents it on purpose ("so the ObjectScript stack is visible"). Round 1 disposition R1-ONT-13. Evidence R3 (`RN/20261113T184444Z-cases-hr`).
- OBS-H9, withdrawn: a global history delete (the `-globally` flag) needs no filter and no confirmation. This is the documented decision `history.delete.global-unfiltered` (`features/history.g5.md:271`) and the `-confirm` help ("Ignored if -globally is passed too"). Together with D2, a filtered global delete can also remove every record, which D2 covers.
- OBS-L15, out of scope: `LoadedResource` rows in IRISTEMP (`Document.cls:186`) survive a rolled-back load. Only the `@API` methods `Validate` and `ReportConflicts` read them and those entry points are excluded in `A/coverage-map.md` section 2. IRISTEMP data is outside the rollback under Q-04's provisional answer and `premises.md` P-10 excludes it.
- Refuted predictions, kept as contrary evidence:
  - OBS-L6 predicted that the other module's manifest stays after an `update -path` mismatch. S3 refuted it. The path is D16.
  - OBS-H6 predicted the start-value row after a Terminate. N-P10 refuted it (`%OnClose` runs at a Terminate). The SIGKILL part holds and is D10.
  - A1 INT-HIST-03 (d) said an abnormally ended record keeps `Finalized=0` and an orphaned IRISTEMP row. C1 and C2 refuted that for a Terminate (D10, D27).
  - Platform premise N-P10 and sub-claim N-P20C2 were refuted. They shape the D1 direction and are not product findings.

## Corrections to the original file

- OBS-H4 was listed as suspected although D5 confirmed it. It is now only an alias of D5.
- D6's run was "listed in `index.jsonl`". It is `RN/20261113T184634Z-repro-activate-invoke-phase-open`. D6's throw is at `Storage/Module.cls:707`, not `:704`.
- D13 cited `Main.cls:2529` for `CleanUp()`. The call is at `:2530`.
- D14 was listed under "Confirmed". Under the provisional answer of Q-22 the escape is intended, so it is now a conditional finding that is a defect only under the alternative.
- The three deviation rows had only Q labels. They are now D25 (Q-05), D26 (Q-08) and D27 (Q-21).
- The original repair proposal for D1 said variant 103 meets every dependency clause. The follow-up showed that holds only when every activation uses the same mode (L-9), found a Terminate window that the proposed ordering leaves open (L-12), and confirmed the ordering rule natively (P-14). D1's direction now carries these.
- OBS-L7, OBS-L11 and OBS-L13 were suspected. Under Q-04's provisional answer no source requires their cleanup, so they are now issue I6. OBS-L14 is now issue I7, because the only governing block supports the refusal. OBS-L15 is now out of scope.
- OBS-L3, OBS-L8, OBS-L16, OBS-H7 and OBS-H11 had no disposition in the original file. They are now OBS-L3 (suspected), I5, OBS-L16 (suspected), OBS-H7 (conditional) and I4.
- Historical files are not edited. `A/coverage-map.md` still says "reader defect D8" and "confirmed defect D6". `B/model-notes.md` V12 still says B1-C51 and C52 were not run (they ran in `RN/20261113T205137Z-replay-c51-c52`).

## Appendix A: ID reconciliation

The registry is `.g5/uplift/ipm-quality-followup/work/id-registry.json` (835 objects). "Where" names the finding in this file.

### Coverage

| Family | Accounted for | Denominator |
|---|---|---|
| A2-OBS | 34 | 34 |
| defect (D1 to D20, including D7 and D8) | 20 | 20 |
| issue (I1, I2) | 2 | 2 |
| model-finding (V1 to V12) | 12 | 12 |
| Deviation rows of the original file | 3 | 3 |
| B1 cases that violate the final contract or are approximate | 36 | 36 (of 59 B1 cases; the other 23 are permitted and raise no finding) |
| Other registry native cases that deviate, are incomplete, refuted, observational or controls | 39 | 39 (of 87 non-B1 native cases) |
| Conforming registry native cases cited as evidence | 8 | 8 (the other 40 conform and raise no finding) |
| Follow-up native cases | 21 | 21 |
| Follow-up brief items (L-1 to L-12, H-1 to H-8, E-1 to E-5) | 25 | 25 |

### A.1 A2 observations

| ID | Family | Disposition in this file | Where |
|---|---|---|---|
| OBS-L1 | A2-OBS | alias, supported | D1 |
| OBS-L2 | A2-OBS | alias, supported | D15 |
| OBS-L3 | A2-OBS | finding, suspected | OBS-L3 |
| OBS-L4 | A2-OBS | finding, suspected | OBS-L4 |
| OBS-L5 | A2-OBS | finding, suspected | OBS-L5 |
| OBS-L6 | A2-OBS | alias, supported; its prediction refuted by S3 | D16 |
| OBS-L7 | A2-OBS | alias, issue (was suspected) | I6 |
| OBS-L8 | A2-OBS | alias, issue (no prior disposition) | I5 |
| OBS-L9 | A2-OBS | finding, suspected | OBS-L9 |
| OBS-L10 | A2-OBS | alias, conditional | D4 |
| OBS-L11 | A2-OBS | alias, issue (was suspected) | I6 |
| OBS-L12 | A2-OBS | alias, supported | D13 |
| OBS-L13 | A2-OBS | alias, issue (was suspected) | I6 |
| OBS-L14 | A2-OBS | alias, issue (was suspected) | I7 |
| OBS-L15 | A2-OBS | out of scope | Withdrawn, refuted and out of scope |
| OBS-L16 | A2-OBS | finding, suspected (no prior disposition) | OBS-L16 |
| OBS-L17 | A2-OBS | finding, suspected | OBS-L17 |
| OBS-H1 | A2-OBS | alias, supported | D2 |
| OBS-H2 | A2-OBS | alias, conditional | D18 |
| OBS-H3 | A2-OBS | alias, supported | D3 |
| OBS-H4 | A2-OBS | alias, supported | D5 |
| OBS-H5 | A2-OBS | alias, supported | D1 |
| OBS-H6 | A2-OBS | alias, supported (renamed to D10); Terminate prediction refuted | D10 |
| OBS-H7 | A2-OBS | finding, conditional (Q-15) | OBS-H7 |
| OBS-H8 | A2-OBS | alias, suspected | OBS-L17 |
| OBS-H9 | A2-OBS | withdrawn, documented decision | Withdrawn, refuted and out of scope |
| OBS-H10 | A2-OBS | split: wrong rows alias of D20, remainder suspected | D20, OBS-H10 (remainder) |
| OBS-H11 | A2-OBS | alias, issue (no prior disposition) | I4 |
| OBS-H12 | A2-OBS | finding, suspected | OBS-H12 |
| OBS-H13 | A2-OBS | finding, suspected | OBS-H13 |
| OBS-H14 | A2-OBS | alias, conditional | D6 |
| OBS-H15 | A2-OBS | finding, suspected | OBS-H15 |
| OBS-H16 | A2-OBS | alias, conditional | D25 |
| OBS-X1 | A2-OBS | alias, supported | D19 |

### A.2 Defects, issues, model findings and deviation rows

| ID | Family | Disposition in this file | Where |
|---|---|---|---|
| D1 | defect | finding, supported | D1 |
| D2 | defect | finding, supported | D2 |
| D3 | defect | finding, supported | D3 |
| D4 | defect | finding, conditional (Q-17) | D4 |
| D5 | defect | finding, supported | D5 |
| D6 | defect | finding, conditional (Q-14) | D6 |
| D7 | defect | withdrawn (wiki/09 L133, R1-ONT-13) | Withdrawn, refuted and out of scope |
| D8 | defect | reclassified as I1 (R1-MOD-12) | I1 |
| D9 | defect | finding, supported | D9 |
| D10 | defect | finding, supported | D10 |
| D11 | defect | finding, supported | D11 |
| D12 | defect | finding, supported | D12 |
| D13 | defect | finding, supported | D13 |
| D14 | defect | finding, conditional (Q-22), defect only under the alternative | D14 |
| D15 | defect | finding, supported | D15 |
| D16 | defect | finding, supported | D16 |
| D17 | defect | finding, supported | D17 |
| D18 | defect | finding, conditional (Q-18) | D18 |
| D19 | defect | finding, supported | D19 |
| D20 | defect | finding, supported | D20 |
| I1 | issue | finding, issue | I1 |
| I2 | issue | finding, issue | I2 |
| V1 | model-finding | alias | D1 |
| V2 | model-finding | alias | D1 |
| V3 | model-finding | alias | D4 |
| V4 | model-finding | alias | D12 |
| V5 | model-finding | alias | D6 |
| V6 | model-finding | alias | D13 |
| V7 | model-finding | alias | D9 |
| V8 | model-finding | alias | D25 |
| V9 | model-finding | alias | D26 |
| V10 | model-finding | alias | D27 |
| V11 | model-finding | alias | D10 |
| V12 | model-finding | alias | D11 |
| Q-05 (deviation row) | other | renumbered | D25 |
| Q-08 (deviation row) | other | renumbered | D26 |
| new Q (round 1) (deviation row, = Q-21, Q-FIN) | other | renumbered | D27 |

### A.3 B1 cases that violate the final contract or are approximate

Final verdicts are against `cases.json` `10410864…`, re-evaluated with the corrected evaluator (`FR/20261114T002646Z-final-replay-reeval`, 0 verdicts changed).

| ID | Family | Disposition in this file | Where |
|---|---|---|---|
| B1-C08 | native-case | alias | D12 |
| B1-C09 | native-case | alias (two clauses) | D12, D9 |
| B1-C10 | native-case | alias (two clauses) | D11, D9 |
| B1-C11, B1-C12, B1-C13, B1-C14, B1-C15, B1-C18 | native-case | alias | D9 |
| B1-C23, B1-C24 | native-case | alias | D4 |
| B1-C28 | native-case | alias | D6 |
| B1-C29 | native-case | alias | D13 |
| B1-C31, B1-C32, B1-C33, B1-C36, B1-C37 | native-case | alias | D1 |
| B1-C42, B1-C43, B1-C50K, B1-C50T | native-case | alias | D25 |
| B1-C44K, B1-C44T | native-case | alias | D10 |
| B1-C45K, B1-C45T, B1-C47K, B1-C47T | native-case | alias (dependency row and top fields) | D1, D10 |
| B1-C46K, B1-C48K | native-case | alias (`Committed` and top fields) | D26, D10 |
| B1-C46T, B1-C48T | native-case | alias | D27 |
| B1-C49K | native-case | alias, APPROXIMATE | D26, D10 |
| B1-C49T | native-case | alias, APPROXIMATE | D26, D27 |
| B1-C51 | native-case | alias | D9 |
| B1-C52 | native-case | alias (two clauses) | D11, D9 |

The 23 permitted B1 cases (C01 to C07, C16, C17, C19 to C22, C25 to C27, C30, C34, C35, C38 to C41) raise no finding. B1-C19, C20 and C40 were reported as violations only in the superseded first replay attempt (`RN/20261113T193745Z-replay-return`) and the corrected evaluator marks those attempt-1 verdicts INDETERMINATE.

### A.4 Other registry native cases

| ID | Family | Disposition in this file | Where |
|---|---|---|---|
| A1, A2, A3 | native-case | alias | D1 |
| B2, B3, B4 | native-case | alias | D4 |
| C1, C3 | native-case | alias | D10 |
| C2 | native-case | alias | D27 |
| C4 | native-case | INCOMPLETE, superseded by C4R and B1-C46K | D10, D26 |
| C4R | native-case | alias (phases and package; `Committed`) | D10, D26 |
| D1 (case) | native-case | alias | D25 |
| E1, E2, E4, E8 | native-case | alias | D2 |
| F0 | native-case | alias | D5 |
| F1 | native-case | alias | D3 |
| H1 | native-case | no verdict row, mutation control of the reader run | not a finding |
| R3 | native-case | basis of withdrawn D7 | Withdrawn, refuted and out of scope |
| R4 | native-case | alias | D9 |
| X2, X3 | native-case | alias (reproduction guards) | D2 |
| S1 | native-case | alias | D15 |
| S2, S2B | native-case | alias | D11 |
| S3 | native-case | alias | D16 |
| S4 | native-case | INCOMPLETE binding, replaced by S4R | D17 |
| S4R | native-case | alias | D17 |
| M5 | native-case | alias (namespace; dependency row; parent `Committed`) | D14, D1, D22 |
| M5N | native-case | alias | D1 |
| H2 | native-case | alias (filters; find status) | D18, D24 |
| X1 | native-case | alias | D19 |
| H10 | native-case | alias | D20 |
| N-P10 | native-case | refuted platform premise, contrary evidence | D10 |
| N-P15 | native-case | observation under Q-20 (a deleted in-progress record is not recreated); the provisional answer permits it | not a finding |
| N-P20 | native-case | premise R-01 confirmed, sub-claim c2 refuted | D1 direction |
| N-P20AN | native-case | control, detected | D1 direction |
| N-P20C2 | native-case | refuted sub-claim, shapes the write order | D1 direction |
| A0 | native-case | conforms; evidence | OBS-H7 |
| E3 | native-case | conforms to the phase 1 expectation; evidence | D24 |
| E7 | native-case | conforms; neighbour | D23, D24 |
| g3, g6, g7 | native-case | conform to the phase 1 expectations; evidence | D9 |
| R2 | native-case | conforms; evidence | I1 |
| S3N1 | native-case | conforms on the checked fields; message evidence | D16 |

Conforming with no finding (40): premise checks N-P01 to N-P09, N-P11 to N-P14, N-P16 to N-P19 and N-P20C; neighbours and valid completions A4, A5, B0, B1 (case), C0, D0, E0, E5, E6, R1, S1N, S2N, S3N2, S4N1, S4N2; version steps g1, g2, g4, g5, g8, g9, g10.

### A.5 Follow-up native cases and brief items

| ID | Family | Disposition in this file | Where |
|---|---|---|---|
| N-P21A | follow-up native | DEVIATES from its author's expectation; premise P-14 | D1 direction |
| N-P21AN, N-P21B | follow-up native | controls (N-P21B INCOMPLETE in the first run, CONFORMS in r2) | D1 direction |
| N-P21R | follow-up native | DEVIATES; repair ordering hazard | D1 direction |
| N-R04-SEL, N-R04-SEL2 | follow-up native | conform to the as-built prediction, which violates the contract | D2 |
| N-R04-CUTOFF | follow-up native | same | D3 |
| N-R04-UNKNOWN | follow-up native | same | D24 |
| N-R04-FIND, N-R04-ORDER, N-R04-DEL | follow-up native | valid neighbours, conform to the contract | no finding |
| N-R05-NESTF | follow-up native | conforms to the as-built prediction | D21, D22, D1, D14 |
| N-R05-NESTS | follow-up native | success neighbour | D21 |
| N-R05-Q23F | follow-up native | conforms to the as-built prediction | D28, D1 |
| N-R05-Q23S | follow-up native | success neighbour | D28 |
| N-R05-DEVUP | follow-up native | conforms to the as-built prediction | I3 |
| N-R05-DEVSAME | follow-up native | neighbour | I3 |
| N-R05-PARDEF | follow-up native | conforms; mode resolution (L-1) | D14 |
| N-R05-DIA, N-R05-HIGHER, N-R05-HIGHEROK | follow-up native | conform, no deviation (L-3, L-6) | no finding |
| L-1, L-3 | brief | mechanism | D14, D21 |
| L-2 | brief | finding | D28 |
| L-4 | brief | finding | D21 |
| L-5 | brief | finding | I3 |
| L-6 | brief | no deviation | no finding |
| L-7 | brief | the effect-prefix clause holds for the as-built product under its own policy (62,764 end states, 0 violations; 5,328 differ transiently during the cleanup window) | no finding |
| L-8 | brief | finding | D22 |
| L-9, L-10, L-11, L-12 | brief | corrections and constraints of a repair proposal, not product defects | D1 direction (D10 by analogy) |
| H-1 | brief | mechanism | D2 |
| H-2, H-7 | brief | mechanism and repair consequence | D3 |
| H-3 | brief | finding | D24 |
| H-4 | brief | finding | D23 |
| H-5 | brief | finding | I4 |
| H-6 | brief | documented (`history.namespace`) | no finding |
| H-8 | brief | valid neighbour | no finding |
| E-1 to E-5 | brief | about the verification artifacts, not the product | not a product finding |

## Appendix B: Sampling record

The follow-up was asked to sample the whole intent inventory (`.g5/uplift/ipm-verus-20261113/A/intent-inventory.md`) for problems the uplift had not filed. Checks were code reading at `571e04a1` against the commitment, plus the existing native and model evidence. No product code was run for this sampling.

### Load commitments (18 of 18 checked)

| Commitment | Found |
|---|---|
| INT-LOAD-01 path forms | D15, I6 (git clone). The git URL and `-branch` are passed to a shell command unquoted (`Main.cls:2356-2357`). Not filed: it needs the caller's own OS callout privilege and is outside this file's scope |
| INT-LOAD-02 path refusals | D17 |
| INT-LOAD-03 archive unpacking | I6. The temporary extraction folder is removed on failure (`Main.cls:2516-2519`), as committed |
| INT-LOAD-04 shared core | OBS-L16, I3 |
| INT-LOAD-05 `-dev` keeps entries | D4. Otherwise conforms (B0, A5, B1-C26) |
| INT-LOAD-06 developer-mode commits | conforms (A2, A5, B1-C33, B1-C38); D1 for the dependency record |
| INT-LOAD-07 developer mode not propagated | conforms (A4, B1-C39); OBS-L4, I3 |
| INT-LOAD-08 transactions and flags | D14, D19, D21, D22, D28, I6 |
| INT-LOAD-09 downgrade refusal | conforms (g6, g7, B1-C11 to C14); I7 |
| INT-LOAD-10 update-required refusal | conforms. Q-02's provisional answer matches the product (g3 to g5, B1-C15 to C18) |
| INT-LOAD-11 system requirements | D4 |
| INT-LOAD-12 developer-mode overwrite guard | I3, I7 (`PermitDowngrade` disables the guard) |
| INT-LOAD-13 read-only repository forces normal mode | no new finding. The forcing follows the update guard (`Utils/Module.cls:1277-1288`), which is Q-02's concern |
| INT-LOAD-14 temporary `.modules` repository | D13, I2. The same `CleanUp` pattern in `Utils/Module.cls:256-259` is noted under D13 |
| INT-LOAD-15 other modifiers | OBS-L3, I6 (lock file). `-skip-sync` and `-env` not checked |
| INT-LOAD-16 write check | no new finding. The archive path relies on directory-creation errors after it removed the old folder (`Main.cls:2507-2510`), which I6 covers |
| INT-LOAD-17 every load has a record | holds for top-level loads outside a caller transaction (`theorem_top_record_durable`); D1, D25 |
| INT-LOAD-18 same version allowed | conforms (g2, B1-C19) |

### History commitments (17 of 17 checked)

| Commitment | Found |
|---|---|
| INT-HIST-01 actions recorded | D29. The module that `verify` installs into the VERIFY namespace gets no record while its dependencies do (OP-LOAD-28). Not filed: `history.actions` excludes lifecycle phase commands and the ontology notes it under `build.verify` |
| INT-HIST-02 failures recorded, survive rollback | D1, D25 |
| INT-HIST-03 record at start, abnormal end | D10, D26, D27; INT-HIST-03 (d) refuted for a Terminate |
| INT-HIST-04 fields | D9, OBS-H7 |
| INT-HIST-05 `Committed` | D12, D22, D26, OBS-L16 |
| INT-HIST-06 uninstall `Committed` | OBS-L17 |
| INT-HIST-07 `Success` versus `Committed` | no new finding |
| INT-HIST-08 dependency records | D1, OBS-H7 |
| INT-HIST-09 when a record exists | D29 |
| INT-HIST-10 `history` actions and filters | D2, D18, D24. The help example spells `-DSucess` (A1 conflict 2), a documentation error not filed here |
| INT-HIST-11 `-globally` and the API | D20, I4, OBS-H10 (remainder) |
| INT-HIST-12 delete | D2, D23. `-confirm` with a filter deletes only matching rows, as Q-11's provisional answer says |
| INT-HIST-13 retention | D3, D5, OBS-H12 |
| INT-HIST-14 finalized records locked | conforms (N-P13, `theorem_r2`); D27 |
| INT-HIST-15 history survives IPM uninstall | OBS-H13, OBS-H15 |
| INT-HIST-16 per-namespace history | conforms; OBS-H13 for the shared IRISTEMP global |
| INT-HIST-17 audit database | future option, nothing to check |

### Catalog rows (24 of 42 checked)

| Row | Found |
|---|---|
| CAT-04 defaults and parameters | D14, D21, D22, D28 |
| CAT-05 system requirements | D4 |
| CAT-06 Invoke | D6 |
| CAT-07 resource processors | I5 |
| CAT-09 update steps | OBS-L5, OBS-L9. The primary-only step promise (`General/Update/VersionBase.cls:18`) has no implementation, but mirroring is documented as future work (wiki/08 L5). Not filed |
| CAT-10 mapping and placement | `${authUnauthenticated}` is documented and not implemented (A1 conflict 25). Documentation, out of scope |
| CAT-11 lifecycle phases | the help lists a `register` phase that does not exist (A1 conflict 26). Documentation, out of scope |
| CAT-12 load | see the load commitments |
| CAT-13 install | I3; Q-03 |
| CAT-14 reinstall | the product pins the installed version, which matches Q-19's provisional answer. No finding |
| CAT-15 update | OBS-L4, OBS-L5, OBS-L9, D16, D29 |
| CAT-16 uninstall | OBS-L17, D30 |
| CAT-18 dependency resolution | conforms for higher installed versions (N-R05-HIGHER, N-R05-HIGHEROK) |
| CAT-20 repository configuration | the help shows `-app`, `-local`, `-info` and `-p4` without an implementation (A1 conflict 29). Documentation, out of scope |
| CAT-23 filesystem repositories | D13, I2 |
| CAT-26 settings | D5 |
| CAT-27 default modifiers | D18, D19 |
| CAT-28 install flags | I7, I8 |
| CAT-30 developer mode | see INT-LOAD-05 to -07 and -12; I3 |
| CAT-33 verify | the verified module gets no record (see INT-HIST-01). Not filed |
| CAT-35 lock files | OBS-L3, I6 |
| CAT-36 authoring utilities | the `run-from-file` help describes the wrong JSON shape (A1 conflict 32). Documentation, out of scope |
| CAT-41 scripting entry points | OBS-H12 |
| CAT-42 history | see the history commitments |

Not sampled: CAT-01, 02, 03, 08, 17, 19, 21, 22, 24, 25, 29, 31, 32, 34, 37, 38, 39 and 40 (18 rows). They cover module identity, resources and scope, Python dependencies, orphans, versions, the default registry, ORAS, publishing, discovery, namespace enabling, sync, packaging, deployed code, environment files, the installer, test output and analytics. A1 records documentation conflicts for some of them (section 6, items 15, 31 and 33 to 37). None was checked against the code here.
