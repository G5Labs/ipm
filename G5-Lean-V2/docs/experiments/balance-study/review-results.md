# Review comparisons and their limits

All initial reviews were fresh Sol/xhigh, inspection-only contexts. Reports were frozen
before author explanations or evaluator feedback. Claims were checked against actual
source and governing contracts by Astra; see the full
[claim-by-claim adjudication](review-adjudication.md) and verbatim reports (`reviews/`).

## Same Experiment 7 artifact, four review treatments

| Treatment | Finds known unrelated-work Cancel defect before a hint | Finds known rolled-back reconciliation rotation | Imposes false chat TTL or categorical missing-progress blocker | Additional supported source findings |
|---|---|---|---|---|
| R-N: native, source/docs | No | No | No | Confirmation-before-Start gap, late Ready expiry path, pending-fork uniqueness/error-handling composition |
| R-O: ontology + native | No | No | No | Confirmation gap, late Ready expiry path, Stop followed by stale compound Start |
| R-H: current review skill + ontology | No | No | No | Late Ready expiry path, Cancel race before durable fork admission |
| R-P: prior review skill + ontology | No | No | No | Confirmation gap, accepted launch drifting at dispatch |

These additional findings are not false simply because the original evaluator omitted
them. Conversely, every review claim is not automatically valid. N/P overstate the need
for separate raw-key namespaces. P's still-unrecorded timely Ready case depends on an
unspecified event-arrival rule. Confirmation must be established before compound compute
admission, but requiring no preceding fork effect or destruction of a usable child would
exceed the partial-failure contract.

H/P have the same application, intent and ontology bytes; only the review skill version
differs. This is a much cleaner revision comparison than two fresh end-to-end feature
runs, but it remains one pair. It demonstrates neither improved detection of the two
known targets nor a regression attributable to the current skill. Different valid
findings in both arms forbid reducing the result to “both reviews had zero value.”

## Directed follow-up in two existing review contexts

After initial reports were saved, N and H each received two concrete questions: follow
compound Cancel's effects on unrelated chat; follow the rotation write to its transaction
outcome. Neither was given a faulty line, patch or expected answer. **Both then found
both defects**, with correct source paths. Incremental output was about 2,866 tokens for
N and 4,411 for H, versus 13,030 and 15,859 in their initial broad reviews.

The ability to trace these defects exists. Directing attention helped. Selection of
these questions used known answers, however: it does not validate a generic automatic
risk selector. Adding more abstract instructions to the current skill is not justified
by a hinted success. The useful next research target is whether a brief, independently
generated risk question can select an equally valuable boundary on unseen changes.

## Same new I-O implementation, two native review treatments

| Finding | R-I-N: source/docs/plan | R-I-O: plus ontology |
|---|---|---|
| UI receipt recovery blocked by stopped compute/fresh preview | Found | Found |
| Accepted launch drifts at dispatch | Found | Found |
| Sync progress context lost | Found | Not reported |
| Copied job concurrency cap ignored | Not reported | Found |
| TaskService initial-input ownership contract missed | Not reported | Found |

This is a useful example of ontology helping a review enforce an architectural promise
without a full review-skill mandate. The same promise was also in the shared plan, so
the comparison cannot separate extra salience, ontology structure and sampling variation.
Complementary findings support combining independent review with executable checks;
they do not establish that either treatment covers the entire defect space.

R-I-O also overstates missing test coverage: the supplied large integration test already
contains nonterminal-source, mixed-prerequisite, missing-input and changed-request refusal
cases. Its lack of execution evidence is legitimate under an inspection-only assignment;
calling the test merely happy-path is inaccurate. Reviewer summaries need verification
too. Successful historical command prerequisites remain permitted when no command is
newly retried; the review's question does not establish a defect.

## Measured effort

| Arm | Elapsed | Output tokens | Noncached input tokens |
|---|---:|---:|---:|
| R-N | 4m13s | 13,030 | 122,115 |
| R-O | 4m45s | 14,453 | 144,516 |
| R-H | 4m42s | 15,859 | 129,873 |
| R-P | 4m32s | 14,291 | 111,438 |
| R-I-N | 4m22s | 15,455 | 131,664 |
| R-I-O | 3m45s | 12,835 | 101,022 |

These are task-start/end telemetry, not controlled compute or price comparisons.
No initial review compacted. The full metadata table is telemetry.json (`telemetry.json`).
Inspection-only reviews do not incur all maintained skill obligations, including fixing,
running checks and updating ontology metadata; these timings do not measure those costs.
