# Design simplifications for IPM load and history

This file proposes design changes that would make the load and history guarantees easier to keep. They are proposals for the IPM maintainers. Nothing here was implemented in the product. The original six proposals (S1–S6, commit `ae619795`) are kept with their IDs and corrected. S7–S9 are new in the follow-up review.

What was modeled. Earlier versions of this file said that none of the proposals had been modeled. That was not accurate. The dependency part of S2 is repair variant 103 in the original Verus crate, and its checks pass when every activation of a load uses the same transaction mode. The follow-up checked S2, S3, S4 and S8 in executable state-machine models, and S2's premise natively. Each proposal states its exact model evidence or says that it was not modeled.

Paths are relative to `.g5/Verus_verification/`. `FR` is `followup/results/`. Product paths start with `src/` and refer to commit `571e04a1`. Finding IDs (D1, I2, …) refer to `bugs_issues.md`, question IDs (Q-17, …) to `open_questions.md`.

## Summary

| ID | Proposal | Owner in the product | Removes | Policy choice it makes | Model evidence |
|---|---|---|---|---|---|
| S1 | Validate the whole load before the first write | `src/cls/IPM/Utils/Module.cls` `LoadNewModule` | D4, D9, D11, D12, V4, V7 | Q-17 (chooses option A) | none as a variant. The decisive cases are already L1 cases |
| S2 | One durable history row per action, written so no rollback can remove it | `General/HistoryTemp.cls`, `General/AbstractHistory.cls` | D1, D10, V1, V2, V9, V10 | Q-05/Q-06b for caller transactions | repair variant 103 (original crate); follow-up `models/loadtx.py` variants REPAIR-103, REPAIR-103-GUARDED |
| S3 | Mark `Committed` only where a write is kept | `AbstractHistory.cls` `Finalize` | needed for S2 to be correct | none | control 05 (original); REPAIR-NAIVE (follow-up) |
| S4 | Structured history filters | `General/History.cls` `ConstructSQLWhere`, `AutoPurgeOldHistory` | D2, D3, D24, D18 (if Q-18 says filters only) | Q-10, Q-18 | `models/history_readers.py` |
| S5 | One exit path per phase | `Storage/Module.cls` `ExecutePhases` | D6, V5 | Q-14 (chooses option A) | none as a variant |
| S6 | Declared setting defaults | `Repo/UniversalSettings.cls` | D5 | none | none |
| S7 | A temporary repository that cannot outlive or fail its load | `General/TempLocalRepoManager.cls`, `Main.cls` `LoadInternal` | D13, I2 | none | none |
| S8 | One transaction plan per load | `Utils/Module.cls` `LoadNewModule`, `LoadDependencies`, `LoadModuleFromDirectory` | D14, D21, D22, D28 and the per-dependency-mode condition on the D1 repair | Q-22, Q-23 (the maintainer chooses the rule) | `models/loadtx.py` M-GLOBAL-MODE |
| S9 | Namespace-aware history readers and setting scope | `General/History.cls` global readers, `Main.cls` default modifiers | D19, D20, D23, I4 | Q-12 (D block), Q-16 | `models/history_readers.py` |

Findings that get no separate simplification, with the reason:

- A developer-mode dependency replaced and its developer mode cleared (I3). This is a policy decision, not a structural problem. Once decided, the rule belongs in S8's load plan.
- I1 (unfinished phase displayed as running). This is a reader display choice that S2 and S5 make rarer but do not settle.
- `.tar.gz` (D15) and relative paths (D17). These are local input checks, and S1's single validation point is where they would live.

## S1: Validate the whole load before the first write

Old complexity. Refusals are spread over the write sequence of `LoadNewModule` (`src/cls/IPM/Utils/Module.cls:1152-1364`). Path checks run in `Main:LoadInternal`. The downgrade and update-required guards run inside the transaction (`:1213-1222`). The no-transaction marker sets `Committed` before the manifest import (`:1224-1227`). Manifest validation happens inside and after `$system.OBJ.Load` (`:1251-1264`). System requirements are checked after the developer-mode commit (`:1321`). So whether a refusal writes anything depends on the mode and on which check fires.

What gets simpler. Read the manifest once (`GetModuleObjectFromPath`, `:1164`). Name the log. Run every refusal on the in-memory object: path, manifest validity, downgrade, update-required, system requirements, and the developer-mode overwrite guard. Only then start the transaction or set the no-transaction marker. Every refusal then writes nothing in every mode, and the record names the package.

Preserved behavior. Developer mode keeps the manifest and phase work of an installable module that fails later (B1-C26). Same-version loads succeed (B1-C19). A refusal still reads `Committed=0`.

Decisive neighboring pair. B1-C23 (system requirements refused under `-dev`) today keeps the manifest. Under S1 it must leave the namespace unchanged. B1-C26 (`-dev` Activate failure) must keep its manifest and resources as today.

Policy choice. Adopting S1 answers Q-17 with option A ("check system requirements before any write"). It does not make the question moot. If the maintainers choose option B (requirements may be checked after a developer-mode commit), S1 must keep that one check where it is.

Tradeoffs. A manifest read twice (once in memory, once by `OBJ.Load`) must not diverge. Validation that depends on the imported document (multiple documents, B1-C51/C52) has to run against the parsed file instead.

Evidence. Not modeled as a variant. The L1 contract already encodes the required outcome. The cases it would change are B1-C08, C09, C10, C11–C15, C18, C23, C24, C51 and C52 (re-evaluated in `FR/20261114T002646Z-final-replay-reeval/delta.md`). Native B2-B4 and g3 show today's behavior.

## S2: One durable history row per action

Old complexity. A log has an IRISTEMP copy and a namespace twin. The twin is written at `Init` and from `%OnClose` (`src/cls/IPM/General/HistoryTemp.cls:30-70`). Whether it survives depends on the transaction level at those two moments and on when IRIS runs `%OnClose` (premises P-06a–c). A dependency's log is created and closed inside its parent's transaction, so a parent rollback removes it (D1).

What gets simpler. Every setter writes the durable row directly, with process journaling off for that save (`%IPM.Utils.JournalManager(0)`), so no rollback and no process end can remove it. IRISTEMP becomes a cache. `Committed` is the one field written journaled, set to 1 just before the commit that makes the action's changes durable, as the row's last write before that commit. Any rollback then restores 0.

Corrections from the follow-up (model `models/loadtx.py`, receipt `FR/20261114T002647Z-final-loadtx-model`):

1. One transaction mode per load. Variant 103 meets every dependency clause only when every activation uses the load's mode. A dependency whose own `<Defaults>` set NoJournal keeps its changes after a parent rollback (the product's behavior, D14), but under S2 its row stays at `Committed=0`. That gives 818 dependency-clause violations (27 at returned ends, 791 at process ends). S2 therefore needs S8, or a rule that marks such a dependency's row committed at its own close.
2. Write ordering at the close. A rollback undoes a journaled write only if the row still holds the value that write set (new premise P-14, native `FR/native/20261113T234716Z-p21-ordering-r2`). IRIS runs `%OnClose` at a Terminate before its rollback (P-06b). If the top-level close finalizes dependency rows with journaling off while the load's transaction is open, a Terminate between the `Committed=1` mark and its commit leaves `Committed=1` on a dependency that was rolled back (60 end states, 30 of them distinct, in variant REPAIR-103). The window is two adjacent statements wide. It follows from the design text "dependency rows are finalized at the top-level close". The original crate's variant 103 models the mark as atomic with the commit and finalizes dependency rows only at a returned close, so it could not show this window. The guarded variant (REPAIR-103-GUARDED) finalizes dependency rows at the close only when no transaction of the load is open, and it removes every violation that does not come from a dependency's own NoJournal default (141 remain, all with such a dependency). That count is lower partly because the guarded close leaves dependency rows unfinalized at such a Terminate, and this model checks `Committed` only on finalized rows. The original contract allows an unfinalized dependency row at a process end. A mutant that writes the rows unjournaled between the mark and the commit is detected.
3. The premise S2 depends on is now stated precisely. The rollback-proof insert is R-01 (native N-P20). The ordering rule is P-14 (N-P20c, N-P20c2, N-P21A, N-P21R). The original text described the hazard of a journaled insert followed by unjournaled updates (N-P20c2). The follow-up shows the opposite order, an unjournaled write after the journaled mark, is the one that defeats the rollback.

Preserved behavior. Finalized rows stay immutable (`history.locked`). A top-level failure reads `Committed=0`. IDs follow start order. In developer mode, a dependency committed by C2 keeps `Committed=1` when the parent fails later (B1-C38).

Decisive neighboring pair. B1-C31 (default parent fails after its dependency loaded): the dependency row must survive with `Committed=0`. B1-C38 (`-dev` parent fails after C2): the dependency row must read `Committed=1`. The as-built evaluator could not have checked the first one correctly (R02a in `followup/replay/`). The corrected evaluator's control "Committed per slot" does.

Unresolved choices. Whether IPM may write history outside a caller's transaction at all (Q-05, Q-06b). Writing it unjournaled makes the record survive the caller's rollback. Whether that is wanted is a maintainer decision. The rule for dependencies whose own defaults turn journaling off depends on Q-22.

Tradeoffs. IRIS mirroring replicates through the journal, so a history row written with journaling off would not reach a mirror member. Neither the original uplift nor this review tested mirroring, and the update framework distinguishes mirror members (`src/cls/IPM/General/Update/VersionBase.cls`).

Evidence. Original: variant 103, `repair-103-dep-clauses` PASS (189,577 states) and `repair-103-invariants` PASS, rerun on the final inputs (`FR/20261113T233031Z-repair-103-dep-clauses`, `FR/20261113T233031Z-repair-103-invariants`). Follow-up: `FR/20261114T002647Z-final-loadtx-model` variants REPAIR-103, REPAIR-103-GUARDED, REPAIR-103-M-LATE-MARK.

## S3: Mark `Committed` only where a write is kept

Old complexity. `Finalize` sets `Committed = Committed || ISOK(status)` (`src/cls/IPM/General/AbstractHistory.cls:127-133`). On the top-level load path this never changes the value, because every successful path already calls `SetCommitted` (original control 05 survives as an equivalent mutant). For a dependency it does change it: a successful dependency closes with `Committed=1` even if its parent later rolls it back. Today D1 hides this because the row is lost.

What gets simpler. `Committed` is written only where a write becomes durable. `uninstall`, which has no transaction and relies on `|| ISOK`, marks it explicitly.

Preserved behavior. A successful top-level load reads `Committed=1`. An uninstall's `Committed` keeps meaning "it succeeded" (`AbstractHistory.cls:41-44`).

Decisive neighboring pair. A successful dependency under a parent that then fails (must read 0 once its row survives) versus a successful top-level load (reads 1 through the C3 marker).

Evidence. Follow-up variant REPAIR-NAIVE, which only makes dependency rows survive and keeps today's `Committed` logic, has 2,712 clause violations: rolled-back dependencies read `Committed=1`, through `|| ISOK` and the no-transaction marker. So S3 is required by any D1 repair, not an independent clean-up.

## S4: Structured history filters

Old complexity. `ConstructSQLWhere` parses operators out of strings and stops at the first key whose name is not alphanumeric (`src/cls/IPM/General/History.cls:93-126`). Every key after it in collation order is dropped too. An unknown column makes the query fail, and `find` shows an empty listing with an OK status. The retention purge builds its cutoff as a quoted string (`:173`), so its comparison selects nothing.

What gets simpler. Filters are `(column, operator, value)` triples. Columns are checked against the table, including real columns with an underscore such as `Version_Major`. An unknown column is refused with an error. The purge passes a triple with an unquoted cutoff.

Worked preserving and refusing pair (native `FR/native/20261113T234810Z-r04-history-readers`, all read-only except an exact delete of two test rows):

| Case | Filter | Today | Under S4 |
|---|---|---|---|
| N-R04-FIND (preserve) | `Package=fu-hr-a` | 2 rows | 2 rows |
| N-R04-DEL (preserve) | delete `Package=fu-hr-del` | exactly the 2 matching rows deleted, others kept | same |
| N-R04-SEL (real underscore column) | `Version_Major=1`, `package=fu-hr-b` | every row (9) | 1 row |
| N-R04-SEL2 | `Package=fu-hr-b`, `Version_Major=1` | 2 rows (version dropped) | 1 row |
| N-R04-UNKNOWN (refuse) | `NoSuchCol=1` | no rows, status OK | an error naming the column |
| N-R04-CUTOFF (purge selection) | `TimeStart <= cutoff` | quoted cutoff selects 0 rows | the 40-day-old row |

Preserved behavior. Wildcard `*`, the comparison operators, the `-confirm` gate and the SQL-injection guard on column names.

Policy choices. Whether default modifiers are filters (Q-18). Under S4 a default modifier that is not a column would be refused, so S4 together with Q-18 option A removes D18. Whether an in-progress record may be purged (Q-20, Q-29). The model shows that once D3 is fixed, an in-progress record older than the cutoff is purged unless S4's purge triple excludes unfinished rows.

Evidence. `models/history_readers.py` (receipt `FR/20261114T002647Z-final-history-readers-model`): the 69 commands of the valid neighbor (plain columns, no default modifiers) match the contract exactly. The deliberately wrong variant "skip invalid keys" (a tempting partial fix) still differs from the contract and is refuted natively by N-R04-SEL. So skipping instead of stopping is not a repair. The variant with an unquoted cutoff is refuted by the archived no-op purge (X4), which confirms the quoting is the cause of D3.

## S5: One exit path per phase

Old complexity. In `ExecutePhases` a phase's end is recorded in the catch for thrown errors and on the success path (`src/cls/IPM/Storage/Module.cls:692-711`). A returned error status from an Invoke leaves the loop through `quit:$$$ISERR(tSC)` (`:669-671`) and bypasses both, so the phase stays open (D6).

What gets simpler. Record the phase end in one place that every exit passes through.

Preserved behavior. A thrown Activate error records the phase as ended with that error (B1-C25). A successful phase records OK.

Decisive neighboring pair. B1-C28 (an Activate-phase Invoke returns an error) today leaves the phase open and must record it ended with the error. B1-C25 (Activate throws) already records it and must not change.

Policy choice. Adopting S5 answers Q-14 with option A.

Evidence. Not modeled as a variant. The L1 clause with Q-14 option A rejects B1-C28 and accepts B1-C25 (`FR/20261114T002646Z-final-replay-reeval`). Native: archived `repro-activate-invoke-phase-open`.

## S6: Declared setting defaults

Old complexity. `ResetToDefault` reads a stored "default" node that the first `SetValue` creates (`src/cls/IPM/Repo/UniversalSettings.cls:87-105`, `:150-163`). So `config delete` restores the first value ever set rather than a factory default (D5).

What gets simpler. One table of factory defaults that both `GetValue` and `ResetToDefault` read.

Preserved behavior. `config set` and `config get` round-trip. Settings without a factory default keep their current special cases (`:95-99`).

Decisive neighboring pair. After `config set HistoryRetain 7`, `config set HistoryRetain 0` and `config delete HistoryRetain`, the value must be the factory default (0, retention off). Today it is 7 (archived run `results/native/20261113T184628Z-repro-config-delete-retain`). The neighbor `config set HistoryRetain 30` then `config get` must still return 30.

Evidence. Not modeled. The decisive case is archived natively.

## S7: A temporary repository that cannot outlive or fail its load

Old complexity. Loading a directory registers its `.modules` folder as a filesystem repository with first priority (`OverriddenSortOrder = -1000`, `src/cls/IPM/General/TempLocalRepoManager.cls:28`). It is created before the transaction (`src/cls/IPM/Main.cls:2528`) and removed only by an explicit `CleanUp()` (`TempLocalRepoManager.cls:73`). A cleanup error after the load committed turns a successful load into a reported failure (D13). A process end leaves the repository registered, and it then shadows later dependency resolution (I2).

What gets simpler. Give the temporary repository a name scoped to the process and the load, run its cleanup in a block that cannot change the load's status, and remove leftovers whose owning process no longer exists the next time a repository is resolved.

Preserved behavior. Dependencies in `.modules` are found before every other repository during the load (B1-C30, N-R05-DIA, N-R05-NESTS).

Decisive neighboring pair. B1-C29 (cleanup fails after the commit) must report the load's success with a warning. B1-C30 (normal cleanup) must not change.

Tradeoffs. A sweep needs a reliable "owner is gone" test. Two concurrent loads of the same directory would each need their own repository.

Evidence. Not modeled. Native: B1-C29 (`FR/20261114T002646Z-final-replay-reeval`), archived `results/native/20261113T210229Z-temp-repos-leftover`.

## S8: One transaction plan per load

Old complexity. Every `LoadNewModule` activation decides for itself whether to start a transaction or turn journaling off, from its caller's params merged over its own `<Defaults>` (`src/cls/IPM/Utils/Module.cls:1165-1172`, `:1191-1196`). A parent's defaults reach its dependencies, but a dependency's own defaults apply only to the keys nobody above set (follow-up L-1). The consequences:

- a dependency's own NoJournal escapes the parent's rollback (D14);
- a dependency's own NoTransaction does not (Q-23, observed natively for the first time in N-R05-Q23F);
- because the top-level activation loads every transitive dependency itself (L-3), a middle dependency's defaults do not reach its own dependencies, so after a failed load a NoJournal dependency can stay installed while its own dependency is rolled back (N-R05-NESTF);
- the no-transaction marker fires per activation, so the top record reads "nothing kept" while a dependency's changes were kept (L-8);
- a dependency's developer mode is overridden on every reload (L-5).

What gets simpler. Before the first write, compute one plan for the whole load from the caller's flags, the top module's defaults and the dependencies' defaults, by a rule the maintainers choose (for example "the load runs in one transaction unless the caller or the top module opts out", or "any opt-out in the closure makes the whole load non-transactional"). Every activation then follows the plan. The history and `Committed` logic sees one mode, which is the domain where repair 103 is already proven.

Preserved behavior. A load whose modules declare no defaults behaves as today. A caller's `-DNoTransaction=1` or `-DNoJournal=1` keeps its meaning.

Decisive neighboring cases. N-R05-NESTF (parent fails; middle dependency declares NoJournal): under a single plan the namespace is either fully rolled back or keeps the whole dependency closure, never a module without its dependency. N-R05-Q23S and N-R05-NESTS (successful loads) must not change.

Policy choice. S8 makes Q-22 and Q-23 one decision. It does not answer them.

Evidence. `models/loadtx.py` variant M-GLOBAL-MODE is the "one mode per load" design, with the dependency defaults ignored (receipt `FR/20261114T002647Z-final-loadtx-model`). It has no `Committed` violation under any option, so the 30 top-record and 818 repair-103 dependency violations disappear. Under Q-22 B and Q-23 B, 144 end states still differ from the contract. All are caller rollbacks of a load that runs without its own transaction while a dependency declares NoJournal: the contract as written still lets that dependency's NoJournal survive the caller's rollback, and the single plan ignores it. The plan rule has to say which wins. Under the product's current policy (Q-22 A) the variant differs in 3,080 end states, as expected, because it no longer lets a dependency's NoJournal escape.

## S9: Namespace-aware history readers and setting scope

Old complexity. History IDs restart in each namespace, but the global readers do not carry the namespace. `GlobalRecords` returns no namespace or ID column, and it reads the caller's namespace once per namespace (D20, archived H10). `DeleteHistoryGlobally` ignores every exception in a namespace, not only permission errors (`src/cls/IPM/General/History.cls:199-230`), so a global delete with an unusable column reports "Deleted 0 record(s)" while the local delete fails. "Namespace" default modifiers are stored instance-wide (D19).

What gets simpler. Every global reader row carries `(Namespace, ID)`. A global delete reports each namespace's outcome and skips only namespaces it may not read. Default modifiers are stored per namespace, or documented and named as instance-wide.

Preserved behavior. `history find -globally` already lists rows per namespace correctly (archived H10). The documented global delete without `-confirm` (D block `history.delete.global-unfiltered`) stays.

Decisive neighboring pair. `GlobalRecords` from a namespace with three rows, with a second IPM namespace present, must return every namespace's rows once with their namespace. `history find -globally` must not change.

Evidence. `models/history_readers.py`: `globalrecords:ROWS` and `delete-global:STATUS` violations in the as-built variant. The dedicated follow-up container has one IPM namespace, so the global cases rely on the archived H10 run.
