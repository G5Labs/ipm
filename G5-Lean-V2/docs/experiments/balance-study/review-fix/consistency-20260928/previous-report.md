# Can an independent reviewer find and fix the missed requirements?

Experiment completed 2026-09-28. This follows the [original balance study](../../README.md).
It answers two separate questions:

1. **Did the review skill improve repairs or efficiency?** The groups repaired different
   defects: the old-skill runs did better on concurrency limits, and the revised-skill
   runs did better on preserving the accepted model. These repairs were independently
   checked. Both skill groups used more time and output tokens than normal review.
   We have not established a general skill advantage or better results per unit of effort.
2. **Could a reviewer reliably discover and correctly repair the issues without hints?**
   Not across this task's requirements. None of the nine workers completed all repairs.
   Some missed a governing rule; some retrieved it but missed the violation; others found
   the defect and delivered an incomplete or incorrect repair. Those are different failures.

## What we ran

The feature lets an owner retry a failed workflow branch without repeating successful
work. We gave **nine fresh GPT-6 Sol/xhigh workers separate copies of the same initial
implementation**. Three used normal review, three the old review skill, and three the
revised review skill. GPT-6 Astra/xhigh supervised and independently evaluated the results.

**Every group received the same code, approved plan and corrected ontology.** Normal
review explicitly required consulting relevant ontology, but did not require a review
skill or ontology/metadata edits. This tests the extra review procedure over shared
knowledge. It does not compare ontology with code-only review, or test the planning harness.

Each worker could investigate, run tests and repair code in its own local environment.
It saved initial findings before production edits and recorded later discoveries separately.
Previous findings, metadata that disclosed those findings, and evaluator tests were withheld
from all workers; the governing rule text was retained. No worker received feedback before
finishing. [Exact setup and preserved versions](../protocol.md), worker prompts (`../prompts/`).

## 1. Did the skill help?

Each cell below says how many of the **three workers in that group found the issue and
how many successfully repaired it**. Discovery includes both the initial report and later
review. “Fixed” means passed the independent check described below, not merely claimed fixed.

| Requirement | Normal review + ontology | Old skill + ontology | Revised skill + ontology |
|---|---|---|---|
| Keep the accepted model when defaults change | 1 found; 1 fixed | 1 found; 0 fixed | 3 found; 3 fixed |
| Allow new work before a future shutdown deadline | 0 found; 0 fixed | 0 found; 0 fixed | 0 found; 0 fixed |
| Enforce the workflow's concurrency limit | 0 found; 0 fixed | 3 found; 3 fixed | 0 found; 0 fixed |
| TaskService creates the task and its first input | 2 found; 2 fixed | 3 found; 3 fixed | 2 found; 2 fixed |
| Recover a saved submission without a fresh preview | 3 found; 3 fixed | 3 found; 3 fixed | 3 found; 3 fixed |
| Keep that recovery action visible after compute stops | 2 found; 2 fixed | 3 found; 3 fixed | 1 found; 1 fixed |
| Show progress for only the selected synchronization work | 1 found; 0 fully fixed | 0 found; 0 fixed | 0 found; 0 fixed |

**These are meaningful differences, not equal results.** An old-skill run could pass
10 of the 12 backend checks by fixing concurrency, while a revised-skill run passed
10 by fixing model preservation. A total alone hides which requirement still fails.

The skill groups also introduced mistakes: one old-skill worker and one revised-skill
worker prohibited reusing an already successful command prerequisite, even though the
retry would not execute that command again. The same independent allowed-behavior check
passed on the original code and the other seven deliveries. Reviews also found useful
additional defects, including receipt-read permissions, a submission deadlock and a
missing dependent task. The table is not an exhaustive quality score.
Individual findings and regressions (`../adjudication-ledger.json`).

### What about efficiency?

| Review procedure, all with ontology | Median elapsed time per worker | Output tokens across its three workers |
|---|---:|---:|
| Normal review | 11m46s | 104,568 |
| Old skill | 20m28s | 171,650 |
| Revised skill | 17m00s | 147,695 |

**Effort was measured; efficiency improvement was not established.** The workers repaired
different things, the skill groups also maintained ontology records, and there was no
comparison at equal actual token use or equal completed quality. Shared host load affects
time. We cannot say that the extra repairs were worth the extra effort from these data.

The nine workers used 423,913 reported output tokens. Astra preparation, evaluation and
reporting used another 219,296 at the original completion snapshot, excluding subsequent
report revisions. Output includes reasoning tokens; these are not dollar costs.
Full effort accounting (`../effort-summary.json`).

## 2. Where did discovery or repair fail, issue by issue?

“Retrieved” below means that the relevant text appeared in a visible tool result.
It does not prove the worker attended to or understood it. Findings, final code and
independent checks let us identify the observed failure stage, but not the worker's
internal reason for taking a particular path. [Run-by-run evidence](../issue-trace.md).

### A. The accepted model changed while work waited

If the owner approves model A, changing the default to B must not silently change the
queued task, provided A remains authorized and runnable. **All nine retrieved the plan's
freeze requirement. Five found the violation; four repaired it.** Four therefore missed
the defect despite having the instruction in context.

The fifth finder, old-skill run **u07**, stored the accepted choice but still resolved
current defaults at dispatch. Its “fix” permanently failed the task when those defaults
differed. That prevented substitution but also rejected valid work instead of using A.
Its check accepted this refusal; the independent changed-default check exposed the failure.
This was a **wrong repair after discovery**, not missing ontology.

### B. A future shutdown warning blocked work too early

The ontology explicitly allows new work during the warning interval before shutdown.
The implementation treated a pending shutdown marker as if shutdown had already begun.
**None of the nine visibly retrieved the exact exception, identified the violation or
repaired it.** A broader search of public outputs also found no matching nudge passage or deadline block ID.
This is consistent with a **retrieval gap**. It does not establish that providing the
passage would have been sufficient; that intervention has not been run.

### C. The workflow's concurrency limit was ignored

If a workflow allows one running task, two ready children must not start together.
**All nine retrieved the ontology's job-cap rule. Only the three old-skill workers
identified the missing enforcement; all three repaired the sequential case we tested.**
The other six missed a violation of a rule already returned to their context.

The code copied the limit into the new workflow, but the agent scheduler never enforced
it. u01 even discussed a zero limit using the command scheduler's behavior, overlooking
the different agent path. This is a **rule-to-code tracing failure**, not simply absent
knowledge. We do not know why the old instructions prompted better coverage here.
Concurrent race safety remains unproven; the verified repair is narrower than that.

### D. The wrong service created the first task input

The plan and ontology assign task and first-input creation to TaskService; ChatService
owns sessions. **All nine retrieved this ownership rule. Seven identified the violation
and repaired it.** u01 and u03 left it in place.

In particular, revised-skill run **u03** recorded the ownership commitment as satisfied
because validation and writes occurred in one transaction. That established atomicity,
not which service owned the operation. This is a **misapplied rule during review**.
A completed review checklist did not make its conclusion correct. Independent source
inspection, rather than a passing rollback test, established the ownership repairs.

### E. Recovering a saved request unnecessarily required a new preview

After the server accepts a retry but the browser loses the response, replaying the saved
request should recover the existing result. It should not depend on a fresh preview
that can now fail. **All nine eventually found this defect and repaired direct replay.**
The independent component check passed in every final delivery. This is a consistent
success at that component boundary, but it does not establish the whole recovery flow.

### F. The recovery component disappeared when compute stopped

The containing page hid the entire recovery panel when the workspace became read-only
after compute stopped. **Six identified and repaired this second barrier. u01, u03 and
u08 fixed the component but missed its parent.** Their isolated component checks could
pass because they mounted a panel that the actual page would hide.

This is a **missed caller and incomplete feature verification**. They did not identify
this specific barrier and then fail to repair it; they stopped short of finding it.
Independent inspection of the real page/component chain exposed the remaining defect.
No full browser journey was executed.

### G. Synchronization progress lost the selected work's context

The retry omitted information used to display synchronization direction and targets.
Eight workers did not report this defect. **Only normal-review run u05 found it**, then
copied the original workflow's entire context. If the original covered documents A and B
but the retry selected only A, its progress could still show B.

This was **discovery followed by a partial repair**. Its test checked that JSON was copied
on a generic job; it did not exercise the synchronization display that interpreted that
JSON. The plan required truthful progress, but tracing that requirement into this consumer
was necessary. Source inspection established the remaining display error, not extra execution.

## What this establishes, and what remains unanswered

The comparison establishes specific successful repairs and concrete failures of discovery,
interpretation and repair verification on this feature. **Access to a rule, finding its
violation, and implementing a correct fix are separate steps.** The revised skill asked
for governing text and evidence at actual consumers, yet its workers still missed rules
and callers and sometimes marked an unmet commitment as satisfied.

Three attempts per procedure on one known feature do not explain why one skill group
covered a particular defect better, establish a generally superior harness, or show a
reliable autonomous reviewer. The successful component repair also shows that failure
was not uniform across requirements. The next useful question is which intervention
repairs each failure stage, rather than whether every additional instruction helps.

## How we will test improvements

The next comparison separates **finding issues** from **repairing a fixed set of issues**.
Our review combines ontology/design coverage, governing invariants, compatibility, architecture
and code quality. The hypothesis is that assigning primary responsibilities helps agents
cover that breadth more consistently, without excluding cross-cutting findings.

1. **Discovery only:** three fresh independent general reviewers versus three fresh independent
   reviewers focused on (a) intent/coverage/invariants, (b) behavior/integration/compatibility,
   and (c) architecture/design/code quality. All see the same full inputs. Repeat the paired
   comparison three times; consolidate identically and preserve minority findings. No repairs.
2. **Repair reliability:** assemble the new discovered findings into one identical packet for
   three fresh fixers. Preserve each first coherent repair, then measure at most two additional
   repair rounds with fresh reviews. Compare actual fixes, regressions, false-clean stopping
   reports and marginal effort. An empty review is not proof that the implementation is aligned.

All workers receive the same concise [review rubric](../REVIEW-RUBRIC.md), with evidence-backed
Supported / Partial / Violated / Unverified assessments. Known defects remain concrete controls;
additional applicable obligations and ordinary defects remain in scope. This is not an audit
of unrelated repository code. Historical /10 totals are not adopted as the optimization target.
Concrete plan, controls and decisions (`../NEXT-EXPERIMENT.md`). **No new trials have run.**

The baseline and both harness versions, all initial findings and final patches remain
preserved. Application repairs stayed isolated and private services are stopped. The
[detailed results](../detailed-results.md) retain test totals, additional claims and limits;
test records (`../test-results.json`) and [evaluator corrections](../evaluator-corrections.md)
retain raw failures and the uniform request-format adjustment needed by one delivery.
No real provider execution, full browser journey or exhaustive concurrency proof is claimed.

## Final alignment comparison in the next report

The goal is implementation consistency with the **original approved ontology diff/design
and the relevant governing contracts, concepts and system invariants**. End the next report
with per-obligation alignment before repair, after the first fix and at the bounded loop's
stop, plus remaining violations and evidence gaps. Discovery accuracy and actual code alignment
are different metrics. This retry case uses its approved shared plan as the feature-intent
reference; its broader draft ontology patch was not approved for coding. Validate actual
ontology-diff alignment on subsequent features that have that frozen input. The full ledger
has not yet been evaluated for the new trials; no alignment score is implied here.

---
Archival reading copy of `approved-README.md`; only relative links were adjusted for this directory. The frozen original is preserved unchanged.
