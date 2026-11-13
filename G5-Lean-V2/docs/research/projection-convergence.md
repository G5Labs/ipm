# P1.3 projection convergence and retained domain consumers

The program owner approved a narrower deletion scope for P1.3 in PR #31.
Projection ownership moves to `maintenance/projection_convergence.rs`;
operation-bound authorization and lifecycle inputs remain until their owners
are replaced by P1.4, P1.5 and P4.8.

The loop enumerates local `refs/heads/*` at startup and every 60 seconds.
`workspace.branch_updated` wakes the same pass early. Hint payload OIDs are
never used as content authority. Each projection takes a per-ref advisory
lock, captures an immutable Git view, checks the ref again before committing,
and records its OID in `projection_state`. Inline writes and their checkpoint
commit in one transaction.
Docs and ontology errors roll back their checkpoint and retry independently.
Search enqueues missing work and checkpoints only after every bound branch
has indexed that OID successfully. Failed jobs are re-enqueued even if the
ref has not moved. The search worker pins the queued OID and checks
Git before and after indexing, under the same projection lock. It uses the
convergence pass to catch up ontology before indexing.

The head columns are repairable caches. Missing refs remove checkpoints; a
missing or unreadable mirror fails the pass instead of pretending its refs
were deleted. Recreated refs and newly attached branch members are projected
again. Binding insertion/ref reassignment invalidates the target ref's
checkpoints under the same advisory lock. This covers a fork transaction that
started before a pass but committed its binding afterward; transaction
creation timestamps cannot establish projection coverage.
Dirty canonical docs drafts protect only their own paths. Other committed docs
continue syncing, while the checkpoint stays pending so the protected paths
retry at the same OID when the drafts clear. The docs cache carries draft content
until P2.4.

## Why the domain persistence remains

`finalize_publish_branch_state` registers `TaskLifecycleConsumerInput` and
`WorkerAuthorityGuardInput` before finalization. These carry the requested
transition and caller's worker session generation. The generation can rotate
while the Git OIDs stay identical, so the finalizer must validate these
inputs inside its application transaction. `advance_consumer` also applies
and replays task transition/retry results and merge completion requests.
Ref OIDs cannot reconstruct those inputs or results.

- `LEAN-TODO(P1.4)`: finalizer authorization, task transitions/retries, CRDT
  rollover, approval-publication settlement, operation sequence DTOs/events,
  and their consumer persistence/receipts.
- `LEAN-TODO(P1.5)`: remote-tracking merge and external-Git handoff ordering,
  including `branch_integration_queue.advance_id`.
- `LEAN-TODO(P4.8)`: job merge completion inputs and replay.

The consumer tables and historical adapter-named constraints remain marked;
`search_reindex_queue.advance_id`, search receipts, operation-bound docs
capture/apply, and the compatibility ontology consumer are removed. The
remaining reference inventory and validation results are in the PR description.
