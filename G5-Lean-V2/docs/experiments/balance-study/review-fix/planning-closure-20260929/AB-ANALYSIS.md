# Why A+B had a lower count and lower average time

Retrospective analysis, 2026-09-29. No new runs or regrading. The original judgments,
public action histories and token receipts remain unchanged.

**A+B was lower on the twelve-constraint measure; a general negative interaction is not
established. It was faster on average, but used slightly more output tokens than B.**

## The count difference comes from two particular concerns

Each cell below counts how many of three final plans correctly incorporated that rule.

| Governing concern | Current | A | B | A+B |
|---|---:|---:|---:|---:|
| Exact branch-collision policy | 1 | 2 | 3 | 1 |
| Public display versus private native request/answer semantics | 2 | 2 | 2 | 0 |
| Fresh child receipt identity | 3 | 1 | 2 | 2 |

B and A+B are identical on the other ten obligation counts. Thus B's **31 versus 27**
is entirely two extra collision-policy supports and two extra native-privacy supports.
A's **29 versus 27** comes from two extra privacy supports and one collision support,
offset by A+B retaining fresh receipt identity in one more plan.

The three A+B plans all end at 9/12, but for different reasons:

| Run | Remaining issues | What the record establishes |
|---|---|---|
| AB1 | Viewer incorrectly barred from reading; collision specifics unresolved; native privacy omitted | Collision/read text was retrieved. Exact native-private body was not visibly retrieved. Review concentrated on saved-source variants, changing member lists, placeholders, Delete/publication races and retention. |
| AB2 | Separate-grant policy changes two frozen access obligations; native privacy omitted | All twelve rule bodies were retrieved. The separate grant is an explicit policy amendment. The final design excludes private requests/bundles but does not settle exact private accepted-answer semantics versus sanitized public history. |
| AB3 | Collision specifics and fresh receipt identity unresolved; native privacy omitted | Collision/identity text was retrieved; native-private retrieval was partial. Review added late-delivery, attachments, current connections, governance separation and safe cleanup, but did not close those remaining distinctions. |

The native rule is specifically about **exact request semantics and accepted answers
remaining private, with sanitized public display**. Excluding credentials, provider
bundles or live pending questions alone does not specify that historical-copy boundary.
Likewise, “use checked fork” alone did not explicitly delegate the exact/slash-prefix,
all-member namespace and equal-OID collision rule under the frozen rubric. These are
particular specification gaps; no data leak or branch failure was executed from these plans.

The extra checkpoint was not simply skipped. Post-initial-snapshot tool calls visibly
revisited other contracts in all three A+B runs. For example AB2 revisited fork access,
identity/proof, retention, tenant/cache/lifecycle, attachments and repository access at
01:44:40–01:46:00 UTC. Its final review incorporated several resulting changes. AB1 and
AB3 also retrieved material through new document/lifecycle/governance routes.

That activity was useful, but **a second route does not specify which unresolved concern
must be revisited**. The observed review routes broadened some areas without completing
the two distinctions above. This supports an attention/coverage hypothesis; it does not
prove that A distracted the model from B's links, or that the combination is inherently
worse. Public evidence cannot tell us that counterfactual or the worker's private reasoning.

Nor is 9→9 proof the review did nothing: several reported improvements fall outside the
twelve frozen obligations or strengthen an already-supported row. The selective score
can miss those gains. It is wrong to translate this result into “A+B produced the worst
design overall.” Five plans across conditions also intentionally changed access policy;
their preservation penalties are not automatic evidence of bad design or failed retrieval.

## The apparent speed advantage is fragile

| Condition | Mean active time | Mean output tokens |
|---|---:|---:|
| Current | 9m 22s | 39,656 |
| A | 8m 37s | 36,772 |
| B | 8m 41s | 33,329 |
| A+B | 7m 55s | 33,649 |

A+B uses **320 more output tokens per run than B**, about 1%. Its uncached input is also
higher: 104,168 versus B's 88,368 on average. So it is not uniformly cheaper than B.

| A+B run | Active time | Output tokens | Top-level tool invocations | Final plan characters |
|---|---:|---:|---:|---:|
| AB1 | 9m 43s | 38,203 | 57 | 8,079 |
| AB2 | 6m 04s | 27,162 | 27 | 5,791 |
| AB3 | 7m 58s | 35,582 | 31 | 7,197 |

AB2 delivered a shorter plan with fewer tool invocations and a shorter post-draft interval,
about 3m 23s versus AB1's 5m 37s and AB3's 4m 12s. Tool invocations may batch several reads;
they are descriptive activity, not an independent measure of useful work. AB2 nevertheless
retrieved all twelve rules, so its result is not explained by refusing to investigate.

As a **sensitivity check only**, excluding AB2 makes the other two average **8m 51s and
36,893 output tokens**—slower than the observed A and B averages. No run is removed from
the published results. This shows how one short run can drive a three-run mean; it does
not justify excluding the run or prove a population timing difference.

A was an instruction checkpoint inside the same budget, not a fixed additional timed
pass. A worker could incorporate it into its ordinary review and still finish earlier.
No planner exhausted fifteen minutes. Random run variation, different design choices,
batching and host contention can change time independently of the number of instructions.
We did not isolate their contributions. More text in the harness does not mechanically
imply more tokens or elapsed time.

## What the next comparison should resolve

The evidence supports **uneven concern coverage and an unstable small-sample timing mean**.
It does not establish a harmful A×B interaction or reliable speed advantage. The proposed
three-scenario C/A/B study tests repeatability and domain dependence with matched waves,
initial/final snapshots and phase timing. It will not establish the cause of A+B's result,
because A+B is intentionally omitted to save cost. A direct interaction claim would need
another independently specified A+B condition later.

Per-run calculations and source hashes (`retrospective-evidence.json`),
original assessment (`deliveries/pe1/assessment/artifacts/planning-assessment.md`),
[next planning comparison](../PLANNING-FOLLOWUP-PLAN.md).
