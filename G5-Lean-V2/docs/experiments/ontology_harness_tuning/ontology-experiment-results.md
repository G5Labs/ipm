# Ontology experiment results

Detailed artifacts in `docs/experiments/runs/` are local-only and ignored by Git.

**Evaluator correction:** The [contract and UI adjudication](ontology-evaluator-adjudication.md)
withdraws Experiment 7's second-run late-chat consent deduction and both final runs'
blanket progress-failure rationale. Cross-action raw-key reuse is a compatibility
characterization until its logical identity scope establishes a violation. Scores below
remain historical ledger results, not current quality rankings. The
[cross-experiment reassessment](ontology-harness-evidence-reassessment.md) records benefit,
cost and the next experimental questions. Frozen candidates and raw receipts are unchanged.

**Follow-up investigation:** The [ontology/native balance study](../balance-study/README.md)
separates high-level design, fresh coding, independent review and informed repair, with
Sol experiments and Astra adjudication. It adds concrete outcome evidence and a matched
prior/current review-skill comparison instead of revising these historical scalar scores.

## Experiment 1

**Task:** Prevent stale editor Saves after VM replacement (candidate 2), using
GPT-6-sol xhigh from the same application snapshot in ontology and plain-coding conditions.

### Final implementation scorecard

Retrospective, evidence-adjusted assessment against [S01–S08](ontology-skills-evaluation.md#stale-editor-save):
1 = supported, 0.5 = partial support, 0-F = demonstrated violation, 0-U = unassessed.
Inspection or limited tests earn at most partial credit where runtime evidence is
required. Equal weights; score = points / 8 × 10. These are scoped assessments, not
general code-quality ratings or estimates of ontology's isolated effect.

| Criterion | Ontology + master | Plain + generic review | Evidence / gap |
|---|---:|---:|---|
| S01 Preserve observed identity | 1 | 0-F | Plain HTML proposals can adopt the replacement attachment at Apply. |
| S02 Preserve version/create checks | 1 | 0.5 | Ontology API cases pass; plain checks inspected, integration unexecuted. |
| S03 Reject equal-hash replacement Save | 1 | 0.5 | Ontology API regression passes; plain guard inspected/test compiled only. |
| S04 Fence admission-to-write replacement | 0-U | 0-U | Dedicated controlled race unexecuted in both. |
| S05 Preserve dirty work and recovery | 0.5 | 0-F | Ontology component probes pass, full page lifecycle unexecuted; plain loses dirty state/text. |
| S06 Preserve current authority | 0.5 | 0.5 | Guards inspected; ontology also tests non-owner refusal; transfer/revocation races unexecuted. |
| S07 Cover applicable Save adapters | 0.5 | 0-F | Ontology covers inspected paths with focused checks; plain HTML bridge remains unfenced. |
| S08 Keep valid Save usable/truthful | 1 | 0.5 | Both pass ordinary editor Save; only ontology has executed API success evidence. |
| **Total** | **5.5/8 → 6.9/10** | **2/8 → 2.5/10** | Ontology: 4 supported, 3 partial, 1 unknown; plain: 4 partial, 3 failed, 1 unknown. |

**Why the difference:** Both found the central server check. Ontology connected it to
other Save surfaces and lifecycle obligations; self-review repaired delayed-response,
cache and proposal ownership. Plain reviewed the narrower implementation and repaired
two reported gaps, leaving these related failures. Common retrospective probes passed
10/10 versus 2/10; failures share causes. Extra master guidance and executed integration
checks also improved the ontology result, so the score difference includes assistance
and evidence depth. Plain still received global skill descriptions, though use was prohibited.

**Design/time:** The ontology plan was readable and high-level. Worker time excluding
pauses/master work was 36m 57s versus 25m 41s; planning took 2m 07s. Additional integration
verification and caller repair took 14m 14s. Ontology reading was not the apparent bottleneck.

**Takeaway:** Keep compact planning and automatic review/repair; strengthen consumer
coverage and asynchronous ownership invariants. The harness still missed New File/New
Folder callers until master review. Pre-generation design's isolated benefit remains unresolved.

Full comparison (local run record `runs/2026-09-26-stale-save-baseline/comparison.md`)

## Experiment 2

**Task:** Restore TaskService allocation ownership (candidate 3), using GPT-6-sol xhigh
from the same application snapshot in ontology and plain-coding conditions.

### Final implementation scorecard

Retrospective scoring using the existing [A01–A08 rubric](ontology-skills-evaluation.md#taskservice-allocation):
1 = supported, 0.5 = partial evidence without a known violation, 0-F = violated/control
absent. Equal weights; score = points / 8 × 10. This assesses scoped final-output evidence;
unequal supervision prevents treating the score difference as ontology's isolated effect.

| Criterion | Ontology + master | Plain + generic review | Evidence |
|---|---:|---:|---|
| A01 Complete allocation owner | 1 | 0-F | Plain moves writes but lacks supported-spec validation. |
| A02 Session composition/identity | 1 | 1 | Both compose ChatService in the caller transaction; session cases pass. |
| A03 Partial graph rollback | 1 | 1 | Both pass real database rollback and late-child failure checks. |
| A04 No dispatch before commit | 1 | 1 | Both refuse uncommitted/rolled-back work; capture sees no invocation. |
| A05 Replay/conflict | 1 | 1 | Both pass identical replay and changed-body conflict checks. |
| A06 Admission/held-session rules | 0.5 | 0.5 | Valid session checks pass; full authority/held/concurrency cases unexecuted. |
| A07 All inventoried allocation paths | 0-F | 0-F | Existing IssueService bypass remains outside this PR in both. |
| A08 Correct existing job behavior | 1 | 0-F | Plain retains unbound workflow inputs, blocking committed dispatch. |
| **Total** | **6.5/8 → 8.1/10** | **4.5/8 → 5.6/10** | Ontology: 6 supported, 1 partial, 1 failed; plain: 4, 1, 3. |

**Why the difference:** Owner validation and task/input association account for the two
point difference. Plain passes precommit safety (A04), but fails successful delivery
(A08). Its three frozen test failures share that one pre-existing binding defect;
ontology passes 3/3. Both pass two supplemental behavior tests. Plain's worker stopped
after a disk-blocked test build; these runtime results came from later common evaluation.
The master supplied extra verification requirements and specific fixes only to ontology.
Ontology self-review caught the binding defect, and master review closed validation gaps;
plain generic review repaired retry defaults but missed the binding defect.

**Design/time:** The readable plan matched the architecture; no governing text changed.
Worker time excluding supervision pauses was 33m 42s versus 8m 45s, including builds and
verification. Planning took 69 seconds; first core compile took 4m 53s versus 3m 48s.

### Lessons and improvements

- **Harness:** Enforce existing consumer-tracing and verification rules with a short
  claim → executed check → remaining gap ledger. A build-only result must explicitly
  leave behavioral verification incomplete; prepare shared test infrastructure first.
- **Ontology:** Make initial input/task association and dispatch-consistent structural
  validation explicit at the allocation owner. Reconcile stale ownership metadata at
  one authoritative home; more generic principles would not address these misses.
- **Experiment:** Keep supervision minimal and free of domain hints, match feedback
  policy and environment, and freeze outputs before common evaluator checks. The initial
  ontology self-review still needed master repairs, so do not credit the harness alone.

Local comparison (local run record `runs/2026-09-26-allocation-ownership-baseline/comparison.md`) ·
Local timing audit (local run record `runs/2026-09-26-allocation-ownership-baseline/timing-audit.md`) ·
[Ontology PR #201](https://github.com/G5Labs/G5-Lean/pull/201)

## Experiment 5 — Drain a worker host before removing it

**Setup:** Fresh GPT-6-sol xhigh sessions received the same minimal gap/goal and identical
application code. Ontology used the maintained planning/authoring/review harness;
plain used a separate branch/session with ontology and skills hidden and their use
prohibited. The supervisor gave no targeted repair hints. Plain's initial commit was
preserved before sending exactly: “do a code review, making sure what you implemented
is correct and fix anything if needed”. Both final deliveries remain frozen.

| Measure | Ontology + harness | Plain + generic review |
|---|---:|---:|
| Production + test changed lines | 1,021 across 14 files | 578 across 10 files |
| Ontology changed lines | 110 across 10 files | 0 |
| Worker elapsed time, including checks/pauses (history-corrected) | 35m 37s | 12m 55s |
| Separate planning | About 2 minutes | None |
| Review/repair/final-check interval | About 21m 24s | 4m 56s |
| Shared runtime probes | 6/8 passed | 6/8 passed |
| Full host/cloud/stale regression suites | 75/75 passed | 71/72 passed |
| Frontend hooks; typecheck | 23 passed; passed | 22 passed; passed |

### Final implementation scorecard

Equal-weight H01–H08 obligations (local run record `runs/2026-09-27-host-drain-plain/evaluator-ledger.md`):
1 = supported within scoped checks, 0.5 = partial evidence, 0-F = demonstrated
violation or absent required control. Score = points / 8 × 10. These are strict
obligation scores, not severity rankings or ontology's isolated causal effect.

| Criterion | Ontology | Plain | Evidence / gap |
|---|---:|---:|---|
| H01 API admission and authority | 1 | 0-F | Plain returns 202 for an empty host already draining for another reason; ontology preserves immediate 204. |
| H02 Durable replay and restart | 0.5 | 0.5 | Stable identity/retry pass; process restart and concurrent DELETE unexecuted. |
| H03 Placement and incarnation fencing | 0.5 | 0.5 | Locking inspected; controlled placement/incarnation races unexecuted. |
| H04 Lifecycle Stop and capture/release | 0-F | 1 | Ontology loses required capture after deadline without force; plain's scripted lifecycle cases pass. |
| H05 Cloud completion and other lifecycle consumers | 0-F | 0-F | Ontology notification bypasses cloud teardown; plain review breaks stale-host replacement. |
| H06 Explicit force, audit and fencing | 1 | 0-F | Ontology retains custody on provider failure; plain falsely completes and releases it. |
| H07 Visible nondestructive deadline/blocker policy | 0-F | 0-F | Ontology deadline is destructive; plain deadline is safe but blockers/recovery guidance are absent. |
| H08 Operator UI | 0.5 | 0-F | Ontology implements pending/blocker/force surfaces; plain only shows Removing. Browser journey unexecuted. |
| **Total** | **3.5/8 → 4.4/10** | **2/8 → 2.5/10** | Overlapping obligations make these unsuitable as a standalone winner ranking. |

Ontology H04/H07 share one defect; plain H07/H08 share the operator-recovery omission.
The earlier ontology-only score was 5.0/10: the newly reproduced notification bypass
changes H05 from partial to failed. The implementation itself has not changed.

### What differed

- **Ontology remaining defects:** ordinary deletion reuses lossy `Drain` semantics;
  after failed capture and deadline expiry it sends Stop, releases custody and marks
  the host terminated with zero completed capture receipts. Separately, `host.drained`
  can mark the host terminated before cloud teardown, causing reconciliation to skip it.
- **Plain remaining defects:** force/provider failure falsely releases custody and
  completes deletion; an empty preexisting drain becomes asynchronous; blocker/force
  UI is missing. Review also introduces a stale-host recovery regression by restricting
  a shared replacement helper to ready/draining hosts, excluding a caller that already
  committed termination. The existing test passed before review and fails twice afterward.
- **Review helped both:** ontology repaired force proof/settlement, immutable audit,
  JSON 202 handling and empty-drain behavior. Plain independently found and fixed
  notification and rotation ownership hazards. Plain selected safe non-loss Stop
  semantics before review. Its shared probes improved from 4/8 to 6/8; ontology's
  original three lifecycle probes stayed at 2/3 before and after review.

### Answers and takeaways

**Harness:** procedural adherence was strong—design before code, approval, amendments,
self-review, repairs and scoped metadata. Enforcement was incomplete: the prescribed
consumer/completion review missed two consequential paths. Some raw logs were overwritten;
2,194 lines of experiment artifacts were committed alongside the ontology candidate.

**Readability/fidelity:** the original design was 607 words, 824 after amendments.
It clearly described intent and ownership and mostly matched the changed architecture.
Its failed-Stop and provider-proof promises did not hold in code. Readable intent was
useful for finding divergence, but was not implementation evidence.

**Value/cost:** ontology helped organize ownership, custody, replay, force audit and UI
coverage. Both kinds of post-generation review produced material repairs; generic review
also found a bug the harness missed, and introduced another. This pair does not establish
uniformly better code from ontology. Planning was short; the extra elapsed time mostly
sat in broader implementation, review, repairs and verification. Metadata-editing time and
dollar costs were not separately measured. A later history audit recovered cumulative
output-token counters: 90,511 versus 42,191; these do not isolate ontology overhead.

**Improve precision, not volume:** existing principles already require semantic
compatibility before sharing policy. Clarify which lifecycle operations authorize lossy
deadlines, distinguish local drain acknowledgement from cloud completion, and require
review to trace every terminal-state writer and every caller of changed shared helpers.
Pair critical promises with discriminating executed checks. More generic principles
would duplicate existing guidance. Neither candidate is ready to merge unchanged.

**Limits:** one sequential paired trial; ontology/planning/review effects are bundled.
Global skill descriptions could remain in plain context despite prohibited use; worktree
isolation was procedural. Shared CPU/database infrastructure and unequal initial disk
conditions limit timing comparisons. Five boundary probes were retrospective for both;
the deadline probe was retrospective for ontology. No evaluator feedback reached workers.
Tests used dedicated PostgreSQL, scripted peers and mock cloud; real cloud/VM, browser,
process-restart and controlled concurrency paths remain unexecuted. Earlier evaluator
setup/cache mistakes were corrected with retained receipts and are excluded from scoring.

Detailed comparison (local run record `runs/2026-09-27-host-drain-plain/comparison.md`) ·
Evidence and scoring (local run record `runs/2026-09-27-host-drain-plain/comparison-evidence.md`) ·
Setup and commit identities (local run record `runs/2026-09-27-host-drain-plain/manifest.json`)

### Follow-up: audit of both original histories

The [23-issue history audit](ontology-experiment-5-history-audit.md) finds that the ontology
worker actually retrieved the lossy Stop policy, then changed a failing admission-test
expectation without exercising deadline completion. Plain inspected the same policy and
selected the non-loss variant. Plain review also inspected the missed terminal-state
writer, but searched a changed replacement helper's callers too narrowly and introduced
the stale-host regression. These are tractable failures to connect obligations with
specific paths and discriminating checks; adding more generic principles is unlikely
to suffice. The audit includes repaired defects, tooling friction and evaluator errors.

Original tool timestamps correct the earlier 35m 32s / 11m 45s timing to **35m 37s /
12m 55s**. The frozen obligation scores remain historical results, but their overlapping
rows double-count some underlying defects and are not calibrated code-quality ratings.
Report the unique critical defects and unexecuted promises alongside them. Both frozen
implementations and all behavioral verdicts remain unchanged.

## Experiment 7 — Fork & Start

**Setup:** Two fresh GPT-6-sol xhigh workers ran in parallel under GPT-6-astra xhigh,
with identical application bytes and the same minimal gap/goal. Ontology used the
maintained planning/authoring/review harness; plain had ontology and skills removed
and their use prohibited. The master gave one design approval and, after freezing
plain's initial output, exactly: “Do a code review, making sure what you implemented
is correct and fix anything if needed.” No evaluator findings reached either worker.
Initial designs/implementations and final deliveries remain frozen in separate worktrees.

| Measure | Ontology + harness | Plain + generic review |
|---|---:|---:|
| Code + test changed lines | 2,500 across 20 files | 2,758 across 19 files |
| Ontology changed lines | 152 across 10 files | 0 |
| Worker elapsed, including checks/pauses | 42m 45s | 48m 26s |
| Separate design work | 2m 27s, plus approval wait | None |
| Review through final delivery | 14m 18s | 15m 55s |
| Common obligation probes, initial → final | 10/13 → 11/13 | 8/13 → 9/13 |
| Existing backend regressions | 41/41 | 41/41 |
| Selected frontend tests; TypeScript | 31 + 2 child-status tests; pass | 33; pass |
| Final strict obligation/evidence score | **2.5/8 → 3.1/10** | **1.5/8 → 1.9/10** |

**Scoring:** The eight E701–E708 obligations were frozen before dispatch. Each earns
1 for the required supported outcome, 0.5 for partial evidence, or 0 for a demonstrated
violation/absent control or unknown outcome; score = points / 8 × 10. Initial totals
are the same: review repairs improved subcases, but remaining failures still bound
the broad rows. These are strict outcome/evidence scores, not calibrated code-quality
ratings. Stop is counted in both consent and cancellation rows; see the unique causes
and full scorecard (local run record `runs/2026-09-27-experiment7-fork-start/comparison-evidence.md`).

### What differed

- **Both still fail:** public Stop leaves queued Start consent eligible; cancelling
  a pending follow-on puts an unrelated chat on `owner_stop` hold; progress becomes
  `completed` when input is queued and does not project delivery uncertainty or actual
  agent execution. Stop exposes an inherited lifecycle gap; the old tests miss it.
- **Plain additionally fails:** a Fork-only receipt reused with the same key is adopted
  and its existing child started; an eligible source reader receives 403 instead of a
  new owned child. Ontology separates replay identities and preserves source-read versus
  child-owner authority.
- **Both reviews helped:** ontology repaired late chat admission after consent expiry,
  stale dialog polling, superseded queued consent and error redaction. Plain repaired
  VM consent extending beyond the original grant and tightened recovery. Common probes
  reproduce and confirm each consent repair. Plain's switch to explicit retry after a
  known pre-delivery failure is retained as an extra characterization, not a frozen
  requirement; raw logs contain 14 cases, one excluded from the 13-probe comparison.

### Harness, design and takeaways

**Adherence was strong, enforcement incomplete.** Ontology planned without application
source, waited for approval, preserved pre-review findings, amended its design and
updated scoped metadata. Its initial 625-word design, later 794 words, clearly identifies
owners, replay and authority. Code follows that structure, but the promised cancellation
and factual progress behavior does not fully hold. The review missed unchanged lifecycle
behavior and effects on other consumers despite existing harness instructions to trace them.

**Value and cost:** retained contracts plausibly helped ontology preserve reader access
and replay compatibility. Both review styles found consequential defects independently.
Ontology finished about six minutes earlier with 124,164 cumulative output tokens versus
146,302, but used more input excluding cache hits (498,961 versus 451,332). Separate
metadata-editing time and dollar costs are unknown; shared hardware, compilation and
different debugging paths prevent an isolated overhead conclusion.

**Next:** test cancellation against unrelated work, use the actual public Stop path,
project progress from VM/input/task records, preserve source-read versus child-execution
authority, and exercise old receipt compatibility. These are specific applications of
existing commitments; more generic principles would add little. Provision real runtime
coverage before another full-outcome trial. Neither candidate is ready to merge unchanged.

**Limits:** one paired trial of bundled context/workflows, with a nonblind supervisor;
global skill descriptions remained visible to plain and filesystem isolation was
procedural. No forbidden context reads were observed. Tests used dedicated PostgreSQL,
Git and scripted readiness. Real VM/provider output, browser journeys, abrupt API restart
and the full failure/race matrix remain unexecuted. Evaluator DTO/clock-fixture errors
and stale Cargo reuse were corrected with retained receipts and explicit workspace-crate
rebuilds; invalid runs are excluded. No evaluator repairs, push or merge were performed.

Detailed comparison (local run record `runs/2026-09-27-experiment7-fork-start/comparison.md`) ·
Evidence and scores (local run record `runs/2026-09-27-experiment7-fork-start/comparison-evidence.md`) ·
Timing and context audit (local run record `runs/2026-09-27-experiment7-fork-start/timing-audit.md`) ·
Manifest and frozen commits (local run record `runs/2026-09-27-experiment7-fork-start/manifest.json`)

## Experiment 4 — Flush ownership across incarnation release (2026-09-27)

**Completed with the revised experiment 5/7 harness.** Fresh Sol/xhigh workers ran
in parallel on identical application code and minimal briefs, with separate services
and build outputs. Ontology received current contracts/skills; plain used native coding
and the prescribed generic review. The supervisor sent only one design approval and
one generic review request. Baseline duplicate-start conflict handling already worked,
so that obsolete claim was removed equally before dispatch.

| Measure | Ontology + revised harness | Plain + generic review |
|---|---:|---:|
| Worker elapsed | 14m 47s | 9m 00s |
| Initial common checks | 57/57 | 57/57 |
| Final common checks | 58/58 | 58/58 |
| Final obligation/evidence coverage | **7/8 → 8.8/10** | **7/8 → 8.8/10** |
| Output tokens | 45,347 | 25,006 |
| Production additions, including migration checksums | 64 | 73 |
| Total PR additions, including tests/ontology | 330 | 290 |

Both implemented atomic database closure of an unfinished receipt at VM release.
Ontology repairs historical abandoned rows on validated successor start; plain backfills
at migration. Both self-reviews independently repaired replay lookup/index scoping to
include epoch. No remaining defect was demonstrated in the exercised scope. Normal
placement uses fresh VM UUIDs; the same-UUID/later-epoch repair is a latent identity
case, not a reproduced ordinary-placement incident.

The 58 final checks comprise 42 frozen regressions, 14 common lifecycle probes,
one historical-database recovery check and one replay in a new service process.
Initial snapshots passed the first 57; the final-only process check adds evidence,
not a review-caused behavior improvement. Coverage uses the frozen eight-row matrix,
with half credit for unexecuted admitted-Git-write races and some release-owner variants.
These are evidence scores, not general quality grades. Real guest execution and full
API crash recovery remain unverified.

**Harness observation:** source-free planning, reuse inspection, preserved findings,
real release-owner testing and scoped qualitative metadata all occurred. The 662-word
initial design became 715 words after reconciliation. Ontology used about 1.8× output
tokens and took 5m 47s longer, with no final correctness advantage shown. Broad searches
and truncated reads occurred in both conditions; ontology also initially guessed the
wrong root path. Existing selective-reading guidance was not consistently applied.

**Takeaway:** this baseline was much easier than the original 1,000–1,600-line estimate;
most callback/Git safeguards already existed. Keep it as a regression check, avoid
adding more harness rules from this pair, and use a harder held-out task to judge the
revised guidance. Experiment 6 needs a usable Linux privilege/process fixture first.
No additional experiment or harness changes were made; candidate code remains isolated.

Detailed comparison (local run record `runs/2026-09-27-experiment4-flush/comparison.md`) ·
Evidence and scorecard (local run record `runs/2026-09-27-experiment4-flush/comparison-evidence.md`) ·
Timing/context audit (local run record `runs/2026-09-27-experiment4-flush/timing-audit.md`) ·
Manifest and frozen commits (local run record `runs/2026-09-27-experiment4-flush/manifest.json`)

## Experiment 7 second run — Fork & Start

**Completed with the revised ontology/harness.** Two fresh Sol/xhigh workers used the
same application bytes, minimal brief and eight obligations as the first Experiment 7.
Ontology followed planning/authoring/review; plain used native coding and the exact
generic review request. Initial and final deliveries were frozen, with separate worker
services/build outputs and no evaluator feedback. This is a repeat after learning from
Experiment 7, not a held-out or causal test of the harness revision.

| Measure | Ontology + revised harness | Plain + generic review |
|---|---:|---:|
| Code + test changed lines | 2,163 across 19 files | 2,441 across 13 files |
| Additional ontology changed lines | 84 across 7 files | 0 |
| Worker elapsed | 42m 46s | 36m 37s |
| Review through final delivery | 19m 29s | 14m 50s |
| Output tokens | 120,523 | 105,788 |
| Input excluding cache hits | 463,501 | 339,496 |
| Original common scenarios, initial → final | 11/13 → 11/13 | 9/13 → 9/13 |
| Additional consent discriminator, initial → final | Fail → Fail | Pass → Pass |
| Existing backend regressions | 41/41 | 41/41 |
| Selected final frontend tests; TypeScript | 32; pass | 31; pass |
| Strict obligation/evidence score, initial → final | **2.5/10 → 2.5/10** | **2.5/10 → 2.5/10** |

**Scoring clarification:** the unchanged eight-row rubric gives each candidate 2/8
points with all available evidence. These are strict obligation/evidence scores, not
general code-quality ratings. Ontology would receive 3.1/10 using only the original
probes and progress inspection. Final source review exposed a weakness in the expiry
probe: it could reject a VM that started too late without checking late chat allocation.
A symmetric additional case establishes compute started **within** the original grant,
then attempts the first chat **after** expiry. Ontology still allocates it; plain refuses.
This removes ontology's provisional consent credit under the existing requirement.
The original 13-case denominator and historical first-run scores remain unchanged; the
new discriminator was not executed against first-run candidates. The separate failed-replay
characterization remains unscored, so original common logs contain 14 cases.

### What changed from the first run

| Property, final deliveries | First run | Second run |
|---|---|---|
| Public Stop withdraws queued Start | Both fail | Both pass |
| Valid source reader gets a new owned child | Ontology passes; plain fails | Both pass |
| Old Fork-only receipt cannot gain Start through key reuse | Ontology passes; plain fails | Both fail |
| VM grant stays within original submission deadline | Both pass | Ontology passes; plain fails |
| Cancel preserves unrelated chat | Both fail | Both fail |
| Progress reflects input delivery and actual execution | Both fail | Both fail |

The unchanged 11/13 versus 9/13 totals conceal traded failures. Plain also again permits
late chat allocation under its extended VM grant. Ontology preserves the VM deadline,
but the additional discriminator exposes a different late-chat consent defect.

**Both reviews made useful repairs.** Ontology improved reload/authentication recovery,
failed-Start withdrawal, retry generation handling, cancelled-fork settlement and Stop
intent fencing. Plain repaired retry-selection races, stale allocation data, lost child
links, newer-Start cancellation and recovery lookups. Their reviews did not fix the
remaining replay, shared-work cancellation, consent and progress defects. An additional
source finding shows ontology's claimed batch-rotation repair updates `updated_at` and
then rolls back that transaction; a 17-operation load outcome was not tested, and this
fairness concern is reported outside the frozen scoring cases.

### What this verifies about the harness

**Correct knowledge was retrieved, but its application remains unreliable.** Public
history confirms ontology read the new operation-identity and outcome-evidence rules,
and its design cites owner-derived progress. It still uses shared Start identity to
authorize workspace-wide cancellation and reports terminal progress at input allocation.
The review accepted durable status plus polling as evidence for the outcome requirement.
Adding more general ontology text is unlikely to solve these particular misses.

There are positive observations: actual Start/Stop semantics were inspected, the original
deadline was carried into Start, source-versus-child launch configuration prompted a
saved design amendment, and metadata was scoped and qualitative. Both workers fixed
public Stop and preserved valid reader access, so those gains cannot be attributed
exclusively to ontology. Ontology's plan grew from 693 to 902 words. It used about 14%
more output and took 6m 09s longer than plain in this pair; this does not isolate marginal
harness overhead.

**Next:** keep the compact contracts and invest in reusable boundary fixtures and selected
counterexamples: old-action replay compatibility, unrelated work on shared compute,
current versus historical consent, owner-derived progress, and persisted transaction
effects. Use the existing design/review artifacts to attach evidence. This run provides
no reason to add a universal checklist or another workflow phase. Neither candidate is
ready to merge; no main application or harness changes were made.

**Limits:** one paired repeat, bundled context/workflow treatment, nonblind supervisor,
shared hardware, inherited global skill/memory infrastructure and procedural filesystem
isolation. No forbidden context reads were observed. Real guest/provider execution,
full browser journeys, abrupt API restart and the full race/uncertain-delivery matrix
remain unexecuted. The readiness fixture correction and additional expiry discriminator
are documented; neither modified worker code. Candidate commits and raw evidence remain
isolated and retained.

Detailed comparison (local run record `runs/2026-09-28-experiment7-second-run/comparison.md`) ·
Evidence and scorecard (local run record `runs/2026-09-28-experiment7-second-run/comparison-evidence.md`) ·
28-observation history audit (local run record `runs/2026-09-28-experiment7-second-run/history-audit.md`) ·
Timing/context audit (local run record `runs/2026-09-28-experiment7-second-run/timing-audit.md`) ·
Manifest and frozen commits (local run record `runs/2026-09-28-experiment7-second-run/manifest.json`)
