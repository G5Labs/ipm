# 02 — Transaction history

Module root: HIST-0

Scope: the history log written by `load`, `install`, `update` and `uninstall` (`%IPM.General.AbstractHistory`,
`HistoryTemp`, `History`, `HistoryPhase`) and the `history` command. Uninstall semantics of the record are module 05.
Models: `models/explorer` (L1 rules explored over every configuration and crash point) and
`models/verus/load_general.rs` (L2 refines L1 under named premises; witnesses for each premise).

## Glossary
| Term | Defined by |
|---|---|
| history record | HIST-D1 |
| action | HIST-D2 |
| success | HIST-D3 |
| committed | HIST-D5 |
| end time | HIST-D6 |
| finalized | HIST-D7 |
| working copy | HIST-D8 |
| dependency record | HIST-D9 |
| retention | HIST-D10 |
| kept | HIST-D4 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| HIST-0 | SYS-0 | R | IPM keeps a log of what each package command did. | I | ipm/src/cls/IPM/General/History.cls#L3 | Module root. Scoped by namespace. |
| HIST-D1 | HIST-0 | D | A **history record** is one row that describes one command. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L8-L49 | `%IPM_General.History`, ordered by ID. |
| HIST-D2 | HIST-0 | D | The **action** of a history record says what the command did. It is load, install, update or uninstall. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L12 | VALUELIST `,load,install,uninstall,update`. |
| HIST-D3 | HIST-0 | D | **Success** in a history record shows if the command worked. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L38-L39 | A %Status; 0 while running or after a crash. |
| HIST-D4 | HIST-0 | D | A change is **kept** when it is still there after the command is over. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L41 | Database changes only (PR #578 review; DEC-HIST-5 resolved). Intent: DEC-HIST-5 resolved (evidence), decisions/research/hist.md. |
| HIST-D5 | HIST-0 | D | **Committed** in a history record shows if any change of the command was kept. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L41-L44 | For uninstall it only shows success (module 05). |
| HIST-D6 | HIST-0 | D | The **end time** of a history record is when the command ended. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L32-L33 | `TimeEnd`, empty while running or after a crash. |
| HIST-D7 | HIST-0 | D | A **finalized** history record is locked. No one can edit it. | O:ipm-finalized-history-row | ipm/src/cls/IPM/General/History.cls#L13-L22 | UPDATE trigger on `Finalized`; delete is still allowed. |
| HIST-D8 | HIST-0 | D | The **working copy** is a second copy of the history record. It is kept where a rollback cannot undo it. | O:env-iristemp-survives-rollback; I | ipm/src/cls/IPM/General/HistoryTemp.cls#L1-L26, #L124 | `^IRIS.Temp.HistoryTempD` maps to IRISTEMP. It is copied to the real row when the log object closes. |
| HIST-D9 | HIST-0 | D | A **dependency record** is the record a dependency load writes. It is its own row. | O:ipm-dep-ok | ipm/src/cls/IPM/Utils/Module.cls#L64-L68 | Action `load` (or `install` during an install), source = the registry. |
| HIST-D10 | HIST-0 | D | **Retention** is the number of days that IPM keeps history records. | I | ipm/src/cls/IPM/Main.cls#L840-L842 | `config set HistoryRetain <days>`; 0 turns it off. |
| HIST-B1 | HIST-0 | B | Each command writes one history record. This holds for load, install, update and uninstall. | O:ipm-load-ok; O:ipm-install-from-fs-repo; O:ipm-update-with-path; T:Test.PM.Integration.History; M:explore-L1-REC-TOP | ipm/src/cls/IPM/Main.cls#L2420, #L2566, #L4438 | Premise P1 (no caller transaction). An operator's delete can remove it, even while the command runs (HIST-B33). Intent: DEC-HIST-6 resolved (assumption), decisions/research/hist.md. No P: one record per command holds by construction of the model (review finding 5). |
| HIST-B2 | HIST-B1 | B | The history record is saved when the command starts. So it stays even after a crash. | O:ipm-halt-txn; O:ipm-terminate-txn; M:explore-L1-REC-TOP; P:g_refines; O:nc-gap-P1-docker-txn-after; O:nc-gap-P1-force-txn-after | ipm/src/cls/IPM/General/HistoryTemp.cls#L21 | Saved at $TLEVEL 0 before the load opens its transaction. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B3 | HIST-B1 | B | The history record of a failed command stays after its own rollback. | O:ipm-activate-fails; O:ipm-dep-parent-fails; M:explore-L1-REC-TOP; P:g_refines | ipm/src/cls/IPM/General/AbstractHistory.cls#L4-L7 | The working copy carries the result past the rollback. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B4 | HIST-B1 | B | The history record names the command, the user and the start time. | T:Test.PM.Integration.History | ipm/src/cls/IPM/General/AbstractHistory.cls#L46-L59 | `CommandString` is the shell text; `<unset>` when called outside the shell. |
| HIST-B5 | HIST-B1 | B | An install record names the registry the module came from. | T:Test.PM.Integration.History | ipm/src/cls/IPM/General/AbstractHistory.cls#L86-L97 | SourceName, SourceMoniker, SourceDetails; empty for load and uninstall. |
| HIST-B6 | HIST-B1 | B | The history record lists each phase with its times and result. | O:ipm-load-ok; T:Test.PM.Integration.History; O!:nc-gap-P3-n2-ph-after-txn; F:F-HIST-9 | ipm/src/cls/IPM/Storage/Module.cls#L569-L572, #L692-L711 | A phase that fails by a returned status from an Invoke may stay open (`#L669`, inspection only). Native: a phase whose Invoke fails by returned status stays open with placeholder ERROR #00 (F-HIST-9). |
| HIST-B7 | HIST-0 | B | Once IPM has read the manifest, the record of a command that ends names the module and its version. | O!:ipm-dev-downgrade-refused; F:F-HIST-5 | ipm/src/cls/IPM/Utils/Module.cls#L1207; ipm/src/cls/IPM/Utils/Module.cls#L1293 | Refused loads show package "" and version 0.0.0 although the name is known at L1207 (F-HIST-5, narrowed). The name may be empty only when it was never known (no or malformed module.xml; wiki 09 Package column). A crash may leave placeholders (HIST-N4). A URL load whose clone failed never read a manifest (F-LOAD-1). Intent: DEC-HIST-3 resolved (evidence), decisions/research/hist.md. |
| HIST-N4 | HIST-B7 | N | A crash can leave the record with no module name and a placeholder version. | O:ipm-halt-txn; O:nc-gap-P1-docker-txn-after; I | ipm/src/cls/IPM/General/AbstractHistory.cls#L14; ipm/src/cls/IPM/General/AbstractHistory.cls#L72 | Wiki 09: a process that dies mid-install leaves a record with an empty end time and a placeholder version. Intent: DEC-HIST-3 resolved (evidence), decisions/research/hist.md. |
| HIST-B8 | HIST-0 | B | A command that works gets success, and its history record is also committed. | O:ipm-load-ok; T:Test.PM.Integration.History; M:explore-L1-SUCCESS-COMMITTED; P:g_refines | ipm/src/cls/IPM/General/AbstractHistory.cls#L127-L134 | `Committed = Committed or OK`. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B9 | HIST-0 | B | A command that ends normally gets an end time, and its record is locked. | O:ipm-activate-fails; T:Test.PM.Integration.History; M:explore-L1-ENDED | ipm/src/cls/IPM/General/HistoryTemp.cls#L30-L58 | No P: the models have no Finalized field (review finding 5). |
| HIST-B10 | HIST-0 | B | After a crash the history record has no end time, and success is not set. | O:ipm-terminate-txn; O:ipm-terminate-notxn; O:ipm-halt-dev; M:explore-L1-ENDED; P:g_refines; O:nc-gap-P1-docker-txn-after; P:g_rollback_crash_honest | ipm/src/cls/IPM/General/AbstractHistory.cls#L33 | "This will be empty if the action is still in progress or ended abnormally." Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B11 | HIST-0 | B | For a load, install or update, committed off means none of its changes were kept. | O!:ipm-halt-dev-crash-in-dependency; F:F-HIST-2; O!:nc-gap-P1-docker-notxn-after; O!:nc-gap-P1-docker-dev-after; F:F-HIST-8; O!:r4-hist-graceful-stop-txn-after | ipm/src/cls/IPM/General/AbstractHistory.cls#L41-L43 | Fails only for a crash while a developer-mode load has its second transaction open (explore-L1-COMMIT-SOUND: 70 runs, all of that shape; `g_dev_crash_dishonest_unless_fixed`). A terminal operator commit is HIST-N2 (a documented operator error). Native: after an IRIS restart (no %OnClose), a kept developer-mode or no-transaction load reads Committed 0 (F-HIST-8). Proved characterization: honest under the fix except the window (g_instance_crash_honest_when_fixed). Native U19: a graceful `iris stop` that waits for a normal load lets it commit and then stops the job before %OnClose; after restart the load is kept but the record reads committed 0, unnamed and not finalized (r4-hist-graceful-stop-txn-after; F-HIST-8, RC-2). Intent: once changes are durable, Committed = 1 even across a crash or restart (SetCommitted comment AbstractHistory.cls#L119, issue #1237). Intent: DEC-HIST-2 resolved (evidence), decisions/research/hist.md. |
| HIST-B12 | HIST-B11 | B | Outside a crash while a developer-mode load has needs loading, that holds. | O:ipm-activate-fails; O:ipm-halt-txn; O:ipm-dev-downgrade-refused; M:explore-nocrash-L1-COMMIT-SOUND; P:g_refines; M:fold-abstraction-check-general | models/verus/load_general.rs | Premises PC, P1, P2, P3. Verus control `no-premise-pc` shows PC is needed. Generalized (load_general.rs `g_refines`): any number of dependencies and prior state, also premise PI (no IRIS restart while a developer-mode or no-transaction load has kept a change; F-HIST-8). Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B13 | HIST-0 | B | For a load, install or update, committed on means some change was kept. A no-transaction load may say so early. | O:ipm-notxn-activate-fails; M:explore-L1-COMMIT-EXACT; P:g_refines | ipm/src/cls/IPM/Utils/Module.cls#L1225-L1228 | A no-transaction load sets it before its first write. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B14 | HIST-0 | B | A developer-mode load that fails after its first commit is marked committed. | O:ipm-dev-activate-fails; O:ipm-dev-missing-dependency; T:Test.PM.Integration.History; P:g_refines | ipm/src/cls/IPM/Utils/Module.cls#L1311-L1314 | Seed table row 2. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B15 | HIST-0 | B | A developer-mode load can be refused before its first commit. Then it is not committed. | O:ipm-dev-downgrade-refused; T:Test.PM.Integration.History; P:g_refines | ipm/src/cls/IPM/Utils/Module.cls#L1213-L1217 | Seed table row 1. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B16 | HIST-0 | B | A failed no-transaction load that reached its writes is committed. | O:ipm-notxn-activate-fails; O:ipm-nojournal-activate-fails; T:Test.PM.Integration.History | ipm/src/cls/IPM/Utils/Module.cls#L1225-L1228 | Seed table row 3. An early refusal is not committed. |
| HIST-B17 | HIST-0 | B | A crashed load with no open transaction is committed if it reached its writes. | O:ipm-halt-notxn; O:ipm-terminate-notxn; O:ipm-terminate-dev; M:explore-L1-COMMIT-EXACT; O!:nc-gap-P1-docker-notxn-after; F:F-HIST-8 | ipm/src/cls/IPM/General/HistoryTemp.cls#L62-L72 | IRIS runs the log's close code on a crash (O:env-halt-open-transaction). Holds for a process crash only; an IRIS restart runs no %OnClose (F-HIST-8). |
| HIST-B18 | HIST-0 | B | Each dependency load writes its own record. | O:ipm-dep-ok; O:ipm-dev-dep-parent-fails | ipm/src/cls/IPM/Utils/Module.cls#L64-L70 | Written after the parent's record, so it has a larger ID. |
| HIST-B19 | HIST-B18 | B | A dependency record stays when the parent load fails. | O!:ipm-dep-parent-fails; F:F-HIST-1; O!:nc-gap-P9-chain-c-fails-txn; O!:nc-gap-P9-two-deps-parent-fails-txn; O!:nc-gap-P9-upgrade-parent-fails-txn | ipm/src/cls/IPM/Utils/Module.cls#L1358-L1361 | The record is lost whenever the parent's transaction rolls back (explore-L1-REC-DEP). Proved for any number of dependencies (g_dependency_records_lost_unless_fixed); with the fix every record is kept (g_dependency_records_kept_when_fixed), except across an IRIS restart (g_instance_crash_loses_pending). Intent: PR #578 (record recursively loaded modules; history saved outside the transaction), wiki 09. Its own result is HIST-B32. Intent: DEC-HIST-1 resolved (evidence), decisions/research/hist.md. |
| HIST-B32 | HIST-B19 | B | A dependency undone by its parent keeps its own result. Its record says it worked, but not committed. | F:F-HIST-1; O!:ipm-dep-parent-fails | ipm/src/cls/IPM/General/AbstractHistory.cls#L131 | Answers the sub-question of DEC-HIST-1: wiki 09 'Committed vs. Success'. Finalize sets Committed from the status inside the parent's transaction (#L131), so a fix must set it from the outcome after the rollback. Intent: DEC-HIST-1 resolved (evidence), decisions/research/hist.md. |
| HIST-B20 | HIST-B18 | B | A dependency record whose changes were kept is never lost. | O:ipm-dev-dep-parent-fails; O:ipm-dep-ok; P:g_refines; O:nc-gap-P9-two-deps-parent-fails-notxn; O:nc-gap-P9-two-deps-parent-fails-dev | models/verus/load_general.rs | The weak contract holds without premise PD. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| HIST-B21 | HIST-0 | B | The working copy is removed when the command ends, even after a crash. | O:ipm-load-ok; O:ipm-terminate-txn; O:ipm-halt-txn; M:explore-L1-NO-TEMP; O:nc-gap-P1-docker-txn-after | ipm/src/cls/IPM/General/HistoryTemp.cls#L69-L70 | historyTempLeft = 0 in all 25 IPM scenarios. |
| HIST-B22 | HIST-0 | B | A locked history record cannot be changed through SQL or objects. | O:ipm-finalized-history-row; O:nc-gap-P1-docker-txn-after | ipm/src/cls/IPM/General/History.cls#L15-L22 | SQL UPDATE gives SQLCODE -132; an object save fails; the row is unchanged. A row left by an IRIS restart is never locked and stays editable by SQL (nc-gap-P1-*, sqlUpdateSQLCODE 0). |
| HIST-B23 | HIST-0 | B | The history command lists records, newest first, with filters on any column. | T:Test.PM.Integration.History | ipm/src/cls/IPM/Main.cls#L4127-L4145, ipm/src/cls/IPM/General/History.cls#L93-L148 | `-D<col>=<value>` with `=`, `<`, `>`, `<=`, `>=`, `<>` or `*`; `-sort asc`; `-limit`. A column name with a symbol stops the filter list there (inspection). |
| HIST-B24 | HIST-B23 | B | With the globally flag, IPM shows the history of all namespaces. | T:Test.PM.Integration.History | ipm/src/cls/IPM/General/History.cls#L295-L317 | |
| HIST-B25 | HIST-0 | B | The history command can delete records. With no filter it needs -confirm. | T:Test.PM.Integration.History; O:ipm-finalized-history-row; O:nc-gap-P2-dev-confirm-after-release; O:ipm-history-delete-needs-confirm | ipm/src/cls/IPM/Main.cls#L4164-L4173 | Locked records can be deleted. With -globally, namespaces it may not write are skipped. Native: an unfiltered delete without -confirm is refused and deletes nothing. |
| HIST-B33 | HIST-B25 | B | If you delete the record of a command that still runs, the delete wins. The command then ends with no record. | O:nc-gap-P2-dev-confirm-after-release; O:nc-gap-P2-txn-by-package-after-release; O:r4-hist-delete-in-compile-txn-after-release; O:r4-hist-delete-in-compile-dev-after-release; O:r4-hist-delete-dep-row-notxn-after-release | ipm/src/cls/IPM/General/History.cls#L15-L22 | Deletes are allowed on any record (PR #578: only inserts and deletes; update trigger only). Holds for rows saved at $TLEVEL 0, during Activate or Compile, in transactional and developer mode, and for every row of a no-transaction load. A -DPackage= filter cannot match a running row (the name is only in IRISTEMP until close; F-HIST-5). F-HIST-10 is now intended behaviour. Intent: DEC-HIST-6 resolved (assumption), decisions/research/hist.md. |
| HIST-N5 | HIST-B33 | N | A record written inside an open transaction is locked. A delete of it fails until that transaction ends. | O:r4-hist-delete-dep-row-txn-after-release; O:r4-hist-delete-dep-row-dev-after-release | ipm/src/cls/IPM/Utils/Module.cls#L64-L68 | SQLCODE -110 after the 10 s lock timeout; applies to dependency rows, which are saved inside the parent's transaction. Nothing is deleted and the error is shown, which is acceptable. Intent: DEC-HIST-6 resolved (assumption), decisions/research/hist.md. |
| HIST-B26 | HIST-0 | B | IPM deletes records older than the retention. It checks once a day, when a command starts. | O!:ipm-history-autopurge; O!:ipm-history-purge-filter-cause; F:F-HIST-6 | ipm/src/cls/IPM/General/History.cls#L162-L181 | It deletes nothing: the date is passed with quotes. |
| HIST-B27 | HIST-B26 | B | The check runs before the new command writes its own record. | I | ipm/src/cls/IPM/Main.cls#L862 | So it cannot remove the running command's record. |
| HIST-B28 | HIST-0 | B | Before a command starts, there is no record for it to change. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L51-L63 | Used by cells for the not-started state. |
| HIST-B29 | HIST-0 | B | Later events never change a locked record, except a delete. | O:ipm-finalized-history-row; M:explore-L1-ENDED | ipm/src/cls/IPM/General/History.cls#L15-L22 | |
| HIST-B30 | HIST-B1 | B | Removing IPM itself writes its record before its own code is deleted. | I | ipm/src/cls/IPM/Storage/Module.cls#L630-L643 | Self-uninstall. |
| HIST-B31 | HIST-0 | B | For an uninstall, committed only shows that it worked. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L43 | Scope note from the property comment and the seed doc (index.md#L33); module 05 proves uninstall records (`unin_refines_rows_honest`). |
| HIST-N1 | HIST-B1 | N | If the caller rolls back, the record of the command is undone too. | O:ipm-load-inside-caller-transaction-rolled-back; P:g_caller_rollback_leaves_no_trace | ipm/src/cls/IPM/General/HistoryTemp.cls#L21 | Premise P1 names this case. The proof g_caller_rollback_leaves_no_trace has no premises (premises: none). Allowed: wiki 00 expects an outer wrapping transaction; the wiki sentence that history is copied outside the transaction needs a caveat (F-HIST-3, now documentation). Intent: DEC-HIST-4 resolved (assumption), decisions/research/hist.md. |
| HIST-N2 | HIST-B11 | N | If the operator commits at a crash prompt, a half-done load stays. Its record may say it was not committed. | O:ipm-halt-txn-operator-commits; I | validation/logs/32-halt-commit-b.log | Premise P2. A documented operator error; IPM does not guard against it. Witness g_operator_commit_dishonest shows the record then reads committed 0 (F-HIST-4, now documentation). Intent: DEC-LOAD-1 resolved (assumption), decisions/research/hist.md. |
| HIST-N3 | HIST-B13 | B | A crash right after the final commit still leaves the record marked committed. | O!:nc-gap-P8-commit-window-after-commit; F:F-HIST-7 | ipm/src/cls/IPM/Utils/Module.cls#L1348-L1350 | Premise P3; model only. Rewritten as intent; witness `g_window_crash_dishonest` in F-HIST-7. |

## States
| ID | State |
|---|---|
| H1 | The command has not started. |
| H2 | The command runs, and nothing it did is kept yet. |
| H3 | The command runs, some change is kept, and no transaction is open. |
| H4 | The command runs, some change is kept, and a developer-mode transaction is open. |
| H5 | A dependency load runs inside the parent's open transaction. |
| H6 | The command has ended. |

## Commands
| ID | Command |
|---|---|
| E1 | The command ends with success. |
| E2 | The command ends with a failure. |
| E3 | A crash; the open transaction rolls back. |
| E4 | A crash at a terminal; the operator commits. |
| E5 | The caller transaction rolls back. |
| E6 | `history delete` removes the record. |
| E7 | Auto-purge runs. |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| HIST-C1 | H1 | E1 | HIST-B28 | |
| HIST-C2 | H1 | E2 | HIST-B28 | |
| HIST-C3 | H1 | E3 | HIST-B28 | |
| HIST-C4 | H1 | E4 | HIST-B28 | |
| HIST-C5 | H1 | E5 | HIST-B28 | |
| HIST-C6 | H1 | E6 | HIST-B28 | |
| HIST-C7 | H1 | E7 | HIST-B28, HIST-B26 | |
| HIST-C8 | H2 | E1 | HIST-B8, HIST-B9 | |
| HIST-C9 | H2 | E2 | HIST-B3, HIST-B12, HIST-B15 | |
| HIST-C10 | H2 | E3 | HIST-B2, HIST-B10, HIST-B12 | |
| HIST-C11 | H2 | E4 | HIST-N2 |  |
| HIST-C12 | H2 | E5 | HIST-N1 |  |
| HIST-C13 | H2 | E6 | HIST-B33 |  |
| HIST-C14 | H2 | E7 | HIST-B27 | |
| HIST-C15 | H3 | E1 | HIST-B8, HIST-B9 | |
| HIST-C16 | H3 | E2 | HIST-B14, HIST-B16 | |
| HIST-C17 | H3 | E3 | HIST-B17 | |
| HIST-C18 | H3 | E4 | HIST-B17 | |
| HIST-C19 | H3 | E5 | HIST-N1 |  |
| HIST-C20 | H3 | E6 | HIST-B33 |  |
| HIST-C21 | H3 | E7 | HIST-B27 | |
| HIST-C22 | H4 | E1 | HIST-B8, HIST-B9 | |
| HIST-C23 | H4 | E2 | HIST-B14 | |
| HIST-C24 | H4 | E3 | HIST-B11 |  |
| HIST-C25 | H4 | E4 | HIST-B14 |  |
| HIST-C26 | H4 | E5 | HIST-N1 |  |
| HIST-C27 | H4 | E6 | HIST-B33 |  |
| HIST-C28 | H4 | E7 | HIST-B27 | |
| HIST-C29 | H5 | E1 | HIST-B18, HIST-B20 | |
| HIST-C30 | H5 | E2 | HIST-B19, HIST-B32 |  |
| HIST-C31 | H5 | E3 | HIST-B19, HIST-B32 |  |
| HIST-C32 | H5 | E4 | HIST-N2 |  |
| HIST-C33 | H5 | E5 | HIST-N1 |  |
| HIST-C34 | H5 | E6 | HIST-B33, HIST-N5 |  |
| HIST-C35 | H5 | E7 | HIST-B27 | |
| HIST-C36 | H6 | E1 | HIST-B29 | |
| HIST-C37 | H6 | E2 | HIST-B29 | |
| HIST-C38 | H6 | E3 | HIST-B29 | |
| HIST-C39 | H6 | E4 | HIST-B29 | |
| HIST-C40 | H6 | E5 | HIST-N1 |  |
| HIST-C41 | H6 | E6 | HIST-B25 | |
| HIST-C42 | H6 | E7 | HIST-B26 | |
