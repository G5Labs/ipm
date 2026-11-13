# Discovery result: specialization changed the misses, not consistency

Completed 2026-09-28. Eighteen fresh GPT-6 Sol/xhigh subagents reviewed the same
untouched implementation, original approved plan, sanitized governing ontology,
revised review skill and generic rubric. Each had ten minutes. Three general
reviewers ran in parallel per G team; three reviewers with intent, behavior and
architecture priorities ran in parallel per F team. Main Astra consolidated both
arms with the same procedure. It knew the historical cases, so consolidation was
not blind. No historical answer was added to a worker packet.

## Observed discovery

Each entry below counts teams that explicitly identified the named gap. These are
observations from three teams per procedure on one feature, not quality grades,
repair results or estimated success probabilities.

| Gap in the unchanged implementation | General teams | Focused teams |
|---|---:|---:|
| Accepted launch changes with later defaults | 2 of 3 | 3 of 3 |
| Future shutdown warning incorrectly blocks new work | 2 of 3 | 1 of 3 |
| Agent dispatch ignores the job cap | 3 of 3 | 2 of 3 |
| Initial-input allocation bypasses TaskService ownership | 3 of 3 | 3 of 3 |
| Saved recovery requires a fresh preview | 3 of 3 | 3 of 3 |
| Stopped workspace hides the recovery panel | 3 of 3 | 1 of 3 |
| Synchronization progress loses context | 2 of 3 | 3 of 3 |

The individual team totals were G: **7, 5, 6** and F: **5, 5, 6**. The equal totals
in pairs 2 and 3 concealed different misses. F2 recovered launch and warning issues
missed by G2, but lost the cap and parent UI issues. F3 recovered progress missed
by G3, but lost the warning issue. K7 denotes discovery of the progress-context
gap; it does not endorse copying the entire original scope as a correct repair.

Every team's union exceeded its best individual's known-gap coverage. Several
important findings came from only one reviewer, including warning exceptions,
parent visibility and progress. Majority voting would have discarded useful
findings. All **105 raw claim IDs** survived consolidation into **19 causal groups**;
raw reports, conflicting details and uncertain judgments remain preserved.

## Additional coverage and false-claim risks

Across repetitions, general reviews reported 18 distinct groups and focused
reviews 15. Four groups appeared only in G: missing spend admission, a lock-upgrade
deadlock, broken core unit-test compilation, and uncertain tenant-reference
hardening. F alone reported missing fresh explicit permission choice. Both found
such issues as receipt-read authorization, task-bound source input after a
follow-up, successful jobless prerequisites, incompatible provider/tool or permission
settings, and unbound inputs in ordinary workflow producers.

These totals are reported claims, not 19 proven defects. Main consolidation found
source support for 17 groups and retained two unresolved groups: whether the new
schema needs additional composite tenant constraints, and whether historical
prerequisites must appear as graph nodes rather than the existing labelled detail
view. Those concerns were not silently turned into mandatory redesigns.

Some supported reports also contained questionable remedy details: blanket refusal
after a default change does not establish correct accepted-launch behavior; refusal
after genuine account revocation can be required. The original wording and the
consolidator's qualification remain in the packet. Fixers must validate both.

Independent local API/DB/reservation probes subsequently reproduced four newly
measured baseline failures: jobless-success reuse returned 500; an intact bound
input after a later same-session task returned 409; a zero output-token budget
still admitted work; and a catalog-accepted permission policy was accepted into
a graph but could not dispatch. Only these new cases ran on the baseline:
the 13 previously established outcomes were filtered out. This evidence was
not supplied to reviewers or fixers.

## What explains the observed inconsistency?

The visible read traces distinguish two stages without claiming access to internal
reasoning. Only three of eighteen reviewers visibly retrieved the exact future-
warning exception, and all three reported its violation. The three teams missing
that issue had no reviewer visibly retrieve the passage. One focused reviewer (f33) opened the correct lifecycle file but read ownership/deletion
sections135–163, omitting the warning body87–90. The three finders explicitly read that
warning section. This is an observed coverage/navigation gap, not evidence of a wrong
ontology version. It supports testing governing-exception retrieval, but does not prove
that retrieval alone causes success. Read routes (`retrieval-routes.json`).

All eighteen retrieved the accepted-launch plan passage, yet only ten reported its
violation. All three G2 reviewers missed it despite retrieval. A rule-to-dispatch
trace or concrete changed-default case is therefore a different intervention from
more document retrieval. Parent UI and progress misses similarly require tracing
actual consumers beyond a component or producer. Assigned specialties did not
reliably ensure those traces.

This experiment cannot isolate the rubric's benefit: both arms received it and the
same revised skill. It also cannot measure a change in code alignment, because
discovery was read-only. The original nine skill-comparison runs were not repeated.

## Effort and decision

| Procedure | Summed reviewer time | Median per reviewer | Output tokens |
|---|---:|---:|---:|
| General | 69m32s | 7m42s | 233,128 |
| Focused | 71m22s | 7m56s | 238,878 |

The eighteen reviews used 140m54s and 472,006 output tokens. Output includes reasoning
tokens; these figures are not prices. Astra consolidation took approximately
2m32s across G and 2m52s across F, measured separately. Preparation, independent
evaluation and reporting are additional Astra effort. Three reviewers ran together;
the six teams ran in the frozen order G1, F1, F2, G2, G3, F3. No reviewer exceeded
its allowance. Shared host load and actual tokens were measured rather than assumed
equal. Effort summary (`effort-summary.json`); raw receipts and consolidation
timestamps are retained locally.

**Do not adopt mandatory specialization from this pilot.** It recovered G-missed
issues in two pairs but repeatedly lost other important coverage, failing the
predeclared adoption rule. General review also remained inconsistent. Use general
review as the simpler control for the repair-loop experiment, retaining minority
findings and the shared rubric. A later coverage intervention should distinguish
retrieval of exceptions from verification of their actual consumers.

Next, the mechanical union of all six new reports goes unchanged to three fresh
fixers. This separates repair variation from discovery variation. Evaluate their
first patches before fresh general reviews and bounded corrections; keep hidden
evaluation out of feedback and preserve any false-clean stopping result.

[Protocol](PROTOCOL.md) · per-reviewer diagnoses (`diagnosis-map.json`) ·
visible retrieval (`retrieval-observations.json`) · team comparison (`discovery-summary.json`) ·
all claims in local `repair-packet/FINDINGS.json`
