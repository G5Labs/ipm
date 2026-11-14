# Follow-up state-machine models

Two executable models added by the follow-up review. They are explicit-state models in Python that enumerate every configuration of a bounded domain and every end point, then check contract properties at each end state. They are not Verus proofs. They complement the original Verus crate (`crates/ipm-load-history`), whose transaction mode is one global value and whose history model covers writers only.

Run from `.g5/Verus_verification`:

```sh
python3 -I -B followup/bin/receipt.py loadtx-model "expected verdict" --inputs followup/models/loadtx.py -- python3 -I -B followup/models/loadtx.py {OUT}
python3 -I -B followup/bin/receipt.py history-readers-model "expected verdict" --inputs followup/models/history_readers.py -- python3 -I -B followup/models/history_readers.py {OUT}
```

Current receipts: `followup/results/20261114T002647Z-final-loadtx-model` (about 2 minutes) and `followup/results/20261114T002647Z-final-history-readers-model` (seconds). Each writes a JSON result beside its receipt. The exit code is 0 only if every correspondence row or the valid-neighbor check holds. The substantive results are the violation counts below.

## `loadtx.py`: load with per-activation modes, process ends and retry (R05, R06)

```
 caller params ──┐
 top <Defaults> ─┼─ merged per key ─▶ top activation ─── LoadDependencies ─▶ dep1, dep2 (flat, dependency order)
 dep <Defaults> ─┘                     │                                      each merges its own <Defaults>
                                       │                                      under the top's merged params
                                       ▼
    journal store: open levels with undo entries (node, before, after); journaling flag;
    IRISTEMP (never undone); module records; History twins
                                       │
    end points: returned · caller commit · caller rollback · Terminate or SIGKILL before every step
    observations: transient (before cleanup), settled (after rollback), retry from settled
```

What it represents. Every source line it transcribes is cited in the module docstring. The main ones:

- mode resolution per activation (`src/cls/IPM/Utils/Module.cls:1165-1172`, `:1191-1196`)
- the no-transaction marker (`:1224-1227`)
- developer-mode commits C1 and C2, and the final commit C3 (`:1309-1351`)
- exit rollback (`:1357-1362`)
- the dependency's own log created and closed in `LoadModuleReference` (`:58-67`)
- `Finalize`'s `Committed || OK` (`AbstractHistory.cls:127-133`)
- `%OnClose` persistence (`HistoryTemp.cls:30-70`)
- IRIS premises P-01–P-06, R-01 and the new P-14

Reduction for nested and shared dependencies. The top's `LoadDependencies` loads the transitive closure in dependency order with the top's params. Each dependency's own `LoadDependencies` finds its dependencies already installed in the same process and skips them (`Storage/Module.cls:1139-1154`). So a chain m → d1 → d2 is the flat sequence d2, d1 under m, and a shared dependency is loaded once. The model's two flat dependency slots therefore stand for nested and shared graphs of up to two dependencies. Native runs N-R05-NESTS, N-R05-NESTF and N-R05-DIA confirm the order, the single load and the params (below).

Bounds.

- Activations: 0, 1 or 2 dependencies.
- Modes: caller flag, top `<Defaults>` and each dependency's `<Defaults>`, each one of none, NoTransaction or NoJournal.
- Developer mode on or off for the top.
- Faults: none, or an Activate failure in one activation.
- Caller transaction or not.
- Ends: returned (with caller commit and caller rollback), plus a Terminate or SIGKILL before every step (without a caller transaction).
- A retry from every settled process end.

That gives 1,692 configurations and 62,764 end states as built. Repair variants have more steps and so more end states. Not represented: a dependency installed at another version, which native cases cover instead (N-R05-DEVUP, DEVSAME, HIGHER, HIGHEROK); manifest refusals, which the original crate covers; several faults in one run; concurrency.

Contract clauses (independent of the journal mechanics):

| Clause | Statement |
|---|---|
| REC-EXISTS | every started action has a record at a returned or settled end (#366, Q-06b provisional) |
| COMMITTED | a finalized, ended record of an action outside a caller transaction reads `Committed` = "its changes were kept" (dependency: its own module; top: any module) |
| PREFIX | after any end, each module is in the state given by the writes whose durability point came before the end. Durability points: at once for a write outside a transaction or with journaling off; at the commit that returns the load to the caller's level for a transactional write; at the caller's commit with a caller transaction. This replaces the original contract's "any state" for process ends of non-transactional loads (R06a) and the broad developer-mode alternatives (R06b) |
| TLEVEL | a returned load leaves `$TLEVEL` at its entry level |
| RETRY | from every settled process end, a rerun without the fault installs every module |

Policy parameters. Q-22 (A: a dependency's own NoJournal makes its writes durable at once, as built; B: it cannot weaken the parent's transaction). Q-23 (A: a dependency's own NoTransaction suppresses the parent's transaction; B: it is ineffective inside it, as built).

Results (`followup/results/20261114T002647Z-final-loadtx-model/loadtx.json`), under the product's policy Q-22 A, Q-23 B unless stated:

| Variant | What it is | Result |
|---|---|---|
| as-built | the product | PREFIX 0 at every end, TLEVEL 0, RETRY 0 failures. COMMITTED 30, all with a dependency whose own `<Defaults>` set NoJournal under a transactional parent: the top record reads "nothing kept" while that dependency's changes were kept. REC-EXISTS 9,942: dependency rows lost with the parent's transaction (D1), including a NoJournal dependency's row at a Terminate (below), and records lost with a caller's rollback (Q-05/Q-06b deviation). Under Q-22 B: PREFIX 2,440 (D14). Under Q-23 A: PREFIX 5,780 (the product does not suppress the parent's transaction) |
| REPAIR-103 | the original D1 repair design, with the journaled `Committed` mark as its own step before the commit | REC-EXISTS for dependencies fixed (846 remain, all the top record at a caller rollback). COMMITTED 962: 818 dependency violations with a dependency's own NoJournal (27 returned, 791 process ends), 60 top-record ones from the same cause, and 84 dependency-clause violations in 60 end states (30 distinct states, each reached from two adjacent cut points) from a Terminate between the mark and the commit (P-14, below). This window exists because this variant follows the design text "dependency rows are finalized at the top-level close" and IRIS runs that close at a Terminate (P-06b). The crate's variant 103 models the mark as atomic with the commit and finalizes dependency rows only at a returned close, so it cannot show the window |
| REPAIR-103-GUARDED | the close finalizes dependency rows only when no transaction of the load is open | COMMITTED 141, every one with a dependency's own NoJournal. Part of the drop comes from rows left unfinalized, which this model's COMMITTED does not check |
| REPAIR-103-M-LATE-MARK | mutant: an unjournaled write to the dependency rows between the mark and the commit | COMMITTED 1,004 (42 more than REPAIR-103): detected |
| REPAIR-NAIVE | only make dependency rows survive; keep today's `Committed` logic | COMMITTED 2,712: rolled-back dependencies read `Committed=1` through `Committed || OK` and the no-transaction marker (proposal S3) |
| M-GLOBAL-MODE | dependencies ignore their own `<Defaults>` (the original crate's assumption; also design S8) | PREFIX 3,080 under the product's policy: detected, because it cannot produce the native M5 and N-R05-NESTF outcomes. COMMITTED 0 under every option |
| M-BEFORE-IMAGE | rollback restores every before-image unconditionally | identical results to as-built: not distinguishable in this model, because as built no unjournaled write follows a journaled one on the same node before a rollback. Refuted natively by N-P21A and N-P21R. An earlier run that closed objects in the wrong order made it look detected (see Revisions) |
| M-JOURNAL-NOT-RESTORED | journaling not restored after a NoJournal dependency returns | PREFIX 796, COMMITTED 80: detected |

Process ends (R06). Transient and settled states differ in 5,328 end states. That is the window between the end and IRIS's rollback, which native SIGKILL runs measured at 105 s to about 10 min (P-05, P-13). A permitted partial prefix: a NoTransaction load ended after its manifest write and before its compile keeps the module record without resources. An impossible state the PREFIX clause rejects and the original `any_alt` accepts: the same load with resources loaded and no module record. The original contract's `l1_alts_kill` returns `any_alt` there (`crates/ipm-load-history/src/contract.rs:189`). Retry from every settled end converges. `prefix_cases.py` (receipt `followup/results/*-prefix-cases`) runs six such cases: three permitted partial prefixes are permitted, and three impossible states (resources without a module record after a NoTransaction end; a dependency installed while the `-dev` parent's manifest is absent; resources gone after a `-dev` compile that ran outside the transaction) are rejected by PREFIX and accepted by the original `l1_alts_kill`.

Correspondence (exit code). 12 rows, all hold:

- the original cases B1-C31, B1-C34 and B1-C38;
- native M5;
- the control that the global-mode model cannot produce M5;
- the native predictions N-R05-Q23F and N-R05-Q23S;
- native N-P19 (the close order at a Terminate, below);
- the four native P-14 probe sequences (N-P20c, N-P20c2, N-P21A, N-P21B).

Close order at a Terminate (P-06c, native N-P19). IRIS closes live objects innermost frame first. The frames, outermost first: `Main.Load` (top log), the top `LoadNewModule` (top JournalManager), `LoadModuleReference` (dependency log), the dependency's `LoadNewModule` (dependency JournalManager). A JournalManager's `%OnClose` restores the journaling it found (`src/cls/IPM/Utils/JournalManager.cls:14`, `:22-25`). So a dependency whose own `<Defaults>` set NoJournal re-enables journaling before its log closes. Its row is then written journaled and rolled back with the parent's transaction.

What PREFIX does and does not check. PREFIX takes its durability points from the contract (commits that return the load to the caller's level, writes outside a transaction or with journaling off), not from the store's undo log. It reads where those commits occur from the as-built trace's own commit steps. A variant that never committed would therefore still show PREFIX 0. COMMITTED and RETRY catch that case.

End states. An end state is a (configuration, end kind, cut point) triple. Adjacent cut points can reach the same state, so counts of end states overstate distinct states.

Premise P-14 (new, native `followup/results/native/20261113T234716Z-p21-ordering-r2`). A rollback undoes a journaled write only if the node still holds the value that write set. So an unjournaled write after a journaled one survives the rollback, and the journaled value is not restored either. This is an inference from four observations, not a documented IRIS guarantee.

## `history_readers.py`: history consumers (R04)

What it represents.

- `ConstructSQLWhere`, which stops at the first unusable key (`src/cls/IPM/General/History.cls:93-126`)
- `GetHistory` (`:132-147`)
- `DeleteHistory` (`:151-159`)
- the `history delete` guard (`src/cls/IPM/Main.cls:4161-4173`)
- `DeleteHistoryGlobally`, which ignores every exception (`History.cls:199-230`)
- `GetHistoryGlobally` and `GlobalRecords` (`:233-317`)
- `history details` (current namespace)
- `AutoPurgeOldHistory` with its quoted cutoff (`:162-181`)
- default modifiers merged into the filter (`Main.cls:1067-1070`, `CLI.cls:815-827`)

Bounds.

- Database: 4 namespaces (one locked, one without IPM) and 6 distinguishable rows. IDs collide across namespaces, one row is in progress and three are older than the cutoff.
- Filters: up to two keys from six (a real column with an underscore, a case variant, an unknown column), with 2–3 values each.
- Default modifiers: none or NoTransaction.
- Commands: find (two orders and limits), global find, local delete with and without `-confirm`, global delete, `GlobalRecords`, details 1–3, purge with retain 0 or 30 and already run today or not.

That is 524 commands per variant.

Contract.

- SEL: exactly the rows matching every filter on a real column. An unknown key is refused.
- DEL: removes exactly the selection, and refuses a local delete with no effective filter unless `-confirm`.
- PURGE: removes exactly the rows started before the cutoff, in every namespace, once a day. By default the model keeps in-progress rows (option QA). That is the alternative to Q-20's provisional answer, which lets a purge remove any matching record (open question Q-29). The run under the provisional answer, reported as `as-built (QA: active rows may be purged)`, gives the same violation counts.
- GLOBAL: every namespace's rows once, with their namespace.
- DETAILS: the current namespace's row.

Results (`followup/results/20261114T002647Z-final-history-readers-model/history_readers.json`):

| Variant | Result |
|---|---|
| as-built | find: 104 STATUS (an unknown key or default modifier shows no rows with OK instead of an error) and 20 ROWS (filters dropped). Delete: 24 DELETED-UNSELECTED (rows outside the selection deleted, D2). Global delete: 12 DELETED-UNSELECTED and 52 STATUS (errors ignored). GlobalRecords: ROWS (D20). Purge: PRESERVED-SELECTED (D3) |
| valid neighbor | the 69 commands with plain-column filters and no default modifiers match the contract exactly |
| M-SKIP-INVALID (deliberately wrong: skip unusable keys instead of stopping) | agrees with the archived X2 and X3 runs, so archived evidence cannot tell it apart. It predicts 1 row for N-R04-SEL where the as-built model predicts every row. Native N-R04-SEL returned every row (9), so the wrong model is refuted |
| M-NO-QUOTES (purge with an unquoted cutoff) | contradicts the archived no-op purge X4, so the quotes are the cause of D3. With D3 fixed, it would also purge the in-progress row, which matters for Q-20 |

Archived correspondence (all hold for as-built): X2, X3, X4, H2 (find and delete) and H10. Native follow-up cases (`followup/results/native/20261113T234810Z-r04-history-readers`): N-R04-FIND, SEL, SEL2, ORDER, UNKNOWN, DEL and CUTOFF all conform to the as-built predictions written before the run.

Not run natively. The global readers and global delete need a second IPM namespace, which the dedicated container does not have. Installing IPM into another namespace was out of proportion to the question, and the archived H10 run covers `GlobalRecords` and `find -globally`. The widening delete (D2) and the purge deletion were not re-created. Their selection was observed read-only (N-R04-SEL, N-R04-CUTOFF).

## Revisions

- After the independent technical review (`.g5/uplift/ipm-quality-followup/review/review-technical.md`): Terminate now closes JournalManagers and logs innermost frame first (P-06c), and N-P19 was added as a correspondence row. As-built REC-EXISTS rose from 9,558 to 9,942, and the M-BEFORE-IMAGE mutant, previously reported as detected, is equivalent in this model. All other counts are unchanged. Superseded receipt: `followup/results/20261113T235302Z-loadtx-model`.
