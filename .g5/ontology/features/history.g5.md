# Transaction history

IPM keeps a log of what it did to each namespace, so users can find out what changed, when, who ran it and why something failed. This page is in the formal scope of the uplift, together with [loading](load.g5.md).

```
action starts ──► record created (end time empty, placeholder version)
     │                 │ kept in IRISTEMP while the action runs,
     │                 │ so a rollback cannot erase it
     ▼                 ▼
 phases run ─────► phase entries added; Committed set at each commit
     │
action ends ────► Success and end time set; record finalized and
                  copied to the namespace's permanent log (now read-only)
```

## Definitions

- **Action**: one `install`, `load`, `uninstall` or `update` that IPM attempts, including a dependency install it starts.
- **Record**: the log entry for one action.
- **Finalized**: a record whose action has ended and whose result IPM has written to the permanent log.
- **Phase entry**: the name, start time, end time and status of one lifecycle phase the action ran.
- **Kept changes**: changes an action made that are still there after it ends, either committed or written outside a transaction.

## What is recorded

<a id="history"></a>
[I · history]

IPM keeps a log, per namespace, of the package changes it attempts, whether or not they succeed.

<a id="history.actions"></a>
[I · history.actions]

IPM records every `install`, `load`, `uninstall` and `update` it attempts.

- Attempts that fail or are refused are recorded too, for example a load of a folder with a broken manifest.
- Installs that IPM starts for dependencies get their [own records](#history.dependencies).
- Other commands, such as `list`, `sync`, `reload` and lifecycle phase commands, are not recorded.

<a id="history.record"></a>
[I · history.record]

IPM creates a record as soon as an action starts and fills it in when the action ends. A process that dies mid-action still leaves a record, with an empty end time and a placeholder version.

<a id="history.record.unlogged-refusals"></a>
[Q · history.record.unlogged-refusals]

Should IPM record `update` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?

- Provisional answer: not required. These refusals happen before the action starts, for example `update` of a module that is not installed, or `uninstall` with no name.
- Alternative: record every attempt, following the original request to log failed installations.

<a id="history.record.abnormal-end"></a>
[Q · history.record.abnormal-end]

What should the record of an action that never ended say about kept changes?

- Provisional answer: an empty end time means the outcome is unknown. Readers must not trust `Committed` or the phase list on such a record.
- Alternative: write `Committed` to the permanent record at each commit, so it stays accurate after a crash.

<a id="history.survives-rollback"></a>
[I · history.survives-rollback]

A record survives the rollback of its own action, so failures stay visible afterwards.

<a id="history.survives-rollback.temp-copy"></a>
[D · history.survives-rollback.temp-copy]

IPM keeps the in-progress record in IRISTEMP, which a rollback does not undo, and copies the result to the namespace's permanent log outside the action's transaction.

<a id="history.fields"></a>
[I · history.fields]

Each record holds:

- `Action`: `load`, `install`, `uninstall` or `update`.
- `Package`: the module name. It may be empty if the action failed before the name was known.
- `Version`: the module version. It holds a placeholder, shown as `<unset>`, until the version is known.
- Source name, type and details: the repository an install came from. They are empty for load and uninstall.
- `TimeStart` and `TimeEnd`: the end time is empty while the action runs or after it ended abnormally.
- `UserName`: the user who started the action.
- `Success`: the action's status. It is `0` while the action is in progress.
- `Committed`: whether the action kept changes ([committed](#history.committed)).
- `CommandString`: the command as typed.
- `Phases`: the [phase entries](#history.phases).

<a id="history.fields.dependency-source"></a>
[Q · history.fields.dependency-source]

How should the log show a dependency that IPM pulled from a repository while running a `load` or `update`?

- Provisional answer: keep the record and its source. Label such records `install`, or show their source in the listing.
- Reason: the source field's description covers top-level actions, and hiding a real source loses useful history.
- Alternative: clear the source of records labeled `load`.

<a id="history.phases"></a>
[I · history.phases]

Each record lists the lifecycle phases its action ran, with each phase's start time, end time and status. A phase with no end time never finished, which points to a hang or crash in that phase.

- Load and install record Initialize, Reload, Validate, Compile and Activate. Update also records ApplyUpdateSteps, and uninstall records Clean.
- A failed action lists the phases up to and including the one that failed.

<a id="history.phases.returned-errors"></a>
[Q · history.phases.returned-errors]

Must every phase that starts also record its end and status, including when an `<Invoke>` returns an error status instead of throwing one?

- Provisional answer: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one.
- Alternative: accept open phases for returned errors and document it.

## Committed and Success

<a id="history.committed"></a>
[I · history.committed]

For `load`, `install` and `update`, `Committed` shows whether the action kept any changes, either by a commit or by writing outside a transaction. An uncommitted action wrote nothing or was rolled back.

- A successful action is committed.
- A failure rolled back in full is not committed. This includes a developer-mode failure before its [first commit](load.g5.md#load.outcome.dev.early-failure).
- A developer-mode failure after its first commit is committed.
- A failure with `-DNoTransaction=1` or `-DNoJournal=1` is committed once the action has started writing.
- A refusal before anything is written is not committed, in any mode.

<a id="history.committed.marking"></a>
[D · history.committed.marking]

IPM sets `Committed` at each commit, or just before the first write when there is no transaction. It does not work it out afterwards from developer mode and the final status.

<a id="history.committed.uninstall"></a>
[I · history.committed.uninstall]

Uninstall runs without a transaction, so for uninstall `Committed` only shows whether it succeeded. A failed uninstall that removed some resources still reads uncommitted.

<a id="history.committed.vs-success"></a>
[I · history.committed.vs-success]

`Success` and `Committed` answer different questions, because a failed action can keep changes. A failed but committed record is the one to look for when a namespace is in an unexpected state.

<a id="history.committed.nested"></a>
[Q · history.committed.nested]

Is `Committed` correct when IPM's commits sit inside another transaction? This happens when a caller wraps an IPM command in its own transaction, or for a developer-mode dependency loaded inside its parent's transaction.

- Provisional answer: `Committed` is exact only for actions started outside any transaction. Record this limit, because an outer rollback undoes an inner commit.
- Reason: the fix chose marking at each commit over comparing transaction levels, which would also handle nesting.
- Alternative: compare the transaction level at the end with the level at the start.

<a id="history.committed.side-effects"></a>
[Q · history.committed.side-effects]

Does `Committed` cover changes outside the database, such as copied files or pip installs?

- Provisional answer: no. It is defined by database commits and by writes outside a transaction.
- Alternative: also count file and operating-system changes.

## Dependencies and namespaces

<a id="history.dependencies"></a>
[I · history.dependencies]

Installs that IPM starts for dependencies are recorded as their own records, with their own phases.

<a id="history.dependencies.parent-rollback"></a>
[Q · history.dependencies.parent-rollback]

Must a dependency's record survive when the parent action rolls back?

- Provisional answer: yes. The original request asks to record dependency installs and to keep history across rollbacks.
- Alternative: accept that such records are lost, and document it.

<a id="history.namespace"></a>
[I · history.namespace]

Each namespace has its own log. IPM writes a record in the namespace where the action runs, and record IDs are unique only within a namespace.

## Reading the log

<a id="history.read"></a>
[I · history.read]

The `history` command (alias `log`) reads and manages the log with the actions `find` (the default), `details <id>`, `delete` and `schema`. Any other action is an error.

<a id="history.read.find"></a>
[I · history.read.find]

`history find` lists records in the current namespace, newest first.

- `-sort asc` or `-sort desc` sets the order by record ID.
- `-limit <n>` shows at most n records, and `0` shows all. Without it, IPM uses the `DefaultLogEntryLimit` setting, 20 by default.

<a id="history.read.filter"></a>
[I · history.read.filter]

`-D<column>=<value>` filters records, and several filters must all match.

- A value can use `*` as a wildcard, or start with `>`, `>=`, `<`, `<=` or `<>`.
- `history schema` lists the columns that can be filtered.
- Versions are filtered by part, such as `Version_Major`.
- Filters work the same way for `find` and `delete`.

<a id="history.read.filter.invalid-column"></a>
[Q · history.read.filter.invalid-column]

What should happen to a filter on a column such as `Version_Major`, or on a column that does not exist?

- Provisional answer: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter.
- Reason: a dropped filter widens `history delete`, which could then delete a whole namespace's records without `-confirm`.
- Alternative: refuse an unknown column with an error, or ignore it with a warning and apply the rest.

<a id="history.read.filter.default-modifiers"></a>
[Q · history.read.filter.default-modifiers]

Should the namespace's [default modifiers](../concepts/settings.g5.md#settings.default-modifiers) act as `history` filters?

- Provisional answer: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests.
- Reason: a default such as `NoTransaction` names no history column, so it would break or empty every listing.
- Alternative: apply them as filters and document it.

<a id="history.read.details"></a>
[I · history.read.details]

`history details <id>` shows every field of one record in the current namespace, including full repository details and the raw status. `-phases` adds each phase's timing and status. An unknown ID is an error.

<a id="history.read.global"></a>
[I · history.read.global]

`-globally` reads every namespace on the instance and groups the records by namespace.

- Namespaces without IPM add no records.
- `-limit` applies to each namespace, not to the total.
- `history details` always reads the current namespace.

<a id="history.read.api"></a>
[I · history.read.api]

Programs can read the log through the SQL table `%IPM_General.History` and the class queries `Records` (current namespace) and `GlobalRecords` (all namespaces).

## Deleting and keeping records

<a id="history.delete"></a>
[I · history.delete]

`history delete` deletes the records that match its filters.

- Deleting every record in the namespace, with no filter, requires `-confirm`.
- With `-globally`, it deletes in every namespace and does not require `-confirm`. With no filter, that deletes every record on the instance without asking.
- `-verbose` shows how many records each namespace lost and which namespaces were skipped for lack of permission.

<a id="history.delete.confirm-with-filter"></a>
[Q · history.delete.confirm-with-filter]

Does `-confirm` together with a filter delete every record, or only the matching ones?

- Provisional answer: only the matching ones. `-confirm` only permits a delete without a filter.
- Reason: the wiki describes it this way, while the command help says `-confirm` "will delete all records in the current namespace".
- Alternative: follow the command help.

<a id="history.delete.global-unfiltered"></a>
[Q · history.delete.global-unfiltered]

Should `history delete -globally` with no filter require `-confirm`?

- Provisional answer: no. Keep the documented behavior.
- Alternative: require `-confirm`, as a proposed safety change.

<a id="history.locked"></a>
[I · history.locked]

Once a record is finalized, it cannot be edited through SQL `UPDATE` or by saving the object. Records can only be added or deleted, which protects the log from tampering.

<a id="history.retention"></a>
[I · history.retention]

`config set HistoryRetain <days>` makes IPM delete records that started more than that many days ago. `0`, the default, turns this off, and the log then grows without limit.

- The purge runs on any IPM command, at most once per calendar day.
- It applies to every namespace, not just the current one.

<a id="history.retention.effect"></a>
[Q · history.retention.effect]

Must the daily purge really delete old records, and may a purge failure stop the user's command?

- Provisional answer: the purge must delete records older than the limit, judged by start time. A purge failure should not stop the command, which is a proposed change.
- Reason: no source says a purge failure may block other commands.

<a id="history.keep-on-ipm-uninstall"></a>
[I · history.keep-on-ipm-uninstall]

The log survives uninstalling IPM itself. Full cleanup removes all modules and IPM's metadata except the history log.

<a id="history.keep-on-ipm-uninstall.other-namespaces"></a>
[Q · history.keep-on-ipm-uninstall.other-namespaces]

May uninstalling IPM in one namespace delete the in-progress records of other namespaces?

- Provisional answer: no. History belongs to each namespace.
- Alternative: accept it, because removing IPM is rare.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) (`CAT`, `INT`, `Q`), [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md) (`OP`, `OBS`, `P`) and the [coverage map](../../uplift/ipm-verus-20261113/A/coverage-map.md). Every block on this page is in the formal scope, and its model and native evidence are pending stage B. History classes: `General/AbstractHistory.cls`, `General/HistoryTemp.cls`, `General/History.cls`, `General/HistoryPhase.cls`; tests in `tests/integration_tests/Test/PM/Integration/History.cls`.

### history

- human-provided
- user-facing-behavior
- test-coverage
- Children: [actions](#history.actions), [record](#history.record), [survives rollback](#history.survives-rollback), [fields](#history.fields), [phases](#history.phases), [committed](#history.committed), [dependencies](#history.dependencies), [namespace](#history.namespace), [reading](#history.read), [delete](#history.delete), [locked](#history.locked), [retention](#history.retention), [IPM uninstall](#history.keep-on-ipm-uninstall).

Sources: `wiki/09` L14-22; `Main.cls:818-821`; issue #366 by isc-tleavitt ("The user persona here is a user of IPM looking for history on their own instance"); CHANGELOG 0.10.5 (`#578`). Inventory: CAT-42. Test: `TestSimpleHistory`. Future option, not a commitment: issue #366 asked to investigate the IRIS audit database (INT-HIST-17). Nothing implements it.

### history.actions

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).

Sources: `Main.cls:820`; `wiki/09` L18; issue #366; PR #578 review r2557338086 by isc-dchui ("install, load, uninstall, and update seem most important"). Inventory: INT-HIST-01, INT-LOAD-17; OP-HIST-02. Tests: `TestSimpleHistory`, `TestUpdate`, `TestDevMode` (refused load recorded). Observed: `reinstall` is recorded as `install`. Loads of test-scoped or verify-scoped dependencies during `test` and `verify` are recorded as `load`, and the module itself is not recorded when `verify` loads it into the VERIFY namespace (OP-LOAD-28, OP-LOAD-29).

### history.record

- human-provided
- user-facing-behavior
- Expands: [History](#history).
- Children: [unlogged refusals](#history.record.unlogged-refusals), [abnormal end](#history.record.abnormal-end).

Sources: `wiki/09` L20; comments `General/AbstractHistory.cls:3-7`, `:17`, `:32`, `:61` ("Save it now in case Finalize isn't called"). Inventory: INT-HIST-03, INT-HIST-09; OP-HIST-01, OP-HIST-02, OP-HIST-07, OP-HIST-10, OP-HIST-11. Observed: `load` creates its record before checking the path, and `install` before checking for a module name (`Main.cls:2419-2420`, `:2565-2566`). No test covers an abnormal end.

### history.record.unlogged-refusals

- agent
- uncertain
- user-facing-behavior
- Expands: [Record](#history.record).

Sources: A1 Q-09; issue #366 ("we still want to log failed installations"). Observed: `update` refuses `PermitDowngrade` and a module that is not installed before creating a record (`Main.cls:4415-4423`). `uninstall` with no name and `reinstall` of a module that is not installed are refused the same way (`Main.cls:2685-2688`, `:2709-2712`).

### history.record.abnormal-end

- agent
- uncertain
- user-facing-behavior
- Expands: [Record](#history.record).

Sources: A1 Q-08; `wiki/09` L229-233. Observed: the permanent record keeps only its starting values until the action ends, so after a crash it shows an empty package, the placeholder version, `Success=0` and `Committed=0`, even when the load had committed (OBS-H6, P-6). The IRISTEMP copy stays until the instance restarts. Such a record is not finalized and can still be edited.

### history.survives-rollback

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [temporary copy](#history.survives-rollback.temp-copy).

Sources: issue #366 ("if install rolls back we still need the history"); `wiki/09` L18, L22; PR #578 comment by isc-dchui, 2025-11-24. Inventory: INT-HIST-02; OP-HIST-01, OP-HIST-10, OP-HIST-11. Test: `TestFailingPhases` (a rolled-back normal-mode load keeps its record). Dependency records are the open case ([parent rollback](#history.dependencies.parent-rollback)).

### history.survives-rollback.temp-copy

- human-provided
- architecture-decision
- Expands: [Survives rollback](#history.survives-rollback).

Sources: `wiki/09` L22; class doc `General/AbstractHistory.cls:3-7`, `General/HistoryTemp.cls:1`; PR #578 comment by isc-dchui, 2025-11-24 ("requires temporarily storing in IRISTEMP to avoid losing data to transaction rollback"). Inventory: OP-HIST-01 (`HistoryTemp.Init`), OP-HIST-10 (`PersistToTwin`), OP-HIST-11 (`%OnClose`); premises P-1, P-2, P-3. Observed: the permanent copy is written when the action starts and again when the in-progress record object is released. For a top-level action both writes happen outside the action's transaction. Test: `TestSimpleHistory` (no IRISTEMP rows remain).

### history.fields

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [dependency source](#history.fields.dependency-source).

Sources: `General/AbstractHistory.cls:11-49`; `wiki/09` L63-78; issue #366 field list. Inventory: INT-HIST-04, A1 3.2; OP-HIST-01, OP-HIST-03 to OP-HIST-07, OP-HIST-25. Tests: `TestSimpleHistory`, `TestDevMode`. Observed: `Version` is set from the repository before an install starts. Dependency records carry the parent's command string. Actions started outside the shell record `<unset>` as the command, and namespace default modifiers are not shown in it. A record also has a `Finalized` column that the wiki does not describe.

### history.fields.dependency-source

- agent
- uncertain
- user-facing-behavior
- Expands: [Fields](#history.fields).

Sources: A1 Q-15; `General/AbstractHistory.cls:20-27` ("For load and uninstall this will be empty"); `wiki/09` L77. Observed: dependencies of `load` and `update` are labeled `load` but carry a repository source (`Utils/Module.cls:63-69`). The listing shows `n/a` as the source for every record that is not `install`, including a top-level `update` from a registry (`General/History.cls:461-463`, OBS-H7).

### history.phases

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [returned errors](#history.phases.returned-errors).

Sources: `wiki/09` L135-161 ("A phase with no elapsed time never finished"); `Main.cls:824`. Inventory: A1 3.2 `Phases`; OP-HIST-08, OP-HIST-09, OP-LOAD-23. Tests: `TestPhases`, `TestFailingPhases`. Observed: Configure runs inside Activate and is not recorded as its own phase.

### history.phases.returned-errors

- agent
- uncertain
- user-facing-behavior
- Expands: [Phases](#history.phases).

Sources: A1 Q-14; `wiki/09` L161. Suspected gap, stage B to confirm: when an `<Invoke>` returns an error status, `ExecutePhases` leaves without finishing the phase entry (`Storage/Module.cls:603-610`, `:662-671`, OBS-H14).

### history.committed

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [marking](#history.committed.marking), [uninstall](#history.committed.uninstall), [versus success](#history.committed.vs-success), [nested](#history.committed.nested), [side effects](#history.committed.side-effects).

Sources: property comment `General/AbstractHistory.cls:41-43`; CHANGELOG 0.10.11 (`#1237`); issue #1237 by isc-dchui ("should reflect whether the action's changes were actually retained"); PR #1322 (merged as `5d27e15b`, approved by isc-kiyer). Inventory: INT-HIST-05; OP-HIST-06, OP-HIST-07, OP-LOAD-07, OP-LOAD-11, OP-LOAD-15. Tests: `TestSimpleHistory`, `TestDevMode`, `TestDevModeRollbackNotCommitted`, `TestDevModeFailureAfterCommit`, `TestNoTransactionFailure`, `TestFailingPhases`. This supersedes issue #366 ("0 in non-developer mode if installation fails; 1 otherwise") and PR #578 review r2585373293 ("in developer mode, committed will be set to 1 regardless"). `wiki/09` L80-86 predates the fix and explains only developer mode. Suspected gap, stage B to confirm: a legacy XML package that fails after writing reads uncommitted (OBS-L16). Observed: a refused developer-mode or `-DNoTransaction=1` load that saved its manifest reads committed ([requirements timing](load.g5.md#load.refusal.requirements.timing)).

### history.committed.marking

- human-provided
- architecture-decision
- Expands: [Committed](#history.committed).

Sources: PR #1322 body ("`LoadNewModule` now marks it at each commit, or before writing when there is no transaction"); `SetCommitted` doc `General/AbstractHistory.cls:119-125`. Inventory: OP-HIST-06 (callers `Utils/Module.cls:1227`, `:1313`, `:1350`), OP-HIST-07 (`Finalize` sets `Committed` when the status is OK). Issue #1237 listed comparing transaction levels as the smallest fix. The merged change chose per-commit marking instead ([nested](#history.committed.nested)).

### history.committed.uninstall

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Committed](#history.committed).

Sources: `General/AbstractHistory.cls:43`; PR #1322 body ("Uninstall is unchanged. Its validation runs inside `Clean`, so there is no single point where its writes begin"). Inventory: INT-HIST-06; OP-HIST-02, OP-HIST-24. Test: `TestSimpleHistory` (successful uninstall reads committed). Suspected gap, stage B to confirm: an uninstall that IPM skips because other modules need the module returns success and reads committed while the module stays ([dependents](uninstall.g5.md#uninstall.dependents), OBS-L17, OBS-H8). `uninstall zpm` records success before its Clean phase runs (OBS-H15).

### history.committed.vs-success

- human-provided
- user-facing-behavior
- Expands: [Committed](#history.committed).

Sources: `Main.cls:831` (help example); `wiki/09` L80-86, L215-219; PR #578 review r2585373293 by isc-dchui ("which actions have failed but have been committed so that I can debug or manually undo them"). Inventory: INT-HIST-07.

### history.committed.nested

- agent
- uncertain
- architecture-decision
- Expands: [Committed](#history.committed).

Sources: A1 Q-05; issue #1237 option 1; PR #1322. Observed: when a caller holds a transaction, IPM's commits, the record's creation and its final copy all sit inside the caller's transaction, so an outer rollback removes the module and the record although `Committed` was set (P-4, OBS-H16). The same applies to a developer-mode dependency inside its parent's load.

### history.committed.side-effects

- agent
- uncertain
- user-facing-behavior
- Expands: [Committed](#history.committed).

Sources: A1 Q-07; PR #1322 definition. Related: [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).

### history.dependencies

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [parent rollback](#history.dependencies.parent-rollback).

Sources: issue #366 ("install (including as a dependency)"); PR #578 TODO "Record recursively loaded/installed modules" (checked). Inventory: INT-HIST-08; OP-HIST-02, OP-LOAD-13, OP-LOAD-18. Test: `TestWithDependencies`. Observed: a recursive uninstall records each dependency it uninstalls, and records a dependency it skipped as a successful uninstall (OBS-H8).

### history.dependencies.parent-rollback

- agent
- uncertain
- user-facing-behavior
- Expands: [Dependencies](#history.dependencies).

Sources: A1 Q-06; issue #366. Suspected gap, stage B to confirm: a dependency's permanent record is created and finalized inside the parent's transaction, so a parent rollback removes it, while its IRISTEMP copy is already gone (OBS-L1, OBS-H5, P-2, P-4, P-5).

### history.namespace

- human-provided
- architecture-decision
- test-coverage
- Expands: [History](#history).

Sources: class doc `General/History.cls:3`; `wiki/09` L20, L169; PR #578 comment by isc-dchui, 2025-11-24 ("History is now stored per namespace"); review r1925176116 by isc-kiyer. Inventory: INT-HIST-16; OP-HIST-02. Test: `TestMultipleNamespaces`. This supersedes the 2024 decision in PR #578 to keep one shared table, and the `Namespace` field proposed in issue #366. Observed: a record is written only where IPM's history classes exist.

### history.read

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [find](#history.read.find), [filter](#history.read.filter), [details](#history.read.details), [global](#history.read.global), [API](#history.read.api).

Sources: `Main.cls:818-843`; `wiki/02` L306-349; `wiki/09` L24-50. Inventory: INT-HIST-10; OP-HIST-13 to OP-HIST-16. Test: `TestInvalidHistoryCommands`.

### history.read.find

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).

Sources: `Main.cls:822-823`; `wiki/09` L34-38. Inventory: INT-HIST-10; OP-HIST-13, OP-HIST-22. Test: `TestHistoryModifiers` (limit through the class method). Observed: any `-sort` value other than `asc` sorts newest first.

### history.read.filter

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).
- Children: [invalid column](#history.read.filter.invalid-column), [default modifiers](#history.read.filter.default-modifiers).

Sources: `Main.cls:829-837` (examples); `wiki/09` L88-112. Inventory: INT-HIST-10; OP-HIST-13, OP-HIST-16, OP-HIST-17. Test: `TestHistoryModifiers` (filters by action and package). The help example `-DSucess="0*"` (`Main.cls:832`) is misspelled.

### history.read.filter.invalid-column

- agent
- uncertain
- user-facing-behavior, security
- Expands: [Filter](#history.read.filter).

Sources: A1 Q-10; `wiki/09` L112. Suspected gap, stage B to confirm: the filter builder accepts only letters and digits in column names and stops at the first rejected name, so `history delete -DVersion_Major=1` deletes every record in the namespace (`General/History.cls:103-105`, OBS-H1).

### history.read.filter.default-modifiers

- agent
- uncertain
- user-facing-behavior
- Expands: [Filter](#history.read.filter).

Sources: new question from A2 OBS-H2; `Main.cls:820` ("By default, all entries will be affected"); `Main.cls:617-630` (default-modifiers help). Suspected gap, stage B to confirm: stored default modifiers are merged into every command, and `history` reads them as filters (`Main.cls:1067-1069`, `:4136`; `CLI.cls:815-819`).

### history.read.details

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).

Sources: `Main.cls:824`, `:839`; `wiki/09` L114-161. Inventory: INT-HIST-10; OP-HIST-15. Test: `TestInvalidHistoryCommands` (`details 0` and `details blah` fail).

### history.read.global

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).

Sources: `Main.cls:821`; `wiki/09` L163-171. Inventory: INT-HIST-11; OP-HIST-14. Test: `TestMultipleNamespaces`.

### history.read.api

- human-provided
- user-facing-behavior
- Expands: [Reading](#history.read).

Sources: issue #366 ("documented API ... cross-namespace class query wrapper"); `wiki/09` L199-211; `General/History.cls:24-30`, `:232-237`. Inventory: INT-HIST-11; OP-HIST-19, OP-HIST-20. Suspected gap, stage B to confirm: `GlobalRecords` reads other namespaces' results without switching namespace, and `Records` declares an ID column it does not return first (OBS-H10, OBS-H11). No test.

### history.delete

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [confirm with filter](#history.delete.confirm-with-filter), [global unfiltered](#history.delete.global-unfiltered).

Sources: `Main.cls:825`, `:838`; `wiki/09` L187-195. Inventory: INT-HIST-12; OP-HIST-17, OP-HIST-18. Test: `TestMultipleNamespaces`. Observed: delete also removes unfinalized records of actions still running. PR #578 review r1816818561 by isc-kiyer asked for deletes scoped by filter.

### history.delete.confirm-with-filter

- agent
- uncertain
- user-facing-behavior
- Expands: [Delete](#history.delete).

Sources: A1 Q-11; `Main.cls:825`; `wiki/09` L43-44, L187; guard `Main.cls:4165-4167`.

### history.delete.global-unfiltered

- agent
- uncertain
- user-facing-behavior, security
- Expands: [Delete](#history.delete).

Sources: A1 Q-12; `wiki/09` L195; `Main.cls:825` ("Ignored if -globally is passed too"); OBS-H9.

### history.locked

- human-provided
- user-facing-behavior, security
- test-coverage
- Expands: [History](#history).

Sources: `wiki/09` L197; PR #578 reviews r2583818448 by isc-kiyer ("To protect history from tampering ... only inserts and deletes are allowed?"), r2586415526, r2587636282; trigger `General/History.cls:13-22`. Inventory: INT-HIST-14; OP-HIST-12; premise P-8. Test: `TestFinalizedRecordsLocked` (object save and SQL `UPDATE`, SQLCODE -132). Observed: unfinalized records stay editable, and deletes are allowed by design.

### history.retention

- human-provided
- user-facing-behavior
- Expands: [History](#history).
- Children: [effect](#history.retention.effect).

Sources: `Main.cls:840-842`; `wiki/02` L113; `wiki/09` L173-185; CHANGELOG 0.10.7 (`#992`); issue #977. Inventory: INT-HIST-13; OP-HIST-21, OP-HIST-22. Test: `tests/unit_tests/Test/PM/Unit/CLI.cls` `TestAutoPurgeOldHistory` runs the purge but does not check that anything was deleted, so no coverage tag. Observed: the purge runs only through `%IPM.Main:Shell`, and the retention setting and last-purge date are shared by the whole instance. The opt-in default answers the audit-trail concern raised on PR #578 by isc-dchui, 2025-12-02.

### history.retention.effect

- agent
- uncertain
- user-facing-behavior
- Expands: [Retention](#history.retention).

Sources: A1 Q-13. Suspected gaps, stage B to confirm: the cutoff value carries literal quote marks, so the purge likely deletes nothing while still recording that it ran (`General/History.cls:172-173`, OBS-H3). A purge error is returned before the user's command runs (`Main.cls:862-865`, OBS-H12). `config delete HistoryRetain` restores the first value ever set rather than `0` (OBS-H4).

### history.keep-on-ipm-uninstall

- human-provided
- user-facing-behavior
- Expands: [History](#history).
- Children: [other namespaces](#history.keep-on-ipm-uninstall.other-namespaces).

Sources: prompt `Main.cls:2735` ("fully remove all modules and IPM metadata (except history log)"); maintainer comment `Lifecycle/Module.cls:54` ("Preserve ^IPM.General.HistoryD for audit trail"); CHANGELOG 0.10.7 (`#1057`). Inventory: INT-HIST-15; OP-HIST-23. Observed: the uninstall record of IPM itself is finalized as successful before its Clean phase runs (OBS-H15). No test.

### history.keep-on-ipm-uninstall.other-namespaces

- agent
- uncertain
- user-facing-behavior
- Expands: [IPM uninstall](#history.keep-on-ipm-uninstall).

Sources: A1 Q-16. Observed: full cleanup kills `^IRIS.Temp.HistoryTempD`, which every namespace shares (`Lifecycle/Module.cls:51`, OBS-H13, P-1).
