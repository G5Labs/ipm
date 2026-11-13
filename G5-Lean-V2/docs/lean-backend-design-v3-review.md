# Review of the backend design, draft 3

Reviewed 2026-09-13 against [draft 3](./lean-backend-design-v3.md), the answered questions from both rounds, and the current backend. Recommendation: revise before acceptance. The workspace VM, native Git, explicit Save, Postgres chat history, and per-turn provider processes form a coherent direction. The findings below concern contracts needed to make that direction work.

This is a design review, not a report of deployed failures. The Git findings include local reproductions; the distributed failure sequences are consequences of the protocols as written. No backend or design file was changed.

1. **[P1] Restore committed-but-unpushed history, not just the checkpoint's final diff.** [Design §5.4, lines 308–329](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:308)

   Suppose internal Git has branch head `A`, the VM commits `B` without pushing it, and stop successfully pushes checkpoint `C`, whose parent is `B`. A fresh VM checks out `A`. Because `C^ != A`, the proposed restore cherry-picks `C` without committing. That applies only `B → C`; it omits `A → B`. If the working tree was clean at stop, that diff is empty: resume succeeds with the file introduced by `B` missing.

   **Validation:** reproduced using temporary repositories and the specified temporary-index/commit-tree/cherry-pick sequence. The checkpoint contained the file, cherry-pick exited 0, and the restored checkout was clean with the file absent. `B` remains recoverable through the checkpoint temporarily, but the next force-update can remove that recovery reference.

   **Required revision:** distinguish the internal branch head observed at checkpoint time, local HEAD, and the working-tree snapshot. Restore the local commit chain as well as uncommitted edits, with a defined reconciliation path if the internal branch changed. A single shadow ref can still retain the necessary objects. Add a stop/resume test with a clean, committed-but-unpushed file and a second test with both local commits and dirty edits.

2. **[P1] Fence an old VM before admitting a replacement.** [Design §6.1, lines 408–413](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:408)

   Missing heartbeats establish loss of contact, not that the old VM stopped. The draft replaces it on another host, while its renewable JWT is scoped only to the workspace (§5.2). A partitioned host can reconnect after the replacement starts, still run tasks, push the workspace branch or checkpoint, and upload older session files. The unique `workspace_vms` row prevents two current database assignments; it does not by itself revoke the previous runtime's authority.

   **Required revision:** make a VM incarnation, such as the existing `vm_id`, part of every credential, renewal, command and event. Reject stale incarnations at Git writes, session-file writes, task events, and the LLM proxy. Define host behavior on lease expiry, atomic placement/renewal, and how an old VM is stopped on reconnect. This is one workspace-level fence; it does not require restoring the old per-task lease system. Test a network partition and late reconnect, in addition to killing a host.

3. **[P1] Give task dispatch and events a recovery protocol.** [Design §7.3, lines 577–580](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:577)

   Inserting a queued task and sending an RPC are separate operations. An API crash between them strands the task. If the dispatcher launches the process but its response is lost, a resend can launch it twice. If an exit event is lost while the VM remains running, the specified reconciler never finishes the task because it only checks VM state. This can also strand a pending chat assistant message. JSON-RPC request IDs correlate replies; they do not establish execution idempotency or durable replay.

   **Required revision:** use `(task_id, attempt, vm_id)` as the execution identity, deduplicate start/cancel/answer commands, retain process outcomes until acknowledged, and sequence/deduplicate events. On reconnect, reconcile the API's tasks with the dispatcher's process/outcome inventory and redispatch eligible queued tasks. Persist completion and the corresponding chat updates before acknowledging terminal events. Test disconnects immediately before and after spawn and process exit, plus API restart with the VM still alive.

4. **[P1] Define stop as a durability barrier, including suspended questions.** [Design §5.7, lines 384–394](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:384)

   The specified order is checkpoint, then stop the VM. Explicit stop and host drain can occur while tasks, terminal commands, previews or Save requests still write files. Writes after the snapshot are lost even on a successful, planned stop. The design also does not specify what happens if one repository's checkpoint push fails.

   Waiting tasks expose a related state conflict: they may be stopped and resumed (§7.2), but their provider is blocked mid-turn, session files are harvested only after turns (§8.4), and every process exit ends a turn (§8.2). A resumed session can therefore have only the previous turn's transcript, lacking the pending tool call. Treating the shutdown exit as ordinary failure instead would defeat the promised waiting-task resume.

   **Required revision:** close mutation admission, quiesce processes, explicitly suspend waiting continuations, persist their question/transcript revision, then checkpoint every repo and wait for durable acknowledgements before releasing the VM. Preserve recoverable storage and report failure if this cannot complete. Define how shutdown exits differ from completed/cancelled turns, and how an answer racing with stop is delivered once. Keep the approved idle policy; add tests for active writes, a waiting permission request, and a failed checkpoint push during stop.

5. **[P1] The proposed Codex transport cannot deliver native approval answers.** [Design §8.3, lines 712–724](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:712)

   Mapping an approval to `task_questions` does not supply a response channel to `codex exec --json`, whose prompt stdin has already been closed. This is not behavior the existing wrapper supports: [the current adapter explicitly returns `CodexApprovalUnsupported`](/Users/zliu/Workspace/G5-Lean/backend/g5d/sidecars/codex/sidecar_source.mjs:915), and its targeted regression test passed during this review. A fake CLI that accepts invented approval replies would conceal the incompatibility.

   **Required revision:** choose a bidirectional Codex transport for interactive chat permissions, or explicitly restrict the supported chat permission modes. The [official App Server protocol](https://learn.chatgpt.com/docs/app-server) provides approval request/response messages; it can be evaluated with a process lifetime bounded to one turn. Preserve the one-process-per-turn decision, but verify the actual binary's approval round trip before replacing the adapter. Replaying JSON output alone is insufficient validation.

6. **[P1] Bind approval, publication and merge to the same immutable source SHA.** [Design §10.2–10.3, lines 866–895](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:866)

   PR creation publishes source `S1`. An agent later pushes `S2` only to internal Git, which refreshes the internal PR head and permits approval of `S2`; §5.5 explicitly does not publish every push. Calling the provider merge API can now merge `S1` while G5 records the approved `S2` as merged. Separately, using the event-refreshed PR head or rereading a mutable branch after approval checks creates a race with concurrent pushes.

   **Required revision:** read the authoritative Git source/target OIDs, evaluate decisions for that exact source OID, and retain those OIDs throughout the operation. Require the published provider head to match it, and pass the expected source SHA to the provider merge call. Both [GitHub](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request) and [GitLab](https://docs.gitlab.com/api/merge_requests/#merge-a-merge-request) expose this precondition. Pin agent review input and its submitted decision to the reviewed SHA too. Test an internal push after initial PR publication and a source push racing with merge.

7. **[P1] Replace mirror-first merge and rollback with a recoverable operation.** [Design §10.3, lines 891–896](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:891)

   Updating the authoritative internal target before provider acceptance exposes a commit that might subsequently be rejected. A crash before the provider call leaves it exposed indefinitely; an accepted provider merge followed by a network timeout leaves G5 unable to know whether rollback is appropriate. A rollback can also overwrite a concurrent inbound fetch or merge. Independently computing the internal merge and asking the provider to merge can produce different commit IDs, especially with squash/rebase merge methods.

   **Required revision:** persist a small merge operation record, possibly on the PR row, containing the pinned inputs and pending/completed outcome. For provider-connected targets, establish provider acceptance and its actual result before publishing the corresponding internal result. Reconcile ambiguous responses and retry idempotently; serialize/CAS internal target updates with inbound mirroring. Keep local-only merges as a Git CAS operation. Test crashes around each external boundary, a lost success response, and an inbound target update during merge. A synchronous rollback is not a substitute for recovery.

8. **[P1] Git's receive-pack CAS does not enforce the claimed force-with-lease safety.** [Design §5.3, lines 290–293](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:290)

   A normal `git push --force` sends the remote head it just learned during push negotiation as `old_sha`. That matches receive-pack even if the client never fetched or incorporated that commit. The check only catches a ref changing after advertisement; it does not prove that the rewrite was based on the author's expected head.

   **Validation:** with two clones, clone A pushed a commit unknown to clone B. B's `--force-with-lease` was rejected, but its subsequent `--force` succeeded and displaced A's commit. This is consistent with the [Git documentation](https://git-scm.com/docs/git-push).

   **Required revision:** either make the stronger expected-head precondition explicit and enforceable through a rewrite authorization protocol, or document that workspace branches permit arbitrary owner force-pushes and that lease safety is a client obligation. The current text promises server protection it does not provide. Test the unseen-remote-commit case, not only a ref moving during receive-pack.

9. **[P1] Keep provider session directories outside the Git snapshot boundary.** [Design §11.4, lines 957–960](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:957)

   The proposed session directories are under `/work/.g5`, inside the checkout, while checkpointing uses `git add -A` and relies on the repository's `.gitignore`. An arbitrary connected repository need not ignore these new directories. Consequently both checkpoints and an agent's ordinary `git add -A` can capture provider transcripts, violating the explicit Postgres-only decision and potentially publishing chat contents with source code.

   **Validation:** the specified checkpoint sequence included a synthetic `/work/.g5/codex-home/sessions/...jsonl` file in a clean repository without a matching ignore rule.

   **Required revision:** put provider runtime state outside every member repository's working tree, using a stable path for resume. If it remains under the checkout, specify platform-managed exclusions for checkpointing and native commits, including behavior when files were already tracked. Test both providers with a customer repo that has no `.gitignore`.

10. **[P2] Give Git-derived projections a repair path independent of event delivery.** [Design §5.3, lines 294–296](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:294)

    A ref update can succeed immediately before the API crashes, so `branch.updated` is never emitted. The replacement removes the durable advance handoff and specifies subscription-driven projections, leaving docs, ontology, search, PR heads and list caches stale until another event happens. Today's [EventBus is a nonblocking WebSocket fan-out](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/mod.rs:185), not a durable queue. Moving that fan-out to Redis pub/sub would not close the Git-update/event-publication gap.

    **Required revision:** make events hints and periodically compare authoritative branch/checkpoint OIDs against each projection's applied version, including at startup, or provide an equivalent durable recovery mechanism. Route fork, merge, inbound fetch, checkpoint and deletion through that convergence contract. Test a successful Git update with its event deliberately dropped and verify convergence without another push.

One additional correction: [§6.3, lines 469–473](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v3.md:469) says two agents editing the same file show conflicting changes in `git status`. Ordinary file overwrites produce one modified file, with the last writer's content; Git does not preserve both edits or create conflict stages. A local probe confirmed this. Keep the accepted shared-folder/notice-only policy, but describe its actual lost-update semantics and avoid presenting `git status` as conflict detection.

Validation performed: full design and decision-register review; targeted reads of Git ingress, Codex launch/approval handling, task ownership, session persistence, approval settlement and event delivery; isolated Git reproductions for unpushed-commit restore, force versus force-with-lease, session-file capture and shared-folder overwrites; one existing Codex approval regression test. No cloud deployment, paid provider run, or new backend implementation was exercised. Before phase estimates are treated as commitments, prove the revised persistence, reconnect, approval and merge contracts with small executable acceptance tests.
