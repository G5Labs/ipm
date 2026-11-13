# Ontology and harness evidence reassessment

**Follow-up:** The [evaluator adjudication](ontology-evaluator-adjudication.md) now resolves
the decisive ambiguities discussed here. It withdraws the unsupported consent deduction
and blanket progress failure, and scopes the old-key judgment to logical request identity.
The sections below retain the reasoning and proposed experiments that led to that step.

**Subsequent study:** The [balance study](../balance-study/README.md) now separates design,
fresh implementation, independent review and directed repair. It includes a matched
prior/current review-skill pair on identical source and ontology. Read its conclusions
for current evidence; the proposed pilot below remains the historical plan.

## Decision summary

This reassessment responds to the challenge to Experiment 7's second-run scores and the
follow-up questions about benefit, regression, cost, generalization and next steps.
It changes analysis and the proposed experiment plan only. Application code, ontology,
skills, frozen candidates and historical score ledgers are unchanged; no new experiment
or application test was run for this reassessment.

1. **There is positive, limited evidence that the ontology/workflow bundle is useful.**
   Several concrete improvements were found before supervisor intervention. Calling
   everything merely “unproven” understates this evidence. The magnitude, reliability
   and contribution of individual components remain uncertain.
2. **The latest revision has not demonstrated an overall correctness gain.** It also
   has not demonstrated a causal regression. Its failure to prevent unrelated-work
   cancellation is meaningful negative evidence about that specific intended mechanism.
3. **Keep the revision provisionally; do not revert it wholesale.** Retain the compact
   semantic distinctions and reduced metadata ceremony. Treat the revised review
   guidance as an intervention whose effectiveness needs direct comparison. Do not add
   another layer of instructions in response to these results.
4. **Observed Experiment 7 cost is modest.** Across its two pairs ontology takes only
   28 seconds more total worker elapsed time and uses about 2.9% fewer output tokens.
   This descriptive total does not estimate marginal harness overhead or billing cost.
5. **Repeating a training case is useful.** It tests whether retained learning addresses
   the original failure. A separate, unseen case tests transfer. Neither question
   invalidates the other.
6. **Validate the evaluator, then run a small comparison of old and revised guidance
   on identical review inputs.** Full feature trials and real-guest infrastructure are
   unnecessary for the first causal question.

## Scope and evidence standard

Reviewed the six reported paired comparisons: experiments 1, 2, 5, 4 and both runs of
7. Candidate numbers and chronological experiment labels differ: experiment 1 is stale
Save (candidate 2), and experiment 2 is allocation ownership (candidate 3). The earlier
ontology-only stale-Save rerun is considered separately because it reused a solution.

The analysis uses comparison receipts, original designs, preserved pre-repair findings,
supervision records, the exact revision patch, and selected original public trace
events. In particular, the host-drain policy/callback events, both experiment 4 message
timelines, allocation review commands, and second-run experiment 7 decision messages
were inspected. This is a targeted cross-experiment audit, not a claim to have reread
every repeated tool-output line. Public actions establish retrieval, choices and edits;
they do not reveal private reasoning or establish why a particular choice occurred.

Historical reports contain evaluator judgments under reconsideration below. A saved
report is evidence of what was measured and claimed, not independent proof its expected
behavior was correct.

## 1. What the traces actually contribute

| Experiment and trace | Observation | What it supports | Limit or contrary evidence |
|---|---|---|---|
| **1: stale Save, ontology's first returned self-review** | Before later master repair, review found stale cache publication, Save responses acknowledging a replaced VM, HTML proposal identity and stale reload responses. It repaired these and added focused checks. | Concrete autonomous review value across existing consumers; the ontology bundle did more than produce a plan. | Later master help supplied runtime setup and discovered broken New File/New Folder callers. The whole final advantage cannot be credited to the worker alone. |
| **1: plain public messages and common probes** | Generic review found unmount-related buffer loss and other Save adapters; repair broadened coverage. Final common probes still distinguished delayed-response/cache/HTML behavior: 10/10 versus 2/10. | Both reviews helped; ontology's broader response-ownership treatment is a real observed advantage. | Cases were retrospective and correlated. This is not eight independent wins or proof of the planning phase's isolated effect. |
| **Earlier stale-Save ontology rerun** | Supervision records and file comparison confirm prior design/review access and actual implementation reuse. | Retained solutions can speed practical work. | This rerun cannot count as a fresh successful replication of harness-generated discovery. It is materially different from learning a general rule. |
| **2: allocation ontology self-review, before master repair** | The worker identified and bound an initial workflow input to the task consumed by dispatch. Plain retained the unbound input. | A consequential producer-to-consumer improvement arose autonomously in the ontology condition. | Owner validation was initially incomplete; the master supplied the malformed-spec and retry-default findings. |
| **2: plain review commands at 02:32–02:35 UTC** | Plain inspected schema defaults and repaired ordinary retry counts, a regression the ontology worker needed master help to repair. | Native review independently handled an important compatibility case. | Plain still missed input binding and complete allocation validation; disk/setup conditions also limited its own runtime evidence. |
| **5: O025/O031/O116/O117/O133 versus P022–P024** | Ontology read that `Drain` permits deadline loss, changed its admission assertion to match the nudge, and left ordinary deletion using that policy. Plain read the same policy and chose non-loss `User`. | A precise failure to apply available knowledge. It identifies a tractable intervention: compare the chosen policy's effects with the caller's authorized outcome. | Retrieval alone is insufficient. There is no evidence ontology caused the bad choice; the current revision was not yet present. |
| **5: O095 versus P090–P094** | Ontology inspected the host-control file for another purpose but missed `host_drained` writing termination before cloud teardown. Plain review inspected and repaired this alternate writer. | Review selection matters more than merely reading a file. An actual ordered counterexample can expose this without a real cloud or race scheduler. | Generic review succeeded on a failure the ontology workflow missed. |
| **5: ontology force review and P107–P108** | Ontology repaired missing exit proof, stopping-state settlement and durable loss audit. Plain retained force/provider-failure problems and introduced a stale-host replacement regression after searching shared-helper callers in only one file. | Ontology supplied broader custody/audit coverage; preserving valid callers is also a concrete review need. | Both retained consequential defects. Equal 6/8 probe totals and the 4.4/2.5 score conceal different consequences. |
| **4: both public timelines, 23:47–23:54 UTC** | Both independently found that receipt replay lookup and uniqueness omitted epoch, then repaired and checked it. Both fixed release settlement through the actual VM-row transition. | Native review and revised ontology review both worked on this scope. The revised workflow was usable. | Most surrounding safeguards already existed; only 64/73 production additions were needed. This was a weak discriminator for the original complex-integration hypothesis. |
| **7 first run: original review and O211** | Ontology preserved reader access and separated its checked-fork key; plain did not. Ontology also tested Stop by directly incrementing generation, leaving the real public Stop early return broken. | Positive outcome differences coexist with a concrete verification mistake that the revision targets. | The old-key property's product semantics need explicit scoping; direct generation mutation never proved the owner transition. |
| **7 second run: ontology messages 00:27:51 and 00:33:17 UTC** | The worker noticed Start's effect-time clock and preserved the submission deadline; source inspection also prompted a prospective-child launch-configuration amendment. | Useful application of reuse inspection and consequential design maintenance. | The original ontology run already handled the original deadline in final code. These actions alone do not establish an improvement caused by the revision. |
| **7 second run: review finding 2, exact-intent repair and common cancellation probe** | The worker correctly protected a later manual Start from stale cancellation, but cancelling the compound operation still held an unrelated chat under the same original Start. | The relevant rule was retrieved, and the agent found one valid counterexample, but its scope analysis stopped too early. This is direct negative evidence against reliable prevention by the revised wording. | The scoped repair itself is useful. One miss does not establish zero average benefit or an agent reasoning ceiling. |
| **7 second run: review finding 3 and final `reconcile`** | Review claimed its `updated_at` rotation repair was effective, but the UPDATE transaction rolls back. | There are ordinary code/evidence errors independent of ontology semantics. More governing prose is unlikely to repair transaction reasoning by itself. | This is a source finding; a 17-operation runtime outcome was not executed and was not a frozen scored case. |

Trace sources:

- Experiment 1: first self-review (local run record `runs/2026-09-26-stale-save/first-returned-delivery/self-review.md`),
  plain public history (local run record `runs/2026-09-26-stale-save-baseline/worker-public-history.json`),
  supervision (local run record `runs/2026-09-26-stale-save/supervision.md`),
  common comparison (local run record `runs/2026-09-26-stale-save-baseline/comparison.md`).
- Stale-Save rerun: supervision and reuse disclosure (local run record `runs/2026-09-26-stale-save-harness-v2/supervision.md`),
  file comparison (local run record `runs/2026-09-26-stale-save-harness-v2/implementation-reuse-comparison.json`).
- Experiment 2: preserved review phases (local run record `runs/2026-09-26-allocation-ownership/final-self-review.md`),
  supervision (local run record `runs/2026-09-26-allocation-ownership/supervision.md`),
  plain public tool receipts (local run record `runs/2026-09-26-allocation-ownership-baseline/final-reviewed/public-tool-receipts.json`),
  comparison (local run record `runs/2026-09-26-allocation-ownership-baseline/comparison.md`).
- Experiment 5: [history audit and event references](ontology-experiment-5-history-audit.md),
  original ontology events (local run record `runs/2026-09-27-host-drain-plain/history-audit/ontology-readable.json`),
  original plain events (local run record `runs/2026-09-27-host-drain-plain/history-audit/plain-readable.json`),
  evidence (local run record `runs/2026-09-27-host-drain-plain/comparison-evidence.md`).
- Experiment 4: ontology public history (local run record `runs/2026-09-27-experiment4-flush/ontology/final/public-history.jsonl`),
  plain public history (local run record `runs/2026-09-27-experiment4-flush/plain/final/public-history.jsonl`),
  evidence (local run record `runs/2026-09-27-experiment4-flush/comparison-evidence.md`).
- Experiment 7: [first history audit](ontology-experiment-7-history-audit.md),
  second public history (local run record `runs/2026-09-28-experiment7-second-run/ontology/final/public-history.jsonl`),
  second preserved review (local run record `runs/2026-09-28-experiment7-second-run/ontology/final/worker-artifacts/ontology-review.md`),
  second evidence (local run record `runs/2026-09-28-experiment7-second-run/comparison-evidence.md`).

### Synthesis

The positive pattern is broader attention to retained user intent and consumers: delayed
Save responses, task/input association, force custody/audit, reader access, and launch
configuration. The negative pattern is incomplete application at the actual decision:
which Stop policy, which terminal writer, which existing caller, which other work shares
the resource. Both patterns appear before and after self-review.

An actionable interpretation is **useful knowledge with inconsistent decision coverage**.
There is no evidence that these failures are inherently beyond agent understanding:
the other condition solved several, and the missed cases have small counterexamples.
This supports improving when knowledge is applied, while avoiding another generic
instruction that merely repeats what both old and revised harnesses already request.

## 2. Benefit, regression and scores

The historical rubric yields 3.1 versus 1.9 in first7 and 3.1 versus 2.5 for second7
under its original checks. Those observations are positive comparative evidence, not
proof of a stable numerical effect size. They should not be dismissed because the
experiment lacks every causal control. Conversely, they cannot establish that the
revision improved the ontology condition: its original selected probe total stayed
11/13, while plain stayed 9/13 with different failures.

The added second-run consent deduction is disputed. The
[lifecycle contract](../../workspace-vm-api-v9.md#lifecycle-admission-start-intent-and-stop-durability)
says the 15-minute Start intent expires before attachment and is consumed at readiness.
The new test (local run record `runs/2026-09-28-experiment7-second-run/consent-discriminator-ontology-final.rs`)
requires chat refusal after a timely consumed Start on still-running compute. A separate
chat-allocation deadline is not clearly established by the frozen requirement. The
previous report's assertion of a confirmed defect is withdrawn in this reassessment;
the historical test result remains a valid observation of behavior.

Two further distinctions matter:

- **Progress:** an orchestration record completing at allocation is not necessarily a
  false claim that the agent is running. Existing VM and chat components expose capacity,
  queued input and delivery uncertainty. Assess the composed user journey and recovery
  controls before treating the new banner's narrower projection as an absent capability.
- **Old action key:** both second-run candidates fail the frozen action-conflict criterion.
  The test sends a new explicit compound request, so clarify key scope and whether
  adoption of an old child is forbidden before calling it unauthorized replay. The
  first ontology candidate's protection is an observed benchmark advantage; the broader
  product judgment needs this qualification.

No replacement aggregate score is assigned here. Broad rows mix missing evidence,
correctness and completeness; one defect can zero a row or affect several rows. Unknown
runtime outcomes should remain separate from reproduced defects. Exp. 5 further shows
that a higher aggregate can coexist with a worse ordinary-delete data-loss behavior.

The revision's cancellation-prevention mechanism failed in the second run. That is
stronger than saying “nothing can be concluded,” but weaker than saying it has no value.
No old-versus-revised matched review of the same candidate has yet been performed.

## 3. Exactly what changed, and what the repeat tests

The recorded revision patch (local run record `runs/2026-09-28-experiment7-second-run/context-changes-since-first7.patch`)
separates these changes. Its AGENTS diff also includes experiment-isolation instructions;
those should not be mistaken for new general product-workflow requirements.

| Change | Generality and origin | Evidence after revision | Current recommendation |
|---|---|---|---|
| Keep consequential reuse assumptions in the design; inspect actual policy/effects before reuse | General; exp. 5 loss-policy mismatch and exp. 7 Stop | Exp. 4 inspects release owners; exp. 7 notices consent clock and child configuration | Keep provisionally; direct old/new comparison needed |
| Select review by reused guards, shared-state writers/consumers and deferred effects | General; callbacks, helper callers and delayed authority in both audits | Applicable paths were inspected; cancellation scope still missed | Keep as the current experimental version; do not claim reliable improvement |
| Check that tests exercise the owner transition and distinguish the relevant guard | General; exp. 5 seeded release/busy-host guard and exp. 7 direct generation mutation | Exp. 4 checks actual release; second7 public Stop works | Useful observed adherence; both plain conditions can also do this |
| Distinguish operation purpose/grant from shared-resource state | General; exp. 5 drain/delete/rotation and exp. 7 shared-compute cancellation | Rule retrieved in second7; newer Start protected, unrelated chat still affected | Valid distinction; wording alone insufficient in this case |
| Bind replay to operation kind and accepted intent | General principle; old Fork-only receipt case supplied motivation | Second7 does not satisfy the frozen old-action check | Define namespace/adoption semantics before scoring the specific interpretation |
| Derive outcome claims from their authoritative evidence | General; exp. 5 local drain versus cloud completion and exp. 7 progress | Rule retrieved; second7 UI claim needs broader evaluation | Keep principle; reassess the alleged exp. 7 violation |
| Explicitly cover Stop while compute is stopped but consent queued | Domain rule, applicable beyond Fork & Start | Both second7 candidates fix it | Useful retained domain knowledge; no exclusive ontology advantage shown |
| Footer names `workspace_vm_service.rs::admit_stop` and its early return | Exact known baseline defect from exp. 7 | Ontology received a direct location/condition hint | Retain accurate production knowledge; label this as learned-task evidence |
| Update only changed metadata assessments; make percentages optional | General overhead/reporting simplification | Exp. 4 and second7 use scoped qualitative metadata | Keep; visible adherence, no isolated time-saving measurement |

The maintained AGENTS/planning/review/format text decreased from 3,617 to 3,592 words;
two compact ontology commitments were added. This is mainly a refocusing of existing
guidance, not a large expansion of instructions. Text size alone does not measure effort.

“Benchmark contamination” was too broad a dismissal. Distinguish three valid questions:

1. **Does the system learn from a failure and handle it on repetition?** Repeating 7
   is appropriate, including accurate retained defect knowledge. The cancellation miss
   is disappointing evidence on exactly this question.
2. **Does general guidance transfer to a new interaction?** Use an unseen task or a
   genuinely different authority/lifecycle variant; an exact Stop hint cannot answer it.
3. **Did this revision cause a change?** Compare old and revised conditions directly,
   ideally on the same source or review snapshot with repeated fresh workers.

The second7 run compares revised ontology against plain. Comparing it with first7 adds
an old-version observation, but only one sample per version. It is useful evidence, with
weak causal resolution. Plain's own wins/losses show why unchanged totals cannot settle
the revision's average effect. Actual patch reuse in the earlier stale-Save rerun is a
separate and stronger source of solution exposure; it was not observed in second7.

## 4. Cost should be judged against useful outcomes

| Trial | Ontology elapsed | Plain elapsed | Absolute difference | Output tokens, ontology / plain |
|---|---:|---:|---:|---:|
| Exp. 5 | 35m37s | 12m55s | +22m42s | 90,511 / 42,191 |
| Exp. 4 | 14m47s | 9m00s | +5m47s | 45,347 / 25,006 |
| Exp. 7 first | 42m45s | 48m26s | −5m41s | 124,164 / 146,302 |
| Exp. 7 second | 42m46s | 36m37s | +6m09s | 120,523 / 105,788 |
| **Both exp. 7 pairs, descriptive total** | **85m31s** | **85m03s** | **+28s** | **244,687 / 252,090** |

The first two experiments also have additional supervision and environment differences,
so their elapsed gaps are especially unsuitable as estimates of ontology overhead.
Even above, broader implementation, review/repair paths, builds and shared hardware
contribute. No direct monetary cost or ontology-only effort was measured.

The user's tolerance for modest additional effort changes the practical decision:
preventing one consequential defect can justify several extra minutes. It is not
necessary to establish a speedup to justify keeping useful guidance. Exp. 5 still warns
that large extra effort can coexist with dangerous omissions; cost should not disappear
from the record. There is currently no strong cost-based reason for a blanket revert.

## 5. Updated: what we should do before changing anything

### Execution update: what was learned before further procedure changes

The evaluator audit is complete for the decisive disputed assertions. The later
[study](../balance-study/README.md) adapted the proposed pilot to the user's design,
implementation and review questions: nine fresh designs, two fresh implementations,
four reviews of the same Experiment 7 artifact (including the matched prior/current
skill pair), and two independent reviews of the new I-O implementation. Two directed
review follow-ups distinguish attention from an inability to trace the defects; a
bounded repair separately tests whether concrete feedback is actionable. A further
answer-blind selector plus guided fresh review did not recover the known misses and
introduced misleading leads; it does not justify a mandatory selection stage. The suggested
12-run pilot below was **not** executed as written.

Current decisions are to retain accurate scoped knowledge, preserve native coding,
and avoid stacking new generic harness instructions. Restored exceptions helped their
design repair case; both implementations still missed explicit consumer-level promises.
The matched review pair did not establish a detection benefit or regression for the
revision. Future harness evaluation should test whether a short, answer-blind boundary
selector improves review of unseen changes at acceptable total cost, including false
restrictions and permitted variants. This is a narrower question than another full
Experiment 7 score comparison. Detailed decisions and supervision limits are in the
study's conclusion; its frozen reports preserve successes, misses and reviewer errors.

### A. Finish a bounded evaluator audit

Reclassify the existing evidence by concrete user outcome: demonstrated success,
demonstrated defect, ambiguous requirement or unexecuted boundary. Trace each decisive
assertion to its contract. Resolve Start consumption versus chat expiry, action/key
scope, and the complete progress UI before further ranking. Preserve the original
ledgers and add explicit corrections rather than silently replacing history.

Prioritize high-confidence cases already supported by the traces: unauthorized ordinary
deadline loss, false provider completion, unrelated-work cancellation, input/task binding,
stale response ownership and valid shared-helper callers. These do not require a real
guest to analyze or exercise at their owning boundaries. Full runtime coverage remains
necessary for claims about real guest/provider execution, not for every useful comparison.

### B. Compare the revision directly on fixed review inputs

Suggested **small diagnostic pilot**, not yet dispatched:

- Use three preserved review cases: exp. 5 ordinary-delete loss policy, exp. 5 alternate
  terminal writer/shared-helper consumers, and exp. 7 unrelated-work cancellation.
  Each case includes the normal task goal and sufficient surrounding code; workers are
  not told the defect or given evaluator probes. Cases from one feature are not treated
  as independent feature-level replications.
- Give fresh workers either the prior or revised harness/general commitments. Keep
  application bytes, domain facts, model settings, generic task request and tools equal.
  Hold exact known-defect footers equal or omit them from both experimental snapshots.
  Only the guidance/content intended for comparison differs. Keep production ontology
  intact. Do not allow prior histories, patches or findings to leak into either arm.
- Two independent starts per version per case produce **12 bounded review runs**.
  This is a diagnostic sample, not a promised statistically decisive study. Predeclare
  equal effort ceilings and report unfinished runs rather than silently extending one
  arm. Provide warm, isolated builds and the existing feasible database fixtures.
- Evaluate frozen results with the same contract-validated checks. Each review input
  also contains a closely related, already-correct path; exercise that path as a control
  so over-restrictive “fixes” and false findings are visible. Choose these controls before
  delivery, not to favor a candidate.
- Record: identified before a hint; correct boundary inspected; valid counterexample
  attempted; defect actually fixed; allowed behavior preserved; new defects; tokens and
  elapsed effort. Grade correctness blind to condition where practical. Merely citing a
  contract, creating a table or claiming review completion earns no correctness credit.

This comparison isolates the marginal revision on review/repair much better than another
fresh ontology-versus-plain feature. It does not measure how planning changes the initial
implementation or prove transfer. If the revised guidance still misses its targeted
defects repeatedly, stop crediting it with that benefit; simplify or replace the ineffective
instructions instead of stacking more requirements on them.

### C. Test transfer only after the mechanism is understood

Use a small number of new boundary variants or one genuinely difficult new feature whose
baseline and runnable outcomes have been inspected. Compare prior and revised bundles
directly, with multiple fresh starts. Keep relevant allowed paths, revoked authority and
unrelated consumers in the evaluator, without supplying the solutions to workers.

If the question becomes whether retained ontology itself earns its cost, add a plain arm
at this stage. A complete ontology × planning × review factorial is unnecessary for the
current keep/revert decision. Isolate another component only when its contribution would
change what we retain. Do not immediately launch experiment 6 merely because it is next
in the list; its privilege/process fixture must support the outcome under examination.

### D. Use practical decision rules

| Evidence obtained | Decision it would support |
|---|---|
| Revised guidance fixes additional material defects on matched inputs, preserves valid behavior and has acceptable cost | Keep it; test transfer before claiming broad reliability |
| Revised guidance repeatedly misses the same target despite retrieval and concrete inspection | Rework the timing/application mechanism; do not add another abstract rule |
| Extra artifacts appear without better decisions or outcomes | Remove or shorten that ceremony while retaining valid domain commitments |
| A new rule or interpretation reliably causes overrestriction or a regression | Revert or narrow that specific change |
| Old and revised perform similarly on an easy task | Treat it as a regression/control case; draw little conclusion about hard integration |
| Outcomes remain mixed after the pilot | Report the uncertainty; add the few runs that resolve the decision rather than a broad new benchmark campaign |

The aim is an engineering decision with defensible evidence, not proof of universal
superiority or exhaustive correctness across every interleaving. The current evidence
supports retaining a compact ontology/workflow and testing the revision more directly.
It supports neither dismissing the observed benefits nor declaring the revised harness
validated. No revert or additional harness change is made by this report.
