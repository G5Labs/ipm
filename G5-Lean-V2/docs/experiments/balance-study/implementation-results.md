# Fresh implementation fidelity: first deliveries

Two fresh Sol/xhigh coders implemented the same [approved design](implementation-plan.md)
using their native workflow. I-N had source and ordinary documents; I-O also had maintained
ontology, with a short instruction to consult relevant contracts for consequential
questions. Neither had a mandatory planning/review skill, evaluator feedback, or an
author from the design stage. See the protocol (`implementation-protocol.md`).

## What was delivered

| | I-N: native | I-O: ontology available, native |
|---|---|---|
| Frozen first commit | `00bb81b1512486096fd72161a4a0ddb8e33f3c8d` | `844536fe77da51dda540bd7a9c2fba3e5b539069` |
| Application and test diff | 2,386 added / 1 deleted lines: 1,846 non-test and 540 test additions; another 11 lines amend the plan | 1,880 added / 3 deleted lines: 1,360 non-test and 520 test additions |
| Task elapsed | 34m04s | 26m54s |
| Output tokens, including reported reasoning output | 79,868 | 67,623 |
| Noncached input tokens | 379,795 | 395,268 |
| Compactions | 1 | 1 |
| Source-driven design amendment | Legacy workflow initial input can lack `task_id`; use only a uniquely recoverable original initial input and always bind the new one | None saved |

These are source/task metrics, not productivity or price estimates. Both share a host;
build/test waits and model variation were uncontrolled. I-O used less elapsed/output in
this pair, so a general assertion that ontology necessarily makes implementation expensive
is contradicted by this observation. This single pair does not establish a cost advantage.
Added lines include tests and migrations; line count is a scope indicator, not quality.

Both created real routes, durable replay/lineage schema, graph selection and atomic
allocation, UI integration, and tests. Both ran native checks before returning. I-O fixed
provisional mutation-policy and host-health omissions during its own work; those draft
gaps are not charged to its delivery. The saved patches and receipts (`implementations/`)
preserve the actual final source. Author check summaries are separate from the independent
results below.

## Identical independent runtime checks

Tests used private PostgreSQL/Redis and real API, transaction, task-cancellation and
dispatch-reservation owners. A synthetic source graph has a failed root, two causally
blocked descendants, one successful prerequisite and one successful sibling, with
nonempty immutable attempt history. No real guest/provider or browser run is claimed.
Only route spelling differs between arms.

| Independently exercised property | I-N | I-O |
|---|---|---|
| Exactly three fresh tasks/sessions/inputs, edited root only, internal/external edges, original history unchanged, concurrent identical replay creates one run, changed request conflicts | Pass | Pass |
| Unsupported graph, failed external prerequisite, nonterminal source and missing effective input refuse without partial work | Pass | Pass |
| Stopped/base/closed admission refuses fresh work; accepted receipt remains readable after Stop | Pass | Pass |
| Outsider and nonowner refuse without allocation | Pass | Pass |
| Malformed typed launch/task specifications refuse | Pass | Pass |
| A forced input-writer error rolls back jobs/tasks/sessions/receipt; a subsequent valid retry succeeds | Pass | Pass |
| Ordinary owner task cancellation settles only the new graph through normal lifecycle owners | Pass | Pass |
| Read-only/freezing workspace refuses fresh agent work | Pass | Pass |
| With unchanged settings, dispatcher reserves the accepted model | Pass | Pass |
| Organization default changes after submission: dispatch preserves the accepted model | **Fail** | **Fail** |

Thus both initial deliveries pass nine of ten test functions. **This is not a 9/10
quality score.** Several functions cover multiple obligations; many other obligations
remain unexecuted. Both persist a resolved launch for display but let the dispatcher
resolve inherited preferences again. The probe observes `claude-opus-4-8[1m]` accepted
and `claude-opus-4-8` reserved after a config change. It observes the real invocation
payload before guest execution. The unchanged-setting control passes. This is an
unfulfilled explicit new-feature promise through inherited dispatch behavior, rather
than proof that either patch regressed previously working ordinary dispatch.

Two source-driven diagnostics added **after independent reviews** also fail in both:

| Additional case | Executed result | Interpretation |
|---|---|---|
| Valid future capacity nudge, VM still running and admission open | HTTP 409, no retry allocated | Both new guards incorrectly equate pending Stop identity with closed admission. Established v9 meaning permits this case. |
| Retried root has succeeded; two sibling descendants are ready; copied job cap is one | After confirming the first start, ordinary reservation admits a distinct second child | Existing agent dispatch omits the job cap; copying its value does not enforce it. This is an inherited consumer gap exposed by the new feature. |

The extended suite therefore has nine passing and three failing functions for each
unchanged first delivery. The original ten-test observation remains preserved. The completed retry root is a fixture precondition; no real provider completion is
claimed. The strengthened nudge probe invokes the actual lifecycle owner before the
public retry request, preserving its generation and notice writes. Both candidates
still fail. Actual admission/reservation effects are executed. The nudge omission prompted one concise correction in `g5.lifecycle.deadline`.

## Architecture and other consumers

Both implementations depart from the plan's explicit allocation ownership: TaskService
validates/inserts tasks, ChatService allocates sessions **and initial inputs**, and
JobService sequences them. The plan gives TaskService complete task/initial-input
validation and allocation, with ChatService owning sessions. This is a real architectural
deviation even though all writes share one transaction and the rollback probe passes.
Neither is a Jobs-owned raw task writer concealed by a forwarding shim; that stronger
accusation would be inaccurate.

Independent fresh reviews of the **same I-O artifact** also found UI recovery blocked
when compute stops or fresh preview fails, and missing sync progress context. These are
source findings, not browser-tested results. The ontology-informed reviewer detected
the allocation-owner deviation and job-cap omission; the native reviewer detected sync
context loss. Both found launch drift and UI recovery. This does not license comparing
I-N and I-O by the number of findings: I-N did not receive the same two complete reviews.
Detailed claim adjudication is in [review-adjudication.md](review-adjudication.md).

## Evaluator corrections and coverage limits

Initial evaluator runs had a wrong config column, an invalid stopped-VM row shape, and
an attempt to clear an unreleased Stop identity. The first additional nudge fixture
also hard-coded its generation; the final version invokes the real nudge owner. These
were evaluator fixture defects,
not candidate failures. The construction record (`implementation-protocol.md`) identifies
each correction. Expectations did not change; original bytes/logs and subsequent hashes
remain locally preserved. The exact final common probe and concise output logs are saved.

Unexecuted boundaries include real guest/provider completion, browser reload/uncertain
response recovery, full attachment retention, comprehensive permission changes, and
concurrent Stop/transfer admission. The independent synthetic graph cannot certify all
legacy producers, progress profiles or event interleavings. Neither first delivery is
established ready to merge.

The practical result is stronger than “the plan was ignored” and weaker than “a good plan
guarantees code”: both native coders delivered substantial correct composition, while
the same explicit consumer-level and ownership obligations were missed. More text in the
plan is not the demonstrated missing ingredient. Independent tracing to execution owners
and allowed-case probes revealed what ordinary author checks did not.
