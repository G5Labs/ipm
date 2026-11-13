# Stage C round 2 review: confirmation and reproduction

Independent reviewer for round 2. I wrote none of the reviewed artifacts and took no part in round 1. Product `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. `VV` = `.g5/Verus_verification`.

My writes are this file and these receipts:

- The runbook's own scripts wrote `VV/results/verus/20261113T210746Z-repro-asbuilt`, `…210836Z-repro-control-01`, `…210920Z-repair-103-dep-clauses` and `…210930Z-asbuilt-dep-clauses`. `receipt.py` also appended their four lines to `VV/results/index.jsonl`.
- `VV/native/run-iris.sh` wrote `VV/results/native/20261113T210955Z-seed-history` and `…211000Z-repro-history-delete-underscore`. `evaluate.py` added the `verdict.json` in the second.
- I wrote `VV/results/verus/20261113T212431Z-review-r2-mutants` by hand: diffs, harness, raw logs and `receipt.json` for the variants in section 4.

`./run-iris.sh up` recreated `ipm-verus-uplift-iris` and `ipm-verus-uplift-oras`. Both are left running and healthy. I touched no other container. I ran one Verus process at a time with `--num-threads 4`. The model variants were built in a scratch copy at `/tmp/r2-model`, outside the repository.

Order of work: first the runbook reproduction, cold, from `VV/README.md` only. Then the original sources: `Main.cls` `XData Commands`, README, CHANGELOG, wiki `781e98f`, issues #366, #1237, #1322, #363, #887 and #474 (plus #706 for D17), and the code. I wrote section 1 next, then read the ontology, models, bindings and evidence. Round 1 reviews, dispositions and the B reports came last.

## 1. Expectations derived from the sources (written before reading the artifacts' claims)

### Load

- E-L1. `load` needs a path. A nonexistent, non-URL path that is not absolute is refused, and a nonexistent absolute path fails (#706, #726).
- E-L2. `.tgz` and `.tar.gz` archives are handled as follows:
  - Both are extracted, and the top-most `module.xml` is used (#474).
  - The content is copied to `<ModuleRoot>/<name>/<version>/` (#945).
  - The temporary folder is removed after success and after failure (#1183).
  - Code: `extension = $piece(path,".",*)` can never equal `tar.gz`. `f83b207e` (#887) replaced the two-piece test. I expected `.tar.gz` loads to fail, against help `Main.cls:282` and #474.
- E-L3. A downgrade is refused. A newer version whose manifest has `UpdatePackage` is refused for `load` and `install` without `-force` or `-dev` (#550).
- E-L4. In normal mode, one transaction covers the whole load including dependencies, and a failure rolls back everything (`-dev` help, wiki/00).
- E-L5. In `-dev` mode:
  - The manifest is committed first.
  - Dependencies load in a second transaction.
  - The module's own phases run outside any transaction, and a failure keeps what they did (help, wiki/00, wiki/09).
- E-L6. With `-DNoTransaction=1` or `-DNoJournal=1` there is no transaction, and a failure keeps partial changes (#1237).
- E-L7. Dependencies come from the configured repositories and from `.modules`. Every parameter except DeveloperMode propagates (#316). A read-only repository forces normal mode.
- E-L8. When the caller already holds a transaction, IPM's `tcommit` only lowers `$TLEVEL`.

### History

- E-H1. Every load, install, uninstall and update has a record, including dependency installs (#366). A failure stays visible after a rollback (#366, wiki/09).
- E-H2. A killed process leaves a record with an empty end time (wiki/09).
  - Phases reach the durable row only through `PersistToTwin`, so a killed record lists no phases.
  - That contradicts wiki/09 L161 ("localizes a hang or a crash to that phase").
- E-H3. `Committed` says whether changes were kept (#1237, #1322, CHANGELOG):
  - A `-dev` load rolled back before its first commit reads 0.
  - A NoTransaction failure that wrote reads 1.
  - A dependency's own commit inside its parent's transaction is not final.
- E-H4. A dependency's durable row is inserted and persisted inside its parent's transaction, so the parent's rollback deletes it. That is against #366.
- E-H5. Filters are AND-combined, with `*` and the comparison operators. wiki/09 documents the `Version_*` columns. `ConstructSQLWhere` stops at the first name containing `_` and drops every later filter, so a filtered delete can delete everything.
- E-H6. `history delete` needs a filter or `-confirm`, and `-globally` skips `-confirm` (wiki/09).
- E-H7. `HistoryRetain` deletes records by `TimeStart`, once per day, in every namespace. The cutoff is bound with embedded quotes, so I expected the purge to delete nothing.
- E-H8. A finalized row rejects UPDATE (wiki/09, trigger).
- E-H9. With `-globally`, `-limit` applies per namespace.
- E-H10. The help example `-DSucess` is misspelled.

Every expectation above is represented in the ontology. E-H2's crash case is now an I rule in `history.record`, with D10 confirmed. E-L2 is D15, E-H4 is D1, E-H5 is D2 and E-H7 is D3.

## 2. Reproduction log (cold, from `VV/README.md` only)

| # | Command (working directory as the README says) | Expected (README) | Observed | Receipt | Final receipt compared | Match |
|---|---|---|---|---|---|---|
| F1 | `python3 -I scripts/receipt.py verus repro-asbuilt as-built "verified, 0 errors" "runbook: as-built proofs"` (crates) | `verified 259, errors 0`, matching `20261113T204233Z-asbuilt-proofs-r1`. Under a minute when cached | `verified 259, errors 0`, exit 0. About 33 s of build and verify | `results/verus/20261113T210746Z-repro-asbuilt` | 204233Z: 259/0. `inputs.sha256` agrees on every `.rs`, `Cargo.toml` and `Cargo.lock`. It differs only in `scripts/receipt.py`, which gained the `repair-survey` kind about a minute after 204233Z | yes |
| F2 | `python3 -I scripts/receipt.py verus repro-control-01 control-01 "failure of lemma_lnm0_b" "runbook: control 01" --features control-01` | errors in `lemma_lnm0_b` and `case_c23` | 2 errors: `proofs.rs:281` (`lemma_lnm0_b`, pc 8) and `cases.rs:130` (`case_c23`), exit 101. These are real verification failures, not compile errors | `…210836Z-repro-control-01` | `…204430Z-control-01-verus-r1`: identical error set | yes |
| F4 | `python3 -I scripts/receipt.py explore repair-103-dep-clauses repair-103 pass "runbook: D1 repair variant"` | `PASS`, exhaustive, 189,577 states | `pass`, 189,577 states, 189,996 transitions, depth 119, not truncated | `…210920Z-repair-103-dep-clauses` | `…204417Z-repair-103-dep-clauses`: identical counts and inputs digest | yes |
| F4b | `python3 -I scripts/receipt.py explore asbuilt-dep-clauses as-built violation "…"` (the counterpart the README names) | VIOLATION | `violation`, 78,628 states, depth 15. Clauses `H-RECORD-FIELDS[top]` and `H-RECORD-EXISTS[dep1]` | `…210930Z-asbuilt-dep-clauses` | `…204417Z-asbuilt-dep-clauses`: identical counts and identical counterexample clauses | yes |
| N1 | `./run-iris.sh up` (native) | `up: ipm-verus-uplift-iris healthy` | `up: ipm-verus-uplift-iris healthy` after 7 s. Both containers were recreated at 21:09:43Z | none (the script writes no receipt for `up`) | n/a | yes |
| N2 | `printf … > /tmp/seed.cos; ./run-iris.sh exec seed-history /tmp/seed.cos` | gives the history at least one record | COMPLETED rc=0, `simplest-module` 1.0.0 loaded | `results/native/20261113T210955Z-seed-history` | n/a | yes |
| N3 | `./run-iris.sh exec repro-history-delete-underscore repro/history-delete-underscore.cos` | `process_outcome` COMPLETED | COMPLETED rc=0, `NATIVE-END` present | `…211000Z-repro-history-delete-underscore` | `…184623Z-repro-history-delete-underscore`, the run `bugs_issues.md` cites. Same script, byte-identical | yes |
| N4 | `python3 -I evaluate.py ../results/native/20261113T211000Z-repro-history-delete-underscore cases/expected-repro.json` | X3 DEVIATES. `total.after` 0 with `total.before` ≥ 1 and `truth` 0. `total.restored` = `total.before` | X3 DEVIATES (`mismatch=total.after`). before=4, truth=0, `Deleted 4 record(s).`, after=0, restored=4. Expectations were snapshotted before the run (`written_before_run: true`) | same directory, `verdict.json` | 184623Z: before=81, after=0, restored=81, DEVIATES. The counts differ only because `up` resets the container | yes |

Result: every requested step reproduced with its stated outcome, and every reproduced Verus and explorer result is identical to the authors' final receipt.

Places where the README is wrong, incomplete or needs knowledge it does not give:

1. At the start of this review, `.g5/uplift/ipm-verus-20261113/handoff.md` did not exist. The README "Status" section and the last line of `coverage-map.md` send the reader there for open decisions and incomplete work. The orchestrator created it at 21:08:08Z, during this review, marked `STATUS: DRAFT`. I read it and include it in this review.
2. The native section cites no final receipt for D2. A reproducer has to find `20261113T184623Z-…` in `bugs_issues.md`. The README should also say that `total.before` depends on the container's state, and that the image itself already holds records, so `up` plus one seed gave 4.
3. Unstated prerequisites:
   - The offline build needs the pinned `vstd` and `verus_state_machines_macros` crates already in the local cargo registry.
   - It needs the rustup toolchain `1.98.1-x86_64-unknown-linux-gnu` installed.
   - The image-build hint needs BuildKit, because the Dockerfile uses `RUN --mount`, and network access for the base image.
   - The README gives none of these, or how to obtain them.
4. Running the runbook appends to the tracked `VV/results/index.jsonl`, and the README does not say so. `receipt.py` labels the repair-variant run `repair-103-dep-clauses` with `kind: "control"`, although it is a repair check that expects `pass` (finding R2-10).
5. Step 1 says the result "matches" the final receipt. That holds for the verdict. The inputs digest differs, because `receipt.py` changed after 204233Z, and the README does not say this is expected.

## 3. Findings

Domains are noted in each title. Severity is material or minor.

### R2-01 (model, material): L1 does not require developer-mode phase work to survive a phase failure, so a product that rolls it back passes every contract check
- Kind: model error
- Source or owner:
  - `load.outcome.dev` (I): "The module's own phases, Initialize through Activate, run outside a transaction. Whatever they did before a failure stays."
  - `dev-mode.keep-on-failure` (I).
  - wiki/00 L126: "If a later step fails, you are left with a partial install you can inspect and fix".
  - Owner: `VV/crates/ipm-load-history/src/contract.rs` `l1_alts`, last branch.
- Trigger: reviewer variant 212, a faulty implementation (`results/verus/20261113T212431Z-review-r2-mutants`, `model.diff`).
  - After C2 it opens an inner transaction around the developer-mode phases. It commits that transaction on success and rolls it back on failure.
  - A `-dev` load whose Activate fails therefore loses the classes that its Reload loaded.
- Discrepancy:
  - For a developer-mode load that fails in Activate or after it (verdict 7 or 8), `l1_alts` returns `mx(4, ver, 1)`, which means "present at ver, resources any" (`contract.rs:163`).
  - The native expectation reads the same way. `cases.json` B1-C26 is titled "Activate fails (-dev): manifest and resources kept", but its `l1_permitted` says `"resources": "any"`. B1-C38 says the same.
  - Under variant 212, 168 of 5,850 domain configurations behave differently. Zero L1 clauses change verdict, and so do zero COMMIT-SOUND, COMMIT-EXACT and TLEVEL results (`rev2.log`).
  - In Verus, the only failures are the transaction-depth shape invariant (`inv` at `lnm0`/`lnm1` pc 13 and 29) and the as-built case lemma `case_c46` (`verus-review-212.log`). No contract theorem fails.
  - Four replayed `l2_predicted` values change (B1-C26, C38, C46T and C46K), but no native `l1` verdict flips (`case-predictions.txt`).
  - So the product is pinned only by the binding check. A future refactor that changes the depth invariant, such as a D1 repair, would see L1 still pass.
  - `coverage-map.md` lists `load.outcome.dev` as "supported" with formal support `case_c29` and controls 01 and 04. None of these constrains the phases' writes.
- Limits:
  - I did not test a developer-mode Reload or Compile failure. The model has only Activate faults.
  - The process-end expectations (`l1_alts_kill`, kind 5 "unchanged or present") are also looser than the reachable set. `model-notes.md` section 9 already discloses that.
  - For `-DNoTransaction=1` and `-DNoJournal=1` failures, `any_alt()` also leaves the namespace unconstrained. There, the Committed clause compensates.
- Proposed repair:
  - B1:
    - For a developer-mode failure at verdict 7 or 8 with transactions, require the module installed at `ver` with resources loaded (`mx(1, ver, 1)`, or a new kind for "present, resources loaded").
    - Add variant 212 as a control that L1-NS-ALL must detect.
    - Regenerate `cases.json`. B1-C26 and C38 `l1_permitted` become "resources loaded".
  - B2: re-evaluate the replays against the new `cases.json`. No IRIS run is needed: native A5 and B1-C26 already observed the classes kept.
  - Coverage map: cite the new control under `load.outcome.dev`.

### R2-02 (repair proposals, material): The D1 and D10 repair proposals cannot meet their process-end obligations, and the D1 repair variant passes only through an unnamed write mechanism
- Kind: model error (repair variant 103 against its proposal) and an incomplete repair proposal (`bugs_issues.md` D1 and D10, `design_simplification.md` S2)
- Source or owner:
  - `history.record` (I): "A process that dies mid-action still leaves a record … lists its record with Success 0, no end time and the phases it has started".
  - `history.survives-rollback` (I).
  - Premise R-01, UNVERIFIED in `premises.md`.
  - Owners: `VV/bugs_issues.md` D1 and D10, `VV/design_simplification.md` S2, `B/model-notes.md` section 5, `BINDINGS.md` "Repair variant 103".
- Trigger, part 1: a SIGKILL inside the parent's Activate after a dependency loaded, in normal mode (B1-C45K).
  - Under the concrete mechanism D1 and S2 propose, the top-level `LoadNewModule` persists each nested log "after its own final `tcommit` or rollback at the entry `$TLEVEL`" (D1), and "writes the durable rows once" (S2).
  - So the dependency has no durable row until the top-level action ends, and the kill leaves none.
- Trigger, part 2: B2 case C3, a non-dev SIGKILL in Activate (`native-report.md` L127), for D10.
  - D10's proposal copies phases to the durable row only "at each phase boundary and commit at `$TLEVEL` 0" and otherwise "inside a transaction, keep them in IRISTEMP and copy them at the action's end".
  - A normal-mode load runs all its phases at `$TLEVEL` 1, so after a SIGKILL its durable row still has no phases.
- Discrepancy:
  - Variant 103 does not model the proposed mechanism. It inserts and persists dependency rows with `autonomous_twin` from the dependency's start (`model.rs` `twin_insert`, `dep_autonomous`), which assumes R-01.
  - That is why `repair103_c45` reports that the dependency row survives a kill and a terminate. The deferral the proposal names cannot produce that.
  - D1's distinguishing checks (A1, A3, A5/C38, A0) all end with the command returning. They would pass for a deferral fix that still loses dependency rows at process ends.
  - D10 lists C3 as a distinguishing check, but C3 cannot pass under D10's own mechanism.
  - S2 says it "removes D1, V1, V2, V9 and V10" and names only the caller transaction as not removed. Yet its own remark applies equally to the load's own transaction: "An in-process write cannot leave the caller's transaction except through IRISTEMP or with journaling switched off".
- Limits:
  - I did not model the deferral variant.
  - A journal-off write of the History row looks feasible. N-P07 shows unjournaled sets survive `TROLLBACK` with the flag restored, and M5 raised no `<ROLLFAIL>`.
  - I did not test whether such a write inside an open transaction, mixed with journaled updates of the same row, avoids `<ROLLFAIL>`.
- Proposed repair, in the orchestrator's `bugs_issues.md` and `design_simplification.md`:
  - Name the in-process mechanism for a write that no rollback undoes. For example, insert and update the History row under `%IPM.Utils.JournalManager(0)`.
  - Turn R-01 into a native premise check: an unjournaled History insert inside an open transaction survives `TROLLBACK 1`, the parent's rollback and SIGKILL, with later journaled updates.
  - Otherwise, restate D1, D10 and S2 to exclude process ends. Keep C45K and C3 as expected to stay failing.
  - Add B1-C45K/T to D1's distinguishing checks.
  - B1: either keep variant 103 and cite the named mechanism, or add a deferral variant and report what it still violates.

### R2-03 (defect classification, material): D14 is an unresolved precedence decision between two documented rules, filed as a confirmed defect
- Kind: unresolved decision
- Source or owner:
  - First rule: `load.outcome` (I), normal mode: "The whole load, including dependencies loaded for it, is rolled back".
  - Second rule:
    - `transaction.flag-sources` (I): "These flags can come from … the module's own `<Defaults>`".
    - wiki/00 L175: `<Defaults>` are "applied whenever this module is installed".
    - wiki/00 L149: NoTransaction is for "when some modules in the install conflict with an outer wrapping transaction".
    - The maintainer doc comment at `Utils/Module.cls:1033-1040`, whose example `<Defaults>` sets `NoTransaction`.
  - wiki/00 L155: with NoJournal, "changes cannot be rolled back".
  - Owner: `VV/bugs_issues.md` D14, `coverage-map.md` (`load.outcome`, `transaction.flag-sources`), `handoff.md` "Human decisions needed".
- Trigger: stage C case M5 (`results/native/20261113T204405Z-stagec2`). A normal-mode parent loads a dependency whose `<Defaults>` set `NoJournal=1`, then fails, and the dependency stays installed.
- Discrepancy:
  - The native evidence is real: `d.exists=1`, `d.cls.compiled=1`, no `<ROLLFAIL>`.
  - The behavior is what two documented rules jointly produce: a `<Defaults>` NoJournal applies whenever the module is installed, and NoJournal writes cannot be rolled back. Only the normal-mode rollback rule contradicts it.
  - D14 is still filed as "confirmed" against `load.outcome`.
  - Its repair, "ignore `NoTransaction` and `NoJournal` from a dependency's `<Defaults>` while a parent transaction is open, or refuse such a dependency", would remove documented behavior. Its preserve list keeps `<Defaults>` flags only for a top-level load.
  - The coverage map itself marks the precedence "unestablished". `handoff.md` lists no human decision for it.
- Limits: I did not search GitHub for a maintainer statement on dependency `<Defaults>` inside a parent transaction.
- Proposed repair:
  - A3: add a Q under `transaction.flag-sources`, for example "Does a dependency's own `<Defaults>` NoTransaction or NoJournal apply inside its parent's transaction?". Give a provisional answer and both alternatives.
  - Orchestrator: mark D14 "confirmed, decision pending", add the Q to the handoff's decision table, and keep the repair as an option under the Q.

### R2-04 (ontology, minor): Ontology metadata is stale after B2 phase 2
- Kind: missing or stale evidence
- Source or owner: disposition R1-ONT-12 ("record confirmed, refuted and not-run status with run IDs on the owning blocks").
- Discrepancy: several blocks still say the behavior was not yet run, although phase 2 confirmed it:
  - `load.path`: "Suspected regression, not yet run natively (queued for B2 phase 2)". This is D15, S1.
  - `load.refusal`, `load.refusal.manifest` and `update.refusal`: "not yet run natively (queued …)". These are D11, D16, S2, S2B and S3.
  - `load.refusal.path`: "An existing relative path is queued for phase 2". This is D17, S4R.
  - `transaction.flag-sources` and `settings.default-modifiers`: "Suspected gap, not run natively in B2 phase 1". These are D18, D19, H2 and X1, and D14, M5.
  - `load.dependencies.local-repo`: OBS-L12 is still "not yet run". This is D13, B1-C29.
  - No ontology block names D10-D20, I1 or I2.
  - "Formal (B1) evidence is linked at handoff" has not been done.
- Proposed repair: A3 updates these metadata paragraphs with the phase-2 run IDs and defect IDs, and links `VV/coverage-map.md` from the two in-scope pages.

### R2-05 (bindings, minor): BINDINGS and model-notes still call R1-MOD-05 pending
- Kind: missing or stale evidence
- Discrepancy:
  - `BINDINGS.md` "Abstractions" says "Open obligation (R1-MOD-05): B2 native case pending; the model cannot predict it".
  - `model-notes.md` section 9 says B2's case "had no result when this was written".
  - M5 and M5N (`20261113T204405Z-stagec2`) and D14 now exist.
- Proposed repair: B1 cites M5 and D14 in both places, and keeps the "not representable" limit and R2-03's open decision.

### R2-06 (repair proposal, minor): D12's example repair would break commit soundness for B1-C10, and it changes a D statement without saying so
- Kind: product defect (repair proposal quality)
- Source or owner: `history.committed.marking` (D, PR #1322: "before writing when there is no transaction"); `theorem_commit_sound`; owner `bugs_issues.md` D12.
- Trigger: B1-C10, a NoJournal load whose manifest validation refusal keeps the `.ZPM` document (D11).
- Discrepancy:
  - The example "mark `Committed` only when the first write is kept, for example after `OBJ.Load` returns OK" leaves B1-C10 at `Committed=0` with module data kept, until D11 is also fixed.
  - The preserve list omits `theorem_commit_sound` and B1-C10.
  - Moving the mark also reverses the human-provided marking decision. It should be flagged as such.
- Proposed repair: in D12, prefer S1 or make the fix depend on D11. Add "Preserve: changed implies `Committed=1` (B1-C10, B1-C52, S2)" and note that the D statement changes.

### R2-07 (defect scope, minor): D6 is scoped to Activate, but the same exit exists for every phase
- Discrepancy:
  - `Storage/Module.cls` exits without `FinalizePhase` both for Invokes that run before a phase and for Invokes that run after one, in every phase. These are `quit:$$$ISERR(tSC)` at the `OnBeforePhase` and `OnAfterPhase` loops (BINDINGS cites 608-610 and 669-671).
  - D6's title, obligation and reproduction name only Activate. The repair proposal is already generic.
- Proposed repair: widen D6's title and path to "any phase whose Invoke returns an error", and keep the Activate reproduction.

### R2-08 (coverage, minor): Git URL loads have no reliance row or open obligation in the verification coverage map
- Discrepancy:
  - `load.path` lists a git URL as one of three path forms.
  - Its metadata records OBS-L13: the clone's exit code is unchecked and the clone is never removed.
  - `VV/coverage-map.md` covers `.tgz` and `.tar.gz` only.
  - Neither `handoff.md` "Native checks not run" nor the `bugs_issues.md` suspected list mentions git URLs or OBS-L13.
- Proposed repair: add the git URL case to the `load.path` row as "unestablished". List OBS-L13 under suspected issues, or tie it to Q-04, which already covers the clone folder's leftovers.

### R2-09 (handoff, minor): Runbook gaps
The items in section 2, points 1-3 and 5. Proposed repair: cite the D2 final receipt and its state dependence, list the cargo-cache, rustup and BuildKit prerequisites, and say that a run appends to `index.jsonl`. Item 1 resolved itself when the handoff draft appeared.

### R2-10 (evidence labeling, minor): Some index lines read as failures although they are expected
- Discrepancy:
  - `receipt.py` sets `kind: "control"` for any explorer run whose variant is not `as-built`. The repair checks `repair-103-*` are therefore indexed as controls.
  - The `control-05-finalize-no-isok` and `control-05b` index lines carry `expect: "violation"` with outcome `pass`. Their concern text and `model-notes.md` section 6 say the control is expected to survive.
  - A reader filtering for `expect != outcome` sees false alarms.
- Proposed repair: B1 annotates these lines, as was done for 11b, and adds a `repair` kind to `receipt.py`.

### R2-11 (handoff claims, minor): Two established claims overstate their formal basis
- Discrepancy: `handoff.md` says "Each claim is proved in Verus over that domain". Two claims do not rest on a symbolic theorem:
  - "Downgrade and update-required refusals write no module data in any mode" rests on case lemmas (`case_c11`, `case_c15`) for single configurations and on the exhaustive explorer run (L1-NS-ALL).
  - "a failure after [the first commit] keeps the manifest" cites controls 01, 02 and 04, which concern `SetCommitted` and the rollback loop.
  - R2-01 shows L1 does not constrain the rest of the developer-mode outcome.
- Proposed repair: say "proved for the listed cases and explored exhaustively" for the refusal claim, and cite the L1 clause, after R2-01's repair, for the developer-mode claim.

## 4. Model checks tried in this round (not tried in round 1)

Round 1 tried variants 201-206: SetCommitted moved before the guards, a dependency row at the parent's entry level, rows outside with Committed=1, Finalize ignoring its status, SetCommitted moved before C3, and a durable inner TCOMMIT. My variants are 211-214 in a scratch copy. The evidence is `results/verus/20261113T212431Z-review-r2-mutants`.

The harness `rev2.rs` evaluates every end state of all 5,850 configurations of the single-fault domain:
- the normal end;
- kill-like and terminate-like ends after every step;
- caller commit and caller rollback.

That is the full reachable end-state set of this deterministic machine, without foreign edits. `b1 cases` was also rerun per variant against the 59 replayed cases.

| Variant | Kind | What changes | Formal result | Native replay (59 cases) | Verdict |
|---|---|---|---|---|---|
| 211 | faulty implementation | The pre-#1322 product: no `SetCommitted` anywhere, and `Finalize` sets `Committed = (-dev) \|\| ISOK` (`git show 5d27e15b^`) | Detected. COMMIT-SOUND in 696 configurations, COMMIT-EXACT in 369, `H-COMMITTED[top]` in 1,065. Verus fails `inv` (`cs_ok`) at `top_step` and at `lnm0` pc 3, 8 and 30, plus `case_c08` | 13 `l2_predicted` values change | detected |
| 212 | faulty implementation | Developer-mode phases inside an inner transaction, rolled back on failure | Not detected by any L1 clause or property: 168 configurations behave differently, 0 verdicts change. Verus fails only the depth shape invariant and `case_c46` | 4 `l2` values change, 0 `l1` flips | contract gap, R2-01 |
| 213 | false premise | JournalManager does not restore journaling on scope exit (negates half of P-03) | 825 NoJournal configurations under a caller transaction change clauses. The Q-06b loss of the top row disappears, and record-field deviations appear | 0 `l2` changes, because no replayed case has NoJournal under a caller transaction | Refuted only by the native premise probe N-P07 (`journal.enabled.after.scope=1`, `20261113T181129Z-premises`). The model's NoJournal caller-transaction predictions rest on that probe and on the UNVERIFIED P-06c. That is disclosed, so no finding |
| 214 | erroneously permitted step | IPM commits a caller-held transaction when the command ends | Detected: L1-NAMESPACE at caller rollback in 816 configurations. It also masks the Q-06b record loss | 3 `l2` values change (B1-C41..C43) | detected |

Valid outcome:
- The unmodified crate verifies 259 obligations with 0 errors (F1).
- The valid completion (default mode, one dependency, no fault) satisfies L1 with no violated clause under variant 0 and under each of 211-214.
- Regenerating `cases.json` predictions from the current as-built model reproduces all 59 `l2_predicted` values and all `l1_vs_l2` verdicts exactly.

## 5. What I confirmed

- **Reproduction.** Formal steps 1, 2 and 4, the step 4 counterpart, and the native D2 reproduction all match the README and the authors' final receipts (section 2).
- **Round 1 repairs.** Every row of `C/round-1/dispositions.md` is present in its owning artifact, with two exceptions:
  - R1-ONT-12 and R1-MOD-05 were done for the evidence that existed then, but are stale after B2 phase 2 (R2-04, R2-05).
  - Rows I checked in the artifact itself:
    - R1-ONT-01: `history.survives-rollback`, `temp-copy` scoped to top-level actions at `$TLEVEL` 0, and `dep_rec` `ex=1`.
    - R1-ONT-02: `history.record`, Q-08 flipped with its alternative kept, Q-21 added, and `qprov` with `q08_exact` true and `qfin_unended` false.
    - R1-ONT-03: the `Package` bullet, and `top_rec` with `pkg=1` for verdicts 3 and above and for `mf>=3`.
    - R1-ONT-04: phase-2 cases S1-S4R and H2.
    - R1-ONT-05 to -11, -13 and -14.
    - R1-MOD-01: `q05_caller_exact`, `dep_rec` `com=3` without a caller transaction, variant 103, and D1 restated with A5/C38.
    - R1-MOD-02: `l1_alts` verdicts 1-4 unchanged in every mode, plus `mf=4`.
    - R1-MOD-03: `kill_rec` `t.ph`, `t.phst` and `t.pkg`.
    - R1-MOD-04: D12 and D13.
    - R1-MOD-06: diagnostic 11b.
    - R1-MOD-08: per-clause domains.
    - R1-MOD-09: 204233Z.
    - R1-MOD-10: `verdict-cases-05842975.json` beside each replay.
    - R1-MOD-11: re-evaluations kept beside the originals.
    - R1-MOD-12: D6 decision pending, D8 became I1.
    - R1-MOD-13: `native-report.md` section 4.3.
    - R1-MOD-14: `20261113T204317Z-sanity-false-claim` fails as expected.
    - R1-MOD-15: BINDINGS abstractions.
- **Ontology against `7ff5df56`.** No condition, exception or open question was lost:
  - Every Q whose provisional answer changed (Q-08, Q-09) kept the old answer as its alternative.
  - Q `history.delete.global-unfiltered` became D, supported by wiki/09 L195 and the `-confirm` help ("Ignored if -globally is passed too").
  - The dependency half of the old `history.committed.nested` and `history.dependencies.parent-rollback` became I on the strength of #366 (class a), which is intent, not code behavior.
  - New I bullets cite wiki/09 L161 and L229-233.
  - I found no code behavior promoted to intent. R2-03 concerns the opposite: an I rule applied without its competing documented rule.
- **Defect classification.** D1-D6, D9-D13 and D15-D20 each have a governing source of the stated authority. D17 is backed by #706: "load command shouldn't accept relative path". D2 is backed by wiki/09 L112. The exceptions:
  - D14 (R2-03).
  - D12 depends on reading the D marking statement against #1237 (R2-06).
  - The Q deviations are correctly marked decision pending.
  - D7 is correctly withdrawn (wiki/09 L133).
- **Native evidence.** I opened the raw logs and verdicts of these runs. The `OBS` lines are real, the markers complete, and the verdicts follow from the observations:
  - 184617Z dep-history-lost (D1: `ids.allocated=2`, `rows=1`, `dep.rows=0`)
  - 184623Z (D2)
  - 184626Z autopurge-noop (D3)
  - 184628Z config-delete-retain (D5: returns 7)
  - 184630Z sysreq-dev-manifest-left (D4)
  - 184634Z activate-invoke-phase-open (D6: Activate open, `ERROR #00`)
  - 184632Z caller-tx-history-lost (Q-05)
  - 204152Z stagec (D11, D15, D16, D18, D19, D20)
  - 204405Z stagec2 (D14, D17)
  - 193952Z replay-return-b (D12 B1-C08, D13 B1-C29)
  - 183827Z c4r (Q-08, D10)
  - 184444Z cases-hr (D9, I1)
  - The phase-1 case runs (`182759Z` to `183827Z`) carry `written_before_run: false`. That is disclosed in `native-report.md` section 8, and the cases were replayed with snapshots (`184230Z-replay-fast`) or superseded by K-variant replays.
  - The S4 CONFORMS in `204152Z` was correctly re-evaluated to INCOMPLETE and replaced by S4R.
- **Coverage-map receipts.**
  - The final proof receipt 204233Z shows 259/0, with no `assume`, `admit` or `external_body` in `src`.
  - The `cases.json` sha256 `05842975…457961` covers 59 cases.
  - Against the final `cases.json`, 57/57 realized cases agree and C49T/K are approximate. I recomputed this from the verdict files. My field count differs from the 1161 product fields cited, because I included binding fields.
  - Controls 01-04 and 06-11 are detected and control 05 survives, as `model-notes.md` section 6 says, with receipts 204419Z-204957Z.
  - Every theorem and case lemma the map names exists in `proofs.rs` or `cases.rs`.
  - Exceptions: the `load.outcome.dev` formal support (R2-01) and the D1 repair's process-end claims (R2-02).

## 6. Limits

- I did not rerun README step 3 (the D1 counterexample), the native replays, the product suites or the premise probes. I reused their receipts after checking applicability: the same `cases.json` sha, `inputs.sha256` and image.
- My variants live in a scratch copy and my harness `rev2.rs` is unverified Rust over the executable twins. The Verus runs of variants 211 and 212 used the reviewed proofs unchanged, apart from the feature switch.
- I did not test a developer-mode Reload or Compile failure, deferral or journal-off repair variants, or a NoJournal load under a caller transaction natively.
- I did not search GitHub beyond the issues listed in the brief and #706. In particular, I found no maintainer statement on dependency `<Defaults>` inside a parent transaction (R2-03) or on concurrent loads. No source I read addresses concurrent package changes. The handoff lists concurrent loads as not run.
- The capability-level ontology outside load and history was read only where load and history link to it (transactions and developer mode, dependencies, settings, update, uninstall).
