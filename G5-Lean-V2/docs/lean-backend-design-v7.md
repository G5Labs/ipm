# G5-Lean backend re-architecture — design (draft 7)

Status: draft 7, 2026-09-16, candidate for acceptance. Supersedes [lean-backend-design-v6.md](lean-backend-design-v6.md) (draft 6, accepted 2026-09-13); draft 6 and the earlier drafts are kept unchanged for history. This draft is self-contained: it states the target system directly and does not require draft 6 to be read alongside it.

Three companion notes are normative for the surfaces they define and are adopted here at the level of the design; they hold the field-level detail that this document summarizes:

- [workspace-vm-api.md](workspace-vm-api.md): the Workspace Runtime API of a workspace VM, the declared-module runtime, the private control-plane protocol and the flush (called `workspace.sync` there).
- [worker-host-api.md](worker-host-api.md): the worker host protocol, `VmSpec`, the host credential and lease, placement, and the host-side proxies.
- [agent-api-surface.md](agent-api-surface.md): the contract cut between Agents, Workspace and Worker & Host, the `agent.turn` module, the provider-neutral event and outcome schemas, the inbox and question states.

Where this document and a note differ, this document says so in place and wins; the notes are then to be amended.

Implementation state at this draft: Phases 0 and 3 of the draft 6 plan and most of Phase 1 are on `main` (git refs authoritative, receive-pack policy, projection convergence, head-operation registry deleted, native worker push, native-git profiles, the Rust provider runner, `task_questions`, `chat_session_files`, `naive_react` removed, IDE mockup and non-GitHub login removed). The QA GCP project, its Cloud Build lanes and the frozen schema baseline with forward migrations are in place. Everything in §5.4 onwards that describes the workspace VM, the host protocol and the Agents service is not implemented yet. The delivery plan is [lean-implementation-plan-v7.md](lean-implementation-plan-v7.md). This draft was reviewed as [lean-backend-design-v7-review.md](lean-backend-design-v7-review.md); its findings R1 to R5 are addressed in place (R1 in §8.1 and §8.2, R2 in §7.2 and §8.4, R3 in §5.4, R4 in §5.8 and §6.2, R5 in §5.5).

---

## 0. What changed since draft 6

1. **Durability is agent-driven.** There is no periodic checkpoint task. Agents commit and push their own work; G5 flushes uncommitted state to internal git only at a graceful teardown of the VM (a user stop, restart or delete, and G5-decided teardowns such as a host drain, a rollout, an expiring host credential, or idle reclamation), at a fork, and on an explicit user request. Work that was neither pushed nor flushed is lost on an ungraceful loss, and that loss is accepted and shown. (§5.4, §5.8)
2. **The flush publishes through a receipt, never a mutable ref.** Each flush pushes each repository's snapshot to an immutable per-flush ref and completes a multi-repository receipt; restore, stopped reads, fork and projections consume the last complete receipt only, so a partial flush is never visible. (§5.4, §5.5)
3. **Live flushes need quiescence.** A flush that does not end the VM (fork, explicit) is an exact cut only when no execution is active; otherwise it waits, or is refused, or interrupts on request. Pull-request publication does not flush: it commits and pushes through the changes module with laptop semantics. (§5.4, §10.2)
4. **The dispatcher exposes the Workspace Runtime API.** Files, terminal, declared modules, ports, status and diagnostics; `proc.*`, `git.*`, `agent.run_turn`, `preview.*` and `checkpoint.*` do not exist. Every platform-run process is a declared module with a versioned contract, limits and a capability grant. (workspace-vm-api.md; §6.6, §6.7)
5. **The host protocol is the worker-host-api note.** A versioned `ctl` stream with hello, heartbeat, journaled VM events, idempotent commands and a `VmSpec`; a renewable host credential; an `unreachable` host state; retained workspace roots; host-side LLM and credential proxies. No credential enters a provider process; the workspace token exists only in the dispatcher and in the host proxy. The host lease can no longer outlive the control plane's grant. (§6.2 to §6.5)
6. **Agents is a bounded context of the control plane.** It owns tasks, chats, the inbox, questions, session files, provider adapters and usage; its only VM-hosted piece is the `agent.turn` module. The provider adapter is the merged direct-CLI runner, one process per turn for both providers; the Claude Agent SDK is not used, because it wraps the same CLI protocol, and a later project may keep the process resident for a session behind the same module contract. (agent-api-surface.md; §7, §8)
7. **Task contract corrections.** Loss of an incarnation settles unconfirmed starts too (G1); cancellation is a durable dispatcher disposition retained for the life of the incarnation, independent of journal pruning (G2); a successor turn is created only after interruption, loss or suspension, never after a user cancel (G3); event keys and acknowledgement watermarks carry the attempt (G4); an applied local merge is recognized through a receipt ref written in the same ref transaction (G5). (§7.5, §10.6)
8. **Questions have two deadlines.** The runner wait bounds how long a process blocks on a question; the answer expiry bounds the question. A turn whose runner wait ends is `suspended`, the VM may stop, and the answer wakes the workspace into a successor turn. (§8.4)
9. **Save resumes before it checks the branch.** A Save on a stopped workspace resumes it first and asks the running incarnation for the association. (§9.2)
10. **Workspace model.** Actions are create, stop, resume, restart, delete. Base workspaces have no VM and are read-only. One preview per workspace. Any member with write access may edit, chat and use the terminal. Chat messages typed while a turn runs are queued, editable and withdrawable. (§5.8, §6.8, §8.1)
11. **Schema by forward migration.** The baseline is frozen and every change is a numbered migration validated as a fresh install and as an upgrade; no baseline rewrite; dev and QA databases persist across deploys. (§12)
12. **Deployment.** The QA GCP project and its Cloud Build lanes are the CI/CD; self-hosted installation on pre-provisioned VMs, Microsoft Entra login, VPC-internal routing from hosts to the control plane, and backup and restore are retained requirements. The gaps against this design are listed and scheduled as a later phase; backup must cover Postgres and the git store under one barrier. (§16)
13. **Delivery by capability.** Three owners (Workspace, Agents, Worker & Host) plus a Git and governance track; an MVP milestone; governance, self-hosted, the pre-launch audits and the resident-process provider option after it. (§13)

Draft 6's size accounting (§16 there) is not carried; the before-metrics were recorded by P0.1 (PR #1).

---

## 1. Summary

G5 today runs one microVM per task attempt, keeps uncommitted edits in a server-hosted CRDT, kept branch heads in Postgres until Phase 1 made git refs authoritative, keeps chat VMs alive between turns through a sticky-runtime protocol, and layers approvals, merge runs and remote-tracking sync on top. The target is a developer-laptop model: one VM per workspace, ordinary files on the workspace branch, native git, agents that commit and push their own work, and a control plane that only steps in at the boundaries. The VM is not as persistent as a laptop: it is terminated when idle and re-provisioned on request, and when G5 terminates it gracefully it first flushes uncommitted state to internal git so the next VM can pick up where the last one left off.

| # | Proposal | Section | What it replaces |
|---|---|---|---|
| 1 | One VM per workspace; tasks are processes launched by a dispatcher through declared modules | §6, §7 | per-task VM boot, materializer, host poll/claim/ack, golden images |
| 2 | Remove the CRDT layer | §9 | `g5-crdt`, both CRDT peers, drafts, task-commit protocol, yjs in the UI |
| 3 | Internal git as persistence: agents push; G5 flushes at graceful teardown; restore from a receipt | §5.4, §5.5 | staging refs, Postgres-owned heads, the draft 6 checkpoint cron |
| 4 | Native git for agents; a small platform surface | §11 | ~20K lines of CRDT-bound CLI (already deleted), internal tools |
| 5 | Lightweight workspace↔branch check | §5.7 | git blocklists, rebase pause files, authority-scope typing |
| 6 | Governance at the git boundary | §10 | 11-table approval ledger, job-merge runs, PR provider lifecycle |
| 7 | Explicit-Save editing, readable while the VM sleeps | §9 | draft sessions, presence, CRDT-anchored comments |
| 8 | Shared chats, no forced fork, no process between turns, queued messages | §8 | sticky runtimes, worker pins, SDK sidecar, context links |
| 9 | Preview on the workspace VM, one per workspace | §6.8 | preview job type, sessions table, reaper, chat-attach machinery |
| new | Claude Code and Codex only; one CLI process per turn; a resident process per session as a later option | §8.3, §8.7 | the SDK sidecar (deleted), `naive_react` (deleted) |
| new | Declared modules, host protocol and proxies, Agents as a bounded context | §6, §7, §8 | ad-hoc RPC surfaces, per-task credentials in the VM, host-side task logic |

The estimated effect is unchanged from draft 6: on the order of 250K to 300K of the ~1.1M lines of Rust deleted or replaced by much smaller code, 118 tables reduced to about 36 through drop migrations, and one execution model instead of three.

---

## 2. Goals, constraints, non-goals

**Goals.** Simplest possible backend; easy to keep stable; the nine proposals; laptop-like developer experience with a first-class workspace that maps 1-1 to a git branch; UI like GitHub Codespaces; a dispatcher core that is small, well tested and never edited for features once it ships, with features as modules outside it.

**Constraints.**
- Incremental: the existing source is modified and the frontend components reused; every PR leaves a buildable, testable product; vestigial references to the old architectures are removed as they are met.
- One repository for the MVP; the split into separately deployed services is not attempted now.
- Schema: the baseline is frozen; every change is a forward migration; no data compatibility with the original G5 platform is required, but dev and QA databases are not disposable (§12).
- Providers: Claude Code and Codex, pinned CLIs (`scripts/claude-code-cli-version`, `scripts/codex-cli-version`), one process per turn. No long-lived agent process between turns in the MVP. The Claude Agent SDK is not used: the runner drives the CLI's own stdio protocol, which is what the SDK wraps; the SDK's published type definitions may serve as a schema source (§8.7).
- Platform login: GitHub in the SaaS; Microsoft Entra for self-hosted installations returns in the self-hosted phase (§16). Repository providers: GitHub and GitLab, and local-only repos.
- Deployment targets: the GCP SaaS (GKE control plane, GCE worker hosts, the QA project as CI/CD), self-hosted Linux hosts on pre-provisioned VMs with the Azure canary lane, macOS hosts for development only. Customer-specific worker images are dropped.
- Ontology and grounding: the G5-Lean codebase is developed without its own ontology and grounding gate; the ontology product surface (uplift, downlift, sync, grounding tools, ontology projections) is retained in the code and decoupled from git and CRDT as in draft 6, but it is not part of the MVP acceptance and is revisited afterwards. This is the reading this draft takes of the team's decision to remove the ontology during MVP development; it needs confirmation.
- External CI integrations are postponed; agents run tests in the workspace VM with local compute.
- Credentials: environment variables remain the mechanism for repository and task secrets in the MVP (§6.5); provider, git and platform credentials never enter a provider process (§6.5).

**Non-goals for this iteration.** Real-time co-editing or presence (a document-only CRDT can come later behind the Save API); hardening beyond today's microVM boundary and the credential rules in §6.5; new cloud providers; tracking of provider subagents (the provider process is the unit); interactive Codex tool-permission approvals (§17.2); live steering of a running turn (a resident-process capability, §8.7); agent-initiated workspace creation (§17.3); multiple API replicas (§17.2); a CI service outside workspaces.

---

## 3. Starting point

### 3.1 What is on `main`

| Area | State on `main` (2026-09-16) |
|---|---|
| Git boundary | Reads resolve heads from `refs/heads/<branch>`; receive-pack enforces the policy table for protected refs and workspace branches; `projection_state` and the convergence loop; the head-operation registry, staging refs and finalizer are deleted. Remote tracking still exists (P1.5). |
| Worker | Runs the provider directly, commits with `git add -A` semantics and pushes `HEAD` to the assigned branch with its token; the CRDT peer and the CRDT-bound CLI are gone from the worker; profiles describe native git. The backend CRDT layer and the frontend editors' CRDT binding still exist (P2.1, P2.2, P2.4). |
| Provider runtime | One Rust runner per turn for both CLIs (`backend/g5d-worker/src/runner/`), both Node sidecars deleted, fake `claude` and `codex` binaries, `task_questions` with `ask_user` and `request_permission` by answer polling, `chat_session_files` harvest and restore, provider enum `claude_code | codex`. |
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
| Preview as a job (~6K; relay kept) | Run in the workspace VM (§6.8) |
| Fleet bring-up specific to task pools, 24 h host JWT | Simplify to slots on hosts with a renewable credential (§6.2) |
| Internal issue tracking (~10K, 13 tables) | Remove (approved) |
| Chat context links | Remove |

The approved deployment-gate removals of draft 6 §3.2 stand as landed (issue tracking pending as P8.1). One of them is partly reversed later: Microsoft Entra platform login returns for self-hosted installations in the self-hosted phase (§16.3).

---

## 4. Target architecture

### 4.1 Components and owners

Ownership follows the capability, not the process a component runs in. Four capabilities own the system; three have named owners in the plan, the fourth is assigned by the team.

| Capability | Owns | Runs in |
|---|---|---|
| **Workspace** | Workspace registry and members, `workspace_vms` and incarnations, the five actions and the stop sequence, the flush and restore, the Workspace Runtime facade, the dispatcher (`g5d dispatch`) and the module runtime, the built-in `workspace.*` modules (changes, preview, bootstrap, diagnostics), the workspace shell UI (files, Save, Changes, terminal, ports, preview) | g5-api, the VM |
| **Agents** | Tasks, attempts and jobs, chats and the inbox, questions and permissions, provider accounts and per-session credential grants, session files, usage, the provider adapters and the `agent.turn` module, the chat and task UI | g5-api, the VM (module only) |
| **Worker & Host** | `g5d host`, the `ctl` protocol on both ends, `worker_hosts` and host states, placement and quotas, `VmSpec` and images, the host proxies, drain and rotation, the operator API, the runtime image and the pinned provider binaries in it | the host, g5-api |
| **Git and governance** | The internal git service and `RepoStorage`, receive-pack policy, the flush-ref namespace rules, protected-branch mirroring and publish, pull requests, approval rules and decisions, the merge operation, the projections' convergence | g5-api |

The control plane is one binary (`g5-api`) with these four bounded contexts as crates or modules with internal interfaces; a split into services is not a goal now. The frontend follows the same ownership per panel.

**g5-api (control plane).** Postgres-backed. Owns identity and orgs, the four capabilities above, the internal git server, the tunnel registry, the browser WebSocket, and the preview and terminal relays.

**Internal git store.** One bare repo per connected repository under `RepoStorage` (local disk for self-hosted, Filestore for the SaaS). Holds mirrored protected branches, every workspace branch, and the immutable flush refs. Git refs are authoritative for files.

**Worker host (`g5d host`).** A Linux machine with KVM (GCE VM, a self-hosted VM with nested virtualization, or a macOS dev host with HVF). Registers, dials one outbound tunnel, boots and stops equal-size workspace VMs on command, relays streams, runs the LLM and credential proxies, and enforces the per-VM lease. No task logic, no knowledge of branches or approvals.

**Workspace VM.** One libkrun microVM per non-base workspace, attached on create or resume and terminated by the stop sequence or by loss. Each attach is a new incarnation (`vm_id`, `epoch`). Its disk is a cache. It runs the **dispatcher** (`g5d dispatch`): restores the workspace from the last flush receipt, serves the Workspace Runtime API, runs declared modules, journals their events, and performs the flush when asked. Chat turns, agent tasks, the preview and terminals are processes in this VM, in one folder, on the workspace branch.

**Frontend.** The React shell with a Codespaces-like center: file tree and editors with explicit Save, a Changes panel, a terminal, Ports and Preview, chat with a queued-message strip, tasks, PRs and approvals.

**Providers.** The pinned `claude` and `codex` CLIs on the VM's runtime mount, launched by the `agent.turn` module, one process per turn. Model traffic goes through the host proxy at a per-session URL; no provider key or platform token is in the VM.

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
2. **One workspace, one branch, one folder, one VM incarnation, one dispatcher.** The `workspace_vms` row with its lease, `vm_id` and `epoch` is the only custody record; every boundary rejects an incarnation that is not current (§6.4).
3. **One write path for files.** All file writes happen inside the VM: agents natively, the UI through `fs.write`. The API never writes a working tree.
4. **Branch protection lives in receive-pack.** Protected refs accept updates only from the API's merge operation after approval checks; flush refs accept writes only from the workspace's current incarnation and deletions only from the API.
5. **A turn is a process; a task is a process.** State derives from the process and its exit; task completion is exit code zero plus the outcome the adapter derives. No leases, claims or per-task heartbeats. Execution identity is `(task_id, attempt, vm_id)`; starts are confirmed before they count as executions (§7.5).
6. **Platform code runs only as declared modules.** Nothing in the control plane can spawn an arbitrary process in a VM; a feature that needs to run workspace code declares a module with a contract, limits and requested capabilities (§6.7).
7. **Durability boundaries are the flush and the push.** Pushed commits are durable. Everything else survives only through a complete flush receipt, and a receipt is complete only when every repository's snapshot is retained by an immutable ref that the API has verified (§5.4).
8. **Stop is a barrier.** A VM is released only after the flush receipt and the session-file harvest are durable, or after a deadline-bound teardown has recorded that the flush failed; the draining incarnation stays current until release (§5.8).
9. **No secret in a provider process.** Provider keys, git tokens and the workspace token live in host memory and are attached by the host proxies for the current incarnation only; the dispatcher alone holds the workspace token (§6.5).
10. **Projections converge from git.** Events are hints; every projection can be rebuilt from authoritative OIDs without an event (§9.4).
11. **Nothing crosses the VM boundary at-most-once.** Starts, answers, events and outcomes are persisted on one side and confirmed by the other; duplicates are detected by key, never by ordering (§7.5).
12. **Versions are negotiated, never assumed.** Host, VM image and control plane deploy independently; the protocol version is negotiated per tunnel, the spec version per boot, the module catalog version per incarnation; the control plane deploys first and hosts may be at most one version stale (§6.10).

---

## 5. Workspace and Git

### 5.1 Data model

```
repos(id, org_id, provider NULL|github|gitlab, provider_full_name, git_url,
      default_branch, protected_branches text[], created_at)

workspaces(id, org_id, name, slug, branch_name, base_branch, is_base,
           visibility org|invite_only, created_by, created_at, deleted_at)

workspace_members(workspace_id, user_id, role write|read)

workspace_repos(workspace_id, repo_id, root_path, role code|docs,
                head_sha_cache, PRIMARY KEY (workspace_id, repo_id))

workspace_vms(workspace_id PK, host_id NULL, vm_id NULL, epoch bigint,
              status stopped|starting|running|stopping|failed,
              image_ref, image_digest, module_catalog_version,
              lease_expires_at, started_at, last_activity_at,
              stop_reason NULL, stop_deadline_at NULL,
              last_flush_id NULL, unflushed_loss_at NULL, last_error)

workspace_flushes(flush_id PK, workspace_id, vm_id, epoch,
                  reason shutdown|fork|explicit, materialization snapshot,
                  repos jsonb,        -- [{repo_id, branch_oid, local_head_oid, snapshot_oid, dirty}]
                  concurrent_writers jsonb,   -- terminals and previews open during a live flush
                  started_at, complete_at NULL, error NULL)
```

`workspace_repos` carries no authoritative head; `head_sha_cache` is a projection (§9.4). A workspace spans N repos (code repos plus the org docs repo) with the same branch name in each; pushes are per repo, like a laptop with several clones. There are no job or task branches and no workspace status column: a workspace exists or is deleted, and its VM state lives in `workspace_vms`. `vm_id` is the current incarnation and `epoch` counts attaches; both change on every attach and `vm_id` is cleared at release. `last_flush_id` names the receipt the next attach restores.

### 5.2 Branch naming and identities

- Workspace branch: `<slug>-<8 hex of workspace id>` in every member repo.
- Base workspace: `branch_name` is the repo's default branch, which is in `protected_branches`. A base workspace has no VM: it is browsed read-only from the committed tree, and chat, preview and terminal on it are offered as "fork and …". Nothing can push to it.
- Identities seen by the git service: **user** (workspace member with write, JWT as today); **workspace incarnation** (the workspace token, a JWT with claims `workspace_id`, `vm_id`, `epoch`, minted at each attach, renewed through `lease.renew` while the incarnation is current, held by the dispatcher and by the host credential proxy, never by a provider process); **API server identity** (merges, forks, flush-ref pruning, in-process).

### 5.3 Receive-pack policy (the git boundary)

`routes/git.rs` keeps its shape: parse ref updates, decide in Rust, spawn `git receive-pack --stateless-rpc`.

| Ref | user (member, write) | workspace incarnation (current) | API server identity |
|---|---|---|---|
| `refs/heads/<protected>` | deny | deny | merge operation only (§10.6) |
| `refs/heads/<workspace branch>` | allow FF; allow rewrites (below) | same | fork creation only |
| `refs/heads/<other workspace's branch>` | deny | deny | n/a |
| `refs/g5/flush/<workspace id>/<flush id>` | deny | create only (no update, no delete) | delete (pruning) |
| `refs/g5/merges/<merge op id>` | deny | deny | merge operation only |
| delete of a workspace branch | deny | deny | workspace deletion |
| tags, other namespaces | deny | deny | n/a |

A token whose `vm_id` is not the workspace's current incarnation is rejected before any ref is considered; a draining incarnation (`stopping`) is still current (§6.4).

**Rewrites of the workspace branch.** Agents rebase onto `main`, so the workspace branch accepts non-fast-forward updates from the workspace's own identities. The server enforces exactly git's update-command check: the `old_sha` the client sends must equal the ref's current value, and a zero `old_sha` is refused on an existing ref. That rejects a ref that moved during push negotiation and makes `--force-with-lease=<branch>:<expected>` work as designed; it does not prove that the pusher incorporated the previous head. Lease safety is a client obligation: the profiles require `--force-with-lease` and forbid bare `--force`. In the shared-folder model every agent in a VM works in one clone, so a rewrite that discards an unseen commit can only happen between the VM and a user pushing from a laptop, which is the laptop semantics asked for. The platform never moves the agent's remote-tracking refs behind its back: the flush observes the internal head with `git ls-remote` and never fetches.

`git-upload-pack` requires read access as today. After a successful push the handler publishes `branch.updated {repo_id, ref, old, new, actor}` as a hint; projections converge from git regardless (§9.4).

### 5.4 Durability: agents push, G5 flushes

There are two ways work becomes durable.

**Agents and users push.** The profiles tell agents to commit often and to push to `origin` when a unit of work is done; the Commit and Push buttons do the same for users through the `workspace.changes` module. Pushed commits are never lost, whatever happens to the VM. This is the normal path and the only one that runs while the workspace works. Losing work when a VM is lost between pushes is accepted.

**G5 flushes at boundaries.** `workspace.flush({reason, interrupt?})` is a privileged lifecycle operation of the dispatcher's private protocol, called by the Workspace lifecycle service only. It captures the exact state of every member repository into internal git and completes a receipt. It runs:

| Reason | When | Ends the VM |
|---|---|---|
| `shutdown` | every graceful teardown (§5.8): user stop, restart, delete, idle reclamation, host drain, rollout, expiring host credential, operator drain | yes |
| `fork` | a fork of a running workspace (§5.8) | no |
| `explicit` | the user's "Flush now" (a safety action, not a habit; also used before a planned host maintenance by an operator) | no |

Only `snapshot` materialization exists: the receipt records commits and a snapshot; nothing is committed onto the workspace branch by a flush. Publishing dirty work onto the branch is a user or agent commit (§10.2), never a flush.

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

**Quiescence.** A `shutdown` flush runs after the stop sequence has interrupted every module execution and closed previews and terminals (§5.8), so the snapshot is an exact cut of a quiet tree. A `fork` or `explicit` flush on a running VM does not end the VM, so the dispatcher first closes admission of new module invocations and of file mutations (`fs.write`, `fs.delete`, `fs.rename`, through the same admission barrier the stop sequence uses), waits for the mutations already admitted to finish, and waits for the active bounded-run executions (agent turns, bootstrap) to end, up to `quiesce_secs` (default 30). If any is still active, the flush is refused with `WORKSPACE_BUSY {active_exec_ids}` unless the caller passed `interrupt: true`, in which case those executions are interrupted like a stop. The barrier stays closed through the capture of every member repository. Open terminals and a running preview are not interrupted by a live flush; they are recorded in the receipt's `concurrent_writers`, and a consumer that needs an exact cut (a fork) shows them and offers "stop them and fork". Both admissions reopen when the flush completes, is refused or fails.

**Failure policy.** A flush that fails (a push refused, the API unreachable, the verification failed) is retried with backoff. What happens next depends on the caller:

- A user-initiated stop, restart, delete or fork **aborts**: the VM keeps running, admission reopens, `last_error` records the cause and the user is notified. Nothing is lost because nothing was torn down.
- A G5-decided teardown with a deadline (drain, rollout, credential expiry) retries until `stop_deadline_at`, then **proceeds without a receipt**: the incarnation is released, `unflushed_loss_at` and `last_error` record the loss, a notification names the workspace, and the next attach restores the previous complete receipt. The loss is visible, never silent.
- Idle reclamation has no deadline: a failed flush cancels the idle stop and the workspace stays up; the idle timer restarts.

**Retention.** Flush refs are immutable. The API prunes, per workspace, every flush ref older than the two newest complete receipts, and every ref of an incomplete flush 24 hours after its `started_at`; a workspace deletion prunes them all. Retained receipts keep their rows.

**Guarantee.** Work pushed by an agent or a user is never lost. Work committed locally but not pushed, and uncommitted edits, survive every graceful teardown whose flush completed, and survive a fork. They are lost on an ungraceful loss (host loss, lease expiry, VMM crash, OOM, operator force stop) and on a deadline-bound teardown whose flush failed, back to the last complete receipt if one exists and otherwise to the branch head. Provider session files are not on the VM's critical path: they are in Postgres (§8.5).

### 5.5 Restore on attach

Every attach restores from the workspace's last complete receipt, on a warm retained root or a cold one:

1. Clone or reflink each member repo from the internal store (the host's bare cache is an optimization) and fetch the workspace branch.
2. If `last_flush_id` is null, check out the workspace branch at the internal head; done.
3. Otherwise fetch `refs/g5/flush/<workspace id>/<flush id>` for each repo and set the local branch to the receipt's `local_head_oid`: `git checkout -B <branch> <local_head_oid>` with upstream `origin/<branch>`. This reproduces committed-but-unpushed history exactly.
4. If `snapshot_oid != local_head_oid`, materialize the snapshot tree into the working tree without moving `HEAD` (`git restore --source=<snapshot> --worktree -- .` plus untracked files present in the snapshot and absent from `HEAD`), leaving changes unstaged.
5. If the internal head moved while the VM was down (a user pushed from a laptop), the local branch is now behind or diverged from `origin/<branch>`. Nothing is merged automatically; the status strip shows ahead/behind and "Update from origin" runs `workspace.changes update`, or the agent rebases natively.

A warm retained root is a cache of ignored files and git objects, never of tracked content: the attach runs the same steps on it (steps 3 and 4 are idempotent) and removes untracked, non-ignored paths that the snapshot does not contain, so the working tree equals the receipt whether the root was warm or cold. `HEAD`, `git status` or a list of dirty paths are never used as proof that a tree equals the receipt; only the materialization is. A retained root of an incarnation that was lost ungracefully may hold work newer than the receipt; that work is discarded by the same materialization, because the receipt is the only state the platform can vouch for.

### 5.6 Provider integration (GitHub / GitLab)

- **Inbound.** For each connected repo the API fetches the protected branches from the provider into the mirror on webhook and on a periodic poll (default 5 minutes). Inbound updates to a repository's protected refs are serialized with merge operations on that repository (§10.6). Workspace branches are never fetched from the provider.
- **Outbound.** The API pushes a workspace branch to the provider only on explicit actions: PR publish (§10.2), "Publish branch", and the republish step of a merge operation (§10.6), using the acting user's stored token and `--force-with-lease`, same branch name.
- **Updating a workspace from upstream** is a git operation inside the VM: `git fetch origin main && git rebase origin/main` (origin is the internal mirror through the credential proxy). The UI's "Update from origin" runs it as `workspace.changes update`; agents do it natively.
- **Connecting a local repo to a provider** updates `repos.provider/git_url`, performs the first publish, and enables inbound mirroring of the protected branches.
- **Local-only repos** keep working: a bare repo in the store with no sync.
- **Provider credentials never enter the VM.** Private git dependencies on GitHub or GitLab are reached through the credential proxy with an org-level read-only grant (§6.5) or are unsupported.

Removed: `tracking_mode`, `push_mode`, `auto_pull_enabled`, `base_auto_pull`, sync runs, pair leases, advisory pinger, remote-tracking merge runs, the branch-integration worker and scheduler.

### 5.7 The workspace↔branch association check

`workspace.changes get` reports, per member repo, the symbolic `HEAD`, whether it equals `workspaces.branch_name`, the dirty set, and ahead/behind against `origin/<branch>`. The API calls it before every UI-triggered mutation of a running workspace (Save, Commit, Push, Update, preview start, PR create). A mismatch returns:

```
409 WORKSPACE_BRANCH_MISMATCH
{ repo: "backend", expected: "fix-login-3fa9c2d1", actual: "main", detached: false }
```

The UI shows the mismatch in the status strip with one action, "Switch back" (`workspace.changes switch_back`: `git switch <branch>` if clean, otherwise stash or commit first). Nothing prevents an agent or a terminal user from running `git checkout`; the profile text tells agents to stay on the branch and the check catches drift at the next UI operation. Several chats on one branch reconcile their work themselves (worktrees, merge back), not through platform mechanics.

### 5.8 Workspace actions, teardown reasons and the stop sequence

A workspace has five user-visible actions. There is no archive.

| Action | Steps | Result |
|---|---|---|
| **create** | fork the source branch head into `<slug>-<8hex>` in every member repo (server identity), or the source workspace's last complete receipt when forking a workspace (below); insert the rows; optionally carry chats (§8.6); attach a VM (`workspace_vms` → `starting`, new `vm_id`, `epoch + 1`); the dispatcher restores | running workspace on a fresh branch |
| **stop** | the stop sequence with reason `user`; then release | branch, receipt, chats, session files and settings remain; reads come from the receipt or git (§9.1) |
| **resume** | attach a VM on a host with a free slot (placement §6.10), restore from `last_flush_id` (§5.5); session files restore lazily on the first chat turn (§8.5) | running workspace with the last flushed state |
| **restart** | the stop sequence with reason `restart`, then resume immediately, with the image re-resolved from `.g5/workspace.yaml` (§6.3) | the same workspace on a fresh VM; also the repair for a broken VM and the way to pick up an image change |
| **delete** | stop if running (force variant permitted, which records the loss); delete the workspace branch, the flush refs and the receipts in every repo; delete chats, session files, tasks and rows | gone |

**Forking a workspace** carries every committable artifact of the source, dirty edits and unpushed commits included, but not local file-system state such as build outputs. A running source is flushed first with reason `fork` (quiescence rule of §5.4); a stopped source is forked from its last complete receipt; a clean, idle source yields a receipt with no dirty entries. The fork's branch is created at each repo's `local_head_oid`, the source receipt's flush refs are copied under the fork's workspace id, and the copy becomes the fork's `last_flush_id`, so the fork's first attach restores the dirty tree.

**Teardown reasons.** A graceful teardown is any teardown that runs the stop sequence; it is the only way the platform ends a VM on purpose.

| Reason | Decided by | Deadline | Flush failure |
|---|---|---|---|
| `user` (stop, restart, delete) | the user | none | abort, the VM stays up |
| `idle` | the idle reconciler (org default 30 minutes, workspace override; activity = running executions, an open terminal, preview traffic, file RPC, an open question within its runner wait; not an open browser tab) | none | cancel the idle stop, restart the timer |
| `drain` | an operator or the rotation loop (`host.drain`) | the drain deadline (default 30 minutes after the nudge) | proceed at the deadline, record the loss |
| `rollout` | the deployment procedure (§6.10): a new VM image or dispatcher version | as `drain` | as `drain` |
| `credential` | the host whose credential cannot be renewed (renewal refused because the host is draining, terminated, or its runtime identity changed), with a margin before expiry | the credential's expiry minus a margin | as `drain` |
| `capacity` | an operator action to free a slot (never automatic) | as `drain` | as `drain` |

Every deadline-bound teardown starts with a **nudge**: a notification to the workspace's members and a notice event appended to every running chat session that is delivered at the next turn boundary (a running CLI turn cannot receive a mid-turn message in the MVP), asking to commit and push. Turns that are still running at the deadline are interrupted and continue after the resume (§7.2).

An ungraceful loss is everything else: host loss, lease expiry, a VMM exit, OOM, an operator force stop. It runs no flush and loses unpushed work back to the last receipt.

**The stop sequence.** Throughout it the incarnation remains current: its `vm_id` is unchanged, its lease keeps renewing, and every boundary accepts it. What closes is admission of new work, not the incarnation's authority.

1. **Decide.** `workspace_vms.status` → `stopping`, `stop_reason` and `stop_deadline_at` recorded in the same update.
2. **Close admission.** The Workspace Runtime facade refuses new Save, module invocations, preview, terminal and chat-turn requests for the workspace with `WORKSPACE_STOPPING`, and the UI offers "Resume" after the stop. Agents closes its own admission for the workspace (`agents.workspace_stops.prepare`): no new inbox reservations, queued inbox rows stay queued and editable. File mutations already admitted complete; the dispatcher's admission barrier returns only when none is in flight.
3. **Quiesce.** Active module executions are interrupted (SIGINT, SIGKILL after 15 s). Agent turns end with outcome `interrupted` (§7.2); open questions stay answerable and become suspended (§8.4); tasks with `retry_on_vm_loss` are re-queued; every outcome and remaining event is reported and acknowledged through the normal contract (§7.5). The preview and the terminals are closed. Idle stop has nothing to interrupt by definition.
4. **Flush** with reason `shutdown` (§5.4) and wait for the complete receipt, or for the deadline.
5. **Harvest.** Session files changed since the last harvest are uploaded (§8.5) and acknowledged; the event journal is drained and acknowledged; Agents commits its stop receipt (`agents.workspace_stops.commit`), which proves that no Agents state remains only in the VM.
6. **Stop the VM.** The lifecycle service sends `vm.stop graceful` to the host and waits for the host's `exited` event or the `stopped(exit)` result. `vm.stop` is idempotent and fenced by host ownership of the `vm_id`, not by currency (§6.2), so a retry can never be refused as stale. The row stays `stopping` and the incarnation stays current meanwhile, so a resume request waits instead of racing the exit; a lifecycle service that restarts finds the `stopping` row and re-sends the same command, and the reconciler does the same for any `stopping` row whose VM a host still lists.
7. **Release.** The row becomes `stopped` with `vm_id` cleared and `last_flush_id` set (or `unflushed_loss_at` set) in one update; from this point the old incarnation is stale and every boundary rejects it (§6.4). The host freed the slot at exit and retains the workspace root (§6.3).

Any wake trigger performs resume: a chat message, a task run, a Save, a preview or terminal open, an explicit Resume, an answer to a suspended question. Reads never resume the VM. Host drain performs the stop sequence for every VM on the host. Capacity: when no slot is free the resume is `queued_for_capacity`, the position is shown, and the dispatch loop retries; nobody's slot is taken to make room.

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

The host is a slot provider. Retained from today: registration and heartbeat, libkrun boot (`vm-exec`), OCI rootfs templates and COW clone, the XFS reflink bare-repo cache, port allocation, passt networking, the outbound tunnel and the credit-flow stream multiplexer, boot-time attestation, and `G5D_HOST_MAX_MICROVMS` as the number of equal-size VM slots. Removed: poll/claim/ack/nack/observation, the per-task `Materializer`, golden lanes, parked runtimes, the discard fence, sticky heartbeat riders, per-task attestation, zero-slot polling, the separate preview registry, the in-guest LLM proxy, the fixed 24-hour host JWT.

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
  incarnation:   { workspace_id, vm_id, epoch, org_id, lease_ttl_secs, idle_timeout_secs },
  image:         { ref, digest },                       -- OCI, in the host cache or fetchable
  size:          { cpus, mem_mib, scratch_disk_mib },   -- must equal the host's slot size
  workspace_root:{ retain_hours },                      -- host path is host-chosen: <workspaces_root>/<workspace_id>
  repos:         [{ repo_id, root_path, git_url, branch }],   -- written to /work/.g5/layout.json for the dispatcher
  restore:       { flush_id, repos: [{ repo_id, local_head_oid, snapshot_oid }] } | null,
  env:           { G5D_WORKSPACE_ID, G5D_VM_ID, G5D_EPOCH, G5D_WORKSPACE_TOKEN, G5D_API_URL, HOME, PATH, provider paths, G5D_LLM_PROXY_URL },
  secrets_env:   [{ name, secret_ref }],                -- repository and task secrets, resolved by the host into the dispatcher's environment (§6.5)
  network:       passt,
  entrypoint:    ["/opt/g5/bin/g5d", "dispatch"],
  credential_grants: [                                   -- consumed by the host proxies, never mounted
    { kind: llm_provider, provider: anthropic | openai | gateway, secret_ref, session_id? },
    { kind: git_https,    target_host, source: workspace_token | secret_ref },
    { kind: g5_api,       source: workspace_token },
    { kind: gh,           target_host, secret_ref }      -- later phase
  ]
}
```

The host validates before boot, in order, and the first failure is the answer: known `spec_version`; `org_id` equals the host credential's org; UUIDs and relative `root_path` without `..` or symlinks; slot size, a free slot, no other VM of this workspace on the host, boot capacity; `image.digest` matches the cached or pulled image; `env` contains only allowlisted keys and no credential-like value; grants and `secrets_env` resolve (values stay in host memory; `secrets_env` is passed to the dispatcher process only); the workspace token's claims match the incarnation.

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
| Model requests (Claude Code, Codex, any OpenAI-compatible client) | `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` = `http://g5-llm:<port>/s/<session_id>[/v1]`, set by `agent.turn` per session (a one-shot task's session id is its task id); a placeholder key so the CLI agrees to run | strips the session prefix, selects the `llm_provider` grant for that session (a session-scoped grant when present, else the workspace-wide one), attaches the credential, re-originates over HTTPS to the provider or the org's gateway, refuses a non-current `vm_id` (`403 stale_incarnation`), an untagged request (`400 missing_session`) and a missing grant (`403 no_grant`), meters usage and captures quota headers per session, and serves `GET http://g5-llm/_g5/status` (per-session `requests_open` and usage; never credentials or bodies) inside the VM and the same view on `vm.status` |
| git over HTTPS (the internal git service; GitHub or GitLab for private dependencies when granted) | a dispatcher-managed global gitconfig rewrites `https://<host>/` to `http://g5-llm:<port>/git/<host>/` for every granted host | recovers the real host from the path, re-originates over HTTPS and injects the `git_https` credential; the internal service's grant is the workspace token itself (`source: workspace_token`) |
| Platform API calls from processes in the VM (`g5d` commands, the `g5-tools` MCP server: questions, permissions, PR creation) | `G5D_API_URL=http://g5-llm:<port>/api` in the launch environment; `G5D_EXEC_ID` and `G5D_SESSION_ID` are plain values, not secrets | injects the workspace token (`g5_api` grant) and forwards; the API still applies the incarnation fence and the execution identity carried in the request |
| `gh` and other clients with a hard-coded API host (later phase, §16) | `HTTPS_PROXY` plus a per-VM CA in the trust store, a placeholder `GH_TOKEN` | terminates TLS under the per-VM CA, injects the `gh` credential, re-originates |

The dispatcher keeps the workspace token in its own environment for its calls to the control plane (`runtime.ready`, `lease.renew`, receipts, harvests) and for its own git operations such as the flush push, and does not pass it to the processes it launches; provider processes hold placeholders only. A real `Authorization` header a client set itself is forwarded untouched.

**Grants are references, and they change.** `credential_grants` name secrets by reference; the host resolves them through the control plane at boot and keeps values in memory. The control plane pushes a full replacement list with `vm.grants.update` when a user changes a key, an org rotates a token, or a chat session of another user starts in the same workspace (multi-writer workspaces: each session uses its own user's provider account, so a `llm_provider` grant may be scoped to a `session_id`; a session without a grant gets `403 no_grant`, and one user's key is never used for another user's session). The proxy re-resolves a grant once on an upstream `401` before surfacing the error.

**Repository and task secrets** (`repo_secrets` and org secrets that builds and tests need) are environment variables in the MVP, as today: the host resolves `secrets_env` into the dispatcher's environment at boot, and the dispatcher passes them to every module execution it launches. Because every turn spawns a fresh process, a changed secret takes effect at the next turn without a restart (the dispatcher re-reads the current set from the control plane before each launch); a resident terminal shell keeps the values it started with. These secrets are workspace-scoped, so two writers of one workspace see the same set; per-user isolation of task secrets, secret files and secrets-manager integration are later items (§17.2).

### 6.6 Dispatcher and the Workspace Runtime API

Boot: mount `/work`, create `/home/agent` and `/var/lib/g5d`, restore (§5.5), start the runtime listener on the guest port with the per-VM listener token, load the module catalog, start the file watcher and the port scanner, open the event journal, report `runtime.ready(vm_id, epoch, module_catalog_version, dispatcher_contract_version)`.

The **Workspace Runtime facade** in the control plane is the only client that opens VM channels through a host, and only after the lifecycle service has made the incarnation current; every other platform system (Agents above all) calls the facade. Every request through it is bound to a `workspace_id`, the expected `vm_id`, an actor or grant, a deadline and an idempotency key.

**Workspace Runtime API** (JSON-RPC on the `rpc` stream; events pushed on the same stream and journaled):

| Surface | Methods | Caller | Contract |
|---|---|---|---|
| Files | `fs.list`, `fs.read`, `fs.stat`, `fs.write(path, content, expected_hash)`, `fs.delete(path, expected_hash?)`, `fs.rename(from, to, expected_hash?)`, `fs.watch` → `file.changed` | editors and the file tree through the facade | explicit editing and observation; mutation preconditions prevent a silent overwrite |
| Terminal | `terminal.open(cols, rows)`, `terminal.write`, `terminal.resize`, `terminal.close`; bytes on a `pty` stream | a workspace member with write | the user's escape hatch; never a platform-extension mechanism |
| Modules | `modules.list`, `modules.describe`, `modules.invoke(exec_id, module_id, version, operation, input, grant_id)`, `modules.status`, `modules.cancel` | platform features through the facade | the sole execution and extension path; callers cannot supply `argv`, environment, cwd or signal targets |
| Ports | `ports.list`; `port.opened`, `port.closed` events | the Ports panel | observes listeners; starts nothing |
| Workspace status | `workspace.status.get` | the workspace UI, support, operators | lifecycle state, current or last `vm_id`, placement, lease freshness, the last receipt with a freshness label; when stopped, only durable and last-reported state, never a live claim, and never a wake |
| Runtime inspection | `runtime.inspect.summary` | owner or support grant | dispatcher, image and catalog identity, boot time, uptime, resource pressure, mounted roots, port summary, fixed health probes |
| Module diagnostics | `modules.logs(exec_id, cursor?)`, `modules.events(exec_id, cursor?)`, `modules.history(filter)` | owner or support grant | bounded, redacted logs and structured events; no raw host processes |
| Diagnostic bundle | `diagnostics.capture(profile)`, `diagnostics.probe(name)` | support grant; owners for safe profiles | versioned redacted bundles and declared read-only probes (`workspace-integrity`, `runtime-health`, `port-connectivity`, `module-catalog-integrity`); no caller-supplied shell command |

**Private control-plane protocol**, used only by the facade, the lifecycle service, the host and the dispatcher: `runtime.ready`, `lease.renew`, `health`; `events.replay(exec_id, from_seq)`, `events.ack(exec_id, up_to_seq)`, `outcomes.pending()`; `session_files.restore`, `session_files.harvest`; the admission barrier (`admission.close` returns when in-flight mutations finish; `admission.open`); `workspace.flush` (§5.4).

**Explicitly absent:** `proc.spawn/signal/wait/list` (a private runner used only after module resolution), `git.*` (native git for agents, the git service for refs and policy, `workspace.changes` for the UI projection), `agent.run_turn` (the `agent.turn` module), `preview.*` (the `workspace.preview` module), `checkpoint.*` and periodic checkpoint events (agent-driven persistence plus `workspace.flush`), arbitrary diagnostic shell execution (fixed probes and bundles).

Concurrency: every process runs in the same folder on the workspace branch. There is no worktree, branch or lock per task. Concurrent edits to one file have lost-update semantics: the last writer wins and git shows only the final content. Git raises a conflict only at rebase or merge time. Agents are told (the profile says so), and the UI shows a notice when more than one execution is running. Nothing stronger is enforced.

### 6.7 Declared modules

Every platform-initiated executable unit is a declared module invocation. A module declares: a stable `module_id`, a semantic version and an immutable manifest hash; named operations with typed input, output and event schemas; a pinned entrypoint and a lifecycle class (`bounded_run` or a desired-state `service`); limits (time, CPU, memory, disk, concurrency, output and log size); the minimal capabilities it requests (workspace read or write scope, bounded port allocation, network egress policy, git operation scope, provider-turn access); and its observability (health probes, diagnostic profile, event kinds, redaction policy). The dispatcher resolves the immutable catalog entry, validates the input, intersects the requested capabilities with the control-plane-issued `grant_id`, and only then uses its private runner. A manifest requests capabilities; it never grants them. The runtime image and the module catalog are hash-pinned; a workspace configuration may select or configure an installed module but cannot add code or privilege. Each invocation is auditable as `(workspace_id, vm_id, exec_id, actor, module_id, version, manifest_hash, grant_id, input_digest, capability_grant, outcome)`.

`modules.invoke` is idempotent on `exec_id = (task_id, attempt, vm_id)` and returns a disposition: `started`, `already_running`, `already_finished(outcome)`, or `cancelled` (a cancellation disposition exists for this identity, §7.5). Module events and terminal outcomes use the dispatcher journal, ordered replay, acknowledgement and the incarnation fence.

| Module | Owner | Class | Operations |
|---|---|---|---|
| `agent.turn` | Agents | bounded run | `run(AgentTurnInput)` → events `AgentTurnEvent`, outcome `AgentTurnOutcome` (§8.3) |
| `workspace.changes` | Workspace | bounded run | `get` (the normalized association, dirty, ahead/behind, conflict projection); `commit(message, paths?, push)`; `push`; `update` (fetch and rebase onto the base branch; refused when dirty); `switch_back` |
| `workspace.preview` | Workspace | service | `start(profile)`, `stop`, `status` (§6.8) |
| `workspace.bootstrap` | Workspace | bounded run | `run` (deterministic workspace setup; uplift's bootstrap submits child tasks through the API with its execution identity) |
| `workspace.diagnostics` | Workspace | bounded run | `capture(profile)`, `probe(name)` |

The dispatcher core (listener, journal, module runtime, admission, flush, restore) is edited only for defects once it ships; features arrive as modules and as versions of modules.

### 6.8 Preview

`workspace.preview start(profile)` runs the command from `.g5/preview_profiles/<name>.yaml` or the `preview:` block of `.g5/workspace.yaml` as a supervised desired-state service and records its port. **One preview per workspace**: a second `start` replaces the first, and a workspace that needs two running apps is forked. A chat that rebuilds the app may stomp on the running preview; that is accepted, and a fork gives separate execution. The browser reaches the preview through the API relay over the host tunnel (`http` streams, one per TCP connection) at `/api/v1/workspaces/:id/preview/:port/*`, with the relay code from `routes/preview.rs`, target resolution keyed by workspace and port, a workspace-scoped ticket cookie, and subdomain mode optional. Ports the agent opens on its own appear in the Ports panel from `ports.list` and can be opened through the same relay. The preview stops with the VM; preview traffic counts as activity for the idle timer.

Removed: the `preview` job type, `job_preview_sessions`, boot generations, `preview_reaper`, the `preview_vms` registry, the chat-attach listener, token and host drive. "Chat attached to a preview" is the default: agent and app share the VM.

### 6.9 Terminal

A browser terminal (xterm.js) over a `pty` stream after `terminal.open`, gated by workspace write access; the shell runs as uid 1000 in `/work` with the same environment the dispatcher gives modules (proxied model and git egress, task secrets). The PTY manager and the multiplexer exist today; the UI panel and the channel are new. Terminal activity counts as VM activity. Terminals are not recorded or audited in this iteration.

### 6.10 Fleet, placement, drain and rollout

**Hosts** are API-provisioned GCE instances, self-hosted Linux machines (including the Azure canary lane), or macOS dev hosts. A host is a set of equal-size VM slots (`G5D_HOST_MAX_MICROVMS`, VM CPU and memory from the host configuration), not a task pool. The reconciler (provisioning → ready, orphan sweep, `unreachable` after three missed heartbeats) stays. There is no maximum host lifetime: a host is drained only for an upgrade or by an operator, so a long-running workspace is never interrupted by a calendar.

**Placement** is a control-plane decision; the host only accepts or refuses. In order: a host in the org's region with a free slot that still holds this workspace's retained root; then any host in the region with a free slot; then `no_capacity`, surfaced to the user as `queued_for_capacity` with a position and retried by the dispatch loop. Org quota is `max_running_vms`, enforced before `vm.start`. A host that answers `slot_unavailable` despite the registry's count is authoritative and the control plane re-places; `workspace_busy` means the previous incarnation of the same workspace has not exited on that host yet.

**Drain and rotation.** Rotation is drain and replace: provision the replacement first, send `host.drain`, then for each VM run the stop sequence with reason `drain` (nudge, deadline, flush, release) and `vm.stop`; a VM whose flush fails is retried with backoff until the drain deadline, after which the teardown proceeds and records the loss; `host.drained` is followed by instance termination.

**Rollout of a new version.** Product versioning is per module and per component, with feature flags in `configs` for behavior that must switch independently of a deploy. The order is fixed by the version negotiation: the control plane deploys first and accepts the current and the previous host protocol version; hosts are replaced by drain-and-replace with the new runtime image; workspaces pick up a new VM image or dispatcher by a rollout drain (per workspace: nudge, deadline, stop sequence with reason `rollout`, release, and a resume with the new image on the next demand, immediately when the workspace had running work). A turn interrupted by the rollout continues after the resume as a successor turn whose prompt notes the interruption and asks the agent to continue (§7.2). Workers may be at most one version stale; the QA project exercises the sequence before production.

---

## 7. Task lifecycle (Agents)

### 7.1 Model

```
tasks(id, org_id, workspace_id, job_id NULL, kind chat_turn|agent|command,
      status queued|running|waiting|succeeded|failed|cancelled,
      spec jsonb,        -- provider, model, permission_mode, prompt or module input, profile, retry_on_vm_loss, wall_clock_limit
      outcome jsonb,     -- AgentTurnOutcome or the command outcome: status, code, retryable, final message ref, blocker, usage
      attempt int, vm_id NULL, epoch NULL, start_confirmed bool, depends_on uuid[],
      chat_session_id NULL, created_by, created_at, started_at, ended_at)

task_attempts(task_id, attempt, vm_id, epoch, started_at, ended_at, outcome,
              reason exit_code|provider_error|vm_lost|timeout|cancelled|interrupted|suspended,
              acked_event_seq bigint, PRIMARY KEY (task_id, attempt))

jobs(id, org_id, workspace_id, kind, status (derived), spec jsonb, created_by,
     max_concurrency, created_at, ended_at)          -- optional grouping for DAG runs

task_events(id bigserial, task_id, attempt, vm_id, seq bigint, ts, kind, payload jsonb,
            UNIQUE (task_id, attempt, vm_id, seq))    -- the agent timeline

task_questions(id, task_id, exec_id, kind question|approval|escalation, payload jsonb,
               status open|answered|consumed|expired|cancelled,
               answer jsonb, answer_version int,
               runner_wait_deadline_at, suspended_at, expires_at, answered_by, answered_at, consumed_at,
               successor_task_id NULL UNIQUE)         -- one HITL table
```

Job kinds are profiles over the same task machinery (`uplift`, `downlift`, `sync`, `explainer`, `agent_review`, `custom`). All jobs of a workspace run in that workspace's VM, in the same folder. A single agent run is a task without a job. A `command` task is a `workspace.bootstrap` invocation; `chat_turn` and `agent` tasks are `agent.turn` invocations.

### 7.2 State machine

```
queued ──invoke confirmed by the dispatcher──► running
running ──outcome succeeded──► succeeded
running ──outcome failed | timed_out──► failed
running ──question or permission opened──► waiting ──answer consumed──► running
running | waiting | queued ──cancel──► cancelled
running | waiting ──stop, drain or rollout──► failed(interrupted)   [re-queued once if spec.retry_on_vm_loss; a chat turn continues as a successor turn]
running | waiting ──runner wait ended with the question open──► failed(suspended) [the answer starts a successor turn]
running | waiting ──vm lost──► failed(vm_lost)                     [re-queued once if spec.retry_on_vm_loss]
```

Six states, one writer: `TaskService::apply(event)`, with events from the dispatcher (`started`, `question.opened`, `question.consumed`, `outcome`), from users (`cancel`, `answer`), from the stop sequence (`interrupted`) and from the reconciler (`vm_lost`, `timeout`). `running` is recorded when the API sends an invocation; the task counts as an execution only once the dispatcher has confirmed it (§7.5).

Rules.
- **Waiting tasks keep the VM awake for the runner wait.** An open question is activity for the idle timer while `runner_wait_deadline_at` (default: the workspace's idle timeout) has not passed. When it passes, the `agent.turn` supervisor suspends the execution (§8.4): it records the suspension on the question row with a conditional update, interrupts the provider, harvests the session files, and emits outcome `suspended` with blocker `{question, id}`; the VM may then idle-stop. A tool error is never what ends a turn. The question stays open until `expires_at` (org default 24 hours).
- **Answering** commits the answer to the question row first (`answered`, `answer`, `answer_version`, `answered_at`), bound to the question's `exec_id`. If the execution is still blocked, delivery is a read of that committed state by the blocked process's MCP server (§8.4); when consumed, the server records `consumed_at` and the task returns to `running`. If the execution ended (`suspended`, `interrupted`, `vm_lost`) with `consumed_at` null, the answer wakes the workspace (`workspace.runtime.ensure(reason = agent_question_resume)`) and the reconciler creates **exactly one** successor `chat_turn`, recorded in `successor_task_id`, whose prompt carries the answer and a note that the previous turn was interrupted while waiting. No transcript is resumed mid-turn. Delivery is at-least-once: a process that consumed the answer and was interrupted before the consumption record landed gets the answer again in the successor.
- **Interruption.** Stop, drain and rollout end a running or waiting turn with outcome `interrupted`: SIGINT, then SIGKILL after 15 seconds; partial assistant output is kept and marked `stopped`. A chat turn interrupted by a drain or rollout with no open question gets one successor turn after the resume whose prompt asks the agent to continue; a turn interrupted by a user stop does not.
- **Cancel** is the same signal sequence with outcome `cancelled`. It cancels the task's open and answered-but-unconsumed questions and any successor task that has not started, under the same lock the reconciler takes to create successors, so a cancel can never be followed by a continuation of the cancelled work. A cancel that arrives before the start was confirmed is delivered to the dispatcher as a durable disposition (§7.5).
- **Successor eligibility** is explicit: only outcomes `interrupted`, `suspended` and `vm_lost` with an answered, unconsumed question (or, for drain and rollout, none) create a successor; `succeeded`, `failed`, `timed_out` and `cancelled` never do.
- No automatic retry on agent-reported failure. Human retry creates a new task.
- Job status is a pure function of child states; cancel cascades; there is no pause and no rollback.
- The reconciler loop (§7.5) settles lost incarnations, expires questions, creates successor turns, and enforces `wall_clock_limit` with outcome `timed_out`.

### 7.3 Dispatch path

`POST /workspaces/:id/tasks` and a chat send insert `queued` work and return. The dispatch loop (every 2 seconds, and immediately after an insert, an answer, or a VM becoming `running`) does the following, in this order, so a stopped workspace is woken by demand rather than skipped:

1. Select `queued` tasks whose dependencies are met, regardless of VM state, and group them by workspace.
2. For each workspace whose VM is `stopped` or `failed`, call `workspace.runtime.ensure(reason = agent_turn)`; nothing else happens for that workspace this iteration (a `queued_for_capacity` answer is shown on the task).
3. For each workspace whose VM is `running` with a free concurrency slot, take the next ready task, reserve its inbox input (§8.1), record `attempt`, `vm_id`, `epoch`, `status=running`, `start_confirmed=false` in one update, and call `modules.invoke(exec_id, "agent.turn" | "workspace.bootstrap", …)` through the facade with `expected_vm_id`.
4. When the disposition arrives (`started`, `already_running`, `already_finished`, `cancelled`), record `start_confirmed=true` and apply an outcome if one came back.
5. Tasks that are `running` with `start_confirmed=false` on a current incarnation are re-invoked with the same `exec_id` on every iteration and on reconnect until a disposition arrives; the call is idempotent, so a delivered start whose reply was lost does not launch twice.

Workspaces whose VM is `starting` or `stopping` are skipped until the next iteration. Events stream back through the facade; the API appends `task_events` and publishes `task.event` / `task.status` with today's WebSocket event names. Uplift's deterministic bootstrap runs as a `command` task (`workspace.bootstrap run`) and submits child tasks through the API.

### 7.4 Batch jobs on one folder

Child tasks of one job run concurrently up to `max_concurrency` in the same folder on the workspace branch. There is no job branch and no final merge task; each agent commits and pushes its own work to the workspace branch, rebasing onto it first when another task pushed in between. Conflicts are the agent's to resolve. This removes `job_merge_service`, job merge runs, direct-clean preflights and the two-operation completion protocol.

### 7.5 Dispatch and event contract

The contract that makes §7.2 hold across crashes and lost messages, in place of leases and the lifecycle kernel. Every message that crosses the VM boundary is persisted on one side and confirmed by the other; duplicates are detected by key, never by ordering.

- **Execution identity.** `exec_id = (task_id, attempt, vm_id)`. The API sets it before any command; the dispatcher keys its process table, journal and dispositions by it.
- **Two-phase start.** The API records `running, start_confirmed=false` before invoking. `modules.invoke` is idempotent on `exec_id` and returns `started`, `already_running`, `already_finished(outcome)` or `cancelled`. The API records `start_confirmed=true` on any disposition. Until then the start is an outstanding intent that the loop and reconnect reconciliation re-send with the same identity, but only while the intent's incarnation is current.
- **Loss settles every intent.** When an incarnation is lost (row `failed`, or a different `vm_id`), every active execution bound to it, confirmed or not, is settled as `vm_lost` and follows the retry policy (one re-queue when `retry_on_vm_loss`, which creates a fresh attempt and wakes a replacement). Missing confirmation does not prove that no process ran; a replacement VM cannot execute a stale identity, so nothing is replayed across incarnations.
- **Durable cancellation.** `modules.cancel(exec_id)` records a **cancellation disposition** in the dispatcher's small disposition table for that identity even when no process exists yet; a later `invoke` of the same identity returns `cancelled` without spawning; a process that already won the launch decision is terminated. The API retries the cancel until the dispatcher acknowledges it and never re-sends a start for a terminal task. Dispositions (`cancelled`, `already_finished` with the outcome digest) are retained for the life of the incarnation, independently of event-journal pruning; they are small and die with the VM, and a new incarnation cannot receive an old identity.
- **Journaled events.** The dispatcher appends every event of an execution (tool calls, tool results, partial output, questions, the outcome) to an on-disk journal under `/var/lib/g5d/journal/<exec_id>` with a per-execution monotonic `seq`, and streams them live. The API inserts events with the unique key `(task_id, attempt, vm_id, seq)` and ignores duplicates; it keeps `task_attempts.acked_event_seq` as a cumulative watermark per attempt and acknowledges it (`events.ack(exec_id, up_to_seq)`), and the dispatcher prunes the journal up to the watermark. There is no highest-seen gate: a late event with a lower `seq` is inserted if its key is new. Events of an old attempt can never transition the current attempt.
- **Replay.** On (re)connect the API sends its watermark per open execution; the dispatcher replays from `watermark + 1` in order and queues live events behind the replay. Sequence spaces are per execution, so concurrent tasks never interleave. Acknowledged journals of finished executions are pruned oldest first under a disk budget.
- **Retained outcomes.** The outcome is the last journaled event (a C3 value, §8.3): the API persists the task transition and, for chat turns, the assistant message update in one transaction, then acknowledges. A crash before acknowledgement replays; a crash after is harmless.
- **Reconnect reconciliation.** When the channel to a VM is (re)established, the API sends watermarks and outstanding start intents; the dispatcher replays journals, returns dispositions for the intents and lists its executions. Confirmed-running tasks the dispatcher does not know become `vm_lost`; processes for tasks the API holds as terminal are cancelled; unconfirmed intents on the current incarnation are re-sent.
- **Reconciler loop (30 s).** Active executions whose incarnation is not current → `vm_lost` (a draining incarnation is current); questions past `runner_wait_deadline_at` whose execution is still active and whose supervisor has not suspended them (a dead or wedged module) are suspended by the reconciler through the same conditional update followed by `modules.cancel`, with one `suspended` outcome; questions past `expires_at` → `expired`, with the task failing with `timeout` if it is still waiting; answered questions with no consumer and no successor whose execution ended with an eligible outcome → one successor turn (§7.2); tasks past `wall_clock_limit` → cancelled with `timed_out`; `queued` tasks and unconfirmed intents re-offered to the dispatch loop.
- **Chat linkage.** A chat turn's `pending` assistant message is updated in the same transaction as its task's terminal state, so an API crash can never strand an assistant message in `pending` while the task is terminal, and the reconciler repairs any assistant message whose task is terminal.

---

## 8. Chats and the provider runtime (Agents)

### 8.1 Model, history and the inbox

```
chat_sessions(id, org_id, workspace_id, title, provider claude_code|codex, model,
              permission_mode, provider_session_id, session_manifest_generation int,
              created_by, forked_from_id, forked_at_index, created_at, updated_at)

chat_messages(id, session_id, index, role user|assistant|system, author_id NULL,
              content, attachments jsonb, revision int,
              delivery queued|reserved|delivered|delivery_unknown|withdrawn,   -- user rows
              status pending|streaming|complete|error|stopped,                 -- assistant rows
              task_id NULL, usage jsonb, created_at)

chat_session_files(session_id, provider, provider_session_id, relative_path,
                   bytes bytea, sha256, size, harvested_from_vm_id, harvested_epoch, updated_at,
                   PRIMARY KEY (session_id, provider_session_id, relative_path))
```

Any workspace member with write access may send into any session (`author_id`). Chat history is a Postgres feature: `chat_messages` holds the conversation and `task_events` the agent timeline for every turn, complete across API restarts (§7.5). The browser reads both through the API; no VM is involved, and a stopped or deleted VM never affects what a user sees.

**The inbox.** A user message is a durable row before it is anything else. A message sent while no turn runs is reserved immediately by the dispatch loop; a message typed while a turn runs stays `queued`, is shown in a queued strip, and may be edited (a new `revision`) or withdrawn until it is reserved. `reserved` freezes one revision for one execution; `delivered` is recorded when the adapter journals `input.accepted`, which for the CLI adapters is the moment the frozen prompt has been written to the provider's stdin and stdin closed (a module `started` disposition confirms only that the execution exists, not that the provider received anything); an execution that ends without `input.accepted` (session restore, provider spawn or the stdin write failed) releases the reservation back to `queued`; `delivery_unknown` is the state after a loss of the incarnation between reservation and an acknowledged `input.accepted`, and it is resolved on reconciliation: the reservation is released back to `queued` (the successor attempt re-delivers it) unless the replayed journal or the outcome shows it was accepted. Delivered revisions are append-only transcript truth. A withdrawn message is absent from the provider transcript and kept as an audit row. Each turn takes the FIFO head of the queue; whether several queued messages are coalesced into one turn is an implementation-time choice. There is no forced fork, no worker pin, no route classification and no resident process.

### 8.2 A turn

1. `POST /sessions/:id/messages` inserts the user message (`queued`) and, if no turn is running, creates a `chat_turn` task; otherwise the row waits in the queue.
2. The dispatch loop ensures the runtime if needed (§7.3), reserves the message and calls `modules.invoke(exec_id, "agent.turn", "run", AgentTurnInput)`; the disposition confirms that the execution exists.
3. The module restores the session files if they are missing on the VM (§8.5), launches the provider CLI (§8.3), writes the frozen prompt to its stdin and closes it, journals `input.accepted` (which marks the message `delivered`), then journals and streams its events mapped to `chat.chunk`, `chat.message_updated`, `chat.done`, `chat.error` and to `task_events`, and waits for the process to exit.
4. Claude tool-permission prompts and both providers' user questions become `task_questions` rows; the process blocks inside the tool call until the committed answer is read or the runner wait ends (§8.4).
5. Process exit is the end of the turn. The module harvests changed session files, emits the outcome, and nothing keeps running. The provider exits only when its own subagents and background work are done; G5 does not track them (a Claude turn that asks to continue in the background is a failed turn with a clear code in the MVP).

Concurrent sessions in one workspace are concurrent processes in one folder. Fork is a row copy of messages up to an index (§8.6).

### 8.3 The `agent.turn` module and the provider adapters

Agents owns the module and releases it; Workspace hosts it. Every cross-boundary fact belongs to exactly one category:

| Category | Meaning | Examples |
|---|---|---|
| **C1** control-plane command or lifecycle fact | request and response outside the ordered event stream | `workspace.runtime.ensure`, `modules.invoke`, cancel, process exit |
| **C2** ordered non-terminal turn event | a replayable observation within one `exec_id`; can update projections, never finishes the attempt | message delta, tool completion, question opened, usage update |
| **C3** exactly one terminal turn outcome | the sole durable completion authority for an `exec_id` | `succeeded`, `failed`, `cancelled`, `interrupted`, `timed_out`, `suspended` |

Consequences: the event union contains only C2 members (there is no `turn.completed` event); the outcome is written once; a provider's terminal-looking signal is only a candidate until the adapter's settlement rule (result frame plus EOF plus exit status for the CLIs) holds; an invocation disposition, an RPC error or a process exit is C1, never a fabricated turn event; an unknown provider variant fails the compatibility gate rather than being classified as success.

```
AgentTurnInput { task {task_id, attempt, session_id, initial_input?: reservation},
                 provider {provider_id, model_id, permission_mode non_prompting | tool_approval, options},
                 prompt_ref, workspace_target {repo_id?, relative_path?},
                 session_binding {provider_session_id?, expected_manifest_generation?},
                 resume? {reason: question_answered, prior_exec_id, question_id, answer_version},
                 limits {wall_clock_ms, max_turns?, max_output_bytes} }

AgentTurnEvent  = provider.session | provider.status | message.delta | message.final | thought.delta | thought.final
                | input.accepted | tool.started | tool.progress | tool.completed | workspace.change
                | question.opened | question.consumed | permission.opened | permission.resolved
                | context.compacted | usage.updated | notice | terminal.progress | …   (the full union is agent-api-surface.md §9)

AgentTurnOutcome { status, code, retryable, final_message_ref?, blocker? {kind question|permission, id},
                   provider_session_id?, session_manifest_generation?, usage?, last_event_seq }
```

Large or sensitive bodies travel as references. The module input cannot contain `argv`, environment, host paths, signals, credentials or host endpoints; the module's manifest requests `workspace.native_files`, `workspace.native_git`, `workspace.native_shell`, `workspace.native_tests`, `network.agent_control_plane` and `network.agent_llm_proxy`, and the runtime grants their intersection with policy.

**Adapters for the MVP: one CLI process per turn, for both providers.** This is the runner already on `main` (`backend/g5d-worker/src/runner/`, contract in [lean-provider-cli-contract.md](lean-provider-cli-contract.md)), moved into the module and pointed at the proxies:

```
codex exec [resume <provider_session_id>] --json --skip-git-repo-check --output-last-message <tmp> [--model <m>]
      -c approval_policy="never" -c sandbox_mode="workspace-write" [-c model_reasoning_effort="<e>"] [-i <attachment>...]
      # prompt on stdin, then EOF; CODEX_HOME=/home/agent/.codex with the g5-tools server (approval_mode = approve per tool) and its tool timeout,
      # OPENAI_BASE_URL=http://g5-llm:<port>/s/<session_id>/v1, placeholder key

claude -p --output-format stream-json --input-format stream-json --verbose --include-partial-messages
       [--resume <provider_session_id> | --session-id <new uuid>] --permission-mode <mode>
       ( --permission-prompts host --permission-prompt-tool mcp__g5-tools__request_permission --mcp-config /work/.mcp.json | --permission-prompts none )
       --strict-mcp-config [--settings /work/.claude/settings.json] --setting-sources project --disallowedTools AskUserQuestion
       [--model <m>] [--effort <e>] [--max-turns <n>] [--append-system-prompt <runtime boundary text>]
       # one stream-json user message on stdin, then EOF; CLAUDE_CONFIG_DIR=/home/agent/.claude,
       # ANTHROPIC_BASE_URL=http://g5-llm:<port>/s/<session_id>, placeholder key, MCP_TOOL_TIMEOUT above the runner wait; runs as uid 1000
```

Runner behavior common to both: chat turns use the server-frozen mode from `chat_sessions.permission_mode`; non-chat tasks run Claude with `bypassPermissions` and Codex with `approval_policy=never` / `sandbox_mode=workspace-write`, so they never block on a human for permissions (they may still ask questions through `ask_user` when their profile allows it); for a new session the runner generates the UUID (`--session-id`) or records Codex's id from the first item, and passes `--resume` / `exec resume` later; input is written to stdin and stdin is closed, after which the adapter journals `input.accepted` for the reservation, so there is no live steering (a queued message is the next turn); output is one JSON object per line, mapped by the adapter into `AgentTurnEvent`s, `chat.chunk` deltas and `task_events`; cancel and interruption are SIGINT then SIGKILL after 15 s; the turn ends at process exit and the outcome is settled from the result frame, EOF and the exit status together; `settings.json` hooks still fire in print mode and the profile keeps only `SessionStart` and `Stop`; the runtime boundary text is passed with `--append-system-prompt`. The interim silence timers of the current runner (first-frame, no-tool and in-flight-tool) are kept only as generous safety nets under `wall_clock_limit` and are not lifecycle signals.

**Two kinds of human input, and which provider supports which.**

| Interaction | Mechanism | Claude | Codex |
|---|---|---|---|
| Agent asks the **user a question** | MCP tool `ask_user(question, options)` on the `g5-tools` server (also `g5d ask`): posts `task_questions(kind=question)` through the proxied API with the execution's `exec_id`, then long-polls the committed answer and returns it as the tool result, recording `consumed_at`; never touches the process's stdin | supported | supported |
| Provider asks for **tool-permission approval** | Claude: `--permission-prompt-tool mcp__g5-tools__request_permission` posts `task_questions(kind=approval)`, polls the same way, returns `{"behavior":"allow"|"deny", …}`. Codex: `codex exec` emits approval requests it cannot receive answers to, so Codex runs only non-prompting policies | supported | not supported under `exec` (deferred, §17.2) |

### 8.4 Questions, permissions and their deadlines

The API commits the answer to the question row before anything else happens (§7.2). The `g5-tools` server, blocking inside the tool call, polls `GET /questions/:id?wait=` through the proxy until the row is `answered`, `expired` or `cancelled`, or until its process is reaped. On an answer it returns it to the provider and records `consumed_at` (idempotent, keyed by `answer_version`). **Suspension is a decision of the `agent.turn` supervisor, never of the tool.** A tool error is only a tool result, which the model may retry or ignore before finishing normally, so it cannot be the authority that ends a turn. When `runner_wait_deadline_at` passes with the question still `open` and unanswered, the supervisor records `suspended_at` on the row through an API call whose conditional update succeeds only while the row is `open` and unanswered (if an answer or a cancel landed first, the call reports it and the turn continues or is cancelled), interrupts the provider (SIGINT, SIGKILL after 15 s), harvests the session files, and emits exactly one outcome `suspended` with the blocker; the question stays answerable until `expires_at`. If the supervisor is gone, the reconciler performs the same update and cancels the execution (§7.5). An answer that arrives after suspension wakes the workspace and starts exactly one successor turn carrying the answer (§7.2). A question that expires unanswered fails the task with `timeout` if it is still waiting, or is simply closed if the turn had suspended. Every local timeout (`MCP_TOOL_TIMEOUT`, Codex `tool_timeout_sec`, the poll's own timeout) is set above the runner wait so the row's deadlines are the only places expiry is decided; a proxy severing an idle long-poll is a re-park, not a failure. The client renders an open question persistently whether or not a VM is attached, and an open question raises a notification. Whether the providers' native ask-user surfaces can replace the MCP tool is an open question (§17.3).

### 8.5 Provider session files in Postgres

Provider session state must survive VM loss and be independent of any VM. It is stored only in Postgres, in `chat_session_files`, and lives on the VM as a cache outside every working tree: Claude under `CLAUDE_CONFIG_DIR=/home/agent/.claude` (`projects/<escaped cwd>/<session id>.jsonl` and `<session id>/subagents/*`; `cwd` is always `/work/<root_path>`, so the project key is deterministic across incarnations); Codex under `CODEX_HOME=/home/agent/.codex` (`sessions/YYYY/MM/DD/rollout-<ts>-<session id>.jsonl`). After every turn, and again in the stop sequence, the module harvests files whose sha256 changed and upserts them (zstd-compressed, 128 MiB per file, tagged with the harvesting `vm_id` and `epoch`; a harvest from a non-current incarnation is rejected, so an older transcript never overwrites a newer one; `session_manifest_generation` increases with every accepted harvest). Before a turn that resumes, if the files are absent on the VM, the module restores them first. If Postgres has nothing for that session, or the CLI refuses the restored session before any frame, the module starts a new provider session and prefixes the prompt with a history built from delivered transcript rows, then records the new `provider_session_id`. Nothing about sessions is flushed into git, and no session file can be captured by a flush or a native `git add`. The bundle is opaque to Workspace and Worker & Host.

### 8.6 Fork and carry

Forking a chat copies `chat_messages` up to an index into a new session with `provider_session_id = NULL`; the first turn starts a fresh provider session with the history. Creating a workspace with "carry chats" does the same per selected session into the new workspace. Provider-native session forking is not used.

### 8.7 Provider pins, gates and the resident-process option

A provider bump is a schema migration: the pinned versions (`2.1.270` and `0.153.3` on `main`) change only with re-recorded fixtures, the exhaustiveness fixtures of the adapter (every frame type and terminal reason classified; unknown discriminants, malformed lines, contradictory terminal evidence and nonzero exits cannot become success), the golden turns (fresh, resume, restore fallback, question before and after suspension, permission allow and deny, cancel, timeout, VM loss, Codex interruption without a terminal record), the durability test (kill the VM after acceptance and before harvest acknowledgement; the input is neither lost nor duplicated) and the redaction tests.

The `agent.turn` contract does not depend on how the adapter drives the provider. The Claude Agent SDK is not used: it launches the same Claude Code CLI and speaks the CLI's stdio protocol, so it offers no provider capability the CLI lacks, and its published type definitions (`sdk.d.ts`) serve at most as a schema source for the Rust event types. What the per-turn model does not offer is anything that needs a process to outlive a turn: steering a running turn by writing a further message to stdin, an in-band interrupt, and live model or permission changes. A later Agents project may keep the CLI process resident for a chat session (stdin open until the session idles or the VM stops) to add those, after probes on the pinned CLI establish how it queues or folds a mid-turn message and how it answers an in-band interrupt; and it may replace the Codex adapter with the App Server protocol (interactive approvals). Each such change amends this design, keeps the module input, event and outcome schemas, lands its fixtures and failure-injection tests first, and keeps the per-turn adapter as a bounded rollback lane during rollout. Until then the per-turn CLI adapter is the production path.

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

Reads never resume the VM. The file tree and the Changes panel follow the same rule; while stopped, the Changes panel shows the receipt's dirty set.

### 9.2 Writes

Save sends `PUT /workspaces/:id/files/*path` with `expected_hash`. The API authorizes the member, refuses a stopping workspace with `WORKSPACE_STOPPING`, **ensures the VM is running** (a stopped workspace is resumed and the UI shows "resuming workspace", or `queued_for_capacity`), then asks that incarnation for the association (`workspace.changes get`, §5.7) and calls `fs.write` with the same `expected_vm_id`. A hash mismatch returns 409 with the current content; the editor shows a reload-or-overwrite choice. Agent writes reach the UI through `fs.watch` → `file.changed`; an editor with no local edits reloads silently, one with local edits shows "changed on disk". Commit and Push are buttons over `workspace.changes commit` and `push`. Job launchers never auto-commit anything.

### 9.3 Editors

Monaco and the block-style spec editor keep their UI but bind to local document state with Save (Ctrl+S) and optional autosave-on-blur, instead of a Yjs text. The structured question editors and explainer views follow the same pattern; the question-document view that already renders without Yjs is the template. Comments anchor to `(path, line, line hash)`. There is no presence and no co-editing; because all writes go through one Save API, a document-only CRDT can be added later without changing persistence.

### 9.4 Projections and convergence

`docs`, `ontology_nodes` and search remain projections of git files. The event bus is a non-durable fan-out and a ref can be updated immediately before a crash, so projections never depend on an event: each records the OID it was computed from per `(repo_id, ref)` in `projection_state`; a convergence loop at API startup and every 60 seconds lists the authoritative OIDs of every workspace branch, every complete receipt's snapshot ref and every protected branch, compares them with `projection_state`, re-projects what differs and removes rows for refs that no longer exist. `branch.updated` and `flush.completed` events trigger the same work early. PR `head_sha` and `workspace_repos.head_sha_cache` are projections under this contract.

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

Created from the UI or by an agent with `g5d pr create`. Creation reads the source OID from the internal branch at that moment, publishes the branch to the provider at that OID and records it as `published_sha`, and opens a provider PR when the repo has a provider; the provider PR is a mirror for external reviewers, not the source of truth for merging. A PR is created from what is on the branch: if `workspace.changes get` reports dirty or unpushed state, the UI offers "Commit and push all" (`workspace.changes commit(push: true)`, a user commit with laptop semantics, refused while a bounded-run execution is active unless the user chooses to interrupt it) before creating the PR, and an agent pushes before it calls `g5d pr create`. No flush is involved. `head_sha` is a projection of the source branch (§9.4); every decision and the merge operation use OIDs they read from git themselves.

### 10.3 Approvals

```
approval_rules(id, org_id, repo_id, target_branch, required_approvers uuid[],
               min_approvals int, agent_reviewer_profile NULL, created_by, created_at)

approval_decisions(id, org_id, pull_request_id, source_sha, actor_user_id NULL,
                   actor_task_id NULL, outcome approved|changes_requested, note, created_at)
```

A decision binds to the exact `source_sha` the approver reviewed and is written only by an API route: the UI records the OID it displayed; an agent review task is launched with the OID it must review and `g5d pr review` records that OID through the API. A new push makes prior decisions stale. Approvers are the rule's list plus org admins. Approval state belongs to the PR, never to VM state, and every approval comes with explicit instructions on what an approval means (the review flow is defined with the UX team).

### 10.4 Roles and login

Org roles `owner|admin|member|viewer` and workspace visibility with members (`write|read`) are kept. Per-user scope grants, workspace permission overrides and workspace mutation policy are dropped. RLS tenant isolation stays. Platform login is GitHub in the SaaS; self-hosted installations add Microsoft Entra (§16.3). `user_scm_connections` keeps GitHub and GitLab repository credentials for publish and PR operations. Operator permissions (who creates organizations) are not part of this work.

### 10.5 Removed

`approval_definitions/configs/requests/statuses/review_runs`, `merge_evaluations`, `agent_approval_handoffs`, `job_merge_runs/artifacts`, `workspace_pull_request_events` and `_provider_repos`, coordinated repo publication, workspace merge / rebase-merge / change-graph services, stages and merge-target columns, the non-GitHub login adapters (Entra returns for self-hosted only).

### 10.6 The merge operation

`POST /pull-requests/:id/merge` is the only writer of protected refs. It is a persisted, resumable operation with one identity (`merge_op_id`) and pinned inputs kept until the operation is resolved; a retry of an unresolved operation never re-pins.

1. **Pin.** If the PR has an unresolved operation (`pending` or `provider_merged`), resume it at the step its state indicates; otherwise, under a per-repository lock that also serializes inbound fetches (§5.6), read the source and target OIDs from the mirror and store them with a new `merge_op_id` as `merge_source_sha` and `merge_target_sha`, `merge_state=pending`.
2. **Authorize.** PR is `open`; the caller may merge; decisions whose `source_sha` equals `merge_source_sha` satisfy the rule.
3. **Mergeability.** Fast-forward or clean merge of source onto target, computed on the bare mirror without writing refs.
4. **Provider-connected target.** Republish the source at `merge_source_sha` if `published_sha` differs; call the provider merge API with the expected source SHA; on success record `merge_result_sha` and the durable `merge_state=provider_merged`; fetch the target from the provider into the mirror under the same lock (the mirror never computes its own merge commit for a provider-backed target); record `applied`, `status=merged`, `merged_at`, `merged_by`. An import failure leaves `provider_merged` and is retried. Ambiguous replies are reconciled on retry by reading the provider PR: attribute a remote merge to this operation only if the provider reports the pinned source as the merged head; if the head changed, or it merged at another head, record `failed` for a re-run with fresh inputs.
5. **Local-only target.** Compute the merge commit on the mirror, persist its OID as `merge_result_sha` before touching refs, then in **one ref transaction** (`git update-ref --stdin` with `start`/`prepare`/`commit`) compare-and-swap the target from `merge_target_sha` to the result and create `refs/g5/merges/<merge_op_id>` at the result. On recovery, the receipt ref proves application whatever the target holds now (a later fast-forward or rewrite of the target does not hide it); a missing receipt with a target that is neither `merge_target_sha` nor the result means another writer moved the target before application, and the operation records `failed` for a re-run with fresh inputs. Receipt refs are pruned when the PR closes or 30 days after `applied`.
6. **Failure.** Terminal `failed` is recorded only for a known rejection or conflict before anything was applied. Once the provider has merged, or the receipt ref exists, the operation can only progress to `applied`. There is no rollback because nothing that was applied needs undoing.

PRs may also target another workspace's branch; those targets are not protected, so the operation follows step 5 and skips the provider.

---

## 11. Agent surface

### 11.1 Keep (platform operations)

| Command / tool | Purpose |
|---|---|
| `g5d pr create|status|review` | open a PR, read its state, record an agent review decision pinned to the reviewed SHA |
| `g5d ask` and MCP `ask_user` | post a question and block until the committed answer is read, or until the runner wait ends (both providers) |
| MCP `request_permission` | Claude's permission-prompt tool; posts an approval question and blocks the same way |
| `g5d task status|list` | inspect tasks and jobs |
| `g5d preview start|stop|status`, `g5d ports` | control the preview (one per workspace), list open ports |
| `g5d auth`, `g5d doctor` | CLI login for laptops and diagnostics |
| MCP `g5-tools` grounding and ontology tools | retained and decoupled; outside the MVP acceptance (§2) |

All of these reach the control plane through the credential proxy (§6.5) with `G5D_EXEC_ID` naming the execution; none holds a token. Agents cannot create workspaces from inside a workspace; sub-agents inside a workspace are the main agent's business and untracked.

### 11.2 Native

Git (`commit`, `push`, `pull`, `rebase`, `fetch`), files, shell, tests, package managers. `origin` is the internal git service through the proxy rewrite, with the platform excludes file configured. Agents are told: stay on the branch; other agents may be working in the same folder and the last writer wins; commit often; push to `origin` when a unit of work is done, because nothing else saves your work while you run; rebase onto `origin/main` to update; push rewrites only with `--force-with-lease`, never bare `--force`, and after a fetch prefer the explicit form `--force-with-lease=<branch>:<expected sha>`; never push to `main` (the server rejects it anyway); when the platform asks you to checkpoint before a teardown, commit and push at once.

### 11.3 Removed

`g5d commit`, `push`, `connect`, `conflicts`, `revert`, `fork`, `refresh`, `merge`, `sync`, `view`, `spec sync`, `task rebase`, `task complete` (a task ends when its process exits), `workspace push|switch`, `job merge`, `chat context`, `intent-store`, `issue *`; the internal tool registry; the four issue MCP tools; the session-store proxy; MCP preflight reduced to a startup self-check. Most of these are already gone from `main`.

### 11.4 Profiles

`CLAUDE.md` and `AGENTS.md` describe native git, the branch rule, the shared-folder rule, the `--force-with-lease` rule, the checkpoint-on-request rule, `g5d pr` and `ask_user`; `/commit` and `/pr` are the reference commands with `g5d pr create` in place of `gh pr create`; hooks are `SessionStart` and `Stop` only. This is on `main` except the `g5d pr` switch (P6.1) and the teardown-nudge text.

---

## 12. Data model and migrations

**Identity and tenancy:** `orgs`, `users`, `org_memberships`, `org_invitations`, `user_scm_connections`, `api_keys`, `refresh_tokens`, `oauth_states`, `cli_device_authorizations`, the `authz` schema, `self_hosted_licenses`.

**Workspace and git (§5):** `repos`, `repo_secrets`, `workspaces`, `workspace_repos`, `workspace_members`, `workspace_settings` (idle timeout, question deadlines, retain hours, defaults), `workspace_vms`, `workspace_flushes`, `projection_state`.

**Worker & Host (§6):** `worker_hosts` (host id, org, region, kind `gce|self_hosted|macos_dev`, status with `unreachable`, slots and slot size, `runtime_identity`, capabilities, credential expiry, drain and heartbeat times, cloud instance fields).

**Execution and chat (§7, §8):** `tasks`, `task_attempts`, `jobs`, `task_events`, `task_questions`, `chat_sessions`, `chat_messages`, `chat_attachments`, `chat_session_files`, `usage_records`, `provider_accounts`, `provider_quota_snapshots`.

**Governance (§10):** `pull_requests` (with the merge-operation columns), `approval_rules`, `approval_decisions`.

**Projections and product features:** `docs`, `ontology_nodes`, `node_dependencies`, `ontology_grounding`, `search_documents`, `search_reindex_queue`, `comment_index`/`comment_replies`/`comment_mentions`, `notifications`, `user_preferences`, one `configs(scope, scope_id, key, value)` table for org, user and workspace configuration and feature flags, `platform_issue_outbox`.

About 36 tables. Everything else in the baseline (draft, task-commit, head-operation, advance, `workspace_branches`/`workspace_branch_repos`, branch-integration, sync-run, job-merge, approval-ledger, sticky-runtime, `workers`, `worker_tokens`, execution-attempt, question-window, escalation, preview-session, golden and builder tables, issue and hotlist tables, and the rest of draft 6 §12's dropped list) is removed by the PR that deletes its last consumer.

**Migrations.** `001_lean_baseline.sql` is frozen (checksum-guarded); no PR edits it. Every schema change is a forward migration `NNN_<description>.sql` with its SHA384 appended to the checksum catalog, numbered in merge order, validated as a fresh install and as an upgrade from the previous schema on a copy of real rows, with the no-op re-run checked. Dev and QA databases persist across deploys; the "about 36 tables" target is reached by drop migrations, not by rewriting the baseline. Provider enum everywhere: `claude_code | codex`. Task and chat status vocabularies are one enum each in the database, Rust and the frontend.

---

## 13. Delivery

The plan is [lean-implementation-plan-v7.md](lean-implementation-plan-v7.md); this section fixes what it must respect.

**Owners.** Every PR names the capability that owns it (§4.1); the owner reviews and merges it. A component is owned by the capability it serves, not by the process it runs in: the `agent.turn` module runs in the VM and is Agents'; the `ctl` client runs in g5-api and is Worker & Host's; the lifecycle service runs in g5-api and is Workspace's.

**MVP milestone.** The nine proposals on the GCP QA environment: a workspace with a VM, a dispatcher and modules; explicit Save and the Changes panel with no CRDT; agent-driven durability with the flush at graceful teardown and restore from the receipt; Claude and Codex chats as per-turn CLI processes with queued messages, questions and permissions; agent tasks and jobs on the dispatcher; preview and terminal on the VM; the host protocol with proxies; the incarnation fence; the existing git boundary (protected refs, workspace branches, native push). Not in the MVP: PR and approval UI on the new tables and the merge operation (the week after), the provider SDK adapters, the self-hosted phase, the crate consolidation and CI review, the remaining deletions that do not block the above.

**After the MVP, in order of priority.** Governance (P6.x) and the remote-tracking deletion and publish (P1.5, P1.6); the pre-launch audits of the APIs and data model (security, fault tolerance, scalability, with the table list written down after the APIs); lifecycle and schema cleanup (P7.x); the self-hosted phase (§16); the resident-process provider track; crate consolidation and the CI review.

**Rollout procedure** is §6.10; the QA project rehearses control-plane-first deploys, a host drain and a workspace rollout before the first production deploy.

---

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Agents forget to push and a host is lost | Profiles say push when a unit of work is done; the nudge before every deadline-bound teardown; the loss is bounded to the last receipt and shown, never silent |
| A flush publishes a partial multi-repo cut | Immutable per-flush refs plus a receipt completed only after the API verified every ref; consumers read receipts only (§5.4) |
| A live flush cuts a moving tree | Quiescence: module and file-mutation admission closed and drained, bounded runs ended or interrupted; terminals and previews recorded as concurrent writers (§5.4) |
| A message is marked delivered before the provider received it | `delivered` follows the adapter's `input.accepted`, never the module's `started` disposition (§8.1) |
| The model ignores the suspension tool error and finishes the turn | Suspension is the supervisor's decision: a conditional update, reaping, one outcome (§8.4) |
| A fenced host refuses the stop meant to free its slot | `vm.stop` is fenced by ownership only, and release follows the host's exit (§5.8, §6.2) |
| A warm root differs from the receipt with the same paths dirty | The receipt is always materialized on attach; no path-based equality check (§5.5) |
| A host lease outlives the control-plane grant | The host arms its timer from the request's monotonic start; boot leases carry the command deadline; proxies stop at the host deadline (§6.4) |
| A superseded VM reconnects and keeps writing | Incarnation fence at every boundary, including the proxies; reconnect kills stale VMs (§6.4) |
| A cancelled start launches after journal pruning | Dispositions are a separate small table retained for the incarnation (§7.5) |
| An unconfirmed start is stranded by incarnation loss | Loss settles every intent bound to the lost incarnation (§7.5) |
| An answered question restarts cancelled work | Successor eligibility is explicit; cancel and successor creation share a lock (§7.2) |
| Two attempts on one VM collide in the timeline | Event keys and watermarks carry the attempt (§7.5) |
| A stopped workspace never receives its work | Dispatch selects work regardless of VM state and ensures the runtime (§7.3) |
| Save fails on a stopped workspace | Ensure the runtime, then query the association on the same incarnation (§9.2) |
| A crash between a command and its outcome | Two-phase invocation, journaled events, retained outcomes, reconnect reconciliation (§7.5) |
| A question blocks a VM forever | The runner wait suspends the turn; the answer wakes a successor (§8.4) |
| Credentials leak into a provider process or across users | Host proxies inject on egress; per-session grants; the token only in the dispatcher (§6.5) |
| A rollout interrupts working agents | Nudge, deadline, flush, successor turn with a continue note (§6.10) |
| Old hosts receive commands they cannot parse | Protocol and spec versions negotiated; control plane first (§6.2) |
| Backups restore a database whose git objects are gone | Backup barrier across Postgres and the git store (§16.4) |
| Claude print-mode behavior differs from what the adapter expects | Pinned CLI, recorded fixtures, exhaustiveness gates on every bump (§8.7) |
| Codex cannot answer permission approvals under `exec` | Non-prompting policies only; questions still work through `ask_user`; App Server later (§17.2) |
| A provider process never exits | `max_turns`, `wall_clock_limit`, SIGINT then SIGKILL |
| Session-file restore fails | Fall back to a new provider session with the delivered history (§8.5) |
| Several agents in one folder overwrite each other | By design; agents are told; the UI notices |
| Owner force-push discards an unseen commit | Profiles require `--force-with-lease`; only laptop-versus-VM races can hit it |
| Approval, publication and merge drift apart | One pinned source OID; provider merge with expected SHA; receipt ref for local merges (§10.6) |
| An event is lost after a ref update | Projections converge from authoritative OIDs (§9.4) |
| Reads while stopped are only as fresh as the last flush | Every graceful teardown flushes; the UI labels `source: flush` |
| Single API replica | The tunnel registry and event bus are process-local; moving them to Redis is deferred (§17.2) |
| Rebinding two large editors off Yjs is frontend-heavy | The question-document view is the template |

---

## 15. Validation

The Rust end-to-end crate and the Playwright flows are pruned per removed feature and extended per PR with tests that fail without the new behavior and exercise the production boundary. Beyond draft 6's list, which stays in force where the mechanism is unchanged, this draft requires:

- **Flush and receipt:** a shutdown flush of a two-repository workspace whose second push fails, and separately whose receipt commit fails, leaves the previous receipt as the restore point after a deadline-bound teardown and a cold resume, and the retained refs still resolve; a fork of a running workspace while an agent updates two related files is refused or interrupted per the quiescence rule and the fork restores a stable tree; a Save admitted just before a fork flush and released while the capture runs is waited for and the cut is consistent, also for delete and rename and across two repositories; both admissions reopen after a refused and after a failed live flush; a user stop whose flush fails leaves the VM running with admission reopened; a drain whose flush fails proceeds at the deadline and records the loss; pruning keeps the two newest complete receipts and their objects; a receipt whose ref the API cannot verify is never completed.
- **Restore:** committed-but-unpushed commits on a clean tree restore with the branch ahead of origin; local commits plus dirty edits restore both; a tracked file matched by `.gitignore` with a dirty edit is captured and restored, including a same-second same-size edit; a warm root whose file contents changed after the last receipt with the same `HEAD`, dirty paths and sizes attaches with exactly the content a cold attach gives; provider session files are never captured by a flush or a native `git add -A`.
- **Host protocol and lease:** hello, accept and reject with an unsupported version (the host self-drains and keeps its VMs); a `vm.start` whose result is lost is resolved by the inventory; a `vm.event exited` survives a tunnel drop; a delayed renewal reply followed by a partition expires the host before the server grant, the proxy refuses traffic and a replacement is placed only after; a command delivered after its deadline is refused; a stale incarnation is refused on every command, stream and proxy request, while a draining one is accepted; two incarnations of one workspace on one host are refused with `workspace_busy`; `vm.stop` for a superseded `vm_id` the host still runs stops it and frees the slot while `vm.start` and stream opens for it are refused; the row stays `stopping` until the host's exit and a lifecycle restart after `vm.stop` re-sends it and then releases; a restart of the workspace right after a stop never sees `workspace_busy`.
- **Proxies:** a model request without a session prefix is refused; a session with no grant is refused; a grant update takes effect on the next request without a restart; a second user's session in a shared workspace uses that user's grant; no credential appears in any provider process environment; git pushes to the internal service succeed with no token in the VM.
- **Modules:** `modules.invoke` returns the right disposition for started, running, finished and cancelled identities; the module input cannot carry `argv` or host paths; a module exceeding its limits is stopped with a definite outcome; catalog integrity is probed.
- **Dispatch and events:** every draft 6 case, plus: incarnation loss before delivery and after spawn with the reply lost both settle, with and without retry; cancel → durable disposition → acknowledgement and journal pruning → delayed invoke still returns `cancelled`; answer commit → cancel → reconciliation creates no successor, including with a concurrent successor creation; an interrupt-and-requeue on the same incarnation gives the new attempt its own sequence space and delayed acknowledgements from the old attempt do not move the new one.
- **Questions:** an answer during the runner wait is delivered once; a runner wait that ends suspends the turn, the VM idle-stops, and the answer wakes the workspace into exactly one successor; a provider fixture that ignores the tool error and finishes normally is still suspended at the deadline, reaped, its question left answerable, and the answer wakes one successor; an answer racing the suspension and a cancel racing the suspension each yield exactly one outcome; an answer racing a stop is delivered to the process or to one successor, never both.
- **Save:** Save with no VM and a cold restore reaches resume before any module query, then exercises a matching branch and a mismatch.
- **Chats and inbox:** two sessions concurrently in one folder; a message typed during a turn is queued, edited, withdrawn or delivered to the next turn; after the module reported `started`, a failed session restore, a failed provider spawn and a failed stdin write each leave the message not `delivered` and back in `queued`; a VM killed after `input.accepted` and before its acknowledgement neither loses nor duplicates the input; stop and resume with continuity; host kill mid-turn; a Claude session and a Codex session in one workspace.
- **Workspace actions:** create, stop, resume, restart (with an image change from `workspace.yaml`), delete; a base workspace has no VM and its chat offers a fork; one preview per workspace; `queued_for_capacity` when no slot is free.
- **Rollout:** a control-plane deploy with an old host connected keeps that host serving; a rollout drain nudges, interrupts at the deadline, flushes, and the resumed workspace continues its turn on the new image.
- **Governance:** draft 6's list, plus a local merge applied, then the target fast-forwarded by its owner, then recovery reports `applied` from the receipt ref; a rewritten target with no receipt is `failed`.
- **Backup (self-hosted phase):** a workspace deleted or rewritten during a backup is either wholly in or wholly out of the restored installation; every restored receipt resolves to retained objects.
- **Preview and terminal:** the relay reaches a port opened by an agent; a second preview start replaces the first; ticket scope is one workspace; a terminal survives editor navigation.

---

## 16. Deployment targets: the GCP SaaS and self-hosted installations

### 16.1 GCP SaaS and the QA environment

The QA project `g5-qa-1ccc` is an isolated copy of the platform (own VPC, Cloud SQL, Redis, Filestore, KMS, Secret Manager, Artifact Registry, GKE cluster, delegated DNS) built by the Terraform roots under `infra/qa/` and documented in `infra/qa/README.md`. Cloud Build runs the PR lanes (`g5-ci-rust`, `g5-ci-e2e`) on every pull request and the deploy and worker-image lanes on pushes to `main`; the migration checksum guard runs in both PR lanes; deployment pauses are a Terraform variable; the one-time database reset is a runbook. CI is informational until the lanes are reliable enough to gate; the locally reproduced validation ladder is the merge evidence until then. Production and staging follow the same roots with their own inputs.

### 16.2 Self-hosted installations: what is retained

The self-hosted control plane on pre-provisioned VMs is a retained deployment target. The bundle under `infra/self-hosted/` (installer, Compose stack with `postgres`, `redis`, `db-migrate`, `g5-api`, `g5-api-indexer`, `edge`, and the worker service, the `just` operator runner with `setup`, `up`, `logs`, `backup`, `restore`, `upgrade`, `down`, the license and registry-token flow, TLS overlay, starter repositories), the worker-host installer (`install.sh --worker-host`, `g5-workerhost.sh`, which runs the host container with `/dev/kvm` and `G5D_GUEST_HOST_ALIASES` / `G5D_GIT_HTTP_ALLOW_HOSTS`), the Azure canary workflow (a VNet with a private DNS zone so the worker VM reaches the control plane at its private address, a nested-virtualization worker VM, backup and restore, down and up, upgrade), and the bundle contract tests are all on `main` and byte-for-byte the original G5 assets apart from the GitHub-only login change. `SELF_HOSTED=true` and `ENABLE_CLOUD_FEATURES=false` keep the SaaS-only routes off. None of this changes for the MVP, which runs on the SaaS; the self-hosted phase brings it to the workspace-VM model.

### 16.3 Gaps against this design

| Area | State on `main` | Gap | Phase |
|---|---|---|---|
| Worker host on a pre-provisioned VM | `g5-workerhost.sh` starts a per-task worker container with poll and claim | The container must run `g5d host` with the `ctl` tunnel, slots, retained roots and the host proxies; the registration token flow stays; the Compose stack's in-box worker service becomes a `g5d-host` service with `/dev/kvm`; process isolation mode (`G5D_ISOLATION_MODE=process`), which the installation guide allows with an acknowledgement, has no equivalent in the workspace-VM model and needs a decision (drop it, or run the dispatcher as a host process for KVM-less hosts) | P9.1 |
| Internal routing from hosts to the control plane inside the customer VPC | DNS-based: the host resolves the control-plane FQDN through the private zone; `G5D_GUEST_HOST_ALIASES=<fqdn>=<private ip>` seeds the guest; `G5D_GIT_HTTP_ALLOW_HOSTS` | Retained as host configuration: the host seeds the guest's `/etc/hosts` with the aliases beside `g5-llm` and the git proxy rewrites the internal git host; `VmSpec` never carries host addresses; the canary continues to prove the private path | P9.1 |
| Microsoft Entra platform login | Removed with the other non-GitHub logins (P8.3, −2.4K lines); the original adapter (OIDC userinfo with id-token fallback, tenant, default org role map, `G5_MICROSOFT_*` settings) is in the reference clone | Re-add the Entra adapter behind configuration, off in the SaaS and available to self-hosted installations; env template, installer keys, login catalog entry, canary smoke (`entra-login-smoke.js` from the reference clone) | P9.2 |
| Backup and restore | `just backup` dumps Postgres and the configuration; it never copies `/var/lib/g5/storage/repos`, the internal git store; `just restore` restores the dump only; writers keep running during the backup | Files are in git in this design, so a backup without the store is not a backup. `just backup` takes a barrier (the API refuses ref writes, receipts, merges, forks and deletions for the window, receive-pack answers 503 with retry-after, agents retry), dumps Postgres, snapshots the git store (`git bundle --all` per repository or a filesystem snapshot), releases the barrier and records both in the manifest; `just restore` restores both from one backup, refuses mismatched pairs and runs projection convergence; `just upgrade` drains workspaces through the stop sequence before it migrates | P9.3 |
| Upgrades | `just upgrade` scales the worker service to zero before migrating | Drain through the stop sequence so unpushed work is flushed; control-plane-first order for the host protocol | P9.3 |
| Installation guide | Describes per-task workers, task leasing, CRDT state, commands that do not exist (`g5 clone workspace`, `g5 sync`, `g5d worker register|run|list`) and an inverted api-url resolution order | Rewrite for the workspace-VM model: host registration, slots and sizing per workspace VM, flush and loss semantics, the backup barrier | P9.1 |
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
- The exact MCP tool-call timeout values above the runner wait and the answer-polling interval; zstd level and caps for session files.
- The `configs` shape for feature flags; whether `projection_state` is one table or a column set on each projection.
- Retained-root retention as a fixed value or an org setting (default 24 h); one version number or two for the host protocol and `VmSpec` (default: two).
- Whether the passt door can be mapped to a per-VM host address so the proxy secret becomes defense in depth only.
- Crate consolidation targets; the list of `.g5/` runtime subdirectories in the excludes file.

### 17.2 Deferred items

- **Interactive Codex tool-permission approvals** (App Server protocol bounded to one turn; needs a verified prototype).
- **Session-resident Claude process** behind `agent.turn` (steering, in-band interrupt, live model and permission changes), with the gates of §8.7; the Claude Agent SDK stays unused.
- **Document-only CRDT** for real-time co-editing behind the Save API.
- **Org-level read-only provider grants** for private git dependencies, and the `gh` credential path with a per-VM CA.
- **Per-user task secrets, secret files, secrets-manager integration** beyond environment variables.
- **Multi-replica API**: moving the tunnel registry and event bus to Redis with a centralized lease.
- **A CI service outside workspaces** (a read-only clone of the workspace answering API calls at a git version).
- **Snapshot of the tree to object storage** as a mitigation for flush failure.
- **Lightweight disposable forks and parent/child navigation.**

### 17.3 Open questions

- Confirm the reading of the ontology decision in §2 (developed without the gate and outside the MVP acceptance, code retained).
- Whether idle reclamation stays a G5-decided teardown (this draft keeps it, org-configurable) and its default.
- Whether the providers' native ask-user surfaces can replace the MCP `ask_user` tool; whether a mid-turn nudge can reach a running CLI turn through a hook.
- Agent-initiated workspace creation as a way to attach compute (not offered here).
- Share links (read-only by default with a Fork button) and hand-off of a workspace to another user (UX).

---

## 18. Decisions register

Sources: the accepted draft 6 (`v6 §n`, whose own register traces the earlier questions and reviews), the user's revision note (`note n`), the companion notes, the team's aligned decisions (`team`), and the design reviews (`Gn` from the draft 6 review, `Rn` from the draft 7 review, `review` for a finding raised in the review round before this draft and carried by its content).

| Decision | Source | Section |
|---|---|---|
| Ontology tooling retained and decoupled; codebase developed without its own grounding gate; ontology outside MVP acceptance (reading to confirm) | v6 §2; team | §2 |
| Targets: GCP SaaS with the QA project as CI/CD, self-hosted pre-provisioned VMs, macOS dev-only; customer images dropped | v6 §2; note 8 | §2, §16 |
| One repository for the MVP; three capability owners plus Git and governance; ownership follows capability | note 4; team | §4.1, §13 |
| No periodic checkpoint; agents commit and push; G5 flushes only at graceful teardown, fork and explicit request; ungraceful loss accepted | note 1; team; workspace-vm-api §5 | §5.4 |
| Flush publishes through immutable per-flush refs and an API-verified receipt; consumers read the last complete receipt | review | §5.4, §5.5 |
| Live flushes require quiescence or an explicit interrupt; concurrent terminals and previews are recorded | review | §5.4 |
| PR publication commits through `workspace.changes`, never a flush; `external_pr` materialization of the VM note is folded into that operation | review; workspace-vm-api §5 | §10.2 |
| Flush failure: user stop aborts, deadline-bound teardown proceeds and records the loss, idle stop cancels | team; note 1 | §5.4, §5.8 |
| Fork flushes the source first and carries every committable artifact including dirty work | team | §5.8 |
| Teardown reasons: user, idle, drain, rollout, credential, capacity; nudge before deadline-bound teardowns | note 1; team | §5.8 |
| Five workspace actions including restart; image from `workspace.yaml` resolved per attach | team | §5.8, §6.3 |
| Base workspaces have no VM and are read-only; running from base is fork and preview | team | §5.2 |
| Multi-writer workspaces; any member with write may edit, chat and use the terminal | team; v6 Q26 | §5.1, §8.1 |
| The dispatcher exposes the Workspace Runtime API; `proc.*`, `git.*`, `agent.run_turn`, `preview.*`, `checkpoint.*` absent; platform code runs only as declared modules | workspace-vm-api §2 to §7; team | §6.6, §6.7 |
| The Workspace Runtime facade is the only client of VM channels; Agents calls it | workspace-vm-api §1; agent-api-surface §4 | §6.6 |
| Host protocol: versioned `ctl` stream, journaled `vm.event`, idempotent commands with deadlines, `VmSpec`, renewable host credential, `unreachable` state, retained roots, limits, operator API | worker-host-api §2 to §9 | §6.1 to §6.3 |
| Lease: host deadline armed from the request's monotonic start; boot lease covers the command deadline; commands after their deadline refused | review | §6.4 |
| Host-side proxies: model traffic per session with a per-VM secret; git and platform credentials injected on egress; no credential in a provider process; the token only in the dispatcher and the proxy; session-scoped provider grants for multi-user workspaces; grants updated live | worker-host-api §6; agent-api-surface §6.1; team | §6.5 |
| Repository and task secrets as environment variables, re-read per process launch | team | §6.5 |
| One preview per workspace; a rebuild may stomp it | team | §6.8 |
| No maximum host lifetime; rotation is drain and replace; module-level versioning with feature flags; control plane deploys first; hosts at most one version stale; rollout by drain, flush, recreate with a continue successor | team; worker-host-api §3.0, §8 | §6.10 |
| Agents is a bounded context owning tasks, chats, inbox, questions, session files, adapters, usage; `agent.turn` is its only VM-hosted piece; C1/C2/C3 categories; provider-neutral event and outcome schemas | agent-api-surface §2 to §12 | §7, §8.3 |
| One CLI process per turn for both providers; the Claude Agent SDK is not used because it wraps the same CLI protocol; a resident process per session is a later option behind the same contract with gates | team; v6 B4; agent-api-surface §1, §14 | §8.3, §8.7 |
| Inbox: queued, editable, withdrawable messages; reservation and delivery states; no steering in the MVP | agent-api-surface §8 | §8.1 |
| Questions have a runner wait and an answer expiry; a suspended turn's answer wakes a successor; the VM may stop while a question waits | team; agent-api-surface §8.2 | §8.4 |
| Loss settles unconfirmed starts; same-identity replay only on the current incarnation | G1 | §7.5 |
| Durable cancellation dispositions retained for the incarnation, independent of journal pruning | G2, review | §7.5 |
| Successor turns only after interruption, suspension or loss; never after cancel; shared lock | G3 | §7.2 |
| Event keys and watermarks carry the attempt | G4 | §7.5 |
| Local merge applied in one ref transaction with a receipt ref; recovery recognizes application from the receipt | G5 | §10.6 |
| Save ensures the runtime before the association query | review | §9.2 |
| `delivered` follows the adapter's `input.accepted`, not the module's `started` disposition; an execution without acceptance releases the reservation | R1 | §8.1, §8.2 |
| Question suspension is the supervisor's conditional decision with reaping and one outcome; a tool error never ends a turn | R2 | §7.2, §8.4 |
| Live flushes close and drain file-mutation admission and hold it through capture | R3 | §5.4 |
| `vm.stop` is fenced by host ownership only; release follows the host's exit | R4 | §5.8, §6.2, §6.4 |
| A warm root is a cache of ignored files and objects; every attach materializes the receipt | R5 | §5.5 |
| Baseline frozen; forward migrations validated as fresh install and upgrade; no baseline rewrite; databases persist | note 6 | §12 |
| Governance built the week after the MVP; approvals written only through the API; approval state on the PR; external CI postponed | team; v6 §10 | §10, §13 |
| Self-hosted: worker host on `g5d host`, VPC-internal routing by host configuration, Entra login re-added for self-hosted, backup and restore of Postgres and git under one barrier, guide rewrite, canary update; later phase | note 8; review | §16 |
| Chat history in Postgres via the API with no VM; sub-agents untracked; agents do not create workspaces | v6 B8, Q8; team | §8.1, §11.1 |
| Unchanged from draft 6: receive-pack policy and owner rewrites, provider integration, association check, projections converge from OIDs, merge operation shape, approvals pinned to source OIDs, session files in Postgres, fake CLI binaries, cancel signals, idle timeout default, approved removals | v6 §18 | throughout |
