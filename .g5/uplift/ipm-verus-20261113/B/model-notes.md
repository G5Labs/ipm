# B1 model notes: IPM load and transaction history (Verus)

Stage B1 of the IPM Verus uplift. Product `571e04a1`. Paths: `VV` = `.g5/Verus_verification`; crate `VV/crates/ipm-load-history` (`src/model.rs` L2, `src/contract.rs` L1, `src/proofs.rs`, `src/cases.rs`, `src/explore.rs`, `src/bin/b1.rs`); `VV/crates/ipm-load-history/BINDINGS.md`; `VV/premises.md`; `VV/SOURCE.json`; `VV/cases/cases.json`; receipts `VV/results/verus/<run>/`, one line each in `VV/results/index.jsonl`.

Status: COMPLETE for the load path and history writers; history readers (filters, delete, purge) are not modelled (see Gaps). The L1 contract includes the stage C round 1 revision (R1-ONT-01..03).

## 1. Model summary

- L2 (`model.rs`, all inside `exec_spec_verified!`, so each function has an executable twin proved equal to it): the IRIS process (namespace store; one snapshot per open `TSTART` level; IRISTEMP; journaling flag; `$INCREMENT` counter), and the IPM load as three activations with program counters split at every real boundary: `Main:Load` (9 steps), `LoadNewModule` for the loaded module and for each dependency (33 steps: entry, `tstart`/JournalManager, guards, no-transaction marker, manifest import, names, dev save, C1, SetCommitted, T2, system requirements, dependencies, reload, C2, 5 phases x start/body/end, C3, SetCommitted, rollback loop), and `LoadModuleReference` for a dependency (9 steps, its own log). Environment transitions: kill-like end (SIGKILL: rollback only), terminate-like end (Terminate/HALT: `%OnClose` then rollback, per B2 N-P10/N-P11), the caller committing or rolling back its own transaction, and another process editing a History row.
- Inputs (`Cfg`): transaction mode (default, NoTransaction, NoJournal), dev flag (none, `-dev`, `-nodev`), `-force`, path validity, caller-held transaction, the package (version, UpdatePackage, system requirements, Activate outcome: ok / throws / Invoke returns an error, manifest: valid / unparseable / no module document / refused by validation), up to two leaf dependencies (Activate outcome, manifest, snapshot, found), installed state before, and a variant number (0 as-built, 1-11 controls).
- L1 (`contract.rs`): `l1_verdict` from inputs alone, an `Expect` (status, module-state alternatives, one record expectation per action) for normal ends, caller ends and process ends, and `l1_ok`. Open questions are the `Q` parameter (`qprov()` = provisional options).
- Abstraction: L1 reads the durable store `dv(s)` (what survives if the process dies now), the namespace the process sees at an end state, and the auxiliary `att` (per action: started, finalized, record completed). Steps that only touch IRISTEMP or uncommitted data are stutters for L1.
- Bindings: `VV/crates/ipm-load-history/BINDINGS.md` (every step to file:line, abstractions and what they could hide).

## 2. L1 clauses and their sources

| Clause | Content | Source (A1 inventory IDs) |
|---|---|---|
| L1-STATUS | returned status OK iff verdict success | INT-LOAD-01, -09, -10, -11 |
| L1-NAMESPACE refusals | path, downgrade, update-required refusals write no module data in any mode; system requirements too under Q-17 provisional | INT-LOAD-02, -09, -10, -11, Q-17 |
| L1-NAMESPACE failures | default mode: a failed load leaves the namespace unchanged (Q-04: database only); `-dev`: manifest kept after C1, dependencies kept only if all succeed, phases outside a transaction; NoTransaction/NoJournal: partial changes allowed | INT-LOAD-05, -06, -08 |
| L1-NAMESPACE success | module installed at the package version, dev mode `-dev` / `-nodev` / stored (Q-01), dependencies installed non-dev except snapshot under `-dev` | INT-LOAD-01, -07, -15, Q-01 |
| H-RECORD-EXISTS | one record per attempted action, including each dependency load; it survives the action's own rollback and a parent's rollback (#366, stage C R1-ONT-01); under a caller-held transaction survival is Q-06b (provisional: survives); a record exists once its action started, including after either kind of process end | INT-LOAD-17, INT-HIST-01, -02, -03, -08 |
| H-RECORD-FIELDS | Success = the action's status; TimeEnd set and Finalized iff the action ended normally; an unended record says in progress (Success 0, no TimeEnd) and is not Finalized (Q-FIN provisional); Package set once the manifest was read for refusals after that point and for failures (R1-ONT-03); phases started are listed and the last one ends with its status (Q-14); a placeholder version on an unfinished record is permitted, not required (R1-ONT-02) | INT-HIST-03, -04, -14, 3.2 fields, Q-14, Q-FIN |
| H-COMMITTED | Committed = whether any of the action's changes were kept; exact for top-level actions at `$TLEVEL` 0 (Q-05 provisional); on an unended record too (Q-08, provisional changed by R1-ONT-02 to "reflects kept changes"); uninstall exception out of scope | INT-HIST-05, -06, -07, Q-05, Q-08 |
| R1 | a durable History row is never removed by the load | INT-HIST-02 |
| R2 | a finalized durable row never changes (another writer's UPDATE/save refused) | INT-HIST-14 |

Q parameters (provisional first, alternative second; `contract.rs` `Q`):

| Q | Provisional | Alternative | Cases whose verdict depends on it |
|---|---|---|---|
| Q-01 | plain load keeps stored dev mode | reset to non-dev | B1-C20 |
| Q-02 | `-dev` bypasses the UpdatePackage guard | only `-force` | B1-C17 |
| Q-05 | Committed exact only for top-level actions at `$TLEVEL` 0 | exact for every action | B1-C41, C43 (records lost before the question arises), C50 |
| Q-06b (new, R1-ONT-01) | records survive a caller-held transaction's rollback or the process end inside it | may be lost | B1-C42, C43, C50T, C50K |
| Q-08 (default flipped, R1-ONT-02) | unended record's Committed reflects kept changes | unconstrained (unknown outcome) | B1-C46K, C48K, C49T, C49K |
| Q-14 | every started phase ends with its status | open phases accepted | B1-C28 |
| Q-17 | system requirements checked before any write in every mode | kept manifest accepted | B1-C23, C24 |
| Q-FIN (new, R1-ONT-02) | an unended record is not Finalized | may be | B1-C46T, C48T, C49T |

Q-06 (dependency record survives the parent's rollback) is no longer a parameter: it is required.

## 3. What is proved (Verus, `--num-threads 4`, default rlimit)

Final as-built run: `20261113T194721Z-asbuilt-proofs-l1rev1`: verified 248, errors 0 (includes the 180+ exec-twin equality obligations, all proofs and the case lemmas). Earlier green run before the L1 revision: `20261113T193253Z-asbuilt-proofs` (247). A failed earlier run `20261113T192717Z-asbuilt-proofs` (2 resource-limit errors, fixed by splitting functions) is preserved.

Domain: every `Cfg` with `cfg_wf(c)` (all modes, dev flags, force, caller transaction, installed states, up to two leaf dependencies, all faults) and `c.var == ACTIVE_VARIANT()` (0 as-built). Theorems are symbolic over this domain, not bounded.

| Theorem (`proofs.rs`) | Statement |
|---|---|
| `lemma_init_inv`, `lemma_step_inv`, `lemma_kill_inv`, `lemma_terminate_inv`, `lemma_caller_inv`, `lemma_edit_inv`, `lemma_next_inv`, `theorem_reachable_inv` | `inv` holds initially and is preserved by every L2 transition (`next`); every state of every trace from `init(c)` satisfies `inv` |
| `theorem_top_record_durable` (R3-top) | no caller transaction: once the load's History row was inserted, the durable store holds it in every reachable state (own rollback, either process end) |
| `theorem_commit_sound` (COMMIT-SOUND) | no caller transaction, returned: if any module changed, the row reads Committed=1 |
| `theorem_top_record_complete` | no caller transaction, returned: the row exists, is Finalized, has TimeEnd, Success = returned status |
| `theorem_tlevel` | each LoadNewModule activation returns at its entry `$TLEVEL` |
| `theorem_persist_target` | PersistToTwin always finds its row (the N-P15 branch is unreachable from the load) |
| `theorem_r2` (R2) | no transition (load step, kill, terminate, caller end, foreign edit) changes a finalized durable row |
| `theorem_r1` (R1) | no transition except the caller's own rollback removes a durable row |
| `lemma_inv_nonvacuous` | `inv` holds in a concrete initial state |

`inv` also carries: store shape, transaction depth per activation, journaling flag, which slots are fresh, the finalized-row and row-existence chains along the snapshot stack, and the commit-soundness conjuncts.

Case lemmas (`cases.rs`, `by (compute_only)` on the spec machine; configurations use `ACTIVE_VARIANT()`): `case_c01_conforms`, `case_c11_downgrade_refused`, `case_c15_update_refused`, `case_c08_committed_without_change`, `case_c09_committed_without_change`, `case_c23_sysreq_dev_keeps_manifest`, `case_c28_phase_left_open`, `case_c29_error_after_commit`, `case_c31_dep_record_lost`, `case_c33_dev_dep_record_lost`, `case_c45_kill_dep_record_lost`, `case_c45_terminate_dep_record_lost`, `case_c46_dev_end_inside_activate`, `case_c42_caller_rollback_record_lost`. A deliberately false variant was checked to fail ("expression simplifies to false").

Bounded evidence (explorer over the executable twins, single-fault domain: 3 transaction modes x 3 dev flags x caller transaction or not x 5 installed states x 5 dependency shapes x 16 faults, 900-5400 configurations per run, every run exhaustive within its domain, `truncated=false`; depth <= 119): `asbuilt-invariants-{default,notx,nojournal}` PASS (R1, R2, R3-top, COMMIT-SOUND, TLEVEL, PERSIST-TARGET, ISOK-REDUNDANT; 177-183k states each), `asbuilt-r2-with-foreign-edits` PASS (480k states). Final receipts `20261113T1949xxZ-*`. Survey of all 363,372 end states of 5400 configurations: `20261113T194804Z-survey-asbuilt-l1rev1` (`survey.json` there).

## 4. Confirmed as-built violations (model level)

Each is an L2 execution from the initial state that violates the stated L1 clause, exhibited by a Verus case lemma and/or an explorer trace, and listed with its product path. "Native" gives B2's observation where one exists (B2 `native-report.md` sections 4-5; replay `native-replay.md`).

| # | Violation (clause) | Cases | Proof / trace | Product path | Native |
|---|---|---|---|---|---|
| V1 | Dependency records lost when the parent's transaction rolls back (H-RECORD-EXISTS; required by #366) | B1-C31, C32, C33, C36, C37 | `case_c31_dep_record_lost`, `case_c33_dev_dep_record_lost`; explorer `asbuilt-r3-dep` (violation, shortest trace 15 steps) | `LoadModuleReference` LoadInit/twin insert (Utils/Module.cls:63-68 → HistoryTemp.cls:21) and `%OnClose` persist inside the parent's T1/T2; parent rollback loop 1358-1362 | confirmed (B2 A1, A2, A3; D1) |
| V2 | Process end inside the parent's transaction loses dependency records (H-RECORD-EXISTS) | B1-C45T/K, C47T/K | `case_c45_kill_dep_record_lost`, `case_c45_terminate_dep_record_lost` | same insert inside T1, then end-of-process rollback | T variants confirmed by B2 replay (interim); K pending |
| V3 | `-dev`/NoTransaction/NoJournal load refused by system requirements keeps the manifest (L1-NAMESPACE under Q-17 provisional) | B1-C23, C24 | `case_c23_sysreq_dev_keeps_manifest` | C1 commit (1309-1314) or no-tx marker (1227) and manifest import precede `CheckSystemRequirements` (1321) | confirmed (B2 B2-B4; D4) |
| V4 | NoTransaction/NoJournal load whose manifest is refused after the marker reads Committed=1 with nothing kept (H-COMMITTED exactness) | B1-C08, C09 | `case_c08_committed_without_change`, `case_c09_committed_without_change`; explorer `asbuilt-commit-exact` | `SetCommitted` at 1226-1228 precedes `OBJ.Load` (1251), whose ImportStream rolls back its own nested transaction | B2 replay pending (not in B2 phase 1) |
| V5 | Activate-phase Invoke returning an error leaves the phase open (H-RECORD-FIELDS under Q-14) | B1-C28 | `case_c28_phase_left_open` | Storage/Module.cls:669-671 `quit:$$$ISERR` bypasses FinalizePhase | confirmed (B2 D6) |
| V6 | Error after the final commit (CleanUp) returns failure with the module installed in default mode (L1-NAMESPACE) | B1-C29 | `case_c29_error_after_commit` | `tmpRepoMgr.CleanUp()` status after `LoadNewModule` committed (Main.cls:2529-2531, OBS-L12) | B2 replay pending |
| V7 | Refusal after the manifest was read records an empty Package (H-RECORD-FIELDS, R1-ONT-03) | B1-C11..C15, C18 | `case_c11_downgrade_refused`, `case_c15_update_refused` | guards 1213-1223 precede `SetName` (1293) | confirmed (B2 g3, g6, g7; D9) |
| V8 | Caller-held transaction: the caller's rollback (or process end inside it) removes the load's record (H-RECORD-EXISTS under Q-06b) | B1-C42, C43, C50T/K | `case_c42_caller_rollback_record_lost` | twin insert and persist join the caller's transaction | confirmed (B2 D1 case, section 6) |
| V9 | Kill-like end after a commit: unended record reads Committed=0 though changes were kept (H-COMMITTED under Q-08 provisional) | B1-C46K, C48K, C49K, C49T | `case_c46_dev_end_inside_activate` | twin keeps Init values until `%OnClose` (N-P14); SetCommitted writes IRISTEMP only | confirmed (B2 C4R) |
| V10 | Terminate-like end at `$TLEVEL` 0 finalizes an unended record (H-RECORD-FIELDS under Q-FIN) | B1-C46T, C48T, C49T | `case_c46_dev_end_inside_activate` | `%OnClose` → PersistToTwin sets Finalized regardless of TimeEnd | confirmed (B2 C2) |

The explorer run `asbuilt-l1-end` (violation) gives the shortest L1 counterexample across all of these; the survey lists every violating class with an example.

## 5. Controls and verdicts

A control removes one guard or effect (variant number in `Cfg.var`; Verus feature `control-NN`). Explorer verdicts use the final receipts `20261113T19491x-19492xZ-control-*`; L1 controls are evaluated on the configurations where the as-built satisfies the same clause, so a violation is caused by the mutation (baselines `asbuilt-l1-normal-conforming-domain` PASS 1617 configs, `asbuilt-l1-ns-all-conforming-domain` PASS 5040 configs). Verus verdicts: receipts `20261113T194935Z-control-01-verus-l1rev1` through `20261113T195508Z-control-11-verus-l1rev1` (one Verus build per control feature, after the L1 revision; control 05 "verified 248, errors 0", control 10 "verified 243, errors 5", the others stop at a failed evaluation lemma and list their failing lemmas).

| Control | Mutation | Property (explorer) | Explorer verdict | Verus verdict (failing lemmas) |
|---|---|---|---|---|
| 01 | no SetCommitted after C1 | COMMIT-SOUND | detected | detected: `lemma_lnm0_b`, `case_c23` |
| 02 | no SetCommitted after C3 | COMMIT-SOUND | detected | detected: `lemma_lnm0_d`, `case_c29` |
| 03 | no SetCommitted before writes without a transaction | COMMIT-SOUND | detected | detected: `lemma_lnm0_a`, `case_c08` |
| 04 | rollback loop removed | TLEVEL; L1-END-NORMAL | detected; detected | detected: `lemma_lnm0_d`, `lemma_lnm1_d`, `case_c28` |
| 05 | Finalize without `|| ISOK(status)` | COMMIT-SOUND; L1-END-NORMAL (with and without caller transactions) | survives (all three) | survives (verified, 0 errors) |
| 06 | IRISTEMP rolled back with the transaction (P-02 negated) | L1-END-NORMAL | detected (record content after rollback) | detected: `case_c28`; also `lemma_kill_inv`, `lemma_terminate_inv`, exit lemmas (the auxiliary IRISTEMP snapshots are not constrained by `inv`, so these failures are partly an invariant gap) |
| 07 | PersistToTwin does not set Finalized | L1-END-NORMAL | detected | detected: `lemma_top_step` (TOP-RECORD-COMPLETE), `case_c01` |
| 08 | downgrade guard removed | L1-NS-ALL | detected | detected: `case_c11` |
| 09 | UpdatePackage guard removed | L1-NS-ALL | detected | detected: `case_c15` |
| 10 | Finalized trigger removed | R2 (with foreign edits) | detected | detected: R2/inv lemmas (`lemma_top_step`, `lemma_lmr_step`, `lemma_mono_edit`, `lemma_mono_lmr`, `lemma_mono_terminate`) |
| 11 | process end keeps the open transaction (P-05 negated) | L1-NS-ALL; R3-top (11b) | detected; survives (11b) | detected: `case_c45_kill` |

Surviving controls investigated:
- 05 is equivalent on the load path: every successful LoadNewModule path calls SetCommitted (no-transaction marker, C1, or C3) before Finalize, so `|| ISOK` never changes the value. Checked by the explorer property ISOK-REDUNDANT (passes in all as-built invariant runs). `|| ISOK` matters for uninstall, which is out of scope.
- 11b is a deliberate wrong binding: R3-top does not depend on the end-of-process rollback because the top row is inserted at `$TLEVEL` 0.
- Superseded control evidence (kept, with notes appended to `index.jsonl`): the first L1-END control runs (`…193031Z-…193034Z`) used the full domain, where the as-built already violates L1, so their violations were not caused by the mutation; the runs `…194808Z-…194812Z` used an as-built-conforming domain that the L1 revision had emptied (0 configurations), so their "pass" is no verdict; the Verus control runs `…193359Z-…193934Z` were marked INCOMPLETE by a receipt-parser gap (Verus prints no summary when an evaluation lemma fails) and were re-run.

## 6. Native reconciliation (read after cases.json was emitted and receipted)

cases.json history:
- `fc7adec492c7a55f64ecddb44798359c652fbc21beef9973e01d531f123ec8f0` (run `20261113T193019Z-cases-asbuilt`), 57 cases, 15 L1 violations. B2 replays this file.
- `993008003ef5cda84ef42e503619b314af60a4f61cf764b159e1be1f67714557` (run `20261113T194755Z-cases-asbuilt-l1rev1`) after the stage C L1 revision. `l2_predicted` and `inputs` are byte-identical for all 57 cases; `l1_permitted` changed for 31 cases; 31 L1 violations.
- Verdicts that flipped (conforms → violates): B1-C11, C12, C13, C14, C15, C18 (Package, R1-ONT-03); B1-C42, C43, C50T, C50K (Q-06b); B1-C46K, C48K, C49K (Q-08 default); B1-C46T, C48T (Q-FIN); B1-C49T (Q-08 and Q-FIN). No verdict flipped the other way.

Comparison with B2's native observations (phase 1 report sections 4-7, and the interim phase 2 replay): no disagreement found between `l2_predicted` and the product.
- Phase 1 cases matching B1 cases field by field where both observe: A0/A1/A2/A3/A5 ↔ C30/C31/C33/C32/C38; B1-B4 ↔ C22, C23, C24 (and the NoJournal twin of C24); C1 ↔ C44T (Init-value row, IRISTEMP row deleted); C2 ↔ C46T top record (Finalized, Package and version set, Success 0, Committed 1, no TimeEnd, Activate open); C3 ↔ C44K (orphan IRISTEMP row); C4R ↔ C46K top record (Init values, Committed 0; orphan IRISTEMP row with Committed 1); D1 ↔ C42; g1-g10 ↔ C12, C14, C15, C17, C19-C21.
- Phase 2 interim (B2 `native-replay.md`, 19:45Z): returning cases C01-C43 `l2_predicted` AGREES 43/43, T variants AGREES 6/6; B2's L1 judgement matched `l1_vs_l2` in every case (against the fc7adec4 expectations). K variants and C49T/K (approximated by a ZBREAK stall) were still running. Final per-case disagreements, if any, are B2's to report; none was known at the time of writing.

## 7. Trust inventory

- `assume`, `admit`, `external_body`: none in the crate. No `#[verifier::external]` code is relied on by a proof.
- Uninterpreted functions: none (all spec functions are open and defined).
- `exec_spec_verified!`: the macro's generated exec twins are verified against the spec functions (part of the 248 obligations); the macro itself and vstd are trusted (pinned vstd `=0.0.0-2026-09-20-0158`).
- `by (compute_only)`: case lemmas rely on Verus's interpreter.
- Explorer (`vendor/explorer`, plain Rust) and the harness (`explore.rs`, `bin/b1.rs`) are unverified; their results are labelled bounded. The harness only chooses actions and evaluates exec twins of spec predicates.
- Premises: `VV/premises.md` (P-01..P-12 mapped to B2 N-P01..N-P18). All premises the theorems use are CONFIRMED natively; P-06c (order of `%OnClose` at Terminate across frames, matters only for NoJournal under a caller transaction) is UNVERIFIED; P-09 and P-10 are PARTIAL.

## 8. Remaining gaps

- History readers are not modelled: `history find/delete` filters (B2 D2), retention purge (D3), `config delete HistoryRetain` (D5), `-globally`, `GlobalRecords`. Their contracts (INT-HIST-10..13) have only B2's native evidence.
- Commands sharing the path but not modelled: `install` (developer-mode overwrite guard Q-03, read-only repositories), `update` (MarkResourceForUpdate, ApplyUpdateSteps), `reinstall`, uninstall histories, self-uninstall; legacy XML installs; module `<Defaults>` setting NoTransaction for one module; nested dependencies and more than two; non-database effects (Q-04: archives, ModuleRoot, FileCopy, pip, CPF, lock file, git temp dirs).
- The `Committed`-exactness (no false "committed") direction is not a theorem: it fails as-built (V4) and the explorer shows the counterexample.
- Q-05 and Q-06b semantics under a caller transaction are observed, not proved; L1 for dev-mode kills is looser than the real reachable set (each module independently "unchanged or present/installed").
- `inv` does not constrain the control-06 auxiliary IRISTEMP snapshots; the Verus control-06 failures partly reflect that.
- No repair variants were modelled (none was needed to state the findings). Candidate repairs: write a dependency's History row outside the parent's transaction (or persist nested logs from IRISTEMP at the top-level close at `$TLEVEL` 0); check system requirements before the first write; call SetCommitted only after a write was kept; copy SetCommitted to the twin when `$TLEVEL` is 0; set Package before the guards; do not set Finalized without TimeEnd.

## 9. Reproduce

```sh
cd VV/crates
python3 -I scripts/receipt.py verus asbuilt-proofs as-built "verified, 0 errors" "as-built proofs"      # expect: verified 248, errors 0
python3 -I scripts/receipt.py verus control-01-verus control-01 "failure of lemma_lnm0_b" "control 01" --features control-01   # expect: errors in lemma_lnm0_b and case_c23
python3 -I scripts/receipt.py cases cases-check     # rewrites VV/cases/cases.json; expect sha256 9930080…14557 with unchanged inputs
target/release/b1 explore asbuilt-r3-dep           # expect: VIOLATION of R3-dep after 15 steps
target/release/b1 explore control-10-no-trigger    # expect: VIOLATION of R2
```
`scripts/dev-verify.sh` is a development helper without receipts.
