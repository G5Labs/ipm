# B2 phase 2: native replay of B1 model cases, premise mapping and stage-C additions

Status: COMPLETE (21:03Z) except where section 8 says otherwise. `VV` = `.g5/Verus_verification`. Runs are under `VV/results/native/`, and every run and re-evaluation has a line in `VV/results/index.jsonl`.

## 1. Inputs and hashes

| Input | Pin |
|---|---|
| `VV/cases/cases.json` replayed | `fc7adec492c7a55f64ecddb44798359c652fbc21beef9973e01d531f123ec8f0` (57 cases, B1 run `results/verus/20261113T193019Z-cases-asbuilt`). Snapshot copy `VV/native/cases/b1-cases-fc7adec4.json` in each run's `inputs/` |
| intermediate re-emission (19:47Z) | `993008003ef5cda84ef42e503619b314af60a4f61cf764b159e1be1f67714557`. Superseded before use, never evaluated against |
| final `cases.json` (20:43Z) | `058429752d6508a3e4527beb35e5825badf3132c8688b3f6138aa1d9bc457961` (59 cases). Relative to `fc7adec4`: `l2_predicted` and `inputs` are byte-identical for the 57 common cases, `l1_permitted` changed in 39, and B1-C51 and B1-C52 are new. Copy: `VV/native/cases/b1-cases-05842975.json` |
| Binding | `VV/native/cases/replay-binding.json`, from `VV/native/make_replay.py`, snapshotted per run |
| Driver and evaluator | `VV/native/scripts/B2Native/Replay.cls`; `VV/native/evaluate_replay.py` (`python3 -I`; `--cases` re-evaluates against a newer `cases.json` and writes `verdict-cases-<sha8>.json` beside the original `verdict.json`); `VV/native/replay_table.py` |
| B1 premises | `VV/premises.md` (appeared 19:51Z, updated since) |

Because only `l1_permitted` changed and two cases were added, the 57 common cases were re-evaluated against `05842975` without re-running IRIS (R1-MOD-10), and only B1-C51 and B1-C52 were replayed natively.

## 2. Binding

Each case drives `##class(%IPM.Main).Shell("load …")`. T/K variants run the shell in a JOB. After the command the driver reads:
- the returned `%Status`;
- `$TLEVEL` (for "caller rolls back", after that rollback);
- per slot (top, dep1, dep2): the module record (`%IPM_Storage.ModuleItem`), the class `<Pkg>.Main` (`%Dictionary.CompiledClass`), the stored `.ZPM` document version (`%IPM.StudioDocument.ModuleStream`) and `LastUpdated`;
- `%IPM_General.History` rows after a baseline ID (`id_order` = ID minus the counter before the command);
- `%IPM_General.HistoryTemp` rows after a baseline temp ID.

Values are normalized to 1/0, OK/ERR/ZERO/OPEN and `<none>`.

| Model input | Native realization |
|---|---|
| module m, d1, d2; versions 1/2/3 | per-case fixtures `r<NN><t/k><attempt>-m`, `-d1`, `-d2`, versions 1.0.0/2.0.0/3.0.0, one class each (attempt label `b`, `c` for C49) |
| dependency found / not found | in m's `.modules/` / omitted, with the remote `registry` repository disabled for that case only (C35) |
| snapshot dependency | version `1.0.0+snapshot`, compared with the model's 1.0.0 after dropping `+snapshot` |
| installed_before | setup `load` or `load -dev` before the baseline |
| update_package | `<UpdatePackage><Pkg>.Upd</UpdatePackage>` with VersionBase classes |
| system requirements unmet | `<SystemRequirements SYSNamespace="true"/>` |
| manifest not parseable / no module document / refused by validation / two documents | unclosed element / `<Export>` without `<Document>` / resource owned by a setup module `r<NN>b-own` / `.ZPM` plus a class document |
| Activate throws / Activate-phase Invoke returns error | Invoke returning an error with CheckStatus (Configure phase, raised by `%Activate`) / `Phase="Activate" When="After"` |
| error after the last commit (C29) | `.modules/broken/module.xml` naming a module with a space; the filesystem cache rejects it, `TempLocalRepoManager:%OnNew` fails, `CleanUp()` hits `<INVALID OREF>` |
| caller transaction | `tstart` before `Shell`; `trollback` after it returns where the case says so |
| process end inside an Activate | Invoke `Wait()` stalls on an IRISTEMP flag; T: `$SYSTEM.Process.Terminate`; K: `kill -9`, then wait until IRIS removes the slot (563-598 s in this run) |
| process end after the final commit (C49) | approximation, section 5 |

Not compared:
- `writes` (76 field instances): not observable natively.
- `end`: a binding field set by the harness.
- For T/K variants, `status` (no return) and `open_transaction_levels` (0 once the process is gone) are inferred, not observed.

Harness preconditions, each making a case INCOMPLETE when it fails:
- setup commands succeeded;
- for T/K, the stall was signalled and the process disappeared;
- `x.ids_allocated >= 1`, so the command reached IPM. This guard was added after the first evaluation (orchestrator request), so absence checks cannot pass vacuously.

## 3. Results

Fully realized cases (57 of 59): l2 AGREES 57/57. That is 1161 product fields agreeing and 0 disagreeing, plus 76 not comparable and 57 binding fields. Against the final `cases.json`, l1 PERMITTED 23 and VIOLATES 34. The native l1 judgement matches B1's `l1_vs_l2` in every case, under both the replayed and the final `cases.json`.

Counts written as "a / b / c" are PERMITTED / VIOLATES / INCOMPLETE.

| Run | Cases | l2 | l1 vs `fc7adec4` | l1 vs `05842975` |
|---|---|---|---|---|
| `20261113T193745Z-replay-return` (attempt 1, superseded) | C01-C43 | 40 AGREES, 2 DISAGREES (binding), 1 INCOMPLETE | 30 / 12 / 1 | 21 / 21 / 1 |
| `20261113T193952Z-replay-return-b` | C01-C43 | 43 AGREES | 32 PERMITTED, 11 VIOLATES | 23 PERMITTED, 20 VIOLATES |
| `20261113T194122Z-replay-terminate-b` | C44T-C48T, C50T | 6 AGREES | 4 / 2 | 0 / 6 |
| `20261113T194155Z-replay-sigkill-b` | C44K-C48K, C50K | 6 AGREES | 4 / 2 | 0 / 6 |
| `20261113T205137Z-replay-c51-c52` (runs on `05842975`) | C51, C52 | 2 AGREES | n/a | 0 / 2 |
| `20261113T204430Z-replay-approx-b`, `…-replay-approx-c` | C49T, C49K | APPROXIMATE, section 5 | | |

Attempt 1 had three binding errors, all fixed in attempt b, and is kept as recorded:
- `open_transaction_levels` was sampled before the caller's rollback (C42, C43).
- The C29 fixture (an unparseable `.modules` manifest, which `BuildCache` skips) did not produce the fault.
- `LastUpdated` was not observed, so "changes kept" could not see same-value rewrites (C19, C20).

Mutation controls (all detected) are listed in each `verdict.json`:
- return-b: C31 dep row, C01 committed, C42 tlevel, C23 manifest, C29 status, C42 `x.ids_allocated=0`.
- terminate-b: C44T finalized, C46T committed, C47T temp row.
- sigkill-b: C44K temp row, C46K committed, C50K row.
- c51-c52: C52 manifest.

### Per-case table

l1 is shown against both `cases.json` revisions. "B1 l1_vs_l2" is B1's own judgement in the final file.

| Case | Title | l2 (model vs product) | l1 vs replayed cases.json | l1 vs final cases.json | B1 l1_vs_l2 (final) | Native l1 matches B1 | Run |
|---|---|---|---|---|---|---|---|
| B1-C01 | Clean load succeeds | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C02 | Clean -dev load succeeds | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C03 | Clean NoTransaction load succeeds | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C04 | Clean NoJournal load succeeds | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C05 | Relative path refused | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C06 | Unparseable manifest refused (default) | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C07 | Unparseable manifest refused (NoTransaction) | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C08 | Manifest without a module document, NoTransaction: Committed=1 with nothing kept | AGREES | VIOLATES (history.top.committed) | VIOLATES (history.top.committed) | violates H-COMMITTED[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C09 | Manifest refused by ImportStream validation, NoTransaction: Committed=1 with nothing kept | AGREES | VIOLATES (history.top.committed) | VIOLATES (history.top.committed, history.top.package_set) | violates H-RECORD-FIELDS[top], H-COMMITTED[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C10 | Manifest refused by ImportStream validation, NoJournal: document kept, Committed=1 | AGREES | PERMITTED | VIOLATES (modules_one_of, history.top.package_set) | violates L1-NAMESPACE, H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C11 | Downgrade refused (default) | AGREES | PERMITTED | VIOLATES (history.top.package_set) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C12 | Downgrade refused (-dev) | AGREES | PERMITTED | VIOLATES (history.top.package_set) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C13 | Downgrade refused (NoTransaction) | AGREES | PERMITTED | VIOLATES (history.top.package_set) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C14 | Downgrade refused even with -force | AGREES | PERMITTED | VIOLATES (history.top.package_set) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C15 | Upgrade with UpdatePackage refused | AGREES | PERMITTED | VIOLATES (history.top.package_set) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C16 | Upgrade with UpdatePackage allowed with -force | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C17 | Upgrade with UpdatePackage allowed with -dev (Q-02) | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C18 | Upgrade with UpdatePackage refused (NoTransaction) | AGREES | PERMITTED | VIOLATES (history.top.package_set) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C19 | Same version reload allowed | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C20 | Plain load of a module stored in developer mode keeps it (Q-01) | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C21 | -nodev load of a developer-mode module turns it off | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C22 | System requirements refused (default) | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C23 | System requirements refused (-dev): manifest kept (Q-17) | AGREES | VIOLATES (modules_one_of) | VIOLATES (modules_one_of) | violates L1-NAMESPACE | yes | `20261113T193952Z-replay-return-b` |
| B1-C24 | System requirements refused (NoTransaction): manifest kept (Q-17) | AGREES | VIOLATES (modules_one_of) | VIOLATES (modules_one_of) | violates L1-NAMESPACE | yes | `20261113T193952Z-replay-return-b` |
| B1-C25 | Activate fails (default): rolled back | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C26 | Activate fails (-dev): manifest and resources kept | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C27 | Activate fails (NoTransaction): partial kept | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C28 | Activate-phase Invoke returns an error: phase left open (Q-14) | AGREES | VIOLATES (history.top.last_phase_status) | VIOLATES (history.top.last_phase_status) | violates H-RECORD-FIELDS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C29 | Error after the load committed (CleanUp): installed but reported failed | AGREES | VIOLATES (modules_one_of) | VIOLATES (modules_one_of) | violates L1-NAMESPACE | yes | `20261113T193952Z-replay-return-b` |
| B1-C30 | Load with a dependency succeeds | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C31 | Parent Activate fails after the dependency loaded (default): dependency record lost | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.dep1.row) | violates H-RECORD-EXISTS[dep1] | yes | `20261113T193952Z-replay-return-b` |
| B1-C32 | Dependency Activate fails (default): its failure record lost | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.dep1.row) | violates H-RECORD-EXISTS[dep1] | yes | `20261113T193952Z-replay-return-b` |
| B1-C33 | Dependency Activate fails (-dev parent): parent manifest kept, dependency record lost | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.dep1.row) | violates H-RECORD-EXISTS[dep1] | yes | `20261113T193952Z-replay-return-b` |
| B1-C34 | Dependency Activate fails (NoTransaction): dependency record kept | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C35 | Dependency not found: no dependency record, parent fails | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C36 | Second dependency fails (default): both dependency records lost | AGREES | VIOLATES (history.dep1.row, history.dep2.row) | VIOLATES (history.dep1.row, history.dep2.row) | violates H-RECORD-EXISTS[dep1], H-RECORD-EXISTS[dep2] | yes | `20261113T193952Z-replay-return-b` |
| B1-C37 | Second dependency fails (-dev): both dependency records lost, parent manifest kept | AGREES | VIOLATES (history.dep1.row, history.dep2.row) | VIOLATES (history.dep1.row, history.dep2.row) | violates H-RECORD-EXISTS[dep1], H-RECORD-EXISTS[dep2] | yes | `20261113T193952Z-replay-return-b` |
| B1-C38 | Parent Activate fails (-dev) after dependencies committed: dependency record kept | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C39 | Snapshot dependency loaded in developer mode under a -dev parent | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C40 | Dependency already installed: skipped, no record | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C41 | Caller holds a transaction, load succeeds: Committed=1 while nothing is durable | AGREES | PERMITTED | PERMITTED | conforms | yes | `20261113T193952Z-replay-return-b` |
| B1-C42 | Caller holds a transaction, then rolls back: module and history row gone | AGREES | PERMITTED | VIOLATES (history.top.row) | violates H-RECORD-EXISTS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C43 | Caller transaction, -dev load fails at Activate: Committed=1, then caller rolls back | AGREES | PERMITTED | VIOLATES (history.top.row) | violates H-RECORD-EXISTS[top] | yes | `20261113T193952Z-replay-return-b` |
| B1-C44K | Process killed inside Activate (default) | AGREES | PERMITTED | VIOLATES (history.top.package_set, history.top.phases_started, history.top.last_phase_status) | violates H-RECORD-FIELDS[top] | yes | `20261113T194155Z-replay-sigkill-b` |
| B1-C44T | Process killed inside Activate (default) | AGREES | PERMITTED | VIOLATES (history.top.package_set, history.top.phases_started, history.top.last_phase_status) | violates H-RECORD-FIELDS[top] | yes | `20261113T194122Z-replay-terminate-b` |
| B1-C45K | Process killed inside parent Activate after a dependency loaded (default): dependency record lost | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.top.package_set, history.top.phases_started, history.top.last_phase_status, history.dep1.row) | violates H-RECORD-FIELDS[top], H-RECORD-EXISTS[dep1] | yes | `20261113T194155Z-replay-sigkill-b` |
| B1-C45T | Process killed inside parent Activate after a dependency loaded (default): dependency record lost | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.top.package_set, history.top.phases_started, history.top.last_phase_status, history.dep1.row) | violates H-RECORD-FIELDS[top], H-RECORD-EXISTS[dep1] | yes | `20261113T194122Z-replay-terminate-b` |
| B1-C46K | Process killed inside Activate (-dev, with dependency): changes kept, row reads Committed=0 | AGREES | PERMITTED | VIOLATES (history.top.committed, history.top.package_set, history.top.phases_started, history.top.last_phase_status) | violates H-RECORD-FIELDS[top], H-COMMITTED[top] | yes | `20261113T194155Z-replay-sigkill-b` |
| B1-C46T | Process killed inside Activate (-dev, with dependency): changes kept, row reads Committed=0 | AGREES | PERMITTED | VIOLATES (history.top.finalized) | violates H-RECORD-FIELDS[top] | yes | `20261113T194122Z-replay-terminate-b` |
| B1-C47K | Process killed inside the dependency's Activate (default) | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.top.package_set, history.dep1.row) | violates H-RECORD-FIELDS[top], H-RECORD-EXISTS[dep1] | yes | `20261113T194155Z-replay-sigkill-b` |
| B1-C47T | Process killed inside the dependency's Activate (default) | AGREES | VIOLATES (history.dep1.row) | VIOLATES (history.top.package_set, history.dep1.row) | violates H-RECORD-FIELDS[top], H-RECORD-EXISTS[dep1] | yes | `20261113T194122Z-replay-terminate-b` |
| B1-C48K | Process killed inside Activate (NoTransaction): partial kept, row reads Committed=0 | AGREES | PERMITTED | VIOLATES (history.top.committed, history.top.package_set, history.top.phases_started, history.top.last_phase_status) | violates H-RECORD-FIELDS[top], H-COMMITTED[top] | yes | `20261113T194155Z-replay-sigkill-b` |
| B1-C48T | Process killed inside Activate (NoTransaction): partial kept, row reads Committed=0 | AGREES | PERMITTED | VIOLATES (history.top.finalized) | violates H-RECORD-FIELDS[top] | yes | `20261113T194122Z-replay-terminate-b` |
| B1-C49K | Process killed after the final commit, before Finalize (default) | APPROXIMATE (agrees; attempt b disagreed by binding) | n/a | APPROXIMATE (violates: history.top.committed, package_set, phases_started, last_phase_status) | | | `20261113T205218Z-replay-approx-c` |
| B1-C49T | Process killed after the final commit, before Finalize (default) | APPROXIMATE (agrees; attempt b disagreed by binding) | n/a | APPROXIMATE (violates: history.top.committed, history.top.finalized) | | | `20261113T205218Z-replay-approx-c` |
| B1-C50K | Process killed inside Activate with a caller transaction: everything rolled back, row lost | AGREES | PERMITTED | VIOLATES (history.top.row) | violates H-RECORD-EXISTS[top] | yes | `20261113T194155Z-replay-sigkill-b` |
| B1-C50T | Process killed inside Activate with a caller transaction: everything rolled back, row lost | AGREES | PERMITTED | VIOLATES (history.top.row) | violates H-RECORD-EXISTS[top] | yes | `20261113T194122Z-replay-terminate-b` |
| B1-C51 | Manifest with two documents refused after import (default): rolled back | AGREES | VIOLATES (history.top.package_set) | same file | violates H-RECORD-FIELDS[top] | yes | `20261113T205137Z-replay-c51-c52` |
| B1-C52 | Manifest with two documents refused after import (NoTransaction): module record kept | AGREES | VIOLATES (modules_one_of, history.top.package_set) | same file | violates L1-NAMESPACE, H-RECORD-FIELDS[top] | yes | `20261113T205137Z-replay-c51-c52` |

## 4. Disagreements between l2_predicted and the product

None among the 57 fully realized cases.

The only l2 disagreement in the replay is binding-caused (C49T/K attempt b, section 5). Two attempt-1 disagreements (C42, C43 `open_transaction_levels` predicted 0, observed 1) were also binding errors. The driver sampled `$TLEVEL` before the caller's `trollback`, while the model means the end state. Attempt b observes 0.

## 5. Not realized or approximate: B1-C49T and B1-C49K

The model's fault point is a process end after the final commit (C3) and before `Finalize`. No product hook runs user code there. The approximation sets a ZBREAK (action N with execute code) in the JOB, which stalls the process at a chosen line of `LoadNewModule`; the harness then ends the process. A process end at those lines is possible in production, but the injection is a debugger facility. These cases are therefore reported as APPROXIMATE and never as agreeing.

| Attempt | Stall line | Observed | Comparison with l2 |
|---|---|---|---|
| b (`20261113T204430Z-replay-approx-b`) | `do tModule.WriteAfterInstallMessage()` (`LoadNewModule+176`), after the C3 block | in flight the temp row already had Committed=1 (the C3 `SetCommitted` had run). T: durable row finalized by `%OnClose` with package, version, 5 phases (last OK), Success 0, Committed 1, no end. K: twin keeps Init values, orphan temp row with Committed 1. Module installed | DISAGREE on `history.top.committed` (T) and `history_temp.top.committed` (K), predicted 0. Judgement: binding error. The model's AfterC3 point lies between the C3 `tcommit` and its `SetCommitted`, and attempt b stalled after both |
| c (`20261113T205218Z-replay-approx-c`) | the C3 block's `do pLog.SetCommitted()` (`LoadNewModule+172`, previous line `tcommit`) | in flight the temp row had Committed=0 (tcommit done, mark not yet made). T: `%OnClose` finalized the twin with package, version 2.0.0, 5 phases (last OK), Success 0, **Committed 0**, no end. K: twin keeps Init values (Committed 0, no package, no phases), orphan temp row Committed 0. In both, the module is fully installed and committed | all compared fields agree (APPROXIMATE (agrees)); l1 against the final `cases.json` violates Committed (changes kept but 0) and, for T, Finalized; for K also package and phases (D10). Mutation `history.top.committed=1` detected. Product reading, conditional on the approximation: a process end in the window between the C3 `tcommit` and its `SetCommitted` (Utils/Module.cls:1347-1351) leaves an installed module whose record reads Committed=0, and under Terminate that record is finalized, so it cannot be corrected |

## 6. Premise mapping (B2 N-P to B1 P-nn)

| B1 premise | B2 evidence | Status after phase 2 |
|---|---|---|
| P-01 nested transactions | N-P01, N-P02, N-P03 (`20261113T181129Z-premises`) | CONFIRMED |
| P-02 IRISTEMP outside rollback, instance-wide | N-P04, N-P06, N-P12 (IRISTEMP part), N-P17 | CONFIRMED |
| P-03 NoJournal sets not undone by TROLLBACK | N-P07; also N-P19 (unjournaled module and dependency writes survive the caller's rollback) and M5 (section 7) | CONFIRMED |
| P-04 `$INCREMENT` not undone | N-P05; ID gaps in A1/A3/A2 and B1-C31/C32/C36 (`x.ids_allocated` > rows) | CONFIRMED |
| P-05 open transaction rolled back at process end | N-P12 (Terminate), N-P18 (SIGKILL, after slot cleanup); K replay waits of 563-598 s | CONFIRMED (HALT: documentation only) |
| P-06a `%OnClose` at release and during unwinding | N-P08, N-P09 | CONFIRMED |
| P-06b Terminate/HALT run `%OnClose` before the rollback; SIGKILL runs none | N-P10, N-P11, N-P18; phase-1 C1 (twin update undone, temp row deleted) | CONFIRMED |
| P-06c close order innermost first at a terminate-like end | N-P19 (`20261113T204146Z-premise-p19`): NoJournal load with a dependency under a caller transaction, Terminate inside the dependency's Activate. The dependency's row survives finalized (persisted before the top JournalManager re-enabled journaling); the top row is rolled back. Mutation `history.dep1.finalized=0` detected | CONFIRMED (was UNVERIFIED) |
| P-07 deep save of the swizzled twin; missing twin not recreated | N-P14, N-P15 | CONFIRMED |
| P-08 Finalized trigger; second persist refused | N-P13, N-P16 | CONFIRMED |
| P-09 ImportStream's nested rollback undoes the `.ZPM` save on validation refusal unless NoJournal | B1-C09 (NoTransaction: no document, no record) and B1-C10 (NoJournal: document kept) in `20261113T193952Z-replay-return-b`, both l2 AGREES on `zpm_document_version` | CONFIRMED (was PARTIAL) |
| P-10 classes loaded inside the transaction are undone by its rollback | `resources_loaded` = 0 after rollback in B1-C25, C31, C32, C36, C44K/T (l2 AGREES); phase-1 A1, C1 | CONFIRMED for class definitions and compiled classes; non-database effects remain outside (Q-04) |
| P-11 caller transaction absorbs IPM's commits | B1-C41, C42, C43, C50T, C50K (l2 AGREES); phase-1 D0, D1 | CONFIRMED |
| P-12 IRISTEMP emptied at restart | not run: needs an instance restart of the shared container, and nothing in the model depends on it | UNVERIFIED |

## 7. Stage-C round 1 additions (R1-ONT-04), R1-MOD-05 and extras

Expectations were written before each run: `VV/native/cases/expected-stagec.json`, snapshotted in each run's `inputs/`. The evaluator is `VV/native/evaluate.py`. Runs: `20261113T204152Z-stagec` (S1-S4, H2, X1, H10) and `20261113T204405Z-stagec2` (S4R, M5, M5N). Fixtures are in `VV/native/fixtures/stagec*/` (`make_stagec.py`, `make_stagec2.py`).

| Case | Command | Expected (source) | Observed | Verdict |
|---|---|---|---|---|
| S1N | `load …/s1-tgz-1.0.0.tgz` | loads (INT-LOAD-01, -03) | loads | CONFORMS |
| S1 (a) | `load …/s1-targz-1.0.0.tar.gz` | loads (help `Main.cls:280-282` example, CHANGELOG #474) | refused: "File not found to obtain module object: module.xml"; nothing installed | DEVIATES, as A2 OBS-L2 predicted |
| S2N | two-document manifest, default mode | refused "multiple documents", nothing kept | as expected | CONFORMS |
| S2 (b) | same, `-DNoTransaction=1` | refused, no module data kept, Committed=0 (`load.refusal.manifest`, wiki/09 L114-133) | refused; module record, `.ZPM` document 1.0.0 and the extra class kept; Committed=1 | DEVIATES. B1-C52 replays the same case and the model predicts it (l2 AGREES) |
| S2B (b) | class-only manifest, `-DNoTransaction=1` | refused "No module definition found", nothing kept | refused; the class kept; Committed=1 | DEVIATES |
| S3N2 | `update A -path A-2.0.0.tgz -DNoTransaction=1` | A updated to 2.0.0 | as expected | CONFORMS |
| S3N1 | `update A -path B-1.0.0.tgz` (default) | refused with the mismatch message, nothing kept | refused, but with `<INVALID OREF>LoadNewModule+67`; nothing kept in the database; B extracted to `<ModuleRoot>/s3-b2/1.0.0` | CONFORMS on the checked fields (the message was not checked in the neighbour) |
| S3 (c) | same, `-DNoTransaction=1` | refused with the mismatch message (`Utils/Module.cls:1269-1275`), nothing kept, Committed=0 | `<INVALID OREF>LoadNewModule+67^%IPM.Utils.Module.1`; B's manifest not loaded; Committed=1; B extracted to `<ModuleRoot>/s3-b1/1.0.0` | DEVIATES on message and Committed. OBS-L6's prediction ("B's manifest stays") is refuted, because the load crashes earlier: `moduleCurrent` is opened by B's name (`:1207`) and dereferenced at `:1232` while B is not installed. The mismatch guard is unreachable for an uninstalled B |
| S4 (d), first binding | `load s4-rel/1.0.0` after `$SYSTEM.Process.CurrentDirectory(…/s4/)` | refused (CHANGELOG #706) | refused "must be absolute". `%File` did not see the path as existing in that process | INCOMPLETE (binding: the path was not existing for `%File`; re-evaluated with the `rel.exists` guard) |
| S4R (d) | `load ../../../../home/irisowner/zpm/.native/fixtures/stagec/s4/s4-rel/1.0.0` (relative to `/usr/irissys/mgr/user/`, which `%File` uses; `rel.exists=1`) | refused (CHANGELOG #706, `load.refusal.path`) | accepted and loaded, Committed=1 | DEVIATES, as the ontology's code reading predicted |
| S4N1, S4N2 | relative nonexistent / absolute | refused "must be absolute" / loads | as expected | CONFORMS |
| M5N | normal parent fails in Activate; its dependency has no `<Defaults>` | everything rolled back (`load.outcome`) | rolled back; dependency record lost (D1) | DEVIATES on `d.hist.rows` only (D1) |
| M5 (R1-MOD-05) | same, dependency's manifest has `<Defaults><Parameter Name="NoJournal">1</Parameter></Defaults>` | whole load, dependency included, rolled back (`load.outcome`); `$TLEVEL` 0 | parent rolled back; **dependency stays installed** with its class and `.ZPM` document; no dependency history row (its row was journaled and rolled back); no `<ROLLFAIL>`; `$TLEVEL` 0; journaling flag restored | DEVIATES (suspected defect confirmed) |
| H2 (e) | `default-modifiers -set -DNoTransaction=1 -DUpdateSnapshots=1` (the product's help example), then `history find -limit 5`, `history delete -confirm` | 5 rows shown; delete removes all (INT-HIST-10, -12) | 0 rows shown with OK status; delete fails with SQLCODE -29 ("Field 'NOTRANSACTION' not found"); defaults stored under both top-level and `"zpm"` subscripts | DEVIATES (A2 OBS-H2 confirmed). Defaults and history restored |
| X1 (e) | defaults set in USER, read in USER-VERIFY | not visible ("current namespace", `Main.cls` command description) | visible (`NoTransaction=1`); `^%IPM.DefaultModifiers` maps to `^/usr/irissys/mgr/` (IRISSYS) from both namespaces | DEVIATES (A2 OBS-X1 confirmed) |
| H10 | a row inserted in USER-VERIFY, read from USER | `history find -globally` lists it; the GlobalRecords query returns it (INT-HIST-11) | `find -globally` lists it under "Namespace: USER-VERIFY". `SELECT * FROM %IPM_General.History_GlobalRecords(0,0)` returns 458 rows, which is twice USER's 229, and none of USER-VERIFY's 173 | DEVIATES (A2 OBS-H10 confirmed: the documented global query reads the current namespace once per namespace) |

Mutation controls: S1, S1N, S2, H2, X1 detected. S3 `b.exists` and S4 `load.ok` were not detected, because each mutated value equals the observed one, so they are not counted as controls. M5 `d.exists`, M5N `d.exists`, S4R `load.ok` and N-P19 `dep1.finalized` were detected.

Product findings from this section, for the orchestrator to number:
1. `.tar.gz` archives refused by `load` (S1).
2. Late manifest checks under NoTransaction keep the manifest, document and extra documents and log Committed=1 (S2, S2B, B1-C52).
3. `update -path` with another module's archive crashes with `<INVALID OREF>` instead of the documented mismatch refusal, logs Committed=1 under NoTransaction, and leaves the extracted archive (S3, S3N1).
4. An existing relative path is accepted (S4R).
5. Namespace default modifiers act as `history` filters (H2).
6. "Namespace" default modifiers are instance-wide (X1).
7. GlobalRecords returns wrong rows (H10).
8. A dependency's `<Defaults>` NoJournal escapes the parent's rollback and leaves an installed module with no record (M5).
9. A load that ends mid-way (Terminate or SIGKILL inside an Activate) leaves its temporary `.modules` repository registered. The repository is created before the transaction and removed only by the explicit `CleanUp()` (`Main.cls:2528-2530`). After the T/K replays, six `ipm-temp-modules-N` filesystem repositories with `OverriddenSortOrder=-1000` (first priority) still pointed at the cases' `.modules` folders. They were recorded and then deleted (`20261113T210229Z-temp-repos-leftover`). INT-LOAD-14 says the folder is "removed when loading finishes even on error"; no source covers a process end, so this is an observation with a consequence (it shadows later dependency resolution), not a confirmed defect.

Phase-1 process-end cases C1, C3 and C4R also deviate on Phases and Package under the R1-MOD-03 expectations (D10; `native-report.md` section 4.3).

## 8. Incomplete items and evidence notes

- C49T/K: APPROXIMATE only (section 5).
- S4 first binding: INCOMPLETE (binding), replaced by S4R.
- Phase-1 C4: INCOMPLETE (400 s wait), superseded by C4R and by B1-C46K.
- P-12 (IRISTEMP at restart): not run.
- Concurrent loads and self-uninstall: not run (not cheap; out of the requested scope).
- The legacy XML payload path (OBS-L16): not run (optional).
- Expectation changes after runs are recorded rather than hidden. Re-evaluations are written beside the original `verdict.json` as `verdict-reeval-<sha8>.json` (phase-1 guards, process-end revision, S4 guard) or `verdict-cases-05842975.json` (replay against the final `cases.json`). Each has an index line, and the originals are kept (R1-MOD-11).
- Vacuous-pass guards (orchestrator request) were added to phase-1 E0, E1, E2, E3, E4, E5, E7, E8, R4, D1, repro X2 and X3, stage-C H2 and every replay case (`x.ids_allocated`). Re-evaluating the earlier runs changed no verdict.
- Reproduction note for `repro/history-delete-underscore.cos` (X3): one seeded row (`zpm "load /home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/simplest-module/1.0.0/"`) satisfies its guard. `repro/history-filter-widened.cos` (X2) also needs a row whose major version is not 1, for example by then loading `…/simplest-module/2.0.0/`. Otherwise its guard `truth < total` makes it INCOMPLETE.
