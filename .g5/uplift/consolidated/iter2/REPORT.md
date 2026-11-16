# IPM consolidated uplift iteration 2

The foundation improved on the known repair queue and gained narrow Verus representation of consequential registry, publication and final-reader paths. Fresh results remain exploratory and show much lower coverage than known-panel closure. No product behavior, benchmark HTML, grading rules or shared workflow guidance was changed.

## Checkpoints and preservation

Work began on the actual `consolidated_uplift` tree at HEAD `0ac7b931fe6245366ae6c873393dabf1fc9590ca`, including staged, modified and untracked iteration 1 artifacts. Product directories match `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. Both inherited foundations and all iteration 1 receipts remain byte-identical. The existing Git index is unchanged; nothing was reset, cleaned, committed or pushed.

[Baseline receipt and recovery instructions](../../../../../results/experiments/uplift-measurement-20261114/ipm-iter2/baseline/receipt.json) preserve 5,267 fingerprints, binary staged/working patches, index and recoverable archive. Archive SHA256: `a09f539c6f2dc9adf73a8714ab148488d9ee8895fc7db3752e426e75e1539ba3`. Only `.git` objects and the vendor guidance checkout are excluded; the actual index is separately saved. Inherited absolute scratch links are preserved for recovery but skipped during disposable assessment extraction.

The candidate was sealed at 17:44:04 UTC before evaluator inspection: [receipt](../../../../../results/experiments/uplift-measurement-20261114/ipm-iter2/candidate/receipt.json), archive SHA256 `efd9edbe48757941e9f51787cc313a9bde50b99eb37b1fab281135e1407fd8ca`. Its 3,254 answer/formal files remain unchanged after sealing. Later work only adds receipts, assessment and reports. Eight existing canonical pages changed relative to the actual baseline. [Validation](validation.json) records exact preservation, IDs and links.

## Five improvement passes

These are five work packages at one product revision. Boundary and independent source-review lanes ran concurrently; canonical reconciliation was integrated before the candidate seal. They are not five historical product revisions or separately graded intermediate checkpoints. [Pass ledger](five-passes.json) records misses, challenged credit, sibling writers/readers, receipts and remaining scope.

| Pass | Consequential repair or discovery | Challenge and boundary |
|---|---|---|
| 1 | Canonical restart distinction; real process SIGKILL versus whole-instance restart | A process-loss observation cannot establish restart behavior. Temporary progress survives SIGKILL but is lost on restart; the durable initial permanent row remains unfinished. |
| 2 | Accessible and transaction-locked active history through delete/finalize/close | Accessible delete retains absence; the locked schedule recreates the same finalized successful ID at temporary close. Broad no-resurrection inference withdrawn. |
| 3 | Actual lock-manager constructor at BuildCache return; real CPF final configuration | Default immediate mode releases explicit lock by return despite an outer transaction; deferred mode waits for commit. Valid role creation survives rollback, invalid merge exits7 with no role. |
| 4 | ORAS provider-order selection, first-tar integrity, retained-manager cache divergence and partial publication state | Availability is not permission. Provider status and external effects are independent; a failed helper stream is blocked by final GetModule status checking. |
| 5 | Report failure/count/readers, verify setup plan, forced uninstall traversal, canonical cycles/alias/missing-source repairs | Preserve resolving chain and valid alias; keep nested-object and primitive-array GetArg neighbors. Report composition does not establish every format or verify cleanup. |

[Boundary report](boundaries/REPORT.md) includes repeat schedules, actual final-row identity, two clean disposable replays, platform premises and excluded observer-contaminated attempts. Shared IRIS instances were never restarted. CPF and instance restart used a disposable provider removed after use. Main registry/readers used a named loopback fixture, now stopped.

## Known-panel closure

The original 41 coverage cases and 26 precision questions per layer retain their IDs, denominators and original scoring. [Independent known-panel review](known-panel-review.json) gives all 134 per-layer/measure/ID rows; this is unblinded purposive closure, not fresh performance. Coverage uses complete cases as primary; points are supplementary.

| Measure | Actual iteration 1 | Iteration 2 | Gains / withdrawals |
|---|---:|---:|---|
| Complete ontology | 31/41, 9 partial, 1 absent | 36/41, 5 partial, 0 absent | +5 /0 |
| Complete formal representation | 36/41, 5 partial | 37/41, 4 partial | +1 /0 |
| Ontology supported/answered; supported/all | 23/26;23/26 | 26/26;26/26 | +3 /0 |
| Model supported/answered; supported/all | 25/26;25/26 | 26/26;26/26 | +1 /0 |

Ontology full gains: `L8`, `HELD-FILE-ROLLBACK`, `HELD-CPF-ROLLBACK`, `HELD-INSPECTION-NAME`, `X-ATTR-CYCLE`. Formal full gain: `HELD-CPF-ROLLBACK`. Ontology precision gains: file rollback, CPF rollback, inspection name; model precision gain: CPF rollback. X-ATTR-CYCLE precision was already supported and stays supported; the canonical owning coverage is what improved. All26 questions are answered in each layer, with zero partial/contradicted/unconfirmed/unanswered/undetermined precision grades after this scoped review.

Unchanged ontology partials: `L5`, `L6`, `B2`, `B3`, `B12`. Unchanged formal partials: `L12`, `L13`, `B9`, `B11`. Two native L12 schedules do not fully compose lock blocking or all interleavings in Verus. JSON/status readers do not prove all report formats, verify cleanup, tree/Python/sorted inspection, or all external rollback effects. Known-panel points are71→77/82 ontology and77→78/82 formal; these must not replace the complete-case counts.

## Property withdrawals and active assurance

Two correspondence scopes are withdrawn even though no complete known-panel pass or entire member is withdrawn:

- `owners.rs::successful_refresh_holds_outer_lock` still proves its old abstract function, but its default-BuildCache correspondence is false. Actual `%New(1)` and successful refresh release the explicit lock by method return; `%New(0)` is the deferred neighbor. Cache consistency under other database locks remains open; no race corruption was established.
- A generalized no-resurrection reading of accessible delete/finalize is false. In the locked schedule, delete waits until after persistence, then temporary close restores the same finalized ID. The accessible neighbor still stays absent.

[Active assurance view](ACTIVE-ASSURANCE.json) qualifies the immutable legacy artifacts instead of rewriting historical receipts. [Exact withdrawals](boundaries/withdrawals.json) identify the old edge and replacement. Human policy for deleting active rows and cache isolation remains open.

## Breadth, representation and evidence

| Member measure | Before | After |
|---|---:|---:|
| Scoped Verus representation | 174/1,216 (14.31%) | 212/1,216 (17.43%) |
| Direct | 165 | 203 |
| Shared-contract-only | 9 | 9 |
| Unrepresented | 1,042 | 1,004 |
| Mechanically proved ObjectScript bodies | 0 | 0 |

The38 direct member additions are exact unions in [coverage.json](coverage.json), with [bindings](bindings.json), [reverse bindings](reverse-bindings.json) and [all member dispositions](member-dispositions.json). Four are lock/CPF boundaries;34 are source-reviewed registry/publication/reader projections. Duplicate manager gates and shared prefix contracts do not each constitute an independent discovery or proportional quality gain. Whole-member losses are zero; the two property withdrawals above remain explicit.

The independent source-first triage scripted all1,216 members and inspected75 of the original1,042 uncovered members across nine purposive strata:44 material decisions,27 helpers/delegation, four justified exclusions. The other967 remain unclassified by that initial review. Exclusions remain in the denominator. A later source/model mapping review is separately marked, rather than relabeled source-first discovery. [Inventory report](inventory/REPORT.md), exact source slices/hashes and [mapping review](inventory/new-binding-review.json) preserve selection and withheld mappings. Most CLI/datatype/extension callbacks, resource processors, archive operations, storage and Studio paths remain gaps.

[Registry contracts](../../../Verus_verification/consolidated/iter2/registry.rs), [reader contracts](../../../Verus_verification/consolidated/iter2/readers.rs), and [boundary state](../../../Verus_verification/consolidated/iter2/boundaries/boundaries.rs) verify32 items with zero logical errors:26 named proof functions plus six recursive-spec termination items. The new control set rejects35/35 deliberate bad variants at an assertion/postcondition (28 main, seven boundary). These are hand translations, not implementation refinement. Constructor depth and arbitrary sequences are symbolic where declared; history schedules and provider outcomes remain finite/conditional. [TRUST](../../../Verus_verification/consolidated/iter2/TRUST.md) records bounds and ports.

[Native main receipt](native/receipt.json) has21 scheduled assertions, including reversed provider tag order, tar-layer selection, authentication classification, same-retained-manager disable, method/hook counting, full report tree, and final GetArg object/primitive arrays. These are repeated families, not21 independent quality cases. [Boundary native receipt](boundaries/native-replay-20261114T173805Z/receipt.json) has repeated process/restart, lock/cache, active-delete and actual CPF configuration pairs. Deployed-source XML comparison matches47 distinct involved production method bodies ignoring indentation (35 main plus12 boundary), which establishes source identity only.

Six iteration 1 consolidated checker drivers replayed in a disposable baseline copy, preserving all original receipts. Five passed initially; thin-surfaces passed after a path-alias tooling retry. Original G/P campaign receipts and supported/refuted distinctions remain preserved; this iteration did not rerun their entire original campaigns. [Replay receipts](inherited-replay/receipt.json), [thin retry](inherited-replay/retry-receipt.json), [new gate](gate/receipt.json), and [excluded attempts](FAILURES.md) distinguish logical results from fixture/tool failures. A translated proof can stay green while native correspondence is withdrawn.

## Source discoveries outside the repair queue

[Before/after case ledger](before-after-ledger.json) separates these six discovery groups from known-queue repairs and fresh grading:

- Actual untagged ORAS selection depends on provider order despite its highest-version comment. This is a confirmed behavior with a provider-dependent intent discrepancy, not a prevalence claim about registries.
- Integrity compares the first tar layer, and the file reader separately selects a tgz. A later matching layer does not rescue the first mismatch; unique-file/layer assumptions remain unproved.
- A retained Remote publishing manager ignores disable after cached success; ORAS rereads it. Shell-to-shell lifetime is not established.
- Publication/deletion prefixes and postpublication cleanup/local export can leave external effects before an error. This is source plus Verus state; real multi-tag failure remains natively unconfirmed.
- Test method status and case hook failure differ from assertion totals, and JSON file/device readers have different shapes. These are useful recovered behavior, not product defects.
- Forced uninstall traversal may progress while the provider retains rows or resources; actual failed-provider store effects remain open.

[Interaction bindings](interactions.json) name decision owners, competing writers, provider and final reader slices without awarding extra representation. Proposed simplifications are unapplied: select/bind one unambiguous archive, separate completed external stages from command status, and decide explicit lock/cache ownership before changing release behavior. [Owner questions](QUESTIONS.md) point to canonical policy blocks. No owner ratified compensation, deletion or registry policy.

## Fresh evaluation and cost

The final exploratory fresh-panel tables, timing deviations, remaining held-out gaps and measured wall-time basis are appended below. Known closure and member reach must not stand in for them. Human priorities, disputed policy and human readability are unmeasured.

## Reproduction

From the IPM root:

```sh
python3 .g5/uplift/consolidated/iter2/run_all.py
python3 .g5/uplift/consolidated/iter2/run_all.py --native --inherited
python3.11 .g5/uplift/consolidated/iter2/compare_deployed.py
```

The first command verifies the new contracts/controls, source census, exact bindings, known ledger and preservation. The second adds named loopback and disposable native providers plus disposable inherited replay. Snapshot archives must be retained in the experiment folder. Full evaluator reproduction uses its immutable panel and corrected audit scripts; do not rerun freeze scripts over the preserved panel. Benchmark HTML is intentionally left for independent grading. Shared workflow guidance is unchanged; no guidance effect can be inferred from this single exploratory iteration.

## Exploratory fresh results

The strict pre-edit requirement was **not fully met after deduplication**. The source-only evaluator froze24 main contrasts across eight areas and four separate challenges before foundation inspection. Final audit found `I2E23` repeated known `OW-CONSTRUCTOR`, leaving23 qualified fresh main contrasts. E01–E03 also share environment components with broad historical B8; their exact distinction and incomplete original input-frame limits are disclosed, not hidden. The original24 including the known overlap remain an observed-panel appendix, never a fresh win.

A second source-only evaluator froze six additional contrasts across four subsystems against the already sealed artifacts. This is a separate postseal cohort, not a replacement for the failed pre-edit24 requirement. No ontology or model changed after either evaluator could inspect the candidate. These independent-source assessments are exploratory: the first evaluator also corrected a full-ID alignment bug after initial correctness inspection and re-sealed its representation audit. Invalid outputs remain preserved. The supplement properly froze representation before correctness. Neither sample is a probability estimate.

| Cohort and layer | Complete/partial/absent before → after | Supported/answered before → after | Supported/all before → after |
|---|---|---|---|
| Qualified prefrozen23, ontology | 1/9/13 → 1/10/12 | 1/10 → 1/11 | 1/23 → 1/23 |
| Qualified prefrozen23, formal | 0/7/16 → 0/9/14 | 0/7 → 0/9 | 0/23 → 0/23 |
| Separate postseal6, ontology | 0/1/5 → 0/1/5 | 0/1 → 0/1 | 0/6 → 0/6 |
| Separate postseal6, formal | 1/0/5 → 1/0/5 | 1/1 → 1/1 | 1/6 → 1/6 |
| Separate challenge4, ontology | 0/1/3 → 0/1/3 | 0/1 → 0/1 | 0/4 → 0/4 |
| Separate challenge4, formal | 1/0/3 → 1/1/2 | 1/1 → 1/2 | 1/4 → 1/4 |

There is **no complete-case fresh gain** in either main cohort. Formal partial coverage grows, while ontology conditional precision falls1/10→1/11 because another incomplete answer is now given. The rare challenge's conditional formal precision falls1/1→1/2 for the same reason; no supported challenge answer was withdrawn.

On the23 main cases, ontology correctness counts are supported1, partial7→8, contradicted2, unconfirmed0, unanswered13→12, unclassified0; formal supported0, partial7→9, contradicted0, unconfirmed0, unanswered16→14, unclassified0. The separate status display derives unanswered only from explicit no-answer plus absent-representation records; original composite U/Q labels are retained in [normalization ledger](fresh-status-normalization.json). The six supplement cases have ontology contradicted1 and unanswered5; formal supported1 and unanswered5, with no other statuses. All supplied cases have explicit assessment dispositions; this does not classify the rest of the product.

Fresh source-supported gaps include `module.deployed` (explicit false class parameter override), `repo.search-order` (keyword filesystem exclusion), and `module.invoke` (unchecked returned status). They remain owner-linked residuals, not post-evaluation repairs to the sealed candidate. The supplement's complete global Preserve model was already present in inherited Verus, so it earns no new credit. Its other WebApplication/global-path/interoperability decisions remain unanswered.

Fresh runtime confirmation is separate from source/model correctness: the first evaluator credits zero complete exact frozen-pair native confirmations; one partial constructor correspondence belonged to the excluded known overlap. The supplement is runtime-unconfirmed on all6. Parent native successes do not upgrade unobserved fresh cases. Human priorities, disputed policy and human comprehension remain unmeasured.

[First evaluator report](../../../../../results/experiments/uplift-measurement-20261114/ipm-iter2/evaluator-private/REPORT.md), [freshness audit](../../../../../results/experiments/uplift-measurement-20261114/ipm-iter2/evaluator-private/freshness-audit.json), [supplement report](../../../../../results/experiments/uplift-measurement-20261114/ipm-iter2/supplement-evaluator/report.json), and [combined before/after ledger](before-after-ledger.json) retain cases, answers, authority, neighbors, hashes and limitations. Panels and per-case grades were never rewritten to manufacture improvement.

## Cost and final handoff

Measured baseline-capture-to-assembly wall time: 28.39 minutes, including concurrent lanes and waits. Initial pre-capture reading was not fully metered. Tokens and monetary cost are **unknown**, including helper costs. The first evaluator separately records17.8 minutes freeze-to-initial-final, plus2.94 minutes freshness-audit activity; those durations overlap the overall wall interval and omit some earlier source selection. Supplement monetary/tokens and fully metered wall cost are unknown. No cost-effectiveness or guidance superiority claim is supported. [Cost receipt](cost.json).

The updated foundation is the current working tree and the sealed candidate archive; the original index and HEAD are retained. [Canonical delta from the actual iter1 tree](canonical-vs-actual-iter1.diff) shows only this iteration’s owner edits. [Final validation](validation.json) verifies preservation and sealed-answer integrity. The benchmark HTML remains untouched for independent grading.
