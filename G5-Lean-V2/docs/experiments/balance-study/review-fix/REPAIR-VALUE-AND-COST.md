# Did explicit gap closure materially improve the implementation?

2026-09-29. Retrospective comparison of all nine B/P checkpoints and the two C repairs.
No experiment, repair or application test was rerun for this analysis. It combines the
preserved independent execution/source evidence with targeted source inspection. The
original labels and counterexamples remain unchanged.

**Yes: C1 and C2 repair important behavior that remained broken throughout the B/P runs.
C1 is the strongest overall delivery in the measured set. C2 is more complete than its
B2/P3 starting point, but introduces a serious queue-availability regression; it is not
an improvement on every dimension. Neither is ready to declare complete.**

## The original goal and design

The user-facing goal was to **retry a failed workflow branch without rerunning successful
work or damaging the original history**. An owner edits the failed root instruction,
previews what will be retried, and creates a fresh linked run. A lost response or browser
reload must recover that same submission rather than duplicate it.

The [complete original approved plan](consistency-20260928/APPROVED-PLAN.md) required:

1. Select the failed agent root and its dependency-blocked descendants from authoritative
   edges/outcomes. Reuse outside successful prerequisites; refuse an outside failed one.
2. Preserve effective descendant instructions and required material. Allocate fresh task,
   session, input and attempt authority; retain the original graph and history.
3. Let JobService own the retry transaction and graph; TaskService own complete task/input
   allocation, composing ChatService for sessions. Reuse existing dependency settlement.
4. Freeze the accepted launch, while current owner, account, permission, live-state,
   spending and capacity rules still govern. No implicit Start or historical authority.
5. Commit one replay identity with one graph; changed input conflicts, failed allocation
   rolls back, and current read authority permits historical receipt recovery after Stop.
6. Use the existing UI with confirmation, durable reload recovery, lineage and truthful
   progress counting only new work. Disclose that retry uses the current shared checkout.

The eight scored scopes group these requirements and their governing dependencies. They
are not eight equally sized features. The broad I6/I7 rows share the accounting defect;
I1/I4 shared the mixed-prerequisite defect. **“2 supported; 6 violated” did not mean only
25% of useful behavior worked.** B1 actually had 2 supported, 1 partial and 5 violated at
all three checkpoints. The happy-path graph, fresh identities and many admission/replay
checks already worked in the B candidates.

## Were the closed gaps important?

| Gap | Consequence before C | Change in C1/C2 | Materiality |
|---|---|---|---|
| Explicit Stop still accepts retry while VM reports running | Work can be allocated after the owner has withdrawn fresh-execution authority. B2/B3 regress this; B1 already preserves it. | Both C candidates pass explicit Stop, future-warning and due-warning checks. | **High:** lifecycle/authority correctness. |
| Mixed prerequisite order drops a blocked descendant | Merely changing which prerequisite fails first can bypass the promised outside-failure refusal. All B/P stages retain it. | Both refuse correctly in both actual local TaskService settlement orders. | **High:** selected work and dependency correctness. |
| Copied attachments never reach invocation | The retry can have correct database references but execute without required input material. All B/P stages retain missing transport. | Both add authenticated fetch and adapter materialization; residual conditions differ below. | **High, partially repaired:** faithful task input. |
| Incomplete permission/tool mapping | An adopted deny choice is missing, and tool availability must not imply automatic permission approval. | Both repair backend/native-argument mapping and preflight; C1 also updates the generated catalog. Real pinned CLI behavior remains unexecuted. | **High:** permission semantics and usable supported policies, not a demonstrated exploit claim. |
| Unknown usage erased; accounting not ordered with admission | Budget decisions can treat incomplete consumption as known. | Both repair writer flags/shared ordering, but an earlier mapper still turns missing tokens into zero. | **High, incomplete:** the important end-to-end failure remains. |
| Ordinary profile/bootstrap allocation bypasses the complete owner | More than one module implements the session/task/input operation, weakening consistency of validation and future changes. | Both move those producers through TaskService composing ChatService in the caller transaction. | **Material architecture repair:** no independently demonstrated old partial-commit corruption is claimed. |
| Ordinary sync producer loses progress context | The normal producer, unlike the artificial fixture, has no selected file-path metadata; retry loses truthful direction/scope. | Both pass the actual ordinary producer-to-retry-to-progress check. | **Material user-visible correctness:** operational status cannot be trusted before repair. |
| Missing migration checksum | Required repository validation fails. | Both pass the catalog check. | **Small fix, real delivery blocker:** not equal in weight to the authority/accounting defects. |

This is real progress, not merely a table becoming greener. However, the first two C repairs
did not improve every already-working B variant: B1's Stop handling was already correct,
and B2's known failed-task spending check already passed before C. Preserve those credits.

## Quality at every saved checkpoint

The rows distinguish behavior, ownership, compatibility and verification. Incremental
effort is that repair pass alone; P2/P3 build on earlier passes and consume separate review
feedback. All B stages retain the shared attachment/mixed-prerequisite/ordinary-producer
ownership/full-policy gaps described above unless explicitly stated otherwise.

| Delivery | Quality judgment against the goal/design | Incremental repair time / output tokens |
|---|---|---:|
| B1/P1 | Substantial functional repair: launch preservation, complete retry owner, Stop gates, caps and replay. Still misses ordinary sync progress and failed-task spend; introduces a source-supported reload recovery dead end. | 22m 02s / 65,862 |
| B1/P2 | Better multi-repository attribution. It does not close the ordinary producer shape or spending failure; broad completion is unchanged. | 11m 19s / 32,663 |
| B1/P3 | Fixes fresh tasks hidden by completed-seed identity. Useful projection correction, but ordinary sync, spending and recovery limitations remain. | 8m 50s / 27,008 |
| B2/P1 | Known failed-task accounting is better than B1/B3, but accepted defaults are wrongly revalidated/refused; session composition remains outside the complete retry owner. Introduces Stop regression and misses migration catalog. | 25m 21s / 68,481 |
| B2/P2 | Correctly moves retry session composition inside TaskService. Real architecture gain; launch refusal, Stop, ordinary progress and migration failures remain. | 5m 44s / 16,804 |
| B2/P3 | Repairs accepted-launch drift. A materially better candidate than B2/P1, despite the same broad count. Final reviewer falsely calls it clean; Stop, ordinary sync and migration still fail. | 6m 17s / 22,958 |
| B3/P1 | Good initial launch/owner/cap/replay work, but Stop and expired-warning regressions, failed-task spending and ordinary progress remain. | 23m 07s / 69,769 |
| B3/P2 | Tightens structural/tool preflight and current-owner account selection. This improves consistency without proving full supported permission breadth; principal executed failures remain. | 11m 43s / 33,789 |
| B3/P3 | Removes unrelated target/seed projection. Still fails ordinary sync and admission/accounting cases; final review also exposes a component recovery/reset failure. | 6m 19s / 20,360 |
| C1 | Strongest measured overall: closes mixed-cause refusal, Stop, ordinary producer ownership/progress and local policy/catalog integration. Still has an executed upstream accounting failure and unresolved guest attachment conditions. | 38m 18s / 98,940 |
| C2 | Similar major functional/ownership gains, but less convincing integration: accounting failure, new cross-workspace queue blockage, valid-filename materialization failure, catalog drift and conditional subset-progress regression. | 42m 28s / 99,683 |

All B candidates passed many local/core checks. That evidence supports their functioning
subsystems but did not justify overall completion. The B2 false-clean result particularly
shows that a large passing unit suite and a disposition for every supplied finding are
insufficient when the relevant boundary was never checked.

At the code-structure level, C1/C2 improve ownership by moving complete operations into
TaskService while retaining the caller's transaction. Neither creates a second scheduler
or resets source execution. Their allocation APIs differ: C1 uses a general allocator with
many positional/optional arguments and a kind discriminator; C2 uses a parameter object
for ordinary agents and a separate command allocator. C2's call shape is easier to inspect,
but that local maintainability advantage does not compensate for its behavioral regressions.
This is a targeted structural judgment, not a repository-wide style or complexity audit.

There is also retained breadth uncertainty: **C1/C2 inherit B2's categorical refusal of
elevated non-root descendants** (`retry.rs:641–647` approximately). B1 has an explicit
descendant-confirmation choice; B3 has a matching-policy path. The old common adjudication
kept the full heterogeneous-branch requirement unresolved. Neither C should be described
as proven superior to every B in every supported workflow variant. This is retained
conditional product-breadth debt, not a newly executed C regression or a changed score.

## Regressions and incomplete repairs in C

- **C2 queue blocking is a confirmed new regression.** An accounting-unavailable workspace
  aborts selection before the dispatcher considers another eligible workspace. The actual
  writer/reservation probe fails C2 and passes C1. Its blast radius is wider than one retry.
- **C2 loses a previously passing selected-subset progress fixture.** B2/P3's saved log
  passes `known_sync_progress_consumer_shows_only_the_retried_target`; C2 fails it. Its exact
  retained multi-agent shape has unproved current producer reachability, while the ordinary
  producer now passes. This is a real fixture regression with conditional product impact.
- **C2's filename and catalog failures are introduced integration defects.** A valid
  255-byte display filename becomes a 257-byte physical filename; backend/frontend catalog
  values diverge. The attachment case is a defect in newly added transport, not proof that
  B2 had a working provider attachment path which C2 broke. The catalog check proves drift,
  not a tested live dropdown failure.
- **C1 has no newly demonstrated unconditional loss of an earlier passing product case
  in the evaluated set.** That is a bounded statement, not a regression-free certification.
  Its `/mnt/g5` preparation and pre-existing-file permissions remain conditional concerns.
  C2 uses writable fresh files; its enclosing production HOME provides the inspected privacy
  boundary. No real guest access/confinement or provider-file consumption was executed.
- **Both retain the material accounting failure.** Missing tokens become zero in the
  actual provider event mapper, so the improved writer cannot distinguish the unknown.
  This is an inherited gap that the repair failed to close, not a defect introduced by C.

[Independent C assessments and uniform reconciliation](planning-closure-20260929/reconciliation/REPORT.md),
[B/P source and execution comparisons](consistency-20260928/P3-ASSESSMENT.md),
original common adjudication (`consistency-20260928/alignment-results.json`).

## What did the extra improvement cost?

An average first B repair took 23m 30s and 68,037 output tokens. C1/C2 took **63%/81% more
time and 45%/47% more output tokens** than that average. But they received a sixty-minute
cap rather than thirty, more precise diagnoses, a mature starting candidate and broader
explicit inherited-producer scope. This is not a controlled cost-effectiveness comparison.

| Trajectory | Repair time P1–P3 | Repair output tokens | Including all three intervening/final Sol reviews | Output tokens including those reviews |
|---|---:|---:|---:|---:|
| B1 | 42m 11s | 125,533 | 63m 28s | 201,536 |
| B2 | 37m 23s | 108,243 | 59m 10s | 183,756 |
| B3 | 41m 08s | 123,918 | 62m 55s | 203,080 |
| B2 path, then C1 | 75m 41s | 207,183 | 97m 28s | 282,696 |
| B2 path, then C2 | 79m 51s | 207,926 | 101m 38s | 283,439 |

C1 and C2 each add their own repair pass to the same preserved B2/P3. The two cumulative
rows describe alternative paths, not a claim that B2 was physically run twice. They exclude
initial feature implementation, initial discovery/consolidation and independent Astra
evaluation. Those costs are additional; the study does not identify the cost of obtaining
an equally good gap table in routine use.

For the latest C evaluation alone, the two fresh source assessors and uniform reconciliation
used about **26m 17s and 67,583 output tokens**, plus root runtime orchestration/reporting.
The root also supervised planning, so its **110,361 output-token cutoff is combined work**,
not a clean repair-only overhead estimate. The original Astra effort of finding and
adjudicating the gap packet is also not free. Cost must include diagnosis and verification,
not just the final coder pass. Input/cache receipts are preserved; output tokens are not
a billing estimate and already include reasoning.

## Recommendation for the review skill

The evidence justifies **explicit closure of confirmed material gaps before declaring
completion**, especially against the approved design and relevant reused producers.
It does not yet justify a mandatory additional forty-minute phase for every change,
or prove that an expensive Astra audit is always worth its cost.

For substantial changes, use the already adopted three independent general reviews →
main consolidation → coherent repair. Make consolidation hand the repairer a concise
goal-to-gap table: governing commitment, concrete failing/allowed case, affected origin
and consumer, required outcome and known evidence limit. Preserve minority findings.
The final report should reconcile the original goal, not merely mark the supplied bug
list done. This can improve the existing handoff without adding another routine review loop.

Much of this is **already required by the current skill**, including
`commitment → actual path → counterexample/check → result or gap`, unchanged producers,
and closure at the authoritative consumer. A second copy of those instructions is not
the missing feature. The practical change is for main consolidation to check design
coverage and the evidence behind closure, rather than merely combine reviewer reports;
reuse the existing artifact. The C experiment tests a stronger supplied diagnosis, not
proof that the present skill can reliably produce that diagnosis unaided.

One correction to the earlier proposal matters: the supplied G5 packet **already said
to exercise the actual usage producer and reservation, not just inserted rows**. Repeating
that sentence is not a new experimental intervention. A sharper future test would require
the reviewer to identify the precise originating producer and authoritative consumer,
then independently verify that the closure evidence actually traverses them. Here a
lifecycle writer is downstream of the provider normalizer; testing only the writer left
the earlier information loss untouched. The agent still chooses the relevant boundaries,
but those choices become inspectable rather than implicit.

I would make that consolidated design-gap handoff explicit when material findings exist,
using the skill's existing table rather than adding another ledger. I would test its
reliability/cost before mandating a separate full design audit or repeated fix/review
cycles. **No skill change or repair experiment is made by this
analysis.** Calculated effort and provenance (`planning-closure-20260929/retrospective-evidence.json`).

## Final alignment with original intent

| Checkpoint | Supported / partial / violated out of the same eight broad scopes |
|---|---|
| B1/P1, P2, P3 | 2 / 1 / 5 at each checkpoint |
| B2/P1, P2, P3 | 2 / 0 / 6 at each checkpoint |
| B3/P1, P2, P3 | 2 / 0 / 6 at each checkpoint |
| C1 | 5 / 1 / 2 |
| C2 | 4 / 1 / 3 |

The unchanged B counts hide narrower gains; the C gains correspond to material repairs.
C1/C2 are more complete against the agreed design, with C2's regressions explicitly
preventing a simple dominance claim. Every candidate remains incomplete at the declared
source/local boundary; real provider, VM and full browser behavior remains unverified.
