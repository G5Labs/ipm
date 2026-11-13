# Approved study design: retry a failed agent workflow branch

This is the common implementation contract for both I-stage workers. It consolidates
the D2 designs after Astra adjudication; the user delegated design decisions to this
study. It is an experimental feature, not a production rollout. Both workers receive
identical bytes. Implement the backend and existing frontend experience with native
coding and verification. No prescribed generation method or separate review ceremony.

## Outcome and product choices

On a terminal failed workflow job, its current workspace owner can select a genuinely
failed agent task and retry that task and its transitively dependency-blocked agent
descendants as a fresh linked run. Every task in the source job must be terminal;
checking only the job's derived failed label is insufficient. Cancelled/held work keeps
its existing actions. Bootstrap/control-command retries and mixed unsupported task kinds
are outside this bounded feature: refuse them clearly without silently dropping work.

The server derives the closure from authoritative task dependencies and outcomes.
Descendants that already succeeded or failed independently are not silently retried.
If a dependent also needs a failed/cancelled prerequisite outside the selected set,
refuse with the blocking prerequisite identified. An incoming prerequisite outside the
set must have authoritative terminal success. Record that success immutably and reuse
it without new execution. Different intentional submissions may create distinct linked
runs; identical transport retries must return the same run.

The owner sees the selected tasks, copied effective instructions, reused success and
the current-checkout caveat before confirming. Only the selected task's new instruction
is editable. Preserve descendants' frozen effective input and required input material;
if the original executable input cannot be recovered faithfully, refuse with an
actionable reason. Never silently synthesize an empty or approximate instruction.

## Responsibilities and durable meaning

JobService owns a complete retry operation and coordinates one tenant-scoped transaction.
It selects/validates the source slice and records the new job, source job/root and task
mapping, immutable external-success citations, and replay receipt. TaskService owns
validation and allocation of tasks and initial inputs; ChatService owns new sessions.
Compose their owning operations in the same transaction. A forwarding wrapper over a
Jobs-owned raw task/input writer does not satisfy that responsibility. Keep new graph
validation, writes and failure rollback together; dispatch/events follow commit.

Each new agent task gets a fresh logical task, session and initial input, with its own
future attempts. Keep all source tasks, attempts, transcript, inputs, statuses, usage
and output history intact. No old provider-native session, unresolved inbox item or
delivery acceptance supplies new-run authority. Copy non-authority task specifications
as provenance; explicitly validate and freeze the new accepted launch specification.

Recreate internal edges between new tasks. Ordinary normalized same-workspace dependency
edges may refer to the original successful prerequisite, with an immutable success
citation explaining the reused result. Keep one readiness/settlement authority and
retain required referenced source evidence. Do not fabricate successful execution
tasks, reset old terminal states, add a second scheduler, or count reused success as
newly executed work. Existing TaskService dependency settlement and cancellation remain
effective on the new graph. A job's status is derived from its own tasks.

## Admission and replay

Fresh submission requires the current owner, non-base workspace, healthy running
incarnation, open admission and ordinary stop/transfer gates. Resolve the current owner's
eligible provider accounts and validate complete task kind/provider/model/permission/
input specifications. Historical ownership or credential snapshots confer no authority.
Dispatch still enforces current admission, capacity, task/session serialization, held
work, spending and dependency readiness. Retry submission never records Start consent
or boots a stopped workspace; expose the ordinary explicit Start action separately.

Bind one replay key in a declared scope to the accepted source job/root, exact edited
instruction and launch selection, and persist its resulting job in the graph transaction.
The same request returns the same graph after a lost response. Changed accepted input
conflicts without allocating anything. Validate current read access before returning a
historical receipt; receipt replay alone has no new execution effects. Do not require a
still-running VM merely to read an already completed submission result. Concurrency and
failed allocation must leave at most one committed graph per accepted identity.

## Product integration and acceptance

Use the existing job DAG/progress and chat surfaces. Show an eligible owner action and
a confirmation panel with the source/new instruction, selected closure and reused
prerequisites. Keep the client key stable across an uncertain submission and offer safe
recovery after reload. Link both original and retry runs and each new task to its source.
Show accurate queued, held, running and terminal state from the ordinary owners; count
only new tasks in this run's progress. Reused successes are labelled as historical.
New selected chat input contains the edit; old chat remains unchanged. Durable history
is readable when compute stops. Existing shared checkouts are used: there is no rollback,
branch isolation or guarantee that historical prerequisite files remain unchanged.

The distinguishing success case has a failed root, two causally blocked descendants,
an external successful prerequisite and an unrelated successful sibling. It produces
exactly three new tasks with fresh sessions/input and correct internal/external edges;
the successful tasks have no new attempts. The edited instruction affects only the new
root. Dependency completion releases the correct descendants. Replay returns the same
run; changed input, missing effective input, mixed failed prerequisites and invalid
authority/specification refuse without partial graph or VM start. Both directions of
lineage and truthful progress survive reload. Relevant existing job/chat behavior works.

## Consequential source assumptions

The high-level design does not certify existing helpers. During implementation establish
how terminal causes are recorded, effective initial input/material is retained, successful
task evidence and cross-job edges survive retention, and TaskService/ChatService can
compose allocation and validation atomically. Confirm actual UI consumers and derived
status. Use the smallest sound adaptation. If source facts force a consequential design
change, save the reason and revised choice before dependent work, preserving the original
contract for evaluation. Routine implementation choices need no supervisor approval.

Verification should exercise meaningful public and persistence boundaries using the
assigned private services; distinguish inspection from execution and unavailable real
guest/provider coverage. Save the first completed patch and exact check results before
any later independent review or evaluator feedback.
