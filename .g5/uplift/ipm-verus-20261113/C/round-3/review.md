# Stage C round 3 review: final confirmation

Independent reviewer for round 3, the last round. I wrote none of the reviewed artifacts and took no part in rounds 1 or 2. Product `571e04a19aba9f9b4bc0ea23fbe09182038f4e55` (no product file differs from it at `HEAD` `9b9cd72c`). `VV` = `.g5/Verus_verification`. Scope: package loading, transaction history and the handoff.

My writes:

- This file.
- `VV/results/verus/20261113T220410Z-review-r3-asbuilt`, written by `receipt.py`, which also appended its line to `VV/results/index.jsonl`.
- `VV/results/verus/20261113T222035Z-review-r3-mutants`, written by hand: `model.diff`, the harness `rev3.rs`, raw logs, the regenerated-output hashes, the replay re-evaluation counts and `receipt.json`. It is not indexed in `index.jsonl`.

Everything else ran in scratch copies outside the repository (`/tmp/r3-model`, `/tmp/r3-clean`, `/tmp/r3-review`). I ran no IRIS session and touched no container. Verus ran one process at a time with `--num-threads 4`.

Order of work, stated plainly because it limits independence: the brief, the plan, the handoff draft and the round 2 review and dispositions came first, because the task's first question is about them. Then the methodology, then the sources (`Main.cls` `XData Commands`, wiki/00 and wiki/09 at `781e98fd`, `Utils/Module.cls` `LoadModuleReference`, `LoadNewModule` and `LoadDependencies`, `AbstractHistory`, `HistoryTemp`, `History`, `Storage/Module.cls` `ExecutePhases`). I wrote section 1 from those before reading the models, bindings and evidence. The B reports came last.

## 1. Expectations derived from the sources

### Load

- E-L1. A failed normal-mode load keeps no module data. One transaction covers the manifest, the dependencies and the phases (wiki/00 L125, `Utils/Module.cls:1195`, rollback loop `:1358-1362`).
- E-L2. A developer-mode load commits in steps, and what each step committed stays after a later failure:
  - The manifest is committed first (`:1309-1314`).
  - Dependencies and the manifest reload run in a second transaction, committed at `:1335-1339`.
  - The phases run outside any transaction, so a failure in Activate keeps what Reload and Compile did (wiki/00 L126, `-dev` help "transactions will not be rolled back").
- E-L3. With `-DNoTransaction=1` or `-DNoJournal=1` IPM opens no transaction, so a failure keeps everything written before it (wiki/00 L145-155). With NoJournal even an outer transaction cannot undo the writes.
- E-L4. The downgrade and update-required guards run before any module write in every mode (`:1213-1223`). The requirements check runs after the developer-mode first commit (`:1321`), against wiki/03's "validated before module installation".
- E-L5. A module's `<Defaults>` are merged under the caller's parameters for every `LoadNewModule`, including a dependency's (`:1166-1169`, wiki/00 L175). Two consequences:
  - A dependency whose `<Defaults>` set `NoJournal` writes unjournaled inside its parent's transaction, so the parent's rollback cannot undo it.
  - A dependency whose `<Defaults>` set only `NoTransaction` skips its own `tstart` and sets its no-transaction marker, but its journaled writes stay inside the parent's open transaction and are undone with it.
- E-L6. Under a caller-held transaction every IPM `tcommit` is nested, so the caller decides.

### History

- E-H1. A top-level action's durable row is inserted at `Init` (`HistoryTemp.cls:21`) and completed by `%OnClose` (`:62-73`). Both happen at the caller's `$TLEVEL`, so outside a caller transaction the row survives the load's own rollback (#366, wiki/09).
- E-H2. A dependency's log is created and closed inside the parent's transaction (`LoadModuleReference` `:65-67`, `:207-209`), so the parent's rollback removes its row. That is against #366 ("including as a dependency").
- E-H3. `Committed` lives in IRISTEMP until close. A killed process leaves the durable row at `Committed=0` with no phases, even when changes were kept.
- E-H4. Finalized rows are locked against UPDATE and object save by the trigger, and DELETE stays possible (wiki/09 L197).
- E-H5. A phase whose before-phase or after-phase Invoke returns an error is left open, because `quit:$$$ISERR` leaves the try block and the later `$$$ThrowOnError` skips `FinalizePhase` (`Storage/Module.cls:608-610`, `:669-671`, `:704`).

Every expectation is represented in the ontology (`load.outcome`, `load.outcome.dev`, `transaction.opt-out`, `transaction.flag-sources`, `history.*`). Two are only partly carried into the artifacts:
- E-L3 is an I statement (`load.outcome` table, `transaction.opt-out`), but the L1 contract leaves it unconstrained (R3-01).
- E-L5's NoTransaction half is not distinguished in Q-22 (R3-03).

## 2. Round 2 repairs, checked in their owning artifacts

| Item | Owning artifact checked | Against source and code | Verdict |
|---|---|---|---|
| R2-01 | `contract.rs` `l1_alts`: a transactional developer-mode failure at verdict 7 or 8 now requires the module installed at its version with resources loaded, plus the needed dependencies installed (`three(mx(1, ver, 1), dep_installed(0), dep_installed(1))`). `model.rs` control 12 (`lnm_b2` pc 13, `lnm_c` pc 29). `cases.rs` `case_c26_dev_failure_keeps_phase_work`, placed first. `cases.json` B1-C26 and C38 now read "installed". Coverage map row `load.outcome.dev` cites controls 01, 04 and 12 | Matches E-L2: phases run after C2 with no open IPM level, and the dependency commit precedes them. Verdict 7 is an Activate fault only, which is the only phase fault the model has | Present and correct. Control 12 is detected by L1-NAMESPACE (`20261113T215122Z-control-12-…`, counterexample B1-C26 shape) and by Verus (`…215819Z-final-control-12-verus`) |
| R2-02 | `bugs_issues.md` D1 and D10, `design_simplification.md` S2, `premises.md` R-01, `B/native-replay.md` section 6, raw log `20261113T214135Z-premise-p20-r01` | The write mechanism (journaling off around the History save) is named. N-P20 A, AN, B, C, D and E are CONFIRMED in `verdict.json`, with expectations snapshotted before the run (`written_before_run: true`) and four mutation controls detected. C2 is REFUTED and disclosed as a caveat | Write mechanism present and correct. Two residuals: `BINDINGS.md` "Repair variant 103" and `B/model-notes.md` sections 5 and 8 still say R-01 "is being checked natively", so B1 did not cite the verdict (R3-04). The `Committed` policy in D1, D10 and S2 differs from the one variant 103 checks and fails at process ends (R3-02) |
| R2-03 | Ontology Q `transaction.flag-sources.dependency-defaults` (Q-22), cross-linked from `transaction.flag-sources` and `load.outcome`. D14 marked decision pending with both options. Handoff decision table row Q-22 | Sources cited correctly (wiki/00 L147, L149, L155, L175; `Utils/Module.cls:1033-1040`) | Present. The question mixes NoTransaction with NoJournal, and only NoJournal can escape a parent's rollback (R3-03, minor) |
| R2-04 | Metadata of `load.*`, `history.*`, `update.refusal`, `transaction.flag-sources`, `settings.default-modifiers` now cite phase-2 and stage C runs and D10-D20, I1, I2. Load and history pages link `VV/coverage-map.md`. Checker: 18 files, 148 blocks, 0 issues | Run IDs exist | Present. Residual: `transaction` metadata still says "Formal evidence is linked at handoff", and `A/coverage-map.md` still says "Formal (B1) evidence is still pending" (R3-06, minor) |
| R2-05 | `BINDINGS.md` abstractions and `model-notes.md` section 9 cite M5, M5N and D14, and keep the "not representable" limit | Matches the stage C log | Present. They name the decision "open question R2-03" instead of Q-22 (part of R3-06) |
| R2-06 | D12 prefers S1, explains that moving the marker breaks `theorem_commit_sound` for B1-C10 until D11 is fixed, flags the change to the D block `history.committed.marking`, and adds the preserve items | Consistent with PR #1322 and the model | Present and correct |
| R2-07 | D6 now covers any phase whose before-phase or after-phase Invoke returns an error | `Storage/Module.cls:608-610`, `:669-671`, `:704`, `:710` confirm it | Present and correct |
| R2-08 | `VV/coverage-map.md` `load.path` row marks git URL loads unestablished. OBS-L13 heads the suspected list, tied to Q-04. Handoff lists git URL loads as not run | `Main.cls:2351-2372` | Present |
| R2-09 | README: prerequisites (Verus dir, rustup toolchain, offline crates, BuildKit), the `index.jsonl` append, inputs-digest note, D2 final receipt and its state dependence | | Present |
| R2-10 | `receipt.py` has kind `repair`. Annotation lines exist for control 05 and 05b and for the repair-103 runs | | Present |
| R2-11 | Handoff: the refusal claim says "proved for the listed cases, explored exhaustively". The developer-mode claim cites the L1 clause and control 212 | I checked the "exhaustive" wording against the full survey (section 3). No downgrade or update-required configuration violates L1-NAMESPACE at any end, so the conforming-domain filter excluded none of them | Present and accurate |

## 3. Evidence currency

- **As-built proof rerun.** `20261113T220410Z-review-r3-asbuilt`: `verified 260, errors 0`, exit 0. Its `inputs.sha256` equals the current files and equals `20261113T214349Z-asbuilt-proofs-r2` on every line, except that `crates/scripts/final-candidate-batch.sh` was added after 214349Z. The script is not a proof input. The crate has no `assume`, `admit`, `external_body` or `#[verifier::external]`.
- **Final-candidate receipts `20261113T215113Z`..`215941Z` (41 runs).** Every `inputs.sha256` matches the current sources except for the batch script itself, which was committed after the batch. Observed and expected verdicts:
  - As-built explorer passes: invariants per mode, R2 with foreign edits, both conforming-domain L1 runs.
  - As-built explorer violations: R3-dep, L1-END, COMMIT-EXACT, dependency clauses.
  - Controls 01-04, 04b and 06-12 are `violation`, as expected. 05, 05b and 05c pass as expected, and so do diagnostic 11b and both repair-103 runs.
  - Verus controls 01-04 and 06-12 fail with real verification errors in the named lemmas, exit 101. Control 05 verifies 260/0 as the expected survivor. `sanity-false` fails exactly `sanity_false_claim`.
  - No timeout, INCOMPLETE or truncated exploration.
- **`cases.json`.** sha256 `d3334b16…476cd97`. Regenerated from a clean scratch copy with `b1 cases`, it is byte-identical. `b1 survey` regenerates `20261113T214445Z-survey-asbuilt-r2/survey.json` byte-identically: 5,850 configurations and 375,912 end states.
- **Replay re-evaluations.** I re-ran `evaluate_replay.py --cases VV/cases/cases.json` on copies of each replay run. The four stored `verdict-cases-d3334b16.json` files (return-b, terminate-b, sigkill-b, c51-c52) are reproduced exactly, apart from the timestamp and path.
  - Totals: 57 realized cases, all AGREE.
  - Product fields: 1,161 AGREE and 0 disagree, plus 57 binding fields and 76 N/A.
  - Contract verdicts: 23 PERMITTED and 34 VIOLATES.
  - B1-C26 and C38, the only cases changed from `05842975`, are PERMITTED with resources loaded.
  - The approximate runs (`204430Z`, `205218Z`) have no d3334b16 file. They need none, because C49T/K are unchanged between the two `cases.json` versions and my re-evaluation gives the same APPROXIMATE verdicts.
- **Citations.** Every run ID cited in `VV/coverage-map.md`, `handoff.md`, `bugs_issues.md`, `README.md`, `premises.md` and `design_simplification.md` exists. Three pointers are wrong or stale (R3-06):
  - The coverage map says the explorer reruns are "after 21:55 UTC". They are at 21:51:13-21:51:26.
  - `model-notes.md` section 6 still names `214455Z`-`214500Z` and `204430Z`-`204957Z` as the final control receipts.
  - `premises.md` still lists P-06c as UNVERIFIED and P-10 as PARTIAL, while `B/native-replay.md` section 6 confirms both (N-P19, and B1-C25, C31, C32, C36 and C44K/T). The coverage map cites `premises.md` for "P-01..P-11 CONFIRMED" (R3-05).

## 4. Handoff check

Against `Verus/setup_foundation.md` and `Verus/uplift.md` section C:

| Required | In `handoff.md` | Assessment |
|---|---|---|
| Established claims, scoped | Section "Established claims": domain, caller transactions excluded, theorems named, non-theorem claims say what they rest on, SIGKILL cleanup window | Accurate, except that "These rest on IRIS premises P-01..P-11, which were confirmed natively" relies on `B/native-replay.md`, while `premises.md` disagrees (R3-05). `theorem_r1` ("never removes a durable history record") is correct, because a dependency row written inside the parent's transaction is never durable. A reader comparing it with D1 would need that sentence (R3-07) |
| Conditional claims | Q-dependent verdicts are in the decision table. The repair variant is conditional on R-01, now confirmed | Present. The repair claim "meets every dependency clause, including at process ends" holds for variant 103, not for the D1 proposal text (R3-02) |
| Product defects | D1-D6, D9-D20, I1, I2, withdrawn D7, decision-pending marks | Present. Wording slips are R3-07: "Each has a governing source" includes I1 and I2, which have none by definition, and D13 is grouped under wrong history content although its obligation is status against namespace |
| Human decisions with provisional answers | Table of Q-17, Q-22, Q-05/Q-06b, Q-08, Q-21, Q-14, Q-18, Q-02, Q-03 and the rest | Present. The catch-all row lists Q-12, which is closed as a D block (R3-07). Q-22's provisional answer is ambiguous for NoTransaction (R3-03) |
| Incomplete work | Unmodeled areas, native checks not run, approximate cases, suspected list | Present. Not listed: the domain excludes a dependency already installed at another version and a developer-mode dependency (BINDINGS discloses only "a newer version"), and M5's NoTransaction variant was not run (R3-08) |
| Missing evidence | Two usage-limit stops, transcript gap, void runs, superseded runs, expectation provenance, final-candidate reruns | Present and consistent with `index.jsonl` |
| Runbook reproduction of a result and its control | Round 2 reproduced it cold | Present. Step 2's expectation was updated to `case_c26` and matches `215127Z` |

### Claims resting on bounded or native evidence

The handoff says so for the refusal claim (explorer) and the developer-mode claim (explorer and case lemmas). `VV/coverage-map.md` cites "Explorer L1 runs over the conforming domain pass" as formal support for `load`. That pass holds by construction, since the domain is the set where the as-built passes. The non-circular evidence is the survey, which shows no L1-STATUS or L1-NAMESPACE violation for success at any end. The claim is true, but the cited evidence is the wrong one (minor, part of R3-06).

The coverage map marks `transaction.opt-out` and the NoTransaction/NoJournal row of `load.outcome` "supported". Their formal support is the as-built depth invariant, and no L1 clause covers them (R3-01).

### Ontology against the first draft (`git diff 7ff5df56 -- .g5/ontology`)

- No block was removed. No block changed parent or page.
- One kind change: `history.delete.global-unfiltered`, Q to D, supported by wiki/09 L195.
- Three blocks were added: Q-20, Q-21 and Q-22.
- Changed provisional answers keep the old answer as their alternative for Q-08 and Q-09. Q-15 (`history.fields.dependency-source`) lost two of its draft options: "show their source in the listing" and the alternative "clear the source of records labeled `load`" (R3-09).
- The dependency half of `history.committed.nested` and the survival half of `history.dependencies.parent-rollback` became I on #366. That is intent, not code.
- `deps.conflict` changed from "refuses" to "fails the load". This follows the maintainer comment at `Utils/Module.cls:1316-1317`, as adjudicated in the reconstruction dispositions.
- The normal-mode row of `load.outcome` changed from "Nothing in the database" to "No module data". That is consistent with surviving history rows.

No other condition, exception or uncertainty was lost.

## 5. New attempt: faulty implementations not tried in rounds 1 and 2

Rounds 1 and 2 tried variants 201-206 and 211-214. I built three variants in a scratch copy (`model.diff` in the mutants receipt) and a harness, `rev3.rs`. For each configuration, the harness compares the base and the variant at three kinds of end:
- the returned end, aligned;
- the caller-commit and caller-rollback ends, aligned;
- the set of clauses violated at any kill-like or terminate-like end.

It also counts DEP-REPAIR violations over every end state. Sanity checks: `rev3 0 0` and `rev3 103 103` show no difference, and variant 103 has 0 DEP-REPAIR violations in default mode, as its receipt says.

| Variant | Kind | Change | Result |
|---|---|---|---|
| 15 | faulty implementation | `-DNoTransaction=1` ignored: the load runs as in default mode, with `tstart`, no no-transaction marker and default-mode commits | NoTransaction domain, 1,950 configurations, 1,650 behave differently. L1-NAMESPACE newly violated at 0 end states. The only new violations are `H-RECORD-EXISTS[dep1/dep2]`, which is the existing D1 loss now happening in NoTransaction mode. Configurations without dependencies are never detected. The variant even removes the D11 and D12 violations. Verus on the scratch copy (`verus-control15.log`) fails only `case_c08`, an as-built observation lemma, and the transaction-depth lemmas `lemma_lnm0_a/b/d` and `lemma_lnm1_*`. No contract theorem fails |
| 17 | faulty implementation on the repaired baseline | Variant 103 (the modeled D1 repair) plus variant 15 | Against 103: 1,650 configurations behave differently, and 0 L1 clauses are newly violated at any end, including process ends. Once D1 is repaired as modeled, no L1 clause detects a product that ignores `-DNoTransaction=1` (R3-01) |
| 16 | faulty implementation of a proposed repair | The D1 repair as `bugs_issues.md` D1 and `design_simplification.md` S2 word it. Dependency rows are written outside the transaction, as in 103. `Committed` comes from the dependency's own log and is set back to 0 only on the parent's rollback path | Against 103 in default mode, 352 configurations behave differently. `H-COMMITTED[dep1/dep2]` is newly violated at process ends in 176 configurations, and DEP-REPAIR end states go from 0 to 10,718. Returned ends are unchanged. Example: normal mode, one dependency, SIGKILL in the parent after the dependency finished. The dependency row survives with `Success` OK and `Committed=1`, while IRIS's rollback leaves the dependency uninstalled (R3-02) |

Valid completion:
- The unmodified crate verifies 260 obligations with 0 errors (section 3).
- The default-mode load with one dependency and no fault violates no clause under variants 0, 15, 16 and 17.

## 6. Findings

Material: R3-01 and R3-02. Minor: R3-03 to R3-09.

### R3-01 (model, material): L1 leaves the namespace unconstrained after a NoTransaction or NoJournal failure, so a product that ignores `-DNoTransaction=1` passes every contract clause
- Kind: model error (contract gap)
- Source or owner:
  - `load.outcome` (I), table row "`-DNoTransaction=1` or `-DNoJournal=1` | Everything written before the failure".
  - `transaction.opt-out` (I): "`-DNoTransaction=1` runs without a transaction, so a failure leaves a partial install".
  - wiki/00 L145-155.
  - Owner: `VV/crates/ipm-load-history/src/contract.rs` `l1_alts` (`else if !tx { any_alt() }`) and `l1_alts_kill` (`c.txm != 0` gives `any_alt()`). The coverage-map rows `load.outcome` and `transaction.opt-out` mark both "supported".
- Trigger: variant 15 (NoTransaction ignored) and variant 17 (the same on top of the D1 repair 103). Evidence is in `results/verus/20261113T222035Z-review-r3-mutants`.
- Discrepancy:
  - L1 permits any module state after a NoTransaction or NoJournal failure. `model-notes.md` section 2 calls this "partial changes allowed", which is weaker than the ontology's "everything written before the failure" stays.
  - Under variant 15, no new L1-NAMESPACE violation appears (some disappear, because D11 goes away). The only detection comes from the dependency-record clauses, through the existing defect D1.
  - Under variant 17, against the repaired baseline, no L1 clause changes at any end.
  - Verus detects variant 15 only through the as-built transaction-depth invariant and the as-built observation lemma `case_c08`, not through a contract theorem.
  - The product is therefore pinned only by the binding, as it was for developer mode before R2-01. A refactor, for example a D1 or D10 repair that touches the transaction structure, could make NoTransaction transactional and still pass L1.
  - Round 2 saw the `any_alt()` branch and expected the `Committed` clause to compensate (its R2-01 limits). Variant 15 shows that it does not, because the variant also drops the no-transaction marker.
- Limits:
  - I did not build the NoJournal analogue. The same `!tx` branch covers it.
  - Natively, the as-built behavior is observed: B3, B4, B1-C08 to C10, S2 and S2B keep partial work. The finding concerns the contract, not the product.
- Proposed repair:
  - B1, in `l1_alts`: for `!tx` at verdict 7 or 8, require the top module installed at its version and the needed dependencies installed, as the developer-mode branch now does. At verdict 6, require the top module present at its version and the dependencies before the failing one installed. At verdict 5, keep the Q-17 branch.
  - Add variant 15 as control 13, which L1-NS-ALL must detect, together with a case lemma such as a NoTransaction Activate failure keeping its resources. Regenerate `cases.json` and re-evaluate the replays, with no IRIS run.
  - Until then, change the two coverage-map rows to "as-built proved (depth invariant) and observed natively, not an L1 clause".

### R3-02 (repair proposal, material): The `Committed` policy in D1, D10 and S2 is not the one variant 103 checks, and as written it records `Committed=1` for a dependency that a process end rolled back
- Kind: model error (repair variant against its proposal) and an incorrect repair proposal
- Source or owner:
  - `history.committed` (I): a dependency undone by its parent's rollback reads uncommitted.
  - L1 `kill_rec`: a completed row's `Committed` equals "changes kept".
  - Owners: `VV/bugs_issues.md` D1 ("the parent's rollback path updates those rows (`Committed=0`)") and D10 ("copy … `Committed` … at each … commit"), `VV/design_simplification.md` S2 ("`Committed` is written at each commit and set back to 0 by the rollback path"), `BINDINGS.md` "Repair variant 103", `model.rs` `persist_row`, `dep_commit_mark`, `handoff.md` incomplete-work bullet on variant 103.
- Trigger: variant 16, a normal-mode `load` with one dependency, killed by SIGKILL or Terminate inside the parent after the dependency finished (B1-C45K/T shape).
- Discrepancy:
  - Under the proposals as written, the dependency's row is persisted unjournaled with the `Committed=1` that its own inner commit and `Finalize` set. A process end runs no in-process rollback path, so IRIS rolls the dependency back while its row keeps `Committed=1`. Variant 16 shows this in 176 configurations and 10,718 end states.
  - Variant 103 checks a different policy: `Committed=0` while an IPM transaction of the load is open, and set to 1 by `dep_commit_mark` in the same model step as the commit that makes the dependency durable. `BINDINGS.md` and `model-notes.md` section 5 describe that policy, but D1, D10 and S2 do not.
  - The handoff's "meets every dependency clause, including at process ends" therefore holds for the model, not for the repair a maintainer would implement.
  - Variant 103's mark is also atomic with `tcommit`. An implementation needs either a window between the commit and the update, which is visible at a process end, or a journaled `Committed=1` update inside the committing transaction. N-P20C confirms the latter is safe: a journaled update of an unjournaled row is undone to the unjournaled value. But D1 and S2 forbid it ("the insert and every update of the row must use the unjournaled path"). N-P20C2 only rules out the opposite order, a journaled insert followed by unjournaled updates.
  - D1's distinguishing checks for B1-C45K/T require only that the row survives, not its `Committed` value.
- Limits:
  - Variant 16 is my reading of D1 and S2. D1's phrase "the parent records … whether its writes were later undone" could be read as variant 103's policy, but its next clause and S2 describe write-then-correct.
  - Not run natively. I did not model the journaled-update implementation.
- Proposed repair:
  - Orchestrator, in D1, D10 and S2: state variant 103's policy. A dependency row is written unjournaled with `Committed=0` while any IPM transaction of the load is open. `Committed=1` is set by a journaled update immediately before the commit that brings the load back to its entry `$TLEVEL` (C2 in developer mode, C3 in normal mode), or at the dependency's close when no IPM transaction is open.
  - Replace "every update must be unjournaled" with the precise rule: never a journaled insert followed by unjournaled updates (N-P20C2), with journaled updates over an unjournaled row allowed (N-P20C).
  - Add "B1-C45K/T: dependency row present with `Committed=0`" to D1's distinguishing checks.
  - B1: cite that mechanism in `BINDINGS.md` as the justification for the atomic `dep_commit_mark`, or split the mark from the commit and disclose the window.

### R3-03 (ontology, minor): Q-22 treats a dependency's `NoTransaction` like `NoJournal`, but only `NoJournal` can escape the parent's rollback
- Kind: unresolved decision (incorrectly framed)
- Source or owner: `transaction.flag-sources.dependency-defaults` (Q-22), `handoff.md` decision row Q-22, `bugs_issues.md` D14. Code `Utils/Module.cls:1166-1169`, `:1191-1196`, `:1226-1228`. N-P01.
- Trigger: a normal-mode parent loads a dependency whose `<Defaults>` set only `NoTransaction=1`, then fails. This is the variant of M5 that was not run.
- Discrepancy:
  - The question asks whether either flag applies "so that the dependency's changes escape the parent's normal-mode rollback", and the provisional answer is "yes".
  - A NoTransaction dependency only skips its own `tstart`. Its writes are journaled inside the parent's open transaction and are undone with it, whatever the answer. It also sets its no-transaction marker, so its row reads `Committed=1` before writes that the parent then rolls back.
  - The handoff's "If accepted, D14 becomes intended behavior" covers only NoJournal. Accepting "yes" for NoTransaction would describe behavior the product cannot have.
- Proposed repair: A3 splits the question. NoJournal is the physical escape, which is D14. For NoTransaction, ask whether a dependency's flag should suppress the parent's transaction. The orchestrator then states the NoTransaction case in the handoff row and lists M5's NoTransaction variant as not run.

### R3-04 (bindings, minor): B1 documents still call premise R-01 pending
- Kind: missing or stale evidence
- Owner: `BINDINGS.md` "Repair variant 103" ("is being checked natively by B2; until then every repair-103 result is conditional on it"), and `B/model-notes.md` section 5 ("is being checked natively by B2") and section 8 ("R-01 is a design premise of the repair variant only").
- Discrepancy: N-P20 confirmed R-01 at 21:50Z. The R2-02 disposition asked B1 to cite that verdict, which `premises.md`, `B/native-replay.md`, D1 and the coverage map already do.
- Proposed repair: cite `20261113T214135Z-premise-p20-r01` and its C2 caveat in both places.

### R3-05 (premises, minor): `premises.md` contradicts the premise mapping that the handoff relies on
- Kind: missing or stale evidence
- Owner: `VV/premises.md` P-06c (UNVERIFIED, "not tested by B2") and P-10 (PARTIAL), and `model-notes.md` section 8. Against them: `B/native-replay.md` section 6 (P-06c CONFIRMED by N-P19, `20261113T204146Z-premise-p19`; P-10 CONFIRMED for class definitions and compiled classes).
- Discrepancy: the coverage map and the handoff say "P-01..P-11 … CONFIRMED natively" and cite `premises.md`, whose table says otherwise. OBS-L15 (`LoadedResource` rows) stays suspected, so P-10's scope note matters.
- Proposed repair: update the two rows and their evidence in `premises.md` and `model-notes.md` section 8.

### R3-06 (evidence pointers, minor): Stale or wrong receipt pointers and status lines
- Owners and corrections:
  - `VV/coverage-map.md`: the explorer reruns are at 21:51 UTC, not "after 21:55 UTC".
  - `VV/coverage-map.md`: the formal support for `load` should cite the survey `20261113T214445Z-survey-asbuilt-r2` instead of the conforming-domain pass, which holds by construction.
  - `VV/coverage-map.md` "Status" and `bugs_issues.md` header: "Updated after stage C round 1" should say round 2.
  - `B/model-notes.md` section 6: name the final-candidate receipts `215113Z`-`215941Z`.
  - `A/coverage-map.md`: "Formal (B1) evidence is still pending" and 51 "pending B" cells should point to `VV/coverage-map.md`.
  - Ontology `transaction` metadata: "Formal evidence is linked at handoff" should link `VV/coverage-map.md`.
  - `BINDINGS.md` and `model-notes.md` section 9: "open question R2-03" should be Q-22, `transaction.flag-sources.dependency-defaults`.

### R3-07 (handoff, minor): Wording in the defect summary and the decision table
- Owner: `handoff.md`.
- Discrepancy:
  - "Each has a governing source" includes I1 and I2, which by definition have none.
  - D13 is grouped under "Wrong history content", but its obligation is that the returned status agrees with the namespace.
  - The catch-all decision row lists Q-12, which `A/coverage-map.md` says is closed as a decision.
  - The `theorem_r1` claim would read more safely with "a dependency row written inside its parent's transaction is not durable, so D1 is not a counterexample".
- Proposed repair: reword those four places.

### R3-08 (coverage disclosure, minor): Domain exclusions for installed dependencies are not in the handoff
- Owner: `handoff.md` "Incomplete work", `BINDINGS.md` abstractions.
- Discrepancy:
  - `cfg_wf` admits an installed dependency only at the requested version and only non-dev. So a parent `load` that upgrades an installed dependency is outside the domain, and so are the dependency-level downgrade and UpdatePackage guards (`guard_refuses` for the dependency frame, where `-dev` does not reach a non-snapshot dependency).
  - BINDINGS mentions only "a newer version" and a developer-mode dependency. The handoff lists neither.
- Proposed repair: add "dependency already installed at another version, or in developer mode; version conflicts" to the not-modeled list.

### R3-09 (ontology, minor): Q-15 lost two draft options
- Owner: `history.fields.dependency-source` (Q-15).
- Discrepancy: the draft offered "show their source in the listing" (part of its provisional answer) and the alternative "clear the source of records labeled `load`", with the reason "hiding a real source loses useful history". The final Q keeps only "label them `install`".
- Proposed repair: keep the dropped options as further alternatives, or say why they no longer apply.

## 7. Limits

- My expectations in section 1 were written after reading the round 2 review, so they are not fully independent of it.
- I did not run IRIS. Native claims rest on the existing receipts, which I checked for markers, verdict files, snapshot provenance and agreement with the current `cases.json`.
- I reran only the as-built proof in the repository. I did not rerun the explorer or Verus controls there. Instead I compared their inputs with the current sources and regenerated `cases.json` and the survey from a clean scratch copy.
- Variants 15, 16 and 17 and the harness `rev3.rs` are unverified Rust over the executable twins of a modified scratch copy. Only variant 15 was also run through Verus. Variant 16 is one reading of the proposal text.
- I read the capability-level ontology outside load and history only where it links to them. I searched GitHub for nothing beyond the sources the artifacts cite.
- I did not review the history readers (D2, D3, D5, D18, D20) beyond confirming that their receipts exist. They are outside the formal model and were checked in rounds 1 and 2.
