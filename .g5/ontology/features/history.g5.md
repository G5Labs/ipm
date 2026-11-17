# Transaction history

IPM keeps a log of what it did to each namespace, so users can find out what changed, when, who ran it and why something failed.

```
action starts ──► permanent record written (Success 0, no end time,
     │             placeholder version), outside the action's transaction;
     │             in-progress copy kept in IRISTEMP, so a rollback cannot erase it
     ▼
 phases run ─────► phase entries added; Committed set at each commit
     │
action ends ────► Success and end time set; result copied to the
                  permanent record outside the transaction; finalized (read-only)
```

## Definitions

- **Action**: one `install`, `load`, `uninstall` or `update` that IPM attempts, including a dependency install it starts.
- **Record**: the log entry for one action.
- **Finalized**: a record whose action has ended and whose result IPM has written to the permanent log.
- **Phase entry**: the name, start time, end time and status of one lifecycle phase the action ran.
- **Kept changes**: changes an action made that are still there after it ends, either committed or written outside a transaction.

## Actions and records

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

IPM creates a record as soon as an action starts and fills it in when the action ends. A process that dies mid-action still leaves a record with an empty end time. Fields not yet known, such as the version, may hold a placeholder.

- While the action runs, and after it stops without ending, the log lists its record with `Success` 0, no end time and the [phases](#history.phases) it has started.
- `history find -DSuccess=0` therefore finds actions that are running or never finished.

- Working assumption: record them. Examples are `update` or `reinstall` of a module that is not installed, and `uninstall` with no name.
- Working assumption: `Committed` reflects the changes kept, even on an unfinished record. IPM writes it to the permanent record at each commit.

<a id="history.fields"></a>
[I · history.fields]

Each record holds:

- `Action`: `load`, `install`, `uninstall` or `update`.
- `Package`: the module name. It is empty only if the action failed before any name was known, such as a load of a folder without a valid manifest. A refusal after IPM read the manifest records the name.
- `Version`: the module version. Until the version is known it is stored as `0.0.0-0` and shown as `<unset>`. As built, a module whose declared version is `0.0.0-0` is also shown as `<unset>`, because listings compare the stored text only (defect C:D43).
- Source name, type and details: the repository an install came from. They are empty for load and uninstall, and listings show `n/a` for any action other than install.
- `TimeStart` and `TimeEnd`: the end time is empty while the action runs or after it ended abnormally.
- `UserName`: the user who started the action.
- `Success`: the action's status. It is `0` while the action is in progress.
- `Committed`: whether the action kept changes ([committed](#history.committed)).
- `CommandString`: the command as typed.
- `Phases`: the [phase entries](#history.phases).

- Working assumption: keep the `load` label for dependencies of `load` and `update`, keep their stored source, and keep the rule that listings show a source only for installs.

<a id="history.phases"></a>
[I · history.phases]

Each record lists the lifecycle phases its action ran, with each phase's start time, end time and status. A phase with no end time never finished, which points to a hang or crash in that phase.

- Load and install record Initialize, Reload, Validate, Compile and Activate. Update also records ApplyUpdateSteps, and uninstall records Clean.
- A failed action lists the phases up to and including the one that failed.

- Working assumption: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one.

## Survival after rollback

<a id="history.survives-rollback"></a>
[I · history.survives-rollback]

A record survives any rollback that undoes its action, so failures stay visible afterwards. This includes a parent load's rollback of a dependency it loaded.

- Whether a record survives a rollback by the caller's own transaction is open ([nested](../../Verus_verification/open_questions.md#history.committed.nested)).

- As built, a dependency's permanent record inserted inside its parent's transaction can disappear with the parent's rollback (G:D1 = P:F-HIST-1). Its successful result in IRISTEMP, installed data, and permanent row are three separate observations.

- This contradicts the survival requirement; it does not redefine success.

- Working assumption: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.
- Working assumption: Keep current survival requirement and report the repair as undecided; no product repair applied.

<a id="history.survives-rollback.temp-copy"></a>
[D · history.survives-rollback.temp-copy]

IPM writes the permanent record when the action starts and keeps the in-progress record in IRISTEMP, which a rollback does not undo. When the action ends, IPM copies the result to the permanent record. For a top-level action started outside any transaction, both writes happen outside the action's transaction, so its rollback cannot undo them.

- Process loss and instance restart differ. In a checked SIGKILL schedule, temporary progress and the initial permanent row remained. After a whole-instance restart, the temporary row was gone and the durable initial permanent row remained unfinished.
- These observations require the checked IRISTEMP mapping and durable database. They do not establish power-loss durability, replication or every caller-transaction schedule. [Paired native and formal evidence](../../uplift/consolidated/iter2/boundaries/REPORT.md).

<a id="history.dependencies"></a>
[I · history.dependencies]

Installs that IPM starts for dependencies are recorded as their own records, with their own phases.

<a id="history.dependencies.parent-rollback"></a>
[D · history.dependencies.parent-rollback]

A dependency's `Success` records its own returned status, even if its parent's later rollback undoes its data. `Committed` describes kept database writes.

- The surviving-record requirement remains in [history.survives-rollback](#history.survives-rollback).

- As built, the child can finish successfully and then lose its permanent row with the parent's rollback (G:D1 = P:F-HIST-1). These facts are supported by the property comments, native receipts, and the history design discussion recorded in P DEC-HIST-1.

- Q-06 remains an alias of this resolved meaning; audit retention under a caller's transaction stays [open](../../Verus_verification/open_questions.md#history.committed.nested).

- Working assumption: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.

## Interpreting Success and Committed

<a id="history.committed"></a>
[I · history.committed]

For `load`, `install` and `update`, `Committed` shows whether the action kept any changes, either by a commit or by writing outside a transaction. An uncommitted action wrote nothing or was rolled back.

- A successful action is committed, unless a parent load's rollback later undoes it. A dependency undone that way reads uncommitted.
- A failure rolled back in full is not committed. This includes a developer-mode failure before its [first commit](load.g5.md#load.outcome.dev.early-failure).
- A developer-mode failure after its first commit is committed.
- A failure with `-DNoTransaction=1` or `-DNoJournal=1` is committed once the action has started writing.
- A refusal before anything is written is not committed, in any mode.

- `Committed` describes database writes; it does not promise to undo copied files, lock files, pip effects or CPF changes. The property comment and maintainer history design review settle this field meaning (P DEC-HIST-5).

- Inherited [side-effects question](#history.committed.side-effects) is retained as an alias for that resolved interpretation. After a post-commit error the record can be committed and unsuccessful.

- Working assumption: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.
- Working assumption: Answering "Commit" at a crash prompt is an operator error to document.
- Working assumption: Count surviving child writes in the parent outcome; retain the distinction between parent and child records.

<a id="history.committed.marking"></a>
[D · history.committed.marking]

IPM sets `Committed` at each commit, or just before the first write when there is no transaction. It does not work it out afterwards from developer mode and the final status.

- Working assumption: Keep the intent that a no-write refusal is uncommitted; classify the early marker as a defect.

<a id="history.committed.uninstall"></a>
[I · history.committed.uninstall]

Uninstall runs without a transaction, so for uninstall `Committed` only shows whether it succeeded. A failed uninstall that removed some resources still reads uncommitted.

<a id="history.committed.vs-success"></a>
[I · history.committed.vs-success]

`Success` and `Committed` answer different questions, because a failed action can keep changes. A failed but committed record is the one to look for when a namespace is in an unexpected state.

<a id="history.committed.side-effects"></a>
[D · history.committed.side-effects]

This question is resolved for the field's meaning: `Committed` covers database changes. It makes no filesystem rollback promise.

- The property comment and maintainer explanation recorded in P DEC-HIST-5 support this reading; the wiki's “written to disk” wording is broader and needs correction. Filesystem effects remain observable even when this bit is false.

## Finding and reading records

<a id="history.read"></a>
[I · history.read]

The `history` command (alias `log`) reads and manages the log with the actions `find` (the default), `details <id>`, `delete` and `schema`. Any other action is an error.

- Working assumption: Return identities sufficient for a caller to distinguish colliding namespace-local IDs.

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

- As built, `ConstructSQLWhere` walks keys in IRIS collation order and stops at the first absent value or name outside `[A-Za-z][A-Za-z0-9]*`. It keeps earlier predicates and ignores that key and every later one.

- An underscore column such as `Version_Major` can therefore leave no predicate: an unmatched `Version_Major=99` deleted all three seeded identities. A valid `Package=del-b` or `Success=0` deleted only its matches.

- An unknown alphanumeric column raises SQL error; `find` can hide that error behind an empty successful listing while delete throws. These are failures of the rule, not permitted filtering behavior (G:D2).

- Working assumption: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter. When `find` cannot run its query, it reports the error instead of showing an empty log.
- Working assumption: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests.

<a id="history.read.details"></a>
[I · history.read.details]

`history details <id>` shows every field of one record in the current namespace, including full repository details and the raw status. `-phases` adds each phase's timing and status. An unknown ID is an error.

- Phase lines keep the stored order. Each shows its start time, its duration only when it has an end time, and `Success` or the decoded error text. A phase left open after a returned error shows its start time and error text (see G:D6).

<a id="history.read.status-bytes"></a>
[D · history.read.status-bytes]

A history summary writes `Success` for a successful status and decoded error text for a failed one. Details selects the stored raw status, as [required](#history.read.details), then makes CR, LF and TAB visible as `<CR>`, `<LF>` and `<TAB>`.

- The checked no-repository failure has a 311-byte encoded status. Its summary is valid UTF-8 error text; its detailed status preserves the stored bytes and is not valid UTF-8. This is a limit on treating raw status output as text, not a reversal of the established raw-status rule or its withdrawn defect.
- An ordinary failing lifecycle hook supplies a status containing CR, LF and TAB. The displayed detail replaces those three bytes with visible labels. Color enabled or disabled changes ANSI formatting, while the uncolored field retains the same selected content and labels.
- These observations cover finalized successful and failed records from the checked ordinary installs. Status encoding, error decoding, color configuration and terminal adaptation are provider boundaries; arbitrary messages, unfinished records and general output encodings retain their separate scope.

<a id="history.read.unfinished"></a>
[I · history.read.unfinished]

A record of an action that has not ended shows its start time without a duration. Its result must read as not finished, not as an error.

- As built, `history find` shows such a record's result as `ERROR #00: (no error description)`. This is a known defect.

<a id="history.read.global"></a>
[I · history.read.global]

`-globally` reads every namespace on the instance and groups the records by namespace.

- Namespaces without IPM add no records.
- `-limit` applies to each namespace, not to the total.
- `history details` always reads the current namespace.

<a id="history.read.api"></a>
[I · history.read.api]

Programs can read the log through the SQL table `%IPM_General.History` and the class queries `Records` (current namespace) and `GlobalRecords` (all namespaces).

- The two global readers differ as built. CLI `find -globally` fetches each result in its own namespace, labels that namespace and applies the limit per namespace.

- `GlobalRecordsFetch` fetches each handle in the caller's namespace, so it repeats the caller's rows; its row schema includes neither namespace nor ID. Correct CLI output does not establish programmatic query correctness (G:D20).

- Working assumption: Return identities sufficient for a caller to distinguish colliding namespace-local IDs.

## Deleting records

<a id="history.delete"></a>
[I · history.delete]

`history delete` deletes the records that match its filters.

- Deleting every record in the namespace, with no filter, requires `-confirm`.
- With `-globally`, it deletes the matching records in every namespace ([no confirmation](#history.delete.global-unfiltered)).
- `-verbose` shows how many records each namespace lost and which namespaces were skipped for lack of permission.

- The intended exact-selection rule above is violated by the filter parser's stop rule ([filter conditions](#history.read.filter)). Neither the presence of a syntactic filter nor `-confirm` ensures an effective predicate.

- Global delete ignores namespace exceptions in the as-built loop. A valid filter's exact deleted identities remain part of the guarantee.

- Working assumption: only the matching ones. `-confirm` only permits a delete without a filter.
- Working assumption: a delete or purge may remove any matching record, and the running action must still finish normally whether or not its record comes back.
- Working assumption: Report errors that mean requested deletion was not performed.

<a id="history.delete.global-unfiltered"></a>
[D · history.delete.global-unfiltered]

`history delete -globally` does not require `-confirm`. With no filter, it deletes every record on the instance without asking.

## Related rules on other pages

<a id="history.fields.source-snapshot"></a>
- Current supplier and historical source — [history.fields.source-snapshot](../concepts/data-policy.g5.md#history.fields.source-snapshot).

<a id="history.namespace"></a>
- Namespace boundaries — [history.namespace](../concepts/data-policy.g5.md#history.namespace).

<a id="history.locked"></a>
<a id="history.retention"></a>
<a id="history.keep-on-ipm-uninstall"></a>
- Protection and retention of history — [history.locked](../concepts/data-policy.g5.md#history.locked), [history.retention](../concepts/data-policy.g5.md#history.retention), [history.keep-on-ipm-uninstall](../concepts/data-policy.g5.md#history.keep-on-ipm-uninstall).

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) (`CAT`, `INT`, `Q`), [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md) (`OP`, `OBS`, `P`) and the [coverage map](../../uplift/ipm-verus-20261113/A/coverage-map.md). Every block on this page is in the formal scope. Native results of stage B2 phase 1 are cited per block by run, as `<time>-<name>`. The full run ID is `20261113T<time>-<name>` under `.g5/Verus_verification/results/native/`, and the [B2 report](../../uplift/ipm-verus-20261113/B/native-report.md) explains each phase-1 case and defect. Formal (B1) and native evidence per block, with reliance status, is in the [verification coverage map](../../Verus_verification/coverage-map.md). B2 phase 2 and stage C runs are listed in [B/native-replay.md](../../uplift/ipm-verus-20261113/B/native-replay.md) sections 3-7, and defects `D10`-`D20`, `I1` and `I2` in [VV/bugs_issues.md](../../Verus_verification/bugs_issues.md). History classes: `General/AbstractHistory.cls`, `General/HistoryTemp.cls`, `General/History.cls`, `General/HistoryPhase.cls`; tests in `tests/integration_tests/Test/PM/Integration/History.cls`.

### history

- human-provided
- user-facing-behavior
- test-coverage
- Children: [actions](#history.actions), [record](#history.record), [survives rollback](#history.survives-rollback), [fields](#history.fields), [phases](#history.phases), [committed](#history.committed), [dependencies](#history.dependencies), [namespace](../concepts/data-policy.g5.md#history.namespace), [reading](#history.read), [delete](#history.delete), [locked](../concepts/data-policy.g5.md#history.locked), [retention](../concepts/data-policy.g5.md#history.retention), [IPM uninstall](../concepts/data-policy.g5.md#history.keep-on-ipm-uninstall).

Sources: `wiki/09` L14-22; `Main.cls:818-821`; issue #366 by isc-tleavitt ("The user persona here is a user of IPM looking for history on their own instance"); CHANGELOG 0.10.5 (`#578`). Inventory: CAT-42. Test: `TestSimpleHistory`. Future option, not a commitment: issue #366 asked to investigate the IRIS audit database (INT-HIST-17). Nothing implements it.

Findings: [G:D11 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d11). The linked entry qualifies the caller, reader and providers.

Findings: [G:D12 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d12). The linked entry qualifies the caller, reader and providers.

Findings: [G:D13 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d13). The linked entry qualifies the caller, reader and providers.

### history.actions

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).

Sources: `Main.cls:820`; `wiki/09` L18; issue #366; PR #578 review r2557338086 by isc-dchui ("install, load, uninstall, and update seem most important"). Inventory: INT-HIST-01, INT-LOAD-17; OP-HIST-02. Tests: `TestSimpleHistory`, `TestUpdate`, `TestDevMode` (refused load recorded). Observed: `reinstall` is recorded as `install`. Loads of test-scoped or verify-scoped dependencies during `test` and `verify` are recorded as `load`, and the module itself is not recorded when `verify` loads it into the VERIFY namespace (OP-LOAD-28, OP-LOAD-29). Native evidence: every B2 load case produced a top-level record (`182759Z-cases-a`, `182820Z-cases-b`, `182903Z-cases-c-terminate`, `182832Z-cases-dg`), and the History suite passed 13 of 13 (`175623Z-smoke-history-suite-oras`).

### history.record

- uncertain
- human-provided
- user-facing-behavior
- Expands: [History](#history).
- Children: [Should IPM record `update`, `reinstall` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?](../../Verus_verification/open_questions.md#history.record.unlogged-refusals), [What should `Committed` say on the record of an action that never ended?](../../Verus_verification/open_questions.md#history.record.abnormal-end).

Sources: `wiki/09` L20, L72 ("An entry with no elapsed time either is still running or ended abnormally"), L161, L229-233 (`-DSuccess=0` finds actions that never finished); comments `General/AbstractHistory.cls:3-7`, `:17`, `:32`, `:38`, `:61`. Inventory: INT-HIST-03, INT-HIST-09; OP-HIST-01, OP-HIST-02, OP-HIST-07, OP-HIST-10, OP-HIST-11, OP-HIST-26. Reconstruction check R3 moved the visible-progress rule here. Round 1 R1-ONT-02 dropped the placeholder version as a required outcome, because `wiki/09` L20 describes what a reader sees.

Native evidence (B2 phase 1). Process termination splits in two. `$SYSTEM.Process.Terminate` runs the end copy (premise N-P10 refuted), and SIGKILL does not (N-P18).
- C1, a normal-mode load terminated in Activate: empty end time, conforming, but `Package=""` and no phases. The end copy was undone by the rollback and the IRISTEMP copy deleted. Deviates from listing started phases (`182903Z-cases-c-terminate`).
- C2, a developer-mode load terminated in Activate: package, version, `Committed=1`, open Activate phase, no end time, but finalized ([May IPM finalize, and so lock, the record of an action that never ended?](../../Verus_verification/open_questions.md#history.locked.unfinished)) (`182903Z-cases-c-terminate`).
- C3 and C4R, SIGKILL: the permanent record keeps only its start values (`Package=""`, no phases), and the IRISTEMP copy is orphaned. Confirmed product defect D10, formerly OBS-H6 (`182927Z-cases-c-sigkill`, `183827Z-cases-c4r`; C1 replayed in `184230Z-replay-fast`). B1's model cases C44-C48 in T and K variants agree with the product (`194122Z-replay-terminate-b`, `194155Z-replay-sigkill-b`).
- Listings of a running action from another process were not observed.

Observed: `load` creates its record before checking the path, and `install` before checking for a module name (`Main.cls:2419-2420`, `:2565-2566`). No product test covers an abnormal end.

Findings: [C:D12 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d12). The linked entry qualifies the caller, reader and providers.

Open questions: [What should `Committed` say on the record of an action that never ended?](../../Verus_verification/open_questions.md#history.record.abnormal-end), [Should IPM record `update`, `reinstall` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?](../../Verus_verification/open_questions.md#history.record.unlogged-refusals)

### history.fields

- design-verified

- uncertain
- human-provided
- user-facing-behavior
- Expands: [History](#history).
- Children: [How should the log label and show a dependency that IPM pulled from a repository while running a `load` or `update`, and what source does an `update` from a registry store?](../../Verus_verification/open_questions.md#history.fields.dependency-source).

Sources: `General/AbstractHistory.cls:11-49` (`:17` placeholder `0.0.0-0`, `:20-27` source empty for load and uninstall); `wiki/09` L63-78 (L70 `<unset>`, L76 source `n/a` for anything other than install), L112 (version parts); issue #366 field list ("for install only"). Reconstruction check R9, R14. Inventory: INT-HIST-04, A1 3.2; OP-HIST-01, OP-HIST-03 to OP-HIST-07, OP-HIST-25. Tests: `TestSimpleHistory`, `TestDevMode`. Observed: `Version` is set from the repository before an install starts. Dependency records carry the parent's command string. Actions started outside the shell record `<unset>` as the command, and namespace default modifiers are not shown in it. A record also has a `Finalized` column that the wiki does not describe. A top-level `update` from a registry stores its source, and listings show `n/a` for it (`Main.cls:2631`, `General/History.cls:461-463`). Round 1 R1-ONT-03 made the `Package` bullet say when it may be empty, a necessary implication of `wiki/09` L69. Confirmed deviation D9: refused loads whose manifest was read (g3, g6, g7) record `Package=""` and version `<unset>`, so `history find -DPackage=update-simple` shows 14 of the 20 rows that name it (R4) (`182832Z-cases-dg`, `184444Z-cases-hr`). Observed cause: the name is set only after the version checks (`Main.cls:2420`, `Utils/Module.cls:1293`).

Extension Verus: [history.rs::set_source](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [G:D9 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d9). The linked entry qualifies the caller, reader and providers.

Iteration38 rendering: [render.rs](../../Verus_verification/consolidated/iter38/render.rs) `summary`, `placeholder_hides_version`, `source_only_for_install`, `known_placeholder_indistinguishable` ([handoff](../../uplift/consolidated/iter38/HANDOFF.md)).

Findings: [History shows a module whose real version is 0.0.0-0 as `<unset>`](../../Verus_verification/bugs_issues.md#c-d43)

Open questions: [How should the log label and show a dependency that IPM pulled from a repository while running a `load` or `update`, and what source does an `update` from a registry store?](../../Verus_verification/open_questions.md#history.fields.dependency-source)

State machines: [render · iter38](../../Verus_verification/models/iter38-render/manifest.json)

### history.phases

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [Must every phase that starts also record its end and status, including when an `<Invoke>` returns an error status instead of throwing one?](../../Verus_verification/open_questions.md#history.phases.returned-errors).

Sources: `wiki/09` L135-161 ("A phase with no elapsed time never finished"); `Main.cls:824`. Inventory: A1 3.2 `Phases`; OP-HIST-08, OP-HIST-09, OP-LOAD-23. Tests: `TestPhases`, `TestFailingPhases`. Observed: Configure runs inside Activate and is not recorded as its own phase. Issue I1 (formerly D8) shows an open phase as `ERROR #00: (no error description)` (case R2, `184444Z-cases-hr`). No source governs that text. Case C2 shows the open Activate phase after termination (`182903Z-cases-c-terminate`).

Open questions: [Must every phase that starts also record its end and status, including when an `<Invoke>` returns an error status instead of throwing one?](../../Verus_verification/open_questions.md#history.phases.returned-errors)

### history.survives-rollback

- uncertain
- human-verified
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [temporary copy](#history.survives-rollback.temp-copy).

Sources: issue #366 by isc-tleavitt ("History should be logged for ... 'install' (including as a dependency)", "if install rolls back we still need the history"); `wiki/09` L18, L22; PR #578 comment by isc-dchui, 2025-11-24. Round 1 R1-ONT-01 widened the rule from the record's own action to any rollback that undoes it. The dependency case was the survival half of Q-06. Inventory: INT-HIST-02, INT-HIST-08; OP-HIST-01, OP-HIST-10, OP-HIST-11. Test: `TestFailingPhases` (a rolled-back top-level load keeps its record).

Native evidence: a top-level normal-mode failure keeps its record (A1 parent record). Confirmed product defect B2 D1: a dependency's record is lost when the parent's transaction rolls back, including the record of a dependency that itself failed. Its ID is allocated but no row persists (cases A1, A2, A3; `182759Z-cases-a`, `184230Z-replay-fast`, `184617Z-repro-dep-history-lost`).

Findings: [G:D1 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d1). The linked entry qualifies the caller, reader and providers.

Findings: [P:D1 — scoped repair comparison](../../Verus_verification/design_simplification.md#p-d1).

Open questions: [When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?](../../Verus_verification/open_questions.md#history.committed.nested), [Which repair should preserve child history without lying about Committed?](../../Verus_verification/open_questions.md#history.survives-rollback.repair-policy)

### history.survives-rollback.temp-copy

- human-provided
- architecture-decision
- Expands: [Survives rollback](#history.survives-rollback).

Sources: `wiki/09` L20, L22; class doc `General/AbstractHistory.cls:3-7` ("persisted to %IPM.General.History on init and when the object falls out of scope"), `General/HistoryTemp.cls:1`; PR #578 comment by isc-dchui, 2025-11-24. Inventory: OP-HIST-01 (`HistoryTemp.Init`), OP-HIST-10 (`PersistToTwin`), OP-HIST-11 (`%OnClose`); premises P-1, P-2, P-3. Reconstruction check R2 added the start write. Round 1 R1-ONT-01 limited the decision to top-level actions started outside any transaction, because for a dependency or under a caller's transaction both writes happen inside the outer transaction (B2 D1 and case D1).

Native evidence: the premises behind the decision hold for an ordinary error. IRISTEMP sets survive a rollback (N-P04). `%OnClose` runs when the last reference drops and while an exception unwinds, so the end copy follows the action's own rollback (N-P08, N-P09; `181129Z-premises`). The premise is refuted for process termination. `$SYSTEM.Process.Terminate` runs `%OnClose` before IRIS rolls the open transaction back, so in normal mode the end copy is undone and the IRISTEMP copy deleted (N-P10, case C1; `181129Z-premises`, `182903Z-cases-c-terminate`). SIGKILL skips `%OnClose` (N-P18, `181724Z-premise-p18-sigkill-r3`). Test: `TestSimpleHistory` (no IRISTEMP rows remain).

### history.dependencies

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [parent rollback](#history.dependencies.parent-rollback).

Sources: issue #366 ("install (including as a dependency)"); PR #578 TODO "Record recursively loaded/installed modules" (checked). Inventory: INT-HIST-08; OP-HIST-02, OP-LOAD-13, OP-LOAD-18. Test: `TestWithDependencies`. Observed: a recursive uninstall records each dependency it uninstalls, and records a dependency it skipped as a successful uninstall (OBS-H8). Native evidence: A0, A4 and A5 conform, with each dependency record OK and committed (`182759Z-cases-a`, `184230Z-replay-fast`). Records lost under a parent rollback: B2 D1 ([survives rollback](#history.survives-rollback)). All B2 dependency cases used a `.modules` filesystem repository. Remote and ORAS sources were not run.

### history.dependencies.parent-rollback

- agent
- uncertain
- user-facing-behavior
- Expands: [Dependencies](#history.dependencies).

Sources: A1 Q-06; issue #366; `wiki/09` L84 ("`Success` is the status the action returned"). Round 1 R1-ONT-01 moved survival into [survives rollback](#history.survives-rollback) and the `Committed` value into [committed](#history.committed), leaving only `Success` open. Native evidence: in cases A1, A2 and A3 the record is lost (B2 D1), so the provisional value could not be observed (`182759Z-cases-a`, `184617Z-repro-dep-history-lost`).

Open questions: [When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?](../../Verus_verification/open_questions.md#history.committed.nested)

### history.committed

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [marking](#history.committed.marking), [uninstall](#history.committed.uninstall), [versus success](#history.committed.vs-success), [When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?](../../Verus_verification/open_questions.md#history.committed.nested), [side effects](#history.committed.side-effects).

Sources: property comment `General/AbstractHistory.cls:41-43`; CHANGELOG 0.10.11 (`#1237`); issue #1237 by isc-dchui ("should reflect whether the action's changes were actually retained"); PR #1322 (merged as `5d27e15b`, approved by isc-kiyer). Inventory: INT-HIST-05; OP-HIST-06, OP-HIST-07, OP-LOAD-07, OP-LOAD-11, OP-LOAD-15. Tests: `TestSimpleHistory`, `TestDevMode`, `TestDevModeRollbackNotCommitted`, `TestDevModeFailureAfterCommit`, `TestNoTransactionFailure`, `TestFailingPhases`. This supersedes issue #366 ("0 in non-developer mode if installation fails; 1 otherwise") and PR #578 review r2585373293 ("in developer mode, committed will be set to 1 regardless"). `wiki/09` L80-86 predates the fix and explains only developer mode. Suspected gap, not run natively by B2: a legacy XML package that fails after writing reads uncommitted (OBS-L16). Observed: a refused developer-mode or `-DNoTransaction=1` load that saved its manifest reads committed ([Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)). Reconstruction check R4: the first bullet now names top-level actions, because a dependency's success can be undone by its parent's rollback. Round 1 R1-ONT-01: a dependency undone by its parent's rollback reads uncommitted by this definition. Native evidence: top-level outcomes conform in cases A0, A1, A5, B1 and g1 to g9 (`182759Z-cases-a`, `182820Z-cases-b`, `182832Z-cases-dg`, `184230Z-replay-fast`). Deviations are owned elsewhere: B2 to B4 read committed after a requirements refusal ([Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing), B2 D4), C4R reads uncommitted after a SIGKILLed developer-mode load kept changes ([What should `Committed` say on the record of an action that never ended?](../../Verus_verification/open_questions.md#history.record.abnormal-end)), and undone dependency records are lost rather than uncommitted (B2 D1). Confirmed product defect D12: a `-DNoTransaction=1` or `-DNoJournal=1` load refused after the no-transaction marker reads `Committed=1` with nothing kept (model case B1-C08, `193952Z-replay-return-b`). This contradicts the last bullet.

Open questions: [Should a parent record count writes kept only by its NoJournal dependency?](../../Verus_verification/open_questions.md#history.committed.escaped-dependency), [When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?](../../Verus_verification/open_questions.md#history.committed.nested), [What history outcome is required when an operator commits a stopped load at the terminal?](../../Verus_verification/open_questions.md#history.committed.terminal-choice-policy)

State machines: [Load outcome contracts](../../Verus_verification/models/load-outcomes/manifest.json)

### history.committed.marking

- design-verified

- uncertain
- human-provided
- architecture-decision
- Expands: [Committed](#history.committed).

Sources: PR #1322 body ("`LoadNewModule` now marks it at each commit, or before writing when there is no transaction"); `SetCommitted` doc `General/AbstractHistory.cls:119-125`. Inventory: OP-HIST-06 (callers `Utils/Module.cls:1227`, `:1313`, `:1350`), OP-HIST-07 (`Finalize` sets `Committed` when the status is OK). Issue #1237 listed comparing transaction levels as the smallest fix. The merged change chose per-commit marking instead ([When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?](../../Verus_verification/open_questions.md#history.committed.nested)).

Open questions: [Should the no-transaction marker run only after every refusal?](../../Verus_verification/open_questions.md#history.committed.marking.refusal-policy)

State machines: [Commit marker soundness](../../Verus_verification/models/commit-soundness/manifest.json)

### history.committed.uninstall

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Committed](#history.committed).

Sources: `General/AbstractHistory.cls:43`; PR #1322 body ("Uninstall is unchanged. Its validation runs inside `Clean`, so there is no single point where its writes begin"). Inventory: INT-HIST-06; OP-HIST-02, OP-HIST-24. Test: `TestSimpleHistory` (successful uninstall reads committed). Suspected gap, not run natively by B2: an uninstall that IPM skips because other modules need the module returns success and reads committed while the module stays ([dependents](uninstall.g5.md#uninstall.dependents), OBS-L17, OBS-H8). `uninstall zpm` records success before its Clean phase runs (OBS-H15).

### history.committed.vs-success

- human-provided
- user-facing-behavior
- Expands: [Committed](#history.committed).

Sources: `Main.cls:831` (help example); `wiki/09` L80-86, L215-219; PR #578 review r2585373293 by isc-dchui ("which actions have failed but have been committed so that I can debug or manually undo them"). Inventory: INT-HIST-07.

### history.committed.side-effects

- agent
- uncertain
- user-facing-behavior
- Expands: [Committed](#history.committed).

Sources: A1 Q-07; PR #1322 definition. Related: [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).

### history.read

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [find](#history.read.find), [filter](#history.read.filter), [details](#history.read.details), [unfinished records](#history.read.unfinished), [global](#history.read.global), [API](#history.read.api).

Sources: `Main.cls:818-843`; `wiki/02` L306-349; `wiki/09` L24-50. Inventory: INT-HIST-10; OP-HIST-13 to OP-HIST-16. Test: `TestInvalidHistoryCommands`.

Open questions: [Must programmatic global history rows carry their namespace and ID?](../../Verus_verification/open_questions.md#history.read.global-shape)

### history.read.find

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).

Sources: `Main.cls:822-823`; `wiki/09` L34-38. Inventory: INT-HIST-10; OP-HIST-13, OP-HIST-22. Test, partial: `TestHistoryModifiers` calls `GetHistory` directly, so `-sort` and the default limit are untested. Observed: any `-sort` value other than `asc` sorts newest first.

Extension Verus: [history.rs::selection](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### history.read.filter

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).
- Children: [What should happen to a filter on a column such as `Version_Major`, on a column that does not exist, or when the query fails?](../../Verus_verification/open_questions.md#history.read.filter.invalid-column), [Should the namespace's [default modifiers](../operations/settings.g5.md#settings.default-modifiers) act as `history` filters?](../../Verus_verification/open_questions.md#history.read.filter.default-modifiers).

Sources: `Main.cls:829-837` (examples); `wiki/09` L88-112. Inventory: INT-HIST-10; OP-HIST-13, OP-HIST-16, OP-HIST-17. Test, partial: `TestHistoryModifiers` filters by action and package through `GetHistory` directly, without `-D` parsing. Native evidence: E0, E5 and E6 conform, while E1, E2, E4 and E8 deviate (B2 D2). `find` and `delete` differ for an unknown column (E3, E7) ([What should happen to a filter on a column such as `Version_Major`, on a column that does not exist, or when the query fails?](../../Verus_verification/open_questions.md#history.read.filter.invalid-column); `182845Z-cases-ef`). The help example `-DSucess="0*"` (`Main.cls:832`) is misspelled.

Extension Verus: [history.rs::where_prefix](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [G:D2 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d2). The linked entry qualifies the caller, reader and providers.

Open questions: [Should the namespace's [default modifiers](../operations/settings.g5.md#settings.default-modifiers) act as `history` filters?](../../Verus_verification/open_questions.md#history.read.filter.default-modifiers), [What should happen to a filter on a column such as `Version_Major`, on a column that does not exist, or when the query fails?](../../Verus_verification/open_questions.md#history.read.filter.invalid-column)

### history.read.details

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Reading](#history.read).

Sources: `Main.cls:824`, `:839`; `wiki/09` L114-161. Inventory: INT-HIST-10; OP-HIST-15. Test: `TestInvalidHistoryCommands` (`details 0` and `details blah` fail). Native evidence: cases R1 and R2 conform (`184444Z-cases-hr`). The raw status follows `wiki/09` L133, so round 1 withdrew B2 D7 (R1-ONT-13). Consolidated round 3 (`20261114T032054Z-r3-history.partial-purge-failed`, ARM64): for a failed action the raw status is printed as the stored bytes, which include the internal call stack (H3d); repository details read `reg3 (filesystem): /tmp/c3/reg/*` (H1d); `-phases` lists name, time and result per phase (HB1d, `20261114T032620Z-r3-history-b`); a missing ID reports "No record found with ID 999999" (H5). Model `Verus_verification/consolidated/models/history_display.py`.

Extension Verus: [history.rs::source_shown](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Iteration38 phase rendering: [render.rs](../../Verus_verification/consolidated/iter38/render.rs) `detail_phases`, `phase_rendering_keeps_order_and_status`.

State machines: [render · iter38](../../Verus_verification/models/iter38-render/manifest.json)

### history.read.status-bytes

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Details](#history.read.details).

[Status-byte refinement](../../Verus_verification/consolidated/iter23/status_reader.rs) reuses the inherited symbolic raw-versus-decoded reader and adds stored bytes, control-character cleanup, color formatting and consumer observations. [Iteration 23](../../uplift/consolidated/iter23/HANDOFF.md) retains original stdout/stderr bytes, escaped text, provider hex values, finalized record identities and color-on/none comparisons. The established withdrawal of B2 D7/R1-ONT-13 and raw/internal-stack evidence remain unchanged. No new UTF-8 promise is inferred for raw details.

State machines: [status reader · iter23](../../Verus_verification/models/iter23-status-reader/manifest.json)

### history.read.unfinished

- agent
- user-facing-behavior
- Expands: [Reading](#history.read).

Sources: `wiki/09` L72 ("An entry with no elapsed time either is still running or ended abnormally"); `General/History.cls:439-455` (no duration without an end time), `:474-483` (`$$$ISOK(0)` is false, so the error text of status 0 is shown). Native H4d and H4f: a record started by `LoadInit` and never finalized shows `<T>` without duration, `ERROR #00: (no error description)` and `Uncommitted`. Defect C:D12; related to G I1 (phase display) and G D26 (an unfinished record reads uncommitted).

Extension Verus: [history.rs::success_to_string](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### history.read.global

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Reading](#history.read).

Sources: `Main.cls:821`; `wiki/09` L163-171. Inventory: INT-HIST-11; OP-HIST-14. Test: `TestMultipleNamespaces`. Native evidence: `history find -globally` lists a row written in USER-VERIFY under its namespace (stage C case H10, `20261113T204152Z-stagec`).

Extension Verus: [history.rs::cli_global](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### history.read.api

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Reading](#history.read).

Sources: issue #366 ("documented API ... cross-namespace class query wrapper"); `wiki/09` L199-211; `General/History.cls:24-30`, `:232-237`. Inventory: INT-HIST-11; OP-HIST-19, OP-HIST-20. Confirmed product defect D20: the `GlobalRecords` query fetches other namespaces' result sets without switching namespace, so it returns the current namespace's rows once per namespace. It returned 458 rows, twice USER's 229, and none of USER-VERIFY's 173 (stage C case H10, `20261113T204152Z-stagec`). Not run: `Records` declares an ID column it does not return first (OBS-H11). No test.

Extension Verus: [history.rs::programmatic_global](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [G:D20 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d20). The linked entry qualifies the caller, reader and providers.

Open questions: [Must programmatic global history rows carry their namespace and ID?](../../Verus_verification/open_questions.md#history.read.global-shape)

### history.delete

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [History](#history).
- Children: [Does `-confirm` together with a filter delete every record, or only the matching ones?](../../Verus_verification/open_questions.md#history.delete.confirm-with-filter), [global unfiltered](#history.delete.global-unfiltered), [May `history delete` or the daily [purge](../concepts/data-policy.g5.md#history.retention) remove the record of an action that is still running, and what should that action's final write do then?](../../Verus_verification/open_questions.md#history.delete.running-records).

Sources: `Main.cls:825`, `:838`; `wiki/09` L187-195. Inventory: INT-HIST-12; OP-HIST-17, OP-HIST-18. Test, partial: `TestMultipleNamespaces` deletes with `-confirm` and `-globally`, never with a filter. Native evidence: E5 (unfiltered, refused) and E6 (filtered, exactly one row deleted) conform. E4 deleted everything (B2 D2) (`182845Z-cases-ef`, `184230Z-replay-fast`). Observed: delete also removes unfinalized records of actions still running ([May `history delete` or the daily [purge](../concepts/data-policy.g5.md#history.retention) remove the record of an action that is still running, and what should that action's final write do then?](../../Verus_verification/open_questions.md#history.delete.running-records)). PR #578 review r1816818561 by isc-kiyer asked for deletes scoped by filter.

Extension Verus: [history.rs::delete](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [Does `-confirm` together with a filter delete every record, or only the matching ones?](../../Verus_verification/open_questions.md#history.delete.confirm-with-filter), [Must global delete report namespace errors other than missing permission?](../../Verus_verification/open_questions.md#history.delete.global-errors), [May `history delete` or the daily [purge](../concepts/data-policy.g5.md#history.retention) remove the record of an action that is still running, and what should that action's final write do then?](../../Verus_verification/open_questions.md#history.delete.running-records)

### history.delete.global-unfiltered

- human-provided
- user-facing-behavior, security
- Expands: [Delete](#history.delete).

Sources: `wiki/09` L46, L195 ("`-globally` bypasses the `-confirm` requirement, so `history delete -globally` with no filter deletes every record on the instance without prompting"); `Main.cls:825` ("Ignored if -globally is passed too"). Inventory: A1 Q-12, OBS-H9; OP-HIST-18. This was Q-12. Reconstruction check R10 found the behavior documented, so it is a decision, not open intent. Proposal (not adopted): require `-confirm` for an unfiltered global delete.
