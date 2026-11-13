# Bindings: L2 model to IPM ObjectScript at 571e04a1

Paths are relative to the IPM repository root at `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. `model.rs` names are the spec functions in `src/model.rs`; each has an executable twin `exec_<name>` that `exec_spec_verified!` proves equal to it. The model is a transcription: no product body is verified (L3 is a source-bound transcription plus B2's native observations).

## Stores

| Model | Product | Notes |
|---|---|---|
| `ns.mods[i]` (`Mod`) | `%IPM.Storage.Module` record (`^IPM.Storage.ModuleD`): `present` = `NameExists`, `ver` = `VersionString`, `dev` = `DeveloperMode`; `doc` = `%IPM.StudioDocument.ModuleStream` (`.ZPM` document) version; `res` = the module's class resources loaded; `wr` = count of record writes | `wr` stands for LastUpdated/Hash changes so a rewrite counts as a change. Resources are one flag. |
| `ns.twins[i]` (`Twin`) | `%IPM.General.History` row (`^IPM.General.HistoryD`), `General/History.cls:4-22` | `id` = allocation order; `ph`/`phst` summarise `Phases` (count, last phase status); `edited` marks a foreign edit |
| `tx` | `$TLEVEL` and one snapshot per open level | see P-01, P-03 |
| `temp[i]` (`Temp`) | `%IPM.General.HistoryTemp` row in `^IRIS.Temp.HistoryTempD`, `General/HistoryTemp.cls:3-130` | P-02 |
| `ctr` | History ID counter (`$INCREMENT`) | P-04 |
| `jrn` | process journaling flag (`%IPM.Utils.JournalManager`, `Utils/JournalManager.cls:9-75`) | P-03 |
| `att` | none (auxiliary history variable) | 0 not started, 1 row inserted, 2 Finalize ran, 3 row completed after Finalize. Used only by the abstraction to L1 and by invariants. |
| slots 0/1/2 | the loaded module / its first and second dependency (and their logs) | at most two leaf dependencies |

## Steps

### Main:Load (`top_step`, `top_pc`)

| pc | Model action | Product | Guard / write / commit |
|---|---|---|---|
| 0 | `put_temp(0, fresh_temp(false))` | `Main.cls:2420` `HistoryTemp.LoadInit("")` → `AbstractHistory.cls:55-61` (`SetVersion` saves at 109, `%Save` at 61) | IRISTEMP insert, Package empty, placeholder version |
| 1 | `twin_insert(0)` | `HistoryTemp.cls:14-24` (twin `%Save` at 21, link save at 24) | History insert at the caller's `$TLEVEL`; `ctr` increments |
| 2 | path check; `lnm0 := dlnm()` | `Main.cls:2447-2452` (empty, relative), then `:2529` `LoadNewModule` | refusal → `top_err`, skip to Finalize |
| 3 | wait for `lnm0.pc == 32`; `top_err := lnm0.err || cleanup_fails` | `Main.cls:2529-2531` (`tmpRepoMgr.CleanUp()` status added at 2530, thrown at 2531) | `cleanup_fails` stands for OBS-L12 |
| 4 | `t_final(…, top_err)`; `att := 2` | `Main.cls:2427` / `:2430` → `AbstractHistory.cls:127-134` | `Committed := Committed || ISOK` (control 05 removes `|| ISOK`) |
| 5 | `persist(0)` | `%OnClose` `HistoryTemp.cls:62-67` → `PersistToTwin` `:30-59` | twin write at current `$TLEVEL`, `Finalized=1` (control 07 drops it); trigger refuses a finalized row (`History.cls:15-22`, control 10) |
| 6 | temp row deleted | `HistoryTemp.cls:70` `%DeleteId` | IRISTEMP delete |
| 7 | returned | `ShellInternal` returns | caller actions follow when `caller_tx` |

### LoadNewModule (`lnm_a`, `lnm_b`, `lnm_b2`, `lnm_phase`, `lnm_c`; `Lnm.pc`)

| pc | Model action | Product (`Utils/Module.cls`) | Guard / write / commit |
|---|---|---|---|
| 0 | `lnm_enter`: `itl := $TLEVEL`; `mf == 1` fails | 1162 `tInitTLevel`; 1164 `GetModuleObjectFromPath` (throws at 341-347, 359-362) | unparseable manifest fails before any transaction |
| 1 | `lnm_begin` | 1172 `tUseTransactions`; 1191-1193 JournalManager(0); 1194-1196 `tstart` | T1 or journaling off. `etx(c)` is the mode the steps use; it equals the requested mode except under control 13 (NoTransaction ignored) |
| 2 | `guard_refuses` | 1213-1223 (downgrade 1214-1217; update-required 1218-1222) | controls 08, 09 remove each guard |
| 3 | `set_com` when no transaction | 1225-1228 | IRISTEMP only (control 03 removes it) |
| 4 | `lnm_load` | 1251-1264 `$system.OBJ.Load` → `StudioDocument/Module.cls:149-202` `ImportStream` (nested `tstart` 184, `ModuleStream.%Save` 185, `UpdatePersistentFromStream` 190, `tcommit` 194, `trollback 1` 198-200) | `mf == 2`: nothing loaded, "No module definition found." (1255-1257); `mf == 3`: validation refusal, nested rollback (document kept only when journaling is off)  `mf == 4`: both documents imported, then "module.xml contains multiple documents." (1258-1260) |
| 5 | `t_names` | 1290-1295 `NameOpen`, `pLog.SetName`, `pLog.SetVersion` | IRISTEMP only |
| 6 | `lnm_devsave` | 1298-1307 | module save when `params("DeveloperMode")` is defined; otherwise stored mode (Q-01) |
| 7 | C1 `tcommit` when dev and transactions | 1309-1311 | durable only at `$TLEVEL` 1 → 0 |
| 8 | `set_com` | 1312-1314 | control 01 removes it |
| 9 | `tstart` (T2) | 1318 | |
| 10 | system requirements | 1321 → `Storage/Module.cls:337-354` | refusal throws inside T1 (non-dev) or T2 (dev) |
| 11 | `lnm_deps` | 1323-1328 → `LoadDependencies` 1368-1423 (BuildDependencyGraph 1380-1381; skip `ServerName=""` 1407-1409; `LoadModuleReference` 1411-1412, error thrown) | dependency not found → failure before any dependency log; installed at the requested version → skipped, no log |
| 12 | manifest reload | 1330-1333 | module record rewritten |
| 13 | C2 `tcommit` when dev and transactions | 1335-1339 | |
| 14-28 | phases Initialize, Reload, Validate, Compile, Activate: start (InitPhase), body, end (FinalizePhase) | 1346 → `Storage/Module.cls:447-720` (`InitPhase` 570-572; catch `FinalizePhase(error)` 692-697; Invoke error-status exits 608-610, 669-671 skip FinalizePhase; `FinalizePhase(OK)` 709-711) | Reload body sets `res` (`Lifecycle/Base.cls:615` `%Reload`, `ResourceProcessor/Default/Document.cls:350`/`378` loads); Activate `act == 1` → phase finalized with error (Configure-phase Invoke error through `Lifecycle/Module.cls:95-132`); `act == 2` → phase left open (Invoke with `Phase="Activate"` returning an error) |
| 29 | C3 `tcommit` when non-dev and transactions | 1347-1348 | durable at `$TLEVEL` 1 → 0 |
| 30 | `set_com` | 1349-1351 | control 02 removes it |
| 31 | `lnm_exit`: `trollback 1` while `$TLEVEL > tInitTLevel`; JournalManager restores the flag on return | 1355-1363 | control 04 removes the loop |

### LoadModuleReference for a dependency (`lmr_step`, `lmr_pc`)

| pc | Model action | Product (`Utils/Module.cls`) |
|---|---|---|
| 1 | `put_temp(slot, fresh_temp(true))` | 63-68 `LoadInit(pModuleName)` (Package set from Init) |
| 2 | `twin_insert(slot)` | `HistoryTemp.cls:14-24`, inside the parent's T1/T2 |
| 3 | `t_ver` | 69-70 `SetSource`, `SetVersion` (both IRISTEMP; merged into one step) |
| 4 | `lnm1 := dlnm()` | 128-140 filesystem repository → `LoadModuleFromDirectory` 293-330 (`DeveloperMode` override 309-310: snapshot under a `-dev` parent) → `LoadNewModule` 322 |
| 5 | wait for `lnm1.pc == 32` | |
| 6 | `t_final`; `att := 2` | 207-209 `log.Finalize(tSC)` |
| 7 | `persist(slot)` | `%OnClose` when `LoadModuleReference` returns, at the parent's `$TLEVEL` |
| 8 | temp row deleted | `HistoryTemp.cls:70` |

### Environment and process ends

| Model | Product / provider |
|---|---|
| `kill` | SIGKILL of the process: no `%OnClose`, rollback of every open level (P-05, P-06b) |
| `terminate` | `$SYSTEM.Process.Terminate` or HALT: `%OnClose` of the dependency log, the top JournalManager and the top log, then rollback (P-05, P-06b, P-06c) |
| `caller_end` | the caller of `Shell("load ...")` that already held a transaction commits or rolls back (P-11) |
| `user_edit` | another process's object save or SQL UPDATE of a History row, refused for finalized rows (P-08) |

## Repair variant 103 (not the as-built product)

D1 repair, used only by `repair103_*` lemmas, the `repair-103-*` explorer runs and the repair survey. Dependency History rows (slots 1, 2) are inserted and persisted by `autonomous_twin` (no rollback of the load undoes them, premise R-01). Mechanism named by the orchestrator (stage C R2-02): insert and update the dependency's History row with process journaling off, `%IPM.Utils.JournalManager(0)` scoped around the write, so neither the load's `TROLLBACK` nor the rollback at a process end undoes it (P-03, N-P07). R-01 (an unjournaled History-row insert inside an open transaction survives `TROLLBACK 1`, a full rollback and SIGKILL, without `<ROLLFAIL>`) is CONFIRMED natively by N-P20 (`results/native/20261113T214135Z-premise-p20-r01`), including through `%IPM.General.History` under `JournalManager(0)`. Caveat C2: B2 expected a journaled insert followed by an unjournaled update of the same node to be removed by the rollback; instead the node survived with the unjournaled value (single global node; index consistency for that mixed order not probed). `autonomous_twin` writes the row into every open snapshot (an unjournaled write).

Justification of the atomic `dep_commit_mark` (stage C R3-02). The implementable mechanism is: the dependency row is inserted unjournaled with Committed=0 (and completed unjournaled at the dependency's close, still Committed=0 while an IPM transaction of the load is open); Committed=1 is then set by a journaled update that is the row's last write before the commit that returns the load to its entry `$TLEVEL` (C2 in developer mode, C3 in normal mode), or at the dependency's close when no IPM transaction is open. Any rollback, including the one after a process end, undoes that journaled update back to the unjournaled value 0 (N-P20C). Never a journaled insert followed by unjournaled updates (N-P20C2). The model applies the mark as one step right after that commit. With no caller-held transaction the two forms agree: the commit returns to `$TLEVEL` 0, so before it a rollback restores 0 in both, and after it both read 1. Ordering matters: the journaled Committed=1 update must be the row's last write before that commit, because the effect of rolling back a journaled update on later unjournaled updates of the same stored row was not probed. Two points where the model is not that mechanism: (1) under a caller-held transaction the model's mark is unjournaled and survives the caller's rollback, while the journaled update would be undone to 0; the L1 Committed clause is unconstrained there (Q-05 provisional), so no repair-103 result depends on it; (2) `dep_finalize` at the top-level close writes unjournaled after the journaled mark; at `$TLEVEL` 0 (no caller transaction) no rollback can follow, but under a caller-held transaction this is the unprobed order. A repair that instead defers the dependency rows to the top-level close at `$TLEVEL` 0 meets the clauses at a normal end but not at a process end: at SIGKILL no `%OnClose` runs, and at Terminate the close persists inside the open transaction, which IRIS then rolls back (B1-C45K/T, native C3). a dependency's row takes Committed=1 only when its writes are durable (at its close if no IPM transaction of the load is open, otherwise when the parent's C2/C3 commit reaches the load's entry level, `dep_commit_mark`); a parent rollback leaves it at 0; the rows are finalized at the top-level close (`dep_finalize`). The top-level row, all guards and all other writes are as built.

## Abstractions and what they could hide or invent

- Early refusals of `LoadModuleReference` (repository unavailable, required-version mismatch, Utils/Module.cls:84-126) are not modelled. They happen after the dependency's log Init (inside the parent's transaction), so they would produce a dependency failure whose row is lost like D1; the model reaches that outcome only through the dependency's own failures.
- An error from `WriteAfterInstallMessage` after C3 (Utils/Module.cls:1354) is not modelled. It would behave like the CleanUp error (`cleanup_fails`, V6): an error status after the commit.
- In the fault domain the manifest, not-found and system-requirements faults are applied to the first dependency only; the second dependency only gets the Activate fault. Nothing in the code treats the second dependency differently, but its combinations are not explored.
- Process death is one step (death and rollback together). After SIGKILL, IRIS rolls back only at slot cleanup (105 s to about 10 min, N-P18), and other processes can read the uncommitted writes meanwhile (premise P-13). The model observes only after the rollback.
- Per-module `<Defaults>` transaction flags are not representable (`Cfg.txm` applies to every activation), so the model makes no prediction for them. Native evidence (B2 `20261113T204405Z-stagec2`): M5, a normal-mode parent whose dependency's manifest has `<Defaults><Parameter Name="NoJournal">1</Parameter></Defaults>` and whose Activate fails, leaves the dependency installed with its class and `.ZPM` document, with no dependency History row, no `<ROLLFAIL>`, `$TLEVEL` 0 and journaling restored; M5N, the same without `<Defaults>`, rolls everything back and loses only the dependency row (D1). Recorded as D14; whether a module's own `<Defaults>` may override the parent's rollback is open question Q-22 (`transaction.flag-sources.dependency-defaults`), so D14 is decision pending. A dependency's `<Defaults>` NoTransaction only skips its own `tstart`; its writes stay journaled inside the parent's transaction (R3-03).

- Combined steps. `SetSource` + `SetVersion` (IRISTEMP only) are one step; `ImportStream` is one atomic step (a process end inside it equals one before it when journaling is on); phase bodies other than Reload and Activate write nothing. These merge only IRISTEMP or invisible writes; a native observation at a finer point (for example a process end inside ImportStream under NoJournal) is not represented.
- Snapshot stack instead of an undo log. Equivalent to IRIS rollback when no key written in an open transaction is also written with journaling off or by another process in that transaction. Unjournaled writes are pushed into every snapshot; foreign edits are restricted to rows the load has not modified in its open transaction (`user_edit_enabled`).
- Not modelled: archive and git sources, ModuleRoot copies, `.modules` temporary repository definitions (durable at `$TLEVEL` 0 for the top module; for a dependency, inside the parent's transaction), lock file, pip, CPF, `LoadedResource` (IRISTEMP), analytics, read-only repository forcing, `install`-only guards (developer-mode overwrite, Q-03), `update` (MarkResourceForUpdate, ApplyUpdateSteps), module `<Defaults>` that set NoTransaction for one module, legacy XML installs, the requirement guard in `LoadModuleReference`, more than two dependencies or nested dependencies. Effects outside the database (Q-04) are invisible to the model.
- Dependency resolution is reduced to: skipped when installed at the requested version, otherwise loaded in order; "not found" fails before any dependency log. Version conflicts and phase-scoped dependencies are not modelled.
- Versions are ordinals 1 < 2 < 3; semantic-version subtleties (snapshots, prereleases) are reduced to the `snap` flag.
- Phases are summarised by count and last status, which is enough because phases run in order and stop at the first failure.
- Initial states (`cfg_wf`): the loaded module absent, or installed at 1, 2 (dev or not) or 3; dependencies absent or installed at exactly the requested version and non-dev. Outside the domain (R3-08): a dependency already installed at another version or in developer mode, so a parent load that upgrades an installed dependency, and the dependency-level downgrade and UpdatePackage guards (`guard_refuses` on the dependency frame, where `-dev` does not reach a non-snapshot dependency); version conflicts between modules.
