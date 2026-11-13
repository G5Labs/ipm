# G5-Lean backend re-architecture — design (draft 8)

Status: draft 8, 2026-09-17, candidate for acceptance. Supersedes [lean-backend-design-v7.md](lean-backend-design-v7.md); draft 7 and earlier versions remain unchanged. This draft is self-contained and incorporates the user’s preview correction, [workspace-sharing clarification](workspace-sharing-v8.md), and subsequent clarification that a workspace enforces one shared branch name across its member repositories and creates those branches only by forking. It also incorporates the subsequent scope revision: remove workspace search and user commit/flush controls, unify agent sessions/tasks, and use provider-native interaction protocols. It describes the target architecture, not implemented or deployed behavior.

Three companion notes are normative for the surfaces they define and are adopted here at the level of the design; they hold the field-level detail that this document summarizes:

- [workspace-vm-api-v8.md](workspace-vm-api-v8.md): the Workspace Runtime API of a workspace VM, the declared-module runtime, the private control-plane protocol and the private `workspace.flush` operation.
- [worker-host-api-v8.md](worker-host-api-v8.md): the worker host protocol, `VmSpec`, the host credential and lease, placement, and the host-side proxies.
- [agent-api-surface-v8.md](agent-api-surface-v8.md): the contract cut between Agents, Workspace and Worker & Host, the `agent.turn` module, the provider-neutral event and outcome schemas, the inbox and question states.

Where this document and a note differ, this document says so in place and wins; the notes are then to be amended.

Implementation state at this draft: Phases 0 and 3 of the draft 6 plan and most of Phase 1 are on `main` (git refs authoritative, receive-pack policy, projection convergence, head-operation registry deleted, native worker push, native-git profiles, the Rust provider runner, `task_questions`, `chat_session_files`, `naive_react` removed, IDE mockup and non-GitHub login removed). The QA GCP project, its Cloud Build lanes and the frozen schema baseline with forward migrations are in place. Everything in §5.4 onwards that describes the workspace VM, the host protocol and the Agents service is not implemented yet. The delivery plan is [lean-implementation-plan-v8.md](lean-implementation-plan-v8.md). The predecessor was reviewed as [lean-backend-design-v7-review.md](lean-backend-design-v7-review.md); its findings R1 to R5 were addressed in draft 7; their reliability invariants remain, with native-request lifetime and task-continuation mechanics superseded by the latest requirements (R1 in §8.1 and §8.2, R2 in §7.2 and §8.4, R3 in §5.4, R4 in §5.8 and §6.2, R5 in §5.5).

---

## 0. What changed since draft 7

1. **Preview remains a task.** A Preview button creates a durable `tasks.kind = preview` row and launches `workspace.preview` through normal dispatch. Its launcher, state and Cancel action are visible in the workspace task queue. The per-task preview VM and legacy session registry are removed; the preview task is retained. (§6.8, §7)
2. **Exactly one human writer.** `workspaces.owner_user_id` is the write authority. Org admins may read any workspace; other members read according to org visibility or the invite list. All readers have the same read surfaces as the owner and may fork into a workspace they own. There is no membership write role. (§5.1, §5.3, §9, §10.4)
3. **Preview is the reader exception.** Any reader may start or cancel any preview, including replacing another user’s preview. It runs under workspace identity and secrets, with the launcher recorded only for audit. One active preview per workspace; no workspace concurrency slot, no wall-clock limit, no automatic restart after a VM stop/resume. Preview traffic counts as activity, with the normal org idle default and workspace override. (§6.8)
4. **Transfer replaces adding writers.** The owner may transfer to any member of the org; an org admin may transfer to anyone in that org, including themselves. Transfer runs the stop sequence with reason `transfer`, then changes the owner and records both users and the time. A failed flush aborts with the old owner unchanged. The new owner explicitly resumes with their provider account; history stays with the workspace; ended native questions remain visible but must be reissued by a resumed provider before answering (§8.4). (§5.8)
5. **Authorization follows ownership everywhere.** Save, Push (including laptop receive-pack), chat/inbox/questions, terminal, lifecycle actions, PR creation and workspace approval recording require the current owner. An org admin takes ownership before writing. Concurrent agents started by that owner still share one folder. (§5–§11)
6. **One owner’s provider account per VM.** Session-scoped provider grants and per-message author labels are removed. Session IDs remain useful for routing and usage accounting, not credential selection. Transfer revokes the old incarnation before any new-owner process can run. (§6.3–§6.5, §8)
7. **Shared branch names are enforced.** A working workspace maps to a vector of `(repo_id, branch_name)` entries with the same branch name in every member repository. Fork is the only creator of new workspace branches. The proposed workspace name is the exact branch name; a collision in any member repository rejects the whole fork and asks for another name, without an automatic suffix. (§5.1–§5.3, §5.8)
8. **Search and manual durability controls are removed.** Workspace search is removed end to end; chat-history search, editor Find, agent-local code search and document/ontology projections remain (§9.5). Agents commit regularly; there is no Commit button/API/module operation or Flush now action. The private flush has only `shutdown` and `fork` reasons (§5.4, §9.2).
9. **One agent task/session model.** Human typed input and workflow templates initialize the same `agent` kind and conversation lifecycle. Jobs are optional; continuations retain the logical task, job and dependency identity. Policies are explicit, never selected merely because work came from a chat or a job (§7–§8).
10. **Native provider interaction.** Remove `g5-tools.ask_user`, `g5d ask` and their custom poll/late-answer engine. Claude stays on its direct CLI using native `AskUserQuestion` and control replies; Codex MVP stays on noninteractive `exec --json` with no custom question fallback. A later scheduled phase migrates Codex to `app-server` over stdin/stdout for native questions/approvals (§8.3–§8.7). Unrelated MCP tools remain.
11. **Delivery is versioned.** The v8 plan and three API companions are new files, with existing PR IDs retained. Earlier designs, plans, API notes and the v7 status tracker remain intact; the v8 status tracker carries merged evidence forward and updates remaining dependencies. The v7 review corrections remain in force.

The latest scope requirements override the sharing source note only where it assumed an old native question could remain answerable after its process ended; history and task/session continuity still survive transfer. The source snapshot and security review of candidate `6191bdb` are preserved as historical evidence, not rewritten to claim the reviewed gaps are fixed.

This explicitly reverses the earlier team decision to allow multiple writers. Pairing requires transfer or a fork; reviewing with an agent requires a fork. The implementation rollout must tell the team about that changed collaboration model. A reader cancelling the owner’s preview is an accepted nuisance; owner unavailability is mitigated by admin transfer.

---

## 1. Summary

G5 today runs one microVM per task attempt, keeps uncommitted edits in a server-hosted CRDT, kept branch heads in Postgres until Phase 1 made git refs authoritative, keeps chat VMs alive between turns through a sticky-runtime protocol, and layers approvals, merge runs and remote-tracking sync on top. The target is a developer-laptop model: one VM per workspace, ordinary files in each member repository on the workspace’s shared branch name, native git, agents that commit and push their own work, and a control plane that only steps in at the boundaries. The VM is not as persistent as a laptop: it is terminated when idle and re-provisioned on request, and when G5 terminates it gracefully it first flushes uncommitted state to internal git so the next VM can pick up where the last one left off.

| # | Proposal | Section | What it replaces |
|---|---|---|---|
| 1 | One VM per workspace; tasks are processes launched by a dispatcher through declared modules | §6, §7 | per-task VM boot, materializer, host poll/claim/ack, golden images |
| 2 | Remove the CRDT layer | §9 | `g5-crdt`, both CRDT peers, drafts, task-commit protocol, yjs in the UI |
| 3 | Internal git as persistence: agents push; G5 flushes at graceful teardown; restore from a receipt | §5.4, §5.5 | staging refs, Postgres-owned heads, the draft 6 checkpoint cron |
| 4 | Native git for agents; a small platform surface | §11 | ~20K lines of CRDT-bound CLI (already deleted), internal tools |
| 5 | Lightweight workspace↔branch check | §5.7 | git blocklists, rebase pause files, authority-scope typing |
| 6 | Governance at the git boundary | §10 | 11-table approval ledger, job-merge runs, PR provider lifecycle |
| 7 | Explicit-Save editing, readable while the VM sleeps | §9 | draft sessions, presence, CRDT-anchored comments |
| 8 | Workspace chat history shared for reading; owner sends, no process between turns, queued messages | §8 | sticky runtimes, worker pins, SDK sidecar, context links |
| 9 | Preview task on the workspace VM, one active per workspace, visible and cancellable in the task queue | §6.8 | per-task preview VM, sessions table, reaper, chat-attach machinery |
| new | Claude Code and Codex only; one CLI process per turn; a resident process per session as a later option | §8.3, §8.7 | the SDK sidecar (deleted), `naive_react` (deleted) |
| new | Declared modules, host protocol and proxies, Agents as a bounded context | §6, §7, §8 | ad-hoc RPC surfaces, per-task credentials in the VM, host-side task logic |

The draft 6 deletion estimate was on the order of 250K to 300K of roughly 1.1M Rust lines. The latest removals require a fresh measurement; the retained inventory in §12 and one execution model define the target rather than a fixed table or line count.

---

## 2. Goals, constraints, non-goals

**Goals.** Simplest possible backend; easy to keep stable; the nine proposals; laptop-like developer experience with a first-class workspace that maps to one branch per member repository, using the same branch name in every entry; UI like GitHub Codespaces; a dispatcher core that is small, well tested and never edited for features once it ships, with features as modules outside it.

**Constraints.**
- Incremental: the existing source is modified and the frontend components reused; every PR leaves a buildable, testable product; vestigial references to the old architectures are removed as they are met.
- The control plane remains one deployed binary; a split into separately deployed services is not attempted now.
- Schema: the baseline is frozen; every change is a forward migration; no data compatibility with the original G5 platform is required, but dev and QA databases are not disposable (§12).
- Providers: Claude Code and Codex, pinned CLIs (`scripts/claude-code-cli-version`, `scripts/codex-cli-version`), one process per turn. No long-lived agent process between turns in the MVP. The Claude Agent SDK is not used: the runner drives the CLI's own stdio protocol, which is what the SDK wraps; the SDK's published type definitions may serve as a schema source (§8.7).
- Platform login: GitHub in the SaaS; Microsoft Entra for self-hosted installations returns in the self-hosted phase (§16). Repository providers: GitHub and GitLab, and local-only repos.
- Deployment targets: the GCP SaaS (GKE control plane, GCE worker hosts, the QA project as CI/CD), self-hosted Linux hosts on pre-provisioned VMs with the Azure canary lane, macOS hosts for development only. Customer-specific worker images are dropped.
- Ontology and grounding: the G5-Lean codebase is developed without its own ontology and grounding gate; the ontology product surface (uplift, downlift, sync, grounding tools, ontology projections) is retained in the code and decoupled from git and CRDT as in draft 6, but it is not part of the MVP acceptance and is revisited afterwards. This is the reading this draft takes of the team's decision to remove the ontology during MVP development; it needs confirmation.
- External CI integrations are postponed; agents run tests in the workspace VM with local compute.
- Credentials: environment variables remain the mechanism for repository and task secrets in the MVP (§6.5); provider, git and platform credentials never enter a provider process (§6.5).

**Non-goals for this iteration.** Real-time co-editing or presence (a document-only CRDT can come later behind the Save API); hardening beyond today's microVM boundary and the credential rules in §6.5; new cloud providers; tracking of provider subagents (the provider process is the unit); interactive Codex questions and approvals in the MVP (scheduled stdio app-server phase, §17.2); live steering of a running turn (a resident-process capability, §8.7); agent-initiated workspace creation (§17.3); multiple API replicas (§17.2); a CI service outside workspaces.

---

## 3. Starting point

### 3.1 What is on `main`

| Area | State on `main` (2026-09-16) |
|---|---|
| Git boundary | Reads resolve heads from `refs/heads/<branch>`; receive-pack enforces the policy table for protected refs and workspace branches; `projection_state` and the convergence loop; the head-operation registry, staging refs and finalizer are deleted. Remote tracking still exists (P1.5). |
| Worker | Runs the provider directly, commits with `git add -A` semantics and pushes `HEAD` to the assigned branch with its token; the CRDT peer and the CRDT-bound CLI are gone from the worker; profiles describe native git. The backend CRDT layer and the frontend editors' CRDT binding still exist (P2.1, P2.2, P2.4). |
| Provider runtime | One Rust runner per turn for both CLIs (`backend/g5d-worker/src/runner/`), both Node sidecars deleted, fake `claude` and `codex` binaries, legacy `task_questions` with `ask_user` and `request_permission` by answer polling (removed by this revision), `chat_session_files` harvest and restore, provider enum `claude_code \| codex`. |
| Execution | Still per-task microVMs, sticky runtimes and the 12-state kernel; the workspace VM, dispatcher, host protocol and Agents service of this draft are not built. |
| Schema | `001_lean_baseline.sql` frozen at `6d3936c` with a checksum guard; forward migrations from `002_*` on. |
| CI/CD | QA GCP project `g5-qa-1ccc`: Cloud Build lanes `g5-ci-rust` and `g5-ci-e2e` on every PR, `g5-deploy-dev` and `g5-packer-worker-main` on pushes to `main`, Terraform roots under `infra/qa/`, worker image bake and repin, migration checksum guard, QA reset runbook. |
| Removals | IDE mockup, hidden settings UI, takeover remnants, non-GitHub platform login. |

### 3.2 Cut list

What remains to be removed or replaced, with sizes from the survey (`docs/research/`):

| Subsystem | Disposition |
|---|---|
| CRDT layer (~47K backend + ~12K frontend) | Remove (§9) |
| Remote tracking (~9K, 8 migrations' worth of tables) | Remove; fetch protected branches, publish on explicit action (§5.6) |
| Sticky runtimes (~13K, 27 migrations' worth) | Remove (§8) |
| Per-task execution: supervisor poll/claim/ack, materializer, repo cache, rootfs COW, handoff, capacity bookkeeping (~22K) | Replace with the workspace VM lifecycle, host protocol and dispatcher (§6) |
| Golden images (~10K) | Remove; caches live on the VM disk and in retained roots (§6.3) |
| Task lifecycle kernel (33-command kernel, 12 statuses, leases, fences; ~120K with `task_service.rs`) | Replace with the six-state model and the dispatch and event contract (§7) |
| Governance (approval ledger, job merge, coordinated publication, PR lifecycle; ~25K) | Replace with protected refs + `pull_requests` + rules + decisions + one merge operation (§10) |
| Legacy preview execution (~6K; relay kept) | Preserve `preview` as a task kind and queue entry; run its service in the workspace VM; remove separate preview VM/session machinery (§6.8) |
| Fleet bring-up specific to task pools, 24 h host JWT | Simplify to slots on hosts with a renewable credential (§6.2) |
| Internal issue tracking (~10K, 13 tables) | Remove (approved) |
| Chat context links | Remove |
| Workspace search | Remove UI, routes, background search indexing, embeddings, search-only data/settings, tests and operational tooling; preserve the distinct capabilities in §9.5 |
| Manual commit/flush product controls | Remove Commit and commit shortcuts/handlers; remove Flush now and `explicit` flush reason; agent commits and lifecycle snapshots remain |
| Separate `chat_turn` kind and custom ask-user transport | Replace with shared `agent` sessions/tasks and provider-native requests; retain unrelated MCP |

The approved deployment-gate removals of draft 6 §3.2 stand as landed (issue tracking pending as P8.1). One of them is partly reversed later: Microsoft Entra platform login returns for self-hosted installations in the self-hosted phase (§16.3).

---

## 4. Target architecture

### 4.1 Components and owners

Ownership follows the capability, not the process a component runs in. Four capabilities own the system; three have named owners in the plan, the fourth is assigned by the team.

| Capability | Owns | Runs in |
|---|---|---|
| **Workspace** | Workspace registry and members, `workspace_vms` and incarnations, the six actions including ownership transfer and the stop sequence, the flush and restore, the Workspace Runtime facade, the dispatcher (`g5d dispatch`) and the module runtime, the built-in `workspace.*` modules (changes, preview, bootstrap, diagnostics), the workspace shell UI (files, Save, Changes, terminal, ports, preview) | g5-api, the VM |
| **Agents** | Tasks, attempts and jobs, chats and the inbox, questions and permissions, provider accounts and workspace-owner credential bindings, session files, usage, the provider adapters and the `agent.turn` module, the chat and task UI | g5-api, the VM (module only) |
| **Worker & Host** | `g5d host`, the `ctl` protocol on both ends, `worker_hosts` and host states, placement and quotas, `VmSpec` and images, the host proxies, drain and rotation, the operator API, the runtime image and the pinned provider binaries in it | the host, g5-api |
| **Git and governance** | The internal git service and `RepoStorage`, receive-pack policy, the flush-ref namespace rules, protected-branch mirroring and publish, pull requests, approval rules and decisions, the merge operation, the projections' convergence | g5-api |

The control plane is one binary (`g5-api`) with these four bounded contexts as crates or modules with internal interfaces; a split into services is not a goal now. The frontend follows the same ownership per panel.

**g5-api (control plane).** Postgres-backed. Owns identity and orgs, the four capabilities above, the internal git server, the tunnel registry, the browser WebSocket, and the preview and terminal relays.

**Internal git store.** One bare repo per connected repository under `RepoStorage` (local disk for self-hosted, Filestore for the SaaS). Holds mirrored protected branches, every workspace branch, and the immutable flush refs. Git refs are authoritative for files.

**Worker host (`g5d host`).** A Linux machine with KVM (GCE VM, a self-hosted VM with nested virtualization, or a macOS dev host with HVF). Registers, dials one outbound tunnel, boots and stops equal-size workspace VMs on command, relays streams, runs the LLM and credential proxies, and enforces the per-VM lease. No task logic, no knowledge of branches or approvals.

**Workspace VM.** One libkrun microVM per non-base workspace, attached on create or resume and terminated by the stop sequence or by loss. Each attach is a new incarnation (`vm_id`, `epoch`). Its disk is a cache. It runs the **dispatcher** (`g5d dispatch`): restores the workspace from the last flush receipt, serves the Workspace Runtime API, runs declared modules, journals their events, and performs the flush when asked. Chat turns, agent tasks, the preview and terminals are processes in this VM, in one folder, on the workspace branch.

**Frontend.** The React shell with a Codespaces-like center: file tree and editors with explicit Save, a Changes panel, a terminal, Ports and Preview, chat with a queued-message strip, tasks, PRs and approvals.

**Providers.** The pinned `claude` and `codex` CLIs on the VM's runtime mount, launched by the `agent.turn` module, one process per turn. Model traffic goes through the host proxy using the workspace owner’s account, with a per-session URL for accounting; no provider key or platform token is in the VM.

### 4.2 Diagram

```
 browser ──HTTPS/WS──► g5-api ◄──HTTPS──► GitHub / GitLab (protected branches in, PR publish out, provider merges)
                        │ ▲
      Postgres ◄────────┘ │  internal git: /git/<repo_id>  (smart HTTP, policy in receive-pack)
   (metadata, chats,     │
    inbox, questions,    │  bounded contexts: Workspace · Agents · Worker & Host · Git and governance
    session files,       │
    flush receipts)      │
                 outbound tunnel (WS + stream mux; the host dials the API; streams ctl · rpc · pty · http)
                          │
                    g5d host (per machine, N equal VM slots, host credential, lease timers)
                     ├─ libkrun boot/stop from VmSpec, OCI rootfs templates, retained workspace roots
                     ├─ proxies (g5-llm): model traffic per session, git and platform credentials injected on egress
                     └─ per VM: passt network, stream relay
                          │
                 workspace VM  (one per workspace, incarnation vm_id/epoch, /work = checkout on the workspace branch)
                     └─ g5d dispatch
                          ├─ Workspace Runtime API: fs.* · terminal.* · modules.* · ports.* · workspace.status · diagnostics
                          ├─ private: runtime.ready · lease.renew · events.replay/ack · session_files · admission · workspace.flush
                          ├─ modules: agent.turn (Agents) · workspace.changes · workspace.preview · workspace.bootstrap · workspace.diagnostics
                          └─ journal under /var/lib/g5d; provider state under /home/agent (outside every working tree)
```

### 4.3 Invariants

1. **Two sources of truth.** Files: git in the internal store (branches and flush refs). Metadata, chat messages and timeline, inbox, questions, provider session files and flush receipts: Postgres. The VM disk is a cache. A VM never writes the database; every write goes through the API, and transient VM state (open ports, resource use) is read over the runtime API, not stored.
2. **One working workspace, a vector of same-named repository branches, one VM incarnation, one dispatcher.** Each member repository has its own checkout under the shared workspace root; concurrent tasks use those same checkouts. Only workspace fork creates the authoritative member branch refs; it must first accept one collision-free name across the whole vector. The `workspace_vms` row with its lease, `vm_id` and `epoch` is the only custody record; every boundary rejects an incarnation that is not current (§6.4).
3. **One write path for files.** All file writes happen inside the VM: agents natively, the UI through `fs.write`. The API never writes a working tree.
4. **Branch protection lives in receive-pack.** User-requested protected-ref changes go through the API's merge operation after approval checks; repository bootstrap and provider imports are separate API-only paths. Flush refs accept create-only writes from the workspace's current incarnation or the API's checked fork-copy operation, and deletions only from the API.
5. **An attempt is a process; an agent task is a logical unit.** Bounded attempt completion derives from process exit plus the adapter outcome. Agent continuations retain the same task/session/job/dependency identity; only final logical success releases dependents (§7). A preview is a service: readiness is running, cancellation ends it normally, and unexpected exit is failure (§6.8). Execution identity is `(task_id, attempt, vm_id)`; confirmed starts distinguish intent from execution (§7.5).
6. **Platform code runs only as declared modules.** Nothing in the control plane can spawn an arbitrary process in a VM; a feature that needs to run workspace code declares a module with a contract, limits and requested capabilities (§6.7).
7. **Durability boundaries are the flush and the push.** Pushed commits are durable. Everything else survives only through a complete flush receipt, and a receipt is complete only when every repository's snapshot is retained by an immutable ref that the API has verified (§5.4).
8. **Stop is a barrier.** A VM is released only after the flush receipt and the session-file harvest are durable, or after a deadline-bound teardown has recorded that the flush failed; the draining incarnation stays current until release (§5.8).
9. **No secret in a provider process.** Provider keys, git tokens and the workspace token live in host memory and are attached by the host proxies for the current incarnation only; the dispatcher alone holds the workspace token (§6.5).
10. **Projections converge from git.** Events are hints; every projection can be rebuilt from authoritative OIDs without an event (§9.4).
11. **Nothing crosses the VM boundary at-most-once.** Starts, answers, events and outcomes are persisted on one side and confirmed by the other; duplicates are detected by key, never by ordering (§7.5).
12. **One human writer.** The workspace owner alone mutates it; read access never implies write access. Reader preview start/cancel and a fork into the reader’s own workspace are the explicit exceptions. An ownership change is committed only after the old incarnation is released, with an audit record in the same transaction.
13. **Versions are negotiated, never assumed.** Host, VM image and control plane deploy independently; the protocol version is negotiated per tunnel, the spec version per boot, the module catalog version per incarnation; the control plane deploys first and hosts may be at most one version stale (§6.10).

---

## 5. Workspace and Git

### 5.1 Data model

```
repos(id, org_id, provider NULL|github|gitlab, provider_full_name, git_url,
      default_branch, protected_branches text[], created_at)

workspaces(id, org_id, name, slug, branch_name NULL, is_base,
           visibility org|invite_only, owner_user_id NOT NULL, owner_resume_required bool DEFAULT false,
           created_by, created_at, deleted_at)

workspace_members(workspace_id, user_id, PRIMARY KEY (workspace_id, user_id)) -- read invite list

workspace_ownership_transfers(id, workspace_id, from_user_id, to_user_id,
                              requested_by, requested_at, status pending|completed|aborted,
                              completed_at NULL, error NULL) -- durable operation and transfer audit

workspace_repos(workspace_id, repo_id, root_path, role code|docs,
                base_branch, head_sha_cache, PRIMARY KEY (workspace_id, repo_id))

workspace_forks(id, org_id, source_workspace_id, target_workspace_id,
                requested_by, idempotency_key, proposed_name,
                status pending|completed|aborting|aborted,
                repo_manifest jsonb, source_flush_id NULL,
                created_at, completed_at NULL, error NULL)

workspace_fork_reservations(repo_id, branch_name, fork_id,
                            PRIMARY KEY (repo_id, branch_name))

workspace_vms(workspace_id PK, host_id NULL, vm_id NULL, epoch bigint,
              status stopped|starting|running|stopping|failed,
              image_ref, image_digest, module_catalog_version,
              lease_expires_at, started_at, last_activity_at,
              stop_reason NULL, stop_deadline_at NULL,
              last_flush_id NULL, unflushed_loss_at NULL, last_error)

workspace_flushes(flush_id PK, workspace_id, vm_id, epoch,
                  reason shutdown|fork, materialization snapshot,
                  repos jsonb,        -- [{repo_id, branch_oid, local_head_oid, snapshot_oid, dirty}]
                  concurrent_writers jsonb,   -- terminals and previews open during a live flush
                  started_at, complete_at NULL, error NULL)
```

`workspace_repos` carries no authoritative head; `head_sha_cache` is a projection (§9.4). A working workspace spans N repos (code repos plus the org docs repo). Its mapping is `workspace_id → [(repo_id, workspaces.branch_name), ...]`: identical branch names are enforced, while commits and OIDs are independent per repository. `workspaces.branch_name` is the single authoritative name, not a per-repo option; `workspace_repos.base_branch` is each repository’s upstream/update target, inherited from the source mapping or selected protected default; it may differ across repositories. The fork manifest separately pins each actual source branch/OID. Working workspaces require non-null `branch_name` equal to their accepted name. A read-only base workspace has no fork-created working branch (`branch_name` is null) and resolves its existing protected refs per repo; the identical-name constraint applies to the working branch vector created by its fork. Pushes are per repo, like a laptop with several clones. There are no job or task branches and no workspace status column: a workspace exists or is deleted, and its VM state lives in `workspace_vms`. `vm_id` is the current incarnation and `epoch` counts attaches; both change on every attach and `vm_id` is cleared at release. `last_flush_id` names the receipt the next attach restores.

The fork operation pins its complete member-repository set and source OIDs in `workspace_forks.repo_manifest` before writing refs. `workspace_fork_reservations` reserves the proposed name in every member repo until completion or verified cleanup; overlapping names that Git cannot store together (such as `feature` and `feature/x`) conflict too. A pending operation survives API restart and cannot be bypassed by another branch producer. The destination is not exposed as a usable workspace and cannot boot until all branches and any copied snapshot refs exist and final workspace/receipt metadata commits. These are operation records, not another workspace lifecycle status or head authority.

`owner_user_id` names exactly one org member and is the authority for human writes; `created_by` is immutable provenance, not permission. The owner always reads their workspace. Org admins read any workspace; other org members read when visibility is `org` or their invite is present in `workspace_members`. An invite grants read access only. An org admin who is not the owner must transfer ownership before writing. The transfer audit preserves the old owner, new owner, requester and completion time (§5.8); a pending or aborted operation is not a completed transfer. Base workspaces retain an owner for administration but remain read-only and have no VM.

Readers and owners browse files, docs, chat history, task timelines and PRs and clone/fetch internal git identically. Only the owner sends into any chat, edits or withdraws queued messages, answers questions, uses terminal, saves files, commits/pushes, starts/cancels non-preview tasks, changes workspace settings, performs lifecycle actions or creates PRs and records workspace approval decisions. Preview start is the sole reader operation that wakes the source VM; cancellation never wakes it. Fork is allowed to readers but creates a new workspace owned by the caller; it never resumes a stopped source.

| Capability | Reader | Workspace owner |
|---|---|---|
| Browse files, docs, chats, task timelines and PRs; clone/fetch internal git | yes | yes |
| Start a preview or cancel any preview | yes | yes |
| Send/edit/withdraw chat input, answer questions, use terminal | no | yes |
| Save, native Git writes or Push; start/cancel non-preview tasks | no | yes |
| Stop, resume, restart, delete; change workspace settings | no | yes |
| Create a PR or record a decision from the workspace (subject to approver rules) | no | yes |
| Transfer ownership | org admin only | yes |
| Fork into a workspace owned by the caller | yes | yes |

`owner_resume_required` is durable admission state, independent of VM status. Transfer completion sets it; only explicit current-owner Resume clears it, even if a reader preview already woke the VM. Pending transfer operations separately close admission until completion or abort. Generic task dispatch and all same-task continuations consult both barriers; preview-only wake never clears the owner-resume requirement.

### 5.2 Branch naming and identities

- Workspace branch: the user’s proposed workspace name, validated as a Git branch name and used unchanged as `refs/heads/<name>` in every member repository. For a new working workspace, `name = branch_name`; do not silently slugify it, append a workspace ID, choose a fallback or reuse an existing branch. A URL slug is routing metadata only. Invalid names and collisions return a field error asking the user to choose another name.
- Base workspace: is a read-only view of each member repository’s existing protected/default ref. Those source refs may have different names (`main` in one repo, `master` in another); they are not the working branches allocated by a fork. Repository onboarding and initial protected-ref bootstrap remain repository setup, not alternate creators of workspace branches. A fork from this base creates the proposed common destination name in every member. Base has no VM; chat, preview and terminal offer "fork and …" through that checked operation. Network pushes to protected refs are denied.
- Identities seen by the git service: **user** (current workspace owner for receive-pack; any authorized reader for upload-pack, JWT as today); **workspace incarnation** (the workspace token, a JWT with claims `workspace_id`, `vm_id`, `epoch`, minted at each attach, renewed through `lease.renew` while the incarnation is current, held by the dispatcher and by the host credential proxy, never by a provider process); **API server identity** (merges, forks, protected-ref bootstrap/import and internal-ref pruning, in-process).

**Only fork creates the mapping.** Every user/API/CLI action that creates a working workspace, including a UI labelled Create, forks a source workspace (which may be a base workspace) through §5.8. A raw branch-creation endpoint, clone/ensure fallback, missing-ref repair or member-repo edit must not mint a branch outside that operation. Renaming a workspace must not silently change its mapped branch name; no independent branch/workspace rename workflow is introduced here. Local Git experimentation cannot create a new platform workspace or published member-branch mapping.

### 5.3 Receive-pack policy (the git boundary)

`routes/git.rs` keeps its shape: parse ref updates, decide in Rust, spawn `git receive-pack --stateless-rpc`.

| Ref | user (current owner) | workspace incarnation (current) | API server identity |
|---|---|---|---|
| `refs/heads/<protected>` | deny | deny | approved merge (§10.6); repository bootstrap or provider import (§5.2, §5.6), honoring fork reservations |
| `refs/heads/<workspace branch>` | update existing mapped ref only: FF or rewrite (below); never create | same | create only through the checked fork; authorized merge updates an existing target (§10.6) |
| `refs/heads/<other workspace's branch>` | deny | deny | n/a |
| `refs/g5/flush/<workspace id>/<flush id>` | deny | create only (no update, no delete) | checked fork-copy create only (§5.8); delete (pruning/verified fork cleanup) |
| `refs/g5/merges/<merge op id>` | deny | deny | merge operation only |
| `refs/g5/forks/<fork id>` | deny | deny | fork receipt creation/cleanup only (§5.8) |
| delete of a workspace branch | deny | deny | workspace deletion |
| tags, other namespaces | deny | deny | n/a |

A token whose `vm_id` is not the workspace's current incarnation is rejected before any ref is considered; a draining incarnation (`stopping`) is still current (§6.4).

**Rewrites of the workspace branch.** Agents rebase onto each member repository’s configured upstream (`workspace_repos.base_branch`), so the workspace branch accepts non-fast-forward updates from the workspace's own identities. The server enforces exactly git's update-command check: the `old_sha` the client sends must equal the ref's current value, and a zero `old_sha` is refused for every user/incarnation update of a workspace branch, even if the ref is absent. An absent mapped branch is an integrity error, never permission to recreate it by push; only the identified fork operation can finish its own pending creation. That rejects a ref that moved during push negotiation and makes `--force-with-lease=<branch>:<expected>` work as designed; it does not prove that the pusher incorporated the previous head. Lease safety is a client obligation: the profiles require `--force-with-lease` and forbid bare `--force`. In the shared-folder model every agent in a VM shares the same clone of each member repository, so a rewrite that discards an unseen commit can only happen between the VM and a user pushing from a laptop, which is the laptop semantics asked for. The platform never moves the agent's remote-tracking refs behind its back: the flush observes the internal head with `git ls-remote` and never fetches.

`git-upload-pack` requires workspace read access (§5.1). `git-receive-pack` checks `owner_user_id` at the write boundary, including laptop pushes; neither original creator, former writer role nor org-admin status bypasses it. An admitted push and transfer share the workspace mutation barrier so a push cannot complete with stale human authority after ownership changes. VM commits are authored as the incarnation’s owner; workspace-incarnation pushes retain their audit identity. Transfer closes admission to new human pushes, drains admitted writes, and still permits the old current incarnation’s flush before release. After a successful push the handler publishes `branch.updated {repo_id, ref, old, new, actor}` as a hint; projections converge from git regardless (§9.4).

### 5.4 Durability: agents push, G5 flushes

There are two ways work becomes durable.

**Agents own commit cadence.** Profiles require regular native commits and pushes to `origin` when a unit of work is done. There is no platform Commit button, commit endpoint, commit shortcut or `workspace.changes commit` operation. The retained owner Push action can publish existing commits only; it never stages files or creates a commit. Pushed commits are never lost, whatever happens to the VM. This is the normal path and the only one that runs while the workspace works. Losing work when a VM is lost between pushes is accepted.

**G5 flushes at boundaries.** `workspace.flush({reason, interrupt?})` is a privileged lifecycle operation of the dispatcher's private protocol, called by the Workspace lifecycle service only. It captures the exact state of every member repository into internal git and completes a receipt. It runs:

| Reason | When | Ends the VM |
|---|---|---|
| `shutdown` | every graceful teardown (§5.8): user stop, restart, delete, ownership transfer, idle reclamation, host drain, rollout, expiring host credential, operator drain | yes |
| `fork` | a fork of a running workspace (§5.8) | no |

Only `snapshot` materialization exists: the receipt records commits and a snapshot; nothing is committed onto the workspace branch by a flush. Agents publish dirty work through their normal native commit/push cycle (§10.2). A flush creates internal snapshot commits only; it never manufactures a user-visible branch commit. There is no public manual-flush endpoint or operator alias: planned maintenance uses shutdown through drain. Reject the removed `explicit` reason on every runtime/API path.

**Steps of a flush**, per member repository, with the real index, `HEAD` and the working tree untouched:

1. **Observe** the internal head with `git ls-remote origin refs/heads/<branch>` (never `git fetch`, which would move `refs/remotes/origin/<branch>` and widen an agent's implicit `--force-with-lease`).
2. **Snapshot** a commit `S` whose tree is the working tree and whose parent is the local `HEAD`, from a temporary index seeded from the real index so tracked files matched by an ignore rule stay tracked:

   ```
   cp -p .git/index $TMP_INDEX
   GIT_INDEX_FILE=$TMP_INDEX git update-index --really-refresh || true    # exit 1 = "needs update", expected
   GIT_INDEX_FILE=$TMP_INDEX git add -A
   GIT_INDEX_FILE=$TMP_INDEX git write-tree                                # -> tree
   git commit-tree <tree> -p HEAD -m "<flush message>"                    # -> S; S == HEAD's tree means "clean": S is skipped and snapshot_oid = local HEAD
   ```

   Ignore rules and the platform excludes file suppress only untracked files. Ignored build outputs, `node_modules`, `target/`, the docker scratch disk and provider state are caches and are not flushed. Symlinks, hard links, modes and ignored files follow git's own rules; the captured set is what `git add -A` captures.
3. **Push** `S` (or the local `HEAD` when clean) to the immutable ref `refs/g5/flush/<workspace id>/<flush id>`, with the token the dispatcher holds, either through its own git credential helper or through the host credential proxy (either keeps the token out of provider processes, §6.5). Because `S` descends from the local `HEAD`, the push carries every unpushed local commit.
4. **Complete the receipt.** The dispatcher reports the vector `[{repo_id, branch_oid, local_head_oid, snapshot_oid, dirty}]`; the API verifies with `git for-each-ref` on each mirror that the flush ref exists at `snapshot_oid`, then sets `complete_at` in one transaction and updates `workspace_vms.last_flush_id`. A receipt without `complete_at` is not a receipt.

Flushes of one workspace are serialized. `flush_id` is a UUIDv7 so receipts order by time.

**Quiescence.** A `shutdown` flush runs after the stop sequence has interrupted every module execution and closed previews and terminals (§5.8), so the snapshot is an exact cut of a quiet tree. A `fork` flush on a running VM does not end the VM, so the dispatcher first closes admission of new module invocations and of file mutations (`fs.write`, `fs.delete`, `fs.rename`, through the same admission barrier the stop sequence uses), waits for the mutations already admitted to finish, and waits for the active bounded-run executions (agent turns, bootstrap) to end, up to `quiesce_secs` (default 30). If any is still active, the flush is refused with `WORKSPACE_BUSY {active_exec_ids}` unless the caller passed `interrupt: true`, in which case those executions are interrupted like a stop. The barrier stays closed through the capture of every member repository. Open terminals and a running preview are not interrupted by a live flush; they are recorded in the receipt's `concurrent_writers`, and a consumer that needs an exact cut (a fork) shows them and offers "stop them and fork". Both admissions reopen when the flush completes, is refused or fails.

**Failure policy.** A flush that fails (a push refused, the API unreachable, the verification failed) is retried with backoff. What happens next depends on the caller:

- A user-initiated stop, restart, delete, ownership transfer or fork **aborts**: the VM keeps running, admission reopens, `last_error` records the cause and the user is notified. Nothing is lost because nothing was torn down.
- A G5-decided teardown with a deadline (drain, rollout, credential expiry) retries until `stop_deadline_at`, then **proceeds without a receipt**: the incarnation is released, `unflushed_loss_at` and `last_error` record the loss, a notification names the workspace, and the next attach restores the previous complete receipt. The loss is visible, never silent.
- Idle reclamation has no deadline: a failed flush cancels the idle stop and the workspace stays up; the idle timer restarts.

**Retention.** Flush refs are immutable. The API prunes, per workspace, every flush ref older than the two newest complete receipts, and every ref of an incomplete flush 24 hours after its `started_at`; a workspace deletion prunes them all. Retained receipts keep their rows.

**Guarantee.** Work pushed by an agent or a user is never lost. Work committed locally but not pushed, and uncommitted edits, survive every graceful teardown whose flush completed, and survive a fork. They are lost on an ungraceful loss (host loss, lease expiry, VMM crash, OOM, operator force stop) and on a deadline-bound teardown whose flush failed, back to the last complete receipt if one exists and otherwise to the branch head. Provider session files are not on the VM's critical path: they are in Postgres (§8.5).

### 5.5 Restore on attach

Every attach restores from the workspace's last complete receipt, on a warm retained root or a cold one:

1. Resolve the completed workspace’s full `(repo_id, shared branch_name)` vector, clone or reflink each member repo from the internal store (the host’s bare cache is an optimization), and fetch its existing workspace branch. A missing mapping/ref fails attach; restore never invents a branch in the internal store. Local checkout materialization is not authoritative ref creation.
2. If `last_flush_id` is null, check out the workspace branch at the internal head; done.
3. Otherwise fetch `refs/g5/flush/<workspace id>/<flush id>` for each repo and set the local branch to the receipt's `local_head_oid`: `git checkout -B <branch> <local_head_oid>` with upstream `origin/<branch>`. This reproduces committed-but-unpushed history exactly.
4. If `snapshot_oid != local_head_oid`, materialize the snapshot tree into the working tree without moving `HEAD` (`git restore --source=<snapshot> --worktree -- .` plus untracked files present in the snapshot and absent from `HEAD`), leaving changes unstaged.
5. If the internal head moved while the VM was down (a user pushed from a laptop), the local branch is now behind or diverged from `origin/<branch>`. Nothing is merged automatically; the status strip shows ahead/behind and "Update from origin" runs `workspace.changes update`, or the agent rebases natively.

A warm retained root is a cache of ignored files and git objects, never of tracked content: the attach runs the same steps on it (steps 3 and 4 are idempotent) and removes untracked, non-ignored paths that the snapshot does not contain, so the working tree equals the receipt whether the root was warm or cold. `HEAD`, `git status` or a list of dirty paths are never used as proof that a tree equals the receipt; only the materialization is. A retained root of an incarnation that was lost ungracefully may hold work newer than the receipt; that work is discarded by the same materialization, because the receipt is the only state the platform can vouch for.

### 5.6 Provider integration (GitHub / GitLab)

- **Inbound.** For each connected repo the API fetches the protected branches from the provider into the mirror on webhook and on a periodic poll (default 5 minutes). Inbound updates to a repository's protected refs are serialized with merge operations on that repository (§10.6). Workspace branches are never fetched from the provider.
- **Outbound.** The API publishes an already-created workspace branch to the provider only on explicit actions: PR publish (§10.2), "Publish branch", and the republish step of a merge operation (§10.6), using the acting user’s stored token, with the same name as the internal branch. Initial provider publication is create-if-absent; an independently created remote branch that appeared after fork preflight is a collision, never an overwrite or implicit adoption even at the same OID. Subsequent updates use `--force-with-lease` against the recorded publication owned by this workspace. Publishing its external counterpart does not create another workspace or internal branch identity.
- **Updating a workspace from upstream** is a git operation inside each selected member checkout: fetch and rebase onto that mapping’s `workspace_repos.base_branch` (`git fetch origin main && git rebase origin/main` when it is `main`; origin is the internal mirror through the credential proxy). The UI's "Update from origin" runs it as `workspace.changes update`; agents do it natively.
- **Connecting a local repo to a provider** updates `repos.provider/git_url`, performs the first publish, and enables inbound mirroring of the protected branches.
- **Local-only repos** keep working: a bare repo in the store with no sync.
- **Provider credentials never enter the VM.** Private git dependencies on GitHub or GitLab are reached through the credential proxy with an org-level read-only grant (§6.5) or are unsupported.

Removed: `tracking_mode`, `push_mode`, `auto_pull_enabled`, `base_auto_pull`, sync runs, pair leases, advisory pinger, remote-tracking merge runs, the branch-integration worker and scheduler.

### 5.7 The workspace↔branch association check

`workspace.changes get` reports, per member repo, the symbolic `HEAD`, whether it equals `workspaces.branch_name`, the dirty set, and ahead/behind against `origin/<branch>`. After authorization (owner for writes, reader for preview start), the API ensures the runtime and calls it before every UI-triggered mutation of a running workspace (Save, Push, Update, preview start, PR create). A mismatch returns:

```
409 WORKSPACE_BRANCH_MISMATCH
{ repo: "backend", expected: "fix-login", actual: "main", detached: false }
```

The UI shows the mismatch in the status strip with one action, "Switch back" for the owner (`workspace.changes switch_back`: `git switch <branch>` if clean, otherwise stash or commit first). Nothing prevents an agent or a terminal user from running `git checkout`; the profile text tells agents to stay on the branch and the check catches drift at the next UI operation. Several chats share the same member-repository checkouts and reconcile their edits with native Git. A local branch change cannot create another managed branch through receive-pack, and association checks still require the workspace’s shared branch name in every member repo.

### 5.8 Workspace actions, teardown reasons and the stop sequence

A workspace has six user-visible lifecycle actions; Create is a workspace fork, not a separate branch-creation path. Stop, resume, restart and delete require the owner; transfer additionally permits an org admin. There is no archive.

| Action | Steps | Result |
|---|---|---|
| **create (fork)** | run the checked fork below from a source/base workspace using the proposed exact name; create that branch in every member repo; publish rows with the caller as owner only after the complete mapping exists; optionally carry chats (§8.6), then attach and restore | a new workspace with same-named branches across all members; any collision asks for a different name |
| **stop** | the stop sequence with reason `user`; then release | branch, receipt, chats, session files and settings remain; reads come from the receipt or git (§9.1) |
| **resume** | attach a VM on a host with a free slot (placement §6.10), restore from `last_flush_id` (§5.5); session files restore lazily on the first chat turn (§8.5) | running workspace with the last flushed state |
| **restart** | the stop sequence with reason `restart`, then resume immediately, with the image re-resolved from `.g5/workspace.yaml` (§6.3) | the same workspace on a fresh VM; also the repair for a broken VM and the way to pick up an image change |
| **transfer** | close admission, run the stop sequence with reason `transfer`, release the VM, then atomically change owner and complete the audit record; when already stopped, no VM work | stopped workspace under the new owner; new owner explicitly resumes |
| **delete** | stop if running (force variant permitted, which records the loss); delete the workspace branch, the flush refs and the receipts in every repo; delete chats, session files, tasks and rows | gone |

**Forking a workspace** is allowed to any reader and creates a new workspace owned by that reader. A reader may request a non-interrupting flush of a running source; if bounded owner work prevents quiescence, return `WORKSPACE_BUSY`. Only the source owner can authorize interrupting that work or closing its terminal for an exact cut; a reader may independently cancel a preview. Forking a stopped source reads the receipt or branch head and never wakes it. Forking carries every committable artifact of the source, dirty edits and unpushed commits included, but not local file-system state such as build outputs. A running source is flushed first with reason `fork` (quiescence rule of §5.4); a stopped source is forked from its last complete receipt, or the branch head when no receipt exists; a clean, idle source yields a receipt with no dirty entries. After all-repository name validation and reservation, the fork’s branch is created at each repo’s `local_head_oid` (or pinned source branch OID without a receipt), the source receipt's flush refs are copied under the fork's workspace id, and the copy becomes the fork's `last_flush_id`, so the fork's first attach restores the dirty tree.

**Checked fork and branch creation.** The control plane owns this operation; hosts and VMs only materialize a completed mapping.

1. **Validate and pin.** Authorize read access to the source and validate the exact proposed workspace name as a Git branch ref, without hidden normalization or suffixes. Pin the source workspace, full member-repository set, destination ID and request idempotency key in a fork operation. A retry with that key and the same inputs resumes its durable phase; different inputs under the same key are refused. Recovery verifies this operation’s receipts and pinned inputs instead of treating its proven prior creates as new collisions or re-pinning the source.
2. **Check every member before creating any branch.** For a new operation, check authoritative internal Git for `refs/heads/<name>` in each member repository, regardless of whether another workspace owns it; check Git ref-prefix conflicts as well. For provider-connected repos, also inspect the provider’s current branch namespace without importing or mutating its branches; a branch that exists there counts as a collision. If any member conflicts, return `409 WORKSPACE_NAME_CONFLICT` identifying the proposed name and conflicting accessible repositories, and the UI asks for a different name. Do not create any destination branch, reuse an existing branch or retry with an automatic suffix. Failure to verify a repository is a retryable verification error, not evidence the name is free.
3. **Reserve across the whole set.** Under deterministic repository lock ordering, reserve `(repo_id, name)` across all members, rejecting overlapping/prefix-conflicting reservations, and recheck internal refs at the mutation boundary. Internal fork, protected-ref import, deletion and other API ref writers honor these reservations. Two forks whose member sets overlap cannot both win the same name. Pin the complete source receipt/OID vector once using the live-flush or stopped/base read path above; a failed source flush releases unused reservations and creates no destination branch.
4. **Create with proof.** In each member repo, use a create-if-absent Git ref transaction to create both `refs/heads/<name>` at the pinned source OID and `refs/g5/forks/<fork_id>` at that OID. Persist progress in the manifest; on a lost reply the receipt proves this operation created that ref. A pre-existing ref without this operation’s receipt is a collision, even if its OID matches. Copy any source flush refs under the destination workspace ID and verify the complete destination receipt before publication. Recheck source access before admitting publication; do not expose a partial destination or start its VM.
5. **Publish only the complete fork.** After verifying every branch/receipt, atomically insert the usable workspace and all `workspace_repos` rows, its complete flush receipt if present, the caller as owner, and the fork’s completed result. Release reservations, then finish any requested chat carry idempotently under the same fork operation and attach. The same idempotency key returns that same completed workspace on retry. A later boot failure does not create a second fork.
6. **Recover partial work.** Git ref transactions are per repository, not cross-repository atomic. An incomplete fork remains non-runnable while its durable operation resumes creation or rolls back. Rollback deletes only branches/snapshot refs proven to belong to this fork, with expected-OID checks and the fork receipt; it never deletes a pre-existing or subsequently changed branch. Keep reservations until completion or verified cleanup. A conflict discovered after a partial create is returned as a name conflict after cleanup; if cleanup cannot finish, expose operation recovery status, keep the destination unavailable, and do not claim the failed fork left no refs. Retain fork receipts through durable settlement, then prune them through the API.

The source repository/default refs may differ; only the new destination name is identical across the vector. No existing default ref is renamed or rewritten to achieve this. The collision check applies to the whole member set, including the docs repository, and to every fork entry point: Create, Fork, Fork and preview, Fork and chat, and API/CLI equivalents. Adding a repository to a completed workspace must not create its missing branch as a side effect; any operation requiring new member branches uses a new checked fork. The shared name is a constraint on the mapping, not an optional naming preference.

**Teardown reasons.** A graceful teardown is any teardown that runs the stop sequence; it is the only way the platform ends a VM on purpose.

| Reason | Decided by | Deadline | Flush failure |
|---|---|---|---|
| `user` (stop, restart, delete) | the user | none | abort, the VM stays up |
| `transfer` | the owner or an org admin | none | abort; old owner remains, the VM stays up |
| `idle` | the idle reconciler (org default 30 minutes, workspace override; activity = running bounded executions, an open terminal, preview traffic (mere service existence is not activity), file RPC, a live native request within its response deadline; not an open browser tab) | none | cancel the idle stop, restart the timer |
| `drain` | an operator or the rotation loop (`host.drain`) | the drain deadline (default 30 minutes after the nudge) | proceed at the deadline, record the loss |
| `rollout` | the deployment procedure (§6.10): a new VM image or dispatcher version | as `drain` | as `drain` |
| `credential` | the host whose credential cannot be renewed (renewal refused because the host is draining, terminated, or its runtime identity changed), with a margin before expiry | the credential's expiry minus a margin | as `drain` |
| `capacity` | an operator action to free a slot (never automatic) | as `drain` | as `drain` |

Every deadline-bound teardown starts with a **nudge**: a notification to the workspace's members and a notice event appended to every running chat session that is delivered at the next turn boundary (a running CLI turn cannot receive a mid-turn message in the MVP), asking to commit and push. Turns that are still running at the deadline are interrupted and continue after the resume (§7.2).

An ungraceful loss is everything else: host loss, lease expiry, a VMM exit, OOM, an operator force stop. It runs no flush and loses unpushed work back to the last receipt.

**The stop sequence.** Throughout it the incarnation remains current: its `vm_id` is unchanged, its lease keeps renewing, and every boundary accepts it. What closes is admission of new work, not the incarnation's authority.

1. **Decide.** `workspace_vms.status` → `stopping`, `stop_reason` and `stop_deadline_at` recorded in the same update.
2. **Close admission.** The Workspace Runtime facade refuses new Save, module invocations, preview, terminal and agent-turn requests for the workspace with `WORKSPACE_STOPPING`, and the UI offers "Resume" to the current owner after the stop. Agents closes its own admission for the workspace (`agents.workspace_stops.prepare`): no new inbox reservations, queued inbox rows stay queued; they remain owner-editable for an ordinary stop, while a transfer also closes human mutation admission until it commits or aborts. File mutations already admitted complete; the dispatcher's admission barrier returns only when none is in flight.
3. **Quiesce.** Interrupt active module executions (SIGINT, then SIGKILL after 15 s), invalidate their unanswered/undelivered native provider requests and settle each attempt as `interrupted`. Preserve an unfinished agent task as held or queued according to the stop/continuation policy (§7.2); do not terminally fail a dependency merely because its attempt ended. Report and acknowledge all remaining outcomes/events through the normal contract (§7.5), close terminals and cancel preview service tasks with the teardown reason. No ended request remains answerable.
4. **Flush** with reason `shutdown` (§5.4) and wait for the complete receipt, or for the deadline.
5. **Harvest.** Session files changed since the last harvest are uploaded (§8.5) and acknowledged; the event journal is drained and acknowledged; Agents commits its stop receipt (`agents.workspace_stops.commit`), which proves that no Agents state remains only in the VM.
6. **Stop the VM.** The lifecycle service sends `vm.stop graceful` to the host and waits for the host's `exited` event or the `stopped(exit)` result. `vm.stop` is idempotent and fenced by host ownership of the `vm_id`, not by currency (§6.2), so a retry can never be refused as stale. The row stays `stopping` and the incarnation stays current meanwhile, so a resume request waits instead of racing the exit; a lifecycle service that restarts finds the `stopping` row and re-sends the same command, and the reconciler does the same for any `stopping` row whose VM a host still lists.
7. **Release.** The row becomes `stopped` with `vm_id` cleared and `last_flush_id` set (or `unflushed_loss_at` set) in one update; from this point the old incarnation is stale and every boundary rejects it (§6.4). The host freed the slot at exit and retains the workspace root (§6.3).

**Transfer completion and recovery.** Persist one operation ID with the expected old owner and the intended new org member; serialize it with stop/resume/delete, dispatch, fork, human writes and other transfers. Validate the caller and target again before committing. Keep admission closed through the VM exit and release. In the final transaction, compare the current owner with the recorded old owner, set `owner_user_id` and `owner_resume_required=true`, mark the operation `completed` with both users and `completed_at`, and leave the workspace stopped. Repeated requests or a crash after release resume the same operation; a completed request returns its receipt without transferring twice. A flush failure records `aborted`, restores the old owner’s admission and leaves ownership unchanged; interrupted preview tasks do not silently restart. A stopped workspace transfers with the same owner/audit transaction and no boot or flush.

Chats, queued messages, tasks, PRs, session files and history stay with the workspace. Unresolved native provider requests are invalidated when their originating execution ends; their timeline history remains. The new owner can explicitly continue the same logical agent task after Resume, using their account, and answer any fresh request the provider issues. A response to an invalidated request is rejected; it cannot wake a VM or start a custom successor. Existing queued work and automatic continuations do not resume a workspace stopped for transfer: the new owner explicitly resumes it. A newly requested reader preview may still use the normal preview-only wake path, but cannot release held non-preview work; the next explicit owner resume releases that work even if the preview already woke the VM. The previous owner retains only the read access conferred by org visibility/invites or org-admin status. Old terminal channels, runtime grants and credential bindings end with the old incarnation; a transfer never hot-swaps the human owner of a running VM.

Every authorized fresh wake trigger performs resume, subject to the transfer hold above: a chat message, a task run, a Save, a preview or terminal open, an explicit Resume, or an explicit Continue of held agent work. Readers have only the preview-start wake trigger. Reads and preview cancellation never resume the VM. Host drain performs the stop sequence for every VM on the host. Capacity: when no slot is free the resume is `queued_for_capacity`, the position is shown, and the dispatch loop retries; nobody's slot is taken to make room.

---

## 6. Workspace VM, host and dispatcher

### 6.1 VM lifecycle and host states

```
stopped ──resume/restart──► starting ──runtime.ready──► running ──stop sequence──► stopping ──release──► stopped
                             │                            │                           │
                             └──boot failure──────────────┴──lease lost / VM exited───┴──► failed ──next resume──► starting
```

| Transition | Actor | Details |
|---|---|---|
| resume | the Workspace lifecycle service, on a wake trigger, dispatch demand (§7.3) or explicit Resume | one compare-and-swap update on `workspace_vms` sets `status=starting`, a new `vm_id`, `epoch + 1`, the chosen `host_id`, the image and a lease; the service sends `vm.start(spec)` on the host's `ctl` stream |
| runtime.ready | the dispatcher, forwarded by the host as `vm.event ready` | restore done; runtime API listening; `module_catalog_version` recorded; the row becomes `running` |
| stop | the stop sequence (§5.8) | the incarnation stays current until release |
| lease lost | the reconciler: the lease was not renewed, or the host reported `exited`, `oom`, `lease_expired_killed` or `stale_killed` | VM `failed`, `vm_id` and `host_id` cleared; confirmed and unconfirmed active executions settle as `vm_lost` (§7.5); the next resume restores from the last receipt on any host |

Hosts have their own states: `provisioning → ready → draining → terminated`, with `unreachable` for a host whose tunnel is lost while it may still run VMs (three missed heartbeats). During `unreachable` the control plane places nothing on the host, keeps every incarnation current until its lease expires, and expects a reconnect with an inventory. Slot occupancy is not stored on the host row; it is derived from `workspace_vms`, so a crashed start cannot leak a slot.

### 6.2 Host daemon and protocol (`g5d host`)

The host is a slot provider. Retained from today: registration and heartbeat, libkrun boot (`vm-exec`), OCI rootfs templates and COW clone, the XFS reflink bare-repo cache, port allocation, passt networking, the outbound tunnel and the credit-flow stream multiplexer, boot-time attestation, and `G5D_HOST_MAX_MICROVMS` as the number of equal-size VM slots. Removed: poll/claim/ack/nack/observation, the per-task `Materializer`, golden lanes, parked runtimes, the discard fence, sticky heartbeat riders, per-task attestation, zero-slot polling, the separate preview registry, remaining per-task proxy wiring and the fixed 24-hour host JWT. The current worker already has a host-side provider-injection path (`backend/g5d-worker/src/llm_proxy.rs`); reuse it where it fits the owner-bound workspace proxy contract instead of treating all proxy code as absent.

**Transport.** One outbound WebSocket per host, authenticated with the **host credential**: a renewable token with claims `host_id`, `org_id`, `runtime_identity_hash`, TTL 24 h, renewed by the host from 2 h before expiry (`host.credential.renew`); renewal is refused when the host is draining, terminated, or its runtime identity changed, and the old credential stays valid until expiry so renewal never interrupts a tunnel. The socket carries tagged streams:

| Stream | Initiator | Payload |
|---|---|---|
| `ctl` (one per tunnel) | host | JSON-RPC 2.0 both ways: commands and messages below |
| `rpc` | control plane | the dispatcher's runtime API and its pushed events, verbatim |
| `pty` | control plane | raw bytes of one terminal, after `terminal.open` on `rpc`; one stream per terminal |
| `http` | control plane | raw TCP to a guest port for preview and port forwarding; one stream per TCP connection |

Every VM-scoped command and stream carries `(workspace_id, vm_id)`, a deadline and an idempotency key; a command received after its deadline is refused as `expired` and not executed; a `vm.start`, a `vm.grants.update` or a stream open naming an incarnation that the control plane no longer holds as current is answered `stale`, never acted on. `vm.stop` is fenced by ownership only: the host stops any `vm_id` it runs, current or not, because stopping a superseded VM is always safe (it is what the reconnect kill does) and a stop must never be refused by the fence it serves; `not_found` is the answer for a `vm_id` the host does not run. There is no `llm` stream: model traffic never crosses the tunnel.

**Versioning.** `host.hello` carries `protocol_version` and the host's `runtime_identity` (g5d version, base image digest, libkrun, libkrunfw, kernel, providers); the control plane answers `host.accept {protocol_version}` with the highest version both speak, or `host.reject {unsupported_version}`, on which the host self-drains (refuses `vm.start`, keeps its VMs and lease timers) and retries with backoff. `VmSpec.spec_version` is checked per boot; an unknown version is `spec_rejected` and excludes that host for that workspace. Adding a field, event kind or result variant is a minor change; removing or changing meaning is a new version. Deploy order follows: control plane first, hosts second, and a host is drained only for a runtime upgrade.

**Commands (control plane → host)**, all idempotent on `(command, idempotency_key)`, answered within the deadline or with `timeout` (which is not a negative result: the host keeps every completed VM-scoped result for `result_retention_secs`, default 600, so a retry after a tunnel drop gets the original result):

| Command | Result |
|---|---|
| `vm.start(workspace_id, vm_id, spec)` | `started` \| `already_running` \| `slot_unavailable(slots \| disk \| boot_capacity \| workspace_busy)` \| `spec_rejected(reason)` \| `boot_failed(reason, log_tail)` |
| `vm.stop(workspace_id, vm_id, mode graceful \| kill, grace_secs)` | `stopped(exit)` \| `not_found`; never `stale` (fenced by ownership only); `graceful` is SIGTERM to the guest init (the dispatcher) with SIGKILL at `grace_secs`; the stop sequence has already run on `rpc` |
| `vm.status(workspace_id, vm_id)` | state, boot and ready times, resources, lease, open streams, per-session proxy view |
| `vm.grants.update(workspace_id, vm_id, credential_grants)` | `applied` \| `stale` \| `grant_unresolved`; the proxy's grant table is swapped atomically, in-flight requests finish with the old credential |
| `host.drain(deadline)` | `accepted`; the host refuses `vm.start` and reports `host.drained` when its last VM exits; stopping the VMs is the control plane's job |

**Messages (host → control plane):** `host.hello` (identity, capabilities, slots, the inventory `[(workspace_id, vm_id, state, boot_at)]`, retained roots) answered by `host.accept {current_incarnations, config}`, after which the host kills every VM in its inventory that is not current, without a stop sequence, then replays its unacknowledged event journal; `host.heartbeat` every 10 s (occupancy, load, memory and disk free, per-VM liveness and lease freshness), whose acknowledgement carries only the current-incarnation set for the proxies and never extends a lease; `vm.event` (`booted | ready | exited{code, signal} | oom | lease_expired_killed | stale_killed | boot_failed`) journaled on the host with a per-VM `seq` until acknowledged, so an `exited` cannot be lost to a tunnel drop and a slot cannot leak; `lease.renew` forwarded from the dispatcher (§6.4); `host.credential.renew`; `host.drained`.

**Limits** are host configuration with defaults and definite refusals: `slots_total`, `max_concurrent_boots` (2), `max_pty_streams_per_vm` (8), `max_http_streams_per_vm` (256), `max_inflight_commands` (16, `busy`), `max_ctl_message_bytes` (1 MiB), `proxy_max_body_bytes` (32 MiB), `result_retention_secs` (600), `log_tail_bytes` (64 KiB). A host never hangs on a limit.

**Operator API** (org operators): provision a GCE host or mint a one-time `g5w_*` registration token for a self-hosted or macOS host; list hosts with derived occupancy and the last heartbeat; request a drain (returns a drain id; progress on the host record); delete (drain, then terminate or forget; refused while a VM is running unless `force=true`, which records the loss per VM).

### 6.3 VM start spec, images and retained roots

The control plane sends everything the host needs to boot one incarnation; the host adds nothing from its own state except the base image files and the cache paths.

```
VmSpec {
  spec_version:  1,
  incarnation:   { workspace_id, vm_id, epoch, org_id, owner_user_id, lease_ttl_secs, idle_timeout_secs },
  image:         { ref, digest },                       -- OCI, in the host cache or fetchable
  size:          { cpus, mem_mib, scratch_disk_mib },   -- must equal the host's slot size
  workspace_root:{ retain_hours },                      -- host path is host-chosen: <workspaces_root>/<workspace_id>
  repos:         [{ repo_id, root_path, git_url, branch, base_branch }],   -- shared working branch; per-repo upstream; written to /work/.g5/layout.json
  restore:       { flush_id, repos: [{ repo_id, local_head_oid, snapshot_oid }] } | null,
  env:           { G5D_WORKSPACE_ID, G5D_VM_ID, G5D_EPOCH, G5D_WORKSPACE_TOKEN, G5D_API_URL, HOME, PATH, provider paths, G5D_LLM_PROXY_URL },
  secrets_env:   [{ name, secret_ref }],                -- repository and task secrets, resolved by the host into the dispatcher's environment (§6.5)
  network:       passt,
  entrypoint:    ["/opt/g5/bin/g5d", "dispatch"],
  credential_grants: [                                   -- consumed by the host proxies, never mounted
    { kind: llm_provider, provider: anthropic | openai | gateway, secret_ref, owner_user_id },
    { kind: git_https,    target_host, source: workspace_token | secret_ref },
    { kind: g5_api,       source: workspace_token },
    { kind: gh,           target_host, secret_ref }      -- later phase
  ]
}
```

The host validates before boot, in order, and the first failure is the answer: known `spec_version`; `org_id` equals the host credential's org; UUIDs and relative `root_path` without `..` or symlinks; slot size, a free slot, no other VM of this workspace on the host, boot capacity; `image.digest` matches the cached or pulled image; `env` contains only allowlisted keys, with provider/SCM credentials forbidden and `G5D_WORKSPACE_TOKEN` as the explicit dispatcher-only exception; grants and `secrets_env` resolve (values stay in host memory; `secrets_env` is passed to the dispatcher process only); the workspace token's claims match the incarnation; `owner_user_id` and provider grants match the control plane’s current owner binding. The owner binding is immutable for that incarnation.

**Image.** The VM image is the org's default image or the one named by `base_image:` in the code repo's `.g5/workspace.yaml`, resolved at each attach and recorded on `workspace_vms`. Changing it takes effect at the next restart; the Restart button exists for that and for repairing a broken workspace. The image is a property of the workspace, not of the host: hosts report only their own `runtime_identity`, and a host answers `spec_rejected(image_not_cached)` if it cannot fetch the image. Customer-specific worker images do not exist. The runtime directory `/opt/g5/bin` (g5d, the pinned provider CLIs, the fake binaries in CI images) and the module catalog are part of the runtime image and are hash-pinned; the dispatcher reports `module_catalog_version` in `runtime.ready`.

**Mounts.** `/work` is the host workspace root over virtio-fs; `/opt/g5/bin` read-only; provider assets read-only; the scratch disk is a block device for docker and build caches. Provider state lives under `/home/agent/.claude` and `/home/agent/.codex`, dispatcher state under `/var/lib/g5d`; none is inside a working tree. A platform-managed excludes file (`core.excludesFile=/var/lib/g5d/excludes`, listing `.g5/` runtime subdirectories) is applied to the flush and to agent git invocations, so neither a flush nor a native `git add -A` captures runtime files even in a repository with no `.gitignore`.

**Retained roots.** After `vm.stop` the host keeps the workspace root and scratch disk for `retain_hours` (org setting, default 24 h) so a resume placed on the same host is warm; placement prefers such a host. Retained roots are a cache: under disk pressure the host evicts them oldest first and reports it on the heartbeat; a resume that finds no root restores from git. The bare-repo cache is host-internal; the dispatcher's restore uses it when present and the git service otherwise.

### 6.4 Incarnation, lease and fence

Loss of contact with a host does not prove that its VMs stopped. The fence makes a superseded VM harmless even if it later reconnects, while a draining VM keeps the authority it needs to finish.

- **Incarnation.** Each attach mints a new `vm_id` and increments `epoch` in the same compare-and-swap that places the VM and grants its lease. An incarnation is current while the row holds its `vm_id`, in any of `starting`, `running`, `stopping`.
- **Credential.** The workspace token carries `vm_id` and `epoch`. The dispatcher renews it through `lease.renew` on `rpc` at half the TTL; the host forwards on `ctl` and returns the answer `{token, lease_ttl_secs}` or `stale`, and on `stale` kills the VM (`vm.event stale_killed`). The host passes the token through as an opaque blob; the dispatcher checks the `vm_id` claim before installing it.
- **Lease bound.** The control plane extends `lease_expires_at` to *its handling time* plus the TTL. The host arms its timer from the *monotonic time at which the renewal request left the host* plus the TTL, not from the reply's arrival, so the host's deadline never outlives the grant. Renewals carry a sequence number and an obsolete reply is ignored. At boot the control plane grants `lease_ttl_secs + start_deadline_secs` (the `vm.start` command's deadline) and the host arms `lease_ttl_secs` from receipt of `vm.start`; since a command received after its deadline is refused, the host's first deadline is again inside the grant. The proxies stop serving an incarnation at the host's deadline. Expiry kills the VM (`lease_expired_killed`).
- **Boundaries that check the fence.** Git receive-pack and upload-pack; session-file harvest; event replay, outcome reports and acknowledgements; runtime API replies (the facade drops replies from a stale incarnation); the host proxies (a request from a non-current incarnation is refused with `403 stale_incarnation`, so a partitioned VM cannot keep spending or pushing); `vm.start`, `vm.grants.update` and stream opens on the host (`vm.stop` is deliberately exempt: it can only make a superseded VM go away sooner). All of these accept a draining incarnation.
- **Reconnect.** On `host.hello` the host reports its inventory and the control plane replies with the current incarnations, including draining ones; the host kills any VM not listed, without a flush (its work was superseded by a replacement that restored from the last receipt; unflushed work is lost, which is the documented loss case).
- **Replacement admission.** The reconciler marks an incarnation `failed` and clears `vm_id` and `host_id` only after the lease has expired or the host reported the VM gone; a resume then places a new incarnation. Because the host's deadline is inside the grant, the old VM has stopped itself and its proxies have stopped serving by the time a replacement can be placed, and the boundaries above cover the case where it did not.

### 6.5 Host-side proxies and credentials

Provider keys, git tokens, platform tokens and the workspace token stay on the host. The VM reaches the outside world through one host-side service per VM, reachable inside the guest at the fixed name `g5-llm` (a passt door to a per-VM host loopback listener, seeded into the guest's `/etc/hosts` as today's `g5-host` alias is). Because every VM's passt maps the door to the same host loopback, the listener is guarded by a per-VM proxy secret minted at boot and passed only to the dispatcher (`G5D_LLM_PROXY_SECRET`), which includes it in the proxy configuration it gives every process it launches; a wrong or missing secret is `403 wrong_vm`. The secret grants nothing but access to that VM's own proxy.

| Traffic | Steering inside the VM | What the proxy does |
|---|---|---|
| Model requests (Claude Code, Codex, any OpenAI-compatible client) | `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` = `http://g5-llm:<port>/s/<session_id>[/v1]`, set by `agent.turn` per session (all agent tasks have a shared-session ID); a placeholder key so the CLI agrees to run | strips the session prefix, selects the `llm_provider` grant for the incarnation’s workspace owner and selected provider; the session ID is an accounting tag, never credential authority; attaches the credential, re-originates over HTTPS to the provider or the org's gateway, refuses a non-current `vm_id` (`403 stale_incarnation`), an untagged request (`400 missing_session`) and a missing grant (`403 no_grant`), meters usage and captures quota headers per session, and serves `GET http://g5-llm/_g5/status` (per-session `requests_open` and usage; never credentials or bodies) inside the VM and the same view on `vm.status` |
| git over HTTPS (the internal git service; GitHub or GitLab for private dependencies when granted) | a dispatcher-managed global gitconfig rewrites `https://<host>/` to `http://g5-llm:<port>/git/<host>/` for every granted host | recovers the real host from the path, re-originates over HTTPS and injects the `git_https` credential; the internal service's grant is the workspace token itself (`source: workspace_token`) |
| Platform API calls from processes in the VM (`g5d` commands and unrelated `g5-tools` MCP tools, such as PR creation; native question/permission replies use the trusted adapter channel) | `G5D_API_URL=http://g5-llm:<port>/api` in the launch environment; `G5D_EXEC_ID` and `G5D_SESSION_ID` are plain values, not secrets | injects the workspace token (`g5_api` grant) and forwards; the API still applies the incarnation fence and the execution identity carried in the request |
| `gh` and other clients with a hard-coded API host (later phase, §16) | `HTTPS_PROXY` plus a per-VM CA in the trust store, a placeholder `GH_TOKEN` | terminates TLS under the per-VM CA, injects the `gh` credential, re-originates |

The dispatcher keeps the workspace token in its own environment for its calls to the control plane (`runtime.ready`, `lease.renew`, receipts, harvests) and for its own git operations such as the flush push, and does not pass it to the processes it launches; provider processes hold placeholders only. A real `Authorization` header a client set itself is forwarded untouched.

**Grants are references, and they change.** `credential_grants` name the current owner’s provider secrets by reference, resolved by the host at boot and held only in memory. Every session in that incarnation uses that owner’s provider account, regardless of who created the session. Missing owner credentials fail explicitly; there is no fallback to the launcher, previous owner, another session creator or an admin’s account. The control plane pushes a full replacement list with `vm.grants.update` for key/token rotation within the same owner and incarnation. Ownership transfer instead stops and releases the incarnation; the next attach binds the new owner’s account. Session-scoped provider grants are deleted; session tags remain for routing, metering and quota/liveness reporting. The proxy re-resolves a grant once on upstream `401` before surfacing the error.

**Repository and task secrets** (`repo_secrets` and org secrets that builds and tests need) are environment variables in the MVP: the host resolves `secrets_env` into the dispatcher’s environment at boot, and the dispatcher passes them to module executions. Changed secrets take effect on the next launch when the dispatcher re-reads the current set; a resident terminal keeps its initial values. These secrets belong to the workspace and need no per-user isolation within it. A reader-launched preview uses this same workspace identity and secret set; the launcher’s credentials are never installed. The preview contract forbids tracked-file mutation but does not sandbox writes (§6.8).

### 6.6 Dispatcher and the Workspace Runtime API

Boot: mount `/work`, create `/home/agent` and `/var/lib/g5d`, restore (§5.5), start the runtime listener on the guest port with the per-VM listener token, load the module catalog, start the file watcher and the port scanner, open the event journal, report `runtime.ready(vm_id, epoch, module_catalog_version, dispatcher_contract_version)`.

The **Workspace Runtime facade** in the control plane is the only client that opens VM channels through a host, and only after the lifecycle service has made the incarnation current; every other platform system (Agents above all) calls the facade. Every request through it is bound to a `workspace_id`, the expected `vm_id`, an actor or grant, a deadline and an idempotency key.

**Workspace Runtime API** (JSON-RPC on the `rpc` stream; events pushed on the same stream and journaled):

| Surface | Methods | Caller | Contract |
|---|---|---|---|
| Files | `fs.list`, `fs.read`, `fs.stat`, `fs.write(path, content, expected_hash)`, `fs.delete(path, expected_hash?)`, `fs.rename(from, to, expected_hash?)`, `fs.watch` → `file.changed` | editors and the file tree through the facade | explicit editing and observation; mutation preconditions prevent a silent overwrite |
| Terminal | `terminal.open(cols, rows)`, `terminal.write`, `terminal.resize`, `terminal.close`; bytes on a `pty` stream | the workspace owner | the user's escape hatch; never a platform-extension mechanism |
| Modules | `modules.list`, `modules.describe`, `modules.invoke(exec_id, module_id, version, operation, input, grant_id)`, `modules.status`, `modules.cancel` | platform features through the facade | the sole execution and extension path; callers cannot supply `argv`, environment, cwd or signal targets |
| Ports | `ports.list`; `port.opened`, `port.closed` events | the Ports panel | observes listeners; starts nothing |
| Workspace status | `workspace.status.get` | the workspace UI, support, operators | lifecycle state, current or last `vm_id`, placement, lease freshness, the last receipt with a freshness label; when stopped, only durable and last-reported state, never a live claim, and never a wake |
| Runtime inspection | `runtime.inspect.summary` | authorized reader or support grant | dispatcher, image and catalog identity, boot time, uptime, resource pressure, mounted roots, port summary, fixed health probes |
| Module diagnostics | `modules.logs(exec_id, cursor?)`, `modules.events(exec_id, cursor?)`, `modules.history(filter)` | authorized reader or support grant | bounded, redacted logs and structured events; no raw host processes |
| Diagnostic bundle | `diagnostics.capture(profile)`, `diagnostics.probe(name)` | support grant; owners for safe profiles | versioned redacted bundles and declared read-only probes (`workspace-integrity`, `runtime-health`, `port-connectivity`, `module-catalog-integrity`); no caller-supplied shell command |

**Private control-plane protocol**, used only by the facade, the lifecycle service, the host and the dispatcher: `runtime.ready`, `lease.renew`, `health`; `events.replay(exec_id, from_seq)`, `events.ack(exec_id, up_to_seq)`, `outcomes.pending()`; `session_files.restore`, `session_files.harvest`; `modules.input.respond(workspace_id, expected_vm_id, expected_epoch, task_id, attempt, exec_id, provider_request_id, reply_version, validated_response_ref, idempotency_key)` for an owner-authorized response to a live native provider request (§8.4); the admission barrier (`admission.close` returns when in-flight mutations finish; `admission.open`); `workspace.flush` (§5.4).

**Explicitly absent:** `proc.spawn/signal/wait/list` (a private runner used only after module resolution), `git.*` (native git for agents, the git service for refs and policy, `workspace.changes` for the UI projection), `agent.run_turn` (the `agent.turn` module), `preview.*` (the `workspace.preview` module), `checkpoint.*` and periodic checkpoint events (agent-driven persistence plus `workspace.flush`), arbitrary diagnostic shell execution (fixed probes and bundles).

Concurrency: every process uses the same workspace root and the same checkout of each member repository, all on the shared workspace branch name. There is no worktree, branch or lock per task. Concurrent edits to one file have lost-update semantics: the last writer wins and git shows only the final content. Git raises a conflict only at rebase or merge time. Agents are told (the profile says so), and the UI shows a notice when more than one execution is running. Nothing stronger is enforced.

### 6.7 Declared modules

Every platform-initiated executable unit is a declared module invocation. A module declares: a stable `module_id`, a semantic version and an immutable manifest hash; named operations with typed input, output and event schemas; a pinned entrypoint and a lifecycle class (`bounded_run` or `service`); limits (time for bounded runs, CPU, memory, disk, concurrency, output and log size; preview explicitly has no wall-clock cap or bounded-work concurrency charge); the minimal capabilities it requests (workspace read or write scope, bounded port allocation, network egress policy, git operation scope, provider-turn access); and its observability (health probes, diagnostic profile, event kinds, redaction policy). The dispatcher resolves the immutable catalog entry, validates the input, intersects the requested capabilities with the control-plane-issued `grant_id`, and only then uses its private runner. A manifest requests capabilities; it never grants them. The runtime image and the module catalog are hash-pinned; a workspace configuration may select or configure an installed module but cannot add code or privilege. Each invocation is auditable as `(workspace_id, vm_id, exec_id, actor, module_id, version, manifest_hash, grant_id, input_digest, capability_grant, outcome)`.

`modules.invoke` is idempotent on `exec_id = (task_id, attempt, vm_id)` and returns a disposition: `started`, `already_running`, `already_finished(outcome)`, or `cancelled` (a cancellation disposition exists for this identity, §7.5). Module events and terminal outcomes use the dispatcher journal, ordered replay, acknowledgement and the incarnation fence.

| Module | Owner | Class | Operations |
|---|---|---|---|
| `agent.turn` | Agents | bounded run | `run(AgentTurnInput)` → events `AgentTurnEvent`, outcome `AgentTurnOutcome`; declared live `input.respond` channel for correlated native responses (§8.3–§8.4) |
| `workspace.changes` | Workspace | bounded run | `get` (the normalized association, dirty, ahead/behind, conflict projection); `push`; `update` (fetch and rebase each selected repo onto its own base branch; refused when dirty); `switch_back` |
| `workspace.preview` | Workspace | service | `start(profile)`, `stop`, `status` (§6.8) |
| `workspace.bootstrap` | Workspace | bounded run | `run` (deterministic workspace setup; uplift's bootstrap submits child tasks through the API with its execution identity) |
| `workspace.diagnostics` | Workspace | bounded run | `capture(profile)`, `probe(name)` |

The dispatcher core (listener, journal, module runtime, admission, flush, restore) is edited only for defects once it ships; features arrive as modules and as versions of modules.

### 6.8 Preview

A Preview button creates a durable **task of kind `preview`**, with `created_by` recording the launching user, and dispatches `workspace.preview start(profile)` through the same task path as other types (§7.3). It appears in the workspace task queue and timeline from `queued` through its terminal state, with launcher, service status and Cancel. The command comes only from `.g5/preview_profiles/<name>.yaml` or the `preview:` block of `.g5/workspace.yaml`; a reader cannot supply arbitrary shell, environment, module, credentials or capability grants. The task runs as a supervised service, remains `running` while the preview serves, and is not marked successful merely because a port is ready.

**Access and identity.** Any member with workspace read access may start a preview and cancel any preview, including one launched by another reader or the owner. Reader access grants no other task creation/cancellation, generic module invocation, terminal or write capability. The API checks the persisted task kind for cancellation, not caller-supplied kind or launcher identity; the Preview panel, task queue, API and `g5d preview` share this authorization and lifecycle. Preview starts wake a stopped workspace through normal dispatch and placement; reads, preview status and cancellation never wake it. The process runs under workspace identity with workspace secrets, never under the launcher’s credentials. Its launcher is audit data only.

**One active preview per workspace.** A second start replaces the current preview regardless of who launched it. The start/replacement operation is serialized per workspace and idempotent by request key: durably cancel the previous queued/running intent, deliver its cancellation disposition and wait for its process to stop (or its incarnation to be lost) before launching the successor. Concurrent starts choose one current intent; stale readiness, exit, cancellation or port events are fenced by task/attempt/incarnation and cannot change the successor. Never run two preview processes during a slow or disconnected cancellation. Cancelling an already terminal task is idempotent and cannot cancel its replacement.

**Service lifecycle.** Preview consumes no workspace bounded-work concurrency slot and has no wall-clock limit; host VM capacity/quota and module resource limits still apply. Traffic through its relay counts as activity, using the org idle default with a workspace override. An idle service’s existence alone does not keep the VM awake. Cancel, replacement or VM stop terminates the service and records the reason on its task; unexpected service exit is a terminal failure. Loss settles even an unconfirmed preview start. Preview has no automatic retry or successor across process failure, VM loss, stop, transfer or resume; the user starts a new preview task explicitly. A preview alongside an owner’s task therefore does not block task scheduling or idle reclamation once other activity ends.

**Tracked-file contract.** A preview must not modify tracked files. G5 does not enforce this in the MVP because the command comes from the repository’s profile; a tracked change appears in Changes and is captured by a flush. A discard-on-write overlay is a later hardening option. A rebuild may stomp the running app; that and a reader cancelling the owner’s preview are accepted. Forking gives separate execution.

The browser reaches the preview through the API relay over the host tunnel (`http` streams, one per TCP connection) at `/api/v1/workspaces/:id/preview/:port/*`, with relay code carried forward from `routes/preview.rs`, target resolution keyed by workspace, current preview execution and port, a workspace-scoped ticket cookie, and optional subdomain mode. Current read access is enforced when opening/reopening the relay; a ticket never grants terminal, write or cross-workspace access. Ports opened by the owner’s agents appear in Ports from `ports.list` and use the same read-authorized relay.

Removed: `job_preview_sessions`, boot generations, `preview_reaper`, the `preview_vms` registry, chat-attach listener, token and host drive, and the separate per-task preview VM path. Preserve preview as a task kind and retain or adapt existing preview launch/task-queue UI until the replacement is working. A legacy preview job wrapper may be removed only after its launch, status, cancellation and history consumers use the preview task; deleting the feature/type from the task queue is prohibited. Agent and app share the workspace VM.

### 6.9 Terminal

A browser terminal (xterm.js) over a `pty` stream after `terminal.open`, gated by current workspace ownership (an org admin takes ownership first); the shell runs as uid 1000 in `/work` with the same environment the dispatcher gives modules (proxied model and git egress, task secrets). The PTY manager and the multiplexer exist today; the UI panel and the channel are new. Terminal activity counts as VM activity. Terminals are not recorded or audited in this iteration.

### 6.10 Fleet, placement, drain and rollout

**Hosts** are API-provisioned GCE instances, self-hosted Linux machines (including the Azure canary lane), or macOS dev hosts. A host is a set of equal-size VM slots (`G5D_HOST_MAX_MICROVMS`, VM CPU and memory from the host configuration), not a task pool. The reconciler (provisioning → ready, orphan sweep, `unreachable` after three missed heartbeats) stays. There is no maximum host lifetime: a host is drained only for an upgrade or by an operator, so a long-running workspace is never interrupted by a calendar.

**Placement** is a control-plane decision; the host only accepts or refuses. In order: a host in the org's region with a free slot that still holds this workspace's retained root; then any host in the region with a free slot; then `no_capacity`, surfaced to the user as `queued_for_capacity` with a position and retried by the dispatch loop. Org quota is `max_running_vms`, enforced before `vm.start`. A host that answers `slot_unavailable` despite the registry's count is authoritative and the control plane re-places; `workspace_busy` means the previous incarnation of the same workspace has not exited on that host yet.

**Drain and rotation.** Rotation is drain and replace: provision the replacement first, send `host.drain`, then for each VM run the stop sequence with reason `drain` (nudge, deadline, flush, release) and `vm.stop`; a VM whose flush fails is retried with backoff until the drain deadline, after which the teardown proceeds and records the loss; `host.drained` is followed by instance termination.

**Rollout of a new version.** Product versioning is per module and per component, with feature flags in `configs` for behavior that must switch independently of a deploy. The order is fixed by the version negotiation: the control plane deploys first and accepts the current and the previous host protocol version; hosts are replaced by drain-and-replace with the new runtime image; workspaces pick up a new VM image or dispatcher by a rollout drain (per workspace: nudge, deadline, stop sequence with reason `rollout`, release, and a resume with the new image on the next demand, immediately when the workspace had running work). An attempt interrupted by the rollout may continue after resume as a new attempt of the same logical task, preserving its session, job and dependencies; its input records the interruption and continuation intent (§7.2). Workers may be at most one version stale; the QA project exercises the sequence before production.

---

## 7. Task lifecycle (Agents)

### 7.1 Model

```
tasks(id, org_id, workspace_id, job_id NULL, kind agent|command|preview,
      status queued|running|waiting|succeeded|failed|cancelled,
      spec jsonb,        -- requested policy/defaults, initial input origin/template revision, limits, retry/continuation policy
      outcome jsonb,     -- final logical task result only; attempt outcomes are separate
      attempt int, vm_id NULL, epoch NULL, start_confirmed bool, depends_on uuid[],
      chat_session_id NULL, continuation_pending bool, continuation_reason NULL,
      hold_reason NULL, created_by, created_at, started_at, ended_at)

task_attempts(task_id, attempt, vm_id, epoch, policy_snapshot_ref NULL, started_at, ended_at, outcome,
              reason exit_code|provider_error|vm_lost|timeout|cancelled|interrupted|suspended,
              acked_event_seq bigint, PRIMARY KEY (task_id, attempt))

jobs(id, org_id, workspace_id, kind, status (derived), spec jsonb, created_by,
     max_concurrency, created_at, ended_at)          -- optional orchestration over logical tasks

task_events(id bigserial, task_id, attempt, vm_id, seq bigint, ts, kind, payload jsonb,
            UNIQUE (task_id, attempt, vm_id, seq))

provider_requests(id, task_id, attempt, vm_id, epoch, exec_id, session_id, provider,
                  provider_request_id, kind question|permission, tool_name, payload_ref, display_ref,
                  status pending|reply_pending|responded|invalidated,
                  response_ref NULL, response_version, responded_by NULL, response_deadline_at,
                  created_at, responded_at NULL, invalidated_at NULL, invalidation_reason NULL,
                  UNIQUE (exec_id, provider_request_id))
```

There are exactly three task kinds. Every `agent` task has a non-null session and invokes `agent.turn`. Human chat supplies typed input; a workflow supplies a rendered, versioned template as initial input. Both then use the same inbox, transcript, questions, permission policy, provider session files, turn adapter and task timeline (§8). The existing table names `chat_sessions`, `chat_messages` and `chat_session_files` are shared storage for every agent workflow, not a separate chat execution model. A `command` task invokes `workspace.bootstrap`; `preview` remains a service task with reader start/cancel, no bounded-work concurrency charge, no wall-clock timeout and no auto-retry.

A task is the stable logical unit that dependencies and jobs reference; an attempt is a provider/module execution. An interrupted, suspended or retryable-lost attempt does not terminally fail its logical task while a continuation remains eligible. A continuation increments `attempt` on that same task, with the same session, `job_id`, incoming dependencies and outgoing dependents. A conversational follow-up after terminal completion creates a new `agent` task in the same session; it does not reopen a completed dependency or silently join the old job. Job kinds remain profiles (`uplift`, `downlift`, `sync`, `explainer`, `agent_review`, `custom`); a session/task needs no job.

`provider_requests` is an execution-bound projection and reply outbox for actual provider-native requests. It is not a replacement custom ask-user tool or a durable questionnaire engine. No `successor_task_id`, provider-independent polling tool or answer-after-exit protocol remains. Historical `task_questions` content is preserved as read-only history during migration; old actionable rows are closed explicitly before retiring that engine.

### 7.2 State machine

```
queued ──invoke intent, then start confirmation──► running
running ──native provider request pending──► waiting (live attempt)
waiting (live) ──all native requests delivered/settled──► running
running | waiting ──continuable attempt ended──► queued or waiting (no live attempt; held)
waiting (held) ──authorized Continue/Resume──► queued (same task, next attempt)
running ──final logical success──► succeeded
running | waiting ──non-continuable failure / exhausted retry──► failed
queued | running | waiting ──owner cancel──► cancelled
```

Six states, one writer: `TaskService::apply(event)`. Persist both task state and attempt state; `waiting` is explicitly labeled as live native input or held continuation. `running` is recorded before invoking, but counts as execution only when start is confirmed (§7.5). The frontend must not imply that a held task still has a running process.

- **Dependencies and jobs.** Only final logical `succeeded` satisfies a dependency. A completed process with an interrupted/suspended outcome cannot release a dependent. Final failure/cancellation is propagated through the job’s explicit dependency-failure policy; no dangling permanent wait. Job aggregation includes held logical tasks as unfinished. Cancel cascades to the logical task and its pending continuation under one lock.
- **One unfinished task per session.** All entry points serialize the shared session inbox and permit at most one nonterminal logical agent task and one active attempt in it. New input waits behind live or held work; no-process status cannot bypass a hold. A job/template and a human message cannot launch two providers against one session concurrently. Distinct sessions may run concurrently in the shared workspace checkouts.
- **Owner authorization.** Only the current owner creates/cancels agent/command work or answers native requests. Readers may create/cancel preview only. All dispatch/continuation paths honor transfer admission and `owner_resume_required`; preview wake cannot release owner work. Each attempt resolves the current owner’s account, never a frozen former-owner account.
- **Native waiting.** A live Claude question/approval keeps its execution active while its bounded response deadline is open. Only the request’s current execution can consume a response. Timeout invalidates the request, interrupts and reaps the provider, harvests session state and settles that attempt as `suspended`; the logical agent task becomes held `waiting`, without a live request. An explicit Continue starts a fresh attempt in the same session; the provider may re-ask with a new request ID. Old answer controls remain disabled. Codex MVP has no interactive waiting state.
- **Interruption.** Stop/transfer ends the attempt and invalidates pending native requests. Preserve unfinished logical work, session and dependencies as held until explicit current-owner Resume/Continue. Drain/rollout may automatically queue the same logical task’s next attempt after release, according to its resolved continuation policy. Use SIGINT, then SIGKILL after 15 seconds; retain partial output marked stopped. No separate chat-only successor path exists.
- **Loss and retry.** Settle every active intent, confirmed or not, as `vm_lost`. An explicitly retryable task may queue one fresh attempt with the same logical identity; otherwise fail it. Never redeliver an old native request ID into a replacement process. Model-reported terminal failures do not auto-retry; an owner retry after terminal failure creates a new logical task, and an orchestrator must explicitly replace/rebind a failed DAG node if intended.
- **Cancellation.** Record cancellation before sending it; invalidate pending/reply-pending native requests and clear continuation eligibility atomically. A delayed answer, event, invoke or reconciler tick cannot restart cancelled work. Cancellation of a preview retains its special service semantics (§6.8).
- **Permission policy.** Every agent session/profile resolves an explicit policy and snapshots it per attempt (§8.3); human versus workflow origin never selects a privilege mode. Unsupported interactive Codex policies fail before launch; there is no silent escalation to a permissive mode.

### 7.3 Dispatch path

Human message send and workflow/template launch both persist a session input, then create its `agent` task with optional `job_id`/dependencies when that session has no unfinished task. Inputs behind live or held work stay queued; an explicit Continue reuses the unfinished task rather than creating another. `POST /workspaces/:id/tasks` validates the kind and shared session contract. The dispatch loop runs every 2 seconds and on demand:

1. Select eligible queued logical tasks whose dependencies are finally satisfied, excluding transfer/owner-resume holds and sessions with an active attempt.
2. If their workspace is stopped/failed, call `workspace.runtime.ensure` for the eligible demand; expose `queued_for_capacity`. Starting/stopping workspaces wait for lifecycle progression.
3. Lock the task and session. Reserve the shared inbox input/revision or persisted continuation input, resolve the current-owner account and freeze the explicit policy for this attempt, increment `attempt` only for a new execution, and persist `vm_id`, `epoch`, `running`, `start_confirmed=false`.
4. Invoke `agent.turn`, `workspace.bootstrap` or `workspace.preview` through the facade. Apply `started`, `already_running`, `already_finished` or `cancelled` dispositions without inventing a second attempt.
5. Re-send an unconfirmed start with the same `exec_id` only on its current incarnation. On completion, atomically persist the attempt, transcript and logical task/continuation decision; only then consider dependents or next session input.

A preview uses its one-active-preview gate, while bounded tasks require capacity. Both honor lifecycle admission. Native answer delivery targets a live attempt; it never calls `ensure`. Explicit Continue/Resume may create dispatch demand for a held task. Uplift bootstrap remains a `command` task whose orchestration creates ordinary `agent` tasks with shared sessions.

### 7.4 Jobs and dependencies across continuations

Jobs orchestrate logical tasks and derive status from them; they do not own a separate agent runner, permission shortcut or chat-less session model. Child sessions may run concurrently up to `max_concurrency` in the same member-repository checkouts. Every continuation remains on its original task/node, preserves `job_id` and both sides of the dependency graph, and reuses the same session transcript. A downstream task waits through a native question, held suspension, rollout continuation and retryable VM loss, and becomes eligible once when the logical prerequisite finally succeeds. Jobs may be cancelled; there is no rollback or separate job-merge task. Each agent commits/pushes its own changes and resolves native Git conflicts.

### 7.5 Dispatch and event contract

- **Execution identity.** `exec_id = (task_id, attempt, vm_id)`. Task identity stays stable across continuations; a fresh execution gets a new attempt and its own sequence space. Invocation retries keep the same identity.
- **Two-phase start.** Persist the invocation intent before sending it. Idempotent dispositions distinguish started/running/finished/cancelled. A missing confirmation is not evidence that no process ran.
- **Durable cancellation.** The dispatcher keeps a cancellation/finished disposition for the incarnation independently of journal pruning. A late invoke returns the disposition without spawning. Logical cancellation and continuation creation share a lock in the API.
- **Journal and replay.** Per-execution events under `/var/lib/g5d/journal/<exec_id>` have monotonic `seq`; insert-ignore on `(task_id, attempt, vm_id, seq)`, cumulative acknowledgements and ordered replay prevent duplicate transitions. An old attempt’s event may fill history but cannot mutate current state.
- **Outcome versus logical completion.** One C3 settles each attempt. In the same API transaction, update its assistant output and either finalize the logical task or set a held/queued continuation. Keep `tasks.outcome`/`ended_at` terminal only for real logical completion. No failed parent plus independent successful successor may bypass a job dependency.
- **Native responses.** Persist the owner’s response intent before dispatch; bind it to `exec_id`, `provider_request_id` and response version. The live adapter serializes writes and records delivery evidence. Replaying a UI/API intent must not write a duplicate native control response. If delivery or process continuity cannot be established, mark the request invalidated and surface uncertainty; do not claim the provider consumed it or replay it to a replacement.
- **Reconciliation.** On reconnect, replay journals and resolve start/cancel/response dispositions for the same live process. Confirmed executions missing from the dispatcher become lost; tasks already cancelled cannot continue. Response deadlines invalidate/reap live waits; ended attempts invalidate outstanding requests. Continue only through the stored logical-task policy and owner hold.
- **Shared transcript.** Every agent workflow uses `chat_messages` plus `task_events`. Transcript status and attempt/task settlement commit together; repair stranded pending rows from those authorities. Provider-native request cards are timeline projections, never a second completion authority.

---

## 8. Shared agent sessions and the provider runtime (Agents)

### 8.1 Model, history and the inbox

```
chat_sessions(id, org_id, workspace_id, title, provider claude_code|codex, model,
              permission_policy jsonb, provider_session_id, session_manifest_generation int,
              created_by, forked_from_id, forked_at_index, created_at, updated_at)

chat_messages(id, session_id, index, role user|assistant|system,
              content, attachments jsonb, revision int, input_origin human|workflow|continuation,
              template_ref NULL, template_revision NULL,
              delivery queued|reserved|delivered|delivery_unknown|withdrawn,
              status pending|streaming|complete|error|stopped,
              task_id NULL, attempt NULL, usage jsonb, created_at)

chat_session_files(session_id, provider, provider_session_id, relative_path,
                   bytes bytea, sha256, size, harvested_from_vm_id, harvested_epoch, updated_at,
                   PRIMARY KEY (session_id, provider_session_id, relative_path))
```

These existing `chat_*` names now hold every agent workflow’s shared conversation. Human chat creates initial typed input; a workflow renders its template and persists the exact input, template revision and resolved policy before launch. Thereafter both use the same session/inbox/adapter, may receive owner follow-up input, and expose the same history and timeline. Tasks have optional orchestration, not a second session system. One nonterminal logical agent task per session protects provider-session continuity, including a task held without a running process; at most one attempt may run for that task. New input remains in the session inbox behind running or held work and cannot bypass a hold by creating a second task. Different sessions may run concurrently.

Only the current owner sends/edits/withdraws input or changes session policy. Readers see the same history; chat-history search remains a Postgres feature and never depends on removed workspace search. History survives VM deletion. Transfer preserves transcripts, sessions and logical tasks, but new attempts use the new owner’s account after the explicit resume gate. Native request cards survive as history; live request authority does not survive the originating process.

**Inbox.** Persist input before dispatch. Queued input may be edited or withdrawn until reservation freezes a revision for one execution. `input.accepted` is journaled only after the adapter successfully hands the reserved prompt to its provider transport, not on module start: Codex receives prompt plus EOF; Claude receives the initial stream-json user frame while stdin stays open for native protocol replies. This records transport handoff, not proof that every downstream model side effect occurred. A pre-handoff failure releases the reservation; loss after an unacknowledged handoff is `delivery_unknown` and reconciles from the same-attempt journal/outcome before any retry. Do not promise exactly-once model side effects across an ambiguous VM loss. Delivered revisions remain transcript truth, withdrawn rows remain audit only, and session inputs are processed in FIFO order. General mid-turn human steering remains deferred; native control responses are a separate protocol channel, not a new user turn.

### 8.2 A shared agent turn

1. Human send or workflow launch persists a session input and creates an `agent` task only when the session has no nonterminal logical task. Further input remains queued behind running or held work. Only explicit owner Continue/Resume clears a hold on unfinished work and uses the same logical task; the absence of a provider process alone never admits another task. Once the prior task is terminal, the next input creates a new task in that session.
2. Dispatch reserves the input, resolves explicit policy/current-owner account and invokes `agent.turn` with the execution and session identities.
3. The adapter restores session files, launches the direct provider CLI, hands off the frozen input and journals acceptance and provider events into the common timeline.
4. Claude native `AskUserQuestion` and policy-governed approval requests become live `provider_requests` cards. The owner’s response returns over the native CLI control protocol. Codex MVP never exposes an interactive question/approval callback or custom MCP substitute.
5. On result/EOF/exit settlement, harvest session files and emit exactly one attempt outcome. TaskService atomically decides final logical completion or same-task continuation. No provider process remains between attempts; the shared session persists independently.

### 8.3 The `agent.turn` module and provider adapters

Agents owns this module; Workspace hosts it. C1 is command/lifecycle control, C2 is an ordered non-terminal event, and C3 is exactly one terminal outcome per execution. Invocation dispositions, native response commands and process-exit facts are C1. Transcript deltas, native question/permission observations and usage are C2. C3 settles the attempt, while TaskService separately decides whether the logical task is finished (§7.5). Provider terminal-looking frames remain candidates until the adapter verifies result, EOF and exit status; an unknown variant cannot imply success.

```
AgentTurnInput { task {task_id, attempt, session_id, input_origin human|workflow, initial_input?: reservation},
                 provider {provider_id, model_id, permission_mode, policy_ref, options},
                 prompt_ref, owner_proxy_binding_ref,
                 workspace_target {repo_id?, relative_path?},
                 session_binding {provider_session_id?, expected_manifest_generation?},
                 continuation? {prior_exec_id, reason interrupted|vm_lost|suspended},
                 limits {wall_clock_ms, max_turns?, max_output_bytes} }

AgentTurnEvent = provider.session | provider.status | input.accepted
              | message.delta | message.final | thought.delta | thought.final
              | tool.started | tool.progress | tool.completed | workspace.change
              | provider_request.opened | provider_request.replied | provider_request.invalidated
              | context.compacted | usage.updated | notice | terminal.progress | …

AgentTurnOutcome { status, code, retryable, final_message_ref?,
                   provider_session_id?, session_manifest_generation?, usage?, last_event_seq }
```

The normative schemas and adapter compatibility gates are in [agent-api-surface-v8.md](agent-api-surface-v8.md). No module input carries arbitrary argv, environment, host paths, credentials or signals. Requested capabilities are intersected with runtime policy.

**Explicit policy.** Resolve and persist the session/profile’s question policy, tool-approval mode, sandbox/filesystem scope, allowed tools and resource limits. Snapshot it immutably in `task_attempts.policy_snapshot_ref` for each agent attempt after current-owner authorization; continuing the same task must not overwrite a prior attempt’s effective policy. Human or workflow input origin is provenance only. Claude native prompting or an explicitly authorized non-prompting mode is selected by that policy; workflow launch never automatically supplies `bypassPermissions`. Codex MVP accepts only a compatible noninteractive policy (`approval_policy=never` plus an explicit supported sandbox); unsupported interactive policies are rejected before launch. A denied/unsupported action is surfaced, not silently broadened. Provider-native permission decisions and G5 PR approval decisions remain distinct authorities.

**Claude MVP.** Keep the direct pinned CLI process and Rust adapter. Use bidirectional stream-json: send the initial user frame, leave stdin open for native control responses, recognize a native `can_use_tool` request for `AskUserQuestion`, and return its typed answer in the matching control response. Native tool-approval requests use the same protocol with their own policy and schema. Do not disable `AskUserQuestion` or route it through MCP `ask_user`; retire the related custom `request_permission` prompt-tool adapter. The exact CLI flags, initialization and response framing must pass fixtures/probes at repository pin `2.1.270` before cutover; an installed newer binary or an SDK schema is not proof at that pin. The Claude Agent SDK is not the runtime.

**Codex MVP.** Keep `codex exec [resume <provider_session_id>] --json` at the repository pin (`0.153.3` at this revision), the prompt-on-stdin/EOF transport, noninteractive policy and structured JSON event/terminal handling. No interactive user questions, approvals, `g5d ask` or custom MCP fallback. If the model writes a prose question, it is ordinary output, not an actionable question card; the owner can send a later ordinary conversation message. A protocol-level unsupported interaction is a clear failure, not fabricated completion or a hidden blocking wait. Unrelated `g5-tools` MCP tools remain configured where allowed.

**Later Codex phase.** Migrate the adapter to `codex app-server` over stdin/stdout after the MVP, with a dedicated plan entry and acceptance gates (§8.7, §17.2). It will map native questions and approvals into the same request/timeline contract. The [official exec documentation](https://learn.chatgpt.com/docs/non-interactive-mode) and [app-server protocol](https://learn.chatgpt.com/docs/app-server) establish the separate transports; the implementation must still validate the selected binary’s generated schemas and actual behavior.

### 8.4 Native questions, approvals and request lifetime

A native request is bound to provider, session, logical task, attempt, VM incarnation and provider request ID. Persist the typed request as a timeline projection before showing an owner answer control. The same UI renders native questions for human-started and templated workflow sessions; readers may view but not answer. Validate the response against the provider’s question/options schema and recheck ownership, live execution and request state under the answer/cancel/transfer barrier. Store `reply_pending` before sending a C1 reply intent through `modules.input.respond` to the trusted adapter; it emits the native response exactly for the active request and records delivery evidence. Do not assert native acknowledgement when the protocol only proves a successful write; preserve ambiguous delivery explicitly.

There is no custom tool polling `/questions/:id`, global late-answer queue or question-created successor task. Repeated response submissions with the same version are idempotent; differing duplicates, stale IDs and answers after invalidation are refused. A reconnect to the same live adapter may reconcile its journal/disposition. A replacement process can never consume an old request ID, even when resuming the same provider session.

Cancel, stop, transfer, provider exit, VM loss or deadline expiry invalidates pending and undelivered native requests. The UI preserves their text and any recorded response as non-actionable history. On response-wait expiry the supervisor interrupts/reaps the provider and records `suspended` for the attempt; the logical task waits for explicit owner Continue, without holding the VM awake. Continue/Resume preserves the task/session/dependency identity and lets the provider issue any required fresh question. After transfer this is gated by the new owner’s explicit Resume and new account. A late click on the old card never starts compute. An invalidated permission approval is never reused to approve an action in a later attempt.

Live native waits are bounded by the response deadline and overall attempt limit. Timeout, response, cancellation and provider exit share a single arbitration path; only one attempt outcome settles. The timeline remains readable after VM shutdown, but a historical card is not an active provider request.

### 8.5 Provider session files in Postgres

Provider session state must survive VM loss and be independent of any VM. It is stored only in Postgres, in `chat_session_files`, and lives on the VM as a cache outside every working tree: Claude under `CLAUDE_CONFIG_DIR=/home/agent/.claude` (`projects/<escaped cwd>/<session id>.jsonl` and `<session id>/subagents/*`; `cwd` is always `/work/<root_path>`, so the project key is deterministic across incarnations); Codex under `CODEX_HOME=/home/agent/.codex` (`sessions/YYYY/MM/DD/rollout-<ts>-<session id>.jsonl`). After every turn, and again in the stop sequence, the module harvests files whose sha256 changed and upserts them (zstd-compressed, 128 MiB per file, tagged with the harvesting `vm_id` and `epoch`; a harvest from a non-current incarnation is rejected, so an older transcript never overwrites a newer one; `session_manifest_generation` increases with every accepted harvest). Before a turn that resumes, if the files are absent on the VM, the module restores them first. If Postgres has nothing for that session, or the CLI refuses the restored session before any frame, the module starts a new provider session and prefixes the prompt with a history built from delivered transcript rows, then records the new `provider_session_id`. Nothing about sessions is flushed into git, and no session file can be captured by a flush or a native `git add`. The bundle is opaque to Workspace and Worker & Host.

### 8.6 Fork and carry

Forking a chat copies `chat_messages` up to an index into a new session with `provider_session_id = NULL`; the first turn starts a fresh provider session with the history. Creating a workspace with "carry chats" does the same per selected session into the new workspace owned by the caller. Readers fork chats only into a workspace they own; they cannot create or mutate a session in someone else’s workspace. Provider-native session forking is not used.

### 8.7 Provider pins, gates and later transports

Repository pins remain `2.1.270` (Claude) and `0.153.3` (Codex) until an explicitly scoped upgrade lands. The direct Claude native-control path needs recorded fixtures and a real pinned-binary probe before replacing its old MCP prompt transport. Capture initial input, native multi-question/options answers, permission allow/deny, cancellation, timeout, lost response, process exit and session resume. Unknown/malformed frames, wrong request IDs, contradictory terminal evidence or nonzero exit cannot become success. Preserve fake-provider and real-boundary tests and session-harvest redaction/durability checks.

Codex MVP fixtures must prove `exec --json` remains noninteractive, unsupported interactive policy is rejected and no custom ask-user tool is registered. The later stdio app-server phase adds initialization/capability negotiation, thread/session resume, turn start/cancel, request correlation, native user questions/approvals, stderr/stdout separation and process-loss recovery using the chosen pin’s generated protocol. It retains the logical task/session/dependency and owner-authority contracts; migrate session handles/history explicitly and never fall back mid-interaction to exec. A capability gate controls rollout and preserves a clearly labeled noninteractive rollback lane.

A session-resident Claude process and general live steering remain separate deferred choices. Keeping stdin open during one Claude turn for native replies does not require an SDK or a process that survives between turns. The adapter-specific protocol detail lives in the versioned Agent API note; the old [provider CLI contract](lean-provider-cli-contract.md) remains current-code historical evidence and is superseded for the v8 native-input target.

### 8.8 Removed

`chat_sticky_runtimes` and every sticky column, function, trigger and heartbeat rider; `required_worker_id`, `branch_mode`, `attach_preview_task_id`, `chat_sessions.status`; `chat_worker_loss`; the session-store proxy (deleted); context links; task-scoped session types; the Claude Agent SDK and both Node sidecars (deleted); the `fake` and `naive_react` provider values (deleted).

---

## 9. Documents and editing

### 9.1 Reads

`GET /workspaces/:id/files/*path` (and the docs and ontology content routes that wrap it):

| VM state | Source | Marker |
|---|---|---|
| running | `fs.read` through the facade | `source: live` |
| stopped, complete receipt present | the receipt's snapshot tree in the internal store | `source: flush` |
| stopped, no receipt; base workspace | branch head in the internal store | `source: committed` |

Owners and readers use identical read routes and sources; reads never resume the VM. Access is checked on every request and stream admission. The file tree and the Changes panel follow the same rule; while stopped, the Changes panel shows the receipt's dirty set.

### 9.2 Writes

Save sends `PUT /workspaces/:id/files/*path` with `expected_hash`. The API authorizes the current owner, refuses a stopping workspace with `WORKSPACE_STOPPING`, **ensures the VM is running** (a stopped workspace is resumed and the UI shows "resuming workspace", or `queued_for_capacity`), then asks that incarnation for the association (`workspace.changes get`, §5.7) and calls `fs.write` with the same `expected_vm_id`. A hash mismatch returns 409 with the current content; the editor shows a reload-or-overwrite choice. Agent writes reach the UI through `fs.watch` → `file.changed`; an editor with no local edits reloads silently, one with local edits shows "changed on disk". Changes retains diffs/status and Push for existing commits, plus the retained update/association controls. Remove Commit, commit dialogs/shortcuts, `workspace.changes commit` and all UI/API commit convenience handlers. Agents decide regular native commit/push cadence; Save, job launch, PR creation and lifecycle flush never invoke a hidden user-commit path. The implementation sequence waits for the real VM write facade before replacing CRDT-backed Save; it adds no interim server-commit Save API. There is no Flush now control or public manual-flush route. The existing owner terminal/native Git capability is unchanged; this removes platform-managed commit controls rather than introducing shell command policing.

### 9.3 Editors

Monaco and the block-style spec editor keep their UI but bind to local document state with Save (Ctrl+S) and optional autosave-on-blur, instead of a Yjs text. The structured question editors and explainer views follow the same pattern; the question-document view that already renders without Yjs is the template. Comments anchor to `(path, line, line hash)`. There is no presence and no co-editing; because all writes go through one Save API, a document-only CRDT can be added later without changing persistence.

### 9.4 Projections and convergence

`docs` and `ontology_nodes` remain projections of git files; workspace search and its embedding/index projections are removed. The event bus is a non-durable fan-out and a ref can be updated immediately before a crash, so projections never depend on an event: each records the OID it was computed from per `(repo_id, ref)` in `projection_state`; a convergence loop at API startup and every 60 seconds lists the authoritative OIDs of every workspace branch, every complete receipt's snapshot ref and every protected branch, compares them with `projection_state`, re-projects what differs and removes rows for refs that no longer exist. `branch.updated` and `flush.completed` events trigger the same work early. PR `head_sha` and `workspace_repos.head_sha_cache` are projections under this contract.


### 9.5 Workspace search removal boundary

Remove workspace search end to end: the global/workspace search box, result panels and navigation/shortcuts; workspace search endpoints and clients; text/vector/embedding search indexing, reindex queues/workers and scheduled jobs; search-specific tables, columns, indexes, settings, provider credentials/configuration, feature flags and events; search-only tests, fixtures, benchmarks, seed/backfill/reindex tools, commands and deployment wiring. Remove APIs and consumers together; an inert control, empty endpoint or permanently disabled worker is not removal. Applied migrations remain immutable; drop search-owned data with forward migrations after its writers/readers retire.

Retain chat-history search over session messages, editor Find, and agent-local code search such as `rg`, native provider tools and permitted unrelated MCP tools. Retain document/ontology projections, their convergence/rebuild work and shared infrastructure they still consume. Classify each indexer/embedding dependency by its actual consumers before deleting it; a process named `g5-api-indexer` must be split or retained for document/ontology projection work if that work shares it. The search feature cannot remain reachable through an ontology endpoint or legacy client alias. Search removal does not remove docs/ontology reads, graphs, grounding or chat history.

---

## 10. Governance

Governance stays in the design and is built the week after the MVP milestone; nothing in the MVP may block it (§13).

### 10.1 Protected refs

`repos.protected_branches` holds the default branch and any branch a base workspace tracks. Receive-pack denies every network push to those refs (§5.3). At repo connect time the API reads the provider's branch protection for the default branch and warns when G5 approval rules and provider rules disagree. Both gates apply.

### 10.2 Pull requests

```
pull_requests(id, org_id, workspace_id, repo_id, source_branch, target_branch,
              status open|merged|closed, head_sha, title, body,
              provider_pr_number NULL, provider_pr_url NULL, published_sha NULL,
              merge_op_id NULL, merge_state NULL|pending|provider_merged|applied|failed,
              merge_source_sha NULL, merge_target_sha NULL, merge_result_sha NULL,
              merge_error NULL, created_by, created_at, merged_at, merged_by)
```

Created by the current owner from the UI or by their agent with `g5d pr create`; a reader or non-owner admin must fork or take ownership first. Creation reads the source OID from the internal branch at that moment, publishes the branch to the provider at that OID and records it as `published_sha`, and opens a provider PR when the repo has a provider; the provider PR is a mirror for external reviewers, not the source of truth for merging. A PR is created from the existing internal branch OID. If `workspace.changes get` reports dirty or unpushed state, refuse creation with a clear not-ready result and show that the agent must finish its checkpoint/push; an owner may use retained Push for already-created local commits. Do not offer Commit and push all, silently commit, or publish a flush snapshot onto the branch. The agent commits/pushes before `g5d pr create`; a flush is not PR publication. `head_sha` is a projection of the source branch (§9.4); every decision and the merge operation use OIDs they read from git themselves.

### 10.3 Approvals

```
approval_rules(id, org_id, repo_id, target_branch, required_approvers uuid[],
               min_approvals int, agent_reviewer_profile NULL, created_by, created_at)

approval_decisions(id, org_id, pull_request_id, source_sha, actor_user_id NULL,
                   actor_task_id NULL, outcome approved|changes_requested, note, created_at)
```

Recording a decision from a workspace requires its current owner (or the current incarnation acting for that owner), plus the approver eligibility below. Reader access alone never authorizes it; a non-owner admin takes ownership first. A decision binds to the exact `source_sha` the approver reviewed and is written only by an API route: the UI records the OID it displayed; an agent review task is launched with the OID it must review and `g5d pr review` records that OID through the API. A new push makes prior decisions stale. Approvers are the rule's list plus org admins. Approval state belongs to the PR, never to VM state, and every approval comes with explicit instructions on what an approval means (the review flow is defined with the UX team).

### 10.4 Roles and login

Org roles `owner|admin|member|viewer` and the org visibility/invite read model are kept. `workspace_members` grants read access only; `workspaces.owner_user_id` is the sole human writer. Org admins may read any workspace and transfer ownership to any org member, including themselves, but must take ownership before writing. The workspace owner can transfer to any org member. Read access includes clone/fetch, preview start/cancel and forking into a new owned workspace; the capability matrix in §5.1 governs all routes. Per-user scope grants, workspace permission overrides and workspace mutation policy are dropped. RLS tenant isolation stays. Platform login is GitHub in the SaaS; self-hosted installations add Microsoft Entra (§16.3). `user_scm_connections` keeps GitHub and GitLab repository credentials for publish and PR operations. Operator permissions (who creates organizations) are not part of this work.

### 10.5 Removed

`approval_definitions/configs/requests/statuses/review_runs`, `merge_evaluations`, `agent_approval_handoffs`, `job_merge_runs/artifacts`, `workspace_pull_request_events` and `_provider_repos`, coordinated repo publication, workspace merge / rebase-merge / change-graph services, stages and merge-target columns, the non-GitHub login adapters (Entra returns for self-hosted only).

### 10.6 The merge operation

`POST /pull-requests/:id/merge` is the only path for user-requested protected-ref changes. Repository bootstrap and provider imports are separate API-only paths (§5.2, §5.6); a workspace fork never rewrites an existing protected ref. Merge is a persisted, resumable operation with one identity (`merge_op_id`) and pinned inputs kept until the operation is resolved; a retry of an unresolved operation never re-pins.

1. **Pin.** If the PR has an unresolved operation (`pending` or `provider_merged`), resume it at the step its state indicates; otherwise, under a per-repository lock that also serializes inbound fetches (§5.6), read the source and target OIDs from the mirror and store them with a new `merge_op_id` as `merge_source_sha` and `merge_target_sha`, `merge_state=pending`.
2. **Authorize.** PR is `open`; the caller may merge; decisions whose `source_sha` equals `merge_source_sha` satisfy the rule.
3. **Mergeability.** Fast-forward or clean merge of source onto target, computed on the bare mirror without writing refs.
4. **Provider-connected target.** Republish the source at `merge_source_sha` if `published_sha` differs; call the provider merge API with the expected source SHA; on success record `merge_result_sha` and the durable `merge_state=provider_merged`; fetch the target from the provider into the mirror under the same lock (the mirror never computes its own merge commit for a provider-backed target); record `applied`, `status=merged`, `merged_at`, `merged_by`. An import failure leaves `provider_merged` and is retried. Ambiguous replies are reconciled on retry by reading the provider PR: attribute a remote merge to this operation only if the provider reports the pinned source as the merged head; if the head changed, or it merged at another head, record `failed` for a re-run with fresh inputs.
5. **Local-only target.** Compute the merge commit on the mirror, persist its OID as `merge_result_sha` before touching refs, then in **one ref transaction** (`git update-ref --stdin` with `start`/`prepare`/`commit`) compare-and-swap the target from `merge_target_sha` to the result and create `refs/g5/merges/<merge_op_id>` at the result. On recovery, the receipt ref proves application whatever the target holds now (a later fast-forward or rewrite of the target does not hide it); a missing receipt with a target that is neither `merge_target_sha` nor the result means another writer moved the target before application, and the operation records `failed` for a re-run with fresh inputs. Receipt refs are pruned when the PR closes or 30 days after `applied`.
6. **Failure.** Terminal `failed` is recorded only for a known rejection or conflict before anything was applied. Once the provider has merged, or the receipt ref exists, the operation can only progress to `applied`. There is no rollback because nothing that was applied needs undoing.

PRs may also target another workspace’s branch; those targets are not protected, so the operation follows step 5 and skips the provider. Updating that target requires its current workspace owner’s authorization; source ownership, read access or org-admin status alone does not grant target write access. Recheck the target owner under the target workspace’s mutation/transfer barrier before the ref transaction so an in-flight merge cannot bypass a transfer.

---

## 11. Agent surface

### 11.1 Keep (platform operations)

| Command / tool | Purpose |
|---|---|
| `g5d pr create\|status\|review` | open a PR, read its state, record an agent review decision pinned to the reviewed SHA |
| `g5d task status\|list` | inspect tasks and jobs |
| `g5d preview start\|stop\|status`, `g5d ports` | control the preview (one per workspace), list open ports |
| `g5d auth`, `g5d doctor` | CLI login for laptops and diagnostics |

All of these reach the control plane through the credential proxy (§6.5) with `G5D_EXEC_ID` naming the execution; none holds a token. Agents cannot create workspaces from inside a workspace; sub-agents inside a workspace are the main agent's business and untracked.

### 11.2 Native

Git (`commit`, `push`, `pull`, `rebase`, `fetch`), files, shell, tests, package managers. `origin` is the internal git service through the proxy rewrite, with the platform excludes file configured. Agents are told: stay on the branch; other agents may be working in the same folder and the last writer wins; commit often; push to `origin` when a unit of work is done, because nothing else saves your work while you run; rebase each member repository onto its own configured upstream base ref to update; push rewrites only with `--force-with-lease`, never bare `--force`, and after a fetch prefer the explicit form `--force-with-lease=<branch>:<expected sha>`; never push to protected refs (the server rejects it anyway); when the platform asks you to checkpoint before a teardown, commit and push at once.

### 11.3 Removed

`g5d ask`, MCP `ask_user` and the related custom `request_permission` adapter; workspace search tools/routes; platform commit and manual-flush controls; `g5d commit`, `push`, `connect`, `conflicts`, `revert`, `fork`, `refresh`, `merge`, `sync`, `view`, `spec sync`, `task rebase`, `task complete` (TaskService derives logical completion from attempt outcomes), `workspace push|switch`, `job merge`, `chat context`, `intent-store`, `issue *`; the internal tool registry; the four issue MCP tools; the session-store proxy; MCP preflight reduced to a startup self-check. Most of these are already gone from `main`.

### 11.4 Profiles

`CLAUDE.md` and `AGENTS.md` describe native git, the branch rule, the shared-folder rule, the `--force-with-lease` rule, the checkpoint-on-request rule, `g5d pr` and provider-specific native-input capability. Agent-local Git is the commit mechanism; remove the product commit shortcut and custom ask-user instructions, and use `g5d pr create` for PR publication; hooks are `SessionStart` and `Stop` only. Existing profiles on `main` still require the v8 policy/native-question/search/commit-shortcut cleanup and `g5d pr` switch; they are not evidence that this target is implemented.

---

## 12. Data model and migrations

**Identity and tenancy:** `orgs`, `users`, `org_memberships`, `org_invitations`, `user_scm_connections`, `api_keys`, `refresh_tokens`, `oauth_states`, `cli_device_authorizations`, the `authz` schema, `self_hosted_licenses`.

**Workspace and git (§5):** `repos`, `repo_secrets`, `workspaces`, `workspace_repos`, `workspace_members`, `workspace_forks`, `workspace_fork_reservations`, `workspace_ownership_transfers`, `workspace_settings` (idle timeout, native-response deadlines, retain hours, explicit policy defaults), `workspace_vms`, `workspace_flushes`, `projection_state`.

**Worker & Host (§6):** `worker_hosts` (host id, org, region, kind `gce|self_hosted|macos_dev`, status with `unreachable`, slots and slot size, `runtime_identity`, capabilities, credential expiry, drain and heartbeat times, cloud instance fields).

**Execution and chat (§7, §8):** `tasks`, `task_attempts`, `jobs`, `task_events`, `provider_requests`, `chat_sessions`, `chat_messages`, `chat_attachments`, `chat_session_files`, `usage_records`, `provider_accounts`, `provider_quota_snapshots`.

**Governance (§10):** `pull_requests` (with the merge-operation columns), `approval_rules`, `approval_decisions`.

**Projections and product features:** `docs`, `ontology_nodes`, `node_dependencies`, `ontology_grounding`, `comment_index`/`comment_replies`/`comment_mentions`, `notifications`, `user_preferences`, one `configs(scope, scope_id, key, value)` table for org, user and workspace configuration and feature flags, `platform_issue_outbox`.

The retained inventory above is authoritative; the implementation plan measures its final table count instead of preserving the old approximate count after removals. Everything else in the baseline (draft, task-commit, head-operation, advance, `workspace_branches`/`workspace_branch_repos`, branch-integration, sync-run, job-merge, approval-ledger, sticky-runtime, `workers`, `worker_tokens`, execution-attempt, question-window, escalation, preview-session, golden and builder tables, issue and hotlist tables, and the rest of draft 6 §12's dropped list) is removed by the PR that deletes its last consumer.

**Migrations.** `001_lean_baseline.sql` is frozen (checksum-guarded); no PR edits it. Every schema change is a forward migration `NNN_<description>.sql` with its SHA384 appended to the checksum catalog, numbered in merge order, validated as a fresh install and as an upgrade from the previous schema on a copy of real rows, with the no-op re-run checked. Dev and QA databases persist across deploys; table removals use forward drop migrations, never a baseline rewrite. Provider enum everywhere: `claude_code | codex`. Task and chat status vocabularies are one enum each in the database, Rust and the frontend. The forward ownership migration backfills one eligible org member per workspace (use its creator only when that is a valid unambiguous owner); missing or invalid owners require explicit resolution before rollout, never silent admin assignment. Collapse existing admin/editor/viewer or write/read workspace membership roles into read invitations, preserve visibility, add the transfer operation/audit and `preview` task kind, and update all read/write authorization consumers before removing legacy role code. Original creator and legacy roles never remain fallback write authorities. Add durable fork operations/reservations and migrate every workspace-branch-creation producer to the checked fork. Store repository-specific upstream/base refs on `workspace_repos`; keep only the enforced common working name on `workspaces`. Base views preserve existing protected/default names. The plan stages the new mapping additively in P4.1, preserves retained-path compatibility, then reconciles and enforces strict name equality when P4.5 cuts every branch creator over to fork. Retire search producers/consumers before forward-dropping search-owned data/settings. Migrate `chat_turn` to `agent`, backfill sessions for workflow agents, map historical continuation chains without losing dependencies, and retire the active custom question engine after preserving its rows as read-only history; only fresh live provider requests populate the native projection. Unsupported permission policies require explicit resolution; no origin-based bypass. Preflight existing workspace names and member refs for the shared-name invariant; do not silently rename branches, invent UUID suffixes or coerce divergent mappings to satisfy the migration.


The immutable baseline currently needs vector types/extensions during fresh-install replay. Retain only the migration/bootstrap compatibility needed to apply that history, then remove live search-owned tables/settings and vector usage through forward migrations after verifying no retained consumer. Do not keep a runtime search/embedding service merely because the historical migration references its types, and do not break fresh installation by deleting its replay prerequisites. Historical receipts/questions retain truthful read-only provenance without reopening removed manual-flush or custom-answer APIs.

---

## 13. Delivery

The plan is [lean-implementation-plan-v8.md](lean-implementation-plan-v8.md); this section fixes what it must respect.

**Owners.** Every PR names the capability that owns it (§4.1); the owner reviews and merges it. A component is owned by the capability it serves, not by the process it runs in: the `agent.turn` module runs in the VM and is Agents'; the `ctl` client runs in g5-api and is Worker & Host's; the lifecycle service runs in g5-api and is Workspace's.

**MVP milestone.** The nine proposals on the GCP QA environment: a workspace with a VM, a dispatcher and modules; explicit Save and the Changes panel with no CRDT; agent-driven durability with the flush at graceful teardown and restore from the receipt; one `agent` kind and shared sessions for typed/templated input, with same-task continuations and explicit policies; Claude direct CLI native questions/approvals and Codex noninteractive exec; agent tasks and jobs on the dispatcher; a visible, cancellable preview task and owner-only terminal on the VM; workspace search and manual Commit/Flush now controls removed; single-owner sharing, audited transfer and reader preview controls; the host protocol with proxies; the incarnation fence; the git boundary (protected refs, same-named member branches created only by collision-checked workspace fork, native updates to existing mapped refs). Not in the MVP: PR and approval UI on the new tables and the merge operation (the week after), the later Codex stdio app-server adapter, the self-hosted phase, the crate consolidation and CI review, the remaining deletions that do not block the above.

**After the MVP, in order of priority.** Governance (P6.x) and the remote-tracking deletion and publish (P1.5, P1.6); the pre-launch audits of the APIs and data model (security, fault tolerance, scalability, with the table list written down after the APIs); lifecycle and schema cleanup (P7.x); the self-hosted phase (§16); the later provider track (Claude process reuse and Codex stdio app-server); crate consolidation and the CI review.

**Ownership cutover.** While legacy per-task or sticky runtimes remain, transfer cannot complete by stopping only the new `workspace_vms` incarnation. P4.5 refuses transfer for a workspace with retained active legacy execution until that work is drained or migrated, and enforces current-owner authorization across those producers during the transition. P4.7/P4.8 remove the temporary restriction only after their producer/consumer cutover proves no old-owner execution or credentials can survive transfer.

**Rollout procedure** is §6.10; the QA project rehearses control-plane-first deploys, a host drain and a workspace rollout before the first production deploy.

---

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Agents forget to push and a host is lost | Profiles say push when a unit of work is done; the nudge before every deadline-bound teardown; the loss is bounded to the last receipt and shown, never silent |
| Two forks pass a name check, or one creates only part of the repository vector | Durable reservations over all member repos, create-if-absent transactions with fork receipts, idempotent recovery and no usable destination before completion (§5.8) |
| A flush publishes a partial multi-repo cut | Immutable per-flush refs plus a receipt completed only after the API verified every ref; consumers read receipts only (§5.4) |
| A live flush cuts a moving tree | Quiescence: module and file-mutation admission closed and drained, bounded runs ended or interrupted; terminals and previews recorded as concurrent writers (§5.4) |
| A message is marked delivered before the provider received it | `delivered` follows the adapter's `input.accepted`, never the module's `started` disposition (§8.1) |
| A native request outlives its process | Invalidate on process end; held logical task needs explicit Continue and a fresh native request (§8.4) |
| A fenced host refuses the stop meant to free its slot | `vm.stop` is fenced by ownership only, and release follows the host's exit (§5.8, §6.2) |
| A warm root differs from the receipt with the same paths dirty | The receipt is always materialized on attach; no path-based equality check (§5.5) |
| A host lease outlives the control-plane grant | The host arms its timer from the request's monotonic start; boot leases carry the command deadline; proxies stop at the host deadline (§6.4) |
| A superseded VM reconnects and keeps writing | Incarnation fence at every boundary, including the proxies; reconnect kills stale VMs (§6.4) |
| A cancelled start launches after journal pruning | Dispositions are a separate small table retained for the incarnation (§7.5) |
| An unconfirmed start is stranded by incarnation loss | Loss settles every intent bound to the lost incarnation (§7.5) |
| A late native answer restarts cancelled work | Answer delivery cannot wake; invalidate under the same task/cancel/continuation lock (§7.2, §8.4) |
| Two attempts on one VM collide in the timeline | Event keys and watermarks carry the attempt (§7.5) |
| A stopped workspace never receives its work | Dispatch selects work regardless of VM state and ensures the runtime (§7.3) |
| Save fails on a stopped workspace | Ensure the runtime, then query the association on the same incarnation (§9.2) |
| A crash between a command and its outcome | Two-phase invocation, journaled events, retained outcomes, reconnect reconciliation (§7.5) |
| A question blocks a VM forever | Bound the native-response wait; reap and hold the same logical task; invalidate the old request and let the VM idle (§8.4) |
| Credentials leak into a provider process or across users | Host proxies inject the incarnation owner’s account on egress; transfer releases the old incarnation before rebinding; the token only in the dispatcher (§6.5) |
| A rollout interrupts working agents | Nudge, deadline and flush, then a same-task continuation with session/job/dependencies intact (§6.10, §7.4) |
| Old hosts receive commands they cannot parse | Protocol and spec versions negotiated; control plane first (§6.2) |
| Backups restore a database whose git objects are gone | Backup barrier across Postgres and the git store (§16.4) |
| Claude print-mode behavior differs from what the adapter expects | Pinned CLI, recorded fixtures, exhaustiveness gates on every bump (§8.7) |
| Codex exec lacks native interactive input | Noninteractive MVP policies only; no question/approval UI or MCP fallback; scheduled stdio app-server migration (§17.2) |
| A reader cancels the owner’s preview | Accepted nuisance; the task records the actor and the owner can restart it |
| A preview modifies tracked files | Profile contract only in the MVP; Changes and flush expose/capture the edits; a discard overlay is deferred |
| The owner is unavailable | An org admin transfers ownership; live pairing requires transfer or a fork |
| A transfer changes the owner before old work stops | Persisted stop barrier, flush, host exit/release, then atomic owner/audit commit; abort on flush failure |
| A provider process never exits | `max_turns`, `wall_clock_limit`, SIGINT then SIGKILL |
| Session-file restore fails | Fall back to a new provider session with the delivered history (§8.5) |
| Several agents in one folder overwrite each other | By design; agents are told; the UI notices |
| Owner force-push discards an unseen commit | Profiles require `--force-with-lease`; only laptop-versus-VM races can hit it |
| Approval, publication and merge drift apart | One pinned source OID; provider merge with expected SHA; receipt ref for local merges (§10.6) |
| An event is lost after a ref update | Projections converge from authoritative OIDs (§9.4) |
| Reads while stopped are only as fresh as the last flush | Every graceful teardown flushes; the UI labels `source: flush` |
| Single API replica | The tunnel registry and event bus are process-local; moving them to Redis is deferred (§17.2) |
| Removing workspace search accidentally removes other reads | Inventory by producer/consumer; retain chat-history search, editor Find, local agent search and doc/ontology projections (§9.5) |

---

## 15. Validation

The Rust end-to-end crate and the Playwright flows are pruned per removed feature and extended per PR with tests that fail without the new behavior and exercise the production boundary. Beyond draft 6's list, which stays in force where the mechanism is unchanged, this draft requires:

- **Workspace mapping and fork:** onboard multiple repositories with different protected/default source names and fork them into one workspace whose branch name is the exact user name in every member repo; reject an existing name in only the second/code/docs/provider repo with no destination refs created and a different-name field error. Test Git-invalid names and prefix conflicts, two concurrent forks with partially overlapping member sets, duplicate requests, changed source membership/access, crash after each member ref transaction and before workspace publication, resume/rollback of partial creation, and cleanup refusal if a created ref changed. No suffix or existing-branch adoption is permitted. Test direct user/incarnation push creation, missing-ref ensure/restore, rename/member-add and legacy create/clone paths cannot bypass fork; Fork and preview/chat use the same operation. Initial provider publication refuses a remote branch created after preflight.

- **Flush and receipt:** a shutdown flush of a two-repository workspace whose second push fails, and separately whose receipt commit fails, leaves the previous receipt as the restore point after a deadline-bound teardown and a cold resume, and the retained refs still resolve; a fork of a running workspace while an agent updates two related files is refused or interrupted per the quiescence rule and the fork restores a stable tree; a Save admitted just before a fork flush and released while the capture runs is waited for and the cut is consistent, also for delete and rename and across two repositories; both admissions reopen after a refused and after a failed live flush; a user stop whose flush fails leaves the VM running with admission reopened; a drain whose flush fails proceeds at the deadline and records the loss; pruning keeps the two newest complete receipts and their objects; a receipt whose ref the API cannot verify is never completed.
- **Restore:** committed-but-unpushed commits on a clean tree restore with the branch ahead of origin; local commits plus dirty edits restore both; a tracked file matched by `.gitignore` with a dirty edit is captured and restored, including a same-second same-size edit; a warm root whose file contents changed after the last receipt with the same `HEAD`, dirty paths and sizes attaches with exactly the content a cold attach gives; provider session files are never captured by a flush or a native `git add -A`.
- **Host protocol and lease:** hello, accept and reject with an unsupported version (the host self-drains and keeps its VMs); a `vm.start` whose result is lost is resolved by the inventory; a `vm.event exited` survives a tunnel drop; a delayed renewal reply followed by a partition expires the host before the server grant, the proxy refuses traffic and a replacement is placed only after; a command delivered after its deadline is refused; a stale incarnation is refused on every command, stream and proxy request, while a draining one is accepted; two incarnations of one workspace on one host are refused with `workspace_busy`; `vm.stop` for a superseded `vm_id` the host still runs stops it and frees the slot while `vm.start` and stream opens for it are refused; the row stays `stopping` until the host's exit and a lifecycle restart after `vm.stop` re-sends it and then releases; a restart of the workspace right after a stop never sees `workspace_busy`.
- **Proxies:** every session uses the incarnation owner’s provider account, never a session creator or preview launcher; an untagged request or missing owner grant is refused; tags meter usage without selecting another user’s credentials; same-owner key rotation takes effect on the next request; no credential appears in provider environments; after transfer old grants fail and new-owner turns bill the new account; internal git pushes succeed without a token in the provider process.

- **Modules:** `modules.invoke` returns the right disposition for started, running, finished and cancelled identities; the module input cannot carry `argv` or host paths; a module exceeding its limits is stopped with a definite outcome; catalog integrity is probed.
- **Dispatch and events:** loss before delivery and after spawn with lost reply settles each attempt; same-incarnation idempotent retry; durable cancel followed by delayed invoke never starts work. Test `A → B` dependencies through A’s live native question, held suspension, drain/rollout continuation, retryable loss and cancellation: task/session/job identity stays fixed, B waits until final A success and dispatches once. Delayed old-attempt events/acks cannot mutate the current attempt. Job aggregation never completes while a child has a pending continuation.
- **Native questions and permissions:** pinned Claude initial stream-json input plus native `AskUserQuestion`, multiple options, answer, permission allow/deny, duplicate reply, malformed/wrong ID, timeout, cancellation, lost acknowledgement, process exit and transfer races. A live answer travels through native control response; stdin remains open only for protocol traffic. Ended requests are read-only history, stale answers cannot wake/continue, and owner Continue preserves task identity while any new question gets a fresh ID. Codex MVP rejects interactive policy, registers no custom ask-user tool and treats prose questions as ordinary output. Later app-server stdio gates cover native requests/approvals and thread/turn resume before enabling them.
- **Removal boundaries:** workspace search UI/shortcuts/routes/index workers/embeddings/data/settings/tools/tests are gone; retained chat-history search, editor Find, agent-local search and docs/ontology reads/convergence still pass. Commit UI/API/module operations and Flush now/manual flush endpoints are absent, and `explicit` requests are rejected; shutdown/fork still restore uncommitted work without advancing the working branch. Unrelated MCP registrations and tests remain.
- **Save:** Save with no VM and a cold restore reaches resume before any module query, then exercises a matching branch and a mismatch.
- **Shared sessions and inbox:** typed human and templated workflow starts both create `agent` tasks using the same tables/adapter, explicit policy and transcript. Same-session launches serialize across both live and held tasks, so a queued message cannot bypass a hold; distinct sessions run concurrently. Input can queue/edit/withdraw until frozen; transport acceptance differs correctly for Codex EOF versus Claude open control stdin. Pre-acceptance failures requeue; uncertain handoff is labeled and reconciled without claiming exactly-once model side effects. After terminal completion, follow-up creates a new task in the same session. Transfer rebinds credentials for all origins without an origin-based permission escalation.
- **Workspace actions:** create, stop, resume, restart, delete and transfer; stopped transfer without VM work; failed flush abort; crash/retry after flush/release; stale-owner Save/push/native reply/dispatch. Transfer preserves history and held logical tasks but invalidates old provider requests; fresh requests after explicit new-owner Resume use the new account. Reader preview wake never dispatches held owner work. Base never has a VM; capacity wait remains visible.
- **Rollout:** the control plane accepts the allowed previous host protocol; drain nudges/interrupts, flushes and releases before a fresh attempt continues the same logical task/session/job/dependency node on the new image. Native request IDs are never reused across that process boundary.
- **Governance:** PR creation refuses dirty/unpushed state without Commit and push all, hidden commit or flush-to-branch publication; existing-commit Push and agent checkpoint work. Preserve OID-pinned approval and local/provider merge recovery, including a target advanced after the operation receipt.
- **Backup (self-hosted phase):** a workspace deleted or rewritten during a backup is either wholly in or wholly out of the restored installation; every restored receipt resolves to retained objects.
- **Sharing:** owner, org admin, org-visible reader, invited reader, removed reader and outsider exercised through real API/UI routes, runtime facade and git boundaries. Readers see identical live/stopped data without waking; only owner writes, uses terminal or starts/cancels non-preview work. Admin writes fail until transfer. Former creator and legacy editor/admin membership confer no residual write authority. Reader forks create an owned workspace without waking the stopped source or interrupting source owner work.
- **Preview and terminal:** button → queued preview task → normal dispatch/wake → service readiness still `running` → Cancel in task queue → stopped process and terminal task; any reader cancels another user’s preview. Exercise full bounded concurrency, no wall-clock timeout, idle traffic/idle reclamation, owner credentials despite reader launch, replacement racing start/cancel/stop, lost start replies and stale port events, and no automatic restart after VM loss/stop/transfer/resume. Preview tracked writes are visible in Changes and a flush (contract is not enforcement). Ticket scope is one workspace and current read access; terminal requires owner and closes on transfer.

---

## 16. Deployment targets: the GCP SaaS and self-hosted installations

### 16.1 GCP SaaS and the QA environment

The QA project `g5-qa-1ccc` is an isolated copy of the platform (own VPC, Cloud SQL, Redis, Filestore, KMS, Secret Manager, Artifact Registry, GKE cluster, delegated DNS) built by the Terraform roots under `infra/qa/` and documented in `infra/qa/README.md`. Cloud Build runs the PR lanes (`g5-ci-rust`, `g5-ci-e2e`) on every pull request and the deploy and worker-image lanes on pushes to `main`; the migration checksum guard runs in both PR lanes; deployment pauses are a Terraform variable; the one-time database reset is a runbook. Both `g5-ci-rust` and `g5-ci-e2e` must pass on the current PR head before merging, alongside relevant local validation; an inherited failure still keeps a PR open, as required by `AGENTS.md`. Production and staging follow the same roots with their own inputs.

### 16.2 Self-hosted installations: what is retained

The self-hosted control plane on pre-provisioned VMs is a retained deployment target. The bundle under `infra/self-hosted/` (installer, Compose stack with `postgres`, `redis`, `db-migrate`, `g5-api`, `g5-api-indexer`, `edge`, and the worker service, the `just` operator runner with `setup`, `up`, `logs`, `backup`, `restore`, `upgrade`, `down`, the license and registry-token flow, TLS overlay, starter repositories), the worker-host installer (`install.sh --worker-host`, `g5-workerhost.sh`, which runs the host container with `/dev/kvm` and `G5D_GUEST_HOST_ALIASES` / `G5D_GIT_HTTP_ALLOW_HOSTS`), the Azure canary workflow (a VNet with a private DNS zone so the worker VM reaches the control plane at its private address, a nested-virtualization worker VM, backup and restore, down and up, upgrade), and the bundle contract tests are all on `main` and byte-for-byte the original G5 assets apart from the GitHub-only login change. `SELF_HOSTED=true` and `ENABLE_CLOUD_FEATURES=false` keep the SaaS-only routes off. The general deployment target remains. Search-specific services, worker duties and configuration are removed by the search-removal MVP gate across SaaS/dev/self-hosted manifests; shared document/ontology projection duties remain available. The later self-hosted phase brings the remaining bundle to the workspace-VM model.

### 16.3 Gaps against this design

| Area | State on `main` | Gap | Phase |
|---|---|---|---|
| Worker host on a pre-provisioned VM | `g5-workerhost.sh` starts a per-task worker container with poll and claim | The container must run `g5d host` with the `ctl` tunnel, slots, retained roots and the host proxies; the registration token flow stays; the Compose stack's in-box worker service becomes a `g5d-host` service with `/dev/kvm`; process isolation mode (`G5D_ISOLATION_MODE=process`), which the installation guide allows with an acknowledgement, has no equivalent in the workspace-VM model and needs a decision (drop it, or run the dispatcher as a host process for KVM-less hosts) | P9.1 |
| Internal routing from hosts to the control plane inside the customer VPC | DNS-based: the host resolves the control-plane FQDN through the private zone; `G5D_GUEST_HOST_ALIASES=<fqdn>=<private ip>` seeds the guest; `G5D_GIT_HTTP_ALLOW_HOSTS` | Retained as host configuration: the host seeds the guest's `/etc/hosts` with the aliases beside `g5-llm` and the git proxy rewrites the internal git host; `VmSpec` never carries host addresses; the canary continues to prove the private path | P9.1 |
| Microsoft Entra platform login | Removed with the other non-GitHub logins (P8.3, −2.4K lines); the original adapter (OIDC userinfo with id-token fallback, tenant, default org role map, `G5_MICROSOFT_*` settings) is in the reference clone | Re-add the Entra adapter behind configuration, off in the SaaS and available to self-hosted installations; env template, installer keys, login catalog entry, canary smoke (`entra-login-smoke.js` from the reference clone) | P9.2 |
| Backup and restore | `just backup` dumps Postgres and the configuration; it never copies `/var/lib/g5/storage/repos`, the internal git store; `just restore` restores the dump only; writers keep running during the backup | Files are in git in this design, so a backup without the store is not a backup. `just backup` takes a barrier (the API refuses ref writes, receipts, merges, forks and deletions for the window, receive-pack answers 503 with retry-after, agents retry), dumps Postgres, snapshots the git store (`git bundle --all` per repository or a filesystem snapshot), releases the barrier and records both in the manifest; `just restore` restores both from one backup, refuses mismatched pairs and runs projection convergence; `just upgrade` drains workspaces through the stop sequence before it migrates | P9.3 |
| Upgrades | `just upgrade` scales the worker service to zero before migrating | Drain through the stop sequence so unpushed work is flushed; control-plane-first order for the host protocol | P9.3 |
| Installation guide | Describes per-task workers, task leasing, CRDT state, commands that do not exist (`g5 clone workspace`, `g5 sync`, `g5d worker register\|run\|list`) and an inverted api-url resolution order | Rewrite for the workspace-VM model: host registration, slots and sizing per workspace VM, flush and loss semantics, the backup barrier | P9.1 |
| Azure canary | Manual dispatch; installs the control plane and one microVM worker; never run green on Lean | Update to `g5d host`, add a workspace VM smoke (create, chat with the fake provider, stop with a flush, resume) and the backup-barrier check | P9.4 |

These are scheduled as a later phase (§13) and change nothing in the MVP's design.

### 16.4 Backup consistency

Postgres rows and git objects describe one state only when they were captured under one barrier. Restoring a database snapshot next to a git store captured later can name receipts, branches or merge results whose objects were pruned or rewritten in between; projection convergence cannot recreate missing objects. The barrier above makes both stores quiescent for the window; the alternative of pinning the ref set at the database snapshot and retaining it through the backup is the implementation-time choice if the window is too long for large stores.

---

## 17. Implementation-time choices, deferred items, open questions

### 17.1 Implementation-time choices

- The journal file format, the disposition table's storage and the disk budget on the VM; the event payload shapes for `branch.updated`, `flush.completed`, `file.changed`.
- `quiesce_secs`, the retry and backoff schedules for flush pushes, receipt completion and merge imports, and the default drain deadline.
- Whether several queued inbox messages are coalesced into one turn.
- Native-response deadlines, bounded protocol buffers and response-delivery reconciliation; zstd level and caps for session files. Unrelated MCP tools retain their own ordinary tool timeouts.
- The `configs` shape for feature flags; whether `projection_state` is one table or a column set on each projection.
- Retained-root retention as a fixed value or an org setting (default 24 h); one version number or two for the host protocol and `VmSpec` (default: two).
- Whether the passt door can be mapped to a per-VM host address so the proxy secret becomes defense in depth only.
- Crate consolidation targets; the list of `.g5/` runtime subdirectories in the excludes file.

### 17.2 Deferred items

- **Codex app-server migration (scheduled later phase).** P3.7 moves the adapter to `codex app-server` over stdin/stdout for native questions and approvals, with generated-schema/pin probes, owner-authorized replies, shared session/continuation semantics and transport-loss gates. MVP remains `exec --json` without interactive input or fallback.
- **Session-resident Claude process** behind `agent.turn` (steering, in-band interrupt, live model and permission changes), with the gates of §8.7; the Claude Agent SDK stays unused.
- **Document-only CRDT** for real-time co-editing behind the Save API.
- **Org-level read-only provider grants** for private git dependencies, and the `gh` credential path with a per-VM CA.
- **Secret files and secrets-manager integration** beyond environment variables. Per-user task-secret isolation within one workspace is unnecessary under the single-owner model.
- **Preview write isolation** through a disposable overlay that discards preview writes.
- **Multi-replica API**: moving the tunnel registry and event bus to Redis with a centralized lease.
- **A CI service outside workspaces** (a read-only clone of the workspace answering API calls at a git version).
- **Snapshot of the tree to object storage** as a mitigation for flush failure.
- **Lightweight disposable forks and parent/child navigation.**

### 17.3 Open questions

- Confirm the reading of the ontology decision in §2 (developed without the gate and outside the MVP acceptance, code retained).
- Whether general mid-turn human steering or a session-resident Claude process is useful after the native one-turn control protocol ships; native question transport is already decided, not an open question.
- Agent-initiated workspace creation as a way to attach compute (not offered here).
- Share-link presentation and discoverability within the org read-access model; ownership transfer semantics are settled in §5.8.

---

## 18. Decisions register

Sources: the accepted draft 6 (`v6 §n`, whose own register traces the earlier questions and reviews), the user's revision note (`note n`), the companion notes, the team's aligned decisions (`team`), the v8 [workspace-sharing clarification](workspace-sharing-v8.md) (`sharing`, which overrides the prior multi-writer decision), the explicit preview-task correction (`v8 correction`), the latest user scope revision (search removal, agent-controlled commits, lifecycle-only flush, shared agent sessions and native provider questions), and the design reviews (`Gn` from the draft 6 review, `Rn` from the draft 7 review, `review` for a finding raised in the review round before this draft and carried by its content).

| Decision | Source | Section |
|---|---|---|
| Ontology tooling retained and decoupled; codebase developed without its own grounding gate; ontology outside MVP acceptance (reading to confirm) | v6 §2; team | §2 |
| Targets: GCP SaaS with the QA project as CI/CD, self-hosted pre-provisioned VMs, macOS dev-only; customer images dropped | v6 §2; note 8 | §2, §16 |
| Shared agent model: human typed/workflow templated initial input, kind `agent`, common sessions and explicit permission policies; `command`/`preview` stay separate | latest user scope revision | §7–§8 |
| Remove workspace search end to end; keep chat-history search/editor Find/agent-local code search/docs+ontology projections | latest user scope revision | §9.5, §12 |
| Direct Claude CLI native AskUserQuestion; remove custom ask-user transport, retain unrelated MCP; Codex MVP exec noninteractive, later app-server stdio | latest user scope revision | §8.3–§8.7, §11 |
| Three capability owners plus Git and governance; ownership follows capability | note 4; team | §4.1, §13 |
| Workspace maps to N member branches with one enforced shared name; only fork creates them; exact proposed name checked in every member repo, any collision asks for another name; no ID suffix | user’s workspace/branch clarification | §4.3, §5.1–§5.3, §5.8 |
| No periodic checkpoint or manual flush; agents commit regularly, lifecycle flush only for shutdown/fork; no user Commit action; ungraceful loss accepted | latest user scope revision; workspace-vm-api §5 | §5.4, §9.2 |
| Flush publishes through immutable per-flush refs and an API-verified receipt; consumers read the last complete receipt | review | §5.4, §5.5 |
| Live flushes require quiescence or an explicit interrupt; concurrent terminals and previews are recorded | review | §5.4 |
| PR publication uses existing agent-created/pushed branch commits; no Commit and push all or flush-to-branch shortcut | latest user scope revision | §10.2 |
| Flush failure: user stop and transfer abort, deadline-bound teardown proceeds and records the loss, idle stop cancels | team; note 1 | §5.4, §5.8 |
| Fork flushes the source first and carries every committable artifact including dirty work | team | §5.8 |
| Teardown reasons: user, transfer, idle, drain, rollout, credential, capacity; nudge before deadline-bound teardowns | note 1; team | §5.8 |
| Six workspace actions including transfer and restart; image from `workspace.yaml` resolved per attach | sharing; team | §5.8, §6.3 |
| Base workspaces have no VM and are read-only; running from base is fork and preview | team | §5.2 |
| Exactly one writer, `owner_user_id`; org/invite readers have identical reads, preview start/cancel and fork; admin takes ownership before writing | sharing; supersedes team and v6 Q26 | §5.1, §5.3, §8.1, §10.4 |
| Transfer stops, flushes, releases, atomically changes owner and audits; failure aborts; history/tasks remain, native request IDs end with the old process, new owner explicitly resumes | sharing; latest native-input revision | §5.8, §8.4 |
| The dispatcher exposes the Workspace Runtime API; `proc.*`, `git.*`, `agent.run_turn`, `preview.*`, `checkpoint.*` absent; platform code runs only as declared modules | workspace-vm-api §2 to §7; team | §6.6, §6.7 |
| The Workspace Runtime facade is the only client of VM channels; Agents calls it | workspace-vm-api §1; agent-api-surface §4 | §6.6 |
| Host protocol: versioned `ctl` stream, journaled `vm.event`, idempotent commands with deadlines, `VmSpec`, renewable host credential, `unreachable` state, retained roots, limits, operator API | worker-host-api §2 to §9 | §6.1 to §6.3 |
| Lease: host deadline armed from the request's monotonic start; boot lease covers the command deadline; commands after their deadline refused | review | §6.4 |
| Host-side proxies: model traffic per session with a per-VM secret; git and platform credentials injected on egress; no credential in a provider process; the token only in the dispatcher and the proxy; one owner’s provider account for the incarnation; session tags only for accounting; same-owner grants updated live; transfer rebinds on a new incarnation | worker-host-api §6; agent-api-surface §6.1; team | §6.5 |
| Repository and task secrets as environment variables, re-read per process launch | team | §6.5 |
| Preview is a visible, cancellable task of kind `preview`; any reader starts/cancels; one active, replacement serialized, launcher audit only; workspace secrets, no bounded slot/time limit/auto-resume; tracked-write prohibition is contractual | sharing; v8 correction | §6.8, §7 |
| No maximum host lifetime; drain and replace, negotiated module/component versions; rollout continuation retains the same logical agent task | team; latest unified-task revision | §6.10, §7.4 |
| Agents is a bounded context owning tasks, chats, inbox, questions, session files, adapters, usage; `agent.turn` is its only VM-hosted piece; C1/C2/C3 categories; provider-neutral event and outcome schemas | agent-api-surface §2 to §12 | §7, §8.3 |
| One CLI process per turn for both providers; the Claude Agent SDK is not used because it wraps the same CLI protocol; a resident process per session is a later option behind the same contract with gates | team; v6 B4; agent-api-surface §1, §14 | §8.3, §8.7 |
| Inbox: owner-only queued, editable, withdrawable messages; reservation and delivery states; no steering in the MVP | agent-api-surface §8 | §8.1 |
| Claude native questions/approvals are live execution-bound requests; timeout/exit invalidates them; history remains and explicit Continue uses the same task with a fresh attempt | latest user scope revision | §7.2, §8.4 |
| Loss settles unconfirmed starts; same-identity replay only on the current incarnation | G1 | §7.5 |
| Durable cancellation dispositions retained for the incarnation, independent of journal pruning | G2, review | §7.5 |
| Continuations retain logical task/session/job/dependencies, have new attempt identities and never arise from cancel or a stale native answer; only final success releases dependents | latest user scope revision | §7.2–§7.5 |
| Event keys and watermarks carry the attempt | G4 | §7.5 |
| Local merge applied in one ref transaction with a receipt ref; recovery recognizes application from the receipt | G5 | §10.6 |
| Save ensures the runtime before the association query | review | §9.2 |
| `delivered` follows the adapter's `input.accepted`, not the module's `started` disposition; an execution without acceptance releases the reservation | R1 | §8.1, §8.2 |
| Native response timeout is supervisor arbitration/reaping with one attempt outcome; it cannot leave an answerable request for a dead process | R2 invariant; latest native-input revision | §7.2, §8.4 |
| Live flushes close and drain file-mutation admission and hold it through capture | R3 | §5.4 |
| `vm.stop` is fenced by host ownership only; release follows the host's exit | R4 | §5.8, §6.2, §6.4 |
| A warm root is a cache of ignored files and objects; every attach materializes the receipt | R5 | §5.5 |
| Baseline frozen; forward migrations validated as fresh install and upgrade; no baseline rewrite; databases persist | note 6 | §12 |
| Governance built the week after the MVP; approvals written only through the API; approval state on the PR; external CI postponed | team; v6 §10 | §10, §13 |
| Self-hosted: worker host on `g5d host`, VPC-internal routing by host configuration, Entra login re-added for self-hosted, backup and restore of Postgres and git under one barrier, guide rewrite, canary update; later phase | note 8; review | §16 |
| Chat history in Postgres via the API with no VM; sub-agents untracked; agents do not create workspaces | v6 B8, Q8; team | §8.1, §11.1 |
| Carried from draft 6 except for the v8 owner authorization gate: receive-pack ref policy and owner rewrites, provider integration, association check, projections converge from OIDs, merge operation shape, approvals pinned to source OIDs, session files in Postgres, fake CLI binaries, cancel signals, idle timeout default, approved removals | v6 §18 | throughout |
