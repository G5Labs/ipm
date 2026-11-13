# What the experiments establish about ontology review

2026-09-29. Retrospective assessment, updated with the six-run ontology-only inventory
experiment below. No new repair or application test was run for this update. Historical
results remain preserved, with later evaluator corrections taking precedence.

**A useful review can leave a complex feature incomplete.** Its value is finding and
closing relevant gaps, accurately explaining what remains, and preserving working
behavior. My earlier emphasis on failure to complete the entire feature obscured those
separate questions.

| Desired condition | Where the evidence puts us |
|---|---|
| Comprehensive review against the design and applicable principles, within the PR | **Partly demonstrated.** Reviews found architecture, authority, compatibility and verification issues as well as functional bugs. Coverage still varies, and some old evaluation rows included unrelated inherited debt. |
| Clear, accurate account of satisfied obligations and remaining gaps | **Useful reporting, but not dependable yet.** Several reviews explicitly refused a clean stop; C1/C2 disclosed substantial uncertainty. We also have a false-clean review and narrower claims of support contradicted by available checks. |
| Progressive repairs without regressions | **Useful progress demonstrated; preservation not reliable.** Later passes closed real defects, but some repairs broke previously working cases. Repetition alone has not established a dependable improvement loop. |

These are judgments about the observed experiments, not population reliability estimates.
The evidence includes the [earlier feature experiments](ontology-experiment-results.md),
the [balance study](../balance-study/README.md), and its
[review/repair follow-ups](../balance-study/review-fix/README.md). Workers were GPT-6 Sol;
GPT-6 Astra supervised and evaluated the balance-study follow-ups. Early comparisons
had unequal assistance; later studies preserved more comparable inputs and independent
checks. The current skill also changed during this work: historical results are not all
trials of today's three-reviewer workflow.

## 1. Review the right obligations, within the change

The review target should be the **original approved ontology changes/design, their
applicable governing concepts and invariants, and the actual implementation diff and
affected callers**. Regular correctness, design, security and repository requirements
still apply when no ontology block names a particular defect.

The plan is the starting inventory, not a ceiling on investigation. A newly discovered
dependency can be essential to the promised change. Conversely, following ontology links
does not authorize repairing every nonconforming component encountered.

| Finding's relationship to the change | Treatment |
|---|---|
| New implementation violates its design, or a repair breaks existing behavior | In scope; repair or explicitly retain the gap. |
| An existing dependency prevents the new feature's promised behavior | In scope as an integration obligation. Establish the concrete affected path; choose a bounded fix or expose the design decision. |
| An unchanged component has independent architectural or behavioral debt | Record separately if useful; exclude from this PR's completion denominator and repairs unless scope is explicitly expanded. |
| The requirement itself is ambiguous or inconsistent | Record a design question. Do not invent a restriction or silently amend intent to obtain a pass. |

There are concrete scope problems in the old evaluation. **Experiment 2 marked A07
failed for an existing IssueService allocation bypass while explicitly calling it
outside the PR. That should not count against PR completion under this definition.**
In the retry study, complete ownership of the *new retry allocation* was required;
migrating otherwise unchanged profile/bootstrap allocation was broader inherited debt.
The later C1/C2 request explicitly included that migration, so it was valid for those
repairs. Missing attachment transport and ordinary-source retry progress, however,
directly prevent the requested retry behavior even when the faulty dependency predates
the PR. [Scope assessment](../balance-study/review-fix/README.md#why-review-did-not-close-the-six-violated-scopes).

The retry implementation had an approved plan, **not an approved feature ontology diff**.
Its eight rows were the evaluator's broad groupings of that plan. They overlap and include
inherited dependencies; “2/8 supported” is not “25% of the requested implementation works.”
We should preserve that history and produce a separately scoped assessment, rather than
silently relabeling the original results.

The original shared plan also omitted explicit governing-node references required by the
planning skill. Reviewers searched the ontology themselves; the evaluator's eight scopes
preceded coding, but its exact 13-node mapping was assembled during discovery before
repairs. These results therefore do not establish review reliability with the complete
prescribed design handoff.

### New evidence: deriving the inventory from supplied ontology

The [six-run inventory experiment](../balance-study/review-fix/review-scope-20260929/README.md)
gave two fresh Sol workers per feature the same ontology diff/governance and no code or
known findings. Retry, host draining and stale Save all produced useful, repeatable core
review areas. Exceptions and supporting conditions were less complete: only one host
worker retained the provider-bundle exception; both Save workers omitted replay-capacity
handling. Those rules had appeared in their retrieved text. Combining the two lists
recovers one-sided details but still misses shared omissions.

All six include ownership and affected consumers; concrete design and conditional
compatibility/verification checks vary. This supports using a saved obligation inventory
as the review starting point, with source validation of conditions and exceptions.
It does not demonstrate exhaustive coverage, the handoff's advantage over a control,
or better code review/repair. Retry used its retained broader D2 draft, including bootstrap,
not a reconstruction of the later coding plan's missing approved ontology diff.

### Did we cover more than functionality?

**Yes, materially—but not comprehensively enough to certify that dimension.** The
[experimental rubric](../balance-study/review-fix/REVIEW-RUBRIC.md) and
[current review skill](../../../.agents/skills/ontology-review/SKILL.md) explicitly cover
architecture, minimal implementation, compatibility, code quality and verification.
The observed evidence is narrower than those instructions:

| Area | Evidence we have | What remains under-evaluated |
|---|---|---|
| Feature and governing-rule alignment | Accepted-launch preservation, fresh retry identities, current authority, replay, material transport and progress received concrete checks. | Discovering all material obligations: only 3/18 reviewers visibly retrieved the exact shutdown-warning exception; all 18 retrieved the launch rule, but only ten reported its violation. |
| Architecture and design principles | Review found incomplete TaskService ownership; B2/P2 moved session composition into the complete operation. C1/C2 improved ordinary allocation ownership. Transaction ownership, immutable history and replay authority were also assessed. | Systematic assessment of API simplicity, dependency direction, duplication and unnecessary mechanisms across the whole diff. The C1/C2 API-shape comparison was targeted source judgment, not a comprehensive maintainability audit. |
| Regression and backward compatibility | Checks covered explicit Stop, valid future warnings, successful prerequisite reuse, historical receipt reads, source-reader access and shared lifecycle callers. | Consistent parent-versus-repair coverage of all affected consumers; actual legacy populations, migration upgrades and full UI journeys. Several cases were found only by later evaluation. |
| Security, data and coding requirements | Owner/account checks, tool permissions, attachment authorization, transaction rollback, migration checksums, builds and type checks were assessed. | A systematic review of applicable coding conventions, error handling, resource cleanup, operability and performance. Concurrency coverage was selective; native provider/guest behavior remained unexecuted. |
| Minimal scope and justified design | Consolidation retained uncertain tenant-constraint and historical-graph proposals as unresolved rather than automatically requiring redesign. | False-positive repair cost and whether all optional cleanups were consistently separated from required changes. |

[Discovery evidence](../balance-study/review-fix/consistency-20260928/DISCOVERY.md) shows
that every three-reviewer team's union covered more known gaps than its best individual.
General teams found 7, 5 and 6 of seven known gaps; focused teams found 5, 5 and 6.
This supports retaining independent full-scope discovery and minority findings. It does
not show exhaustive coverage or isolate the rubric's benefit: both arms received it.

**Next measurement:** freeze a concise, PR-specific obligation inventory before review,
including applicable design/compatibility requirements. Let reviewers extend it with
reasons. Independently audit omissions and unjustified additions, not just discovered
bugs. A design principle can have source evidence; it need not be forced into a runtime
test or expanded into a universal checklist.

## 2. Report partial success accurately

This needs its own evaluation. An evaluator's detailed final gap table does **not** prove
that the worker knew or reported those gaps at the end of its pass.

| Observed report | What it establishes |
|---|---|
| Final B1 and B3 reviewers retained actionable findings and said they could not stop cleanly. | Positive evidence of honest incompleteness. B3 even retained a failed recovery/reset component check. |
| Final B2 reviewer allowed a clean stop after dispositioning supplied groups and passing 985 core tests. Independent checks still failed explicit Stop, ordinary-source progress and the migration catalog. | A false-clean decision within the PR scope, even after excluding inherited architectural debt. It did disclose some unexecuted live/concurrency cases; the error was missing other feasible, material failures. |
| Final B1 reviewer marked spending and progress supported using a zero-threshold test and synthetic selected-target fixtures. Recorded failed-task spending and actual ordinary-source progress still failed. | “Cannot stop” was appropriate overall, but individual closure labels were too broad for their evidence. |
| C1/C2 self-reviews separated local checks from unavailable provider/VM/browser behavior and marked many scopes partial. | Much more useful than a blanket completion claim. However, both missed the upstream conversion of missing usage to known zero; C2 also missed its new cross-workspace queue blockage. Disclosure of test limits is not a complete inventory of remaining defects. |
| Fork & Start second-run self-review claimed a batch-rotation repair, but its timestamp update was rolled back. | A detected issue and edited code were mistaken for a working repair. The source finding is retained; a 17-operation runtime outcome was not executed. |

Sources: saved loop decisions and individual claims (archived `balance-study/review-fix/consistency-20260928/loop-summary.json`),
[independent final-pass assessment](../balance-study/review-fix/consistency-20260928/P3-ASSESSMENT.md),
C1 worker report (local `balance-study/review-fix/planning-closure-20260929/deliveries/c1/repair/artifacts/review.md`),
C2 worker report (local `balance-study/review-fix/planning-closure-20260929/deliveries/c2/repair/artifacts/review.md`),
[independent C reconciliation](../balance-study/review-fix/planning-closure-20260929/reconciliation/REPORT.md),
and [earlier experiments](ontology-experiment-results.md#experiment-7-second-run--fork--start).
Raw worker artifacts are omitted from this PR; the linked study summaries are durable records.

### What a useful iteration report should show

Use one concise row per review obligation, with:

`original obligation → supported subcases and evidence → remaining defect/unknown →
change since the previous pass → next action or blocker`

Keep **known violation**, **missing evidence** and **unresolved design** distinct. Preserve
the evidence boundary: source inspection, helper, actual producer/consumer, public API,
or full user journey. A passing narrower check should not close a broader claim.

Rough percentages can help **if the denominator is visible**. For example, an illustrative
four-case admission obligation might have two supported cases, one known violation and
one unverified case: “2/4 cases verified (50%); Stop still fails; transfer untested.” That
is understandable partial progress, not a claim that the feature is 50% correct. Include
severity: one authority failure can matter more than several passing convenience cases.
Do not turn the historical eight broad rows into a percentage or invent retrospective
percentages from incomplete records.

**Next measurement:** compare each frozen worker closure claim with independent evidence
at that same checkpoint. Count unsupported closure claims, material gaps omitted from the
report, correct uncertainty disclosures, and false-clean decisions. A review that reports
two genuine closures and six accurate remaining gaps should score well on reporting even
if its repair coverage is modest. Existing reports give examples in both directions; we
have not yet completed this systematic claim audit.

## 3. Improve across iterations without losing working behavior

The broad alignment totals concealed real gains. B2/P2 repaired complete retry allocation;
B2/P3 repaired accepted-launch preservation. B1 and B3 corrected narrower progress
projection errors. Later, C1/C2 closed important remaining Stop, mixed-prerequisite and
ordinary-source progress gaps. These were useful changes, not cosmetic score improvements.
[Checkpoint comparison and costs](../balance-study/review-fix/REPAIR-VALUE-AND-COST.md).

Preservation was less dependable:

| Transition or comparison | Evidence and limit |
|---|---|
| Untouched retry → first B repairs | B2/B3 introduced explicit-Stop admission regressions while repairing future-warning admission; B3 also admitted expired warnings. The valid neighboring case was repaired without preserving the forbidden cases. |
| Host-drain implementation → generic review repair | A shared helper change broke stale-host replacement. The existing test passed before review and failed twice afterward: direct before/after regression evidence. |
| B2/P3 → C2 | Unknown accounting in one workspace aborts selection of healthy work elsewhere: a new queue-availability regression. C1 preserves the healthy-workspace case. C2 also loses a previously passing synthetic subset-progress case; its ordinary production reachability remains unresolved. |
| B2/P3 → C1 | Material gaps close without a new unconditional loss of previously passing product behavior demonstrated in the evaluated set. This is encouraging, limited evidence—not a guarantee about all callers or real guests. |
| Flush-ownership experiment, initial → final | Both reviews repaired replay epoch scoping. All 57 common initial checks remained passing; an additional final-only process check also passed. A positive preservation example, although the existing tests did not distinguish the source-level epoch repair and some races remained untested. |

Sources: [first-pass checks](../balance-study/review-fix/consistency-20260928/P1-ASSESSMENT.md),
[host-drain comparison](ontology-experiment-results.md#experiment-5--drain-a-worker-host-before-removing-it),
[C reconciliation](../balance-study/review-fix/planning-closure-20260929/reconciliation/REPORT.md),
and [flush experiment](ontology-experiment-results.md#experiment-4--flush-ownership-across-incarnation-release-2026-09-27).

A newly discovered old defect is **not** a repair regression. C2's attachment work also
illustrates a separate category: its new filename handling is defective, but the starting
candidate did not have a working attachment-transport path to preserve. Record defective
new behavior separately from lost previously working behavior.

**Next measurement:** compare every accepted repair to both its immediate parent and the
original branch base. Reuse valid evidence, recheck behavior affected by the patch, and
retain discriminating checks for earlier repairs plus relevant existing callers. Any new
failure needs origin and applicability adjudication. Report material closures, newly
introduced defects, retained defects and unknowns separately; a larger pass count must
not hide a new serious failure. We have selected comparisons, not a complete transition
matrix demonstrating that the whole loop is monotonic.

## What I would add to the three conditions

1. **A trustworthy evaluator and stable scope.** Our evaluator has also been wrong.
   The [Fork & Start adjudication](ontology-evaluator-adjudication.md) withdrew an invented
   post-readiness chat deadline and blanket progress failures; raw-key compatibility
   needed a declared identity contract. Incorrect expectations can reward overrestriction
   and prompt harmful repairs. Evaluate legitimate allowed cases alongside refusals, and
   preserve corrections without rewriting history.
2. **Progress and an economic stopping rule.** Accurate reporting plus no regressions
   can still plateau indefinitely. Each pass should justify its cost through validated
   closures or useful new knowledge; otherwise expose a concrete human decision,
   missing fixture or unresolved technical problem. B trajectories consumed roughly
   59–63 summed repair/reviewer minutes; C1/C2 then added 38/42 repair minutes to B2/P3,
   excluding diagnostic/evaluation cost. That is useful context, not a controlled
   cost-effectiveness result. No number of repetitions guarantees convergence.
3. **Allow knowledge to become more accurate.** An honest review may downgrade an earlier
   “supported” label after discovering a missed path. Requiring the displayed score to
   increase every time would discourage exactly that correction. Require preservation
   of actual required behavior; welcome better disclosure even when the reported
   coverage decreases. Nor can a reviewer promise that its remaining-gap list is exhaustive.

## The next evaluations worth doing

**Audit the inventory against its sources.** The user-requested three-feature,
two-repetition [execution](../balance-study/review-fix/review-scope-20260929/README.md) is now
complete. Next test one independent condition/exception audit of a consolidated inventory,
without supplying its known omissions. Measure added valid obligations, unjustified
requirements and effort. The original handoff-versus-reconstruction comparison remains
unrun; the section requirement alone is not evidence of improved behavior.

**First, audit existing artifacts without rerunning workers.** Re-scope the obligation
inventory to each original change, retain inherited debt separately, and align worker
claims with independent evidence at each checkpoint. Build the missing transition table
from available receipts; label missing before/after evidence unknown. Audit representative
design and compatibility obligations as well as feature bugs. This directly measures
conditions 1–3 more fairly than another broad success count.

**Then test a bounded loop on other changes.** Use two or three distinct PR-sized tasks
with saved approved designs and applicable governing rules. Retain the adopted three
independent full-scope reviews, consolidation and coherent repair; cap the number of
iterations and record all effort. Freeze claims before independent evaluation. At each
checkpoint measure scope coverage, claim accuracy, material closures and regressions
against the parent. Make every feedback item visible in the record so an informed repair
is not mistaken for independent discovery. A later controlled comparison can test whether
a concise design-gap handoff improves those outcomes per unit of effort. These evaluation
proposals have not run.

The final measure should remain **alignment with the original approved design and
applicable governing ontology**, with supported subcases, remaining defects and unknowns
visible. The existing evidence supports further investment in this review approach. It
does not yet establish comprehensive discovery, consistently accurate closure reporting,
or reliably regression-free repair loops.
