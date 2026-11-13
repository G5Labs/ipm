# Follow-up review of backend design v5

Reviewed 2026-09-13 against [v5](./lean-backend-design-v5.md) and the [response to the v3 review](./lean-backend-design-v3-review-response.md). Recommendation: revise the remaining contracts before acceptance. Seven original findings are closed at the design level; three are partially addressed. The remaining issues below include newly identified interactions, not just repetitions of the original review.

V5 explicitly records Decisions A–D as settled. This review accepts those choices: waiting questions keep the VM awake, explicit stop interrupts tasks, Codex native approvals are deferred, and workspace owners may rewrite branches. The response's older “Decisions needed” section is historical, not a reason to ask for confirmation again.

| Original finding | Assessment against v5 |
|---|---|
| R1: unpushed commits omitted on restore | Closed for the original failure. Reproduced v5's restore: local HEAD and the committed-but-unpushed file survive. See F1 for a separate snapshot-construction defect. |
| R2: old VM remains authoritative | Addressed at the contract level by incarnation identity, lease expiry, boundary checks and reconnect cleanup (§6.8). Mutation-time fencing still needs the listed implementation tests. |
| R3: dispatch/event recovery | Partial. Execution identities and retained outcomes help, but F3, F4 and F8 remain. |
| R4: stop and waiting continuation | Partial. The selected interruption policy is coherent; F5 and F6 remain in its protocol. |
| R5: Codex approval transport | Closed by the explicit scope decision and rejection of prompting policies. Both providers' MCP questions remain in scope. |
| R6: approved/published/merged source mismatch | Normal path addressed by immutable source OIDs, republish and the provider SHA precondition. Recovery must preserve this binding too; see F7. |
| R7: merge rollback and recovery | Partial. Provider-first ordering fixes the principal problem, but F7 remains. |
| R8: receive-pack does not enforce client lease safety | Closed as a documented policy decision. F2 is a separate interaction with the background checkpoint fetch. |
| R9: provider transcripts enter Git | Closed by placing provider directories outside every working tree. The response correctly qualifies the old layout: `/work/.g5` was inside a repository only when that repository occupied the workspace root. |
| R10: projections depend on event delivery | Closed at the design level by authoritative OID comparison at startup and periodically. |
| Shared-folder conflict correction | Closed. The design now states last-writer-wins semantics accurately. |

1. **F1 — [P1] Seed the checkpoint index so tracked files remain tracked.** [§5.4, lines 348–355](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:348)

   The retained `GIT_INDEX_FILE=… git add -A` recipe starts with an empty temporary index. A file tracked by the real index but matched by `.gitignore` is then treated as an ignored untracked file and omitted from `C`. This is a legitimate repository state: a tracked lockfile or fixture can coexist with a broad ignore rule. Restoring `C` deletes it; dirty edits to it are not present in the snapshot.

   **Observed:** created tracked `kept.lock`, added an ignore rule for `*.lock`, edited the file, and ran the proposed snapshot/restore commands. The checkpoint omitted it; restore removed it and `git status` reported `D kept.lock`.

   **Change:** initialize the temporary index from the real tracked-path state, including newly staged paths, before applying working-tree changes. Keep the real index untouched. Ignore rules should suppress untracked files, not silently remove tracked files. Add this case to §15 alongside the now-passing unpushed-commit test.

2. **F2 — [P1] Do not let checkpoint observation advance the agent's implicit lease.** [§5.4, lines 344–345](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:344), [§11.2, lines 1190–1192](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:1190)

   Checkpointing fetches into `origin/<branch>` to record the observed internal head. Profiles prescribe `--force-with-lease` without requiring an explicit expected OID. If a laptop pushes an unseen commit, the background fetch advances the tracking ref; the agent's next implicit-lease push can now erase that commit while following the prescribed command.

   **Observed:** the same push was rejected before the simulated checkpoint fetch, then accepted after it, displacing the laptop's commit. [Git documents this specific background-fetch interaction](https://git-scm.com/docs/git-push).

   **Change:** observe the remote OID without updating the agent's lease-bearing tracking ref, for example with `ls-remote` or a separate observation namespace. Alternatively require an explicit expected SHA captured as part of the agent's rewrite operation. This preserves the accepted owner-rewrite policy; it does not introduce a server rewrite-authorization system.

3. **F3 — [P1] Wake stopped workspaces before filtering for runnable dispatches.** [§7.3, lines 722–727](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:722)

   Task insertion now only inserts `queued` and returns. The dispatch loop first selects tasks whose VM is already `running`, then calls `ensure_vm_running`. A task or chat message for a stopped workspace never reaches that call. Re-offering it every 30 seconds does not change eligibility, so the documented wake-on-message/run behavior does not happen through this path.

   **Change:** select dependency-ready queued work independently of VM state, ensure its workspace is starting/running, and apply the running-VM/concurrency predicate only when issuing the start command. Test a new task and a new chat turn with every VM initially stopped.

4. **F4 — [P1] Replay a start that was recorded but never delivered.** [§7.3, lines 726–727](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:726), [§7.6, lines 771–774](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:771)

   The API records `status=running` before sending the start RPC. If it crashes in between, the VM has never heard of that `exec_id`. Reconnect explicitly classifies this as `failed(vm_lost)`, although the VM is alive and no process ran. Without the opt-in retry the user's task simply fails. This contradicts §15's “disconnect immediately before spawn” criterion of one process run and one outcome.

   **Change:** retain an outstanding start intent and retry it with the same `exec_id` until the dispatcher confirms its disposition. Distinguish “never delivered” from “previously started, execution now lost.” The new idempotent start command makes this possible without per-task leases. Also retain cancellation dispositions so a delayed start cannot resurrect a task cancelled before launch.

5. **F5 — [P1] Keep the draining incarnation authorized for the stop barrier.** [§5.7, lines 443–463](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:443), [§7.6, lines 775–778](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:775)

   Step 1 sets `stopping`; step 5 says any request arriving after step 1 with the old `vm_id` is rejected. The final checkpoint, session harvest and interruption outcomes still come from that same incarnation. Taken literally, this denies the writes required to complete stop. Separately, the reconciler treats any state other than `running` as VM loss, so it can race a legitimate stop and assign `vm_lost` before the barrier records `interrupted`.

   **Change:** separate closing admission from revoking the incarnation. During `stopping`, reject new user work while permitting that incarnation's checkpoint, harvest, terminal reports and acknowledgements. Treat it as a live draining VM until release or actual lease loss. Wait for already admitted file mutations before the final snapshot. Test a reconciler tick immediately after admission closes, a long checkpoint/harvest, and a Save admitted just before stop.

6. **F6 — [P1] Persist answer-delivery intent; a DB transaction cannot deliver an RPC atomically.** [§7.2, lines 701–711](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:701)

   The draft claims the question row is settled in the same transaction that delivers its answer. Postgres and a blocked provider process do not share a transaction. If the process consumes the answer before the database commit and the API crashes, the question reappears as open despite its side effects. Committing first has the opposite gap: the row is answered but the RPC can be lost. Idempotency at a live dispatcher prevents duplicate application there, but does not recover an undelivered answer or settle whether interruption should create a replacement turn.

   **Change:** commit the answer and its intended disposition first, bind delivery to the question's execution identity, then deliver/retry and acknowledge consumption. The MCP tool could instead read the committed answer through the existing API polling path. Either approach needs a durable distinction between delivery to the old process and creation of one successor turn after interruption. Test API loss before/after answer consumption and answer versus stop, including retries.

7. **F7 — [P1] Preserve and reconcile merge side effects after partial success.** [§10.6, lines 1143–1163](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:1143)

   Provider-first ordering is the right revision, but the blanket rule “any failure” → `failed` conflicts with recoverability after provider acceptance. A successful provider merge followed by a failed mirror fetch must retain the accepted result and retry importing it, not be treated as a fresh merge that re-pins its inputs. Recovery also must check that a remotely merged PR corresponds to the pinned source before attributing success to this operation.

   The local-only path has an independent instance of the same partial-success gap: the target CAS can succeed and the API can crash before recording `applied`. Repeating its original CAS then fails, and the specified algorithm reports failure even though the merge happened. **Observed:** an isolated Git CAS succeeded; retry with the same old SHA exited 128 while the target already equalled the intended result.

   **Change:** retain one operation identity and immutable inputs until its outcome is resolved. Persist the local candidate result before its ref update and recognize an already-applied result on recovery. Keep `provider_merged` and its result through import errors; retry/reconcile without re-pinning. Reserve terminal failure for a known rejection or conflict before application. Extend §15 to cover a provider success followed by fetch failure and a local CAS success followed by metadata-commit failure.

8. **F8 — [P2] Retain nonterminal timeline events, not only terminal outcomes.** [§7.6, lines 762–774](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v5.md:762)

   Sequence numbers and duplicate suppression do not recover missing data. Only terminal outcomes are explicitly journaled. During an API disconnect, a live provider can emit tool calls, tool results and partial output; the described reconnect inventory cannot reconstruct those events. Moreover, dropping all out-of-order replays after advancing `last_event_seq` can permanently discard a missing earlier event if replay interleaves with live delivery.

   **Change:** retain the timeline events until durably acknowledged, replay gaps in order, and distinguish duplicate detection from a highest-seen sequence. Use an acknowledgement scheme that accounts for the per-VM sequence space and multiple concurrent tasks. Test an API restart in the middle of a tool call while the VM remains alive and verify the complete timeline after reconnect.

Validation: read the response and v5 contracts, compared v4→v5 decisions, checked relevant existing Git ingress and persistence code, reproduced the corrected unpushed-commit restore, reproduced F1/F2 with temporary repositories, and reproduced the Git side of F7. F3/F4 were also traced with small executable models of the written predicates; these are not backend tests. The distributed failure sequences are design findings, not observed production failures. No backend, original design, or response file was modified.
