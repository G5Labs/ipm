# Can independent review reliably discover and repair ontology violations?

Updated 2026-09-29. [Original balance study](../README.md).

The goal is implementation that realizes the agreed design and governing ontology,
including legitimate existing behavior. An empty findings list or passing local tests
is not sufficient. These experiments separate discovery, repair and verification.

Combining independent reviews improved discovery. Splitting reviewers by specialty did
not consistently improve it. Repair loops produced useful corrections, but every final
implementation still had independently verified failures, including a false-clean stop.

The [review-quality reassessment](../../ontology_harness_tuning/ontology_review_experiment_finding.md) evaluates
these results separately on PR-scoped coverage, accurate remaining-gap reporting and
progress without regressions. Incomplete feature delivery alone does not make a review
unsuccessful; honest partial repair can be valuable.

The latest **six ontology-only inventory runs** tested whether review obligations can be
inferred from supplied ontology changes/governance: two Sol repetitions each on retry,
host draining and stale Save, independently evaluated by Astra. Both repetitions recovered
the main feature concerns; some governing conditions were missed by both despite retrieval.
Consolidation would recover one-sided details, not establish completeness. Worker effort
was 16m 50s total, plus separately recorded evaluation/preparation.
[Results, concrete omissions and limits](review-scope-20260929/README.md).

The separate **27 planning-only runs are complete and independently evaluated**.
Neither retrieval checkpoint A nor direct links B met the frozen screening rules.
**A is now incrementally adopted with user approval:** it covered 91/99 selected
obligation instances versus current guidance's 87/99, with 4% less total planner time and
11% fewer output tokens. B covered 92/99, with 9% more time and 7% more output than current.
These small-sample results support trying a modest improvement while studying further
techniques; they do not establish a dependable improvement across new tasks.
**The main remaining problem was applying retrieved rules:** 25 of 27 incomplete or
uncertain obligation instances had the relevant source text visibly retrieved.
[Results, effort and limits](planning-followup-20260929/RESULTS.md).

The earlier twelve-run pilot and two explicit-gap repairs remain below. They were not
rerun. Those repairs closed material defects but still left accounting and other gaps;
the planning and inventory studies do not test implementation or repair reliability.

This PR tracks the comparison protocol and summarized findings. Per-run inputs,
worker records, evaluations, and setup logs are omitted. Paths in code font below
identify the original study records and are not links to files in this checkout.

## What we tested

The feature retries a failed workflow branch as a fresh linked run, preserving successful
prerequisites and history. GPT-6 Sol/xhigh did all review and repair work; GPT-6 Astra/xhigh
supervised, consolidated findings and independently evaluated preserved candidates.
All workers used isolated copies. No experimental repair was integrated into production.

| Experiment | Question | Procedure |
|---|---|---|
| Previous study; not rerun | Does the review skill help? | Three normal, three old-skill and three revised-skill review-and-fix runs. |
| A: discovery only | Does dividing review responsibilities improve coverage? | Three repetitions of three general reviewers versus three parallel focused reviewers; main Astra consolidation in both. |
| B: repair | Can agents reliably fix the same discovered issues? | Three fresh fixers receive the same untouched implementation and the same new findings; preserve the first repair, then bounded fresh-review/correction rounds. |
| Latest: planning only | Do a retrieval checkpoint, direct rule/exception links, or both improve planning? | Three fresh planners per condition, including current guidance; no implementation. |
| Follow-up: planning on three scenarios | Do A or B improve coverage consistently at acceptable cost? | 27 fresh planners: three repetitions of current/A/B on host removal, safe Git capture and preview visibility. |
| Latest: explicit gap closure | Does the full diagnosis make one repair sufficient? | Two fresh fixers start from the same B2/P3 delivery and receive all known gaps; one attempt each, no feedback loop. |
| Ontology-only review inventory | Can agents infer review obligations from ontology changes/governance? | Two fresh Sol repetitions on each of three features; source-grounded Astra evaluation, no code or repairs. |

**Normal review also had the ontology.** The previous comparison tested the added procedure,
not ontology versus code alone. A and B used the same revised skill and concise
rubric (`consistency-20260928/REVIEW-RUBRIC.md`), so they do not isolate either one's benefit.
Normal review explicitly required consulting relevant ontology but did not require skill or
metadata edits. Baseline code, approved plan, sanitized governing ontology and harness
versions were frozen.
Prior findings, including metadata that disclosed old failed tests, and evaluator answers
were excluded from discovery inputs; governing rule text was retained.
[Protocol and versions](consistency-20260928/PROTOCOL.md).

## What the previous comparison established

The old skill repaired workflow concurrency limits in three of three runs; normal and
revised-skill review repaired none. The revised skill preserved accepted launches in three
of three; normal review repaired one and the old skill none. All nine repaired direct saved
request recovery, but three missed the parent page that hid that action after compute stopped.
None fixed the future-shutdown exception or fully correct synchronization progress.
Two skill runs introduced an unsupported restriction on reusing successful command work.

Thus the results differed by requirement; no procedure completed every repair. Skill groups
spent more time and tokens, without establishing an overall or efficiency advantage.
The [previous report](consistency-20260928/previous-report.md) and
[issue-by-issue traces](issue-trace.md) preserve all nine outcomes. They were not rerun.

## A: three general reviews versus three focused reviews

Each reviewer had ten minutes, including checks and reporting. Focuses were intent/invariants,
behavior/compatibility, and architecture/code quality. All retained the full review scope.
The main agent consolidated the reports with the same five-minute allowance per team,
retaining minority findings and every original finding ID.

Each number below is **how many of the three teams reported that gap**. A team succeeds on
this measure if at least one of its three reviewers reports the gap. This measures discovery
of a specific issue; it does not measure repair, severity or general reliability.

| Known alignment gap | General teams, out of 3 | Focused teams, out of 3 |
|---|---:|---:|
| Accepted launch changes with defaults | 2 | 3 |
| Future shutdown warning blocks work early | 2 | 1 |
| Workflow concurrency limit ignored | 3 | 2 |
| TaskService does not own complete allocation | 3 | 3 |
| Saved replay requires a fresh preview | 3 | 3 |
| Stopped compute hides the recovery action | 3 | 1 |
| Synchronization progress loses selected scope | 2 | 3 |

The three general teams found **7, 5 and 6** of these seven gaps; focused teams found
**5, 5 and 6**. Combining reports improved coverage over the best individual in every team.
Focus recovered some missed issues but repeatedly lost other important coverage, so it did
not meet the adoption rule. Keep independent general reviews as the simpler control.

Beyond these controls, reviewers reported 105 distinct claim IDs, grouped into 19 causal
issues with no IDs lost. Seventeen groups had source support. Tenant-reference hardening remained unresolved;
independent adjudication rejected a demanded historical-success graph layout as an
unsupported requirement. These are not 19 equally proven defects. New supported claims included jobless prerequisite failure,
rejection of a distinct later task in the same session, missing spend admission, and
catalog policies accepted before dispatch rejected them. Main consolidation was unblinded
to the old cases; it preserved raw evidence and did not inject old findings into the packet.
[Findings, tradeoffs and evidence limits](consistency-20260928/DISCOVERY.md).

### Why did discovery vary?

The evidence distinguishes observable stages, not private reasoning:

- **Retrieval gap:** only three reviewers visibly retrieved the exact future-shutdown
  exception; all three diagnosed it. Opening the right ontology file was insufficient:
  one focused reviewer read its ownership/deletion section but missed the deadline section.
- **Diagnosis gap after retrieval:** all 18 retrieved the accepted-launch plan, but only
  ten reported the violation. Returning the rule to context did not ensure tracing it
  through dispatch.
- **Complementary coverage:** a single reviewer sometimes supplied a team's only valid
  nudge or progress finding. Majority voting would have discarded useful evidence.

These observations support testing explicit exception and consumer coverage. They do not
prove that reading an extra paragraph alone cures the failure.
Retrieval evidence (`consistency-20260928/retrieval-observations.json`),
exact read routes (`consistency-20260928/retrieval-routes.json`).

The plan did not explicitly state or pin the future-shutdown exception; reviewers had to
find it in the supplied ontology. In contrast, all eight launch-miss reviewers saw the
freeze requirement, preferences-only write and dispatch re-resolution call; five also saw
the resolver body. None documented the changed-default check. They finished 93–182 seconds
before the deadline. [Each missed review, evidence and proposed improvements](consistency-20260928/FOLLOWUP-ANALYSIS.md).

## B: repairs and bounded loops

Three fresh fixers received the same 105-claim packet and untouched implementation. Each
saved its findings before editing, then had 30 minutes for the first repair (P1). Fresh
reviews then ran for six minutes without the ledger and four minutes checking it; the same
fixer could make at most two further 15-minute corrections. Hidden evaluator results never
entered this loop. All stages and stopping decisions are preserved.

P1/P2/P3 are successive repairs by the **same fixer within each B trajectory**, with a
**fresh reviewer between repairs and after P3**. Each repair review used one reviewer,
not the three-reviewer discovery ensemble. The final correction cap was an experimental
stopping rule, not evidence that the implementation was complete.

The first repair still violated important requirements:

| Required behavior after P1 | B1 | B2 | B3 |
|---|---|---|---|
| Accepted launch survives changed defaults | Pass | Rejects valid launch | Pass |
| Future shutdown warning permits retry | Pass | Pass | Pass |
| Explicit Stop prevents fresh allocation | Pass | New regression | New regression |
| Expired shutdown warning prevents allocation | Pass | Pass | New regression |
| Complete retry allocation belongs to TaskService | Supported in source | Session created outside owner | Supported in source |
| Real sync producer retains selected retry scope | Fails | Fails | Fails |
| Recorded failed-task usage enforces exhausted budget | Fails | Pass | Fails |

All three passed the tested concurrency, saved recovery, current receipt-read authority,
jobless prerequisite reuse, distinct later session task and catalog admission cases. Those
successes did not cancel the violations above. B2 also omitted its migration checksum entry.
Four previously unmeasured baseline cases were executed once; old established baseline
outcomes were reused. Real provider/VM/browser operation remains unverified.

The causes are concrete: B2 tested refusal on changed defaults as success; B2/B3 fixed the
future-warning case while weakening neighboring Stop checks; B1/B3 called a spending helper
that excludes failed-task usage. All three inferred progress from metadata the real producer
does not populate. The initial evaluator's synthetic progress fixture missed that same gap;
a separately identified public-producer check exposed it on all three candidates.
[Per-case evidence, corrections and limits](consistency-20260928/P1-ASSESSMENT.md).

The fresh reviews found further issues, and both permitted correction rounds ran. Findings
saved before the ledger check and findings added afterward remain separate. The ledger
caught B2's incomplete allocation in round one, then its wrong launch criterion in round two.

| Trajectory | First correction (P2) | Final correction (P3) |
|---|---|---|
| B1 | Separate equal target paths across repositories | Stop treating a new task as a completed seed |
| B2 | Compose the complete retry allocation inside TaskService | Preserve accepted launch despite new defaults; omit ambiguous progress targets |
| B3 | Validate tool restrictions and expose replacement-account selection | Remove unrelated repository targets/document seeds from progress |

These are distinct gains at their stated source/local-test boundaries. The independent
changed-default test now passes for B2. The final fresh reviews recorded whether each trajectory
would stop; independent evaluation checked whether each stopping decision was correct.

At P3, **all three still fail real-producer sync progress**; B1/B3 still miss recorded
failed-task spending, B2/B3 admit work after an explicit Stop, and B3 admits work after an
expired warning. B2's migration checksum catalog also fails. These are observed failures,
not merely unverified journeys. [Final repair evidence](consistency-20260928/P3-ASSESSMENT.md).

| Final reviewer | Would stop? | Independent assessment |
|---|---|---|
| B1 | No | Correction cap reached; Claude tool-restriction finding remains. Mixed-prerequisite omission is source-confirmed. |
| B2 | Yes | False clean: explicit Stop, real-producer progress and migration catalog still fail. |
| B3 | No | Correction cap reached; a component probe confirms saved recovery can lack a reset action. |

B2's final reviewer found nothing, assessed all 19 supplied groups and passed 985 core
tests, then said it could stop. That was a **false clean**: it accepted narrower progress
fixtures, did not exercise the explicit-Stop boundary, and missed the migration catalog.
The final source adjudication also confirms a shared mixed-prerequisite omission: a descendant
can disappear when another failed prerequisite settled it first. This was not runtime-tested.
Three review rounds therefore do not establish reliability, and looping until no findings
can terminate incorrectly. Blind findings, ledger additions and stopping decisions (`consistency-20260928/loop-summary.json`).

## Effort and next decision

These are **summed worker times**, including tool waits and reports, not elapsed project
time or a population efficiency estimate. Output tokens include reasoning; they are not
added a second time. Continuation usage is counted once.

| Work | Worker time | Output tokens |
|---|---:|---:|
| General discovery: 9 reviews | 69m 32s | 233,128 |
| Focused discovery: 9 reviews | 71m 22s | 238,878 |
| First repairs: 3 passes | 70m 30s | 204,112 |
| Corrections: 6 passes | 50m 12s | 153,582 |
| Fresh review rounds: 9 reviewers | 64m 50s | 230,678 |
| Independent Astra assessment: 11 workers | 87m 57s | 256,841 |

Main-agent discovery consolidation took 2m 32s for the general teams and 2m 52s for
the focused teams, included in separate supervision. Root Astra preparation, supervision,
runtime evaluation and reporting used 310,819 output tokens through 23:42:15 UTC;
its elapsed window overlaps worker time. Setup/queues are excluded from worker intervals.
Full time/input/cache accounting (`consistency-20260928/effort-summary.json`).

Per three-reviewer team, general discovery averaged **23m 11s of worker time and 77,709
output tokens**, plus 51s of consolidation; focused averaged **23m 47s and 79,626 tokens**,
plus 57s. General prompts were identical apart from private paths. Three reviews cost
roughly three individual passes plus consolidation; parallel elapsed time is not their
sum. We did not compare against one longer reviewer at the same total effort, or establish
that three is better than two. These token figures are not total billing costs.

The user has now adopted **three parallel independent full-scope reviewers → main-agent
consolidation → one coherent repair** for substantial changes in the maintained review
skill. Small localized changes can use one review. Retain minority findings; neither
specialization nor a fixed number of loops is a correctness guarantee.
The shared qualitative rubric usefully states the target—intent, compatibility, ownership,
code quality and verification—but this experiment did not isolate its causal benefit.

The [planning-retrieval proposals](RETRIEVAL-PROPOSALS.md) have now been tested as described
below. The broader repair-reliability/consumer-tracing comparison and further repair loops
remain **proposals only**, as requested. Equal-budget single/two/three-reviewer comparison
also remains necessary before claiming optimal review cost.

These are small comparisons on one feature. Discovery exclusions removed old failed-test
metadata, not governing rules. Evaluation had disclosed fixture corrections and uniformly
applied new probes. Source support, local execution, conditional legacy concerns and unavailable
provider/VM/browser evidence remain distinct. The reports below summarize the results
and evidence limits; detailed run records and audit output are omitted from this PR.

## Pilot: planning retrieval, without implementation

Twelve fresh Sol/xhigh planners designed a named workspace handoff that a teammate could
use after the original workspace changed or stopped. Each had the same request, ontology
meaning and fifteen-minute allowance, with no source code, prior findings or evaluator
answers. Each saved an initial plan and ontology diff, then performed ordinary final
design review. Astra independently checked twelve frozen governing obligations per plan.

| Condition | Rules visibly retrieved, each run /12 | Correctly incorporated, each final plan /12 | Mean active time | Mean output tokens |
|---|---|---|---:|---:|
| Current guidance | 12, 11, 11 | 10, 8, 10 | 9m 22s | 39,656 |
| A: one extra retrieval checkpoint | 12, 12, 12 | 10, 9, 10 | 8m 37s | 36,772 |
| B: direct rule/exception links | 11, 12, 12 | 11, 11, 9 | 8m 41s | 33,329 |
| A+B | 11, 12, 11 | 9, 9, 9 | 7m 55s | 33,649 |

Each number counts specified obligations in one design, not tests or independent tasks.
Retrieval meant the actual rule body appeared in tool output. Incorporation meant the
design retained its relevant behavior. A retrieved all 36 assessed instances but supported
29; retrieval alone therefore did not ensure a correct plan. B supported 31 versus the
control's 28 at lower observed time/output cost, but its second plan's final review newly
forbade authorized viewers from listing handoffs. The same error also appeared in control;
we cannot attribute it to the links.

Five plans deliberately changed source-read policy to separate handoff grants. These
count against the frozen preserve-existing-constraints metric but are explicit proposed
policy amendments, not accidental retrieval failures. The metric is narrower than overall
design quality. Two of B's three extra supported records reflect fewer such amendments,
so that difference must not be attributed entirely to recovered retrieval. All original
judgments and initial/final artifacts are preserved.

**Decision: stop after the first twelve runs; adopt neither variant yet.** B met the
numerical gain/cost threshold but retained a serious false restriction. The preregistered
word “new” did not specify whether it meant new to the design or absent from control.
Using the latter meaning would advance B; we disclose that ambiguity and use the stricter
guard to avoid an automatic second-stage spend. This is an inconclusive pilot, not evidence
that links cannot help. [Screening audit](planning-closure-20260929/SCREEN-AUDIT.md).

The twelve planners used **104m 41s summed worker time, 1,222,926 uncached input tokens
and 430,215 output tokens**. No implementation, repair or second-request planning ran.
[Method, per-obligation results, preparation and evaluation costs](planning-closure-20260929/RESULTS.md).

**Why A+B looked worse/faster:** its four-obligation deficit against B is entirely two
collision-policy and two native-privacy misses. Its reviews did useful work on other
concerns; this does not prove overall design inferiority or a harmful interaction.
A+B used slightly **more** output tokens than B. One six-minute run drives its lower
average time; the other two average 8m 51s. [Exact run-by-run explanation](planning-closure-20260929/AB-ANALYSIS.md).
The following study tested **current/A/B on three further scenarios**, with clearer
policy/severity rules and matched timing. [Frozen protocol](PLANNING-FOLLOWUP-PLAN.md).

## Follow-up: 27 planning runs on three scenarios

Each condition used three fresh Sol/xhigh planners per scenario, with the same request,
ontology meaning and fifteen-minute allowance. **Current** already used the full planning
skill and ordinary design review. **A** added one retrieval-challenge checkpoint.
**B** added canonical links, prepared without seeing the requests. No source code, old
findings, issue checklists or evaluator answers entered planner inputs. No A+B or repairs ran.
All 27 finished within the cap. Three fresh Astra/xhigh evaluators assessed masked cases
against requirements frozen before dispatch; the supervisor checked evidence and arithmetic.

Each number below is **one final plan's supported obligations**, in repetition order.
For example, 10/11 means ten selected behaviors were concretely specified or their
preservation justified. It does not mean ten tests passed or a 91% chance of correct code.
An omitted detail is distinct from a specified contradiction or demonstrated defect.

| Scenario; obligations per plan | Current | A: checkpoint | B: links |
|---|---|---|---|
| Safe busy-host removal; 11 | 11, 10, 11 | 10, 10, 11 | 11, 11, 11 |
| Safe privileged Git capture; 12 | 10, 8, 9 | 10, 10, 10 | 9, 10, 11 |
| Preview status/history; 10 | 9, 9, 10 | 10, 10, 10 | 10, 9, 10 |

**Frozen screening result:** A gains three supported instances in capture and two in preview,
but loses one in host removal: its proof of host destruction does not establish whether
separately retained storage was deleted. That is an unresolved proof scope, not proven data
loss. B gains one, three and one respectively; the frozen rule required at least two in
two scenarios. B also exceeds the capture cost ceiling. No concrete serious violation or
initial-to-final regression was demonstrated in any condition. This is design evidence,
not a security or implementation guarantee.

**Practical decision after discussing these results:** A was incrementally adopted on
2026-09-29 with user approval. The screening rule was deliberately conservative; failing its margin does not show
that a variant is unhelpful. A's one-count host deficit is an ambiguity rather than a
demonstrated harmful design, and its aggregate coverage and effort both improve on current
guidance. Across nine plans, A uses 3.7% less time and 10.7% fewer output tokens; B uses
9.3% more time and 7.1% more output, before navigation preparation and maintenance.
B is also a defensible choice when its slightly higher observed coverage is worth that
cost. A is the preferred first step because it is a small, reversible instruction change
and nearly matches B's coverage. This is a post-study judgment for provisional use, not
a claim that either variant passed the unchanged rules. The exact tested checkpoint is
now in the maintained [planning skill](../../../../.agents/skills/ontology-planning/SKILL.md),
before ordinary design review, with its generated skill copies synchronized. It makes one
deliberate retrieval pass and records references or uncertainty in the existing plan.
The other current planning edits were preserved; B and the proposed application checkpoint
were not added. Frozen experiment inputs and results remain unchanged.

| Mean planner minutes | Current | A | B |
|---|---:|---:|---:|
| Host removal | 9.0 | 7.4 | 10.1 |
| Git capture | 5.9 | 6.4 | 7.7 |
| Preview visibility | 7.0 | 7.2 | 6.1 |
| Output tokens across all nine plans | 245,880 | 219,484 | 263,357 |

A meets all time/output/uncached-input ceilings. B's capture means are **31% more time and
39% more output** than current guidance; the third pair drives much of that difference.
All planners together used **3h 21m summed time, 728,721 output tokens and 2.37 million
uncached input tokens**. B navigation preparation plus audit separately took 10m 43s and
27,914 Astra output tokens. Oracle construction, evaluation and root supervision are
additional study overhead. [Every run, ranges and complete overhead](planning-followup-20260929/RESULTS.md).

**What remained, and why:**

- Two host plans omit explicit retry after repairing a deterministic capture block;
  both read that rule. Another leaves the cleanup-proof ambiguity above.
- Capture plans commonly omit public diagnostic redaction or part of failure recovery.
  All nine leave some drain/callback detail unapplied. The unchanged callback and child-fork
  obligations are scope-sensitive documentation judgments; no deadlock or broken fork was
  demonstrated. A more permissive scoped-reuse interpretation still selects neither variant.
- Two preview plans read redaction rules but do not resolve secret-safe shared metadata.
  One misses the explicit exclusion of passive polling from idle activity and only partially
  retrieves that source set.

Ordinary final review adds **4 supported instances for current, 9 for A and 8 for B**.
Visible retrieval was already 98/99, 99/99 and 94/99 complete source sets respectively;
the remaining sets were partial. Partial retrieval can still support a correct design
through overlapping rules. These observations favor testing **application of constraints**
over simply adding more retrieval. [Judgments, source evidence and scope sensitivity](planning-followup-20260929/EVALUATION.md).

**Next proposal, not run:** use A as the provisional baseline for
one bounded comparison against A plus a constraint-application check. For each changed
boundary, use the existing commitment table to point to the proposed operative clause,
identify retained owner/effects, trace an adverse condition through the proposed flow,
and resolve any gap or record its precise uncertainty. Use two unseen tasks,
three repetitions per condition (12 planning-only runs), with frozen scope and the same
cost/severity controls. This tests whether explicit application closes the observed kind
of gap without requiring every unchanged subsystem to be redesigned.

Three repetitions and selected scenarios do not establish general reliability. App skill
catalogs changed between waves 2 and 3, identically for all arms in each wave; local packets
remained frozen. Early host-load samples are missing. Directories were isolated by scope
instructions and audited calls, not an OS secrecy boundary. No production harness was changed
during the experiment; the subsequent A adoption is recorded above.

## Latest: one repair attempt with every known gap supplied

Two fresh Sol/xhigh workers received identical B2/P3 code, the original approved plan and
ontology, and the full eight-scope gap packet (`planning-closure-20260929/frozen/GAPS.md`).
Inherited producers were explicitly in scope. Each had one attempt of up to sixty minutes,
including self-tests. Findings were saved before edits; no independent evaluation feedback
or second repair turn was supplied. This tests repair given diagnosis, not discovery.

| Supplied gap | C1 | C2 |
|---|---|---|
| Required attachments reach invocation | Partial: transport works locally; guest mount/privacy conditions unresolved | Violated: valid 255-character upload name becomes an unusable 257-byte filename |
| Mixed failed prerequisites, either settlement order | Repaired; actual local settlement checked | Repaired; actual local settlement checked |
| Explicit Stop versus future shutdown warning | Repaired | Repaired |
| Adopted permission/tool mapping | Supported locally | Backend repaired; generated frontend catalog remains inconsistent |
| Unknown usage remains unknown through accounting/admission | Violated: upstream mapper supplies zero | Violated: same mapper failure; also blocks another eligible workspace |
| Complete ordinary profile/bootstrap allocation | Repaired | Repaired |
| Ordinary sync producer retains retry progress | Repaired, including selected-subset check | Ordinary producer repaired; retained synthetic subset still fails, current reachability uncertain |
| Migration checksum catalog | Repaired | Repaired |

The shared accounting failure is concrete: a partial provider-result frame passes through
the real event mapper, lifecycle writer and reservation operation. Missing token usage
becomes zero, the writer marks it known, and another task is admitted. Both workers fixed
the writer's handling of explicit unknowns but missed the earlier conversion. A hand-built
unknown-value test therefore passed while the actual producer path failed.

Independent evaluation found C1's guest attachment mount conditions unresolved; C2
introduced a long-filename failure, catalog drift and cross-workspace queue blocking.
New cases were checked for applicability to both candidates. C2's progress scope was
corrected from violated to partial: its actual ordinary producer passes, and the failing
retained fixture's current reachability is unproven. Original verdicts and executed failures
are retained in the [reconciliation](planning-closure-20260929/reconciliation/REPORT.md).

The attempts used **38m 18s and 42m 28s**, totaling **872,113 uncached input and 198,623
output tokens**. Their original independent backend checks passed 24/24 and 23/24;
the later producer counterexamples show why those counts alone do not establish completion.
No live provider/VM/browser journey was executed. This is not a controlled estimate of
diagnosis benefit over the old thirty-minute repair condition.

Preparation plus independent assessment/reconciliation used another **47m 53s of Astra
worker time and 127,830 output tokens**. Root setup, runtime evaluation, supervision and
reporting added **110,361 output tokens through 02:25:41 UTC**, with later final reporting
outside that cutoff. [Full input/cache/time accounting and evidence limits](planning-closure-20260929/RESULTS.md#effort-and-evidence-limits).

The next repair hypothesis is to require evidence from the actual producer through the
authoritative consumer for each supplied gap, with allowed and failing cases. The accounting
example gives that proposal a specific target. **That comparison and further repair loops
were not run.** Neither experimental repair was integrated into production.

**Was the extra repair worthwhile?** The Stop, dependency-selection, input-material,
permission and ordinary-progress changes are material. C1 is the strongest measured
delivery; C2 has more complete retry behavior but a new cross-workspace queue regression.
Both are additive to B2/P3, not cheaper substitutes for that history: including B2's
repairs/reviews, the paths cost 97m 28s/282,696 output tokens and 101m 38s/283,439, before
discovery and Astra evaluation. The present skill already calls for commitment-to-evidence
closure; duplicating that instruction alone is not a demonstrated remedy.
[Original goal/design, quality of every B/P and C checkpoint, regressions and full cost comparison](REPAIR-VALUE-AND-COST.md).

## Final comparison: alignment with the agreed intent

No approved feature ontology diff existed for this implementation. The feature metric is
**alignment with the original approved plan**; governing ontology is assessed separately.
Astra evaluated the **same eight plan scopes and thirteen governing commitments** at every
checkpoint, including relevant unchanged producers and consumers.

These eight scopes are **the evaluator's groupings of the longer plan**, not eight
independent requirements supplied by the user. The original plan is now directly readable
as [APPROVED-PLAN.md](consistency-20260928/APPROVED-PLAN.md), with bytes verified against the
frozen worker-input hash. The assessment includes inherited dependencies and later
uniformly applied discoveries as well as defects supplied to the fixers. It therefore
does not measure the fraction of supplied findings successfully repaired.

**Supported** means evidence supports the declared source/local boundary; **partial** means
coverage or conditional applicability remains unresolved; **violated** means a concrete
counterexample exists. Each obligation counts once. These are neither run counts nor an
averaged quality score. Real provider/VM/browser behavior remains outside the verified boundary.

| Checkpoint | Approved plan: 8 scopes | Governing ontology: 13 commitments |
|---|---|---|
| Untouched baseline | 1 supported; 7 violated | 4 supported; 1 partial; 8 violated |
| B1/P1 | 2 supported; 1 partial; 5 violated | 9 supported; 1 partial; 3 violated |
| B1/P2 | 2 supported; 1 partial; 5 violated | 9 supported; 1 partial; 3 violated |
| B1/P3 | 2 supported; 1 partial; 5 violated | 9 supported; 1 partial; 3 violated |
| B2/P1 | 2 supported; 6 violated | 8 supported; 1 partial; 4 violated |
| B2/P2 | 2 supported; 6 violated | 8 supported; 1 partial; 4 violated |
| B2/P3 | 2 supported; 6 violated | 8 supported; 1 partial; 4 violated |
| B3/P1 | 2 supported; 6 violated | 8 supported; 1 partial; 4 violated |
| B3/P2 | 2 supported; 6 violated | 9 supported; 4 violated |
| B3/P3 | 2 supported; 6 violated | 9 supported; 4 violated |
| Explicit closure C1 | 5 supported; 1 partial; 2 violated | 13 supported in the original subset; unknown-usage rule outside it violated |
| Explicit closure C2 | 4 supported; 1 partial; 3 violated | 12 supported; 1 violated in the original subset; unknown-usage rule outside it also violated |

An unchanged broad count can contain a real repair: B2 now preserves accepted launches,
but its broader dispatch scope still violates permission/accounting requirements. B3 closes
the narrower specification-validation gap, while the inherited mixed-prerequisite case still
violates the plan’s refusal requirement. No final candidate satisfies the complete agreed intent.
The eight scopes are not eight bugs or tests: one remaining counterexample keeps a whole
scope violated. Use the per-case repair tables above to assess incremental progress.

### All eight scopes: what passed and what remained

The following gives the complete scope list and final statuses. **S/P/V** mean
supported/partial/violated at the evidence boundary above. Source findings are not
presented as executed provider failures.

| Scope from the approved plan | B1 / B2 / B3 at P3 | Supported behavior and reason for the remaining status |
|---|---|---|
| **I1 — Correct selected closure, faithful effective input/material, lineage and reused success.** | V / V / V | The reference graph, copied text, lineage and successful-prerequisite reuse pass. Copied attachments have no transport into ordinary provider invocation. A mixed-failure ordering can also omit a blocked descendant. Both residuals are source findings. |
| **I2 — Preserve original tasks/history and allocate fresh task/session/input/attempt authority.** | S / S / S | Local graph/snapshot checks support fresh identities without resetting the original tasks, inputs or attempts. Real provider history and exhaustive schedules remain unverified. |
| **I3 — Same accepted request, including concurrent replay, returns one graph; changed input conflicts.** | P / S / S | Current-version replay/concurrency/conflict checks pass. B1 has a conditional incompatibility with original-version receipts; an actual deployed legacy population was not established, so this is partial, not a demonstrated current-version failure. |
| **I4 — Refuse invalid graph/input/specification without partial allocation.** | V / V / V | Tested invalid-input/launch and late-failure rollback cases pass. The mixed-prerequisite case can bypass a required refusal when the outside prerequisite failed first—the same source defect also affects I1. |
| **I5 — Current owner/live admission, no implicit Start, and current-read historical replay.** | S / V / V | Future-warning admission and authorized receipt recovery pass. B2/B3 nevertheless allocate after explicit Stop while the VM still reports running; B3 also allocates after an expired warning. These are executed regressions. |
| **I6 — Preserve accepted launch while enforcing current account, permission and dispatch eligibility.** | V / V / V | Frozen launch, including changed defaults, passes after B2/P3. The broader scope retains inherited missing Claude deny-policy mapping and unknown-usage accounting failures in source. B1/B3 additionally fail the executed recorded-failed-task spending check. |
| **I7 — Owning allocation/settlement/cancellation, readiness and capacity.** | V / V / V | Complete retry allocation, tested job caps and ordinary cancellation/settlement pass. Older profile/bootstrap producers still allocate outside the complete TaskService operation; spending also overlaps I6. These broader owner/accounting findings keep the row violated. |
| **I8 — Existing UI submission, reload recovery, durable history and truthful new-run progress.** | V / V / V | Basic saved recovery, lineage and component checks pass. All three fail the actual ordinary sync-producer → retry → progress check: expected direction is lost. Additional candidate-specific UI/retained-shape limits remain in the detailed assessment. |

### Why review did not close the six violated scopes

- **I1:** workers checked persisted text/artifact copying, not attachment consumption by
  the invocation interface. The missing transport was an evaluator discovery after P1,
  not a concrete defect supplied in the original repair packet.
- **I4:** checks covered mixed failed prerequisites with the descendant already attributed
  to the selected root. They missed the opposite settlement order, where it is attributed
  to the outside failed prerequisite and excluded before validation. This interpretation
  was source-adjudicated after the final reviews; no runtime failure is claimed. The
  competing interpretation and why it was rejected remain in the linked addendum.
- **I5:** allowing the future-warning case weakened the Stop guard. Review fixtures used
  stopping/expired states but did not discriminate an explicit Stop marker while the VM
  still appeared running. The positive warning repair was mistaken for complete admission
  coverage. B1 preserved this neighboring behavior; B2/B3 did not.
- **I6:** reviewers closed the original launch and permission-preflight claims at a narrower
  boundary. They did not establish the full adopted policy mapping or unknown-usage writer
  semantics. B2's final reviewer explicitly left unknown accounting unverified; passing
  recorded-spend checks did not resolve that separate obligation.
- **I7:** reviewers repaired the retry owner operation and ordinary input binding, but did
  not migrate older producer allocation into that owner. Some reports explicitly treated
  those older producers as outside retry-specific closure. The evaluator included them
  as material governing dependencies. This is a scope distinction, not evidence that the
  repaired retry allocation itself remained wrong.
- **I8:** fixes and passing fixtures relied on task path metadata the actual ordinary
  producer leaves empty. Repeated checks refined that synthetic case rather than exposing
  the missing real-producer data. Hence several real progress improvements did not repair
  the ordinary sync path.

**Scope judgment:** I1/I4/I5/I8 have direct anchors in the requested retry behavior. I6
combines that behavior with inherited dispatch-policy/accounting obligations. I7 is the
most scope-sensitive grouping: complete ownership of the *new retry operation* is explicit
in the plan and is repaired; migration of older producer operations is a broader governing
ontology assessment. It should be reported as inherited conformance debt, not described as
failure to implement the new retry owner. Applicable inherited defects still matter, but
the report must identify what makes them relevant rather than imply six newly requested
features were left undone.

The denominator also overlaps: one mixed-prerequisite defect violates I1 and I4; accounting
affects I6 and I7. Later evaluator discoveries extend detection coverage and were withheld
from the repair loop. Thus **2/8 is neither a 25% requirement success rate nor a measure of
repair reliability given known defects**. Preserve these broad alignment results, but use
distinct cases with baseline/inherited/introduced provenance to measure repair progress.

The executed Stop regressions and real-producer progress failures remain material gaps.
They do not erase the useful repairs or make honest partial delivery a failed review.
The separate review-quality failures are unsupported closure claims and introduced
regressions, particularly B2's false-clean stop. See the
[three-dimension reassessment](../../ontology_harness_tuning/ontology_review_experiment_finding.md) for the evidence
on scoped coverage, reporting accuracy and preservation across iterations. These results
are not a general population estimate of a coding model's ability to implement requests.

[Exact obligations, evidence, limits and versioned corrections](consistency-20260928/ALIGNMENT.md).
The final source addendum applies the mixed-prerequisite case to every stage, preserving the
earlier labels and the competing interpretation; it adds no new denominator.

### Alignment after the two explicit-gap repairs

The same eight scopes now have these results. These are independent one-shot candidates
from B2/P3, not P4/P5 or additional repair loops. The full gap packet explicitly included
the broader inherited producer obligations discussed above.

| Original plan scope | C1 | C2 | Remaining limitation or evidence |
|---|---|---|---|
| I1 — Selection, effective input/material, lineage, reused success | Partial | Violated | Mixed-cause selection repaired in both. C1 attachment mount/privacy conditions remain unresolved; C2 fails a valid maximum-length filename. |
| I2 — Original history and fresh identities/authority | Supported | Supported | Local persistence and source checks support fresh allocation without resetting original work. |
| I3 — One accepted request, one graph | Supported | Supported | Concurrent replay and changed-input conflict remain supported. |
| I4 — Invalid requests refuse atomically | Supported | Supported | Both real local mixed-settlement orders refuse the outside failed prerequisite; rollback checks pass. |
| I5 — Current admission and historical read recovery | Supported | Supported | Explicit Stop, future warning, expired deadline and read-authorized receipt recovery now pass. |
| I6 — Accepted launch and current dispatch eligibility | Violated | Violated | Launch/account policy improved; actual missing-usage producer still becomes known zero and permits admission. |
| I7 — Owning operations, readiness and capacity | Violated | Violated | Ordinary allocation ownership repaired. Same accounting defect remains; C2 also blocks eligible work in another workspace. |
| I8 — UI recovery, history and truthful progress | Supported | Partial | Both actual ordinary sync producers pass. C2's retained synthetic subset fails; its current production reachability remains unproven. Full browser journeys are outside both boundaries. |

**Neither reached 8/8 supported.** C1's two violated rows share one concrete accounting
defect; its attachment row is partial. C2 also has a concrete attachment regression and
an unresolved progress shape. This is substantial progress from B2/P3, but not complete
alignment. The original thirteen-governing-rule subset omits the separate unknown-usage
rule; C1's 13/13 within that subset must not be presented as full ontology conformance.
The final judgments preserve original authority and distinguish actual counterexamples
from conditional and unavailable evidence. [Initial assessments and reconciled evidence](planning-closure-20260929/RESULTS.md).

### Final metric: planning alignment in the three new scenarios

These totals separate the new request from retained governing constraints across nine
plans per condition. They measure the saved designs and ontology diffs; implementation
alignment remains untested.

| Alignment evidence | Current | A | B |
|---|---:|---:|---:|
| New-request obligations supported /24 | 23 | 23 | 24 |
| Existing constraints supported /75 | 64 | 68 | 68 |
| Complete selected obligation instances /99 | 87 | 91 | 92 |
| Mean coverage with each scenario weighted equally | 88.4% | 92.4% | 93.3% |

The percentages describe selected design coverage, not overall quality or a probability
of correct implementation. Neither variant passed the frozen consistency/cost rules;
A is now incrementally adopted for its observed coverage/effort tradeoff, while treating
the expected modest benefit as a hypothesis to keep testing.
