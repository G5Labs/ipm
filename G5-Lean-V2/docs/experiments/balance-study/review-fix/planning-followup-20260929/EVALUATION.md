# How to read this planning comparison

Evaluation complete: 27 cases, 297 paired obligation rows. Requirements were frozen
before dispatch. Original independent labels were retained; no supervisor relabelling
or extra evaluation run was used.

Each plan is assessed against a fixed set of relevant behavioral obligations: **11 for
host removal, 12 for Git capture and 10 for preview visibility**. The denominators include
both the new request and existing constraints the change must preserve. These are selected
design checks, not tests, overall quality scores or estimated success probabilities.

For each obligation, the evaluator reads the initial/final plan **and** the corresponding
proposed ontology changes. It records supported, omitted, contradicted, uncertain or an
unresolved explicit amendment. A concrete, request-authorized amendment counts supported;
changing a rule is not automatically a failure. Concrete scoped reuse can preserve an
unchanged operation without redesigning it. Missing application-code proof is not a
design failure in this source-free experiment.

Retrieval is separate. The operative source text must actually appear in a reading-tool
result; a filename, link, ID or worker claim is insufficient. An automated quote index
helps locate evidence but does not decide the result. A planner can retrieve a rule and
misapply it, or specify a correct design without visibly retrieving its canonical text.

Serious findings require a concrete actor, operation, condition and wrong outcome:
authority escalation, data loss, secret exposure or blocking an essential permitted
action. Omission alone is not automatically serious. Shared effects are counted once.

Three fresh Astra/xhigh evaluators each assess all nine cases in one scenario, using
masked case IDs and the same fifteen-minute allowance. They do not receive condition
mappings, planner costs, other evaluations or prior-study findings. Histories can expose
navigation/checkpoint differences, so this is masking rather than perfect blinding.
No evaluation feedback goes to planners. The supervisor checks evidence and arithmetic
and applies the [frozen adoption rules](../PLANNING-FOLLOWUP-PLAN.md).

Local raw records retain every per-obligation label and quote, native public histories,
snapshots, numerical effort receipts, masked mappings and input hashes. See
[continuation/evidence policy](HANDOFF.md) and [execution deviations](PROTOCOL-NOTES.md).

## Verified findings and interpretation

| Incomplete or uncertain final obligation | Instances | Source retrieval | What the evidence establishes |
|---|---:|---|---|
| Host deterministic repair/retry gate | 2 | Observed | Safe blocking and separate Force are specified; explicit retry after repair is not applied to the new recovery path. |
| Host cleanup proof scope | 1 | Observed | Destruction is offered as an alternative to purge, without defining whether all retained roots are covered. Uncertain, not a demonstrated unsafe deletion. |
| Capture drain/callback details | 9 | 8 observed, 1 partial | All leave callback lock release/revalidation unapplied; most also leave part of the stream-lifetime distinction unstated. No deadlock is specified. |
| Capture public diagnostic redaction | 7 | Observed | Transport destination/credential isolation does not settle secret-bearing public output. No leak is affirmatively specified. |
| Capture deterministic failure handling | 2 | Observed | Fingerprint/notice/repaired explicit retry is incompletely applied; recoverable work is retained. |
| Capture checked-child publication | 3 | Observed | Reader/owner source choice is specified but some publication/identity/stopped-child effects remain implicit. No broken child is demonstrated. |
| Preview shared metadata redaction | 2 | Observed | Keeping live logs/relay access separate does not completely specify safe new metadata fields. |
| Preview passive polling and idle activity | 1 | Partial | No Start or VM probe is specified, but whether polling counts as owner activity remains unresolved. |

The 27 instances above are **26 omissions and one uncertainty**, not 27 bugs. There are
zero contradicted labels and zero demonstrated serious findings under the frozen rule.
The nine-capture callback and three-child-publication judgments are especially sensitive
to how much an unchanged owner boundary must explicitly preserve in a plan.

The supervisor read the reports and material evidence, examined scoped-reuse examples
and affected plan/diff passages, and independently checked all **618 quoted evidence
fragments** against their designated artifacts. Two initially unmatched fragments were
checker path-parsing limitations: their short second path referred to the same-stage
plan, where the exact quote was verified. Representative retrieval claims were checked
against originating read calls and actual canonical text in public tool output. All
2,340 evaluator input files retained their pre-evaluation hashes. Native receipts confirm
fresh Astra/xhigh and completion within the common fifteen-minute allowance. The evaluator
access audit covered 81 call envelopes (103 execution commands) with no flagged outside
access; root model metadata also confirms Astra supervision.

Independent evaluation uses concrete scoped reuse for untouched restore/materialization,
preview commands/streams and lifecycle owners. It still labels a relevant detail omitted
when the changed path does not concretely retain that effect. We retain those frozen
judgments rather than silently giving credit for unspecified behavior. However, these
coverage omissions must not be described as demonstrated defects or automatic demands to
rewrite unaffected code.

As a **post-evaluation sensitivity calculation only**, crediting C07 and C09 as preserved
in every capture plan changes capture totals from C27/A30/B30 to C32/A33/B34. A's advantage
then falls to one instance and B's to two. Neither passes the full adoption rule; no stored
label or denominator was changed. Conversely, establishing that A's ambiguous host-
destruction proof really covers every retained root would remove its frozen host-count
loss, but that fact is absent from the source-free packet. This dependence on scope and
proof interpretation is part of the uncertainty, not a reason to assume a favorable answer.

The ordinary final review adds two host supports (initial ambiguities clarified), twelve
capture supports and seven preview supports. The new-request counts improve by one;
twenty gains preserve governing behavior. There are no lost supported rows. Many useful
clarifications do not change a frozen row and receive no extra numeric credit.

## Limitations and saved evidence

One independent evaluator assessed each entire scenario; there is no full second-rater
agreement study. Root supervision checked evidence and scope-sensitive claims but did
not duplicate all three assessments. The automatic retrieval candidate index had false
negatives from nested/escaped output and line prefixes; evaluators inspected actual
source deliveries instead. A partial source set can still provide enough overlapping
meaning for a supported design.

`evaluation/{host,capture,preview}/results.json` retains all initial/final dispositions,
source evidence, amendment annotations and limits. Each directory also preserves the
original report, native numerical receipts/public transcript and all evaluator-generated
artifacts. `results-summary.json`, `supervision-quote-check.json`, input hashes and
`summarize_results.py` retain arithmetic and verification. These raw files are local and
ignored by Git; the reports explain the evidence without requiring them in a PR.
