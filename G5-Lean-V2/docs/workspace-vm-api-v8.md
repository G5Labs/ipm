# Workspace VM API Surface — v8

> Status: proposed architecture, draft 8, 2026-09-17
>
> Purpose: define the narrow interface between the G5 control plane and a workspace VM. This is the versioned successor of [workspace-vm-api.md](workspace-vm-api.md), aligned with [G5-Lean backend design, draft 8](lean-backend-design-v8.md). It preserves the historical companion and specifies target behavior, not an implementation claim.

## 1. Boundary and workspace authority

The workspace VM is an ephemeral execution environment and working-tree cache. It is not the authority for Git refs, identity, approvals, provider credentials, or durable platform metadata.

| Place | Owns |
|---|---|
| Workspaces control plane | Workspace identity, exactly one `owner_user_id`, org/invite read access, ownership transfer and its audit, lifecycle admission, VM incarnation, flush and restore. |
| Git service | Refs, receive-pack policy, workspace-branch ownership, protected branches, provider publication, and merges. |
| Workspace VM | The live working tree, declared modules, provider/agent processes, interactive terminals, and VM-local observations. |
| Workspace Runtime facade | The only control-plane client allowed to open the VM channel. Other platform systems call this facade rather than the host tunnel or dispatcher directly. |
| Agents / TaskService | Durable tasks, attempts, shared `chat_sessions`, queue visibility, cancellation and terminal task transitions. Human and workflow inputs use `kind=agent`; `kind=preview` retains its service lifecycle. The Workspace module supplies the preview execution. |
| Agent or terminal inside the VM | Native shell and Git operations under the workspace owner. These are local capabilities, not control-plane extension APIs. |

Every VM request is bound to a `workspace_id`, current `vm_id`, actor or capability grant, deadline, and idempotency key. A stale incarnation is rejected at every execution boundary. `owner_user_id` is fixed for the lifetime of an incarnation; transfer releases that incarnation before changing the owner.

The owner always has read access. Other read access follows the org model: org admins read all workspaces; other org members read according to `visibility=org|invite_only` and the invite list. A workspace has no list of writers. Its owner alone may save files, use terminals, send/edit/withdraw chat inputs, answer live native question requests, submit or cancel non-preview tasks, push existing commits, create PRs, record workspace approval decisions, and stop/resume/restart/delete. G5 exposes no user-controlled Commit action; agents make commits through native Git. Admins take ownership before writing. Readers may fork into a workspace they own, and may start or cancel preview tasks. Preview is their only operation that may wake the source workspace.

Base workspaces remain read-only and have no VM; chat, terminal and preview actions offer a fork into a workspace the caller owns. Ownership does not permit direct writes to a base workspace.

### 1.1 Member-repository mapping and fork

A working workspace maps to `[(repo_id, shared_branch_name), ...]`, with one checkout per member repository under the workspace root. Every mapped branch uses the same name; there is no per-repository branch-name override. The proposed workspace name is the exact Git-valid destination branch name, without slug/UUID suffixes.

Workspace fork, including fork from a base workspace, is the only creator of new internal workspace branches. All Create/Fork/Fork and preview/chat/API/CLI entry points call that same control-plane operation. Before creating any destination branch, it checks every member repository (including connected provider branches) for the name and ref-prefix conflicts. Any collision returns `WORKSPACE_NAME_CONFLICT` and asks the user for a different name; it never overwrites, adopts or automatically renames the branch. A failed verification is not a free name.

Design v8 §5.8 defines durable all-member reservations, create-if-absent branch-plus-fork-receipt transactions, idempotent recovery and safe cleanup after partial failure. The destination is usable and may attach only after its whole branch/receipt vector commits. The dispatcher and restore path consume that completed mapping and cannot recreate a missing authoritative ref; native pushes update existing mapped refs only. Base workspaces read existing protected/default refs per repo, whose names may differ; the fork creates the same proposed destination name from that source vector. Repository setup may initialize protected/default refs, but cannot mint a working workspace branch. Per-repo upstream/update targets remain in `workspace_repos.base_branch` and do not override the shared working name. A working-repository member change cannot mint a new branch outside a new checked fork.

## 2. Workspace Runtime API

These operations are exposed through the Workspace Runtime facade, never through a direct host or dispatcher connection. The product API performs current owner/read authorization before invoking them; a module capability grant is not a session-scoped provider grant.

| Surface | Methods | Caller | Contract |
|---|---|---|---|
| Files: read | `fs.list`, `fs.read`, `fs.stat`, `fs.watch` | Any workspace reader, including the owner | Identical live file reads and `file.changed` events. Reads never wake the VM. |
| Files: write | `fs.write(path, content, expected_hash)`, `fs.delete(path, expected_hash?)`, `fs.rename(from, to, expected_hash?)` | Current workspace owner | Explicit editing; mutation preconditions prevent silent editor overwrite. The product save path may ensure a stopped VM, then uses the same `expected_vm_id` for association check and mutation. |
| Terminal | `terminal.open(cols, rows)`, `terminal.write`, `terminal.resize`, `terminal.close` | Current workspace owner | Interactive shell, never a platform-extension mechanism. Readers cannot attach, write or close the owner's terminal. |
| Modules | `modules.list`, `modules.describe`, `modules.invoke(exec_id, module_id, version, operation, input, grant_id)`, `modules.status`, `modules.cancel` | Authorized platform feature through the facade | Sole platform execution/extension path. No arbitrary `argv`, environment, cwd or signal targets. Reader task creation/cancellation is limited to the persisted `preview` kind and the corresponding fixed module; it cannot authorize an arbitrary invocation. |
| Ports | `ports.list`; `port.opened`, `port.closed` events | Any workspace reader | Observes declared or detected services and permits authorized relay access. Does not start processes or wake the VM. |
| Workspace status | `workspace.status.get` | Any workspace reader; support/operators within their grants | Lifecycle state, current or last `vm_id`, placement, lease freshness, and latest flush receipt with an explicit freshness label. |
| Runtime inspection | `runtime.inspect.summary` | Any workspace reader or support grant | Bounded, redacted live identity, boot time, uptime, resource pressure, roots, ports and fixed health-probe results. No read-triggered wake. |
| Module diagnostics | `modules.logs(exec_id, cursor?)`, `modules.events(exec_id, cursor?)`, `modules.history(filter)` | Any workspace reader or support grant | Bounded, redacted module logs and events, invocations, limits, outcomes and failure classification. No raw host process control or credentials. |
| Diagnostic bundle | `diagnostics.capture(profile)`, `diagnostics.probe(name)` | Support grant; workspace owner for safe profiles | Runs a declared read-only collector such as `workspace-integrity`, `runtime-health`, `port-connectivity`, or `module-catalog-integrity`. No caller-supplied shell. Readers may read permitted diagnostic results but cannot invoke modules through this surface. |

While the VM is stopped, file reads come from the last complete flush snapshot, otherwise the branch head. Status and diagnostics return only durable or last-reported data, labeled `stopped` or `last_reported`, and never pretend it is a live inspection. The same read policy applies to owner and readers.

Workspace search is absent: no workspace-wide search API, index, embedding pipeline, search database, settings or tooling is exposed by this boundary. Chat-history search, editor Find and an agent's local code-search tools remain; docs and ontology projections still serve their retained product surfaces.

## 3. Declared module runtime

`proc.spawn` is a private dispatcher implementation detail. Every platform-initiated executable unit is a declared module invocation.

| Module declaration field | Requirement |
|---|---|
| Identity | Stable `module_id`, semantic version, immutable manifest hash. |
| Contract | Named operations plus typed input, output, event schemas. |
| Execution | Pinned entrypoint and lifecycle class: `bounded_run` or desired-state `service`. |
| Limits | CPU, memory, disk, output/log bounds and appropriate concurrency/time policy. Preview services have no wall-clock limit and consume no workspace bounded-task concurrency slot. |
| Capabilities | Requested workspace scope, bounded ports, network-egress policy, Git operation scope, provider-turn access and related privileges. |
| Observability | Declared health probes, diagnostic profile, event kinds and redaction policy. |

The dispatcher resolves the immutable catalog entry, validates input, and intersects requested capabilities with the control-plane-issued `grant_id` before using its private runner. A manifest requests capabilities; it never grants them. The runtime image and module catalog are signed or hash-pinned. Workspace configuration may select/configure an installed module but cannot install code or expand privilege. The preview's no-tracked-write rule is a repository-profile contract, not an enforced filesystem isolation guarantee (§3.1).

Each invocation is auditable as:

```text
(workspace_id, vm_id, exec_id, actor, module_id, version, manifest_hash,
 grant_id, input_digest, capability_grant, outcome)
```

`modules.invoke` is idempotent on `exec_id = (task_id, attempt, vm_id)`. Its dispositions are `started`, `already_running`, `already_finished(outcome)`, or `cancelled`. Cancellation records a durable disposition even before a process exists, so delayed invoke delivery cannot launch cancelled work. Dispositions last for the incarnation, independently of journal pruning. Module events/outcomes use ordered journal replay, acknowledgement and incarnation fencing.

| Module | Operations | Notes |
|---|---|---|
| `workspace.preview` | `start(profile)`, `stop`, `status` | Service backing a durable `preview` task; declares its port and relay contract. |
| `workspace.changes` | `get`, `push`, `update`, `switch_back` | Normalized association, dirty/ahead/behind/conflict state and owner-authorized mutations. Push sends existing commits; no manual Commit operation is exposed. |
| `agent.turn` | `run`, live `input.respond` | One Claude or Codex attempt for a shared-session `agent` task through [Agent API Surface v8](agent-api-surface-v8.md), including native provider question contracts. |
| `workspace.bootstrap` | `run` | Deterministic setup and `command` tasks. |
| `workspace.diagnostics` | `capture(profile)`, `probe(name)` | Fixed read-only diagnostic collectors. |

### 3.1 Preview task and service

A preview button creates a durable `tasks.kind=preview` row and returns its task identity. The normal dispatch path wakes a stopped non-base workspace, handles `queued_for_capacity`, waits for readiness, checks workspace/branch association on that incarnation, and invokes `workspace.preview start(profile)` with the task execution identity. Task queue and timeline show the launcher's user id, start/stop state and cancellation controls. Preview remains running until cancelled, replaced, failed, or stopped with its VM; module start success is not terminal task success.

Any current workspace reader, including the owner, may launch and cancel any preview. Authorization uses current read access plus the stored task kind, never a client-supplied claim that another task is a preview. Generic task/job cancellation and module invocation APIs must preserve this restriction; readers cannot cascade cancellation into agent or command tasks.

One preview may be active per workspace. Starts, replacement and cancellation serialize under the workspace preview/lifecycle admission lock. A replacement records cancellation for the previous task, settles any ambiguous start and confirms the old execution is stopped before launching the new one. Racing starts or cancel-before-start delivery cannot leave two services or restart a cancelled preview. Ambiguous invocation retries retain their task/execution identity. The service designation never authorizes restart after a process failure; unexpected service exit fails the task and needs a new explicit preview start. Stop/transfer holds admission closed, so a preview cannot wake or launch during the lifecycle barrier.

Preview runs under workspace identity using workspace secrets and the current owner's provider context if needed; `created_by` is launch audit, never credential selection. The selected command comes from the repository's preview profile and must not modify tracked files. G5 does not enforce that contract in v8: any tracked changes appear in Changes and are included in a later flush. Discarding writes with an overlay is future hardening.

Preview does not consume the workspace's bounded-task concurrency slot, has no wall-clock limit and never restarts automatically after stop/resume, transfer, VM loss or rollout. A fresh explicit preview start creates new demand. Preview traffic counts as activity; a running preview process alone does not disable the org idle timeout or its workspace override. This removes dedicated preview VM/session/attach machinery, not the `preview` task kind or the task-list cancellation surface.

### 3.2 Shared agent task lifecycle and live questions

Human chat input and templated workflow input both enter the same `chat_sessions` lifecycle and dispatch `tasks.kind=agent` through `agent.turn`. Input origin is typed provenance, not a separate execution engine or permission policy. The server records permission policy explicitly, independently of whether the input came from a person or a workflow. The Agent API companion owns the field-level contract.

One nonterminal logical agent task occupies a session, even while held with no provider process. New session inputs remain queued behind it; only explicit owner Continue/Resume clears a hold. A continuation retains the same logical `task_id`, session, dependency edges and job grouping; each new process gets a new attempt and `exec_id`. A waiting or interrupted attempt does not complete the task or release dependent work. Dependencies unblock only on the logical task's final `succeeded` state; final failure or cancellation follows the existing dependency failure/cancellation policy without pretending success. Preview remains a separate service task and does not inherit agent continuation behavior.

Claude's direct CLI emits native `AskUserQuestion` requests into the timeline; the current owner answers through the CLI's native stdin control protocol while that process and attempt remain live. MVP Codex `exec --json` has no interactive-question transport and no custom MCP fallback; a later App Server stdio adapter supplies its native interactive path. The provider adapter owns native framing; the facade exposes only its declared `modules.input.respond` channel for validated, owner-authorized replies to a live attempt, never arbitrary stdin or a new host RPC. Unrelated MCP tools remain available.

Native questions and permission requests are projected into `provider_requests`, keyed by `(exec_id, provider_request_id)` with states `pending|reply_pending|responded|invalidated` as defined by Agents. They are bound to the live task attempt and incarnation. Ending that process invalidates every unanswered or undelivered request, including one whose answer was recorded but not delivered before exit. Their timeline history remains readable; a stale request ID cannot be answered, replayed to a replacement or used to wake a VM. A wait timeout suspends the attempt and leaves the logical task waiting for owner Continue. User stop or transfer holds unfinished work for explicit owner action; drain/rollout may continue the same task automatically under the lifecycle policy. A continuing task runs another attempt and lets the provider ask again with a new request ID. Only an active live wait contributes question-related activity; an expired historical card cannot keep a workspace awake.

## 4. Private lifecycle protocol

Only the lifecycle service, Workspace Runtime facade, dispatcher, host and provider runner use these reliability operations.

| Protocol | Responsibility |
|---|---|
| `runtime.ready`, `lease.renew`, `health` | Establish VM custody, renew the current incarnation and report liveness. |
| `events.replay`, `events.ack`, `outcomes.pending` | Replay/acknowledge journaled module events and outcomes. |
| `session_files.restore`, `session_files.harvest` | Restore/persist provider session files outside the working tree. |
| `modules.input.respond(workspace_id, expected_vm_id, expected_epoch, task_id, attempt, exec_id, provider_request_id, reply_version, validated_response_ref, idempotency_key)` | Deliver an owner-authorized native response only to its live `agent.turn` request; the Agent API companion defines dispositions and idempotency. It neither wakes a VM nor accepts raw stdin. |
| `admission.close`, `admission.open` | Close and drain mutation/module admission; reopen after a failed lifecycle operation. |
| `workspace.flush` | Establish a durable complete workspace cut in internal Git for shutdown or fork only. |

### 4.1 Ownership transfer

The current owner may transfer to any org member; an org admin may transfer to an org member, including themselves. A transfer operation carries a durable idempotency key and records old owner, new owner, requester and completion time. Workspaces holds the lifecycle/admission lock from the start through stop/release and the owner/audit transaction.

Transfer uses the stop sequence with `stop_reason=transfer`: close admission, interrupt executions, close previews and terminals, persist attempt outcomes, task continuation state and Agent session state, invalidate unanswered or undelivered native requests from ended processes, flush with reason `shutdown`, confirm host exit, release the incarnation, then atomically change `owner_user_id`, set `owner_resume_required=true` and complete the persisted transfer operation/audit. A stopped workspace skips VM work. An ambiguous host stop must be reconciled; it is not permission to change ownership while the old VM can run.

Flush failure aborts transfer, restores admission and leaves the old owner unchanged. Interrupted execution state remains truthful; abort does not silently relaunch a preview. No forced-loss path completes ownership transfer. A retry of a completed transfer returns its recorded result without a second owner change or audit.

Chats, tasks, PRs, queued inputs and question history remain with the workspace. Transfer never wakes a VM or dispatches queued work or continuation attempts. Only the new owner's explicit Resume clears persisted `workspaces.owner_resume_required` before pending work runs using their provider account, even when a reader preview already made the VM running. An agent continuation retains its task/session/dependencies and uses a new attempt; an ended attempt's question stays read-only, and the provider must issue a fresh native request if it still needs an answer. A fresh reader preview start may wake only its new preview demand; it does not release the owner-resume hold on pending agent/command work. The previous owner retains only the read access independently granted by org visibility/invites.

## 5. Workspace flush

There is no asynchronous checkpoint task. Agents normally commit and push through native Git. Uncommitted edits and unpushed commits may be lost on unplanned VM loss.

```text
workspace.flush({ reason, interrupt? }) -> FlushReceipt
reason = shutdown | fork
materialization = snapshot

FlushReceipt {
  flush_id,
  workspace_id,
  vm_id,
  epoch,
  repos: [{ repo_id, branch_oid, local_head_oid, snapshot_oid, dirty }],
  concurrent_writers,
  complete_at
}
```

The lifecycle service alone requests a flush for shutdown or fork. There is no Flush now button, explicit-flush route or user-triggered checkpoint action. A flush creates lifecycle snapshot objects, not a user commit on the workspace branch. It closes module and mutation admission, drains in-flight file writes, and captures all member repositories using a temporary index without modifying the real index, local `HEAD` or workspace branch. It observes the internal branch without fetching, pushes immutable `refs/g5/flush/<workspace_id>/<flush_id>` refs, and completes the receipt only after the API verifies every required ref. Consumers never use a partial cut.

| Consumer | Required cut | Follow-up |
|---|---|---|
| Graceful shutdown or ownership transfer | Quiet tree after interrupted executions and closed previews/terminals | Release only after a durable receipt and durable Agent stop state, except the design's separately recorded deadline-bound teardown loss policy. Transfer never uses that exception. |
| Workspace fork | Exact multi-repository snapshot including dirty and unpushed work | Check/reserve the proposed shared name in every member, then fork from the receipt through the control-plane operation; a reader uses noninterrupting flush and cannot interrupt owner work. |

A live fork flush waits for bounded executions up to `quiesce_secs` (default 30), then returns `WORKSPACE_BUSY` unless the owner authorized interruption. Live terminals/previews are reported in `concurrent_writers`; a fork requiring an exact cut cannot claim one while they can mutate the source. Readers wait, retry after quiescence, or seek owner action; their fork privilege does not authorize stopping terminals or interrupting agent work. Readers can separately cancel previews under the preview policy. Both admissions reopen on completion, refusal or failure.

A user stop, restart, delete, transfer or fork aborts on failed flush; idle reclamation cancels the stop and restarts its timer. Deadline-bound operational drain/rollout/credential-expiry teardown may proceed only with the explicitly recorded loss required by the design. PR creation never invokes flush or creates a commit: agents commit through native Git, Push sends existing commits, and the Git service publishes the pinned branch OID.

Restore uses the last complete receipt's local head and snapshot without auto-merging newer remote changes; without a receipt it uses the branch head. Warm roots are caches, never an alternative authority.

## 6. Explicitly absent surfaces

| Absent surface | Replacement |
|---|---|
| `proc.spawn`, `proc.signal`, `proc.wait`, `proc.list` | Private runner after module resolution. |
| `git.*` | Native Git in the VM, Git service refs/policy, and `workspace.changes`. |
| `agent.run_turn` | `modules.invoke(..., "agent.turn", "run", ...)`. |
| Raw `preview.start`, `preview.stop`, `preview.status` dispatcher methods | Durable preview task API dispatching the `workspace.preview` module. The product preview button and task cancellation remain. |
| `workspace.sync`, `checkpoint.run`, periodic checkpoint tasks | Agent pushes plus privileged `workspace.flush` at durability boundaries. |
| Commit button, commit API/CLI and `workspace.changes commit` | Agent-native Git commits; Push and Publish operate on existing commits. |
| Flush now, explicit-flush API/CLI and `reason=explicit` | Lifecycle-only `workspace.flush` for shutdown or fork. |
| Workspace search UI/API/indexing/embedding/search database/settings/feature tests/tooling | Retain chat-history search, editor Find, agent-local code search and docs/ontology projections with their coverage. |
| Custom `g5-tools ask_user`, MCP `request_permission` adapter and `g5d ask` | Provider-native live questions and approvals through Agents; no Codex MVP MCP question fallback. Unrelated MCP tools remain. |
| Arbitrary diagnostic shell execution | Fixed probes and versioned bundles. |
| Workspace writer list; session-scoped provider grants | One owner and owner-bound incarnation credentials. Session ids remain accounting tags. |

## 7. Design invariants and acceptance

1. Platform execution uses declared modules through the facade. Readers can invoke only the product's fixed preview-task path.
2. Fork is the only member-branch creation path: the exact proposed name is shared across all members, collision in any repo rejects the fork, and partial creation never yields a runnable workspace. Durable cuts come from complete verified flush receipts. Ownership commits only after the old incarnation is released.
3. Reads are identical for owner/readers, bounded and freshness-labeled, and never wake a VM. Read-only inspection does not expose raw process control.
4. Preview tasks are durable, visible and cancellable before/after start; replacement, cancellation, retries and teardown cannot create duplicate active services.
5. Owner-only authorizations are checked at admission, including generic task, chat, file, terminal and Git boundaries. Old-owner requests cannot succeed after transfer.
6. Transfer failure, API restart between release and owner/audit commit, stale invocation replay, answer-versus-process-exit/transfer and explicit resume under the new account require boundary-level regression coverage. An undelivered answer cannot reach a replacement process; continuation preserves task identity and dependencies, and fresh provider questions use new request IDs.
7. Human and workflow inputs use the same agent task/session path with independently explicit permission policy. Test that interrupted/waiting attempts never release job dependencies, and that Claude native question replies reach only the live requesting attempt while MVP Codex exposes no fallback question route.
8. Removed Commit, explicit flush, workspace search and custom ask surfaces are absent end to end; Push/Publish, lifecycle shutdown/fork snapshots, chat-history search, editor Find, agent-local code search, docs/ontology projections and unrelated MCP remain. These are implementation gates, not claims that implementation or deployment has passed them.
