# Independent review: IPM playbook uplift (§2 Reviewer, §3.8 gate)

Reviewer: independent agent, read-only on everything except this file. Date 2026-11-13.
Target: ipm 571e04a1. Paths below are relative to the experiment root unless they start with `ipm/`.

## What I ran (and saw the output of)

- `verus models/verus/load_general.rs --rlimit 30` on the unmodified file: `39 verified, 0 errors`.
- Four mutants of my own, each edited in a copy under the session scratchpad (`.../scratchpad/mut/`), run with the same Verus binary and `--rlimit 30`. Results are in finding 1.
- Read-backs of `models/verus/results/*.json` after the rerun's controls step. All 8 files report `all_killed_in_expected_place: true`, with 167 controls in total. `record_proofs.py` was still running when I finished.
- No IRIS probe or IPM test was rerun.

Items marked "(delegated)" come from two helper agents. One checked 30 O/T tags against the raw logs; the other compared the ontology with IPM's documentation. I re-opened the raw source for the items marked "(spot-checked)". The rest I report as their findings, not mine.

## Findings

### Blockers

**1. [blocker] The L1 contracts never require that a command which worked records success, or that a crash records failure. Mutants that break this survive every proof, so the P tags on HIST-B8 and HIST-B10 are not supported.**

- **Where:**
  - L1 in `models/verus/load_history.rs`: `honest_end`, L196-L204.
  - L1 in `models/verus/load_general.rs`: `honest`, L279-L291.
  - L1 in `models/verus/update_steps.rs`: `end_failed`, L341-L347.
- **What is wrong:** Each L1 constrains `ok` in one direction only (`ok ==> committed && ...`). Only the transactional atomicity clause forces `ok` when a change was kept. So in developer mode and no-transaction mode, a load that worked can be recorded as failed, and a crash can be recorded as `Success = OK`.
- **Mutants I ran:**
  1. `load_history.rs` `step_activate`: the non-transactional success branch becomes `fail_end(s1)`. Result: **11 verified, 0 errors (survives).**
  2. `load_general.rs` `step_activate`: `else { ok_end(s) }` becomes `else { fail_end(s) }`. Result: **39 verified, 0 errors (survives).**
  3. `load_history.rs` `crash`: the closed row gets `ok: true` for a no-transaction crash in Activate. Result: **11 verified, 0 errors (survives).**
  4. `load_general.rs` `crash_k`: the same edit. Result: **39 verified, 0 errors (survives).**
  5. `update_steps.rs` `step_phases`: a successful developer-mode install or load ends `Out::Failed`. I added proof hints only (no spec change). Then `inst_refines` and all step lemmas verify. The only error is in the concrete witness `inst_witness_dev_install_leaves_steps_due`, so the harness would report "killed-elsewhere", not a refinement kill.
- **The statements affected:**
  - HIST-B8 ("A command that works gets success") cites `P:lemma_refines`.
  - HIST-B10 ("after a crash ... success is not set") cites `P:lemma_refines` and `P:g_rollback_crash_honest`.
  - The explorer has the same gap: `models/explorer/explore.py` L261-L333 has no "worked ⇒ success" or "crash ⇒ not success" rule.
- **Fix:**
  - Add to L1: `phase == Ended && <the command's work completed> ==> ok`, and `phase == Crashed ==> !ok`. Do the same in update_steps `honest_end`.
  - Add the five mutants above as controls, expecting the refinement lemmas.
  - Re-tag HIST-B8 and HIST-B10 until those controls are killed.

**2. [blocker] The §3.3a write-set coverage is not honest. Several verdicts say "preserved" for pairs that the evidence shows are violated, or that the cited proof cannot speak to. The measured command set also misses the writers that matter.**

- **INV-MOD on developer-mode loads** (`semantics/coverage.json` INV-MOD × `load-dev-activate-fails`, verdict "preserved"):
  - INV-MOD says "an installed module's record and its compiled code agree".
  - LOAD-B16's own Notes say a developer-mode failure "leaves the record (with no compiled code)".
  - The cited proof, `P:g_refines`, has no compiled-code variable. `load_general.rs` `Db` has none, and `load_history.rs` `abs` drops `m_cls`.
  - The verdict text itself says "compiled code follows the record (observed)". The verdict should be a finding or "observed", not "preserved".
- **INV-DEP on developer-mode and no-transaction loads** (`load-dev-activate-fails`, `load-notxn-activate-fails`, verdict "preserved under PU, PR"):
  - `dep_refines` is soundness only for a resolution that ends Done.
  - A developer-mode load that fails on a missing dependency keeps the module installed with an unmet requirement (LOAD-B16 Notes, LOAD-B27).
- **INV-REPO:**
  - Both repo measurements failed: `ws-repo-add-fs` ended `status=err; Key not unique` and `ws-repo-delete` ended `status=err; ... does not exist` (`validation/logs/50-write-sets.log`).
  - The coverage turns the failed add into "guarded: the unique root index refuses a second registry". No successful registry create, modify or delete was ever measured.
  - `scripts/semantics_gate.py` L68-L72 accepts write sets from failed commands without comment.
- **Update steps are invisible:**
  - `^IPM.UpdateStep.AnyMemberD` and `PrimaryOnlyD` are in no `global_classes` entry (`semantics/semantics.json`).
  - The only measured update, `ws-update-with-path`, uses `simplest-module`, which has no update package. So no invariant reads step records, and the gate can never surface (invariant, update) pairs.
- **Other commands not measured:**
  - `uninstall -force` (the one that breaks INV-DEP by design).
  - An uninstall or clean of a module with web apps, file copies or globals (F-UNIN-1 writes `^SYS("Security")`).
  - `enable -community`, `install -DPermitDowngrade`, `config set` of other keys, `update -dev`, `generate`.
- **The "35 commands" count includes duplicates:** `install up-dep` twice, the `simplest-module/1.0.0` load three times, and `uninstall up-res-fail` twice.
- **Fix:**
  - Rerun the write-set probe for successful repo create, modify and delete.
  - Add an update with steps, `uninstall -force`, and an uninstall of a module with a web app, a FileCopy and a Global.
  - Add an `UpdateSteps` class and an invariant for it (A13 / INST-B71).
  - Make the gate reject write sets with `status=err` unless the entry is labelled as a refusal.
  - Change the INV-MOD and INV-DEP verdicts for developer-mode and no-transaction loads to findings, citing LOAD-B16, B23 and B27.

### Major

**3. [major] P tags on statements the code contradicts (F). Each proof uses a premise that excludes exactly the case the statement is about, and INDEX still counts them as proved.**

- **REPO-B106** ("Default modifiers count only in the namespace where they were set"), `P:repo_refines_dm_set`:
  - The lemma requires `premise_pns(nss, ns)`, which says the namespace set is the single namespace `ns` (`models/verus/settings.rs` L211, L224-L229).
  - With one namespace the statement is vacuous: there is nowhere else for the setting to count.
- The same pattern:
  - **UNIN-B31** under PW (no web app on a blocked target): the exact F-UNIN-1 case.
  - **UNIN-B66** under PCW (no web app, no installer class).
  - **UNIN-B12** under PGA.
  - **REPO-B55** under PK.
  - **REPO-B99** under PS.
  - **HIST-B1**, which carries F-HIST-10 and P together.
- The premises are named in Notes, but the evidence column reads "proved and contradicted" with equal weight. `scripts/ontology_index.py` L257-L260 does not reject P+F.
- **Fix:** forbid P together with F in the index gate, or render it as a separate kind ("proved for the fixed design / under defect premise") that is excluded from the P count. Keep the witnesses as the evidence for the F.

**4. [major] Defect witnesses tagged P on statements written as the code's behaviour, against FORMAT's "statement keeps intent" rule.**

- **LOAD-N1, HIST-N1** (`P:g_caller_rollback_leaves_no_trace`); **LOAD-N2, HIST-N2** (`P:g_operator_commit_dishonest`); **HIST-N3** (`P:g_window_crash_dishonest`):
  - Each text describes the defect ("a half-done load stays"), yet carries F, which means "the code contradicts the statement". The code agrees with the text.
  - The cells (HIST-C11, C12, C19, C26, C33, C40, per delegated check) use these rows as expected outcomes with an empty Decision, although DEC-HIST-4 and DEC-LOAD-1 are open.
- **INST-B67** cites `P:inst_witness_failed_update_repeats_effects` while DEC-INST-5 is open.
- **DEP-B13** ("IPM does not go back to change an earlier choice") cites `P:dep_witness_greedy_rejects_solvable`. The documented intent is "with backtracking" (`ipm/src/cls/IPM/Storage/Module.cls#L867`, quoted in the row's own Notes).
- **Fix:** rewrite these rows as intent ("a caller rollback does not erase the command's record", "IPM finds a fitting set when one exists"). Keep the F or OPEN tag and cite the witness from the finding, not as P on the statement.

**5. [major] P citations whose lemma does not state the statement, and premises missing from Notes.**

- **LOAD-B10** ("refused if a newer version is installed") cites `witness_refusal_is_clean`:
  - In `load_history.rs`, refusal is a free boolean `Guard(refuse)` with no versions, and the initial state is the empty namespace, so "already installed at a newer version" cannot even be represented.
  - The lemma that does state it is `inst_refines` in `update_steps.rs` (`refuses` and `blocked`). Cite that one.
- **LOAD-B16** (ordering: record kept before needs load) and **LOAD-B17** cite `lemma_refines`:
  - The L1 `keep_allowed` in non-transactional modes only says "monotone". It does not express the ordering or "own transaction, kept if all load".
  - `lemma_refines` also requires PC and PD. PD forbids `Deps(Fail)` in developer mode, which excludes the failure half of B17.
  - The Notes of LOAD-B16 and B17 name neither premise.
- **HIST-B2, B8, B9, B13, B14** cite `lemma_refines` with no premise in Notes; it needs PC and PD.
- **HIST-B9** ("its record is locked"): no model has a Finalized field. `ended` stands for both TimeEnd and Finalized, so the claim is a modelling convention, not proved.
- **HIST-B1** ("one record for load, install, update and uninstall") cites the load lemmas only:
  - The L2 has a single `top`/`top_row` variable, so "one" holds by construction.
  - Install and update create their record in `Main:Install` and `Main:Update` (UpdateInit), which is outside the model.
  - Uninstall is not in these files (`unin_refines_rows_honest` exists but is not cited).
- **Fix:** re-cite or drop these P tags. Name every requires-premise of each cited lemma in Notes; the index script could check this mechanically from the `requires` clause.

**6. [major] "Compiled code" is claimed proved but appears in no proof.**

- LOAD-B8 says "nothing it did stays. This includes its dependencies and compiled code" and cites `P:lemma_refines_txn_mode` and `P:g_refines`.
- STATUS.md's guarantee row says "compiled classes unchanged" at level P.
- `load_general.rs` has no code variable. `load_history.rs` keeps `m_cls` in L2 but `abs` and the L1 drop it. Compiled code (`^oddDEF`, `^rOBJ`, ...) is only observed (O tags).
- **Fix:** add a compiled-code component to `Db` and to L1 `unchanged` (with a control), or downgrade that clause to O.

**7. [major] update_steps.rs L1 encodes a behaviour that IRIS refutes for `load` and `update -path`.**

- `end_ok` requires `a.d.dev == a.op.dev`, and `step_manifest` sets `dev: dev_eff(op)` (`models/verus/update_steps.rs` L190-L194 and L321).
- The code keeps the stored DeveloperMode when no `DeveloperMode` parameter is passed (`ipm/src/cls/IPM/Utils/Module.cls#L1300-L1306`). That is LOAD-B21, observed as `O:ipm-dev-then-plain-load`.
- `Main:Load`, and `Main:Update` with `-path` (`ipm/src/cls/IPM/Main.cls#L4440-L4443`), pass no such parameter unless `-dev` is typed. Seeding also tests the stored mode (`ipm/src/cls/IPM/Lifecycle/Module.cls#L122`).
- So a plain `load` or `update -path` of a developer-mode module runs in developer mode, with no rollback and no seeding. The model treats it as transactional and seeding.
- This is declared as scope item U8 in `models/verus/update_steps.TRUST.md`, but INST-B14, B17, B65 and B66 carry P without that condition.
- **Fix:** add `pre.dev` to the model (dev_eff = `op.dev_given ? op.dev : pre.dev` for Path sources), or add "the module was not in developer mode before" to the Notes of every INST P row.

**8. [major] Evidence problems in O/T tags (delegated; spot-checked items marked).**

- **No way to mark refuting evidence.** Several O tags contradict their statement's text and are counted as "O" in INDEX. Examples: HIST-B7 `ipm-halt-txn` (row `load//0.0.0`), HIST-B11 `ipm-halt-dev-crash-in-dependency`, HIST-B26 `ipm-history-autopurge` (`oldRecordExists=1`, `validation/logs/34-extra.log:153`), RES-B25 `nc-gap-P8-...` (spot-checked: `shipsMarkerSourceFile=1`).
  - Most of these rows also carry F, so the text stays honest. But the INDEX O column overstates support.
  - **Fix:** a refuting-tag syntax (e.g. `O!:`) that the index counts separately.
- **T status ignores skipped assertions.** RESULT lines come from the `Shell()` status. `Test.PM.Integration.Scopes` had a skipped assertion (spot-checked: `validation/logs/893-nsp-ipm-test-skips.log`, "Registry Server not defined"), yet RES-B7 counts it as pass. Skips were checked for only 3 runs.
- **T tags claiming more than the test asserts:**
  - INST-B64: `SingleFailedUpdateTest` never compares timestamps, so "later steps do not run" is not asserted.
  - INST-B24: the test covers only `update`, and checks status and message.
  - HIST-B25: no test runs an unfiltered delete without `-confirm`.
  - UNIN-B46: order not asserted.
  - INST-B69: spot-checked; `ipm/tests/.../Update.cls#L146-L154` has an "Assert it completed" comment with no assertion. The Notes admit this.
- **Contaminated probe values:**
  - `nc-sync-N20-...` `summaryLine` captured the wrong line.
  - `nc-gap-P8-deployed-export-forced-error` `filesWithMethodSource` contains a leaked `grep: ... No such file` (spot-checked: `validation/logs/98-n2-30-p8-probe.log`).
- **Duplicates:** `evidence/ipm-tests.json` has two `Test.PM.Integration.Scopes` records (spot-checked at L209 and L473); the gate keeps the last.
- **Weak observations:**
  - NSP-B3 was run with only `%SYS` and `USER` present.
  - LOAD-B15 ("never undoes work it did not start") writes no caller data before the load.
  - The `ipm-halt-*` "crashes" are terminal HALTs whose prompt defaulted to rollback, and the `nc-gap-P1` IRIS restarts are not in any raw log.
- **Fix:** record per-method assert, skip and fail counts for T. Narrow the five T-tagged statements or add asserting tests. Add caller writes to the LOAD-B15 probe. Re-capture N20 and P8.

**9. [major] Spec vs intent: statements that present code behaviour as intent, and documented areas that are missing (delegated; spot-checked items marked).**

- **HIST-B11, B12, B13** state the Committed guarantee for every command. `ipm/src/cls/IPM/General/AbstractHistory.cls#L41-L43` says that for uninstall it only shows success, and the seed doc (`docs/ipm-uplift/index.md:33`) warns against extending it. Scope these rows to load, install and update, and add an uninstall row.
- **REPO-B59** ("one remote and one ORAS registry can hold the publish flag"; spot-checked) states the code. Its own Notes say help (`ipm/src/cls/IPM/Main.cls#L56-L57`) says one per namespace. It has no F or OPEN, and cell REPO-C17 encodes it as expected. Restate it and add `F:F-REPO-3`.
- **LOAD-B20** ("a refused module keeps its record"; spot-checked) has only `I`. It conflicts with LOAD-D6 ("a refusal is a failure ... before it changes anything") and the seed doc (`index.md:47`). Add `OPEN:DEC-NSP-5`.
- **Missing areas with documentation and IPM tests:**
  - `-env` environment files (`Main.cls#L73` etc., CHANGELOG #1260).
  - The verify and test phases and their reporting (`Main.cls#L50-L52`, `-f`, `-output-file`, `TestReportFormat`).
  - Validate and document phases.
  - The install side of resource processors: CPF merge (CHANGELOG #1274), SystemSetting, web and CSP app creation, InstallerClass, and `<Defaults>` / `${var}` expansion.
  - `load -create-lockfile`; `install -latest` / `-keywords`; the `list-installed` options.
- Seed doc `index.md:49` (KeepOnUninstall for clean as well as uninstall) is covered for uninstall only (UNIN-B19).
- **Fix:** add the rows, or record each excluded area by name in INDEX as out of scope.

### Minor

10. **[minor] L2 drops a post-commit failure point** (inspection only; not run on IRIS).
    - After the outer `tcommit` and `SetCommitted`, `LoadNewModule` calls `tModule.WriteAfterInstallMessage()` (`ipm/src/cls/IPM/Utils/Module.cls#L1353-L1354`), which runs `%Evaluate` (macro, placeholder and system-expression expansion, `Storage/Module.cls#L1833-L1855`).
    - If that throws, the load reports an error with Committed = 1 and every change kept. That breaks the L1 clause `Txn && !ok ==> unchanged`, and neither model has the step.
    - **Fix:** add a `PostCommit(fail)` step to `load_general.rs`, or name it as a premise and probe it with a bad `AfterInstallMessage`.
11. **[minor] Other L2 omissions** that are not named as premises:
    - The developer-mode plus `-DNoTransaction` combination (Mode is a 3-way enum; the code has the dev commit gated on `tUseTransactions`).
    - With zero dependencies, failures between the manifest write and Activate: `CheckSystemRequirements` at `#L1321`, the update-path name check at `#L1269`, the second `OBJ.Load` / `%Reload` at `#L1330-L1333`.
    - These have the same effect as covered failure points, so the risk is low. List them in TRUST.
12. **[minor] Update-step failure means a thrown exception, not a returned status.** `ExecuteUpdateStep` calls `do $classmethod(...)` and discards any return value (`ipm/src/cls/IPM/Storage/Module.cls#L237-L242`), and VersionBase documents no contract. INST-D4 and INST-B64 should say a step fails by throwing.
13. **[minor] Control harness locality is loose.** Many `expect` lists name 4-6 functions (e.g. `load_general.json` `dev-no-setcommitted`, `fix-without-dev-persist`), so "killed in the expected place" is weak evidence of where the mutant is detected. Narrow them to the per-step lemma the mutant targets.
14. **[minor] Open decisions are missing from cells** (delegated): LOAD-C26, C28 and C29 (DEC-LOAD-2); INST-C15 and C33 (DEC-INST-2, DEC-INST-1). Confirmed findings are not F-tagged on their statements, e.g. UNIN-B30, which cites F-UNIN-6 in Notes and has only OPEN (spot-checked). Others: F-REPO-3/REPO-B56, F-REPO-7, F-REPO-8, F-INST-2, F-RES-5, F-RES-6.
15. **[minor] No TRUST file** exists for `load_history.rs`, `load_general.rs`, `settings.rs` or `sync_baseline.rs`. Their abstractions (nat versions, one top module, `refuse` as a free input) are stated only in file headers. Dependency premises PU and PR live in `update_steps.TRUST.md` (D6, D7), which is easy to miss.
16. **[minor] STATUS.md is stale and is the account the gate signs.**
    - It says 3.3a is "Not done", 692 statements, and 3.3 for load and history only.
    - Its guarantee table lists "Every load/install/update writes exactly one top-level history record" and "compiled classes unchanged" at level P (see findings 5 and 6).
    - Regenerate it from INDEX and the rerun before sign-off.

## Confirmed sound

- **load_history / load_general:**
  - The `step_guard`, `step_manifest`, `step_activate`, `step_window` and fail paths match `ipm/src/cls/IPM/Utils/Module.cls#L1162-L1364` in order: TSTART at #L1195; guard; SetCommitted for no-transaction at #L1226; developer-mode TCOMMIT, SetCommitted, TSTART at #L1309-L1319; TCOMMIT then SetCommitted at #L1347-L1350; rollback to `tInitTLevel`.
  - The P3 window is a real separate step, and `g_window_crash_dishonest` and `g_operator_commit_dishonest` state exactly their characterizations.
  - The baseline verifies (I ran it: 39 verified, 0 errors).
  - `lemma_refines_txn_mode` is not vacuous: Txn runs satisfy `inv` and `enabled` from `initial`.
- **update_steps.rs `step_walk` and `step_lock`:**
  - They match `HandleAllUpdateSteps` and `HandleUpdateStepsFromList`: the flag resets per class (#L130) and carries from the all-member list to the primary-only list; the test is `'TimeStampEnd || ISERR(Status)` (#L190).
  - Mirror skip happens after `GetAddUpdateStep` has saved a fresh due record.
  - A busy lock returns `$$$OK` (#L105-L108).
  - Seeding keeps done records and fixes failed ones (#L203-L220).
  - `inst_ok_update_runs_iff_scheduled`, `_in_walk_order` and `inst_all_done_runs_nothing` state what INST-B55..B63 say. They are lemmas about the L1 schedule, and they become code claims only through `inst_refines` (under PL and PF5, both named in Notes).
- **uninstall.rs `check_skip` and `clean_step`:** they match `Lifecycle/Base.cls` CheckBeforeClean #L258-L302 (global-scope skip, Cycle exclusion, level > 0, force) and the FileCopy, Global and Code branches. The GetDependentsList error path is covered by premise PG.
- **No trusted items:** `scripts/record_proofs.py` refuses `assume`, `admit` and `external(_body)`, and the gate refuses P on a file whose controls are missing or not all killed (`scripts/ontology_index.py` L257-L260). After the rerun's controls step, all 8 result files report every control killed in an expected function (167 controls).
- **Mechanical evidence:** the O and T records are parsed from `OBS|` and `RESULT` lines in raw logs (`scripts/collect_evidence.py`); no hand-typed values were found (delegated).
- **Write-set measurement** (`validation/iris/Uplift/Probe/WriteSet.cls`) hashes whole globals before and after, across the namespace's databases, IRISSYS and IRISTEMP. That is a sound net-effect measure for what it covers.
- **The following O/T tags support their statements** (delegated): LOAD-B8 `ipm-dep-parent-fails`; HIST-N1; INST-B65/B66/B67 `nc-inst-4*`; INST-B70; INST-B61/B62 `TestModuleUpdateStepAdded`; UNIN-B44, B45; NSP-B44; REPO-B35/B44/B45.

## Sign-off verdict for the §3.8 gate: **REFUSE**

**Reason.** The spec does not yet say what it claims to have proved, and the 3.3a coverage marks violated pairs as preserved:

- The L1 contracts let a working command record failure and a crash record success. I ran five mutants: four pass every proof, and the fifth fails only a concrete witness, never a refinement lemma (finding 1).
- The write-set coverage marks violated or unmeasurable pairs "preserved", and the measured set leaves out the update-step, registry and web-app writers (finding 2).
- P tags are presented as proof for statements that are contradicted, vacuous under their premise, or not stated by the lemma (findings 3-7).

None of this is a defect in IPM. These are defects in the account, and each has a concrete fix.

**Conditions for a later SIGN WITH CONDITIONS:**

1. Fix finding 1 and add the five mutants as controls, all killed in the refinement lemmas.
2. Fix finding 2: remeasure successful repo commands, add an update with steps, `uninstall -force` and a web-app/FileCopy/Global uninstall; make the gate reject failed measurements; correct the INV-MOD and INV-DEP verdicts.
3. Make the index gate reject P+F (or count it separately), and move witness P tags off intent statements (findings 3 and 4).
4. Re-cite or remove the P tags in finding 5, and name each lemma's premises in Notes.
5. Either model compiled code or downgrade that clause (finding 6). Either model the stored developer mode or qualify the INST P rows (finding 7).
6. Record per-method skip counts for T tags, and narrow the five over-claiming T rows (finding 8).
7. Regenerate STATUS.md from the rerun.

Findings 9-16 may be recorded as decisions instead of fixed.
