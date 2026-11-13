# Stage C round 1 review: model and evidence (load and history)

Independent reviewer for the domain "model and evidence". Product `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. `VV` = `.g5/Verus_verification`. I wrote none of the reviewed artifacts. I changed no source. My only writes are this file and five receipt directories under `VV/results/verus/*-review-r1-*` (section 6). I did not append to `VV/results/index.jsonl` and did not touch IRIS or Docker.

Order of work followed the brief: code, then the governing ontology pages, then the model, bindings, premises and receipts, then the native evidence, then the B reports and `C/round-1/dispositions.md`.

## 1. Pre-read expectations (written from the code before reading the artifacts)

`T0` is `$TLEVEL` when `LoadNewModule` is entered (`Utils/Module.cls:1162`). For a top-level `load` from the shell, `T0 = 0`. For a dependency, `T0` is the parent's level (1 inside the parent's T1 or T2).

Commit boundaries and guards in `LoadNewModule` (`Utils/Module.cls:1152-1364`):

```
enter (T0)
 ├─ GetModuleObjectFromPath (1164, throws on missing/unparseable file) ── fail, nothing open
 ├─ NoJournal: JournalManager(0) (1191-1193)  │ else transactions on: tstart → T0+1 (1194-1196)
 ├─ downgrade guard (1213-1217), update-required guard (1218-1222) ── throw → rollback to T0
 ├─ no transaction: SetCommitted (1226-1228)  [before any product write]
 ├─ OBJ.Load module.xml (1251) → ImportStream nested tstart/tcommit|trollback 1 (StudioDocument/Module.cls:184-200)
 ├─ late manifest checks (1254-1264), update -path name check (1269-1275), read-only repo (1277-1288)
 ├─ NameOpen, pLog.SetName/SetVersion (1290-1295)   [IRISTEMP only]
 ├─ DeveloperMode param defined: save Deployed=0, DeveloperMode, Root (1298-1304)
 ├─ dev && tx: C1 tcommit → T0, SetCommitted, tstart → T0+1 (1309-1319)   [durable only if T0 = 0]
 ├─ CheckSystemRequirements (1321) ── throw
 ├─ LoadDependencies (1328) → LoadModuleReference per dep (1411), fresh log, thrown error
 ├─ reload module.xml, %Reload (1330-1333)
 ├─ dev && tx: C2 tcommit → T0 (1335-1339)
 ├─ ExecutePhases(Activate|ApplyUpdateSteps, pLog) (1346): InitPhase, FinalizePhase(err) in catch, Invoke returned errors skip FinalizePhase (Storage/Module.cls:603-610, 662-671)
 ├─ OK && !dev && tx: C3 tcommit → T0, SetCommitted (1347-1351)
 └─ catch → tSC; while $TLEVEL > T0: trollback 1 (1358-1362, outside the try)
```

History writes and their levels:
- `Main:Load` creates `HistoryTemp.LoadInit("")` before `LoadInternal` (`Main.cls:2419-2420`). `AbstractHistory.Init` saves the IRISTEMP row, then `HistoryTemp.Init` inserts the durable twin at the caller's `$TLEVEL` (`HistoryTemp.cls:14-24`).
- `SetSource`, `SetVersion`, `SetName`, `SetCommitted`, `InitPhase`, `FinalizePhase`, `Finalize` save only the IRISTEMP row (`^IRIS.Temp.HistoryTempD`, not rolled back).
- The twin is rewritten only by `PersistToTwin` from `%OnClose` (`HistoryTemp.cls:30-73`), at the `$TLEVEL` current when the last reference drops, and the IRISTEMP row is then deleted.
- A dependency's log is created in `LoadModuleReference` (`Utils/Module.cls:63-70`) at the parent's level, inside the parent's T1 or T2, finalized at `:207-209` and closed when `LoadModuleReference` returns, still inside the parent's transaction.
- `Main:Load` finalizes after `LoadInternal` returned or threw (`:2427`, `:2430`). `Finalize` sets `Committed := Committed || ISOK(status)`.

Outcome-changing cases I expected to matter:
1. Normal mode success, failure in a phase (rolled back, Committed 0), refusal by each guard.
2. Developer mode: failure before C1 (rolled back), between C1 and C2 (manifest kept, dependencies rolled back), after C2 (phases outside any transaction).
3. NoTransaction and NoJournal: SetCommitted before any write, so a refusal after that point reads Committed 1 even if nothing was kept. NoJournal writes survive rollback.
4. A dependency's record is inserted and persisted inside the parent's transaction, so the parent's rollback should erase it although its IRISTEMP row is already deleted.
5. A caller already holding a transaction: every IPM commit is nested, so Committed=1 is premature and the caller's rollback erases the record.
6. Process death: SIGKILL runs no `%OnClose`, so the twin keeps its Init values, including Committed 0 after a developer-mode commit. Terminate or HALT runs `%OnClose` before IRIS rolls back, so the persisted copy is undone in normal mode.
7. A dependency whose own `<Defaults>` sets NoTransaction or NoJournal runs a different mode inside its parent's transaction.
8. Version and system-requirement checks run after the manifest is written, so their timing decides what a refusal keeps.
9. Errors after the final commit (`tmpRepoMgr.CleanUp()`, `WriteAfterInstallMessage`) report failure for a committed load.
10. A returned (not thrown) Invoke error leaves a phase without `FinalizePhase`.

Cases 1-6, 8 and 10 turned out to be modelled. Case 9 is modelled for CleanUp only. Case 7 is not modelled (R1-MOD-05). I had not anticipated that the CleanUp failure is an `<INVALID OREF>` crash (native B1-C29).

## 2. Material findings

### R1-MOD-01: The L1 contract leaves a dependency's `Committed` unconstrained, so it cannot reject a D1 repair that marks rolled-back dependencies committed
- Kind: model error
- Source or owner: `history.committed` (I, revised by R1-ONT-01): "A successful action is committed, unless a parent load's rollback later undoes it. A dependency undone that way reads uncommitted." `history.committed.nested` (Q) now covers caller-held transactions only. Disposition R1-ONT-01 assigned B1 the L1 update. Owner: `VV/crates/ipm-load-history/src/contract.rs`.
- Trigger: B1-C31 (normal mode, parent Activate fails after its dependency loaded) run against a variant that writes the dependency's History row outside the parent transaction, which is the shape of the D1 repair. Reviewer variant 203 (`VV/results/verus/20261113T202330Z-review-r1-mutants`): the dependency row reads Success OK, Committed=1 while its module was rolled back. L1 with the provisional Q reports no violation. Only the Q-05 alternative reports `H-COMMITTED[dep1]`.
- Discrepancy: `dep_rec` uses `com = if q.q05_all { 3 } else { 2 }` (`contract.rs:201`), and `kill_rec` does the same for dependency slots (`:228-229`). `q05_all` is still documented with its pre-round-1 meaning, "exact for every action, not only top-level actions started at $TLEVEL 0" (`:26-27`), and its provisional value false makes dependency `Committed` "any" even with no caller transaction. Every `dep1`/`dep2` record expectation in `cases/cases.json` reads `"committed": "any"` (for example B1-C32, C34, C38, C39). As built, D1 hides the gap because the row is missing. Once D1 is repaired, the contract no longer checks the `Committed` half of R1-ONT-01. The D1 repair proposal in `VV/bugs_issues.md` ("persists each nested log with `Committed` set from the parent's outcome") would also be wrong for developer mode. In A5 and B1-C38 the parent fails after C2 while the dependency stays, so the dependency must read Committed=1 although the parent failed. Its distinguishing checks omit A5.
- Limits: I did not check `install` dependencies (`InstallInit`) or ORAS sources. Variant 203's Terminate path is approximate: it still persists inside the transaction there.
- Proposed repair: B1: in `dep_rec` and `kill_rec` use `com = 3` (equals changes kept) whenever `!c.caller_tx`, and keep `q05_all` for caller-held transactions only. Rename and redocument the parameter, regenerate `cases.json` and the survey, and report flipped verdicts. Add a D1 repair variant (rows outside the transaction, `Committed` from whether the enclosing transaction committed) and show it passes L1 on A1, A3, A5/C38 and C31. Orchestrator and B2: restate D1's repair as "Committed set from whether the dependency's writes were kept", and add A5/C38 to its distinguishing checks.

### R1-MOD-02: The L1 contract allows a manifest refusal under NoTransaction or NoJournal to keep module data, and B1-C10 passes although the product keeps the `.ZPM` document
- Kind: model error
- Source or owner: `load.refusal` (I): "A refusal happens before IPM writes any module data, so in every mode the installed module's data stays as it was". It covers `load.refusal.manifest`, and the only open exception is the requirements timing. `history.fields` with R1-ONT-03: "A refusal after IPM read the manifest records the name." Owner: `contract.rs` `l1_alts`, `top_rec`; `model.rs` `lnm_load`.
- Trigger: B1-C10, `load <m> -DNoJournal=1` where the module document fails validation (a resource already owned by another module). Native replay `20261113T193952Z-replay-return-b`: status ERR, `modules.top.zpm_document_version=2.0.0` (before: none), `history.top.committed=1`, `history.top.package_set=0`. Verdict `l1:PERMITTED`. B1-C09 (same with `-DNoTransaction=1`) also shows `package_set=0`.
- Discrepancy: `l1_alts` reaches `else if !tx { any_alt }` (`contract.rs:154`) for verdict 2 (manifest refused), so any namespace state is permitted. `B/model-notes.md` section 2 lists path, downgrade and update-required refusals as "no module data in any mode" and silently drops manifest refusals. `top_rec` gives verdict 2 `pkg = any` (`:189`), although for a validation refusal (`mf == 3`) `GetModuleObjectFromPath` has already correlated the `<Module>` element (`Utils/Module.cls:1164`), so the name is known. The model also has no input for the late refusal that does write the manifest ("multiple documents", `Utils/Module.cls:1258-1260`), which the ontology lists as a suspected gap under `load.refusal.manifest`.
- Limits: the "multiple documents" path and an `update -path` mismatch were not run natively (B2 phase 2 queue, R1-ONT-04).
- Proposed repair: B1: verdict 2 requires `unchanged_alt()` in every mode, and `pkg = 1` when the manifest was read (`mf == 3`). Add an `mf` value for "manifest written, then refused" and regenerate cases. B1-C10 then reports `L1-NAMESPACE` and B1-C09/C10 report the Package clause. Orchestrator: record the C10 behavior as a defect (or a Q, if the `.ZPM` document is not "module data"), and extend D9's distinguishing check to the validation refusal.

### R1-MOD-03: The "unended record lists the phases it started" rule is in neither the formal contract nor the native expectations, and its confirmed violation is filed as a Q deviation
- Kind: model error
- Source or owner: `history.record` (I): "While the action runs, and after it stops without ending, the log lists its record with Success 0, no end time and the phases it has started." The round 1 ontology review (`review-ontology.md`, R1-ONT-02 proposed repair) says to keep the phase rule. Owners: `contract.rs` `kill_rec`; B2 native expectations; `VV/bugs_issues.md`.
- Trigger: a load that dies inside Activate. Native C1 (Terminate, normal mode; `20261113T184230Z-replay-fast`), C3 (SIGKILL; `20261113T182927Z-cases-c-sigkill`) and C4R (`20261113T183827Z-cases-c4r`) all observe `hist.row1.Phases=` empty after four phases ended and Activate started. B1-C44K/T, C46K and C48K predict `phases_started 0`.
- Discrepancy: `kill_rec` sets `ph = 255` and `phst = 255` ("any") for every unended or completed-by-close row (`contract.rs:227-229`), so L1 accepts a row with no phases. The native expectations for C1 and C3 do not claim `Phases`. Both verdicts read CONFORMS while the observed field contradicts the I clause (`unexpected_observations: {'hist.row1.Phases': ''}`). `VV/bugs_issues.md` lists "no phases" inside its Q-08 rows under "Deviations under open questions", but the governing clause is I, not Q-08. The ontology metadata itself calls it a "Confirmed gap against the phase rule".
- Limits: I did not decide whether `Package=""` on such rows also violates `history.fields` ("fields not yet known may hold a placeholder" is arguable). I did not observe a running action from another process.
- Proposed repair: B1: in `kill_rec`, require `ph` to equal the phases the action started (available from the IRISTEMP row at the kill point) and `phst = 0` for the open phase. B2: add `Phases` expectations to the process-end cases (C1, C3, C4R and the B1 T/K replays) and re-evaluate. Orchestrator: add a product defect (for example D10) against `history.record`, with a repair such as writing phase entries to the durable row at each phase boundary at `$TLEVEL 0`, and C1/C3/C4R as distinguishing checks.

### R1-MOD-04: Two model-level violations are natively confirmed but missing from the defect list, and the model notes still call them pending
- Kind: missing or stale evidence
- Source or owner: V4 against `history.committed` (I: "A refusal before anything is written is not committed, in any mode"). V6 against `load.outcome` (I: normal mode failure keeps no module data). Owners: `VV/bugs_issues.md`, `B/model-notes.md` section 4, `VV/premises.md`.
- Trigger: native replay `20261113T193952Z-replay-return-b`:
  - B1-C08 (`-DNoTransaction=1`, manifest without a module document): "No module definition found.", nothing written, `history.top.committed=1`. Verdict l2 AGREES, l1 VIOLATES.
  - B1-C09 (`-DNoTransaction=1`, validation refusal): nothing kept, Committed=1, AGREES/VIOLATES.
  - B1-C29 (normal mode, broken `.modules` cache): `ERROR #5002: <INVALID OREF>LoadInternal+83^%IPM.Main.1`, module installed with classes, `history.top.committed=1`. AGREES/VIOLATES.
- Discrepancy: `model-notes.md` V4 and V6 say "B2 replay pending", yet the same file's section 6 reports the 43/43 agreement that includes these cases. `bugs_issues.md` has no entry for V4 or V6 (only OBS-L12 under "suspected"), so two confirmed I-clause violations have no repair proposal. V2 (Terminate variants, confirmed by `20261113T194122Z-replay-terminate-b`) is not in D1's evidence. `premises.md` still marks P-09 PARTIAL "not tested natively", while B1-C09 and B1-C10 now confirm both branches (journaled ImportStream rollback undone, NoJournal document kept).
- Limits: K variants (`20261113T194155Z-replay-sigkill-b`) were still running during this review (at B1-C48K at 20:21Z), with no verdict.
- Proposed repair: Orchestrator: add defects for V4 (repair: mark committed only after the first write is kept, for example after `OBJ.Load` returns OK, preserving A0 and the B3/B4 behavior) and V6 (repair: create the `.modules` repository before the transaction and make `CleanUp` tolerate a failed `%OnNew`, or treat cleanup errors as warnings after the commit, preserving the normal-mode rollback). If an error after the final commit is not a "failed load", record that as a Q under `load.outcome`. B1: update V2/V4/V6 native columns. B1/B2: set P-09 to CONFIRMED with the two run IDs.

### R1-MOD-05: Per-module `<Defaults>` transaction flags are not representable, although a dependency running with NoJournal inside its parent's transaction breaks the normal-mode rollback
- Kind: model error
- Source or owner: `transaction.flag-sources` (I: the flags can come from "the module's own `<Defaults>`"; maintainer doc `Utils/Module.cls:1033-1040` shows `<Parameter Name="NoTransaction">1</Parameter>`). `load.outcome` (I: normal mode "The whole load, including dependencies loaded for it, is rolled back"). Owner: `model.rs` (`Cfg.txm` is one value for every activation), `BINDINGS.md`.
- Trigger: `load parent` in normal mode, where dependency `d`'s `module.xml` has `<Defaults><Parameter Name="NoJournal">1</Parameter></Defaults>` and the parent's Activate then fails. `LoadNewModule` for `d` merges `d`'s defaults under the parent's params (`:1166-1169`). The parent's params do not define `NoJournal`, so `d` creates `JournalManager(0)` (`:1191-1193`) while the parent's T1 is open. `d`'s writes are unjournaled, and N-P07 shows such writes are not undone by `TROLLBACK`. The parent's rollback loop (`:1358-1362`, outside the try) then leaves `d` installed. IRIS documentation cited in P-03 says such a rollback may raise `<ROLLFAIL>`, which would escape `LoadNewModule`. `d`'s own history row is journaled (created before and persisted after its JournalManager), so it is rolled back: an installed module with no record. With `NoTransaction` in `d`'s defaults, the namespace is rolled back correctly, but `d`'s log is marked Committed at `:1227`, which matters once D1 is repaired.
- Discrepancy: BINDINGS and `model-notes.md` section 8 list "module `<Defaults>` that set NoTransaction for one module" as not modelled, without a reason, an open obligation or a native case. The model's `all_mod` path for unjournaled writes covers only the global `txm == 2`.
- Limits: reasoned from code and N-P07, not run. `<ROLLFAIL>` was not observed in N-P07.
- Proposed repair: B2: add a native case (normal-mode parent, dependency with `<Defaults>` NoJournal=1, parent Activate fails, read both modules, the dependency's classes, both records and `$TLEVEL` after the shell returns). B1: either add a per-package transaction mode for dependencies, or record the case as an open obligation with the native result. Orchestrator: record a suspected defect against `load.outcome`.

## 3. Minor findings

- R1-MOD-06 (control 11b). It is not a meaningful control. With no caller transaction, the top row is inserted at `$TLEVEL 0`, and variant 11 can only make more writes durable, so R3-top cannot fail by construction. Its receipt and `index.jsonl` line still say `"expect": "violation"`, `"outcome": "pass"`, which reads as an undetected control unless one also reads `model-notes.md`. Reproduced: `b1 explore control-11b-death-keeps-transaction-r1` gives pass on 900 configurations and 91,155 states. Repair: drop it or relabel it as a non-control with expect "pass". Control 11 with L1-NS-ALL already detects the mutation.
- R1-MOD-07 (control 05 scope). The survival explanation is sound in the modelled domain. ISOK-REDUNDANT passes in all three invariant runs (reproduced for the default mode), and control 05 reproduces as pass. "Equivalent on the load path" overreaches: a legacy XML dependency (`Utils/Module.cls:172-185`, OBS-L16) reaches `Finalize` with no `SetCommitted`, so `|| ISOK` is its only source of Committed=1. Repair: scope the claim to filesystem and archive dependencies, and name the legacy path and uninstall as the callers that rely on it.
- R1-MOD-08 (control domain). `model-notes.md` section 5 says L1 controls use configurations where the as-built "satisfies the same clause". `conforming_domain` (`explore.rs:246`) filters on the whole check (all L1 clauses at the normal end), which keeps 1,617 of 5,400 configurations and drops every version refusal (D9) and every D1 configuration. My variant 201 (no-transaction SetCommitted moved before the version guards) changes 308 configurations. The control method sees it in only 60, all of them NoTransaction or NoJournal configurations that combine a manifest fault (`mf` 2 or 3) with an installed newer version (`rev2.log` in the mutants receipt). Repair: per-clause conforming domains, as already done for L1-NS-ALL.
- R1-MOD-09 (receipt currency). The cited final as-built proof `20261113T194721Z-asbuilt-proofs-l1rev1` predates edits to `src/explore.rs` and `src/bin/b1.rs`. The proof files match, and my reproduction on the current inputs gives 248 verified, 0 errors. Repair: cite `20261113T201030Z-review-r1-asbuilt-proofs` or rerun.
- R1-MOD-10 (replay L1 baseline). Replay verdicts are evaluated against `cases.json` `fc7adec4`, the pre-revision L1 (31 cases changed `l1_permitted`). L2 agreement carries over because `l2_predicted` is byte-identical. The native L1 verdicts do not carry over. Repair: rerun `evaluate_replay.py --cases VV/cases/cases.json` (no IRIS needed).
- R1-MOD-11 (phase 1 expectation provenance). C3 (CONFORMS) and C4R (DEVIATES) were evaluated against an `expected-cases.json` modified after their runs started (`written_before_run: false`, no snapshot), and they were not in `replay-fast`. `native-report.md` section 8 discloses this and also says first evaluation outputs were overwritten. The observations are sound and match the raw logs. Repair: let the K-variant replay supersede them, and keep original verdict files under a new name instead of overwriting.
- R1-MOD-12 (defect classes). D6 is filed as "Confirmed" but its obligation is Q-14 (V5 says "under Q-14"), so it should read "confirmed, decision pending". D8 is filed as "Confirmed" (a governing source requires otherwise), while `native-report.md` and the ontology say "No governing source beyond usability". It should be an issue, not a confirmed defect.
- R1-MOD-13 (stale B2 prose). `native-report.md` section 6 still calls C4R "consistent with the Q-08 provisional rule that readers must not trust Committed", and section 4.3 counts a placeholder-version mismatch against C2. R1-ONT-02 changed both, and the ontology metadata and `bugs_issues.md` already follow the change.
- R1-MOD-14 (unrecorded sanity run). `model-notes.md` section 3 says "A deliberately false variant was checked to fail". No receipt or index line records it.
- R1-MOD-15 (abstractions to list). Several abstractions have the same shape as modelled faults but are not listed in BINDINGS: `LoadModuleReference`'s early refusals (repository unavailable, required-version mismatch, `:84-126`), and an error from `WriteAfterInstallMessage` after C3 (`:1354`). In the fault domain, manifest, not-found and requirements faults apply only to the first dependency. Process death is modelled as death and rollback in one step, while N-P18 shows 105 s to about 10 minutes during which other processes read the uncommitted writes.

## 4. Checked and found correct

L1 fidelity against the ontology and round 1:
- Q defaults match the current bodies: Q-01 keep stored mode (`dev-mode.sticky`), Q-02 `-dev` bypass (`load.refusal.update-required.dev-bypass`), Q-06b caller-held survival (`history.committed.nested`), Q-08 "reflects kept changes" (`history.record.abnormal-end`, R1-ONT-02), Q-14 (`history.phases.returned-errors`), Q-17 (`load.refusal.requirements.timing`), Q-FIN no (`history.locked.unfinished`). The one mismatch is Q-05 (R1-MOD-01).
- R1-ONT-01 existence half: `dep_rec` requires `ex = 1` for every attempted dependency (`contract.rs:200`), and `kill_rec` requires it outside caller transactions. R1-ONT-02: `q08_exact` and `qfin_unended` implemented, and flipped verdicts reported. R1-ONT-03 for verdicts 3-8 (`top_rec` `pkg = 1`, `:188`).
- Dependency outcome under a `-dev` parent (manifest kept, dependencies all-or-nothing, `contract.rs:158-159`), developer-mode early failure (`:152`, `:157`), snapshot dependency in dev mode (`:136`), and `Success` of a dependency equal to its own status (Q-06 narrowed).

L2 binding, checked line by line against the code and transaction level:

| Product step (file:line) | Model | Level | Result |
|---|---|---|---|
| `Main.cls:2419-2420` LoadInit, `HistoryTemp.cls:14-24` twin insert | `top_pc` 0-1, `twin_insert` | caller's | correct |
| `Utils/Module.cls:1164` unparseable manifest | `lnm_enter` `mf == 1` | before any tstart | correct |
| `:1191-1196` JournalManager or tstart | `lnm_begin` | T0 → T0+1 | correct for the global mode (R1-MOD-05) |
| `:1213-1223` guards, `params("DeveloperMode")` for update-required | `guard_refuses`, `devval` | T0+1 | correct |
| `:1226-1228` SetCommitted before writes | `lnm_a` pc 3 | IRISTEMP | correct, precedes `OBJ.Load` |
| `:1251` + `StudioDocument/Module.cls:184-200` ImportStream nested rollback | `lnm_load` `mf == 3` | +1 nested | correct, confirmed by B1-C09/C10 |
| `:1290-1295` SetName/SetVersion | pc 5 | IRISTEMP | correct, after guards (D9) |
| `:1298-1307` dev save | `lnm_devsave` | T0+1 | correct (Deployed=0 not modelled, invisible to L1) |
| `:1309-1319` C1 tcommit, SetCommitted, tstart | pc 7-9 | T0+1 → T0 → T0+1 | correct |
| `:1321` CheckSystemRequirements | pc 10 | T0+1 after C1 | correct (D4) |
| `:1328`, `:1404-1413` LoadDependencies, skip installed, thrown error | `lnm_deps` | parent's level | correct |
| `Utils/Module.cls:63-70`, `:207-209` dependency log, Finalize, `%OnClose` on return | `lmr_step` 1-8 | inside parent's T1/T2 | correct (D1) |
| `:1330-1339` reload, C2 | pc 12-13 | → T0 | correct |
| `Storage/Module.cls:570-711` InitPhase, thrown versus returned errors | `lnm_phase`, `act` 1/2 | per mode | correct (D6) |
| `:1347-1351` C3, SetCommitted | pc 29-30 | → T0 | correct |
| `:1358-1362` rollback loop | `lnm_exit` | → T0 | correct |
| `Main.cls:2427/2430` Finalize, then `%OnClose` | `top_pc` 4-6 | T0 | correct (N-P09 order) |
| `Main.cls:2530-2531` CleanUp error | `cleanup_fails` | after commit | correct, native `<INVALID OREF>` |

Proof strength:
- No `assume`, `admit`, `external_body` or external code in the crate (grep).
- Theorems quantify over every `cfg_wf` configuration symbolically, and `theorem_reachable_inv` bridges to every trace.
- COMMIT-SOUND, TOP-RECORD-COMPLETE and R3-top are scoped to a top-level load with no caller transaction, and COMMIT-SOUND and TOP-RECORD-COMPLETE also to a returned command. `model-notes.md` states these scopes, and the excluded regions are exactly the reported violations (V8, V9, V10) or open Qs. The theorems are non-vacuous: `case_c01_conforms` exhibits a reachable returned state with kept changes and a committed, finalized row.
- Case lemmas use `ACTIVE_VARIANT()` (0 as built). Explorer runs are exhaustive within the single-fault grid (`truncated=false` in every final receipt).

Faults tried (section 6, reviewer harness over the executable twins, 5,400 configurations, every reachable state):
- Faulty implementation 201, SetCommitted moved before the version guards: detected (COMMIT-EXACT, H-COMMITTED[top], 308 configurations).
- Faulty implementation 204, Finalize ignoring its status: detected widely (H-RECORD-FIELDS[top], H-COMMITTED, 1,377 configurations even on the authors' control domain). TOP-RECORD-COMPLETE would also fail.
- Erroneously permitted step 202, a dependency row created at the parent's entry level and persisted inside: detected (H-RECORD-FIELDS replaces H-RECORD-EXISTS in 828 configurations).
- Variant 205, SetCommitted moved one step before C3: no new violation. It removes 80 Terminate-window violations, so it is equivalent or better for History readers, consistent with P-07.
- False premise 206, an inner TCOMMIT durable: the model then predicts that B1-C42's row and module survive the caller's rollback. Native D1 and B1-C42 observe neither, so the native evidence refutes the premise (N-P03). The formal checks alone would not.
- Valid completion: `case_c01_conforms` verifies in my reproduction, and the as-built conforming configurations are unchanged under 205.

Controls: 01-04, 06-10 and 11 detect their mutations, and the receipts' state counts reproduce. Control 02 Verus failures reproduce exactly (`cases.rs:154`, `proofs.rs:317`).

Receipt currency: the explorer receipts `20261113T1949xxZ-*` and Verus control receipts `20261113T194935Z`-`195508Z` match the current input hashes. `cases/cases.json` and the `20261113T194804Z` survey regenerate byte-identical with the current binary.

Native evidence:
- Cases drive `##class(%IPM.Main).Shell` and read back `%IPM_General.History`, `HistoryTemp`, the module record and the class dictionary by SQL (`native/scripts/B2Native/Cases.cls:1-22`, `:503-615`).
- Both evaluators make a case INCOMPLETE on a missing field, a missing marker or a non-COMPLETED process. `evaluate_replay.py` also guards absence checks with `x.ids_allocated` and `fault_check`, and marks approximate realizations.
- The runner snapshots expectations into `inputs/` from 18:42Z on.
- Raw logs matched `verdict.json` field by field for A1 and A0 (`182759Z-cases-a`), C3 and C4 (`182927Z-cases-c-sigkill`), C4R (`183827Z`), D1 and C1 (`184230Z-replay-fast`), and B1-C31, C08, C10 and C29 (`193952Z-replay-return-b`). Each log ends with `NATIVE-END`.
- No PASS rests on a zero-test run: DependencyResolution was corrected to NO-EFFECTIVE-TESTS, with both index lines kept. C4 was kept INCOMPLETE.

Defect classification (my assessment):

| ID | Classification in the artifacts | Assessment |
|---|---|---|
| D1 / V1 | product defect | correct. Repair's Committed rule needs the dev-mode fix (R1-MOD-01) |
| V2 | model violation, T confirmed | product defect, same root as D1. Add to D1's evidence |
| D2, D3, D5 | product defects | correct. Readers and settings are native-only, not in the model |
| D4 / V3 | confirmed, decision pending Q-17 | correct |
| D6 / V5 | confirmed (D6), under Q-14 (V5) | decision pending Q-14 (R1-MOD-12) |
| D8 | confirmed | issue without a governing source (R1-MOD-12) |
| D9 / V7 | product defect (necessary implication) | correct. Extend to validation refusals (R1-MOD-02) |
| V4 | model violation, "pending" | confirmed product defect, missing from the list (R1-MOD-04) |
| V6 | model violation, "pending" | confirmed, product defect or a Q on post-commit errors (R1-MOD-04) |
| V8 | deviation under Q-06b | correct |
| V9 | deviation under Q-08 | correct for Committed. Its "no phases" part is an I-clause defect (R1-MOD-03) |
| V10 | deviation under Q-FIN | correct |

## 5. Answers to the brief's questions

- L1 fidelity: mostly faithful, with two weakened clauses (dependency Committed, manifest refusals in no-transaction modes) and one missing I clause (phases on unended records): R1-MOD-01, -02, -03. R1-ONT-02 is fully implemented. R1-ONT-01 and R1-ONT-03 are partly implemented.
- L2 fidelity: 18 steps checked (table in section 4), with no binding error found. One outcome-changing abstraction is unrecorded as an obligation (R1-MOD-05). Nested and more than two dependencies change no mechanism I could find (D1 extends to grandchildren). Install differs only in the action label and top-level guards, which are out of scope.
- Proof strength: sound and non-vacuous within stated scopes. One receipt citation is stale (R1-MOD-09).
- Controls: 05's explanation is sound in scope but overstated (R1-MOD-07). 11b is not a control (R1-MOD-06).
- Native: honest about INCOMPLETE runs and provenance. Gaps are R1-MOD-10 and -11 and the unclaimed phase field (R1-MOD-03).
- Replay status at review time: returning cases and T variants are evaluated. K variants were still running (at B1-C48K, 20:21Z). C49T/K are "not realized" or approximate, as stated.

## 6. Reproduction log

All Verus runs used the pinned binary `~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/cargo-verus`, offline, `--num-threads 4`, one at a time, from `VV/crates` with `CARGO_TARGET_DIR=VV/crates/target`, `RUSTUP_TOOLCHAIN=1.98.1-x86_64-unknown-linux-gnu`.

| Receipt (`VV/results/verus/`) | Command | Result | Matches |
|---|---|---|---|
| `20261113T201030Z-review-r1-asbuilt-proofs` | `cargo-verus verify -p ipm-load-history --offline --lib -- --num-threads 4` | verified 248, errors 0, 35 s. `inputs.sha256` equals the current files | `194721Z-asbuilt-proofs-l1rev1` (248/0) |
| `20261113T201344Z-review-r1-control-02` | same, with `--lib --features control-02` | INVALID: cargo-verus usage error (features after `--lib`), no verification. Kept as an unsuccessful run | n/a |
| `20261113T201401Z-review-r1-control-02b` | `... --offline --features control-02 --lib -- --num-threads 4` | 2 errors: `cases.rs:154` (case_c29), `proofs.rs:317` (lemma_lnm0_d pc 30), exit 101 | `195008Z-control-02-verus-l1rev1`, identical |
| `20261113T202330Z-review-r1-explorer-repro` | `target/release/b1 explore` on `control-05-finalize-no-isok`, `asbuilt-invariants-default`, `asbuilt-r3-dep`, `control-10-no-trigger`, `control-11b-…`; `b1 cases`; `b1 survey` | pass 2700/266,337; pass 1800/183,382; violation 900/34,558 (15-step trace); violation 900/20,710; pass 900/91,155. `cases.json` sha256 `99300800…14557` and `survey.json` `85f7eea4…e2d` regenerate byte-identical | the 1949xxZ receipts |
| `20261113T202330Z-review-r1-mutants` | scratch copy at `/tmp/review-r1-model/vv` with `model.diff` (variants 201-206) and `rev.rs`, `cargo build --release --offline -j 4 --bin rev`, run 28 s | section 4 "Faults tried" | new |

No IRIS session was started. Native conclusions come from the existing raw logs and verdicts.

## 7. Limits

- I reviewed the load and history path only. History readers (filters, delete, purge, `-globally`) were checked only through B2's evidence, because they are not modelled.
- My mutant harness is unverified Rust over the exec twins of a modified copy. Variant 203's Terminate path persists inside the transaction, so its Terminate results are approximate. The harness omits foreign edits.
- R1-MOD-05 is reasoned from code and premise N-P07, not run. `<ROLLFAIL>` is from the IRIS documentation cited in P-03.
- I did not read the ontology pages outside `load`, `history` and `transactions-and-dev-mode`, the A-stage inventories, or the wiki and issues behind the cited sources. I relied on the ontology's own citations.
- `20261113T194155Z-replay-sigkill-b` and the stage-C additions (S1-S4, H2, X1, H10) were incomplete or queued during this review, so the K-variant claims (V2-K, V9 via B1 cases) remain open.
