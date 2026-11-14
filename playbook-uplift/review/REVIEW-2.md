# Second independent review: IPM playbook uplift (§2 Reviewer, §3.8 gate)

Reviewer: independent agent, read-only on everything except this file. Date 2026-11-13.
Target: ipm 571e04a1. Paths are relative to the experiment root unless they start with `ipm/`.
Subject: the current artifacts after the builders' fixes for `review/REVIEW.md` (the first review, which refused sign-off).
I did not trust the builders' summaries. Every status below comes from the files, the gate scripts, or runs listed here.

## What I ran (and saw the output of)

- **Gates, in check mode, on the real tree:**
  - `python3 -I scripts/ontology_index.py --check` returned 0.
  - `python3 -I scripts/semantics_gate.py --check` returned `129 pairs, 0 problems`.
- **Verus baselines** on copies under the session scratchpad, with `--rlimit 30`:
  - `load_general.rs`: 39 verified, 0 errors.
  - `update_steps.rs`: 39 verified, 0 errors.
  - `scopes.rs`: 8 verified, 0 errors.
- **Eleven mutant runs of my own** in four models. Each edited a scratch copy, used the same Verus binary and `--rlimit 30`, and located errors by enclosing function. The table is in section B.
- **An index-gate probe** on a scratch copy of `ontology/`, `evidence/`, `decisions/`, `findings/`, `scripts/` and `models/verus/`. I injected three P tags and ran `--check` (section B, N5).
- **Read-only IRIS queries** in the running container `ipm-uplift-iris`, which is the container that produced `validation/logs/5{0,3,5}-write-sets*.log`:
  - `ls /usr/irissys/mgr/`;
  - `%SYS.Namespace.GetGlobalDest`;
  - `$data` and `$order` on `^SYS("Security","ApplicationsD")` and on `^SECURITY` in both IRISSYS and IRISSECURITY;
  - `Security.Applications.Exists("/csp/user")`.

  Nothing was written.
- **Read-backs:**
  - `models/verus/results/*.json`: 8 files, 193 controls. All are `killed` with a numeric verification count, and none is a compile error.
  - `evidence/proofs-*.json`: all verified, no trusted items, `controls_all_killed: true`.
  - The full-rerun log `validation/logs/49-full-rerun.log`: `fail=0`. No source file is newer than its start time (21:02:36Z).
- **Sampling:**
  - 24 P tags, listed in section C.
  - 19 O, T and O! tags against their raw `values` and logs (sections A8 and C).
- **Not run:**
  - No IPM test and no IRIS probe was rerun.
  - The explorers and the controls harness were not rerun; I read their output in the rerun log.
- **Side effect to disclose.** My scratch script imported `scripts/ontology_index.py`, and that refreshed `scripts/__pycache__/ontology_index.cpython-311.pyc` (16:27:12). This is a bytecode cache only. No source, evidence or generated file changed.

## A. The first review's findings and conditions, checked against the current artifacts

### Findings 1–16

| # | Status | Evidence |
|---|---|---|
| 1 L1 one-way outcome | **Partly resolved** | **load_general (resolved):**<br>• `honest` now has `phase == Ended ==> (ok <==> worked(cfg))` and `phase != Ended ==> !ok` (`models/verus/load_general.rs` L298-L301).<br>• `worked` reads the configuration, which the L1 holds fixed, so the L2 cannot bend it.<br>• Controls `ok-recorded-as-failure`, `crash-records-success` and `dep-failure-recorded-as-success` are killed in `inv_activate` and `inv_crash`/`inv_instance_crash`.<br>• The explorer has rule `L1-SUCCESS-EXACT`, with 0 violations (rerun log L313).<br>• `load_history.rs` is retired, and no tag cites it.<br>**update_steps (not resolved):** the two-way rule is relative to a ghost `fault` that the L2 step functions write themselves. Two mutants that report Failed after a successful run, and set `fault` to match, pass `inst_refines` and every step lemma (my U1 and U2c, section B). See N2. |
| 2 write-set coverage | **Partly resolved** | **Done:**<br>• Successful repo commands were remeasured: `ws-repo-create-fs`, `-modify`, `-delete-ok`.<br>• `UpdateSteps` is a global class.<br>• `INV-STEPS` exists.<br>• An update with steps, `install -force` with steps, a FileCopy uninstall, a web-app uninstall and `uninstall -force` were measured.<br>• The gate rejects `status=err` unless the entry is labelled `refusal` or `failure_run` (`scripts/semantics_gate.py` L109-L110).<br>• INV-MOD and INV-DEP for developer-mode and no-transaction loads now read "named exception (A4)".<br>**Not done:**<br>• The web-app measurement cannot see the web-app write: the probe does not scan IRISSECURITY (N1).<br>• `INV-DEP × uninstall-force-needed` is "preserved" for exactly the `-force` exception, with no DEC (N3).<br>• No Global-resource uninstall was measured.<br>• Several other "preserved" verdicts still rest on observation, or on a lemma about another command (N3).<br>• The count is still 45 measurements of 40 distinct commands. |
| 3 P on contradicted statements | **Resolved** | • `ontology_index.py` L299-L300 rejects P+F. I confirmed it on a scratch copy: adding `P:g_refines` to HIST-B11 produced the P+F violation.<br>• REPO-B106, UNIN-B31, UNIN-B66, UNIN-B12, REPO-B55 and REPO-B99 now carry F and O! with no P. Each Notes explains why the premise lemma is not cited.<br>• HIST-B1 has no P.<br>• No self-test covers the new rule (N5). |
| 4 defect witnesses as P on behaviour text | **Mostly resolved** | **Rewritten as intent, with F, O! and OPEN, and no P:**<br>• HIST-N1, HIST-N2 and HIST-N3;<br>• INST-B67 (OPEN:DEC-INST-5);<br>• DEP-B13 (OPEN:DEC-DEP-1).<br>**LOAD-N2** has no P.<br>**Left:**<br>• Cells HIST-C11, C12, C19, C26, C33 and C40 still name these OPEN rows with an empty Decision (only C25 and C32 carry DEC-LOAD-1).<br>• LOAD-N1 keeps `P:g_caller_rollback_leaves_no_trace` with OPEN. That is acceptable for a kind-N row describing the platform behaviour.<br>• See N7. |
| 5 P not stated by the lemma; premises missing | **Mostly resolved** | **P tags removed:** LOAD-B10, LOAD-B16, LOAD-B17, HIST-B1 and HIST-B9.<br>**Premise check:**<br>• Every P lemma now has premises declared by `// PREMISE fn = LABEL`.<br>• The gate checks the Notes against each lemma's `requires`. My probe confirmed it catches a missing premise.<br>**Gaps:**<br>• The check misses premises written inline or through a wrapper (N5).<br>• Stale lemma names remain in Notes (N8). |
| 6 compiled code claimed proved | **Resolved** | • LOAD-B8 no longer mentions compiled code.<br>• LOAD-B30 states it with O only.<br>• `load_general.TRUST.md` G4 records the scope. |
| 7 stored developer mode | **Resolved** | • `dev_run` uses the stored mode when no flag is typed (`update_steps.rs`, `dev_run`).<br>• The guard reads the parameter only (`dev_guard`).<br>• Seeding tests the run's mode.<br>• PF5 now covers "no flag typed while stored in developer mode" for registry loads.<br>• Controls `stored-dev-ignored`, `guard-reads-stored-dev` and `seeding-tests-typed-flag` are killed in place.<br>• INST Notes name PF5 with the stored-mode wording.<br>• New observations: `nc-inst-fix-stored-dev-*`. I checked `-plain-load` (0 step rows, version 1.0.0/dev) and `-load-newer` (refused). |
| 8 O/T evidence | **Partly resolved** | **Done:**<br>• An `O!` tag exists and is counted separately (51 statements).<br>• HIST-B7, B11 and B26 and RES-B25 now use O!.<br>• T was dropped where it over-claimed: INST-B64, INST-B24, UNIN-B46 and INST-B69.<br>• N20 was re-captured (`nc-nsp-fix-sync-N20-summary`).<br>• The duplicate test records were merged.<br>• NSP-B3 was rerun with 18 namespaces (`nc-nsp-fix-B3-fresh-install-many-ns`: `otherNamespacesChanged=none`).<br>• The P8 leak is documented in RES-B25 Notes.<br>**Not done:**<br>• Skips are recorded per class, as the maximum across containers, not per method. 61 of 113 test records say "unknown (run's container removed)". The gate does not refuse a T tag whose skip count is unknown or non-zero.<br>• HIST-B25 ("with no filter it needs -confirm") still cites `T:Test.PM.Integration.History`, which always passes `-confirm` (`ipm/tests/.../History.cls` L30, L47 ...). Its new O, `nc-gap-P2-dev-confirm-after-release`, shows only that a delete with `-confirm` works (`deleteConfirm=ok`).<br>• LOAD-B15's observation still writes no caller data, so its second sentence is unsupported. |
| 9 spec vs intent; missing areas | **Resolved, or recorded** | • HIST-B11 to B13 are scoped to load, install and update, and HIST-B31 covers uninstall.<br>• REPO-B59 and REPO-B60 were restated, with F-REPO-3.<br>• LOAD-B20 has OPEN:DEC-NSP-5.<br>• `ontology/OUT-OF-SCOPE.md` names the excluded areas, and INDEX renders them.<br>• One exclusion is a seed-doc contract (N11). |
| 10 post-commit failure point | **Recorded (partly)** | • `load_general.TRUST.md` G7 names premise PM and records two native attempts that did not make it throw (`O:ipm-post-commit-message-fails`).<br>• PM is not named in the Notes of the rows whose P depends on it (LOAD-B8, HIST-B12, HIST-B13), because it is not a spec function (N12). |
| 11 other L2 omissions | **Recorded** | `load_general.TRUST.md` G8. |
| 12 step failure = throw | **Resolved** | INST-B64 Notes, and `update_steps.rs` comment L233-L235. |
| 13 control locality | **Partly resolved** | • The update_steps controls each expect one function.<br>• In load_general, several controls still list 3–4 functions: `no-rollback-on-failure`, `fix-without-dev-persist`, `fix-keeps-logged-committed` and `l1-without-commit-soundness`. |
| 14 open decisions in cells; untagged findings | **Resolved** | • LOAD-C26, C28 and C29 carry DEC-LOAD-2; INST-C15 carries DEC-INST-2; INST-C33 carries DEC-INST-1.<br>• Every finding is F-tagged on a statement except five. F-DEP-4, F-DEP-5, F-DEP-6, F-INST-3 and F-REPO-6 each state why: documentation drift, evidence gap, or refuted. |
| 15 TRUST files | **Mostly resolved** | • `load_general.TRUST.md`, `settings.TRUST.md` and `sync_baseline.TRUST.md` exist.<br>• `dep_resolve.rs` and `scopes.rs` have none.<br>• DEP-B21 still points to `update_steps.TRUST.md` D6 and D7 for PU and PR.<br>• The root `TRUST.md` is stale (N8). |
| 16 STATUS stale | **Not resolved** | Being regenerated; not my subject. What it must say is listed at the end of section D. |

### Conditions 1–7

| # | Status | Evidence |
|---|---|---|
| 1 finding 1 fixed; five mutants as controls killed in refinement lemmas | **Partly met** | • The load_general mutants are controls, killed in the invariant lemmas that `g_refines` calls.<br>• The update_steps mutant 5 is a control killed in `inst_step_phases`, but only because the control leaves the `fault` ghost alone. The same mutant with the ghost set survives (U1). |
| 2 finding 2 fixed | **Not met** | N1, N3. |
| 3 P+F rejected; witness P off intent rows | **Met** | Finding 3 and finding 4 above. |
| 4 re-cite or remove P; premises in Notes | **Met** | Gate-checked through PREMISE declarations; one mechanism gap (N5). |
| 5 compiled code; stored developer mode | **Met** | Findings 6 and 7 above. |
| 6 skip counts for T; narrow the five T rows | **Partly met** | Four of the five rows were narrowed; HIST-B25 was not. Skips are per class, and half are unknown. |
| 7 regenerate STATUS.md | **Not met** | In progress. |

Totals:

- **Findings:** 7 resolved (3, 6, 7, 9, 12, 14, and 11 as recorded); 5 mostly or partly resolved (4, 5, 13, 15, and 10 as recorded); 3 partly resolved with real gaps (1, 2, 8); 1 not resolved (16).
- **Conditions:** 3 met, 2 partly met, 2 not met.

## B. New findings

### My mutants

All runs used a scratch copy, `--rlimit 30`, and the same Verus binary. "Hints" means I changed only proof bodies, so the proof could follow the mutated step. No spec function changed beyond the mutated step.

| Id | Model | Mutation | Result | Reading |
|---|---|---|---|---|
| U1 | update_steps | `step_phases`: a successful developer-mode install or load ends `finish(L2 { fault: true, ..s }, Out::Failed)`. Hints in `inst_step_phases` assert `end_failed`. | 38 verified, 1 error. The only error is in `inst_witness_dev_install_leaves_steps_due`. | **Survives** `inst_refines` and every step lemma. |
| U2 | update_steps | Walk end (seeding and update) ends Failed, with `fault` set. | 36 verified, 3 errors, including an rlimit in `inst_walk_end`. | Inconclusive. Rerun narrower as U2b and U2c. |
| U2b | update_steps | U2 restricted to the seeding walk, with hints. | 37 verified, 2 errors. One is the old hint `assert forall ... done(...)`, which is false after rollback. | Inconclusive (a stale hint). |
| U2c | update_steps | U2b with the stale hint removed. A non-developer install whose seeding walk completed reports Failed and rolls back. | 38 verified, 1 error, only in the same witness. | **Survives** every refinement lemma. |
| LG1 | load_general | The top record is never named: `named: false` in `step_manifest`, `running_temp` and `dev_row`. | 39 verified, 0 errors. | Survives. No P row claims the record names the module (HIST-B7 is F). |
| LG2 | load_general | A failed dependency's record is left not ended: `dep_row(false, kept, false)`. | 39 verified, 0 errors. | Survives. No P row claims dependency records end (HIST-B9 and HIST-B18 have no P). |
| S1 | scopes | L1 `l1_exported` gets `!r.generated &&`, which is RES-B11's "never exported". | 7 verified, 1 error, in `res_export`. | The model **exports** generated resources (N4). |
| S2 | scopes | `on_phase` compiles a generated resource as the code does: `st1.loaded \|\| r.generated`. | 7 verified, 1 error, in `res_present_one`. | The L1 encodes "generated is never compiled", against the code (N4). |
| UN1 | uninstall | `uninstall zpm` in a mapped namespace reports OK and removes nothing. | 49 verified, 2 errors, in `unin_main_shape` (a helper) and `unin_refines_not_found`. | Killed, but in a helper. |
| UN1b | uninstall | UN1, with the helper's ensures adapted to the mutant. | 48 verified, 3 errors, in `unin_refines_honest`, `unin_refines_not_found` and `unin_refines_self_guard`. | **Killed** by the two-way L1. Here the outcome is compared with observable state, which is sound. |

### N1. [blocker] The write-set probe does not scan IRISSECURITY, so web-app and other security writes are invisible to the 3.3a gate

- **Where:**
  - `validation/iris/Uplift/Probe/WriteSet.cls` `Databases` (L8-L19). It scans IRISTEMP, IRISSYS and each namespace's global and routine databases.
  - `semantics/coverage.json` and `semantics/COVERAGE.md`, `uninstall-webapp` entries.
  - `TRUST.md` T16.
- **Evidence:**
  - These are my read-only queries in `ipm-uplift-iris`, the container of `55-write-sets-review.log`.
  - `/usr/irissys/mgr/irissecurity/` exists.
  - `$data(^|"^^/usr/irissys/mgr/irissecurity/"|SECURITY)=10`, and its first subscript is `ApplicationsD`.
  - `$data(^|"^^/usr/irissys/mgr/"|SECURITY)=0`.
  - `$data(^SYS("Security","ApplicationsD"))=0` in %SYS, while `Security.Applications.Exists("/csp/user")=1`.
  - So web-app definitions live in `^SECURITY` in IRISSECURITY.
  - The `rest-app` fixture declares a `<WebApplication>` (`ipm/tests/.../_data/rest-app/module.xml` L20). Its uninstall ran `Unconfigure` (`55-write-sets-review.log` L359-L362).
  - Yet `ws-uninstall-webapp` lists no security or configuration global at all. The only IRISSYS writes are code globals and bookkeeping.
- **Impact:**
  - The very writer that condition 2 named (web-app removal, the F-UNIN-1 class) cannot appear in the write-set obligations.
  - The gate's "exhaustive by construction" claim and its `PASS` are false for every command that creates or deletes a web app, CSP app, role or user. That covers install and uninstall of any module with one, blocked uninstall and `clean`.
  - Had the probe seen `^SECURITY`, the gate would have failed with "unclassified global". There is also no invariant (for example an A6 frame over security definitions) to read it.
  - T16's description of the probe is wrong.
- **Fix:**
  1. Add IRISSECURITY to `Databases`. Check IRISLOCALDATA and IRISAUDIT too.
  2. Classify `^SECURITY`, and add an invariant for it (for example, an uninstall changes only the security definitions its module owns).
  3. Remeasure:
     - install and uninstall of `rest-app`;
     - a blocked uninstall of a module with a web app (F-UNIN-1);
     - `<module> clean`.
  4. Correct T16.

### N2. [major] update_steps: "worked ⇒ OK" rests on a ghost that the L2 writes itself; INST-B80's P is not supported

- **Where:**
  - `models/verus/update_steps.rs`:
    - `fault` (L90, L336) is set only inside `step_phases` (L221) and `step_walk` (L252);
    - `end_ok`, `end_failed` and `honest_end` (L365-L401) compare `out` with `fault`.
  - `models/explorer/inst/twin_update.py` (L148, L175) does the same.
  - `ontology/03-install-update.md` L115, INST-B80 ("report an error only when a guard refuses or a phase or step fails. Otherwise they report success"), cites `P:inst_refines` and `M:inst-explore-refinement-under-premises`.
- **What is wrong:**
  - The L1 never ties `fault` to the environment's choice: the `ok` argument of `Choice::Phases`/`Choice::Walk`.
  - A transcription that reports Failed for a run that worked also writes `fault: true`, and then refines. That is shown by U1 (developer-mode install or load) and U2c (seeding install, rolled back).
  - Only a concrete witness notices. That is the "killed elsewhere" pattern of the first review's mutant 5.
  - The builders' controls (`dev-success-reported-failed`, `walk-end-reported-failed`) leave the ghost unset, so they cannot reveal this.
  - load_general avoids the problem: its L1 reads `cfg.act_ok`, `cfg.outs` and `cfg.refuse`, which no step can change.
- **Fix (one of):**
  - Make the environment's choices a constant of the run, as load_general does (a sequence of phase and step outcomes in `Op` or the L1), with `fault` defined from it.
  - Or compute `fault` in the dispatcher `step` from the `Choice`, so that no step function writes it. Then add U1 and U2c as controls, expected in `inst_step_phases` and `inst_walk_end`.
  - Until then, drop P and M from INST-B80. Also correct the INST-B64 Notes ("reports Failed only because a step or phase threw: INST-B80") and the header of `update_steps.rs` (L16-L19).

### N3. [major] 3.3a verdicts still say "preserved" without the evidence the verdict key requires, and the gate does not check them

- **Where:** `semantics/coverage.json` and `scripts/semantics_gate.py`. The verdict key in COVERAGE.md L7 says *preserved* means "a proof or passing model check covers it".
- **Instances:**
  - **`INV-DEP × uninstall-force-needed`** (`uninstall up-dep -force` after loading `up-parent-ok`):
    - The verdict reads "preserved under PG, except -force, which removes a needed module by design (DEC)".
    - The measured command *is* the `-force` exception, and no DEC id is given.
    - The cited `unin_refines_dependents_protect` is about `!force` (`l1_dependents_protect`, `uninstall.rs` L378-L380).
    - Axiom A11 ("after any command, every installed module's requirements are satisfied") is not qualified for `-force` (UNIN-B35, B40, B65).
  - **`INV-MOD × compile`:** "preserved" with only `O:ws-compile` and `T:...Sync`.
  - **`INV-SYNC × uninstall`, `-2`, `-force-needed`, `-recursive` and `-webapp`:** "preserved" with only O and T.
  - **`INV-SYNC` × `install-from-registry`, `install-up-dep`, `install-force-with-steps`, the `load-*` runs, `package`, `publish`, `reinstall`, `update-dev` and `update-with-steps`:** these cite `P:sync_lemma_stamp_frame`. That lemma is a frame fact about `stamp_all` (`sync_baseline.rs`), not about these commands. Their real evidence is observation.
  - **`INV-MOD` × `load-ok`, `install-from-registry`, `load-same-version`, `load-with-dependency`, `update-with-path` and `load-refused-downgrade`:** "preserved ... compiled code follows the record (observed)". INV-MOD is "record and compiled code agree", and the cited `g_refines` has no code component (finding 6). This is the same pattern the first review flagged.
  - **`INV-STEPS × update-with-steps`:** "preserved".
    - The invariant says each step "runs once".
    - INST-B67 (`O!:nc-inst-4b-nondev-retry`, `fileLines=2`) and DEC-INST-5 show that a failed update followed by a retry reruns steps that worked.
    - No failed update with steps was measured, so the failure path of this pair is missing.
  - **`INV-HIST × uninstall-filecopy`, `-force-needed` and `-webapp`:** the verdict text names "PB, PW". The cited `unin_refines_rows_honest` requires PB, PG, PNF and PR, and not PW.
- **Fix:**
  - Make the verdict a field from a fixed set (preserved, finding, guarded, by-design, observed, refusal), checked by the gate.
  - Require *preserved* to cite at least one P or M whose lemma is about that command. A per-entry `covers` list is enough.
  - Require *by-design* to cite a DEC.
  - Correct the entries above.
  - Qualify A11 for `-force`, with a DEC.
  - Measure a failed update with steps.

### N4. [major] RES-B11 is P-tagged, but the scopes model contradicts half the statement and its own Notes

- **Where:**
  - `ontology/08-resources.md` L48, RES-B11 ("A resource marked Generated is never loaded or exported by IPM"; Notes: "It is still compiled"), cites `P:res_present`.
  - `models/verus/scopes.rs`: `on_phase` (L45-L53), `exported` (L71-L73), `l1_present` and `l1_exported` (L79-L85).
- **What is wrong:**
  - **Export.** The model's `exported` ignores `generated`, so a generated unscoped resource is exported. The L1 `l1_exported` agrees. The code skips generated resources when exporting (`ipm/src/cls/IPM/Lifecycle/Base.cls` L2047). My S1 makes the L1 say "never exported", and `res_export` then fails. So "never exported" is not proved, and the model states the opposite.
  - **Compile.** The model compiles a generated resource only if `st1.loaded`, and generated resources are never loaded. So `res_present` proves that generated resources are *never compiled*. The code compiles them as usual (`ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls` L195-L201: "Generated and preloaded resources are never loaded there, so they compile here as usual"). RES-B11's Notes say the same. S2 (compile them as the code does) breaks `res_present_one`.
  - So the L2 transcription is wrong, and the L1 bakes the error in. The tag counts a falsehood about compile as proved. The controls missed it because no control or witness covers a generated resource's compile or export.
- **Fix:**
  - Separate `loaded` from `compiled` in the L1.
  - Transcribe the Compile branch for generated resources, and the export skip.
  - Re-prove, add a control for each, and only then keep P on RES-B11 (and on RES-B1 to B4, which share `l1_present`).

### N5. [minor] The index gate's premise check can be bypassed, has label collisions, and has no self-test

- **Where:** `scripts/ontology_index.py` `lemma_premises` (L30-L42), the per-tag check (L273-L280), and `self_test`.
- **Evidence:**
  - On a scratch copy I added:
    - `P:g_step_refines` to HIST-B4 (its premises come through the wrapper `premises(s,c,strict)`);
    - `P:g_rollback_crash_honest` to HIST-B23 (its premises are inline: `caller == NoCaller`, `pc != Window`, `!(Dev && Deps)`, `clean`).
  - Neither tag raised a violation, although neither row's Notes names any premise. The same probe's P+F injection and missing-premise injection were caught.
  - Today only HIST-B10 (`g_rollback_crash_honest`) and INST-B69 (PL inline) rely on inline premises, and their Notes happen to name them.
  - Labels collide across files: PI (load_general and uninstall), PR (dep_resolve and uninstall), PU (dep_resolve and settings), PE. A Note naming one file's PR satisfies another file's PR.
  - `--self-test` covers only the parent, grade, definition-order and cell-missing rules. The P+F, premise and O! rules have no self-test (§5 "Self-tests for every structural check").
  - `semantics_gate.py` has no self-test at all.
- **Fix:**
  - Expand wrappers, and declare inline premises as named spec functions.
  - Qualify labels by file (`load_general:PI`).
  - Add self-test cases for each new rule, and a self-test for `semantics_gate.py`.

### N6. [minor] T-skip evidence is coarse and not enforced

- **Where:** `scripts/collect_evidence.py` L90-L107, and the INDEX section "T tags on test runs with skipped assertions".
- **Evidence:**
  - Skips are the per-class maximum over the containers that still exist.
  - 61 of 113 records are "unknown (run's container removed)".
  - A method-level T (`Class#Method`) inherits its class's count.
  - The gate lists such tags but accepts them.
  - HIST-B25 and LOAD-B15 still over-claim, as described in A8.
- **Fix:**
  - Record per-method assert, skip and fail counts at run time.
  - Make the gate refuse a T tag with unknown or non-zero skips, unless the Notes justify it.
  - Narrow HIST-B25, and add a refusal observation for `history delete` without `-confirm`.
  - Add caller writes to the LOAD-B15 probe, or drop its second sentence.

### N7. [minor] P proofs of current behaviour under an open decision are counted as proved, and some cells point at OPEN rows with no Decision

- **Rows:** INST-B18 (DEC-INST-2), INST-B30 (DEC-INST-1), INST-B53 (DEC-INST-8), SYNC-B22 (DEC-SYNC-1) and LOAD-N1 (DEC-HIST-4). Each carries P together with OPEN, and INDEX lists them under "Proved statements". The L1 of update_steps encodes INST-B18's behaviour (`end_ok`, developer-mode branch: records unchanged) while its intent is undecided.
- **Cells:** HIST-C11, C12, C19, C26, C33 and C40 name HIST-N1 or HIST-N2 (OPEN:DEC-HIST-4, DEC-LOAD-1) with an empty Decision.
- **Fix:**
  - Render P+OPEN separately in INDEX ("proved: current behaviour, intent open").
  - Put the DEC in those cells.

### N8. [minor] Stale references to the retired model, and missing TRUST files

- **Stale references:**
  - LOAD-B8 Notes still cite `lemma_refines_txn_mode`.
  - HIST-B11 Notes cite `witness_dev_crash_commit_flag`.
  - `findings/01-02.md` L38 cites `lemma_refines`.
  - All three are `load_history.rs` lemmas.
  - The root `TRUST.md` header and T1 still describe `load_history.rs` as the Verus L2.
  - T16 overstates the probe (N1).
- **Missing TRUST files:** `dep_resolve.rs` and `scopes.rs` have none. The dependency premises PU and PR are still documented in `update_steps.TRUST.md` D6 and D7 (cited by DEP-B21).
- **Fix:** update these, and regenerate `TRUST.md` with STATUS.

### N9. [minor] Unconstrained L1 fields in load_general

- **Evidence:** `Row.named` and a dependency row's `ended` are not constrained by `honest`. LG1 and LG2 survive.
- **Current impact:** no P statement relies on them today: HIST-B7 is F, and HIST-B9 and HIST-B18 have no P.
- **Fix:** record this in `load_general.TRUST.md`, so that a later P tag on those clauses is not added without strengthening the L1.

### N10. [minor] PM is not named where it matters

- **Evidence:** G7's premise PM (nothing fails after the final commit) is a premise of LOAD-B8's "nothing it did stays" and of HIST-B12 and HIST-B13. Their Notes do not name it, and the gate cannot see it because it is not a spec function.
- **Fix:** declare it as a spec function, as a no-op premise, or add it to those Notes.

### N11. [minor] A seed-doc contract was excluded as out of scope

- **Evidence:** `docs/ipm-uplift/index.md` L49 states that KeepOnUninstall files survive "clean or uninstall". `ontology/OUT-OF-SCOPE.md` excludes the clean half with "Specified for uninstall only", which is not a reason.
- **Fix:** specify it (the `FileCopy.cls` L227 cleanup branch is shared), or make it an open DEC.

### N12. [minor] No ledger maps review findings to fixes or decisions

- **Evidence:** §3.8 requires that "every finding becomes a fix or a recorded decision". Today the responses are scattered across revision notes in `findings/` and `decisions/`.
- **Fix:** STATUS.md should carry one table, with one row per finding of both reviews and its fix or DEC.

## C. Confirmed sound

- **load_general.rs.**
  - The strengthened `honest` is two-way, from configuration inputs that L2 steps cannot change.
  - The first review's mutants 2 and 4 are now killed (controls).
  - My LG1 and LG2 survive only on fields that no P claim uses.
  - Baseline: 39 verified.
- **uninstall.rs.** The two-way `l1_honest` (`ok <==> removed`) compares the outcome with observable state. UN1b is killed in `unin_refines_honest`.
- **sync_baseline.rs.** `l1_outcome` takes the environment's outcomes (`ok`, `tests_ok`) as step arguments, and the reported value is a function of them. That is sound for both directions.
- **dep_resolve.rs.** The Failed ghost `why` is checked by the L1 against the graph, so a fabricated reason must still be a real loop or a real gap. Control `exact-requirement-fails` is killed in place.
- **update_steps.rs (finding 7).** The developer mode is the typed flag or the stored mode; the guard uses the parameter only; seeding uses the record's mode. Each has a control killed in place.
- **Gates.**
  - P+F is rejected.
  - Missing premises are caught for every lemma whose premises are declared spec functions in its `requires` (tested).
  - Failed write-set runs need a label.
  - Both gates pass `--check`.
  - The full rerun is fail=0, with no source newer than its start.
  - All 193 controls are killed with numeric verification counts, so none was "killed" by a compile error.
- **P tags I sampled and found supported by the cited lemma, with premises named:**
  - LOAD-B7, B8 (apart from N8 and N10), B9 and B19;
  - HIST-B2, B3, B8, B10, B13, B14 and B20 (`g_refines`, two-way L1);
  - INST-B14, B26, B49, B64 and B69;
  - UNIN-B9 (Notes admit "the transaction itself is one model step"), UNIN-B11 and UNIN-B80;
  - DEP-B21 and DEP-B54;
  - SYNC-B1;
  - NSP-B44 (INFO, RTN);
  - REPO-B105.
- **P tags I sampled and found not supported:** INST-B80 (N2) and RES-B11 (N4).
- **O, T and O! tags I sampled against their raw `values` and found to support their statements:**
  - UNIN-B46, `nc-repo-fix-unin-all-order` (`printedBBeforeA=1`);
  - NSP-B3, `nc-nsp-fix-B3-fresh-install-many-ns`;
  - RES-B25, `O!:nc-gap-P8-deployed-export-forced-error` (`shipsMarkerSourceFile=1`, refuting, with F);
  - HIST-B2, `ipm-halt-txn` (record kept, `ended:0`);
  - UNIN-B80, `nc-unin-3-uninstall-plain` and `nc-unin-8-dependency-hook-fails`;
  - INST-B27, `nc-inst-fix-stored-dev-load-newer`;
  - INST-B18, `nc-inst-fix-stored-dev-plain-load`;
  - LOAD-B21, `ipm-dev-then-plain-load`;
  - HIST-B1, `O!:nc-gap-P2-txn-by-package-after-release` (no row left, with F);
  - INST-B66 and B67, `nc-inst-4b-nondev-retry` and `nc-inst-4-nondev-failed-update`;
  - SYNC-B17, B19 and B20, `nc-sync-N22-retry-mark-after-compile`;
  - UNIN-B18 and B19, `nc-unin-7-control-keep-module`;
  - INST-B24, `nc-dep-12-permit-downgrade`;
  - INST-B65, `nc-inst-5-failed-step-update`;
  - RES-B7, `T:Scopes`: the one skip is explained in Notes.
- **O and T tags I sampled and found not supporting their statement:** HIST-B25 (`-confirm` half) and LOAD-B15 (second sentence).
- **Measurement.** The write-set measurement is sound for the databases it scans, which are IRISTEMP, IRISSYS and the namespace databases. It is not sound for security data (N1).

## D. Sign-off verdict for the §3.8 gate: **REFUSE**

**Reasons.** Most of the first review is fixed, and fixed well. load_general's L1 is now two-way and cannot be bent by the L2. P+F is mechanically refused. Premises are declared and checked, and the stored developer mode is modelled. Three things still keep the account from saying what it claims:

1. **N1 (blocker).**
   - The 3.3a gate reports PASS while the write-set probe cannot see IRISSECURITY.
   - The web-app uninstall that condition 2 required was measured, but its defining write is invisible.
   - Every command that creates or deletes a web app, CSP app, role or user escapes the obligations.
2. **N2 (major, condition 1).** update_steps' "worked ⇒ OK" depends on a ghost the L2 writes itself. Two mutants that report failure for runs that worked pass every refinement lemma, and INST-B80 is tagged P on that basis.
3. **N3 and N4 (major, condition 2 and P honesty).**
   - Several 3.3a verdicts say "preserved" for pairs that their evidence does not cover. This includes the `-force` exception and the "runs once" step invariant.
   - RES-B11 is tagged P on a model that states the opposite of half the statement.

**To reach SIGN WITH CONDITIONS:**

- **Fix N1:**
  - scan IRISSECURITY;
  - classify `^SECURITY`;
  - add an invariant;
  - remeasure the web-app install, uninstall, blocked uninstall and clean;
  - rerun the gate.
- **Fix N2:** derive `fault` from the environment's choices, with U1 and U2c as controls killed in their step lemmas. Or drop P and M from INST-B80 and correct the update_steps header and the INST-B64 Notes.
- **Fix N3:**
  - make the gate check the verdict vocabulary and the evidence each verdict needs;
  - correct the listed entries;
  - add a DEC and an A11 qualification for `-force`;
  - measure a failed update with steps.
- **Fix N4:** correct the scopes transcription and L1 for generated resources, or drop P from RES-B11 (and re-check RES-B1 to B4).

N5–N12 may be recorded as decisions or follow-ups.

**What STATUS.md, TEST-REFLECTION.md and README.md must say when regenerated:**

- **Rerun scope.** The full rerun replays recorded IRIS logs; it reruns no IRIS probe or IPM test.
- **Probe coverage.** The write-set probe covers the databases it scans, and the gaps of N1.
- **Statement and P counts.** 710 statements; 108 with P (after P+F was removed).
- **Compiled code.** It is observed only (LOAD-B30).
- **INST-B80's basis** (N2).
- **Unknown skips.** 61 of 113 test records have unknown skip counts.
- **Rows depending on unchecked premises.** PM (G7), PF5 and PL.
- **Review ledger.** One row per finding of both reviews (N12).
- **No stale content.** Nothing that cites `load_history.rs` or "compiled classes unchanged" at level P.
