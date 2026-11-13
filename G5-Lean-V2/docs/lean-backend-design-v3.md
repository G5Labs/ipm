# G5-Lean backend re-architecture — design (draft 3)

Status: draft 3, 2026-09-13, candidate for acceptance. Supersedes
[lean-backend-design-v2.md](lean-backend-design-v2.md) (draft 2) and
[lean-backend-design-v1.md](lean-backend-design-v1.md) (draft 1); both are kept
unchanged for history. Nothing in this document is implemented.

All questions from round 1
([lean-backend-open-questions-v1.md](lean-backend-open-questions-v1.md)) and round 2
([lean-backend-open-questions-v2.md](lean-backend-open-questions-v2.md)) are
answered and folded in. This draft has no open questions; the small choices
left for implementation time are listed in §17, and §18 is a register that
traces every decision to the question that settled it. Evidence for
statements about "today" is in [docs/research/](research/README.md).

Deployment constraint: G5-Lean ships to a new environment. No compatibility
with existing G5 data is required; the schema is designed from scratch.

---

## 0. What changed since draft 2

| Area | Draft 2 | Draft 3 |
|---|---|---|
| Deployment-gate removals | Assumed approved | Approved (B1): internal issue tracking, the IDE plugin mockup, the hidden notification-delivery settings UI, takeover remnants, and Google/GitLab/Microsoft platform login are removed. Platform login is GitHub only. `g5d issue`, the four issue MCP tools, and the issue and hotlist tables are gone from every list. |
| Workspace lifecycle | `status active|archived`, an archive action | No archive (B10). A workspace has exactly four actions: **create** (git fork, then attach a VM), **stop** (checkpoint commit and push, then stop the VM), **resume** (attach a VM, restore files from the branch and checkpoint), **delete** (as today). Idle stop is the same operation as stop. (§5.7) |
| Chat history | "Rendered from Postgres as today" stated in passing | Stated as a requirement (B8): chat messages and the agent timeline live in Postgres and the browser reads them through the API with no VM and no worker involved, exactly as today. `chat_session_files` is only for provider resume. (§8.1) |
| Provider runner | Rust runner assumed | Confirmed (B4): one Rust runner, both Node sidecars deleted; Node stays on the runtime mount only if the pinned Codex CLI needs it. |
| Claude CLI pin | Bump assumed | Confirmed (B2): bump `scripts/claude-code-cli-version` at the start of Phase 3 to the newest tested release and record the verified flag set. |
| Tests | Fake CLI binaries assumed | Confirmed (B3): the `fake` provider value is gone; tests use fake `claude` and `codex` binaries replaying recorded frames. |
| Defaults | Assumed | Confirmed: non-chat tasks run with `bypassPermissions` / `approval_policy=never` (B5); checkpoint every 10 minutes when the tree changed plus on every stop (B6); cancel is SIGINT then SIGKILL after 15 s (B7); multi-task notice only (B9). |
| Assumption tags | `[A#]`, `[B#]` | Removed. Every statement is a decision. |
| Checkout | Re-checked 2026-09-13 | No code, infra, or asset file changed since draft 2; sizes identical. |

---

## 1. Summary

G5 today runs one microVM per task attempt, keeps uncommitted edits in a
server-hosted CRDT, owns branch heads in Postgres with git refs as
projections, keeps chat VMs and a Claude SDK process alive between turns
through a "sticky runtime" protocol, and layers approvals, merge runs, and
remote-tracking sync on top. The target is a developer-laptop model with one
difference: the VM is not as persistent as a laptop. It is terminated when
idle and re-provisioned on request; before termination the workspace's changes
are auto-committed to a checkpoint ref and pushed to internal git, and that is
what the next VM restores.

| # | Proposal | Section | What it deletes |
|---|---|---|---|
| 1 | One VM per workspace; tasks are local processes launched by a dispatcher over RPC | §6, §7 | per-task VM boot, materializer, host poll/claim/ack, golden images, capacity bookkeeping |
| 2 | Remove the CRDT layer | §9 | `g5-crdt`, both CRDT peers, drafts, task-commit protocol, yjs in the UI |
| 3 | Internal Git as persistence; checkpoint before stop; restore from git | §5 | head-operation registry, staging refs, advance notifier/consumer, Postgres-owned heads |
| 4 | Native git for agents; far fewer G5D commands | §11 | ~20K lines of CRDT-bound CLI, 16K lines of dead internal tools |
| 5 | Lightweight workspace↔branch check | §5.6 | git blocklists, rebase pause files, authority-scope typing |
| 6 | Governance at the git boundary | §10 | 11-table approval ledger, job-merge runs, PR provider lifecycle, mutation policy |
| 7 | Explicit-Save editing, readable while the VM sleeps | §9 | draft sessions, presence, CRDT-anchored comments |
| 8 | Shared chats, no forced fork, no process while idle | §8 | sticky runtimes (27 migrations), worker pins, SDK sidecar, session-store proxy, context links |
| 9 | Preview on the workspace VM | §6.4 | preview job type, sessions table, reaper, chat-attach machinery |
| new | Claude Code and Codex only; no Claude Agent SDK; one CLI process per turn | §8 | the 46K-line bundled SDK sidecar, `claude_session_store_entries`, `naive_react` everywhere |
| new | Approved deployment-gate removals | §3.2 | issue tracking, IDE mockup, hidden settings UI, takeover remnants, non-GitHub platform login |

Estimated effect (file sizes from the survey, not a measured diff): on the
order of 250K to 300K of the ~1.1M lines of Rust plus the 46K-line generated
sidecar bundle are deleted or replaced by much smaller code; 118 tables become
about 35; 292 migrations become one baseline.

---

## 2. Goals, constraints, non-goals

**Goals.** Simplest possible backend; easy to keep stable; the nine proposals;
laptop-like developer experience with a first-class workspace that maps 1-1 to
a git branch; UI like GitHub Codespaces.

**Constraints.**
- Incremental: every phase in §13 leaves a buildable, testable product with
  Cloud Build green.
- Fresh deployment: no data compatibility, no historical-value readers, no
  backfills.
- Providers: Claude Code and Codex, pinned CLIs (`scripts/claude-code-cli-version`,
  `scripts/codex-cli-version`). No other agent engine. No Claude Agent SDK. No
  long-lived agent process between turns.
- Platform login: GitHub only. Repository providers: GitHub and GitLab, and
  local-only repos.
- Deployment targets: GCP SaaS (GKE control plane, GCE worker hosts),
  self-hosted Linux hosts including the Azure canary lane, macOS hosts for
  development only. Customer-specific worker images are dropped.
- Retained features: ontology and grounding tooling is kept but decoupled
  from git and CRDT; everything else is retained except the removals in §3.2.

**Non-goals for this iteration.** Real-time co-editing or presence (a
document-only CRDT can come later behind the Save API); hardening beyond
today's microVM boundary and the credential rules in §6.3 and §11.4; new cloud
providers; tracking of provider subagents (the provider process is the unit;
G5 never looks inside it).

---

## 3. Where the complexity is today

### 3.1 Cut list

Sizes are line counts from the survey (unchanged at re-check) and include
inline tests.

| Subsystem | What it does today | Size | Disposition |
|---|---|---|---|
| CRDT layer (`g5-crdt`, API and worker CRDT peers, `draft_service`, drafts routes, frontend `lib/crdt.ts` and editor bindings) | One Yjs document per draft session hosted in the API, persisted in Postgres, synced to browser and worker; agents' writes scanned into it every 2 s | ~47K backend + ~12K frontend, threaded through finalizer, sticky runtime, task service, editors | Remove (§9) |
| Head-operation "authority" (`workspace_head_operations` + 6 tables, finalizer, advance notifier/consumer, staging refs, projection reapers) | Postgres owns every branch head; two-phase staged pushes; refs re-projected | ~15K plus ~20 migrations; `workspace_service` (16.5K) threaded | Remove; git refs authoritative (§5.3) |
| Remote tracking (`branch_integration_*`, sync runs, pair leases, push modes, auto-pull, make-remote merge routing, remote-tracking merge runs) | Mirror↔GitHub reconciliation in the API with the creator's token | ~9K plus 8 migrations | Remove; fetch protected branches, publish on explicit action (§5.5) |
| Sticky runtimes (`chat_sticky_runtimes`, `sticky_runtime.rs`, runtime keepers, parked runtimes, `chat_worker_loss`, teardown generations, release proofs, discard fence, transcript mirrors and proxy) | Keeps a chat's per-task VM and SDK process alive between turns | ~8K service + ~5K keeper + supervisor and task-service branches, 27 migrations | Remove (§8) |
| Claude Agent SDK sidecar (`backend/g5d/sidecars/claude`: 2.3K source, 46K bundled, fake fixtures, session store adapter, wake hooks) and the 23K-line `claude_code.rs` adapter; the Codex Node sidecar (1.7K) and 15K-line `codex.rs` | Drives Claude through the SDK `query()` with a session store mirrored to Postgres; drives Codex through a Node wrapper around `codex exec --json` | ~90K including the bundle | Replace with one per-turn Rust runner (§8.3) |
| Per-task execution (supervisor poll/claim/ack, materializer, repo cache, rootfs COW, handoff, capacity bookkeeping) | One libkrun VM per task attempt with a fresh reflink checkout | supervisor 13K production, materializer + cache 9K | Replace with VM-per-workspace lifecycle and dispatcher (§6) |
| Golden images (builder/consumer/maintenance, telemetry, triggers) | Pre-built cache disks per workspace | ~10K plus 5 migrations | Remove; caches live on the VM disk (§6.5) |
| Task lifecycle (33-command kernel, 44 arcs, 12 task statuses, leases, claims, attempts fences, recovery statuses, reaper, question windows, completion constraints) | State machine for a shared leased worker pool | ~120K including `task_service.rs` (33K) and `routes/internal.rs` (15K) | Replace with a 6-state dispatcher-driven model (§7) |
| Governance (approval ledger: 11 tables, ~8K backend, ~5K frontend; `job_merge_service` 9K; coordinated publication 3K; PR provider lifecycle 2.2K; mutation policy) | Records approvals that nothing reads at merge time; three merge paths | ~25K backend | Replace with protected refs + `pull_requests` + `approval_rules` + `approval_decisions` (§10) |
| Agent surface (internal tool registry 16K dead; CRDT-bound CLI 20K; session-store proxy 1.2K; MCP preflight) | Tools for the retired native engine and CLI wrappers that mediate git through CRDT and authority | ~37K | Reduce to platform operations (§11) |
| Preview as a job (`job_preview_sessions`, boot generation, reaper, `preview_vms` registry, chat-attach listener/token/drive) | Separate VM per preview from a committed ref | relay 3.5K kept, ~6K removed | Run in the workspace VM (§6.4) |
| Fleet (`cloud/` bring-up, reconciler, rotation, stale-host detector, orphan sightings, 25-column `worker_hosts`, 24 h host JWT) | Shared pool of task hosts | ~6K plus 15 migrations | Simplify to VM slots on hosts (§6.6) |
| Chat context links | Cross-session context injection | 4.7K + 1K frontend, 2 tables | Remove |
| Internal issue tracking (`routes/issues.rs`, `issue_service.rs` 124K bytes, attachments, inference, four MCP tools, `IssuesPage`, hotlists) | Live-disabled ticket module | ~10K backend + frontend, 13 tables | Remove (approved) |
| `naive_react` remnants | Enum variants, DB defaults, normalization in chat routes, frontend mapping, prompts, tests | 39 files | Delete completely (§8.6) |
| Schema | 118 tables, 292 migrations | | One baseline, ~35 tables (§12) |

Already removed before this design: the native ReAct engine, MicrovmAgent
mode, WSL2 backend, old daemon HTTP server, old scheduler family,
disconnected frontend files (dead-code batches 1 to 4).

### 3.2 Checkout re-check and approved removals

No code, infra, or asset file changed between draft 2 and draft 3
(1,107,427 lines of Rust, 293,902 of TypeScript, 118 tables, 292 migrations).

Seven deployment gates were retired to standard behavior before draft 2
(`docs/standard-features-retirement.md`); five are moot under this design
(durable task CRDT, zero-slot polling, strict lifecycle identity, discard
fencing, golden scheduler) and two are carried in simplified form (fork/chat
carry as a message-row copy, §8.5; Make Remote as "connect a local repo to a
provider", §5.5).

The removals proposed in `docs/deployment-gate-audit.md` are approved for
G5-Lean and are part of this design:

| Removal | Effect |
|---|---|
| Internal issue tracking | `g5d issue *`, MCP `file_issue` / `search_issues` / `add_issue_comment` / `update_issue`, `routes/issues.rs`, the internal issue endpoints, `IssueService`, attachments and inference, `IssuesPage` and issue components, issue and hotlist tables, `G5_ISSUE_TRACKING_ENABLED`. Generic comments, notifications, and platform bug reporting stay. |
| IDE plugin mockup | `/ide` route, `IDEPluginView`, mockup fixtures. |
| Hidden notification-delivery settings UI | dev-mode toggles and delivery-channel form in `WorkspaceSettingsContent`; persisted fields dropped after tracing consumers. The notification bell, mentions, toasts, and Slack reporting stay. |
| Takeover / intervene remnants | unreachable banners, `onTakeOver` plumbing, exclusive fixtures. |
| Google, GitLab, Microsoft platform login | adapters, callback and config mappings, login catalog entries, secrets. GitHub login stays. SCM repository connections (GitHub and GitLab repos) and Claude/Codex provider-account OAuth are separate and stay. |

---

## 4. Target architecture

### 4.1 Components

**g5-api (control plane).** Postgres-backed. Owns identity, orgs, workspace
registry, chat and task records, provider session files, PRs and approvals,
projections (docs, ontology, search), the internal git server, the command
channel to hosts, the browser WebSocket, and the preview and terminal relays.

**Internal Git store.** One bare repo per connected repository under the
existing `RepoStorage` abstraction (local disk for self-hosted, NFS Filestore
for SaaS). Holds mirrored protected branches, every workspace branch, and one
checkpoint ref per workspace and repo. Git refs are authoritative.

**Worker host (`g5d host`).** A Linux machine with KVM (GCE VM, self-hosted
Docker with `/dev/kvm`, or a macOS dev host with HVF). Registers, dials an
outbound tunnel, boots and stops equal-size workspace VMs on command, relays
multiplexed streams, and runs the LLM proxy. No task logic.

**Workspace VM.** One libkrun microVM per workspace, attached on create or
resume and terminated on stop or idle. Its disk is a cache. It runs the
**workspace dispatcher** (`g5d dispatch`, name provisional): restores the
workspace from git, serves an RPC surface, supervises processes, checkpoints
to git. Every task, chat turn, preview, and terminal is a process inside this
VM, in the same folder, on the workspace branch.

**Frontend.** The existing React shell with a Codespaces-like center: file
tree and editors with explicit Save, a Changes panel from `git status`, a
terminal, Ports and Preview, chat, tasks, PRs and approvals.

**Providers.** The pinned `claude` and `codex` CLIs on the VM's runtime mount.
One process per turn. LLM traffic goes through the host-side proxy; no
provider keys in the VM.

### 4.2 Diagram

```
 browser ──HTTPS/WS──► g5-api ◄──HTTPS──► GitHub / GitLab (protected branches in, PR publish out)
                        │ ▲
      Postgres ◄────────┘ │  internal git: /git/<repo_id>  (smart HTTP, policy in receive-pack)
   (metadata, chat       │
    messages + timeline, │
    session files)       │
            outbound tunnel (WS + stream mux; host dials the API)
                          │
                    g5d host (per machine, N equal VM slots)
                     ├─ libkrun boot/stop, OCI rootfs templates, bare-repo cache
                     ├─ LLM proxy (provider keys stay here)
                     └─ per VM: TSI port map, mux relay
                          │
                 workspace VM  (one per workspace, /work = checkout on the workspace branch)
                     └─ g5d dispatch
                          ├─ RPC: fs.*, git.*, proc.*, agent.*, pty.*, ports.*, preview.*, checkpoint.*
                          ├─ chat turn  ─► claude -p --resume <id>  |  codex exec resume <id>   (exits at turn end)
                          ├─ agent task ─► same runner, task prompt
                          ├─ preview    ─► user's dev server on port N
                          └─ terminal   ─► PTY shell
```

### 4.3 Invariants

1. **Two sources of truth.** Files: git in the internal store. Metadata, chat
   messages and timeline, and provider session files: Postgres. The VM disk is
   a cache.
2. **One workspace, one branch, one folder, one VM, one dispatcher.** The
   `workspace_vms` row with a host lease is the only custody record.
3. **One write path for files.** All file writes happen inside the VM: agents
   natively, the UI through the dispatcher. The API never writes a working tree.
4. **Branch protection lives in receive-pack.** Protected refs accept updates
   only from the API's merge operation after approval checks.
5. **A task is a process; a turn is a process.** State derives from the process
   and its exit. No leases, claims, or per-task heartbeats. G5 does not track
   what the provider does internally; the provider exits only when its work is
   done.
6. **Association check before UI-triggered operations.** The dispatcher
   verifies every member repo has the workspace branch checked out before
   Save, Run, Preview, Commit, Push, or PR actions; otherwise
   `WORKSPACE_BRANCH_MISMATCH`.

---

## 5. Workspace and Git

### 5.1 Data model

```
repos(id, org_id, provider NULL|github|gitlab, provider_full_name, git_url,
      default_branch, protected_branches text[], created_at)

workspaces(id, org_id, name, slug, branch_name, base_branch, is_base,
           visibility org|invite_only, created_by, idle_timeout_min,
           created_at, deleted_at)

workspace_repos(workspace_id, repo_id, root_path, role code|docs,
                head_sha_cache, PRIMARY KEY (workspace_id, repo_id))

workspace_vms(workspace_id PK, host_id, vm_id,
              status stopped|starting|running|stopping|failed,
              lease_expires_at, started_at, last_activity_at,
              last_checkpoint_at, last_error)
```

`workspace_repos` replaces `workspace_branches` and `workspace_branch_repos`
and carries no authoritative head; `head_sha_cache` is refreshed from push
events for list views. A workspace spans N repos (code repos plus the org docs
repo) with the same branch name in each; pushes are per repo, like a laptop
with several clones. There are no job or task branches, and no workspace
status column: a workspace either exists or is deleted, and its VM state
lives in `workspace_vms`.

### 5.2 Branch naming and identities

- Workspace branch: `<slug>-<8 hex of workspace id>` in every member repo.
- Base workspace: `branch_name` is the repo's default branch, which is in
  `protected_branches`. It gets a VM like any other so users can browse and
  chat on `main`, but nothing can push to it; changes require a fork.
- Identities seen by receive-pack: **user** (workspace member with write, JWT
  as today); **workspace VM** (`G5D_WORKSPACE_TOKEN`, a JWT scoped to one
  workspace id, minted at VM start, TTL = idle timeout plus margin, renewable
  by the dispatcher); **API server identity** (merges and forks, in-process).

### 5.3 Receive-pack policy (the git boundary)

`routes/git.rs` keeps its shape: parse ref updates, decide in Rust, spawn
`git receive-pack --stateless-rpc`. The staging-ref rules are replaced by:

| Ref | user (member, write) | workspace VM token | API server identity |
|---|---|---|---|
| `refs/heads/<protected>` | deny | deny | merge operation only (§10.3) |
| `refs/heads/<workspace branch>` | allow FF; allow non-FF with compare-and-swap (old sha must match the current ref) | same | fork creation only |
| `refs/heads/<other workspace's branch>` | deny | deny | n/a |
| `refs/g5/checkpoints/<workspace id>` | deny | allow force-update | n/a |
| delete of a workspace branch | deny | deny | workspace deletion |
| tags, other namespaces | deny | deny | n/a |

Non-fast-forward updates on the workspace branch are required because agents
rebase onto `main`; git's own old/new sha check in the update command provides
the compare-and-swap, so `--force-with-lease` works and a bare `--force`
against a moved ref is refused. `git-upload-pack` requires read access as
today. After a successful push the handler publishes `branch.updated
{repo_id, ref, old, new, actor}` on the event bus; projections and the UI
subscribe. This replaces the advance handoff tables.

### 5.4 Checkpoint and restore

The checkpoint is the auto-commit that happens before a VM terminates:
uncommitted changes are committed and pushed to internal git so the next VM
can resume.

**When.** On every stop (explicit, idle, host drain), every 10 minutes while
the VM runs and the tree changed (per-org override), and on demand
(`checkpoint.run`, used by "Save all and stop" and before host rotation).

**What, per code repo.** A commit whose tree is the working tree (tracked and
untracked files honoring `.gitignore`) and whose parent is the current branch
head, created with a temporary index (`GIT_INDEX_FILE=… git add -A; git
write-tree; git commit-tree`) so HEAD and the real index are untouched, and
force-pushed to `refs/g5/checkpoints/<workspace id>`. The message records
branch, HEAD sha, timestamp, and the staged/unstaged path summary. Ignored
files (build outputs, `node_modules`, `target/`) and the docker scratch disk
are cache and are not checkpointed. The workspace branch itself is never
polluted by checkpoints.

**Restore on resume.**
1. Clone or reflink each member repo from the internal store (the host's bare
   cache stays as an optimization) and check out the workspace branch.
2. Fetch the checkpoint ref. If absent, done.
3. If the checkpoint's parent equals the branch head, materialize the checkpoint
   tree into the working tree without moving HEAD (`git restore
   --source=<ckpt> --worktree -- .` plus untracked files), leaving changes
   unstaged.
4. If the branch head moved while the VM was down (a user pushed from a laptop),
   apply the checkpoint as a no-commit cherry-pick; on conflict leave the tree
   clean, keep the ref, and notify: "unsaved changes from the last session could
   not be applied automatically".

**Guarantee.** Work committed and pushed by an agent is never lost. Unpushed
work is lost only if the VM dies between two checkpoints, bounded by the
10-minute interval. Provider session files are not on the VM's critical path:
they are in Postgres (§8.4).

### 5.5 Provider integration (GitHub / GitLab)

- **Inbound.** For each connected repo the API fetches the protected branches
  from the provider into the mirror on webhook and on a periodic poll (default
  5 minutes). Workspace branches are never fetched from the provider.
- **Outbound.** The API pushes a workspace branch to the provider only on
  explicit actions: PR publish (§10.2) and "Publish branch", using the acting
  user's stored token and `--force-with-lease`, same branch name.
- **Updating a workspace from upstream** is a git operation inside the VM:
  `git fetch origin main && git rebase origin/main` (origin is the internal
  mirror). The UI's "Update from main" button runs it through
  `git.fetch_rebase`; agents do it natively.
- **Connecting a local repo to a provider** (today's Make Remote adopt/bind)
  updates `repos.provider/git_url`, performs the first publish, and enables
  inbound mirroring of the protected branches. The divergent-ref merge routing
  of Make Remote is not carried over.
- **Local-only repos** keep working: a bare repo in the store with no sync.
- **Provider credentials never enter the VM.** Private git dependencies need an
  org-level read-only token on the host proxy or are unsupported.

Removed: `tracking_mode`, `push_mode`, `auto_pull_enabled`, `base_auto_pull`,
sync runs, pair leases, advisory pinger, remote-tracking merge runs, the
branch-integration worker and scheduler.

### 5.6 The workspace↔branch association check

`git.assert_branch(workspace)` compares each member repo's symbolic `HEAD` to
`workspaces.branch_name`. The API calls it before every UI-triggered mutation.
Failure returns:

```
409 WORKSPACE_BRANCH_MISMATCH
{ repo: "backend", expected: "fix-login-3fa9c2d1", actual: "main", detached: false }
```

The UI shows the mismatch in the status strip with one action, "Switch back"
(`git switch <branch>` if clean; otherwise stash or commit first). Nothing
prevents an agent or a terminal user from running `git checkout`; the profile
text tells agents to stay on the branch and the check catches drift at the next
UI operation. This replaces `g5-scm/src/safety.rs` blocklists, the rebase pause
file, and the authority-scope typing of branch rows.

### 5.7 Workspace actions

A workspace has exactly four user-visible actions. There is no archive.

| Action | Steps | Result |
|---|---|---|
| **create** | fork the source branch head into `<slug>-<8hex>` in every member repo (server identity); insert `workspaces` and `workspace_repos`; optionally carry chats (§8.5); attach a VM (`workspace_vms` → `starting`); dispatcher restores (§5.4) | running workspace on a fresh branch |
| **stop** | checkpoint every repo (commit and push to the checkpoint ref); stop the VM; release the host slot (`workspace_vms` → `stopped`) | branch, checkpoint, chats, session files, and settings remain; reads come from git (§9.1) |
| **resume** | attach a VM on any host with a free slot; restore files from the branch and checkpoint (§5.4); session files restore lazily on the first chat turn (§8.4) | running workspace with the last checkpointed state |
| **delete** | stop if running; delete the workspace branch and checkpoint ref in every repo; delete chats, session files, tasks, and rows, as today | gone |

Idle timeout (default 30 minutes, per-org default with workspace override)
performs **stop**. Any wake trigger (chat message, task run, Save, preview
open, terminal open, explicit Resume) performs **resume**. Host drain performs
stop for every VM on the host. Activity that keeps a VM awake: running tasks,
an open terminal, preview traffic, file RPC. An open browser tab and a
`waiting` task (§7.2) do not.

---

## 6. Workspace VM and dispatcher

### 6.1 VM lifecycle

```
stopped ──resume──► starting ──restored──► running ──stop/idle──► stopping ──checkpointed──► stopped
                      │                       │
                      └──boot failure─────────┴──host lost──► failed ──next resume──► starting
```

| Transition | Actor | Details |
|---|---|---|
| resume | API, on any wake trigger or explicit Resume | pick a host with a free slot; insert/update `workspace_vms` with a lease; send `vm.start` |
| restored | dispatcher reports ready | restore done; RPC listening |
| stop / idle | user, idle reconciler, or host drain | send `vm.stop(checkpoint=true)` |
| host lost | reconciler: host heartbeat missing beyond 3 intervals | VM `failed`; running tasks `failed(vm_lost)`; next resume restores from the last checkpoint on another host |

### 6.2 Host daemon (`g5d host`)

Retained: registration and heartbeat, libkrun boot (`vm-exec`), OCI rootfs
templates and COW clone, XFS reflink bare-repo cache, port allocator and VMM
loopback addressing, passt networking, the outbound tunnel and stream
multiplexer, the LLM proxy, boot-time attestation, `G5D_HOST_MAX_MICROVMS` as
the number of equal-size VM slots.

New: a command channel on the tunnel. The API sends `vm.start(workspace_id,
spec)`, `vm.stop(workspace_id, checkpoint)`, `vm.status`, and opens mux streams
tagged `rpc`, `pty`, `http:<port>`, `llm` for a given VM. The host maps each VM
to its TSI port and per-VM token, as preview-attach connects to the guest
listener today.

Removed: poll/claim/ack/nack/observation, per-task `Materializer`, golden
lanes, parked runtimes, discard fence, sticky heartbeat riders, deferred and
superseded assignment bookkeeping, the separate preview registry, per-task
attestation, zero-slot polling.

Placement: any host with a free slot in the org's region, preferring the host
that last ran this workspace (warm bare cache and, when still present, the
VM disk).

### 6.3 Dispatcher (`g5d dispatch`, inside the VM)

Boot: mount `/work`, restore (§5.4), start the RPC listener on the TSI port
with the per-VM token, start the file watcher, report `ready`.

RPC surface (JSON-RPC over the `rpc` mux stream; events are pushed on the same
stream):

| Group | Methods | Used by |
|---|---|---|
| `fs` | `list`, `read`, `stat`, `write(path, content, expected_hash)`, `delete`, `rename`, `watch` → `file.changed` | file tree, editors, Save |
| `git` | `status`, `diff(path?)`, `log`, `assert_branch`, `switch_back`, `commit(message, paths)`, `push`, `fetch_rebase(base)` | Changes panel, Commit/Push, association check |
| `proc` | `spawn(argv, env, cwd)`, `signal`, `list`, `wait`; streams `stdout`, `stderr` | command tasks (bootstrap, custom) |
| `agent` | `run_turn(provider, provider_session_id?, prompt, attachments, options)` → task events; `answer(question_id, payload)` | chat turns, agent tasks, question and approval answers (§8.3) |
| `pty` | `open(cols, rows)`, `write`, `resize`, `close` | terminal |
| `ports` | `list`; events `port.opened`, `port.closed` | Ports panel, preview auto-detect |
| `preview` | `start(profile)`, `stop`, `status` | Preview button |
| `session_files` | `restore(provider, session_id, files[])`, `harvest(provider, session_id)` → changed files | provider session persistence (§8.4) |
| `checkpoint` | `run`, `last` | stop, "Save all and stop" |
| `health` | `ping`, `resources` | reconciler |

Process supervision: the dispatcher keeps an in-memory process table and
reports `task.started`, `task.event`, `task.exited(code, outcome)` to the API.
If the dispatcher crashes inside the VM, the host detects the listener loss
and the API marks that VM's running tasks `failed(vm_lost)` and restarts the
VM (restore is idempotent).

Identity and credentials in the VM: `G5D_WORKSPACE_TOKEN` (internal git push,
`g5d` platform commands), `G5D_API_URL`, the LLM proxy endpoint, the provider
CLIs on `/opt/g5/bin`. No provider tokens, no host token, no per-task token.

Concurrency: every process runs in the same folder on the workspace branch.
There is no worktree, branch, or lock per task. Two agents editing the same
file show up as conflicting changes in `git status`; agents are expected to be
aware of each other (the profile says so), and the UI shows a notice when more
than one task is running. Nothing stronger is enforced.

### 6.4 Preview

`preview.start(profile)` runs the command from `.g5/preview_profiles/<name>.yaml`
or the `preview:` block of `.g5/workspace.yaml` as a supervised process and
records its port. The browser reaches it as today: through the API relay over
the host tunnel, at `/api/v1/workspaces/:id/preview/:port/*`, with the relay
code from `routes/preview.rs` and target resolution keyed by workspace and port
instead of job id; the ticket cookie becomes workspace-scoped; subdomain mode
stays optional. Ports the agent opens on its own appear in the Ports panel
from `ports.list`. Preview processes stop with the VM.

Removed: the `preview` job type, `job_preview_sessions`, boot generations,
`preview_reaper`, the `preview_vms` registry, and the chat-attach listener,
token, and host drive. "Chat attached to a preview" is the default: agent and
app share the VM.

### 6.5 Caches and images

The VM image is the org or workspace `base_image` (OCI), unchanged. Language
caches and the docker scratch disk live on the VM disk; when the host still has
the disk on resume they are warm, otherwise rebuilt. Because the VM is reused
across tasks and turns, the golden-image machinery is replaced by nothing.

### 6.6 Fleet

Hosts are API-provisioned GCE instances, self-hosted Linux machines (including
the Azure canary lane), or macOS dev hosts. A host is a set of equal-size VM
slots (`G5D_HOST_MAX_MICROVMS`, VM CPU and memory from the host configuration
as today), not a task pool. The reconciler (provisioning → ready, orphan sweep)
stays. Rotation becomes "drain: stop every VM, then replace". The 24-hour host
JWT is replaced by a renewable host credential so long-lived VMs do not force
rotation. Org quotas: max concurrently running VMs.

### 6.7 Terminal

A browser terminal (xterm.js) over the `pty` stream, gated by workspace
access. The PTY manager and the multiplexer exist today; the UI panel and the
channel are new.

---

## 7. Task lifecycle

### 7.1 Model

```
tasks(id, org_id, workspace_id, job_id NULL, kind chat_turn|agent|command,
      status queued|running|waiting|succeeded|failed|cancelled,
      spec jsonb,        -- provider, model, permission_mode, prompt or argv, profile, env, retry_on_vm_loss, wall_clock_limit
      outcome jsonb,     -- exit code, summary, commit shas, usage, error
      attempt int, depends_on uuid[], chat_session_id NULL, created_by,
      created_at, started_at, ended_at)

jobs(id, org_id, workspace_id, kind, status (derived), spec jsonb, created_by,
     max_concurrency, created_at, ended_at)          -- optional grouping for DAG runs

task_events(id bigserial, task_id, ts, kind, payload jsonb)   -- today's timeline_entries; the agent timeline

task_questions(id, task_id, kind question|approval|escalation, payload jsonb,
               status open|answered|expired|cancelled, answer jsonb,
               expires_at, answered_by, answered_at)          -- one HITL table

task_attempts(id, task_id, attempt, started_at, ended_at, outcome,
              reason exit_code|provider_error|vm_lost|timeout|cancelled)
```

Job kinds are profiles over the same task machinery (`uplift`, `downlift`,
`sync`, `explainer`, `agent_review`, `custom`). All jobs of a workspace run in
that workspace's VM, in the same folder. A single agent run is a task without
a job.

### 7.2 State machine

```
queued ──dispatch (VM running, deps met, concurrency slot)──► running
running ──process exit 0 / outcome ok──► succeeded
running ──exit ≠ 0 / provider error──► failed
running ──question or approval posted──► waiting ──answered──► running
running | waiting | queued ──cancel──► cancelled
running ──vm lost──► failed(vm_lost)  [re-queued once if spec.retry_on_vm_loss]
```

Six states, seven transitions, one writer: `TaskService::apply(event)`, with
events from the dispatcher (`started`, `exited`, `question`), from users
(`cancel`, `answer`), and from the VM reconciler (`vm_lost`).

Rules.
- `waiting` does not hold the VM awake. Answering resumes the VM if needed and
  resumes the process if it is still alive; otherwise it re-spawns the provider
  with `--resume` and the answer as the next prompt.
- Cancel sends SIGINT to the provider process, then SIGKILL after 15 seconds;
  the turn's partial assistant output is kept and marked `stopped`.
- No automatic retry on agent-reported failure. Human retry creates a new task.
- Job status is a pure function of child states; cancel cascades; there is no
  pause and no rollback.
- One reconciliation loop every 30 s: `running` tasks whose VM is not running
  become `failed(vm_lost)`; open questions past `expires_at` become `expired`
  and the task fails with reason `timeout`; tasks past `wall_clock_limit` are
  cancelled with reason `timeout`.

### 7.3 Dispatch path

`POST /workspaces/:id/tasks` inserts `queued`, calls `ensure_vm_running`
(resume if needed), then sends `agent.run_turn` or `proc.spawn`. Events stream
back on the same channel; the API appends `task_events` and publishes
`task.event` / `task.status` with today's WebSocket event names. Uplift's
deterministic bootstrap runs as a `command` task in the VM (`g5d bootstrap`)
and submits child tasks through the API.

### 7.4 Batch jobs on one folder

Child tasks of one job run concurrently up to `max_concurrency` in the same
folder on the workspace branch. There is no job branch and no final merge task;
each agent commits and pushes its own work to the workspace branch, rebasing
onto it first when another task pushed in between. Conflicts are the agent's
to resolve. This removes `job_merge_service`, job merge runs, direct-clean
preflights, and the two-operation completion protocol.

### 7.5 Removed

Claim/ack/nack, per-task leases and heartbeats, `worker_session_generation`,
execution-attempt fences, `claimed`/`recovering`/`recovery_blocked`/`paused`/
`blocked`/`rolled_back`, the 33-command kernel and lock plans, the lease
reaper (one reconciler instead), question windows and timeout receipts,
completion constraints, task commit requests, task checkpoints, review
checkpoints, cascade prune, job pause, and the `workers` table.

---

## 8. Chats and the provider runtime

### 8.1 Model and history

```
chat_sessions(id, org_id, workspace_id, title, provider claude_code|codex, model,
              permission_mode, provider_session_id, created_by,
              forked_from_id, forked_at_index, created_at, updated_at)

chat_messages(id, session_id, index, role user|assistant|system, author_id NULL,
              content, attachments jsonb, status pending|streaming|complete|error|stopped,
              task_id NULL, usage jsonb, created_at)

chat_session_files(session_id, provider, provider_session_id, relative_path,
                   bytes bytea, sha256, size, updated_at,
                   PRIMARY KEY (session_id, provider_session_id, relative_path))
```

`author_id` is new: any workspace member may send into a session.

Chat history is a Postgres feature, as it is today. `chat_messages` holds the
conversation and `task_events` holds the agent timeline (tool calls, output,
questions, approvals) for every turn. The browser reads both through the API,
and neither a workspace VM nor any worker is involved in reading them. A
stopped or deleted VM never affects what a user can see. `chat_session_files`
is not shown to users; it exists only so the provider can resume its own
transcript (§8.4).

### 8.2 A turn

1. `POST /sessions/:id/messages` inserts the user message and a `pending`
   assistant message under a per-session advisory lock (one pending assistant
   per session) and creates a `chat_turn` task.
2. The API resumes the VM if needed and calls `agent.run_turn(provider,
   provider_session_id, prompt, attachments, options)`.
3. The dispatcher restores the session files if they are missing on the VM
   (§8.4), spawns the provider CLI (§8.3), maps its streamed events to
   `chat.chunk`, `chat.message_updated`, `chat.done`, `chat.error` and to
   `task_events`, and waits for the process to exit.
4. Tool-permission prompts and questions become `task_questions` rows; the
   process blocks on the answer (§8.3).
5. Process exit is the end of the turn. The dispatcher harvests changed session
   files into Postgres, reports the outcome, and nothing keeps running. There
   is no background work between turns: the provider exits only when its own
   subagents and tasks are done, and G5 does not track them.

Concurrent sessions in one workspace are concurrent processes in one folder.
Fork is a row copy of messages up to an index (§8.5). There is no worker pin,
route classification, forced fork, replacement handoff, or resident process.

### 8.3 One CLI process per turn, for both providers

The Claude Agent SDK, both Node sidecars (the 46K-line Claude bundle and the
Codex wrapper), the session-store adapter and proxy, the retained-sidecar
transport, wake hooks, and both runtime keepers are deleted. In their place
the dispatcher has one **provider runner** in Rust with two argv builders and
two event mappers. Node remains on the runtime mount only if the pinned Codex
CLI still requires it.

**Codex** (today's sidecar behavior, moved into the runner):

```
codex exec [resume <provider_session_id>] --json --skip-git-repo-check
      --output-last-message <tmp> [--model <m>]
      -c approval_policy="<a>" -c sandbox_mode="<s>" [-c model_reasoning_effort="<e>"]
      [-i <attachment>...]                      # prompt on stdin
env: CODEX_HOME=/work/.g5/codex-home (config.toml with the g5-tools MCP server, provider
     block pointing at the host LLM proxy), no OPENAI_* keys
```

**Claude** (new). `scripts/claude-code-cli-version` is bumped at the start of
Phase 3 to the newest tested release that has these flags (verified on
`2.1.269`), and the verified flag set is recorded with the pin:

```
claude -p --output-format stream-json --input-format stream-json --verbose
       --include-partial-messages
       [--resume <provider_session_id> | --session-id <new uuid>]
       --permission-mode <mode>                 # chat: server-frozen mode; non-chat tasks: bypassPermissions
       --permission-prompts host --permission-prompt-tool mcp__g5-tools__request_permission
       --mcp-config /work/.mcp.json --strict-mcp-config
       --settings /work/.claude/settings.json --setting-sources project
       --disallowedTools AskUserQuestion
       [--model <m>] [--max-turns <n>] [--append-system-prompt <runtime boundary text>]
env: CLAUDE_CONFIG_DIR=/work/.g5/claude-config, ANTHROPIC_BASE_URL=<host proxy>,
     placeholder ANTHROPIC_API_KEY as today, no real provider credential; runs as uid 1000
```

Runner behavior common to both:
- **Permission modes.** Chat turns use the server-frozen mode from
  `chat_sessions.permission_mode`. Non-chat tasks (uplift, downlift, sync,
  explainer, review, custom) run Claude with `bypassPermissions` and Codex with
  `approval_policy=never` / `sandbox_mode=workspace-write`, so they never
  block on a human.
- **Session id.** For a new session the runner generates the UUID and passes
  it (`--session-id` for Claude; Codex reports its id in the first `--json`
  item and the runner records it). For later turns it passes `--resume` /
  `exec resume <id>`.
- **Input.** The user message and attachments are written to stdin as one
  `stream-json` user message for Claude (image and text content blocks), and as
  the stdin prompt plus `-i` files for Codex. Then stdin is closed; the
  process must end the turn on its own.
- **Output.** One JSON object per line. Claude: `system` (init, carries
  `session_id`), `assistant` and `user` messages with content blocks (text,
  `tool_use`, `tool_result`), partial deltas, and a final `result` with usage
  and cost. Codex: the `--json` item stream as today. The mapper turns these
  into `chat.chunk` deltas, `task_events` timeline entries, and the final
  assistant message with usage.
- **Permission prompts.** Claude calls the MCP tool
  `request_permission(tool_name, input)` on the `g5-tools` server for anything
  its permission mode would prompt for. The tool posts
  `task_questions(kind=approval)` through the API, blocks until the user
  answers or the question expires, and returns `{"behavior":"allow"|"deny",
  "updatedInput":…, "message":…}`. Codex approval requests from `exec --json`
  are mapped the same way. With `bypassPermissions` no prompt reaches the tool.
- **Questions to the user.** `AskUserQuestion` is disallowed in print mode;
  both providers use the MCP tool `ask_user(question, options)` (also exposed
  as `g5d ask`), which posts `task_questions(kind=question)` and blocks until
  answered. While blocked, the task is `waiting`; if the VM is stopped before
  the answer arrives, the process is gone and the answer re-spawns the provider
  with `--resume` and the answer as the prompt.
- **Cancel.** SIGINT to the process, SIGKILL after 15 seconds.
- **Turn end.** Process exit. Exit code and the final result item decide
  `succeeded` versus `failed`. Nothing else is inspected.
- **Hooks.** `settings.json` hooks still fire in print mode; the profile keeps
  only `SessionStart` and `Stop`.
- **Runtime boundary text** (today's `## Current runtime boundary` appendix)
  is passed with `--append-system-prompt` instead of being written into
  `CLAUDE.md` on every launch.

What this deletes on the worker side: `claude_code.rs` (23K) and `codex.rs`
(15K) collapse into one runner with two small provider modules;
`g5_api_session_store_proxy.rs`, `runtime_keeper.rs` (both), the sidecar
halves of `sidecar_transport.rs` and `vsock_transport.rs`, `mcp_preflight.rs`
(reduced to a startup self-check), `tool_path_projection.rs`,
`screenshot_retention.rs` (attachments live in `chat_attachments`),
`host_lifecycle.rs`.

### 8.4 Provider session files in Postgres

Provider session state must survive VM loss and be independent of any VM. It
is stored only in Postgres, in `chat_session_files`:

- **Claude.** Under `CLAUDE_CONFIG_DIR`, the transcript is
  `projects/<escaped cwd>/<session id>.jsonl` and subagent transcripts are
  `projects/<escaped cwd>/<session id>/subagents/agent-*.jsonl` plus
  `agent-*.meta.json` (layout verified locally). Setting `CLAUDE_CONFIG_DIR` to
  a fixed VM path makes the project key deterministic.
- **Codex.** Under `CODEX_HOME`, `sessions/YYYY/MM/DD/rollout-<ts>-<session
  id>.jsonl` (today's `codex_session_artifacts` harvester already scans this
  layout).

After every turn the runner harvests files under the session's paths whose
sha256 changed and upserts them (zstd-compressed, size-capped at 128 MiB per
file as today). Before a turn that passes `--resume`, if the files are absent
on the VM (fresh VM after stop or host loss), the runner restores them from
Postgres first. If Postgres has nothing for that session id (never persisted,
or the provider rejected the transcript), the runner starts a new provider
session and prefixes the prompt with a history preamble built from
`chat_messages`, then records the new `provider_session_id`. Nothing about
sessions is checkpointed into git.

### 8.5 Fork and carry

Forking a chat copies `chat_messages` up to an index into a new session with
`provider_session_id = NULL`; the first turn starts a fresh provider session
with the history preamble. Creating a workspace with "carry chats" does the
same per selected session into the new workspace. Provider-native session
forking is not used.

### 8.6 Removing `naive_react` and the `fake` provider

- Delete the `NaiveReact` variant from `ChatAgentProvider` and every default
  that resolves to it; delete the `naive_react` normalization in the chat
  routes and `WorkspacePage`; delete prompt-template branches, bootstrap
  references, API schema entries, mock data, and tests that carry the value.
  The fresh schema's provider enum is `claude_code | codex`.
- The `fake` provider value disappears as well. Tests point `G5_CLAUDE_PATH` /
  `G5_CODEX_PATH` at fake `claude` and `codex` binaries that replay recorded
  `stream-json` / `--json` frames captured from the real pinned CLIs. The
  runner does not know it is fake; the `fake_*.mjs` SDK fixtures are rewritten
  as scripted fake-CLI scenarios.

### 8.7 Removed

`chat_sticky_runtimes` and every sticky column, function, trigger, and
heartbeat rider; `required_worker_id`, `branch_mode`, `attach_preview_task_id`,
`chat_sessions.status`; `chat_worker_loss`; `claude_session_store_entries`,
`codex_session_artifacts` (replaced by `chat_session_files`) and the
session-store proxy; context links; task-scoped session types
(`session_type='task'`, continue-in-merged-workspace); the Claude Agent SDK,
both Node sidecars; the `fake` and `naive_react` provider values.

---

## 9. Documents and editing

### 9.1 Reads

`GET /workspaces/:id/files/*path` (and the docs and ontology content routes
that wrap it):

| VM state | Source | Marker |
|---|---|---|
| running | dispatcher `fs.read` | `source: live` |
| stopped, checkpoint present | checkpoint tree in the internal store | `source: checkpoint` |
| stopped, no checkpoint | branch head in the internal store | `source: committed` |

Reads never resume the VM. The file tree and the Changes panel follow the same
rule.

### 9.2 Writes

Save sends `PUT /workspaces/:id/files/*path` with `expected_hash`. The API
asserts the branch (§5.6), resumes the VM if it is stopped, and calls
`fs.write`. A hash mismatch returns 409 with the current content; the editor
shows a reload-or-overwrite choice. Agent writes reach the UI through
`fs.watch` → `file.changed`; an editor with no local edits reloads silently,
one with local edits shows "changed on disk". Commit and Push are buttons over
`git.commit` and `git.push`. Job launchers no longer auto-commit anything.

### 9.3 Editors

Monaco and the block-style spec editor keep their UI but bind to local
document state with Save (Ctrl+S) and optional autosave-on-blur, instead of a
Yjs text. The structured question editors and explainer views follow the same
pattern; the question-document view that already renders without Yjs is the
template. Comments anchor to `(path, line, line hash)`.

### 9.4 Projections

`docs`, `ontology_nodes`, and search remain projections of git files. Their
reconcilers subscribe to `branch.updated` and `checkpoint.updated` instead of
the advance-consumer tables. `docs.content`, `mapping_content`, and
`scope_mode` are dropped.

### 9.5 Presence and co-editing

None in this iteration. Because all writes go through one Save API, a later
document-only CRDT can be added without changing persistence.

---

## 10. Governance

### 10.1 Protected refs

`repos.protected_branches` holds the default branch and any branch a base
workspace tracks. Receive-pack denies every network push to those refs (§5.3).
At repo connect time the API reads the provider's branch protection for the
default branch and warns when G5 approval rules and provider rules disagree.
Both gates apply: the internal boundary and the provider's own protection.

### 10.2 Pull requests

```
pull_requests(id, org_id, workspace_id, repo_id, source_branch, target_branch,
              status open|merged|closed, head_sha, title, body,
              provider_pr_number NULL, provider_pr_url NULL,
              created_by, created_at, merged_at, merged_by)
```

Created from the UI or by an agent with `g5d pr create`. Creation publishes the
source branch to the provider and opens a provider PR when the repo has a
provider; the provider PR is a mirror for external reviewers, not the source
of truth for merging. `head_sha` is refreshed on `branch.updated`.

### 10.3 Approvals and merge

```
approval_rules(id, org_id, repo_id, target_branch, required_approvers uuid[],
               min_approvals int, agent_reviewer_profile NULL, created_by, created_at)

approval_decisions(id, org_id, pull_request_id, head_sha, actor_user_id NULL,
                   actor_task_id NULL, outcome approved|changes_requested, note,
                   created_at)
```

A decision binds to `head_sha`; a new push makes prior decisions stale. An
agent review is an ordinary task with `agent_reviewer_profile` that ends by
calling `g5d pr review --approve|--request-changes`. Approvers are the rule's
list plus org admins.

`POST /pull-requests/:id/merge` is the only writer of protected refs:

1. PR is `open` and the caller may merge.
2. Decisions at the current `head_sha` satisfy the rule.
3. Mergeability: fast-forward or clean merge computed on bare repos.
4. If a provider PR exists, ask the provider whether it is mergeable.
5. Merge in the mirror (server identity), push the target branch to the
   provider with the acting user's token (or call the provider merge API when
   a provider PR exists), mark the PR `merged`. A provider rejection rolls the
   mirror back and returns 409 with the provider's reason.

PRs may also target another workspace's branch; those targets are not
protected, so the operation merges and skips the provider step.

### 10.4 Roles and login

Org roles `owner|admin|member|viewer` and workspace visibility with members are
kept. Per-user scope grants, workspace permission overrides, and workspace
mutation policy are dropped. RLS tenant isolation stays. Platform login is
GitHub only; `user_scm_connections` keeps GitHub and GitLab repository
credentials for publish and PR operations.

### 10.5 Removed

`approval_definitions/configs/requests/statuses/review_runs`,
`merge_evaluations`, `agent_approval_handoffs` (chat tool approvals are
`task_questions`), `job_merge_runs/artifacts`, `workspace_pull_request_events`
and `_provider_repos`, coordinated repo publication, workspace merge /
rebase-merge / change-graph services, stages and merge-target columns, the
Google/GitLab/Microsoft login adapters.

---

## 11. Agent surface

### 11.1 Keep (platform operations)

| Command / tool | Purpose |
|---|---|
| `g5d pr create|status|review` | open a PR, read its state, record an agent review decision |
| `g5d ask` and MCP `ask_user` | post a `task_questions` question and wait for the answer |
| MCP `request_permission` | Claude's permission-prompt tool; posts an approval question |
| `g5d task status|list` | inspect tasks and jobs |
| `g5d preview start|stop|status`, `g5d ports` | control the preview process, list open ports |
| `g5d auth`, `g5d doctor` | CLI login for laptops and diagnostics |
| MCP `g5-tools` grounding and ontology tools (`validate_spec`, `read_ontology`, ledgers) | unchanged; the rebase-pause and CRDT-peer couplings are removed |

### 11.2 Native

Git (`commit`, `push`, `pull`, `rebase`, `fetch`), files, shell, tests,
package managers. `origin = <api>/git/<repo_id>` with the workspace token in
the git credential helper. Agents are told: stay on the branch; other agents
may be working in the same folder; commit often; push to `origin` when a unit
of work is done; rebase onto `origin/main` to update; never push to `main`
(the server rejects it anyway).

### 11.3 Removed

`g5d commit`, `push`, `connect`, `conflicts`, `revert`, `fork`, `refresh`,
`merge`, `sync`, `view`, `spec sync`, `task rebase`, `task complete` (a task
ends when its process exits; the outcome is the exit code plus a JSON summary
file the profile asks the agent to write), `workspace push|switch`,
`job merge`, `chat context`, `intent-store`, `issue *`; the internal tool
registry (`file_ops`, `git_ops`, `terminal`, `task_lifecycle`, `view_ops`,
`search`, `interface`, `skill_invoke`, `send_message`, `pass_rate_tracker`);
the four issue MCP tools; the session-store proxy; MCP preflight reduced to a
startup self-check.

### 11.4 Launch and credentials

As in §8.3: one CLI process per turn, `cwd` = repo root, session files under
`/work/.g5/{claude-config,codex-home}` (VM-local cache of what Postgres
holds), `ANTHROPIC_BASE_URL` / Codex provider block at the host proxy, no
provider keys or tokens in the VM. `G5D_WORKSPACE_TOKEN` is the only
credential. The LLM proxy keeps metering usage and capturing provider quota
headers.

### 11.5 Profiles

`CLAUDE.md` and `AGENTS.md` are rewritten around native git, the branch rule,
the shared-folder rule, `g5d pr`, and `ask_user`. The shipped `/commit` and
`/pr` commands become the reference behavior with `gh pr create` replaced by
`g5d pr create`. Hooks reduce to `SessionStart` and `Stop`. CRDT-era skills
(`g5-coordinated-git`, `g5-rebase-workflow`, `g5-provided-agent-workspace`)
and the issue-reporting skill are deleted or rewritten.

---

## 12. Data model

Fresh baseline; names are proposals.

**Identity and tenancy:** `orgs`, `users`, `org_memberships`,
`org_invitations`, `user_scm_connections`, `api_keys`, `refresh_tokens`,
`oauth_states`, `cli_device_authorizations`, the `authz` schema,
`self_hosted_licenses`.

**Workspace and git (§5):** `repos`, `repo_secrets`, `workspaces`,
`workspace_repos`, `workspace_members`, `workspace_settings` (idle timeout,
defaults), `workspace_vms`.

**Execution (§6, §7):** `worker_hosts` (id, org, kind cloud|self_hosted|dev,
status, slots, heartbeat, image), `tasks`, `jobs`, `task_events`,
`task_questions`, `task_attempts`, `usage_records`, `provider_accounts`,
`provider_quota_snapshots`.

**Chat (§8):** `chat_sessions`, `chat_messages`, `chat_attachments`,
`chat_session_files`.

**Governance (§10):** `pull_requests`, `approval_rules`, `approval_decisions`.

**Projections and product features:** `docs`, `ontology_nodes`,
`node_dependencies`, `ontology_grounding`, `search_documents`,
`search_reindex_queue`, `comment_index`/`comment_replies`/`comment_mentions`,
`notifications`, `user_preferences`, one `configs(scope, scope_id, key, value)`
table for org/user/workspace config, `platform_issue_outbox`.

About 35 tables.

**Dropped (about 83 tables):** all draft, task-commit, head-operation,
advance, `pr1884_*`, `workspace_branches`/`workspace_branch_repos`,
`workspace_dependencies`, branch-integration, sync-run, job-merge,
`merge_evaluations`, approval-ledger, `agent_approval_handoffs`,
`workspace_pull_request*` (replaced), sticky-runtime, session-store and
codex-artifact (replaced by `chat_session_files`), context-link, `workers`,
`worker_tokens`, execution-attempt (replaced), completion-constraint,
task-checkpoint, question-window, escalation, attribution, preview-session,
submission-receipt, builder-spec, build-telemetry, prebuild-state,
`repo_manifests`, `local_agent_job_runs`, `cloud_orphan_sightings`,
`timeline_entries` (renamed), `doc_folders`, `doc_node_links`,
`waitlist_signups`, `oauth_scopes`, `org_default_scopes`, `user_scope_grants`,
`workspace_permission_overrides`, `pr`, and all thirteen issue and hotlist
tables (`issues`, `issue_comments`, `issue_components`, `issue_events`,
`issue_inference_jobs`, `issue_job_links`, `issue_labels`, `issue_links`,
`issue_attachments`, `issue_slug_sequence`, `hotlists`, `hotlist_events`,
`hotlist_memberships`, `hotlist_slug_sequence`).

Provider enum everywhere: `claude_code | codex`.

Migrations: the 292 files are replaced by a single `001_lean_baseline.sql`,
edited in place until the first production deployment; normal forward
migrations resume after that.

---

## 13. Incremental plan

Each phase ends with Cloud Build green, the Rust end-to-end crate and the
Playwright flows adjusted to the removed surface, and a demo of the
user-visible path.

**Phase 0 — Freeze and baseline (1 week).** Snapshot the current schema as the
working baseline migration; tag the last pre-lean commit; record the survey
sizes as the "before" number.

**Phase 1 — Git boundary (2 to 3 weeks).** Git refs authoritative; receive-pack
policy table (§5.3); `branch.updated` event; projections on the event. Delete
the head-operation registry, staging refs, finalizer, advance notifier and
consumer, remote-tracking pipeline, DB-owned head columns, job and task
branches. Workers push natively with their task token as an interim identity.
Exit: a task's commit lands on the workspace branch through plain `git push`;
a push to `main` is rejected; a fork creates a branch server-side.

**Phase 2 — Remove CRDT (3 to 4 weeks).** `source: live|checkpoint|committed`
reads (with "live" temporarily served by the per-task VM), explicit Save with
`expected_hash`, `git.status` Changes panel, Commit and Push buttons; rebind
Monaco and the spec editor; re-anchor comments. Delete `g5-crdt`, both peers,
`draft_service`, the yjs route, the task-commit protocol, `fs_diff`, the git
blocklists, the rebase pause, the frontend CRDT code. Exit: a user edits and
saves while an agent edits another file; both show in `git status`; commit
from the UI works; nothing references `yrs` or `yjs`.

**Phase 3 — Provider runtime (2 to 3 weeks, parallel with Phase 2).** Bump
the Claude CLI pin and record the verified flags. The Rust provider runner
(§8.3) with the Claude argv and event mapper and the Codex path moved in;
`request_permission` and `ask_user` MCP tools; `chat_session_files` harvest
and restore (§8.4); fake `claude` and `codex` binaries for tests. Delete the
Claude SDK sidecar, the Codex Node sidecar, session-store adapter and proxy,
`claude_session_store_entries`, `codex_session_artifacts`, the `naive_react`
and `fake` provider values, and the SDK-emulating fixtures. Sticky runtimes may
still exist at the end of this phase (they hold a VM, not a process); every
turn is already a fresh process. Exit: a Claude chat and a Codex chat each run
several turns with `--resume` continuity across a VM replacement, with
permission prompts and questions answered from the UI, and history readable
with no VM.

**Phase 4 — Workspace VM and dispatcher (4 to 6 weeks).** `g5d dispatch` (RPC,
process supervision, checkpoint/restore), `g5d host` VM lifecycle commands
over the tunnel, `workspace_vms`, the four workspace actions (§5.7), idle
stop, placement, the `vm_lost` reconciler; chat turns and agent tasks through
`agent.run_turn`. Delete the per-task materializer, rootfs-per-task, handoff
file, poll/claim/ack, sticky runtimes and every rider, golden images, worker
pins. Exit: two chats run concurrently in one workspace VM; the VM stops after
idle and a new message resumes it with unsaved edits restored from the
checkpoint and the provider session restored from Postgres; killing the host
mid-turn fails the turn and the next turn recovers on another host.

**Phase 5 — Preview and terminal (1 to 2 weeks).** `preview.start`, `ports`,
workspace-keyed relay and ticket, Ports panel, xterm terminal. Delete the
preview job type, sessions table, reaper, attach machinery.

**Phase 6 — Governance (2 to 3 weeks).** `pull_requests`, `approval_rules`,
`approval_decisions`, the merge operation, `g5d pr`, protected-branch
verification at repo connect, simplified PR and approvals UI. Delete the
approval ledger, job merge, coordinated publication, PR provider lifecycle
tables, mutation policy, scope grants.

**Phase 7 — Task lifecycle and schema squash (2 weeks).** Six-state model and
one reconciler; HITL unified on `task_questions`; one status enum each for DB,
Rust, and frontend; write `001_lean_baseline.sql` and delete the old
migrations.

**Phase 8 — Approved removals, infra, CI (ongoing).** Remove issue tracking,
the IDE mockup, the hidden settings UI, takeover remnants, and non-GitHub
platform login across frontend, routes, services, tools, scripts, Justfile,
and infra; drop the dormant Terraform host MIG, customer image lanes, and
disabled GitHub workflows; keep the Azure canary lane; consolidate crates;
revisit CI last (Cloud Build stays).

Rough total: 17 to 24 weeks for one team; Phases 1, 2/3, and 4 are the
critical path.

---

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| libkrun has no exec-into-VM API | Guest listener + per-VM token + mux, the pattern preview-attach uses today |
| A host loss takes a whole workspace | 10-minute checkpoint plus restore on another host; session files and chat history are in Postgres, not on the VM |
| Several agents in one folder conflict | By design: agents are told; UI shows a notice when more than one task runs; `git status` shows conflicts |
| Claude print-mode behavior differs from the SDK (permission prompt tool, stream-json input, session file layout) | Pin the CLI at Phase 3 start; verify against the pinned binary; the fake CLI replays frames recorded from the real one |
| A provider process never exits | `max_turns`, `wall_clock_limit` in `spec`, cancel with SIGINT then SIGKILL after 15 s |
| Session-file restore fails or the provider rejects a restored transcript | Fall back to a new provider session with the history preamble; chat history in Postgres is unaffected |
| Reads while the VM is stopped are only as fresh as the last checkpoint | Checkpoint on every stop and every 10 minutes; the UI labels `source: checkpoint` |
| Save resumes the VM | The UI shows "resuming workspace"; reads never wait |
| Single API replica (process-local tunnel registry and event bus) | Removing CRDT authority removes the hard reason; move the registry and bus to Redis in Phase 4 if needed |
| Provider branch protection is outside G5 | Verify at connect, warn on disagreement, treat provider rejection as final |
| Rebinding two large editors off Yjs is frontend-heavy | The question-document view is the template; editor tests retargeted in Phase 2 |
| Losing detailed execution-attempt audit | `task_attempts` with five reasons |

---

## 15. Validation

Existing safety net: the Rust end-to-end crate (about 75K lines) and about
100 Playwright flows, pruned per removed feature and extended per phase with
tests that fail without the new behavior and exercise the production boundary:

- **Git boundary:** receive-pack rejects protected refs, cross-workspace refs,
  zero-old-sha non-FF, and deletes; accepts FF and CAS non-FF on the owner's
  branch; `branch.updated` reaches projections; fork creates a ref.
- **Checkpoint/restore:** captures tracked, untracked, and deleted files but
  not ignored ones; restore reproduces `git status`; the diverged-head case
  applies cleanly or leaves the ref and a notification.
- **Workspace actions:** create, stop, resume, delete each leave the expected
  refs, rows, and VM state; idle stop equals stop; delete removes checkpoint
  refs and session files.
- **Provider runtime:** fake `claude` and `codex` binaries replay recorded
  frames; `--resume` receives restored files; `request_permission` and
  `ask_user` round-trip through `task_questions`; cancel delivers SIGINT then
  SIGKILL; a missing session in Postgres falls back to the preamble path; chat
  history renders with every VM stopped.
- **Dispatcher:** wrong-token RPC fails; stale-hash `fs.write` returns 409;
  process outcomes map to task states; dispatcher crash marks running tasks
  `vm_lost`; restore is idempotent.
- **Lifecycle:** the seven transitions and the reconciler rules; job status
  derivation; `waiting` does not keep the VM awake.
- **Chat:** two sessions concurrently in one folder; stop and resume with
  continuity; host kill mid-turn; a Claude session and a Codex session in the
  same workspace.
- **Governance:** merge refused without decisions at head; stale decision after
  a new push; provider rejection rolls back; the VM token cannot push `main`.
- **Preview/terminal:** relay reaches a port opened by an agent; ticket scope
  is one workspace; terminal survives editor navigation.

---

## 16. Size accounting

Estimates from the survey's file sizes (removal candidates only; big files
partially removed are not counted in full):

| Removed or replaced | Approx. lines |
|---|---|
| CRDT (backend) | 46,600 |
| CRDT (frontend, incl. tests) | 12,100 |
| Authority / head operations / advance | ~15,000 (+ large parts of `workspace_service.rs`, 16,500) |
| Remote tracking | ~9,000 |
| Sticky runtimes, keepers, chat worker loss, session-store proxy | ~18,000 (+ branches in `task_service.rs`, `chat_service.rs`, `supervisor.rs`) |
| Claude SDK sidecar (bundle, source, fixtures), Codex sidecar, both adapters | ~48,000 bundle + ~40,000 code replaced by one runner |
| Per-task execution (materializer, cache, supervisor bookkeeping) | ~15,000 |
| Golden images | ~10,000 |
| Job merge, coordinated publication, PR provider lifecycle | ~14,400 |
| Approval ledger (backend + frontend) | ~12,800 |
| Internal tool registry (dead) | ~16,400 |
| CRDT-bound CLI | ~19,800 |
| Chat context links | ~5,700 |
| Task lifecycle kernel, reaper, question windows, attempt fences | ~8,000 (+ most of `task_service.rs`, 33,000) |
| Preview job machinery (excluding the relay) | ~6,000 |
| Issue tracking (backend, tools, frontend) | ~10,000 |
| `naive_react` remnants, non-GitHub login, IDE mockup, hidden settings UI, takeover remnants | 39 files + frontend cleanup |
| Tables: 118 → ~35; migrations: 292 → 1 | |

Added: dispatcher (~4K to 6K including RPC, checkpoint, provider runner), host
command channel (~1K), receive-pack policy (~500), PR/approval/merge (~3K),
lifecycle (~2K), `request_permission` and `ask_user` tools (~500), frontend
Save/Changes/terminal/ports (~4K to 6K).

---

## 17. Implementation-time choices

These do not change the architecture and are left to the implementing phase:

- Exact Claude flag set and the pinned version (Phase 3 records both).
- Whether Node stays on the runtime mount (depends on the pinned Codex CLI).
- zstd level and per-file cap for `chat_session_files`; retention of files for
  deleted sessions (deleted with the session).
- Event names for `checkpoint.updated` and `file.changed` payload shapes.
- The `configs` table shape (one table with scope columns, or three narrow
  tables) and the `worker_hosts` column set after the fleet simplification.
- Whether the tunnel registry and event bus move to Redis in Phase 4 or later.
- Crate consolidation targets in Phase 8.

---

## 18. Decisions register

| Decision | Settled by | Section |
|---|---|---|
| Ontology and grounding tooling kept, decoupled from git and CRDT; everything else retained except the approved removals | Q1, B1 | §2, §3.2 |
| Targets: GCP SaaS, self-hosted Linux including the Azure canary lane, macOS dev-only; customer images dropped | Q2 | §2, §6.6 |
| VM is terminated when idle and re-provisioned on request; changes auto-committed and pushed before termination | Q3, Q5 | §5.4, §5.7 |
| libkrun microVM on shared worker hosts; equal-size VMs, count per host | Q4, Q9 | §4.1, §6.6 |
| Idle timeout 30 minutes; tabs do not count as activity | Q5 | §5.7 |
| VM disk ephemeral; warm when the host still has it | Q6 | §6.5 |
| No isolation inside the VM: one folder, one branch, no worktrees, no job branches | Q7, Q18 | §6.3, §7.4 |
| A turn ends when the provider process exits; no background work; subagents untracked | Q8 | §8.2 |
| Preview through the API relay, subdomain optional | Q10 | §6.4 |
| Browser terminal in this iteration | Q11 | §6.7 |
| Checkpoint = commit of the working tree on a force-updated shadow ref; ignored files not checkpointed | Q12, Q13 | §5.4 |
| Multi-repo workspaces with per-repo pushes; org docs repo stays a member | Q14, Q15 | §5.1 |
| Git refs authoritative; CAS non-FF on workspace branches; protected refs never | Q16, Q17 | §5.3 |
| Provider tokens never in the VM; publish server-side on explicit action; upstream via mirrored protected branches | Q19 | §5.5 |
| `RepoStorage` and its deployment choice unchanged | Q20 | §4.1 |
| Job kinds are profiles over one task machinery; all jobs run in the workspace VM | Q21 | §7.1 |
| No automatic retry on agent failure; one re-queue on VM loss when opted in | Q22 | §7.2 |
| No job pause; cancel only; no rollback | Q23 | §7.2 |
| One HITL table `task_questions`; `task_attempts` audit with five reasons | Q24, Q25 | §7.1 |
| Any workspace member may send into a chat; `author_id` recorded | Q26 | §8.1 |
| Context links dropped; task-scoped session types folded into chats; preview attach machinery dropped | Q27, Q29, Q30 | §8.7 |
| Provider session files persisted to Postgres only; restored before `--resume` | Q28 | §8.4 |
| No co-editing or presence; Save resumes the VM; comments anchored by path, line, hash; all four editors kept | Q31 to Q34 | §9 |
| Enforcement at both the internal git boundary and the provider; minimal rules and decisions tables; approvers = rule list plus org admins | Q35 to Q37 | §10 |
| Agents open PRs via `g5d pr create`; provider PRs mirrored; mutation policy and scope grants dropped | Q38 to Q41 | §10 |
| Host-side LLM proxy kept; ontology MCP tools kept; retained `g5d` commands; profiles rewritten with minimal hooks | Q42 to Q45 | §11 |
| Phase order as proposed; migrations squashed to one baseline; Cloud Build kept, revisited last | Q46 to Q48 | §12, §13 |
| Approved removals: issue tracking, IDE mockup, hidden settings UI, takeover remnants, non-GitHub platform login | B1 | §3.2, §12 |
| Claude CLI pin bumped at Phase 3 start with the verified flag set | B2 | §8.3 |
| Fake CLI binaries replace the `fake` provider and SDK fixtures | B3 | §8.6 |
| One Rust provider runner; both Node sidecars deleted | B4 | §8.3 |
| Non-chat tasks bypass permissions; chat turns use the frozen mode and `request_permission` | B5 | §8.3 |
| Checkpoint every 10 minutes when changed, plus on stop | B6 | §5.4 |
| Cancel: SIGINT, SIGKILL after 15 s, partial output kept as `stopped` | B7 | §7.2 |
| Chat history from Postgres via the API with no VM, as today | B8 | §8.1 |
| Multi-task notice only | B9 | §6.3 |
| Workspace actions: create, stop, resume, delete; no archive | B10 | §5.7 |
