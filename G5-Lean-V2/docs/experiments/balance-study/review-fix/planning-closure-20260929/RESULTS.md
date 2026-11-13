# Planning retrieval and one-pass repair of explicit gaps

This report preserves the study setup and findings. Per-run plans, prompts, probes,
logs, and evaluator records are omitted from the PR; paths in code font identify
the original records.

Run 2026-09-28 local / 2026-09-29 UTC. GPT-6 Sol/xhigh performed twelve planning runs
and two repairs in fresh contexts. GPT-6 Astra/xhigh prepared and independently evaluated
them. No prior experiment was rerun. No experimental application or ontology patch was
integrated into maintained code. The broader repair-loop comparison remains unrun.

## Planning: did better retrieval produce better plans?

The request was to save a named handoff from a chosen workspace version, with selected
conversations, for a teammate to use later even after the source changed or stopped.
Each planner received the same request and governing meaning, no application source or
tests, and no previous findings. Each had fifteen minutes for a proposed ontology diff,
an initial plan and ordinary final design review. This tested planning only.

The four conditions were current guidance (C), an extra retrieval checkpoint (A), direct
rule/exception links (B), and both (AB). B added 35 navigation links across six files;
it changed no rule meaning. Its author did not know the requests. A separate preparer
selected two requests and froze twelve relevant existing obligations per request before
the planners ran. Initial and final plans, harness variants, and inputs were retained
in the local study archive and are omitted from this PR.
[Protocol](PROTOCOL.md), navigation audit (`frozen/planning-evaluation/navigation-audit.md`).

An independent evaluator checked actual returned rule text and correct incorporation
separately. Case IDs concealed assignments, although public histories could reveal the
variant. A cited filename alone did not establish retrieval; an applicable canonical
rule could govern an operation without being copied verbatim into the plan.

| Condition | Rules visibly retrieved, each run /12 | Correctly incorporated, each final plan /12 | Initial → final supported, /36 | Mean active time | Mean output tokens |
|---|---|---|---|---:|---:|
| C: current | 12, 11, 11 | 10, 8, 10 | 26 → 28 | 9m 22s | 39,656 |
| A: retrieval checkpoint | 12, 12, 12 | 10, 9, 10 | 28 → 29 | 8m 37s | 36,772 |
| B: direct links | 11, 12, 12 | 11, 11, 9 | 28 → 31 | 8m 41s | 33,329 |
| AB: both | 11, 12, 11 | 9, 9, 9 | 26 → 27 | 7m 55s | 33,649 |

Here `/12` means twelve specified existing obligations in one design, not twelve tests
or twelve independent tasks. `/36` sums those obligations over three plans. These are
selective design-alignment measurements, not overall quality ratings or success-rate
estimates. Active time starts at the first tool action and includes reading, editing,
self-review and reporting. Worker-session totals below include the few seconds before it.

The final unsupported obligations divide as follows:

| Condition | Omitted | Contradicted | Uncertain |
|---|---:|---:|---:|
| C | 1 | 5 | 2 |
| A | 2 | 3 | 2 |
| B | 1 | 3 | 1 |
| AB | 3 | 3 | 3 |

The result is more specific than “retrieval did not work.” A retrieved all 36 assessed
rule instances but incorporated 29 correctly. B had the best final count at lower observed
time/output cost, but one final self-review introduced an incorrect restriction: a viewer
could not list handoffs even when authorized to read the source. Creation eligibility
had been applied to reading. Similar errors also occurred in C, A and AB; this is not
evidence that links caused the error.

Five plans deliberately proposed separate handoff grants instead of preserving current
source-read authority. The frozen evaluator counts the resulting read/publication policy
changes as contradictions. These are **explicit policy amendments**, not evidence that
the planner never found the rules; a human could consider them as product alternatives.
The preservation metric is narrower than overall design merit. Other residuals concern
exact source-collision rules, fresh receipt identity and public versus private native
conversation material. There was no qualifying false-completeness claim.

Across all plans, 139 of 144 rule instances were visibly retrieved, two partly and three
not visibly. All five incomplete retrievals concerned the native-private block. Initial
support rose from 108 to 115 after ordinary final review, but review also introduced
errors. This is a retrieval/application distinction, not a measured causal explanation
of the models' reasoning. Full judgments and quotes (`deliveries/pe1/assessment/artifacts/planning-assessment.md`),
machine-readable comparison (`planning-results-1.json`).

We stopped after these twelve runs. B passes the numerical gain/cost thresholds, but
retains a serious false restriction. “No new serious false restriction” did not predefine
whether “new” meant new to the design or absent from control. A control-relative reading
would advance B; a final-safety or initial-to-final regression reading stops it. We use the
stricter guard and disclose the ambiguity rather than claim preregistration uniquely
dictates stopping. [SCREEN-AUDIT.md](SCREEN-AUDIT.md) preserves both interpretations.
Two of B's three extra supported records also reflect fewer deliberate policy amendments;
they should not be described as two recovered retrieval misses. No labels were changed.
No variant has been adopted into the planning skill.
One request and three repetitions per condition cannot establish a reliable improvement;
concurrent host load also limits interpretation of time differences.

## Repair: does supplying the whole gap table achieve eight supported scopes?

Both workers started from the exact same preserved B2/P3 implementation, which had already
undergone three review/repair stages. Both received the original approved plan, governing
ontology, and the full eight-scope assessment plus concrete G1–G8 diagnoses, including
inherited producers. This deliberately tests repair **given diagnosis**, not discovery.
Exact packet (`frozen/GAPS.md`).

Each worker got one uninterrupted attempt, up to sixty minutes, including its own tests
and debugging. Findings were saved before edits. There were no further reviewer messages,
fixer turns or evaluator-guided corrections. The two attempts used 38m 18s and 42m 28s of
worker time. Independent evaluation then examined actual producers and consumers as well
as the original local checks. Neither attempt closed the full intent.

The most informative remaining failure is shared. Both workers repaired accounting storage
and budget admission for explicitly unknown usage. But the actual Claude event mapper
converted a missing token count into zero **before** that writer. We passed its output
through the actual lifecycle writer and reservation operation: the database marked the
usage known, and another task was admitted while the required token total was unknown. This
is an executed local producer-to-consumer counterexample, not an assumption about how
often a live provider returns a partial frame.

Both repaired actual mixed-prerequisite settlement in either order, explicit Stop versus
future-warning admission, complete ordinary profile/bootstrap allocation, the actual
ordinary sync-producer progress path, and the missing migration checksum. Useful repairs
therefore occurred even though broad alignment remained incomplete.

Candidate-specific findings were independently reconciled against both implementations.
The original independent assessments remain intact:
C1 (`deliveries/e1/source/artifacts/source-assessment.md`),
C2 (`deliveries/e2/source/artifacts/source-assessment.md`).
The [reconciliation](reconciliation/REPORT.md) records new-case applicability and any label
correction; findings from it were never sent to the fixers.

The pre-frozen independent backend set passed 24/24 for C1 and 23/24 for C2. C2's failing
case was a retained synthetic multi-task sync subset; both passed the separately identified
actual ordinary sync producer. Core tests passed 985 and 986, adapter tests 26 and 27;
both passed the component recovery probe, frontend typecheck and migration checker.
These passes do not cancel the later independent counterexamples. No live provider,
guest VM or full browser journey was executed.

## What this changes about the next hypothesis

Explicit diagnoses enabled substantial repair but did not establish complete delivery.
The shared accounting miss is a precise candidate for the proposed repair experiment:
require each supplied gap to be checked from its actual upstream producer through the
authoritative consumer, including an allowed case and the failing boundary. For accounting,
that means provider event → normalized usage → lifecycle persistence → reservation,
with missing distinct from zero. Checking the writer with a hand-built unknown value
alone allowed both attempts to look correct locally.

This is a proposal, not a demonstrated remedy. A future controlled repair comparison
should preserve one-shot repair as the control and measure new regressions and unresolved
evidence as well as known-gap closure. The user requested that broader comparison remain
unrun here; no fix/review loop or production repair was added.

## Effort and evidence limits

Costs are native token receipts and summed worker time, including tools and reporting.
Output already includes reasoning; cached input is accounted for separately. Parallel
elapsed time is not the sum, and these are not dollar estimates. Root supervision and
evaluation are additional, not hidden in the Sol figures.

| Work | Summed worker time | Uncached input tokens | Output tokens |
|---|---:|---:|---:|
| Twelve planning runs | 104m 41s | 1,222,926 | 430,215 |
| Two repair attempts | 80m 46s | 872,113 | 198,623 |
| Navigation and request/evaluation preparation | 9m 12s | 147,713 | 27,372 |
| Initial independent assessments: three Astra workers | 28m 59s | 629,960 | 76,405 |
| Screening audit and uniform repair reconciliation: two Astra workers | 9m 42s | 131,426 | 24,053 |

The full input/cache split and root effort were recorded in the local
`effort-summary.json` and `root-effort.json`.
Root preparation, runtime evaluation, supervision and reporting added **581,309 uncached
input tokens and 110,361 output tokens** through 02:25:41 UTC. Its elapsed window overlaps
all worker work; final reporting after that receipt is excluded from this stated cutoff.
Public actions, initial findings, candidate patches, native receipts, input hashes, and
independent checks were retained in the local study archive. This PR tracks the protocol
and summaries. No private reasoning is exported.
The input/checkpoint audit passed 131 checks; the final model, budget, preservation, link
and shutdown audit passed 96. All private services are stopped. These checks establish
experiment integrity, not correctness of the repaired application.

## Final comparison: alignment with the original design and governing ontology

Planning support measures retention of twelve preselected existing constraints; it does
not establish implementation correctness. Repair support uses the original eight broad
plan scopes and relevant governing rules. A single defect may affect several scopes, so
the denominator is neither eight independent bugs nor an overall completion percentage.
Supported means source/local evidence supports the declared boundary; partial leaves
material applicability or evidence unresolved; violated has a concrete counterexample.

| Candidate | Original eight plan scopes |
|---|---|
| Shared starting B2/P3 | 2 supported; 6 violated |
| C1 after one explicit-gap repair | 5 supported; 1 partial; 2 violated |
| C2 after one explicit-gap repair | 4 supported; 1 partial; 3 violated |

For C1, the shared accounting defect keeps I6 and I7 violated; attachment conditions keep
I1 partial. For C2, accounting violates I6/I7, the new attachment failure violates I1,
and uncertain applicability of the failed retained progress fixture leaves I8 partial.
The final eight-scope comparison and reconciled residuals are included in the main
[review-fix report](../README.md#final-comparison-alignment-with-the-agreed-intent).
The old thirteen-commitment subset is preserved for comparability but does **not** contain
the separate unknown-usage rule. Passing that subset cannot establish complete ontology
alignment while the shared accounting counterexample remains.
