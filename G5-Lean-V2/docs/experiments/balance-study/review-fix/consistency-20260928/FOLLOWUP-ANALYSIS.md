# Cost, missed launch findings and repair stopping

Analysis of the preserved experiment, 2026-09-28. No workers or application checks were
rerun. Public actions, returned tool output, findings and verification records are the
evidence; they do not reveal a worker's private reasoning. Existing results are unchanged.

## 1. General reviews and cost

All nine general reviewers received identical prompts after substituting their isolated
directory identifiers. They used the same revised review skill, rubric, plan, code and
sanitized ontology. Focused prompts differed by the assigned priority; their full scope
and other inputs remained the same. Both conditions used three independent parallel Sol
reviewers per team, followed by main Astra consolidation.

| Measured average per three-reviewer team | General | Focused |
|---|---:|---:|
| Summed reviewer time, including tools and reports | 23m 11s | 23m 47s |
| Output tokens, including reasoning | 77,709 | 79,626 |
| Main consolidation time | 51s | 57s |
| Known gaps found in the three teams, out of seven | 7, 5, 6 | 5, 5, 6 |

Focused review used 2.6% more reviewer time and 2.5% more output tokens: essentially
similar measured effort in this sample. These are not dollar costs or total token costs;
input and cached-input usage are retained in effort-summary.json (`effort-summary.json`).
Parallel reviewer time is summed, not wall-clock latency. Setup is excluded.

A single general reviewer averaged 7m 44s and 25,903 output tokens. Three reviewers cost
roughly three such passes plus consolidation. General teams' best individual reviewers
found 5, 4 and 5 known gaps; their unions found 7, 5 and 6. This supports combining
independent findings, but does not establish that three beats two, or a single reviewer
given an equal total budget. General was slightly better overall here; focused was better
on accepted-launch drift and sync scope. One feature and three teams per condition do not
establish universal superiority.

The supported working choice is parallel full-scope discovery followed by main-agent
consolidation that preserves minority findings and checks their evidence. Three is a
reasonable next control, not a proven optimum. Keep discovery separate from coherent
repair. The per-run prompts are retained locally under `prompts/`; the
discovery mapping (`diagnosis-map.json`) is committed.

## 2. The future-shutdown exception was not explicit in the plan

The frozen `PLAN.md` says fresh submission requires open admission and ordinary
stop/transfer gates. It does **not** state the future-deadline exception or pin its block.
The exception is in the correct supplied ontology, `g5.lifecycle.deadline`: a valid future
nudge leaves the VM running and admission open; the pending stop identity alone does not
close admission. The reviewers therefore had to retrieve it from ontology.

Only g13, f21 and g33 visibly returned that exact paragraph; all three reported the bug.
The f33 reviewer opened the lifecycle file but read ownership/deletion ranges that did
not contain the exception. Removing prior findings did not remove this governing rule.
This is partly a handoff weakness: the plan did not explicitly carry the material exception
into its governing-reference map. The study also had no approved feature ontology diff;
its feature contract was the original shared plan.

See retrieval routes (`retrieval-routes.json`) and the preserved
input manifest (`preserved-inputs.json`). The immutable worker copy used for this analysis
was `/private/tmp/g5-review-consistency-20260928/g21/PLAN.md` and its
`.g5/ontology/software_architecture/lifecycle.g5.md`.

## 3. Why eight reviewers missed accepted-launch drift

The missing comparison is concrete: accept model M1 through a default, change the default
to M2 while M1 remains eligible, then dispatch. The frozen accepted launch must remain M1.
Current authorization can still refuse a revoked account; that is a separate condition.
The baseline saved launch preferences and re-resolved them at dispatch using current
effective settings, permitting silent drift.

**All eight missed reviewers visibly received the freeze requirement, the task-preferences
write, and the dispatch re-resolution call.** Five also received the resolver body showing
its use of current effective settings. None documented a changed-default reservation check.
Thus missing ontology or failure to open dispatch does not explain these eight misses.

The raw reviewer reports referenced by ID below remain local under `deliveries/`.

| Reviewer | Observable route and omission |
|---|---|
| `g13: general` | Read caller and resolver body. Found dispatch capacity and future-nudge defects; explicitly left post-commit dispatch unverified. Did not compare accepted resolved identity with later dispatch identity. |
| `f11: intent/invariants` | Read caller and resolver body. Found catalog-valid permission modes rejected at dispatch. Its launch coverage discussed current-account resolution and copied preferences, without checking whether later defaults change the accepted value. |
| `f13: architecture` | Read caller and resolver body. Diagnosed the default Codex permission-mode incompatibility and capacity gap. The launch ledger assessed pre-allocation validity, leaving accepted-value stability through dispatch unassessed. |
| `g21: general` | Saw the dispatch call; the exact current-settings resolver body was not visibly returned. Traced carried-over Claude tool restrictions into a newly selected Codex launch. No separate frozen-value-through-dispatch check appeared in its ledger. |
| `g22: general` | Read caller and resolver body. Reported nine issues, including permission admission versus dispatch. It did not separate that permission obligation from accepted-value stability, and did not execute dispatch. |
| `g23: general` | Explicitly searched launch snapshots and opened the dispatch call, but the exact resolver body was not visibly returned. Its configuration-change finding concerned receipt recovery; no acceptance-to-dispatch default-change comparison was recorded. |
| `g33: general` | Read caller and resolver body. Found permission-mode rejection and the future-nudge bug. Its reported launch counterexample was permission incompatibility; changed-default dispatch remained unchecked. |
| `f32: behavior/integration` | Opened dispatch while tracing capacity; the exact resolver body was not visibly returned. Found UI/progress/capacity defects, but recorded no accepted-launch identity check and left dispatch eligibility unverified. |

The observable failure is incomplete **semantic coverage**: five had both sides of the
resolver behavior in context but did not connect them to the temporal invariant; three
also had an incomplete visible helper trace. Five of the eight reported a different
launch-policy defect. That suggests nearby findings may have substituted for checking
each launch obligation, but attention or psychological anchoring is a hypothesis, not
an observed fact. Absence of an exact marker likewise does not prove that no equivalent
information was ever seen.

These reviewers finished 93–182 seconds before their ten-minute limit. They were not
forcibly cut off. They found other issues and often declared boundaries partial, so these
are missed findings, not eight false-clean reports. A successful reviewer, `g11/F02`,
explicitly connected preferences-only persistence to dispatch re-resolution and proposed
changing defaults before descendant release. That concrete trace distinguishes success.

Returned-output markers and exact actions (`launch-miss-visible-evidence.json`) support
the retrieval claims; deadline audit (`deadline-audit.json`) supports the timing. The
existing review skill already requested deferred-effect traces and counterexamples.
Adding another generic instruction to be comprehensive would not address the observed gap.

## 4. Improvements to test

These interventions are hypotheses; this experiment has not established their benefit.

| Failure mechanism | Proposed enforceable step | Case that would reveal the failure |
|---|---|---|
| Material exception never retrieved | Before discovery, resolve the design's governing block IDs to their authoritative text and exceptions. Have reviewers independently expand that map from changed operations and consumers. | Future warning permits work; explicit Stop and expired warning refuse. |
| Rule retrieved but not checked at its consumer | Record each material obligation separately, including persisted representation, actual consumer, state transition and counterexample. Main consolidation checks omissions as well as findings. | Accept M1, change the default to M2, reserve M1; separately refuse revoked authority. |
| Repair changes the success criterion | Preserve a reproducer and ontology-derived expected result before edits; independently check the assertion before allowing closure. | B2 initially counted rejection of a still-valid accepted launch as success. |
| Tests bypass the real producer or relevant state | Use actual owner/producer operations and paired neighboring-state checks; disclose synthetic-fixture limits. | Ordinary sync producer has empty metadata assumed by the repair; failed-task usage must count toward spending. |
| Empty review list is mistaken for completion | Close individual obligations only with adequate source/execution evidence. A fresh reviewer checks the unchanged acceptance cases and remaining gaps. | A passing core suite does not establish Stop admission, real-producer progress or migration integrity. |

Keep one authoritative ontology home; a generated review packet references it rather than
creating competing rules. A shared map can itself omit something, so independent expansion
remains necessary. More prose, more time or a fixed loop count alone is not the proposed fix.
Use separate comparisons for retrieval mapping and consumer/counterexample enforcement if
we want to identify which component helps; the README's combined-procedure experiment
would measure only their combined effect. Compare one/two/three reviewers under equal total
effort before claiming an optimal review cost.

## 5. What P1/P2/P3 mean, and why work stopped

Each B trajectory had one continuing Sol fixer:

`P1 repair → fresh R1 review → same fixer P2 → fresh R2 review → same fixer P3 → fresh R3 review`.

P1 allowed 30 minutes; P2 and P3 each allowed 15. Each fresh reviewer had six minutes
blind, then four checking the findings ledger. The **repair loop used one fresh reviewer
per round**, not the three-reviewer discovery ensemble. Evaluator findings were kept out
of the loop so that review reliability could be measured. This was a bounded experiment,
not a promise to continue until every independently assessed obligation passed.

Both correction rounds made gains. B2 moved complete retry allocation into TaskService
in P2 and passed the changed-default launch check after P3. B1 improved cross-repository
progress identity and stopped mistaking a new task for a completed seed. B3 improved tool
validation/account selection and removed unrelated progress targets. Yet every final
candidate still failed the real-producer sync case; other spending, Stop, expiry and
migration failures remained as detailed in [P3-ASSESSMENT.md](P3-ASSESSMENT.md).

The aggregate “2 supported; 6 violated” describes **eight broad plan scopes, not eight
bugs or tests**. One remaining counterexample keeps a whole scope violated. B2's launch
fix therefore does not clear the broader dispatch scope while permission/accounting
violations remain. Later uniformly assessed source findings are included too. This
coarse table is useful for whether the full design is satisfied, but poor for measuring
incremental repair progress; the per-case tables must accompany it.

There were two stopping failures. Workers completed their passes below the time limits,
with narrow checks and unresolved coverage; the remaining available time was not used to
establish complete closure. At the final boundary, B1 and B3's reviewers still reported
open findings, and the experiment's two-correction cap prevented another repair. B2's
reviewer instead declared clean after 985 passing core tests and checking all 19 grouped
issues. Independent checks contradicted that decision. Hidden evaluator answers were
not feedback those workers knowingly ignored.

Consequently, neither “three passes” nor “repeat until a reviewer finds nothing” is a
reliability guarantee. A production workflow should continue on actionable failures or
explicitly report incomplete work at its resource limit; a clean stop also needs adequate
evidence for the governing obligations. Round-by-round evidence (`loop-summary.json`).
