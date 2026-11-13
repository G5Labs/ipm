# PairSoft E2: direct review versus full G5 uplift

**Stopped-run assessment, 15 October 2026, 15:08 EDT. The user stopped the experiment because it was taking too long.** The formal process was terminated, its process group was verified stopped, and its outputs were archived. The full uplift remains incomplete; no author work or further experiments were resumed. The original matched-ceiling result remains in [experiment plan §12](experiment_plan.md#12-first-pairsoft-e2-pair--15-october-2026).

The extended G5 run has produced stronger executable defect evidence and recovered additional consequential requirements. It has also consumed **9.12× the generated tokens and 9.85× the active author time** of the unchanged direct-review baseline. It is **not yet a demonstrated efficiency win, a completed whole-backend uplift, or evidence that TLC caused the additional discoveries**. The baseline still has a supported receipt-lifecycle finding absent from the current formal finding ledger.

## Comparable inputs, different effort

Both arms began with the same current-harness E1 ontology: 21 pages, 85 blocks and 5,374 body words. Both used GPT-6 Astra / extra-high, the same sanitized PairSoft code/history/docs, docs as important SOT, and permission to run real-code reproductions. The review baseline reconciled ontology against sources/code without formal modeling. G5 used the unchanged full uplift skill, TLA/TLC and reconstruction checks. No production repairs or harness changes were made during either run.

The baseline finished its report with partial coverage. The formal arm originally stopped at its token ceiling; the user then requested continuation to completion with no overall budget. Only the formal arm received that continuation. The historical comparison and the extended comparison answer different questions.

| Measure | Direct-review baseline | G5 at original stop | G5 at user-requested stop |
|---|---:|---:|---:|
| Generated tokens, including reasoning | 36,813 | 43,359 | 335,590 |
| Active author time | 25m 15s | 32m 15s | 4h 8m 46s |
| Ontology pages / blocks | 21 / 85 | 21 / 85 | 21 / 111 |
| Ontology body words | 5,603 | 5,545 | 7,545 |
| New defects reproduced in actual production components/helpers, evaluator-reviewed | 0 | 2 | **8** |
| Other supported static defects | 2 | 0 | Additional findings; full deduplicated adjudication pending |
| Reporting / coverage milestone | Finished report; explicitly partial coverage | Incomplete | Stopped; reconstruction/finalization unfinished |

Final token/time figures use the [stopped-run manifest](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop/manifest.json); contract counts use the [artifact audit](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop/artifact-audit.json). Resumed-thread token telemetry is cumulative: the original 43,359 is already included. Active time adds the original run to the continuation, excluding the idle interval. Evaluator/setup effort and monetary billing are not measured. Body-word counts include headings/navigation but exclude metadata; length is not a fidelity score. The extended contract is **34.7% longer than the baseline** and has not received its final compression/readability pass.

The [user-stop archive](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop) preserves the complete ontology, model/configuration files, checker snapshots/receipts, runtime probes and unfinished reports. Unchanged source inputs remain linked. The earlier [14:57 EDT interim snapshot](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/continuation-1/interim-20261015T185705Z/snapshot.json) remains separate historical evidence.

## What improved in the contract

There are concrete improvements beyond additional prose:

- **Missing rules become explicit.** Whole-number receipt input and aggregation across repeated PO-line rows now have precise commitments and failing production examples. Session failure backoff and project-optional budget aggregation are further source-supported obligations traced to implementation gaps.
- **Legitimate exceptions survive.** The extended author corrects O0's suggestion that every receipt save recopies mapped UDF values: accepted behavior distinguishes creation, PO changes, new lines and ordinary same-PO edits. It also recovers distinctions in template defaults, Required enforcement and catalog additions to Change Orders. These prevent a reconstruction from imposing unnecessary restrictions.
- **Broader capabilities were recovered.** The expanded contract adds owners for standard templates, import distinctions, accounting context, native identity lifecycle and approval notifications. These additions are documented with source/code evidence, but the complete new inventory has not yet received evaluator adjudication or a fresh-reader reconstruction test.
- **Source authority is sometimes handled better.** Direct review incorrectly resolves the login-batch question using earlier per-row decisions despite conflicting later transaction decisions. G5 keeps the narrower question open. This advantage came from reading and reconciling sources, not TLC.

The baseline also made valuable corrections, including requester-versus-buyer UDF defaults, exporter filtering and architecture exceptions. The extended formal run has independently recovered the UDF-default defect. It has **not yet reported the baseline's existing-draft receipt submission after PO closure/cancellation/invoicing**. Its current receipt contract names pending Change Orders but omits that submit-time terminal-state recheck. More models therefore do not establish uniformly better coverage.

## Eight component/helper reproductions in the reviewed subset

These are distinct findings grouped by shared cause. Multiple failing inputs or affected modules do not multiply the count. The evaluator inspected the production bindings, harnesses and saved results; this is evidence review, not an independent rerun or blinded panel.

| Finding | Concrete observed failure | Evidence boundary |
|---|---|---|
| Duplicate receipt rows evade cumulative quantity limit | Two rows each fit the remaining quantity although their sum exceeds it | Actual storage validation; EF InMemory and explicit context/identity doubles |
| Fractional receipt quantity is accepted | Actual submit processes `0.5` despite accepted whole-number intent | Actual aggregate/state/event path; no complete handler/SQL pipeline |
| GL distribution uses excessive precision | `1.25 × 1.23 = 1.5375`; a complete cent-rounded allocation of `1.54` is rejected | Actual value objects and both distribution validators; one shared cause |
| Projectless budget excludes project actuals | Matching projectless amount 2 plus project amount 3 returns 2 instead of 5 | Actual repository and amount calculation; EF InMemory |
| UDF mapping source eligibility is not validated | Compatible inactive or other-company source passes the actual validator | Actual validator method plus real-caller inspection; no HTTP/SQL or cross-company transaction-data leak claim |
| Assistant logs its configured API credential | Dummy configured credential reaches the Warning logger | Actual client/serialization; intercepted HTTP and capturing logger, no real secrets or logging sink |
| CSV serialization corrupts values | `a"b` becomes `ab`; embedded newline splits one record into two | Actual formatter plus independent Python CSV parser; no full exporter/spreadsheet run |
| Nullable export filters fail conversion | Nullable date/decimal comparisons and nullable-date `isnull` throw | Actual filter helper and matching synthetic row types; no EF/grid/HTTP execution |

The [formal divergence ledger](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop/tla/experiment_output/divergences.md) records these as E2-01, 02, 17, 18, 25, 46, 48 and 50. [Evaluator working notes](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop/evaluator-working-notes.json) preserve additional source/harness checks. None is a full deployed E2E reproduction.

The [baseline ledger](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/review/experiment_output/divergences.md) supports two static defects: manual PO header defaults use the buyer instead of the originating requester; an existing draft receipt can submit after the PO becomes terminal. It did not execute reproductions of those defects. Both methods also identify a conditional stale-approval outcome race. Disclosed O0 defects, unresolved policy choices, deliberately broken models and model mistakes earn no new-defect credit.

Additional extended static findings include shared loading-flag interference during mass-change Save and a selected-account fallback that omits the GL query's company restriction. These have evaluator source corroboration. Many other author entries combine recovered intent, implementation observations and conditional assumptions; their entry count must not be presented as a validated bug count.

## What the process and TLC contributed

**The clearest observed benefit is the workflow's sustained reconciliation and coverage pressure.** It drives investigation beyond the initial summary, including alternate writers, original exceptions and fields crossing service boundaries. The out-of-office investigation illustrates this: a downstream storage check honored the start date, but later ingress inspection found that an upstream writer erased it. The author corrected the earlier correspondence claim and modeled the fuller path.

**TLA/TLC adds a replayable, explicit argument for selected obligations.** Models force choices about current versus original actors, aggregate versus per-row quantities, stale versus current submissions and asynchronous completion. Saved configurations, counterexamples, valid-behavior witnesses and deliberately broken controls make those arguments inspectable and mechanically repeatable within stated bounds. The stopped artifact has 75 model files, 772 configurations and 960 checker receipts; these include retries, controls and model failures, not 960 bugs or verified requirements.

**The stronger bug confirmation comes from executing production code.** For all eight reproduced findings above, the ledger records the initial hypothesis during source/code inspection or specification construction before the relevant checker execution. We therefore have **no demonstrated TLC-first discovery among these eight**. Their runtime probes were allowed in both arms. Formal traces help check the hypotheses and candidate obligations, but the current experiment does not show that a similarly persistent nonformal agent could not find or reproduce them.

**Formalization also creates its own failure modes.** An apparent UDF compatibility failure was retracted after the author found a missing event-handler effect in the model. Several checker failures were syntax/framing mistakes. The OOO example and the baseline-only receipt finding show how a correct model of an incomplete path can leave important behavior unchecked. External transaction, broker, identity and widget assumptions remain material limits.

## Stopped-run answer: which is better, and how much?

On the selected checked cases, the extended G5 artifact offers **more recovered behavioral detail and substantially stronger executable evidence**. It has eight component/helper reproductions versus none in the baseline, alongside additional supported static findings. That is an evidence-strength difference, not an eightfold accuracy score or proof of eight unique advantages caused by TLC.

The baseline is **much cheaper, shorter and already has a finished partial report**. At the original comparable ceilings, each method found two different supported defects; G5 took its two to component execution, but stopped incomplete. After continuation, G5 has spent 9.12× the tokens and 9.85× the time and still misses at least one baseline finding. There is no measured whole-contract fidelity/coverage percentage, fresh-reader result or causal separation of skill, extra effort, formal construction, TLC and runtime testing.

The defensible conclusion is: **extended G5 is yielding a stronger candidate contract and evidence package, at substantially higher cost; reliable overall superiority and cost effectiveness remain unproven.** The user chose to stop before the full uplift checkpoints closed. This result must remain an incomplete investigation, not be converted into a completion claim by editing its reports. No additional baseline run, E3 experiment or downlift was performed. The subsequent harness adjustments below were not part of the measured runs.

## Stop verification and artifact limits

The author stopped at **2026-10-15 19:07:58 UTC**, with exit −15 following the explicit user stop. The continuation lasted 3h 36m 31s and generated 292,231 additional tokens; cumulative formal usage is 335,590. Reasoning tokens are included, not added again. The [stop request](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop/user-stop.json) and manifest distinguish this from a budget cutoff or normal completion.

The evaluator checked both arms' supplied inputs: **zero source or immutable-input changes**. The stopped formal contract retains all original IDs, adds 26 blocks and changes 29 original block bodies/kinds plus seven metadata-only entries. Its links resolve, but the structural audit reports four body/metadata ordering mismatches and three Q entries without the required uncertainty metadata. These unfinished author artifacts are preserved rather than repaired after the stop. The coverage report and final review still identify unfinished work; raw model counts cannot close those checkpoints.

The checker audit found a matching execution fingerprint for every final model/configuration pair: no final configuration is unexecuted or stale. This is stronger artifact freshness than at the original budget stop. It is not a semantic coverage verdict. The 960 wrapper calls consumed **711.6 seconds, about 11m 52s**, versus 4h 9m active author time. The major wall-time cost was therefore outside checker execution; model construction, source investigation, reconciliation, tests and orchestration are not separately timed. The [archive verification](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop/archive-verification.json) confirms all 2,907 fingerprinted artifacts match their saved copies.

## Incremental harness adjustments — applied after the experiment

**Latest direction:** retain the full uplift workflow, including meaningful executed TLA/TLC checks for all important in-scope behavioral obligations and reasoned dispositions for structural items. The user declined changing this direction, then authorized the three incremental adjustments below. They were applied on 15 October 2026 to the maintained harness; the experiment remains stopped and its frozen inputs and outputs are unchanged. This supersedes the earlier recommendation here to make formal verification selective.

The experiments demonstrate specific failure modes that justify incremental adjustments. They do **not** demonstrate that the following amendments improve outcomes: none has received a matched comparison. The existing skills already require conciseness, deduplication, independent coverage discovery, faithful code bindings and current evidence. The edits clarify execution checkpoints in the existing guidance without adding another report layer.

### 1. Reconcile the complete code path before accepting a model's implementation claim

**Observed:** the apparent UDF inactive-target defect was retracted after discovering a mapping-clear event handler omitted from the model. The downstream out-of-office storage model correctly respected the start date, while upstream code erased that input. These are concrete failures of code correspondence, even when a model is internally correct. The baseline-only receipt defect additionally demonstrates that Create/Save checks cannot establish Submit behavior after a PO state change.

The [uplift skill, phases 2–3](../../../.agents/skills/ontology-uplift/SKILL.md) and [model-checking guide](../../../.agents/skills/ontology-planning/references/model-checking.md) already require alternate-writer inspection and faithful bindings. The amendment makes the evidence needed to accept a claim explicit:

> Before accepting a counterexample as a code gap or a pass as implementation conformance, reconcile the relevant input producers, lifecycle entry paths, alternate writers, event-handler effects and consumers. Link that evidence in the existing property/coverage record; identify untraced boundaries and keep the claim conditional. Revisit affected bindings when later discoveries change the path.

This is a claim-acceptance checkpoint, not a requirement to finish whole-system inspection before constructing any model. It preserves iterative discovery and the full formal scope. It cannot establish that every relevant path has been found.

### 2. Integrate each completed concern into the existing contract and evidence

**Observed:** the run accumulated 75 model files, but the coverage report, issue index and final-review draft lagged its investigation ledger. The stopped ontology also had four body/metadata ordering mismatches and three Q entries without uncertainty metadata. The user stopped an unfinished run, so these are intermediate artifact defects, not evidence that the author would have declared them complete. They nevertheless made completed work harder to assess and reuse.

The skill already said to update owning ontology and code gaps together; the [execution guide](../../../.agents/skills/ontology-uplift/references/execution-plan.md) calls for a small, current artifact set. The amendment specifies the integration boundary:

> When closing a concern or switching to an unrelated domain, reconcile its owning ontology block, Q/metadata, canonical finding and coverage disposition. Keep unresolved work explicitly open. Refresh the current milestone and derive duplicate indexes from canonical records where practical. Check changed ontology structure and links at that boundary.

This does not require a report rewrite after every TLC execution or force an unresolved concern to finish before other work can proceed. Cheap deterministic checks could detect the demonstrated metadata defects; they cannot assess semantic completeness or minimalism. Reuse the evaluator's checks where suitable instead of adding another narrative checklist. Useful partial artifacts still do not waive the final uplift checkpoints.

### 3. Bring the independent reconstruction challenge forward

**Observed:** important numbering, PO-terms and native-identity obligations were still being recovered late in the continuation, while the receipt terminal-state submission defect remained absent at the stop. Deep model campaigns had not established broad coverage. E1 also left important retention probes absent across all three recovery methods.

The harness already establishes an independent capability inventory in phase 1 and a reconstruction challenge in phase 4. The adjustment is timing:

> Challenge an initial domain contract against independently gathered operations and source expectations before deepening its model campaign. Record missing obligations and uninspected lifecycle operations in the existing coverage map, and revisit them when discoveries expand scope. Retain the full final reconstruction pass.

This adjustment has weaker support than the first two: late discovery demonstrates remaining breadth work, but does not prove that earlier scheduling would have found those omissions sooner or reduced total effort. It introduces no all-files prerequisite, fixed number of passes, or requirement for one model per operation.

### Changes not supported by these results

The [E1 amendment comparison](pairsoft_e1_intent_comparison.md#7-fresh-current-versus-amended-g5-recovery) did not establish that more conciseness/reconstruction instructions improved the contract; that amendment was reverted. These results do not justify hard word limits, forced compression that loses rules, a new approval process, or a time/model-count cutoff. Nor does the absence of TLC-first discovery among the eight reviewed reproductions establish that repeatable formal exploration adds no value.

**Implementation:** updated the existing uplift entrypoint, execution guide and shared model-checking guide, and synchronized their maintained skill copies. The artifact checkpoint calls for lightweight checks using available validators; no new validation tool or report format was introduced. Comprehensive TLA/TLC and the existing completion standard remain intact. These instruction changes have not been evaluated in another author run; their benefits, including earlier reconstruction, remain unmeasured.
