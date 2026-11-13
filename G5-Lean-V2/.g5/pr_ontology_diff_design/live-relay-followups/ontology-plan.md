# Guest calls without a pinned connection, batched membership rechecks, and a transcript whose render cost follows change

Request: [chat intent](../../chat_intent/live-relay-followups/request.md). Follow-up to
[#966](https://github.com/G5Labs/G5-Lean/issues/966) ([design](../issue-966/ontology-plan.md),
[PR #973](https://github.com/G5Labs/G5-Lean/pull/973)), whose review found the three
costs below but bounded only the follower's side of the first.

## 1. A guest call holds no database connection while the guest answers

### What is wrong

`WorkspaceRuntime::call_once` admits every guest call inside one transaction: it reads
the workspace authority, takes the incarnation row `FOR SHARE`, checks the lease and,
for owner calls, the membership activity, and then keeps that transaction open until the
guest's reply line is read. Only `admission.close`, `admission.open` and `workspace.flush`
commit first. The transaction's only lasting effect is the share lock: the stop admission
takes the same row `FOR UPDATE`, so a stop waits for calls in flight. The cost is one
pooled connection per in-flight call for up to its budget. #973 bounded the follower's
calls to three quarters of the pool; owner requests (Stop delivery, steering, native
replies, Changes, previews, terminal secrets, inventory) are bounded only by the pool, so
a host whose guests accept calls but stop answering turns each such request into a pinned
connection and the whole API into one waiting on the pool.

### Owners and contract

- **Admission** (`WorkspaceRuntime`): the same checks, in a transaction that commits
  before the round trip for every method. Nothing durable was written after the checks,
  so committing earlier changes no durable state; a deactivation or ownership change
  committed during a call no longer waits for it, which
  [`g5.security.membership-streams`](../../ontology/concepts/security.g5.md#g5.security.membership-streams)
  already allows (an admitted finite batch cannot be recalled).
- **In-flight calls** (`WorkspaceRuntime`, process-local): a counter per workspace with
  the latest deadline among its calls and a notify, registered as the call's admission
  is decided, while its `FOR SHARE` on the workspace row is still held, and released
  with the reply or its failure. This replaces the share lock's one guarantee:
  [the stop admission](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.by-user)
  and the transfer drain the count before they take the workspace row and again under
  that row. Under the row, every call whose admission committed is counted (its share
  lock preceded the exclusive one), and a call still waiting for the row is not, so the
  wait never includes a call the row itself blocks and a continuous stream of new
  requests cannot keep it from its decision; the late call is refused once the row is
  released. So no owner call is interleaved with either. The
  wait is bounded by the calls' own deadlines plus a grace second; a call still counted
  past that has outlived its budget, and the stop or transfer refuses (503, retried)
  rather than proceed beside it. Lifecycle-gated methods (admission open and close,
  flush) never held the fence and are not counted. The wait is process-local, which the
  one-replica API deployment allows and
  [`g5.chat.live-delivery`](../../ontology/features/chat.g5.md#g5.chat.live-delivery)
  already records; a multi-replica API would need the share lock back or a shared
  registry, named here as the limit of this mechanism.
- The authority lookup also took the workspace row `FOR SHARE`, so an ownership
  transfer waited for calls in flight as well; the transfer now drains them the same
  way before it moves ownership. The other incarnation-row writers (preview start,
  cancel and projection, which take the row for lock order with the task row) never
  needed to exclude calls; unchanged. Delete and placement take the workspace
  exclusively; unchanged.
- Callers already re-verify at apply time: history batches lock the attempt by VM and
  epoch, the flush callback repeats the owner, VM, epoch and lease checks, and read
  paths return data from the stream the tunnel keyed by VM id. No caller gains an
  assumption.

Choice considered: a dedicated small pool for the fence. Rejected: it moves the bound,
it does not remove the hold, and a stuck guest still exhausts it.

Acceptance: with the test pool of two connections, several owner requests whose guest
holds the reply proceed at once and no connection sits idle in a transaction
(`pg_stat_activity`); a stop racing a held call sends `admission.close` only after the
held call ended; the existing lease, owner and incarnation refusals are unchanged.
Ontology: [`g5.runtime.guest-call-admission`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.guest-call-admission).

## 2. Membership is rechecked per bounded batch of delivered events

### What is wrong

The WebSocket writer runs one membership query per delivered event (`MembershipDelivery::authorize`,
with a five-second timeout) before sending it. A 10,000-row turn is 10,000 queries per
open tab, and a slow database stalls every viewer's delivery, fills the writer queues and
closes clients as too slow: a database incident becomes a chat outage.
[`g5.security.membership-streams`](../../ontology/concepts/security.g5.md#g5.security.membership-streams)
requires a recheck "at each protected input or bounded output-batch admission"; per
message is one allowed shape, not the required one.

### Contract

The writer admits a **batch**: everything already queued, at most 256 events, with one
membership check per organization in the batch, and a verdict is reused for at most one
second, so a trickle of single events does not run one query each. A denial discards the
whole unsent batch and closes the stream with the same code as today; a notification for
another organization still has its own per-event resource lookup and is discarded alone.
The bound (256 events, one second) is the batch the commitment speaks of. A membership
change made through this API also bumps an in-process generation that every standing
verdict is keyed on, so the next batch rechecks at once; a change made elsewhere still
reaches an open stream within one second plus one batch, far below the cleanup
barrier's own bounds, and a missed invalidation cannot permit forwarding beyond that. The hub's 2,048-event queue and the slow-client close are
unchanged.

Acceptance: a burst of events is delivered with one membership query per organization
per second; a deactivation between two batches closes the socket before the second batch
is sent; the existing membership-stream tests hold.

## 3. The transcript's render cost follows what changed

### What is wrong

`ChatPanel` re-projects the whole transcript on every commit (`buildUnifiedTimeline`
and the agent-output projection walk every held entry) and every `AssistantTurn`, a
plain function, rebuilds its blocks. #973 bounded commits to one per animation frame;
each commit still costs time proportional to the rows held: about 25 ms at a window of
two hundred rows, 44 to 96 ms with thousands of live rows, so a burst of 2,000 rows takes
some 27 seconds to settle and a long turn held live stays sluggish.

### Contract

- **Projection per turn**: the unified timeline keeps each turn's item and entry list by
  identity when the turn's entries did not change, so a commit re-projects only the turn
  that received rows and the list reconciles mostly unchanged elements.
- **Turn rendering**: `AssistantTurn` is memoized on its message, its entries' identity
  and the few flags it reads; the callbacks it receives are stable.
- **Offscreen turns**: tried `content-visibility: auto` with an intrinsic size and
  withdrew it. Unrendered turns collapse to placeholders, which shortens the transcript
  so that the history window's "within one viewport" trigger
  ([`g5.chat.history-window`](../../ontology/features/chat.g5.md#g5.chat.history-window))
  fires for turns that are far away once rendered (the pagination flow failed on it in
  CI), and containment clips fixed overlays inside a turn. The acceptance below measures
  React's work per frame, which memoization meets without it; unmounting offscreen turns
  (virtualization) would face the same trigger and overlay questions and is not adopted
  in this change.

Acceptance (the browser harness `e2e/browser/chat-large-response.spec.ts`, real
`ChatPanel`): per-commit cost stays within a small constant factor as the held window
grows from two hundred to ten thousand rows, commits stay one per frame, and the existing
rendering, approval and Stop tests hold. If per-commit cost still grows with the held
rows after these three steps, virtualization becomes the next step, reported as a gap.
Ontology: the sentence added to
[`g5.chat.live-tail-recovery`](../../ontology/features/chat.g5.md#g5.chat.live-tail-recovery).

Measured (this VM, the harness pacing ten rows per frame, one commit per frame): the
mean commit at about two hundred held rows is 7–8 ms, at two thousand 11–12 ms (ratio
1.6, asserted at most 2) and at ten thousand 25 ms (ratio 3.3, asserted at most 4);
before, the same window cost 18 and 70 ms (ratio 3.9 at two thousand). Two thousand
all-live rows settle in 2–7 s instead of 27–40 s. A streaming answer's text renders in
pieces split at paragraph breaks outside code fences (fences read by complete lines
across deltas), so a delta re-renders only the open piece. Remaining gap: at ten thousand held rows a commit still pays a dozen
whole-window passes (projection, ordering, transcript filters, the activity label,
approval classification), each under a microsecond per row; the next step for that is
an incremental transcript index, not virtualization, since unchanged turns no longer
render at all.

## Assumptions

- One API replica per environment (in-process in-flight count and verdict cache), as
  the live relay already assumes.
- The stop's or transfer's drain wait is bounded by the budgets of the calls in flight (up to
  the four-minute rebase budget); a guest that never answers delays a stop by at most that,
  as it did under the share lock.
