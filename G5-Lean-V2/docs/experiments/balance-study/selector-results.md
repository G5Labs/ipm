# Automatic selection versus answer-informed questions

The protocol (`selector-protocol.md`) was fixed before the selector ran. A fresh Sol/xhigh
agent saw the same source, ontology and accepted intent as R-O, with no prior reports or
known-defect list. It chose at most two questions. They were passed verbatim to another
fresh Sol/xhigh reviewer; original input hashes otherwise matched R-O, except the assigned
path and an added instruction paragraph. This is one adaptive trial on a learned artifact.

## Observed selection and review

The selector chose cancellation after first-input allocation but before delivery, and
cross-action raw-key identity. See its questions (`reviews/s-o/questions.md`) and the
guided review (`reviews/r-s-o/review.md`).

| Outcome | Observation and adjudication |
|---|---|
| Known R1: Cancel stops unrelated ordinary work | Still missed. The allocation/handoff cancellation claim is a different issue. |
| Known R2: reconciliation rotation update is rolled back | Still missed. |
| Pending-fork recovery under different keys | Useful additional source finding: after composing an active fork under another key, request loss can leave no durable binding to that fork's eventual completed receipt. The coordinator can later fail on the published name. |
| Allocated but undelivered first input cannot be cancelled | Overstated. Compound Cancel refuses, but Open chat hydrates the session and exposes ordinary queued-input Withdraw; ordinary task cancellation also handles queued work. The exact compound-button handoff policy is a narrower contract/UX question. |
| Different raw-key action must allocate independent children | Unsupported mandatory restriction, as in the earlier adjudication. Shared namespace behavior is real; independent namespaces are not established merely because they are possible. |

The final [source adjudication](repair-and-selector-adjudication.md) distinguishes these
claims. Neither “a lead was followed” nor an extra P1 label counts as correctness. No
tests ran in this review. The new recovery claim remains source-supported evidence.

## Effort

| Work | Elapsed | Output tokens | Noncached input tokens |
|---|---:|---:|---:|
| Selector | 1m18s | 3,994 | 44,792 |
| Guided fresh reviewer | 2m49s | 10,445 | 92,087 |
| Combined pipeline | 4m06s | 14,439 | 136,879 |
| Original R-O broad native review | 4m45s | 14,453 | 144,516 |

The extra stage did not increase total observed worker cost in this single trial, because
the downstream review was shorter. It also did not improve detection of the two known
targets and produced unsupported restrictions. This cannot establish either a general
cost advantage or a causal reduction in coverage; different findings and stochastic
variation remain confounded. The answer-informed follow-ups used existing contexts and
are not a fair cost/control pair for this fresh pipeline.

## Decision

Do **not** add a mandatory generic risk-selector stage on this evidence. The earlier
directed successes showed available reasoning ability, not automatic selection ability.
This trial directly exposes that gap: a short selector can choose a nearby but disputed
boundary, and its wording can anchor the next reviewer toward an unjustified requirement.
The guided request explicitly called the questions leads rather than established findings
or new requirements; that warning did not prevent the overstatement. The exact cognitive
cause is unknown, but another generic caution alone is not a demonstrated remedy.

A selector remains a research option for genuinely unseen changes, with independently
adjudicated permitted cases and repeated starts. The current practical recommendation
is native review plus human/evaluator selection of the few consequential acceptance
questions when high confidence matters. That selection is a real supervision cost; it
must not be credited to autonomous ontology performance.
