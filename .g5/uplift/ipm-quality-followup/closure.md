# Closure ledger: follow-up review of the IPM Verus uplift

This ledger governs the follow-up review of the original IPM Verus uplift. It lists each audit finding R01–R12 from the operator audit (`$IPM_REVIEW_AUDIT`, 2026-11-13), its subcases, whether this follow-up accepts or defers each subcase and why, the evidence that will close it, and its current status. Status changes are appended under "Revisions" with the commit that made them.

| Item | Value |
|---|---|
| Original submission | `ae61979578ecbc23d47b3af44c076c105663f38c` (branch `experiment/ipm-verus-quality-review-20261113`). Its files and receipts stay as committed. A file this follow-up replaces is kept in git history at that commit and listed under "Superseded files". |
| Product pin | `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, read-only |
| Follow-up record | `.g5/uplift/ipm-quality-followup/` (this ledger, plan, audit validation, review record, handoff) |
| Follow-up verification | `.g5/Verus_verification/followup/` (corrected evaluator, models, tests, new receipts and verdicts) |
| Owner files | `.g5/Verus_verification/bugs_issues.md`, `design_simplification.md`, `open_questions.md` |
| Run environment | `IRIS_CONTAINER`, `ORAS_CONTAINER`, `IRIS_NETWORK`, `IPM_UPLIFT_WORK` from the session environment. No other container, network or image tag is created, changed or removed. |

Status values: OPEN (not started), WORKING, CLOSED (closure evidence exists and is linked), PARTIAL (some subcases closed; the rest are listed with reasons), DEFERRED (not done in this follow-up; reason given).

## Ground rules applied to every row

- Validate the audit's claim against the original sources first and record the result in `audit-validation.md`. A refuted or narrowed claim is recorded as such, not silently dropped.
- Documentation alone does not close a gap that can be repaired technically within the permissions of this follow-up. It closes a reporting gap or records a residual.
- No product file, shipped test, configuration, dependency or shipped doc is changed. No product fix is implemented.
- For a security-sensitive or potentially destructive behavior (history deletion that widens, record loss, process termination), use static review, the archived observations and synthetic model or receipt cases. Do not re-create the harmful outcome natively. New native runs are ordinary, non-destructive, narrowly scoped and run only in the dedicated container.
- Old receipts and verdicts stay where they are. New verdicts go to a new evidence directory.

## Ledger

### R01: native L1 evaluator accepts observations the Verus clause rejects

Accepted. Primary class: A1, V5.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R01a | kind 1 `installed`: `installed_ok` requires `m.res`; `slot_installed` ignores `resources_loaded` | accept | corrected predicate; negative control on saved B1-C01 with `resources_loaded=0` must FAIL `L1-NAMESPACE` |
| R01b | kind 2 `manifest_only`: Verus requires `m.doc == e.ver`; Python omits the document version | accept | corrected predicate; paired control (doc equal / doc different) |
| R01c | kind 0 `unchanged`: `slot_unchanged({}, slot)` is true on missing data; Verus `m == i` also covers `wr` | accept | three-valued predicate (missing → UNKNOWN); `wr` bound to the `LastUpdated` proxy and labeled as a proxy |
| R01d | kinds 3–6 (`any`, `manifest_present`, `unchanged_or_present`, `unchanged_or_installed`) | accept | kind 6 inherits R01a; each kind gets a positive and a negative control |
| R01e | write count and its proxy semantics | accept | projection table: `writes` is not observable, `LastUpdated` is the proxy, with its premise and the cases where it decides a clause |
| R01f | unobserved slots and alternative selection | accept | report per alternative and per slot; slots without a native counterpart labeled "not realized"; Verus checks at most three alternatives, Python checks all (record whether any case has more than one) |
| R01g | bind the logical predicate to the observable projection explicitly | accept | decoder from `l1_permitted` back to the Verus `MExp`/`RExp` codes with a byte-exact round trip, and a differential test: the corrected evaluator applied to each case's `l2_predicted` must reproduce the model's own `violated_clauses` |
| R01h | L1-only controls, separate from L2 detection | accept | controls report L1 and L2 detection separately; a control counts for L1 only if the L1 verdict changes |
| R01i | re-score existing receipts case by case | accept | new verdicts for every replay run in a new evidence directory, with an original versus corrected table |

### R02: Committed expectation and clause identity in native aggregation

Accepted. Primary class: A1, V5.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R02a | `kept` uses all slots for every row; Verus uses all slots for the top row and only slot *i* for dependency *i* | accept | per-slot `kept`; a contrast pair where the parent changes and a dependency does not, and the reverse |
| R02b | a qualified row requirement (`ex = 2`) accepts either presence, and row fields are skipped when absent | accept, after checking it against Verus | check that `ex = 2` and `rec_fields_ok` really are vacuous when the row is absent; keep the Q option and clause identity that produced `ex = 2`; distinguish a missing observation from an absent row |
| R02c | aggregation compares one boolean, so a different failed clause can look like agreement | accept | compare the set of violated clauses per case with the model's `violated_clauses`; report exact, superset, subset or different |
| R02d | validate individual failed clauses | accept | one synthetic negative control per clause family (`L1-STATUS`, `L1-NAMESPACE`, `H-RECORD-EXISTS`, `H-RECORD-FIELDS`, `H-COMMITTED`) per slot kind |

### R03: incomplete evidence accepted too readily

Accepted. Primary class: A2, V7.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R03a | `changed_inputs` is computed but not used in `complete` | accept | changed per-case inputs make the case STALE, which counts as needing new execution |
| R03b | only IDs observed or begun are evaluated, so a scheduled case can vanish | accept | the run's schedule is parsed from `input.cos`, bound to `script_sha256` in the receipt, and every scheduled ID gets a verdict |
| R03c | repeated measurements overwrite each other | accept | parser keeps every value; a conflicting repeat makes the field CONFLICT |
| R03d | return code zero says nothing about completeness | accept | per-case BEGIN/END, setup markers, `NATIVE-END`, receipt outcome and environment flags are all required |
| R03e | expectations revised after observation | accept | per-case provenance: the `l1_permitted` revision snapshotted before the run versus the one evaluated; revised cases flagged |
| R03f | bind driver and result revisions | accept | the helper hash in each receipt is compared with the tracked helper sources; the evaluator's own hash is written into each verdict |
| R03g | tests with copied or synthetic receipts: missing case, missing field, stale input, conflicting evidence | accept | a test suite that copies a real run into a temporary directory, mutates it, and asserts each outcome |
| R03h | list the cases that need new ordinary execution | accept | list in the re-evaluation summary, and run the ordinary ones that fit the permissions |

### R04: history consumers are not modeled

Accepted. Primary class: A2, V1.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R04a | `GetHistory` and `history find` filter semantics, order and limit | accept | bounded model of records, filters and reader results; as-built (`ConstructSQLWhere` stops at the first unusable key) versus contract |
| R04b | selection versus action: `history delete` and `DeleteHistory` affect exactly the selected set; unrelated rows preserved | accept | affected and preserved sets in the model; the widening counterexample is reproduced only in the model and from the archived evidence |
| R04c | retention purge: `AutoPurgeOldHistory`, the cutoff, once per day, every namespace, active rows | accept | model of the cutoff comparison and of its scope; archived evidence for the no-op; an ordinary positive native case that only reads |
| R04d | global enumeration: `GetHistoryGlobally`, `GlobalRecords`, `DeleteHistoryGlobally`; row identity across namespaces | accept | model with two namespaces and colliding IDs; archived H10 evidence |
| R04e | `history details` and the query result metadata (`CONTAINID`) | accept | static review plus model of row identity |
| R04f | a deliberately wrong model variant and a valid neighbor | accept | the wrong variant must be rejected by the checks; a valid neighbor (a correct filter) must pass |
| R04g | narrowly scoped ordinary positive native cases | accept | native run in the dedicated container on test records only; no delete-all and no widening reproduction |

### R05: one global transaction mode; dependency defaults, nesting, installed state

Accepted. Primary class: A2, V4. The maintainer decisions in Q-22 and Q-23 stay open.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R05a | a dependency's manifest `<Defaults>` versus the parent and caller settings | accept | per-activation mode in a composable model; trace of how the mode is resolved at each activation, with file:line |
| R05b | recursive dependencies (depth 2) and a shared dependency (diamond) | accept | model instances, or a sound reduction argued from the source |
| R05c | a dependency already installed at another version or in developer mode | accept | model instances and ordinary paired native fixtures |
| R05d | alternate dependency sources (remote registry, ORAS) | defer the native part | needs registry services and publishing; disposition from source review, recorded as not observed |
| R05e | inherited transaction levels (a caller transaction around nested activations) | accept | model instances |
| R05f | Q-22 versus Q-23 affect different guarantees; keep policy alternatives explicit | accept | the model is parameterized by the decision; each option's verdict is shown |
| R05g | ordinary paired fixtures on the pinned product, including the unobserved Q-23 case | accept | native runs of ordinary loads only |
| R05h | quantify the abstraction; check interactions outside the single-fault domain where they change a conclusion | accept | state counts and bounds; two-fault cases where a conclusion could change |

### R06: process-end modeling and the repair ordering

Accepted. Primary class: A2, V4. New native process terminations are not run (see ground rules). Archived T/K observations and synthetic cases are used. Approximate status stays where no exact native witness exists.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R06a | `l1_alts_kill` returns `any_alt` for non-refusal opt-out process ends | accept | an effect-prefix clause: what may remain is the set of effects written before the end point |
| R06b | developer-mode process end admits broad alternatives | accept | the same effect-prefix clause for developer mode |
| R06c | cleanup timing collapsed: transient (before the slot rollback), settled, returned, caller rollback, retry | accept | a model that separates these observation points; archived in-flight observations where they exist |
| R06d | a permitted partial prefix and a rejected impossible state | accept | synthetic positive and negative cases for the new clause |
| R06e | the repair (variant 103) relies on write ordering beyond the R-01 probe | accept | re-derive the ordering the repair needs, compare with N-P20 sub-results (including the refuted c2), and state what stays conditional |
| R06f | the two approximate interruption cases (C49T/K) | accept as residual | keep APPROXIMATE; state what an exact witness would need |

### R07: granularity and environment limits

Accepted. Primary class: A2, V6.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R07a | inventory every material projection and the exact property that survives it | accept | projection inventory table |
| R07b | strengthen the high-impact abstractions with source or ordinary test evidence (producer versus final reader, phase order, multiple operations, persistence) | accept | per projection: evidence added or the premise left conditional |
| R07c | concurrency, restart, files and registry effects, global progress | accept as explicit residual | listed as not established, with the reason |

### R08: broad intent recovery versus deep formal scope

Accepted. Primary class: A2, V7. No whole-product proof is attempted.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R08a | a disposition for every A obligation and observation | accept | scope ledger over the A1/A2 registry |
| R08b | a disposition for every partial or unestablished coverage-map row | accept | same ledger |
| R08c | in-scope parts of other load paths (write check, git and legacy XML, temporary repository cleanup, lock files, namespace defaults, dependency selection) traced to operations, commits and readers | accept | trace table with file:line and baseline evidence |
| R08d | separate per-feature scope exclusions from regressions | accept | column in the scope ledger |

### R09: bugs and observations not comprehensively reconciled

Accepted. Primary class: A1, V7.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R09a | reconcile every ID from A1/A2 (34 OBS rows), model counterexamples, native reports, issues, reviews and prior dispositions | accept | rewritten `bugs_issues.md` with an ID reconciliation appendix |
| R09b | one row per unique finding or explicit alias, deduplicated by cause | accept | same |
| R09c | supported / conditional / suspected / withdrawn / out-of-scope; decision-dependent defects and issues without a governing requirement counted separately | accept | same |
| R09d | expected versus observed, evidence, impact, human decision, missing evidence, proposed direction; no operational misuse detail | accept | same |
| R09e | sample widely across the whole intent inventory | accept | sampling record in the review folder |

### R10: design simplifications incomplete

Accepted. Primary class: A4.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R10a | S5 and S6 lack discriminating checks; S4 lacks a worked preserving and refusing pair | accept | added checks or pairs |
| R10b | introduction says none was modeled while S2 overlaps repair variant 103 | accept | corrected statement with the exact model evidence |
| R10c | S1 claims to make Q-17 moot although adopting it chooses policy | accept | reworded, with the decision left with the maintainer |
| R10d | temporary repository, cross-namespace and dependency-flag findings lack a simplification disposition | accept | explicit entries or a reason for no separate simplification |
| R10e | per proposal: owner, old complexity, what gets simpler, preserved behavior, tradeoffs, decisive neighbor case, unresolved choices, model evidence | accept | rewritten `design_simplification.md` |

### R11: no owner-facing uncertainty file

Accepted. Primary class: A1, V7. Maintainer decisions are not made by this follow-up.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R11a | one section per current Q and per new question, linked to its canonical ontology block | accept | `open_questions.md` |
| R11b | competing interpretations, authority, the decision requested, affected bugs, stakes | accept | same |
| R11c | separate policy questions, unknown platform behavior and missing test evidence | accept | same |
| R11d | resolved and renamed questions; one-to-many historical IDs | accept | same, with a crosswalk |
| R11e | statuses in historical notes that lag the final evidence | accept | listed in the crosswalk; historical files are not edited |
| R11f | research each question through code, history, docs and runtime tests where possible; give the most reasonable assumption and alternatives | accept | per question: what was checked and what it showed |

### R12: final round repairs not independently re-reviewed

Accepted. Primary class: A2, V7.

| Sub | Subcase | Decision | Closure evidence |
|---|---|---|---|
| R12a | freshly check the round-three repairs (opt-out retention clause and control 13; the D1/D10/S2 Committed policy and variant 103; Q-22/Q-23 split; Q-15 options) on the final files | accept | rerun of the affected proofs, controls and explorer runs on the final inputs, with receipts |
| R12b | one representative end-to-end trace and its distinguishing control on the final files | accept | trace from source clause to proof, case, native observation and corrected verdict |
| R12c | round-three report versus handoff on what was independently reproduced | accept | reconciliation note |
| R12d | exact/approximate and expected/observed replay fields stay visible | accept | the corrected verdicts carry them |
| R12e | an independent check of this follow-up's material corrections, with exactly what it covered | accept | fresh reviewer with its own record under `review/` |
| R12f | separate counts: formal proofs, controls, exact native cases, questions, product findings | accept | handoff |

## Status

| Finding | Status | Closure evidence | Residual |
|---|---|---|---|
| R01 | CLOSED | `Verus_verification/followup/replay/` (`l1sem.py`, `evaluate_replay2.py`, `PROJECTION.md`); controls `followup/results/20261114T002645Z-final-replay-controls` (A 175, B 59, C 45, D 14, all pass); re-evaluation `followup/results/20261114T002646Z-final-replay-reeval` (`delta.md`) | `wr` stays a proxy (`LastUpdated`), and `res` stays a one-class projection (R07) |
| R02 | CLOSED | per-slot `kept`, per-clause comparison with `violated_clauses`, controls "Committed per slot", "qualified row", "clause identity" | R02a and R02b are latent in the saved corpus (no observation exercises them) |
| R03 | CLOSED | schedule, markers, conflicts, staleness and provenance in `evaluate_replay2.py`; evidence controls D1–D12. R03f/R03h: the 45 return-path cases (B1-C01..C43, C51, C52) were re-executed with the committed driver bound at start (`followup/results/native/20261114T000144Z-replay-return-rerun`, `…000251Z-replay-c51-c52-rerun`); every observed field equals the archived run and the corrected verdicts are unchanged (`followup/results/20261114T002647Z-final-replay-rerun-compare`) | the twelve T/K process-end cases and the two approximate C49 cases were not re-executed (process termination; see ground rules), so their driver revision stays unrecoverable. No case is stale |
| R04 | CLOSED | `Verus_verification/followup/models/history_readers.py` (receipt `followup/results/20261114T002647Z-final-history-readers-model`): distinguishable rows in four namespaces, filters, find, details, delete, global readers, purge; wrong variant M-SKIP-INVALID refuted natively, valid neighbor 69/69; native `followup/results/native/20261113T234810Z-r04-history-readers` (7 cases conform) | R04d native: no second IPM namespace in the dedicated container, so global readers rest on the archived H10 run and the model. Widening delete and purge deletion not re-created (destructive); their selection observed read-only |
| R05 | CLOSED except R05d | `followup/models/loadtx.py` (receipt `followup/results/20261114T004842Z-final-loadtx-model`): per-activation mode resolution, nesting reduction, Q-22/Q-23 options, repair variants; native `followup/results/native/20261113T234817Z-r05-modes-nesting` (10 cases conform, including the first native Q-23 case, nesting, diamond, developer-mode and higher installed dependencies) | R05d deferred: remote or ORAS dependency sources not run (needs registry services). R05h: interaction checks are mode × fault × process end × retry; two simultaneous faults not modeled |
| R06 | CLOSED except R06f | PREFIX clause in `loadtx.py` and `followup/models/prefix_cases.py` (receipt `followup/results/20261114T005359Z-final-prefix-cases`): permitted partial prefixes and three rejected impossible states; transient, settled, returned, caller rollback and retry separated; repair ordering challenged natively (N-P21, premise P-14) and in the model (Terminate window, guarded variant) | R06f: B1-C49T/K stay APPROXIMATE. An exact witness needs a hook between the C3 commit and `SetCommitted`, which the product does not have. No new native process end was run (destructive) |
| R07 | CLOSED | `Verus_verification/followup/PROJECTIONS.md` | concurrency, restart (P-12), effects outside the database (Q-04), registry effects and liveness remain open, listed there |
| R08 | WORKING | | |
| R09 | WORKING | | |
| R10 | CLOSED | rewritten `Verus_verification/design_simplification.md` (S1–S9, each with owner, old complexity, preserved behavior, decisive pair, choices and exact model evidence) | S1, S5, S6, S7 not modeled as variants; their decisive cases are named |
| R11 | WORKING | | |
| R12 | WORKING | R12a: 19 reruns on the final inputs, all as expected (`followup/results/20261113T232529Z-r12-asbuilt-proofs` .. `20261113T233034Z-control-01-no-setcommitted-c1`); R12b: trace and controls in `Verus_verification/followup/README.md` section 5; R12c and the suite headline: `audit-validation.md`; R12d: corrected verdicts keep exact/approximate and expected/observed per field | R12e independent review and R12f counts pending |

## Superseded files

None yet.

## Revisions

- Initial ledger, written before any artifact was revised. R01a–c and R03c were checked against the saved evidence before writing (see `audit-validation.md`).
- R01–R03 closed. R03f closed for the return path by re-execution with the committed driver (45/45 identical); the T/K cases keep the residual. R02b narrowed: the qualified-row handling matched Verus, and the gaps were missing-data handling and the lost Q identity. R03d narrowed: the original did not rely on the return code alone.
