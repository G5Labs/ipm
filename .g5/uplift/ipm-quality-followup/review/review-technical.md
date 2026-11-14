# Independent technical review of the follow-up corrections

Reviewer: independent. I did not write any of the follow-up. Paths are relative to `.g5/` unless they start with `src/`. `VV` = `Verus_verification`, `FU` = `VV/followup`, `FR` = `FU/results`, `S` = `uplift/ipm-quality-followup/review/scratch`.

## Scope, revision and method

- Product `571e04a1`, read-only. No IRIS session was run and no container was touched. Native evidence was reviewed from saved files only.
- Reviewed revision: commit `2183108e` plus its working tree when I started. The follow-up kept committing while I worked (`44c8db7d`, `be6be6f9`, `30cc5de3`, `af2a9f48`, `bf7daf22`, `f0c30adc`, `8c576c4b`). I re-checked every reviewed file against HEAD `8c576c4b`:
  - The evaluator files `l1sem.py`, `evaluate_replay2.py`, `run_controls.py`, `reevaluate_all.py` and `PROJECTION.md` are unchanged.
  - `loadtx.py` and `history_readers.py` changed only in a docstring line number.
  - MODELS.md changed one number pair (see T3-m6) and one PURGE paragraph.
  - `run-native.sh` gained the `NATIVE_ORIGINAL` staging.
  - The later R03f re-execution (`FR/native/20261114T000144Z-*`, `…000251Z-*`, `FR/20261114T000320Z-replay-rerun-compare`) appeared during the review. I did not review it.
- Method. Before opening any follow-up conclusion I read four sources:
  - `VV/crates/ipm-load-history/src/contract.rs` (all) and `model.rs` (all).
  - `VV/native/evaluate_replay.py` (all) and the driver `VV/native/scripts/B2Native/Replay.cls` (all).
  - The operator audit (`$IPM_REVIEW_AUDIT`, R01–R12).
  - The product source: `src/cls/IPM/Utils/Module.cls` 36–211, 293–330, 1152–1420; `General/History.cls` 1–320; `AbstractHistory.cls`; `HistoryTemp.cls`; `Utils/JournalManager.cls` 1–60; `Storage/Module.cls` 1085–1160 and 1860–1882; `Main.cls` 4127–4177; `CLI.cls` 808–828.

  Only then did I read `PROJECTION.md`, `MODELS.md`, `README.md`, `audit-validation.md` and `work/followup-findings.md`.
- Every script I wrote is under `S/`, and every Python run used `python3 -I -B`. When rerunning the follow-up's `run_controls.py`, I pointed `TMPDIR` at `S/tmp` so its copied receipts stayed inside the repository. `S/ontology-tags.txt` is not mine.

### Expectations written down before reading the follow-up's conclusions

1. A faithful L1 evaluator evaluates `expect_ok` = `st_ok ∧ alts_ok ∧ ∀slot (rec_exists_ok ∧ rec_fields_ok ∧ rec_com_ok)` (`contract.rs:346-360`). It must also do the following:
   - `mexp_ok` kinds 0–6 exactly as `contract.rs:293-301`:
     - kind 0 is `m == i` over all six `Mod` fields, including `wr`.
     - kind 1 requires `res`.
     - kind 2 requires `doc == ver` and `res == i.res`.
     - kinds 5 and 6 are disjunctions with `m == i`.
   - `kept`: for the top action, any of the three slots changed; for a dependency, only its own slot.
   - At most three alternatives.
   - Missing or malformed observations never count as agreement.
2. The observation projection must map `present/ver/dev/res/doc` to the driver's `manifest_present/version/developer_mode/resources_loaded/zpm_document_version`. `wr` is unobservable. `LastUpdated` (stamped in `%OnBeforeSave`, `Storage/Module.cls:1879-1882`) is a sound proxy only if the command starts more than one second after the "before" read. The driver waits 1.1 s (`Replay.cls:33`).

   An absent record exposes no `ver`/`dev`/`LastUpdated`. In the model such a record must keep its initial values: `m_dev`, `m_res` and `m_touch` follow `m_manifest`, and `m_doc` changes only `doc`.
3. Evidence handling must bind the observations to the run's schedule and the script hash, require one BEGIN and one END per case, detect conflicting duplicates and refuse inputs changed since the run.
4. Mode resolution per `Module.cls:1165-1172`: `merge params = defaults` then `merge params = pParams`. A dependency receives the parent's merged params through `LoadDependencies(.params)` → `LoadModuleReference(.pParams)` → `LoadModuleFromDirectory` (`tParams = pParams`, `DeveloperMode` overridden) → `LoadNewModule`.

   At a Terminate, P-06c ("innermost frame first") means a NoJournal dependency's `JournalManager` (in its `LoadNewModule` frame) closes before that dependency's log (in the `LoadModuleReference` frame). The manager's `%OnClose` restores the flag (`JournalManager.cls:22-25`).

## 1. Corrected replay evaluator (`FU/replay/l1sem.py`, `evaluate_replay2.py`, `PROJECTION.md`)

### What I inspected

- `l1sem.py` 1–387 line by line against `contract.rs` 289–360:

  | `l1sem.py` | Lines | `contract.rs` |
  |---|---|---|
  | `mexp_ok` | 261–283 | 293–301 |
  | `installed_ok` | 252–258 | 291 |
  | `mod_eq` | 223–234 | `m == i` |
  | `alts_ok` | 297–300 | 311–313 |
  | `kept` | 348–352 | 318–321 |
  | `rec_exists_ok` / `rec_fields_ok` / `rec_com_ok` | 321–345 | 324–338 |
  | `st_ok` | 355–362 | 346–348 |

  I also checked the decoders against `explore.rs` 465–562 (`mexp_json`, `rexp_json`, `expect_json`) and the clause names against `violated_clauses` (`explore.rs:572-587`).
- `evaluate_replay2.py` 1–363, covering evidence, l2, l1 and relation.
- `PROJECTION.md`, all of it, against `model.rs:52-64, 91-110, 444-450, 486-609` and `Replay.cls:234-384`.
- Bindings: all 59 bindings in `VV/native/cases/replay-binding.json` realize exactly `top` plus one slot per case dependency, with no empty names (scripted check).

### Commands and key outputs

- Controls rerun: `TMPDIR=S/tmp python3 -I -B followup/replay/run_controls.py S/controls-rerun` → `counts {'A': 175, 'B': 59, 'C': 43, 'D': 12}`, all pass, rc 0. The result list and the input hashes are identical to the saved `FR/20261113T232331Z-replay-controls/controls.json` (289/289 results).
- Discrimination test: `S/mutate_checks.py` builds 15 copies of the evaluator, each with one check removed, and reruns the follow-up's controls against each (`S/mutants/mutants-summary.json`). Thirteen are detected, for example:

  | Mutant | Controls that fail |
  |---|---|
  | M1 `installed_ok` without `res` | 6, e.g. "kind 1 installed: resources missing" |
  | M2 `mod_eq` without the LastUpdated proxy | 3, including family B on B1-C19/C20 |
  | M3 dependency `kept` over all slots (the R02a bug) | 4, the "Committed per slot" pair |
  | M4 kind 2 without the document check | 2 |
  | M8 no conflict detection | 1 |
  | M9 no stale check | 1 |
  | M10 no script-hash check | 1 |
  | M12 Committed ignores `kept` | 14 |

  More than three controls are genuinely discriminating. Two mutants survive (T1-m2).
- Adversarial observations (`S/adversarial_l1.py`):
  - B1-C05 (status "error" required) with `status=OKAY`, `ok` or an empty value passes L1-STATUS.
  - B1-C01 with `status=OKAY` fails L1-STATUS instead of being UNKNOWN.
  - End to end, `evaluate_replay2.py … --mutate B1-C05:status=OKAY` gives `l2=DISAGREES | l1=PERMITTED`.
  - An absent record whose resources or stored document appear fails kind 0 and H-COMMITTED as it should.
  - A missing `history.top.row` or malformed `phases_started` gives UNKNOWN as it should.
- Missing per-case end marker, on a copy of return-b with `CASE-END B1-C01` removed: `B1-C01 … INCOMPLETE … problems:markers BEGIN@10`. The evaluator handles it, but no control exercises it.
- Receipt binding: `input.cos` hashes equal the receipts' `script_sha256` for return-b, terminate-b and sigkill-b. Recomputing the runner's `inputs/` digest (`run-iris.sh:131`) gives `9a14945…` for all three, equal to each receipt's `expectations_snapshot_sha256`.

### Findings

- T1-m1 (minor, wrong permit by construction). `st_ok` (`l1sem.py:355-362`) computes `EQ(s == "OK", e.st == 1)`. Any value other than `OK` and `<none>` therefore counts as an error status. The module docstring (`l1sem.py:17-20`) says a malformed observation "can neither pass nor fail a clause", and `malformed_fields` (`evaluate_replay2.py:227-229`) does not list `status`. The driver only ever writes `OK`, `ERR` or `<none>` (`Replay.cls:47, 101`), so no saved verdict is affected. The fix is to map only `ERR` to an error and everything else to None.
- T1-m2 (minor, control coverage). No control fails when the per-case BEGIN/END check is removed (M11) or when `res` is dropped from `mod_eq` (M14). The missing-case control deletes all of the case's lines, so `x.ids_allocated` makes it INCOMPLETE anyway. The code itself is right on both points; only the controls do not discriminate them.
- T1-m3 (minor, evidence). `evaluate_replay2.py` checks only `input.cos` against the receipt. It uses `inputs/replay-binding.json` and the cases snapshot without checking the receipt's `expectations_snapshot_sha256`. My manual recomputation shows they match for the three final runs.
- T1-m4 (minor, projection). The driver files every History row whose package is neither dependency under `top` (`Replay.cls:356-364`). The evaluator does not compare `history.top.x.package` with the top's name or the empty string. A foreign row with ID above the baseline would therefore stand in for a missing top row. This is not plausible in the dedicated container, but the check is cheap.
- T1-m5 (minor, wording). PROJECTION.md says "each fixture has one class". Several replay fixtures have two or three classes in the one `.PKG` resource, for example `native/fixtures/replay/r19b/m/2.0.0/src/R19bM/{Main,Upd/V1,Upd/V2}.cls`. `Main` remains a reasonable all-or-nothing proxy.
- Projection itself: justified.
  - INV-ABSENT matches `model.rs`: `m_dev`, `m_res` and `m_touch` are reached only after `lnm_load` wrote `m_manifest` (pc 4→5→6, 12, phase k=2), and `m_doc` (manifest refusal under NoJournal) changes only `doc`, which is observed.
  - The LastUpdated proxy is sound under the driver's 1.1 s wait. The one assumption is that every module-record write in the load path is an object `%Save`.
  - `kind 0`, `kept` and the alternative cap are faithful.
  - Family B (each case's `l2_predicted` projected back must fail exactly `violated_clauses`) is a real differential test of the transcription. Mutants M2, M6, M12, M13 and M15 are caught by it.

Verdict: holds with caveat. It is a faithful transcription of `contract.rs`, and its evidence handling is substantially stronger than the original's. The caveats are the malformed-status permit (T1-m1) and two untested checks (T1-m2).

## 2. Re-evaluation claim (`FR/20261113T232332Z-replay-reeval`)

### What I inspected

`reevaluate_all.py` 1–131 and `summary.json`. Across the five final runs I count 59 cases, `verdict_changed` 0 and `same clauses` 59. Original and corrected l1 pairs: 34 VIOLATES→VIOLATES, 23 PERMITTED→PERMITTED, 2 APPROXIMATE (violates)→same.

For return-b, terminate-b, sigkill-b and c51-c52, both evaluators used the same target (`cases.json` = `10410864…`, the original's `verdict-cases-10410864.json`). For approx-c the l1 expectation is unchanged between the snapshot and the target. The superseded attempt changes B1-C19 and B1-C20 (VIOLATES `history.top.committed` → INDETERMINATE) and B1-C40 (PERMITTED → INDETERMINATE). That raw.log has no `x.last_updated` line, as claimed.

### Hand spot-checks from raw.log against `cases.json`

| Case (run) | Observation (key lines) | My evaluation | Saved re-eval |
|---|---|---|---|
| B1-C19 (return-b, lines 1544–1648) | `status=OK`. Top present at 2.0.0 before and after, dev 0, res 1, document 2.0.0. LastUpdated 19:40:15 → 19:40:17. Row OK, Committed 1, TimeEnd 1, Finalized 1, 5 phases, last OK | Kind 1 passes. `kept` is true only through the LastUpdated proxy, so Committed 1 passes. PERMITTED | PERMITTED, [] = model |
| B1-C33 (return-b) | ERR. Top absent → present at 2.0.0, dev 1, res 0 = before, document 2.0.0. dep1 absent and unchanged. `history.dep1.row=0` | Kind 2 passes and dep1 kind 0 passes. The top row passes (Committed 1, kept). dep1 needs a row: H-RECORD-EXISTS[dep1] fails | VIOLATES H-RECORD-EXISTS[dep1] = model |
| B1-C38 (return-b) | ERR. Top 2.0.0 dev 1 res 1. dep1 1.0.0 dev 0 res 1. Rows: top ERR, Committed 1, last ERR. dep1 OK, Committed 1 | Both slots kind 1. Committed equals kept for each slot. PERMITTED | PERMITTED = model |
| B1-C46T (terminate-b) | Top present at 2.0.0 dev 1, dep1 1.0.0 res 1. Top row ZERO, Committed 1, TimeEnd 0, Finalized 1, 5 phases, OPEN | Kinds 5 and 6 pass. The top row requires Finalized false and observes 1, so H-RECORD-FIELDS[top] fails | VIOLATES H-RECORD-FIELDS[top] = model |
| B1-C44K (sigkill-b) | Top absent before and after. Row ZERO, Committed 0, package_set 0, 0 phases, `<none>` | Alternative 0 (unchanged) passes. kept false, so Committed 0 passes. Required package_set 1, 5 phases, open: H-RECORD-FIELDS[top] fails | VIOLATES H-RECORD-FIELDS[top] = model |

All five match.

### Notes

- T2-n1. Given that every final case has l2 AGREES on every compared field, clause equality mostly follows from family B: L1 is a function of the same fields. The exception is the `writes` field, which l2 marks N/A while L1 reads the LastUpdated proxy. The result is therefore a consistency check of the corrected evaluator on the corpus, not new product evidence. The follow-up says as much ("the observations IPM actually produced did not hit them").
- T2-n2. 39 of the 59 final-run cases have `l1_permitted` revised after their run (`l1_permitted_revised_after_run`). The follow-up reports this as R03e (27 return-b, all 12 T/K, both approx-b). Re-evaluation stays valid because `inputs` are unchanged in every final case.
- Incidental: B1-C46T's title, from the original submission, says "row reads Committed=0". `l2_predicted` and the observation both say 1.

Verdict: holds.

## 3. Load/transaction model (`FU/models/loadtx.py`)

### What I inspected

- `loadtx.py` 1–697 against the source:
  - mode resolution (`effective`) against `Module.cls:1165-1172, 1191-1196, 1328, 306-311`;
  - the activation structure against `:1224-1227, 1309-1351, 1357-1362`;
  - the history steps against `AbstractHistory.cls:51-62, 120-134` and `HistoryTemp.cls:9-73`.
- The flat-nesting reduction against `Module.cls:1368-1413` and `Storage/Module.cls:1109-1155`. The nested call has `IgnoreInstalled` and `UpdateSnapshots` killed (`Module.cls:309-314`), so the inner `LoadDependencies` skips installed dependencies.
- P-14 against `FR/native/20261113T234716Z-p21-ordering-r2/raw.log` and `…234651Z-p21-ordering` (N-P21B had `<SYNTAX>` there, superseded), against `expected-p21.json`, and against `uplift/ipm-verus-20261113/B/native-replay.md` §6 (N-P20 a–e, c2).
- P-06b/P-06c: `VV/premises.md:13-16`, `Premise.cls:191-232` (N-P10 shows only that `%OnClose` ran), and replay B1-C44T/C45T. Those two Terminate at `$TLEVEL` 1 and the top row reads Finalized 0 and package_set 0, so the close's PersistToTwin ran inside the transaction and was rolled back. That is native support for "close before rollback".
- The original round-three policy: `VV/crates/ipm-load-history/BINDINGS.md:82`, `uplift/ipm-verus-20261113/C/round-3/dispositions.md` R3-02, and `model.rs` `dep_commit_mark`, `dep_finalize`, `terminate`.

### Commands and key outputs

- Full rerun: `python3 -I -B followup/models/loadtx.py S/loadtx-rerun` took 1m41s, not about 10 minutes, with rc 0. Violations, end states, examples, correspondence (11/11 HOLDS) and the hazard row are all identical to `FR/20261113T235302Z-loadtx-model/loadtx.json`.
- Claim decomposition (`S/loadtx_claims.py`, output `S/loadtx_claims.out`):
  - As-built COMMITTED 30: all `COMMITTED[top]` (15 returned, 15 Terminate), all with a dependency's own NoJournal and a transactional top. Faults: top 12, dep2 10, dep1 8. Confirms MODELS.md and L-8.
  - REPAIR-103 COMMITTED 962 = 902 dependency + 60 top. The 902 split into 818 with the slot's own NoJournal (27 returned) and 84 in the window between mark and commit. No dependency violation falls in neither group. Confirms L-9 and the MODELS.md row.
  - GUARDED 141: all with some dependency's own NoJournal, 0 in the window.
- Window detail (`S/loadtx_window.py`): the 84 window violations are 60 (config, cut) end states but only 30 distinct (config, settled) states. Every window has a no-op `C*-late` step in REPAIR-103, so each state is counted twice. The cuts fall after `top:C2-mark` (56) or `top:C3-mark` (28).
- GUARDED with Committed also checked on ended, unfinalized rows (`S/loadtx_guarded.py`): 806 Terminate and 812 SIGKILL violations, all with a dependency's own NoJournal, none in the window. The REPAIR-103 REC-EXISTS 846 are all `REC-EXISTS[top]` at a caller rollback, as MODELS.md says.
- PREFIX independence (`S/loadtx_prefix_independence.py`):

  | Mutation | PREFIX | Other clauses |
  |---|---|---|
  | baseline | 0 | |
  | inner commit made durable | 1,553 | |
  | no rollback at process end | 8,632 | |
  | module writes unjournaled | 9,746 | |
  | top C3 commit a no-op, label kept | 324 | RETRY 3,786 |
  | top C3 commit and its label removed | 0 | RETRY 3,692 failures, COMMITTED 44 |

- Journaling order at a Terminate (`S/loadtx_jm_order.py`, output `S/jm_order.json`): same model, but at a Terminate each open NoJournal activation's flag is restored innermost-first, before the log that encloses it closes:
  - as-built REC-EXISTS 9,558 → 9,942 (+384, all Terminate, all `REC-EXISTS[dep]` with a dependency's own NoJournal);
  - COMMITTED unchanged (30);
  - REPAIR-103 and GUARDED unchanged (962/846, 141/846).

  The 384 extra end states are exactly the set by which M-BEFORE-IMAGE differs from as-built (`S/loadtx_claims.out` §1: "same set as M-BEFORE-IMAGE extras: True").

### Checks that hold

- Mode resolution. `effective()` ORs the caller flag, the top's `<Defaults>` and, for a dependency, its own `<Defaults>`, key by key. This equals the two `merge`s in a domain where a present key is always 1. NoJournal takes precedence over NoTransaction (`:1191-1196`). Dependencies are never in developer mode in the domain, which is right: `DeveloperMode` is overridden to the snapshot rule (`:307-311`). Native N-R05-PARDEF and Q23F/Q23S conform.
- Flat reduction. The source path above, N-R05-NESTF (d1 with its own NoJournal stays, d2 is rolled back, so d2 did not inherit d1's params) and N-R05-NESTS (row order m, d2, d1) all support it. A shared dependency is loaded once (N-R05-DIA, 4 rows, `s` second).
- P-14 against native evidence:
  - N-P20c (A, journaled B, rollback → A) and N-P21AN are consistent with both before-image and compare-and-restore.
  - N-P20c2 (journaled insert V, unjournaled U → U), N-P21A (→ C, before-image predicted A) and N-P21R (Committed 1, Finalized 1) refute unconditional before-image and fit P-14.

  P-14 remains an inference from single-node probes, as MODELS.md says. History rows are one `$lb` node in default storage, so whole-row granularity fits; index nodes were not probed.
- The 30 top-record COMMITTED violations and REPAIR-103's 818 dependency violations with a dependency's own NoJournal are real consequences of the policy text. Committed is marked only at a top commit or at a close with no load transaction open, and a NoJournal dependency's writes are durable before either.
- The Terminate window. P-06b's "close before rollback" is natively supported (N-P10 plus B1-C44T/C45T above), and P-14 is supported. Given both, an unjournaled dependency-row write after the journaled mark makes the mark survive the rollback, and GUARDED removes it.

### Findings

- T3-M1 (material, model fidelity). `run()` closes the open logs at a Terminate with the journaling flag in effect at the cut (`loadtx.py:354-360`). It never runs a `JournalManager`'s `%OnClose` first, although its comment cites P-06c ("innermost first").

  For a cut inside a dependency that runs NoJournal from its own `<Defaults>` under a transactional parent, P-06c and `JournalManager.cls:22-25` predict the following:
  1. The dependency's manager restores journaling before its log closes.
  2. The close's write is journaled.
  3. The rollback removes the row, whose insert was journaled inside the parent's transaction.

  The model writes unjournaled instead, so P-14 keeps the row.

  Consequences:
  - The as-built REC-EXISTS count is low by 384 Terminate end states.
  - The model-level "detection" of M-BEFORE-IMAGE (9,942 vs 9,558, MODELS.md) consists of exactly those 384 states. Under P-06c the mutant is indistinguishable from as-built in this model, so the refutation of before-image rests on native N-P21A/N-P21R alone. That native refutation stands.
  - REPAIR-103 and GUARDED counts are unaffected: they write dependency rows explicitly unjournaled.
  - No native case decides this. N-P19 cannot: the dependency inherited global NoJournal, so its manager restored "off". A Terminate inside a dependency's Activate where only the dependency's `<Defaults>` set NoJournal would decide it.
- T3-m2 (minor, interpretation of PREFIX). PREFIX is independent of the journal mechanics: all three store mutants are caught. It is not independent of the program:
  - Its durability points are the as-built trace's `top:C*-commit` labels (`l1_durable_writes`, `loadtx.py:437-489`).
  - It reuses the as-built mode resolution (`contract_mode` calls `effective(..., "as-built")`), adjusted only by Q-22/Q-23.

  A program that never commits passes PREFIX; only RETRY and COMMITTED catch it. "As-built PREFIX 0" (L-7) therefore says that IRIS's commit and rollback behavior delivers what the program's commit points promise. It does not say the commit points are right. Mode resolution is checked independently by N-R05, and where the original contract was specific, PREFIX agrees with it: default-mode rollback, developer-mode phase work kept. That is why this is minor.
- T3-m3 (minor, design reading). The window violations exist only because the follow-up's REPAIR-103 also finalizes dependency rows in the top log's `%OnClose` at a Terminate (`close()`, `loadtx.py:410-422`). The crate's variant 103 calls `dep_finalize` only at the normal `T_PERSIST` step; `terminate()` (`model.rs:719-725`) never calls it. The follow-up's reading is better grounded: a real implementation in `%OnClose` would also run at a Terminate (P-06b). But L-12 states it as "repair 103's close finalizes the dependency rows" without noting that this departs from the crate.
- T3-m4 (minor, counting). End-state counts are cut points, and cuts before no-op steps (`C*-mark` in as-built, `C*-late` outside the M-LATE-MARK mutant) repeat the same settled state:
  - as-built REC-EXISTS: 9,558 end states, 2,472 distinct (config, end, settled);
  - REPAIR-103 COMMITTED: 962 vs 303 distinct;
  - window: 60 end states vs 30 distinct.

  The follow-up never claims distinctness. Readers should not treat these as outcome counts.
- T3-m5 (minor). MODELS.md says the run takes "about 10 minutes"; it takes about 1m40s. The receipt's input hash `ffe7f6a6…` no longer equals the committed `loadtx.py` (`0dedac23…`) after a docstring-only edit in a later commit.
- T3-m6 (resolved during review). At `2183108e`, MODELS.md gave "Under Q-22 B: PREFIX 2,216 … Under Q-23 A: PREFIX 5,228". Those come from the superseded run `FR/20261113T233802Z-loadtx-model`, an older model hash `fd596dc4…` with 61,984 end states. The cited receipt says 2,440 and 5,780. HEAD `8c576c4b` now gives 2,440 and 5,780.
- T3-m7 (minor). `prefix_cases.py` hard-codes `original_permits: True` for its case 5 instead of computing it. That value is correct for kind 5/6 as written.

Verdict: holds with caveat.

| Claim | Status |
|---|---|
| Mode resolution | holds |
| Flat reduction | holds |
| P-14 | holds as an inference, as stated |
| As-built PREFIX 0 under Q-22 A / Q-23 B | holds, read as T3-m2 |
| As-built COMMITTED 30 | holds |
| REPAIR-103 818 / 27 returned / 60 top | holds |
| Terminate window | holds, with the design reading T3-m3 and double counting T3-m4 |

The process-end semantics omit the innermost-first `JournalManager` close (T3-M1). That hides 384 as-built REC-EXISTS violations and is the whole basis of the model's M-BEFORE-IMAGE detection.

## 4. History-reader model (`FU/models/history_readers.py`)

### What I inspected

- `where_as_built` and `as_built` (lines 101–181) against `History.cls:93-159, 162-230, 233-317` and `Main.cls:4127-4177`, and the filter construction in `CLI.cls:808-828`.
- `expected-r04.json`, `FU/native/sessions/r04.cos`, `FUNative/Obs.cls:113-171`, and the raw.log and verdict.json of `FR/native/20261113T234810Z-r04-history-readers`.

### Commands and key outputs

- Rerun: `python3 -I -B followup/models/history_readers.py S/hr-rerun` gives rc 0, output identical to the saved `history_readers.json`, valid neighbor 69/0.
- `where_as_built({'Version_Major':'1','package':'b'})` gives `[]` as-built and `[('package','=','b')]` under M-SKIP-INVALID. `sorted(['package','Version_Major','Package','TimeStart','action'])` gives `['Package','TimeStart','Version_Major','action','package']`. That is code-point order, which matches IRIS local collation for non-numeric string subscripts.
- Native N-R04-SEL: filter `Version_Major=1;package=fu-hr-b` returned `q.n=9`, every row including `zpm#1` and `testcoverage#3`, with no error. M-SKIP-INVALID predicts the 2 `fu-hr-b` rows and the contract 1. Only "the first key in collation order is the invalid one, so no conjuncts" yields 9 rows with no error. If `package` had sorted first, at most 2 rows would come back. The case discriminates.

  N-R04-SEL2 (`Package` first) returns 2 rows. It does not discriminate between the two models, and the follow-up does not claim it does. UNKNOWN gives 0 rows with `SQLCODE -29` and no exception. CUTOFF: the quoted comparison gives 0 rows, the unquoted one 1 (`cutoff=2026-10-14 00:00:00`). The 7 cases conform, against expectations committed before the run.

### Findings

- T4-m1 (minor, edge). `where_as_built` stops when a key has no value. ObjectScript's three-argument `$order(filter(col),1,value)` behaves differently: on a node with descendants but no data it most likely leaves `value` as it was. So `ConstructSQLWhere`'s `$data(value)#2=0` test only stops on such a key when it is the first one; later, it reuses the previous key's value. Reaching it needs `-Dzpm.a.b=…` (`CLI.cls:815-827`). This is not reachable through ordinary filters.
- MODELS.md's "predicts 1 row … as-built predicts every row" (the model's two-row archived DB) and `expected-r04.json`'s "would return the 2 fu-hr-b rows" (native fixture) are consistent; they describe different databases.

Verdict: holds.

## 5. Native evidence integrity

### What I inspected

`git log --format='%h %cI' -- VV/followup/native/cases`, the six follow-up native receipts, their `inputs/` and `input.cos`, `FU/native/run-native.sh` (all), the R05 fixtures (`FU/native/fixtures/r05/*/…/module.xml`, all 10 scenarios), `sessions/r05.cos` setup commands, and the R05 raw.log and verdict.

### Key outputs

- All three expectation files were committed only in `104fa5e2` at 23:46:46Z. The first run started at 23:46:51Z. The digest of the committed files at `104fa5e2` is `79ec0438…`. That equals the working tree, every run's `inputs/cases` digest and every receipt's `expectations_sha256_at_start`. Each run's `input.cos` equals its receipt's `script_sha256`. All six process outcomes are COMPLETED.
- P21 expectations recorded the before-image prediction (`N-P21A value=A`, `N-P21R committed=0`). They were kept, and the run reports DIFFER. The expectation was not revised after the observation.
- The R05 fixtures realize their case names. Shapes:

  | Case | Fixture shape |
  |---|---|
  | Q23F | dependency `<Defaults>` NoTransaction=1, parent `Invoke … Fail` |
  | NESTF | m → d1 (NoJournal=1) → d2, m fails |
  | NESTS | the same chain, all succeed |
  | PARDEF | parent `<Defaults>` NoTransaction=1, parent fails |
  | DIA | m → d1, d2 → s |
  | DEVUP | `load -dev d 1.0.0`, then m requires 2.0.0 |
  | DEVSAME | `-dev d 1.0.0`, m requires 1.0.0 |
  | HIGHER | d 3.0.0 installed, m requires exactly 1.0.0 |
  | HIGHEROK | requires `>=1.0.0` |

  All 10 CONFORM.
- Runner, `run-native.sh` (at HEAD):
  - IRIS_CONTAINER, IRIS_NETWORK and IPM_UPLIFT_WORK are required with no default (`:18-20`).
  - `up` refuses an existing container without the `ipm-quality-followup=1` label (`:62-65`) and checks the product tree hash (`:53`).
  - `down` removes the container only if labeled (`:145`).

### Findings

- T5-m1 (minor). `exec` (`:78-139`) runs `docker exec -i "$IRIS_CONTAINER" …` without the label check, so it would run a session against a foreign container named in the environment. `down` runs `docker network rm "$IRIS_NETWORK"` without an ownership check. README line 24 claims only "refuses to replace", which is accurate. The task's "refuses foreign containers" holds for `up` and `down`, not for `exec`.
- T5-m2 (minor). Fixtures are hashed (`inputs/fixtures.sha256`) but not copied into `inputs/`. They are committed, so the hash suffices.

Verdict: holds with caveat (T5-m1).

## 6. R12a reruns

### What I inspected

The receipts, command and raw.log of `FR/20261113T232529Z-r12-asbuilt-proofs`, `…232706Z-r12-control-13-verus`, `…232753Z-…-12-…`, `…232839Z-…-01-…` and `…232926Z-r12-sanity-false`, plus the 14 explorer receipts `FR/20261113T2330*`. Also `FU/bin/r12a-batch.sh`, `verus_check.py:56-76`, `VV/results/verus/20261113T222933Z-asbuilt-proofs-r3/inputs.sha256`, the original cold-build log `VV/results/verus/20261113T222035Z-review-r3-mutants/verus-control15.log`, and `vendor/explorer/Cargo.toml`.

### Key outputs

- The as-built raw.log has two summary lines, `2059 verified, 0 errors` and `263 verified, 0 errors`, and `verus_check.py` sums them (`v = sum(...)`). In the original cold build, `2059 verified` appears after `Compiling vstd` and before `Checking ipm-load-history` (lines 8, 17, 18). `explorer` has no dependencies. "2322 = 2059 vstd + 263 crate" is therefore right.
- `diff` of `inputs.sha256` against the original final r3 receipt shows a single difference: `crates/scripts/final-candidate-batch.sh`, a batch script and not a proof input. All crate sources and `Cargo.*` are identical.
- Controls:

  | Control | Outcome |
  |---|---|
  | control-13 | 10 Verus errors, including `case_c27_notx_failure_keeps_partial` and `lemma_lnm0_a/b/d` |
  | control-12 | 5 errors |
  | control-01 | 2 errors: `lemma_lnm0_b`, `case_c26` |
  | sanity-false | 1 error |
  | explorer `control-13-notransaction-ignored` | violation, L1-NAMESPACE on a NoTransaction dependency failure |
  | explorer `repair-103-dep-clauses` | PASS, exhaustive, 1,950 configs, 189,577 states |

  All 19 lines match their expectations.

### Assessment

The reruns establish that the round-three checks reproduce on the final proof inputs: control 13 detected by Verus and by the explorer, and repair 103 passing the crate's dependency clauses in the crate's global-mode domain. They do not re-review the Committed policy beyond that domain. That review is done by `loadtx.py` (L-9, L-12), and the follow-up's README §5 keeps the two apart correctly. Note T3-m3 for how the follow-up's REPAIR-103 differs from the crate's.

Verdict: holds.

## Verdict summary

| Correction | Verdict |
|---|---|
| 1. Corrected evaluator and projection | holds with caveat (T1-m1 malformed status permits; T1-m2 two untested checks) |
| 2. Re-evaluation "no verdict changed, clause sets equal" | holds (5/5 hand checks; it is a consistency result, T2-n1) |
| 3. loadtx model | holds with caveat (T3-M1 material; T3-m2 to m7 minor; T3-m6 already fixed) |
| 4. History-reader model and N-R04-SEL | holds |
| 5. Native evidence integrity | holds with caveat (T5-m1) |
| 6. R12a reruns and 2322 = 2059 + 263 | holds |

## Material findings

1. T3-M1. `loadtx.py` does not apply P-06c to `JournalManager` objects at a Terminate. With a dependency whose own `<Defaults>` set NoJournal under a transactional parent, the model keeps that dependency's History row through an unjournaled close plus P-14. The premise predicts that journaling is restored first and the row is rolled back.

   As a result, the as-built REC-EXISTS count is low by 384 end states, and the model's claimed detection of M-BEFORE-IMAGE consists entirely of those 384 states. The refutation of before-image restore stands on native N-P21A/N-P21R. No other material finding: the remaining items are minor (T1-m1, T1-m2, T3-m2/m3/m4, T5-m1) or already resolved (T3-m6).

## Scratch artifacts (`S/`)

- `mutate_checks.py`, `mutants/` (15 single-check mutants and `mutants-summary.json`).
- `adversarial_l1.py`.
- `controls-rerun/`, `loadtx-rerun/`, `hr-rerun/` (reruns, identical to the saved receipts).
- `loadtx_claims.py`, `loadtx_claims.out`.
- `loadtx_window.py`, `loadtx_guarded.py`, `loadtx_prefix_independence.py`.
- `loadtx_jm_order.py`, `jm_order.json`.
