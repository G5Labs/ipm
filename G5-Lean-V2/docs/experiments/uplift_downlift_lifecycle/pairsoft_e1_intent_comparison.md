# PairSoft intent recovery: plain baselines, current G5 and pre-TLA ontology

14 October 2026 · Artifact comparisons for [experiment E1](experiment_plan.md). Sections 1–6 record the historical comparison; section 7 records the subsequent harness pair; section 8 compares the unchanged current G5 harness with the two original plain baselines. No formal campaign was run.

**Latest decision:** the user declined the harness amendment; the three maintained skill edits and generated copies have been reverted. Frozen experimental inputs/outputs remain preserved. On the 13 previously recorded probes, current G5 fully retains 9 versus 8 for E1-Code and 7 for E1-Sources, at substantially greater artifact length and author effort. This is a limited retention gain, not an established overall workflow advantage; see [section 8](#8-current-g5-versus-the-two-plain-baselines).

**The pre-TLA ontology is recoverable.** It is substantially broader than either fresh account, but much less concise and partly stale against the very same code revision. Both plain-agent runs recover important current behavior absent from that ontology. The ontology also preserves useful requirements and decision rationale that neither short account retains. These observations support a coverage-versus-compression tradeoff; they do **not** establish a G5 advantage under matched experimental conditions.

The clearest practical findings are:

- **Current implementation fidelity:** both baselines recover the new change-order approval protocol and configurable over-receiving, which the historical ontology misses or misstates.
- **Retention of important intent:** the ontology preserves vendor receipt-number uniqueness and the documented gap between manual initial-PO approval and automatic implementation. Both baselines omit those commitments/discrepancies.
- **Conciseness:** the accounts contain 1,621 and 1,381 whitespace-delimited words. The historical ontology contains 85,700 across 43 pages, including substantial metadata, implementation detail, UI material and inherited history.
- **A shared omission:** neither fresh account explicitly preserves the ability to receive against an already fully received PO when enabled tolerance leaves an allowance. The old ontology incorrectly prohibits it.
- **Reusable structure:** the ontology supplies stable rule IDs, semantic owners and provenance. Its stale rules and contradictory drift summary show that this structure alone does not keep the contract accurate.

## 1. Retrieved version and provenance

| Item | Retrieved identity |
|---|---|
| Repository | `/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform` |
| Pre-TLA snapshot | `0e0c3981653bf0935cdf3e4fcdc7101d104de7ff`, committed 24 September 2026, “Add ontology and refresh generated scenario diagrams” |
| Ontology tree | `.g5/ontology`, Git tree `d0062b251e44c3b98c9e1578d30a9cf38febc28b` |
| Cutoff verification | That tree is byte-identical to `.g5/ontology` in the parent of `c4abdb3b79039012971ec16adeb5912ba4865ad1`, the 3 October ontology-strengthening commit. |
| Relationship to E1 | The original, unsanitized code pin for both fresh baselines is **the same `0e0c3981` commit**. Their sanitized Git hashes differ, as recorded in the experiment plan. |
| Preserved copy | [Frozen root][old-root], [all file hashes and measurements][snapshot-manifest], and [extracted block bodies with original locations][old-blocks]. The copy was exported directly with `git show`; its content was not repaired. |

This is the committed ontology before the October TLA-driven amendments, rather than an attempted reconstruction by deleting formal-looking sections from the final ontology. The earlier September 22 migration conversation is also available. It asked the author to preserve existing ontology and **all information in `docs/knowledge`**, then required restructuring without information loss. The historical [format guide][old-format] explicitly says the inherited observations were not freshly verified. See the [original conversation][migration-session] and [lifecycle chronology](pairsoft_lifecycle_study.md).

The exact pre-TLA snapshot has **688 unique I/D/Q blocks: 623 I, 55 D and 10 Q**. The 718-block count belongs to `c4abdb3b`, after 30 additions; it is not the count for this initial snapshot. Stable IDs count authored blocks, not independently validated requirements.

To retrieve the snapshot again without changing a checkout:

```sh
git -C /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform \
  show 0e0c3981653bf0935cdf3e4fcdc7101d104de7ff:.g5/ontology/root.g5.md
```

The frozen directory preserves original relative links verbatim. Links from those copied pages into source/docs require the original repository layout; this report supplies direct links to the inspected source evidence separately.

## 2. What is comparable, and what is not

| Artifact | Inputs and task | Scope and cost |
|---|---|---|
| [E1-Code intent][code-intent] / [coverage][code-coverage] | Fresh GPT-6 Astra / extra-high run; current code, comments, tests, schema and configuration; concise intent recovery. | Backend semantics; 9m 2s and 22,776 generated tokens, including reasoning. |
| [E1-Sources intent][sources-intent] / [coverage][sources-coverage] | Same model/settings and current code, plus sanitized original documents and Git history; the same concise recovery objective. | Same backend scope; 12m 32s and 27,956 generated tokens, including reasoning. |
| [Historical pre-TLA ontology][old-root] | Migration/consolidation of prior ontology, generated knowledge and recorded decisions, with human corrections. It inherited work unavailable to the fresh authors. | Procurement, Expenses, UI, operations and development policies. Comparable end-to-end author cost/model configuration is not established. |

The code revision is matched, which makes concrete current-code discrepancies meaningful. **Source access, task, scope, effort and prior human feedback are not matched.** In particular, the fresh baseline packets deliberately excluded `docs/knowledge`; the historical migration was instructed to retain it. This artifact is therefore a historical comparator, **not an executed E1-G5 treatment** and not a gold reference.

This assessment read both complete baseline accounts and coverage notes, inventoried the entire historical ontology, and inspected selected contract sections and original code/documents for the comparisons below. It did not semantically audit every historical block, run application tests, execute TLA/TLC, or obtain independent human adjudication. Probes were selected retrospectively after reading the outputs. “Omitted” means not stated in the saved account; a link to a large source file is not a recovered commitment. The table is not a random sample or a whole-product recall denominator.

## 3. Conciseness, minimalism and reading burden

| Measurement | E1-Code | E1-Sources | Pre-TLA ontology |
|---|---:|---:|---:|
| Main artifact words | 1,621 | 1,381 | 85,700 |
| Separate coverage-note words | 503 | 480 | No equivalent separate initial-run report identified; drift and evidence are embedded |
| Main artifact pages | 1 | 1 | 43 |
| Words inside explicitly marked I/D/Q bodies | Not this format | Not this format | 41,853 |
| Stable I/D/Q blocks | Not this format | Not this format | 688 |

Counts use decoded Markdown split on whitespace, including markup, source paths and link destinations. Ontology totals include only `*.g5.md`, excluding the 270-word format guide. Block-body counts take the text between a standalone `[I/D/Q · id]` marker and its following `**meta data**` marker; this still includes citations and historical narrative. It is a reproducible size proxy, not an atomic-claim or semantic-density measure. Exact hashes/counts are in the [snapshot manifest][snapshot-manifest].

The historical artifact has **19,235 words in the user-experience chapter** and **5,746 in expense-specific pages**, with 698 words overlapping those categories. Removing those totals would still not produce a clean backend comparison: other chapters mix UI, development guidance and backend semantics. These scopes must be classified by meaning before assigning a coverage-adjusted score.

Even allowing for that extra scope, the old artifact is not a minimal contract. It retains lengthy implementation narration, historical fixes, per-rule source boilerplate and repeated summaries. For example, the UDF membership rule records the reason for permissive handling, the prior regression, class names and the repair history in one long block. A short rule can preserve the required exception while moving that investigation history into evidence. Conversely, the fresh accounts are not demonstrably minimal either: they include details such as fixed UDF slot counts or change-order payload limits while omitting receipt-number uniqueness. **Minimalism should retain consequential distinctions first; word count alone cannot judge that selection.**

E1-Sources is about 15% shorter than E1-Code in this pair, but does not simply contain everything E1-Code says in fewer words. It adds documentary rationale and acceptance boundaries while dropping some cart, approval and configuration exceptions. Their coverage sets differ.

## 4. Concrete recovery probes

“Present” below refers to the named obligation, not the entire feature. “Partial” means that a material condition or exception is missing. Source checks are static and scoped to the cited paths.

| Obligation or distinction | E1-Code | E1-Sources | Pre-TLA ontology |
|---|---|---|---|
| Requisition → approval → order → delivery → receiving are distinct steps, with asynchronous handoffs | Present | Present | Present in the root/workflow owners; the high-level lifecycle is not a G5-exclusive recovery. |
| Legacy approval administrators can bypass normal assignments/minimums; out-of-office substitution does not remove the original approver's ability to act | Explicit about both exceptions | Mentions administrative override/substitution, but omits the original-approver exception and explicit quorum bypass | Explicit in [approval rules 18, 27–28][old-approvals]. |
| Catalog updates refresh carts; removal of the selected UOM resets the line, while moving the item to another company removes it | Explicit | Not stated | Explicit in [shopping rules 12–14][old-shopping]. This is a real information difference between the two short accounts. |
| Vendor receipt number is unique **per vendor** | Omitted | Omitted | Explicit in [receiving rule 5][old-receiving]; supported by original requirements and the current unique index. |
| Cumulative receipt allowance is policy-driven, including enabled-tolerance truncation | Present | Present | Rule 20 states an unconditional ordered-quantity cap; stale against this pin. |
| A `Received` PO can be received again when over-receiving is enabled, tolerance is positive, and allowance remains | Not explicitly retained | Not explicitly retained | Rule 2 excludes `Received` unconditionally; stale. All three fail to preserve this exception correctly. |
| Change approval keeps proposed terms separate from active PO terms, freezes a route, and binds final application to proposal/revision identity | Present, including retries/quarantine | Present, with more evaluation/acceptance detail | Purchasing rules 28–33 describe legacy immediate changes; the new pending-approval protocol is absent. |
| Budget lookup exists and matches accounting dimensions; finding budget support does not establish spending-limit enforcement | Detailed lookup and explicit inference about lack of enforcement | Actuals/visibility and the inference, with less lookup detail | Retains the intended matching rule but says no matching handler was located; that observation is stale. |
| An empty replicated UDF list must not cause legitimate submitted values to be rejected | Explicit, with inactive-item acceptance | Explicit empty-list exception | Correct in [field rule 35][old-fields], but the old [drift queue][old-drift] still claims server validation is absent. |
| Preserve identity across login changes; retiring a duplicate does not reassign its historical transactions | Login reconciliation summarized, but continuity mechanism/transaction-history boundary not stated | Both retained | Both retained in [user rules 19–21][old-users], with fuller authority/rationale. |
| Cross-product runtime independence, with shared identity as the accepted hard dependency | Describes integration dependencies, but not this governing product rule | Retains independence, identity exception and degraded session behavior | Explicit human-provided rule in [product boundaries][old-products], including PaperSave-only tenants and restrictions on new coupling. |
| Original documentation expected human approval of initial POs, while inspected implementation auto-approves them | Automatic behavior retained; historical discrepancy absent | Automatic behavior retained; historical discrepancy absent | [Approval drift][old-approvals] preserves both sides. Whether that old requirement remains governing still needs adjudication. |
| Standard-field requiredness and UDF validation are different; browser enforcement is not universal server enforcement | Explicit, including the expected-failure API test | Explicit, with the server-enforcement proposal | Preserved in the relevant field/lifecycle observations and drift records. No exclusive ontology advantage on this distinction. |

### Receipt numbering: compression lost a reconstructive requirement

The historical rule `pairsoft.kb.receiving.creating-a-receipt.r5` says receipt numbers must be unique per vendor. [User story 119242][receipt-requirement] specifies that scope, and [`ReceivingView`][receipt-index] defines a unique index over `(VendorId, VendorReceiptNo)`. Neither baseline states it.

A replacement could follow either short account, implement receipt quantities/statuses perfectly, and still accept duplicate vendor receipt numbers. This is a concrete fidelity gap, not a preference for longer documentation or G5 syntax. A single additional sentence could preserve it; the historical article's entire length is not necessary.

### Over-receiving: both fresh accounts improve the cap, but miss eligibility

The historical receiving rules exclude `Received` orders and cap receipt quantity at ordered quantity. The [accepted over-receive design][overreceive-design] instead allows policy-dependent extra quantity and explicitly reopens fully received orders with remaining allowance. Current [`OverReceivePolicy`][overreceive-code] implements the allowance calculation, while the [details query][receivable-code] distinguishes the `Received` exception from other terminal statuses and rejects orders with no allowance left.

Both baselines recover the configurable cap, including truncation, without formal tools. Neither expressly says a fully received PO becomes eligible again. Thus an implementation reconstructed from either could calculate the right cap but make it inaccessible through a blanket status guard. This separates **correct arithmetic**, **legitimate completion**, and **coverage of exceptions** without crediting TLC for a documentary/code fact it did not uniquely supply here.

### Change orders and budgets: inherited knowledge was not reconciled with current code

The historical purchasing rule `...change-order.r29` describes a successful change as immediately replacing terms and incrementing revision. The current [change handler][change-code] can instead persist a pending proposal and return before activating it. Both fresh accounts retain that distinction and the downstream identity/recovery protocol. They are clearly more useful for reconstructing this implemented feature.

Likewise, the old budget section says no matching query was located. The current [budget query][budget-code] matches dimensions and joins actuals; both fresh accounts find it. These discrepancies exist **at the common comparison revision**, rather than merely because the baselines saw a later commit. They demonstrate the cost of migrating inherited observations without refreshing them.

### UDFs: structure preserves rationale but does not prevent contradictory summaries

The old owning rule explains that empty local lists can mean replication lag, so unknown values must not be rejected solely for that reason. The current [validator][udf-code] carries the same rationale. E1-Code and E1-Sources both retain the exception. This is a useful result for the code baseline: consequential rationale can already be present in comments.

However, the historical drift queue's class B still says dropdown values are not validated server-side, despite the owning rule recording the fix. That queue labels itself a one-time, non-maintained roll-up, which helps interpretation but does not remove the reader's reconciliation burden. Stable IDs and semantic owners are useful only if consumers follow the owning rule and obsolete summaries are corrected or retired.

### Original intent versus current implementation: the main fidelity risk

The [original Approval Center document][approval-requirement] describes initial-PO approval policies and a PO approval journey. The old ontology preserves the mismatch with unconditional automatic approval. Both baselines correctly describe automatic initial-PO approval, but neither retains the documentary discrepancy or a disposition for it. This concerns **initial POs**, not the separate change-order approval protocol that both recovered.

This is not evidence that manual approval must be implemented now: an original requirement can have been superseded. It is evidence that an account of current behavior is not necessarily an account of original intent. E1-Sources explicitly prioritizes current code and later implementation evidence over older designs. That helps when documents are stale, but cannot by itself settle whether a divergence is an accepted decision or an implementation gap. A stronger recovery artifact should retain the supported requirement, the observed behavior and the unresolved/superseded disposition separately.

The [accepted runtime-independence ADR][independence-adr] illustrates the positive case: E1-Sources preserves a governing design intention that E1-Code does not state, while the historical ontology gives it an explicit owner and provenance. Source access adds value here, although a single pair does not establish how reliably.

## 5. What this says about G5 before TLA

The historical ontology's strongest observed contribution is **retention and addressability**: a future change can refer to a specific requirement, its owner, exception and source. Receipt-number uniqueness, identity continuity and runtime independence illustrate useful retained meaning. It also keeps some unimplemented documentary intent visible instead of collapsing the product definition to shipped code.

Its weaknesses are equally concrete: excessive retained detail, stale code observations, incomplete coverage of a major implemented feature and duplicate summaries that disagree. These are failures relative to G5's own concise/current/consistent goals. More ontology is not automatically better ontology, and the historical artifact cannot fairly represent today's recovery-only skill under the new experiment's budget.

The plain-agent baselines are strong at quickly explaining current purchasing behavior and recovering nontrivial exceptions. E1-Code sometimes retains more useful detail than E1-Sources; E1-Sources adds some purpose, authority and acceptance context. Neither short account is a sufficient reconstruction contract on the inspected probes. No whole-repository precision/recall, validated bug yield, independent reuse score or overall winner is claimed.

**No rerun was necessary to retrieve the historical comparator, so none was launched at that step.** A fresh E1-G5 run was recommended for the causal question: use the E1-Sources packet, model/settings, budget and concise objective, with recovery-only G5 instructions and no TLA/TLC. Compare retained commitments and omissions, and test whether a fresh reader can recover rules/exceptions from each artifact. The subsequently authorized recovery pair is reported in section 7; the historical comparison cannot substitute for that experiment or an independent reuse test.

TLA/TLC's incremental value remains the separate E2/E3 question. The stale over-receive and budget statements were detectable from documents/code alone, and both baselines already recovered sophisticated change-order identities and recovery behavior. The evidence here does not support a claim that agents without ontology or formal tools cannot recover those rules.

## 6. Harness implications: requirements already present, improvements to test

The deficiencies above describe the **historical artifact**, not a demonstrated failure of today's uplift skill. The September migration inherited drafts and was explicitly told to preserve their information. It was not a fresh execution of the current recovery workflow under E1 conditions. We should not infer that the current harness lacks conciseness, minimalism or deduplication requirements.

| Historical failure | Current instruction already addressing it |
|---|---|
| Excess detail | [Authoring skill](../../../.agents/skills/ontology-authoring/SKILL.md): minimal descriptions, one independently changeable commitment, no routine implementation narration. Uplift phase 5 compares compression with source meaning. |
| Stale observations | [Source guide](../../../.agents/skills/ontology-uplift/references/intent-and-design.md): inherited/generated material is evidence to reconcile; distinguish current behavior, supported intent and superseded claims. |
| Missing a major feature | [Uplift phases 1 and 4](../../../.agents/skills/ontology-uplift/SKILL.md): independent capability discovery and a reconstruction challenge. |
| Conflicting duplicate summaries | [Structure guide](../../../.agents/skills/ontology-authoring/references/structure.md): one semantic owner; indexes contain labels/links, not duplicate commitments or assessments. |

**Recommendation: first evaluate the frozen current recovery-only instructions, then make small changes against observed failures.** Do not add another general checklist or treat extra reviewers, word ceilings or ID counts as semantic quality guarantees. The E1 ablation must retain source reconciliation, the reconstruction challenge and final compression when TLA/TLC is removed; those checks do not depend on a model checker.

Three targeted clarifications are worth testing if failures persist:

1. **Preserve information without importing the archive into the contract.** For migrations, retain original narratives, superseded observations and detailed repair history in supporting evidence. The governing body keeps the current obligation, necessary conditions/exceptions, consequential rationale and unresolved meaning. Existing IDs or a recent migration commit do not establish that inherited implementation claims were rechecked. Code divergence still does not authorize weakening valid intent.
2. **Make compression testable within the existing final pass.** Ask both: “What materially wrong behavior would become permitted if this statement disappeared?” and “What important behavior could still be omitted while satisfying what remains?” Preserve consequential rationale even when its value is explaining a future decision. For example, a compact UDF rule can say: “Validate dropdown values against locally replicated list items, including inactive items. If no items have arrived, accept the value because replication lag does not establish invalidity.” Class names and the prior regression remain linked evidence. Do not optimize for an arbitrary document size.
3. **Reconcile derived summaries when an owner changes.** Replace copied live assessments with links to the authoritative owner; move useful dated snapshots outside the live contract. This operationalizes the existing ownership rule and addresses the UDF drift-queue contradiction. It should be part of the existing edit/integration pass, not a new approval phase.

Use the concrete probes above as evaluator-side regression cases: receipt-number uniqueness must survive compression; over-receive eligibility must survive exception handling; stale implementation statements must be reconciled; a major current capability must have an owner; a resolved rule must not coexist with a contradictory live summary. These cases are now known in this conversation, so a new same-context pass is not a blind evaluation. Fresh author packets should contain the agreed sources/instructions without this report or answer hints; assess generalization with additional unseen cases. Measure retained obligations, incorrect claims, contradictions, reading effort and author cost together. A shorter ontology that loses obligations fails, as does a comprehensive archive presented as a minimal current contract.

At the recommendation step, no skills were modified and no further experiment was run.

Subsequently, the user authorized implementing the amendment and comparing it with the frozen current harness. That separate run is recorded in [experiment plan section 11](experiment_plan.md#11-g5-current-versus-amended-harness--14-october-2026); the historical comparison above remains unchanged. Previous plain-baseline output links now point to their preserved archive because the two worktrees were reused for the new pair.

## 7. Fresh current versus amended G5 recovery

**The amendment did not demonstrate an overall improvement in this pair.** Both authors produced partial reconstruction contracts, with useful exceptions retained and consequential omissions remaining. The amended author hit the token threshold before final reporting. Its saved contract can be inspected, but it is not a completed execution of the full requested recovery pass.

Both used GPT-6 Astra / extra-high, identical code, docs and sanitized history, and byte-identical prompts treating **docs as important SOT for intended behavior**. The current package was frozen before the three changes in section 6. Both excluded prior ontology, generated knowledge, previous answers, TLA/TLC and product execution. The [execution record](experiment_plan.md#execution-and-saved-results) contains the protocol, telemetry, validation and limitations.

| Saved artifact | Current harness | Amended harness |
|---|---|---|
| Canonical contract | [Root][g5-current-root] | [Root][g5-amended-root] |
| Coverage and remaining work | [Coverage][g5-current-coverage] | [Coverage][g5-amended-coverage] |
| Pages / I–D–Q blocks | 21 / 85 | 20 / 89 |
| Body words / words including metadata | 5,374 / 7,410 | 5,697 / 7,502 |
| Generated tokens, including reasoning | 38,486 | 42,955; watchdog stop |

Counts cover the whole ontology, not the short `intent.md` entry point. The amended body is about 6% longer. That is a size observation, not a finding that it contains unnecessary detail. Both retain separate investigation records; completeness, minimality and semantic deduplication require judging meaning, not counting files or unique IDs.

### Selected commitment probes

These are evaluator observations of the saved contracts, not a blinded gold-set score or exhaustive fidelity audit. IDs locate exact claims; supporting evidence and source links remain with those owners.

| Commitment | Current harness | Amended harness |
|---|---|---|
| Vendor receipt number must be unique per vendor | Not explicit in the saved contract | Explicit in [orders][g5-amended-orders], `ps.receipt.draft` |
| A fully Received PO can become receivable when enabled positive tolerance leaves capacity | Explicit in [orders][g5-current-orders], `ps.reopenreceipt` | Explicit in [orders][g5-amended-orders], `ps.receipt.policy_time` |
| Missing replicated dropdown membership must not by itself reject values | Explicit, though broadly phrased as absent/lagging projections, in [customization][g5-current-fields], `ps.udflag`; metadata distinguishes observation from proof | Not explicit. `ps.udf.definition` retains missing **usage flags**, a different exception; historical inactive labels also do not express membership validation during lag |
| Original requisition recall restriction excludes requests already reviewed, including partial review | Explicit in [requisitions][g5-current-requisitions], `ps.recall`, with observed code divergence retained | [Purchasing][g5-amended-purchasing], `ps.requisition.lifecycle`, permits recall while Awaiting Approval without this qualification |
| Shared-activity failures allow bounded suppression of idle logout, while current code treats failures as inactivity | [Identity][g5-current-identity], `ps.idlesync`, retains the requirement and metadata records the gap | [Identity][g5-amended-identity], `ps.identity.session`, retains the requirement and metadata records the gap |
| Pending Change Order approval must not immediately replace the active PO | Explicit in [change orders][g5-current-changes], `ps.colock` / `ps.coapply` | Explicit in [change orders][g5-amended-changes], `ps.change.visibility` / `ps.change.apply`, with a separate protocol owner |
| Original initial-PO approval journey versus automatic initial-PO approval | No explicit reconciliation of this documentary discrepancy | No explicit reconciliation of this documentary discrepancy |

The original [recall requirement][recall-requirement] supplies a further concrete docs-versus-code probe: preserving only the Awaiting status guard loses the no-review condition. This is retention evidence, not a newly validated runtime bug. Likewise, the [initial-PO approval requirement][approval-requirement] still needs an accepted/superseded/unresolved disposition; describing change-order approval does not settle it.

Both fresh G5 artifacts retain the Received-PO exception omitted by the earlier two plain accounts. The amended artifact also retains vendor receipt-number uniqueness omitted by those accounts. However, the new common prompt strengthened documentary authority, and there is only one execution per variant. Those gains cannot be attributed solely to ontology structure or to the amendment. The current run's useful documentary restrictions also show that the pre-amendment harness already exercises meaningful source reconciliation.

### What remains unestablished

Both authors explicitly leave broad original requirement families, MasterData rules, authorization paths and integrations incompletely traced. These artifacts therefore do not establish whole-system reconstruction sufficiency. The amendment also received no inherited ontology or contradictory live drift queue, so its migration and summary-reconciliation clauses have little direct test coverage here.

The current author completed its artifact checks. The amended author's check report is stale at the budget stop; a separate evaluator check passes the final saved structure without changing it. Stable IDs, valid links and paired metadata do not prove absence of contradictory meaning. No independent reader reconstruction, human adjudicated coverage set or TLA/TLC run was performed.

The user subsequently declined adopting the amendment. The three maintained skill files were restored byte-for-byte from the frozen current-harness package, and generated copies were synchronized. The frozen amended package and both runs remain unchanged as experimental evidence. This result does not justify adding more harness instructions. Repeated matched runs and independent contract-use evaluation are needed before claiming improved fidelity, minimalism or reliability.

## 8. Current G5 versus the two plain baselines

**Judgment:** the current G5 artifact is a somewhat stronger starting contract for retaining documentary intent and locating rules, while the plain accounts are substantially more concise and cheaper to author. These runs do not establish that G5 is better overall, or that its extra length and effort produce sufficient downstream value. The comparator here is the **unchanged current-harness output**, not the amended output or the historical 85,700-word ontology.

### Same 13 probes, with explicit retention grading

Reuse the 13 rows already recorded in section 4 before the G5 rerun. **Full** means the saved artifact explicitly retains every material distinction named in that probe; **partial** means it retains some but omits a named condition or exception; **absent** means the requested distinction is not stated. Links to a source do not count as recovering its unstated contents. These are manual retention judgments, not whole-repository accuracy, bug counts or proof of correct implementation. Compound rows remain compound for comparability; no fractional weighting is invented.

| Previously recorded probe | E1-Code | E1-Sources | Current G5 |
|---|---|---|---|
| 1. Distinct purchasing lifecycle and asynchronous handoffs | Full | Full | Full |
| 2. Administrative approval bypass **and** original approver eligibility during substitution | Full | Partial | Partial — `ps.approvaladmin` retains the quorum bypass; original approver eligibility is not explicit |
| 3. Cart refresh, selected-UOM removal reset, company-move removal | Full | Absent | Partial — `ps.cart` retains reconciliation/removal but omits the reset to quantity one/base UOM |
| 4. Vendor receipt-number uniqueness per vendor | Absent | Absent | Absent |
| 5. Policy-driven cumulative receiving cap with truncation | Full | Full | Full — `ps.overreceive` |
| 6. Reopen a Received PO when enabled positive tolerance leaves capacity | Absent | Absent | Full — `ps.reopenreceipt` |
| 7. Pending/active PO separation, frozen route and identified outcome application | Full | Full | Full — `ps.cofreeze`, `ps.colock`, `ps.coapply` |
| 8. Budget lookup by accounting dimensions, without inferring spend enforcement | Full | Partial | Full — `ps.budgetlookup` |
| 9. Empty replicated UDF list does not itself establish invalid membership | Full | Full | Full — `ps.udflag`; its broader absent/lagging wording still warrants precision review |
| 10. Login continuity without transferring duplicate-user transaction history | Partial | Full | Full — `ps.rename`, `ps.splitrepair` |
| 11. Cross-product runtime independence with the shared-identity exception | Partial | Full | Full — `ps.independence` |
| 12. Original initial-PO approval requirement versus automatic implementation | Absent | Absent | Absent |
| 13. Standard-field requiredness distinguished from UDF server enforcement | Full | Full | Full — `ps.standardrequired`, `ps.udfrequired` |
| **Fully retained / partial / absent** | **8 / 2 / 3** | **7 / 2 / 4** | **9 / 2 / 2** |

Against E1-Code, G5 fully retains three additional probes (6, 10, 11), but incompletely retains two that Code captured fully (2, 3): **net one additional fully retained probe**. Against E1-Sources, it adds two full probes (6, 8), while retaining all seven that Sources captured fully: **net two**. It is therefore not a strict improvement over Code's coverage. All three still miss receipt-number uniqueness and the initial-PO documentary discrepancy.

The set was selected retrospectively during the earlier comparison, is small, unweighted and not representative of all requirements. Its counts do not justify calling G5 “12.5% better” or “28.6% better” overall. They quantify the observed difference on this set only. False claims elsewhere, comprehensiveness, duplication and reconstruction success remain unscored.

### Additional meaning and reuse structure

Two further documentary distinctions in section 7 favor current G5: the no-recall-after-review condition and bounded suppression of idle logout during shared-activity failure. Both plain accounts omit the former; E1-Sources describes fallback to local session behavior without retaining the accepted bounded allowance. Current G5 keeps the intentions and records the observed implementation gaps. These are additional examples, **not added to the 13-probe numerator or denominator**. They illustrate recovery of original intent beyond summarizing current behavior; they are not execution-validated bugs.

G5 also supplies 85 stable I/D/Q blocks with semantic owners, per-block provenance and seven explicit questions, plus navigation and inspection records. That makes individual rules and open decisions directly addressable for later changes. Both plain baselines already have source citations and uncertainty; reusable evidence is not unique to ontology. Whether G5's structure makes future implementation or updates more reliable has not been tested by an independent reader/downlift task.

### How much extra effort and reading material?

| Measurement | E1-Code | E1-Sources | Current G5 |
|---|---:|---:|---:|
| Main artifact words, including citations/metadata | 1,621 | 1,381 | 7,410 |
| Main artifact words excluding ontology metadata footers | 1,621; no such footer | 1,381; no such footer | 5,374 |
| Separate supporting Markdown words | 503 | 480 | 5,253 |
| Generated tokens, including reasoning | 22,776 | 27,956 | 38,486 |
| Elapsed author time | 9m 2s | 12m 32s | 19m 47s |

The G5 main artifact is **4.57× Code / 5.37× Sources** by raw word count; even excluding ontology metadata it is **3.32× / 3.89×**. G5 uses **69% / 38% more generated tokens** and takes **2.19× / 1.58×** the elapsed time. Supporting records are separately reported, not hidden from the reading burden. These are whitespace counts and observed generation effort, not normalized prose density, measured reading time or billed monetary cost.

All three use the same model/reasoning setting and nominal resource ceiling. Current G5 and Sources have the same supplied code/docs/history, but the G5 prompt additionally makes documentary authority explicit, asks for a reconstruction contract and invokes the recovery workflow. Actual effort and output length also differ. Code has less source access. Consequently the comparison measures the resulting workflows/artifacts, **not the isolated causal benefit of ontology syntax or the harness**. One sample per arm is insufficient for reliability estimates.

For a quick, compact understanding of current behavior, the plain baselines offer better observed economy; Code retains more of these 13 probes, while Sources adds useful documentary rationale. For maintaining an addressable intent contract, current G5 is the better-structured candidate and retains some additional important requirements, but the gain shown here is modest relative to its extra cost. None is established as sufficient for whole-backend reconstruction. No TLA/TLC contribution, validated bug yield or simplification advantage can be inferred from this E1 comparison.

[g5-current-root]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/root.g5.md
[g5-amended-root]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/.g5/ontology/root.g5.md
[g5-current-coverage]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/experiment_output/coverage.md
[g5-amended-coverage]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/experiment_output/coverage.md
[g5-current-orders]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/features/orders.g5.md
[g5-amended-orders]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/.g5/ontology/features/orders.g5.md
[g5-current-fields]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/features/customization.g5.md
[g5-current-requisitions]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/features/requisitions.g5.md
[g5-amended-purchasing]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/.g5/ontology/features/purchasing.g5.md
[g5-current-identity]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/features/identity.g5.md
[g5-amended-identity]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/.g5/ontology/features/identity.g5.md
[g5-current-changes]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/features/change_orders.g5.md
[g5-amended-changes]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/.g5/ontology/features/change_orders.g5.md
[recall-requirement]: </Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e1-code/docs/analysis/requisitions/My Requisitions/Requisition form for PairSoft's Procure to Pay platform.md>

[old-root]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/root.g5.md
[old-format]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/format.md
[snapshot-manifest]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/snapshot-manifest.json
[old-blocks]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/blocks.json
[migration-session]: /Users/ziniuw/.codex/sessions/2026/09/22/rollout-2026-09-22T17-22-12-01a0caff-6db8-7a80-81d7-2138c420e1d5.jsonl
[code-intent]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-code/experiment_output/intent.md
[code-coverage]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-code/experiment_output/coverage.md
[sources-intent]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-sources/experiment_output/intent.md
[sources-coverage]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-sources/experiment_output/coverage.md
[old-receiving]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/core_workflow/receiving.g5.md
[old-approvals]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/core_workflow/approvals.g5.md
[old-shopping]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/core_workflow/shopping_and_checkout.g5.md
[old-fields]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/data_model/field_configuration.g5.md
[old-drift]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/operations/contract_review.g5.md
[old-users]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/security/users_and_groups.g5.md
[old-products]: /Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/comparison/pre-tla/.g5/ontology/software_architecture/product_boundaries.g5.md
[receipt-requirement]: </Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/docs/analysis/Receiving/Create Receipt from Receiving Area/Create Receipt from Receiving Area.md>
[receipt-index]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/src/Orders/Orders.Infrastructure/Database/Receiving/ReceivingView.cs
[overreceive-design]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/docs/designs/120543-over-receive-purchase-orders.md
[overreceive-code]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/src/Orders/Orders.Domain/Receivings/OverReceivePolicy.cs
[receivable-code]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/src/Orders/Orders.Adapters.Rest/Receiving/Queries/GetReceivablePODetails/GetReceivablePODetailsQueryHandler.cs
[change-code]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/src/Orders/Orders.Adapters.Rest/Orders/Commands/Change/ChangePurchaseOrderCommandHandler.cs
[budget-code]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/src/MasterData/MasterData.Adapters.Rest/Accounting/Budgets/Queries/GetBudgetsByLineItem/GetBudgetsByLineItemQueryHandler.cs
[udf-code]: /Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/src/Requisitions/Procurement.Requisitions.Adapters.Rest/Commands/Udf/UdfListMembershipValidator.cs
[approval-requirement]: </Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/docs/analysis/Approval Center/Approval Center of Procure To Pay platform.md>
[independence-adr]: </Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform/docs/architecture/decisions/0084 Cross-product runtime independence.md>
