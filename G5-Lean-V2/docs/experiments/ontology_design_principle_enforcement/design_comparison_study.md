# Design comparison study: experiments and results

Updated 9 October 2026. These comparisons examine plain design, G5 without TLC,
and G5 with explicitly requested TLC. Design authors used GPT-6 Astra / xhigh;
the two #246 implementation/review authors used GPT-6.1 Sol / xhigh.

**No general treatment winner is established.** G5 sometimes produced smaller
ownership and storage designs; TLC added useful bounded checks and some concrete
refinements. Plain remained competitive. The strongest findings are specific
architectural differences and reproduced defects, not aggregate design scores.

See the [experiment overview](design-evaluation-candidates.md) for the cases and
[PairSoft results](pairsoft-harness-experiment-takeaways.md) for the separate domain study.

## Design comparison results

| Case | G5 without TLC | G5 with TLC | Plain | Conclusion |
|---|---|---|---|---|
| #246: conversation recovery | Current file rows, one manifest, existing execution owner | Immutable whole-bundle generations and retention machinery | Staged generation files, checkpoint lifecycle and installation marker | Prefer current storage under the required capture/admission fences; this is a fundamental implementation difference. |
| #339: Stop and Start | Original run found more concrete Git-lock and host-cache dependencies | Useful cleanup simplification and stronger bounded evidence; later repeats caught important dependencies | Credible cutoff/recovery design, with unresolved implementation dependencies | Repeats support a practical tie or possible slight TLC improvement, not the original single-run ranking. |
| #186: Git authority and synchronization | Sole membership map, Git refs, exact saved revisions and provider reconciliation | Same main architecture; useful checked recovery and capture-deadlock refinement | Same core design with more operation/preparation state | All six designs converge; no demonstrated overall winner. |
| PM-1: collaboration | Reuses workspace/receipt mechanisms; preserves bounded Stop | Removes a redundant recovery selector; no-TLC independently avoids it | More runtime/operation state and a conflicting failed-save retention policy | Credible common architecture; different unresolved product and integration boundaries. |
| PM-2: persistent demos | Independent hosting, immutable releases, isolated visitor state | Same core architecture with compact ownership and a narrower assembled-artifact capture contract | Same core architecture, stronger capture promise and more workflow state | Capture scope was chosen before modeling; fewer states alone does not establish superiority. |
| PM-3: coordinated releases | Immutable releases and protected provider receipts; separate publication operation | Release revision owns publication; receipt also settles cancellation races | Immutable releases with more run/attempt state and unresolved remote ambiguity | TLC offers a useful local simplification and recovery mechanism, conditional on actual provider support. |
| #289/#293: documents and reviews | Existing tasks with pinned readers and server publication | Checked child workspace and generated-artifact publication | Isolated review module with runtime-mediated publication | Stored document roots converge; execution confinement and publication recovery remain consequential differences. |

## Concrete limits and findings

| Case | Important unresolved or discriminating finding |
|---|---|
| #246 | All designs preserve native continuation, but full provider file completeness and exact recall of a fact found only in a tool result remain unqualified. Current-row reuse requires capture quiescence and admission fencing. |
| #339 | Deadline enforcement must include independently owned Git work. Retries must overcome cached nonterminal host failures. A model can pass while falsely reporting a completed save as lost. |
| #186 | A synchronous Save can wait on the terminal/agent that called it. One TLC repeat resolves this capture deadlock; broader Git/admission lock composition remains conditional. One TLC design substitutes merge-target semantics for the required parent view. |
| PM-1 | No-TLC leaves child Project Docs identity unresolved; TLC leaves repeated partial incorporation/rebase incomplete; plain changes existing bounded Stop behavior without reconciliation. |
| PM-2 | Coherent source capture is a stronger, unresolved capability than reviewing an assembled artifact. Plain's retirement design needs an explicit fence against acquiring new references after retirement begins. Hosting/containment remains unverified. |
| PM-3 | Atomic branch-plus-receipt publication, protected receipt retention and native writer participation remain provider/integration premises. Plain and no-TLC have weaker cancellation resolution than TLC's receipt fence. |
| #289/#293 | No-TLC's native input confinement remains conditional and its stated artifact size cap is wrong. TLC's single-review freshness policy prevents multiple required reports from coexisting; its exact-rewind recovery can republish. Plain omits a needed native-writer fence repair and lacks equally concrete erased-acceptance evidence. |

### Stop and restart findings

The original #339 no-TLC run found Git-lock and host-cache dependencies missed by
its counterparts. A later TLC-only rerun followed the saved-design/model/final-review
order and strengthened false-loss assertions, but still missed those dependencies.
Two additional runs per G5 condition varied materially: both no-TLC repeats missed
the Git-lock dependency; the second TLC repeat resolved it and had the strongest
failure/implementation-fit argument among those four. This variation prevents a
stable method ranking or attribution to review ordering alone.

## What formal checking established

| Case | Executed evidence | Practical limit |
|---|---|---|
| #246 | 21 original TLC outcomes independently reproduced | Native file completeness, writer quiescence and real provider behavior remain premises. The original models did not compare current-row storage against retained generations. |
| #339 | Rerun improved false-loss sensitivity; repeats explored different recovery arguments | Passing original assertions did not exclude false-loss reporting; source-lock and cache dependencies could remain outside the model. |
| #186 | Bounded recovery checks and a concrete capture-deadlock refinement | Broader lock composition, runtime adapters and live checkout behavior remain implementation obligations. |
| PM-1 | 31 formal outcomes independently reproduced | Cross-model capture/authority composition and some repeated-integration behavior remain unqualified. |
| PM-2 | 26 formal outcomes independently reproduced | No full platform containment or capture verification; non-TLC arms supplied different useful local pilot evidence. |
| PM-3 | 33 formal outcomes independently reproduced | Provider atomicity, protected receipts and composition with real Git remain assumptions. |
| #289/#293 | 25 author outcomes audited against saved sources, hashes and logs; not independently rerun | Exact rewind and multiple simultaneous review requirements were outside the publication model. |

These counts include expected-failing controls and reachability witnesses, not just
passing safety checks. A control that removes a guard demonstrates sensitivity;
it is not automatically discovery of an unknown product defect. TLC checks supplied
alternatives and properties; it does not generate a different architecture by itself.

## Author effort

Elapsed time covers the full design author turn, excluding evaluator work. Different
investigation, artifact effort and shared-host effects prevent an isolated efficiency claim.

| Case | G5 without TLC | G5 with TLC | Plain |
|---|---:|---:|---:|
| #246 original | 13m 15s | 14m 41s | 8m 35s |
| #186, two runs each | 11m 16s–11m 32s | 15m 31s–23m 00s | 8m 58s–9m 34s |
| PM-1 | 17m 43s | 21m 54s | 14m 19s |
| PM-2 | 17m 10s | 18m 16s | 12m 22s |
| PM-3 | 13m 49s | 20m 20s | 11m 16s |
| #289/#293 | 19m 18s | 23m 54s | 20m 03s |

For #289/#293, TLC used about 17% more uncached-input-plus-output tokens and took
24% longer than no-TLC. Those are whole-turn measurements, not TLC execution cost
or a causal price for improved design quality.

## Conversation recovery implementation and review

The #246 no-TLC and plain designs were implemented separately, then each original
author performed the same requested fixing review. There was no TLC implementation
arm. **The smaller G5 storage design survived implementation and review.**

| Dimension | G5-seeded implementation | Plain-seeded implementation |
|---|---|---|
| Durable checkpoint storage | Current file rows and a session manifest | Retained byte generations and a separate checkpoint table |
| Capture ownership | Existing execution owner | Additional nested provider owner |
| Additional bookkeeping | No per-turn byte archive | Unused restore installation marker; no pruning found for nonzero generations |
| Remaining concrete defect after review | Changed module catalog retains stale deployed hash pins; existing test still fails | Linux cleanup consumes a one-use reap receipt twice; repeated cleanup still stalls |
| User-flow coverage after review | Workspace carry selector added, with public-route and browser coverage | Workspace carry API exists, but fork UI still lacks conversation selection |
| Shared qualification gap | Exact tool-result-only recall and complete pinned provider layouts remain unproved | Same; full deployed-VM qualification also absent |

The Linux cleanup finding is a component reproduction plus caller inspection,
not an entire VM hang reproduced end to end. G5's missing referenced Claude-file
check also remains a provider-completeness gap. Both outputs still need fixes.

| Measurement | G5 | Plain |
|---|---:|---:|
| Initial implementation elapsed | 2h 44m 22s | 2h 08m 37s |
| Initial uncached input + output | 1,021,833 | 938,816 |
| Initial production/schema diff | +2,502 / −427 lines | +3,321 / −472 lines |
| Implementation plus review elapsed | 3h 17m 31s | 2h 59m 10s |
| Implementation plus review uncached input + output | 1,668,532 | 1,596,403 |
| Final production/schema diff | +2,731 / −428 lines | +3,353 / −473 lines |

G5 had stronger Linux dispatcher/API/database evidence. Plain added broader UI scope
and reused selected historical implementation changes. Its resumed review also loaded
global G5 wrappers, so that stage was no longer a clean no-skill control. Code size,
coverage counts and these mixed conditions do not establish fewer bugs or isolate
ontology's causal contribution.

## Human-readable plans and narrow harness experiments

A fresh #289/#293 run with a minimal request and explicit concise-output guidance
reduced the main plan from **7,965 to 3,220 words (59.6%)**. Companion prose fell from
505 words to none; ontology diff-added words increased from 1,712 to 1,936. Both
prompt and skill changed, so their individual effects are not isolated.

The remaining density is specific: the review lifecycle uses 1,001 words, navigation/
editing/attribution 463, and acceptance/closing review 363. Detailed operation contracts
repeat between plan and ontology. Ownership, publication, recovery and genuine product
assumptions should remain visible; useful acceptance histories should not be discarded.

| Probe | Result | Harness decision |
|---|---|---|
| 8 October: six authors testing capability checks, outcome-sensitive tests and reuse | Both conditions found the important counterexamples and same core designs, including an unplanned stale-retry defect | Consolidate overlapping ownership/reuse guidance, 637 → 583 words. Do not adopt the extra uncertainty/check paragraph. The exact consolidation was not separately resampled. |
| 9 October S: explicitly consider removing an owner or commit boundary | Same main architecture in all three paired cases | Do not adopt |
| 9 October B: put detailed contracts in ontology and link them | Plans shortened 22–34%, but two cases added more ontology prose than they removed from the plan | Do not adopt |
| 9 October C: explain choices once and omit repeated walkthroughs/inventories | Both plan and ontology shrank; plan reductions were 16%, 3% and 13%, below the predeclared threshold | Do not adopt |

The 9 October comparison contains twelve fresh design-only author runs across
checkpoint, activation and payment cases. The original controls are reused across
candidate comparisons; these are not independent replications of the controls.

| Case | Current plan | B plan | B ontology change | C plan | C ontology change |
|---|---:|---:|---:|---:|---:|
| Checkpoint | 1,559 words | 1,221 | +375 words | 1,307 | −413 words |
| Activation | 1,489 words | 1,135 | +558 words | 1,451 | −299 words |
| Payment | 1,509 words | 998 | +87 words | 1,310 | −310 words |

No companion design prose was produced. Ontology counts include metadata. Adoption
required at least 20% less plan-plus-companion prose in two cases, preserved decisions,
and no comparable displacement elsewhere. B failed displacement; C failed the plan
reduction threshold. These are adoption heuristics, not significance tests.

A real local structural improvement appeared in C's activation design: derive audit
and current catalog from immutable terminal submission results. This removes maintaining
three stored representations consistently while preserving rejected-result replay.
The architectural guidance was unchanged, so this does not establish that C caused
better search. C's single-value checkpoint alternative retained an unresolved aggregate
size premise; current file rows avoid that premise. Payment's derived key still requires
permanent identity and namespace guarantees. Fewer fields alone is not a better design.

## Earlier design-review comparison

A separate 1 October G5 review comparison used Claude Opus 5.5 / max. Labels were
LLM-evaluated and provisional; human adjudication remained pending.

| Measure | Formal-style reasoning | Design guide |
|---|---:|---:|
| Supported critical/high findings | 17/25 | 10/25 |
| Harmful claims | 7 | 2 |
| Precision | 39/45 | 30/31 |
| Active time | 51 minutes | 25 minutes |

Formal-style reasoning found more serious issues but overreached more. The guide
was more precise and cheaper. Both simple-prompt attempts required salvaged reports
because of harness failures, so that condition cannot support a fair comparison.
Reference bias, disclosed issues and one run per condition limit the result.

## Harness conclusions

Retain a concise ontology diff, relevant governing constraints and consequences,
high-level design, and only consequential decision notes. The earlier four-category
output clarification remains; none of the 9 October clauses was adopted.

Inspect actual reuse guarantees and compare credible alternatives under the same
required outcomes. Keep one evolving design and native implementation. Separate
review and incremental TLA/TLC require their respective explicit requests; ordinary
implementation checks and required tests remain. The [PairSoft summary](pairsoft-harness-experiment-takeaways.md#narrow-harness-results)
records the routing evidence and rejected workflow additions.

The evidence supports specific simplifications and stronger scoped verification.
It does not establish a universal ranking, a measured bug-rate reduction, complete
production correctness, or a reliable brevity fix. Most comparisons have only one
or two author observations per condition, and important runtime/provider assumptions
remain unverified.
