# G5-Lean backend re-architecture — design (draft 1)

Status: draft for review, 2026-09-12. Nothing in this document is implemented.

This design was written after a read-only survey of the current codebase. The
survey notes, with `file:line` evidence, are in [docs/research/](research/README.md).
Statements about "today" cite those notes. Decisions that depend on input the
code cannot settle are tagged `[A#]` and each tag maps to a question in
[lean-backend-open-questions-v1.md](lean-backend-open-questions-v1.md) (`Q#`). The
design proceeds under the stated working assumption for every open question so
that a second iteration can turn it into an accepted design.

Deployment constraint: G5-Lean ships to a new environment. There is no
compatibility requirement for existing G5 data, so the schema is designed from
scratch and no migration of stored rows is planned.

---

## 1. Summary

G5 today runs one microVM per task attempt, keeps the source of truth for
uncommitted edits in a server-hosted CRDT, owns branch heads in Postgres with
git refs as projections, keeps chat VMs alive between turns through a
"sticky runtime" protocol, and layers approvals, merge runs, and remote-tracking
sync on top of all of it. Each of these was a reasonable answer to a real
problem, but together they are the complexity the user wants removed.

The target is a developer-laptop model:

| # | Proposal | Design section | What it deletes |
|---|---|---|---|
| 1 | One VM per workspace; tasks are local processes launched by a dispatcher over RPC | §6, §7 | per-task VM boot, materializer, host poll/claim/ack, golden images, capacity bookkeeping |
| 2 | Remove the CRDT layer | §9 | `g5-crdt`, API and worker CRDT peers, drafts, task-commit protocol, y-monaco/yjs in the UI |
| 3 | Internal Git as persistence, checkpoint before VM stop, restore from git | §5 | head-operation registry, staging refs, advance notifier/consumer, Postgres-owned heads |
| 4 | Native git for agents; far fewer G5D commands | §11 | ~20K lines of CRDT-bound CLI, 16K lines of dead internal tools, session-store proxy |
| 5 | Lightweight workspace↔branch check | §5.6 | git subcommand blocklists, rebase pause files, "authority scope" typing |
| 6 | Governance at the git boundary | §10 | 11-table approval ledger, job-merge runs, PR provider lifecycle, mutation policy |
| 7 | Explicit-Save document editing, readable while the VM sleeps | §9 | draft sessions, presence, CRDT-anchored comments |
| 8 | Shared chats, no forced fork, no process while idle | §8 | sticky runtimes (27 migrations), worker pins, transcript mirrors, context links |
| 9 | Preview on the workspace VM | §6.4 | preview job type, preview sessions table, reaper, chat-attach machinery |

Estimated effect (from file sizes in the survey, not a measured diff): on the
order of 200K to 300K of the ~1.1M lines of Rust are deleted or replaced by
much smaller code, 118 tables become roughly 40, and 292 migrations become one
baseline. The retained pieces are the ones that are already small and correct:
the git smart-HTTP server, the repo storage abstraction, the libkrun boot path,
the host↔API tunnel and stream multiplexer, the provider sidecars, the LLM
proxy, auth/orgs/RLS, and the projections for docs, ontology, and search.

---

## 2. Goals, constraints, non-goals

**Goals**

- Make the backend as simple as possible, and make it easy to keep stable.
- Deliver the nine proposals above.
- Developer experience like a laptop: the agent works on ordinary files in a
  long-lived environment, uses ordinary git, and the UI looks like GitHub
  Codespaces with one addition: a workspace is a first-class object that maps
  1-1 to a git branch. `[A3]`

**Constraints**

- Incremental: every phase in §13 leaves a buildable, testable product.
- Fresh deployment: no data compatibility, no historical-value readers, no
  backfills.
- Reuse what is good. The codebase is "not that bad"; whole crates survive.
- A retained/removed feature list is still pending from the user. This design
  is written so that feature removals compose with it rather than depend on it
  (`[A1]`).

**Non-goals for this iteration**

- Real-time multi-user co-editing of one file. A document-only CRDT can be added
  later behind the same Save API (§9.5).
- Hardening against hostile agents beyond today's microVM boundary and the
  credential rules in §6.3 and §11.4.
- New cloud providers. The fleet model in §6.6 keeps GCE hosts and self-hosted
  Linux hosts. `[A2]`

---

## 3. Where the complexity is today

This table is the cut list. Sizes are line counts from the survey and include
inline tests. "Disposition" is this design's proposal.

| Subsystem | What it does today | Size | Why it exists | Disposition |
|---|---|---|---|---|
| CRDT layer: `g5-crdt`, `g5-api-core/src/crdt`, `g5d-foundation/src/crdt`, `draft_service`, `routes/drafts`, frontend `lib/crdt.ts` and editors | One Yjs document per draft session hosted in the API, persisted in Postgres, synced to browser and worker; agents' file writes are scanned into it every 2 s | ~47K backend + ~12K frontend, plus assumptions threaded through the finalizer, sticky runtime, task service, and both editors | Makes uncommitted edits visible to the UI and durable across VM loss without any VM RPC | Remove (§9) |
| Head-operation "authority": `workspace_head_operations` and 6 sibling tables, `workspace_finalizer`, `advance_notifier`, `advance_consumer`, staging/committed refs, projection reapers | Postgres owns every branch head; a two-phase operation stages objects to `refs/g5/staged/...`, CAS-updates the DB, then re-projects `refs/heads/*` | ~15K in the API plus ~20 migrations; `workspace_service` (16.5K) is threaded with it | Atomic multi-repo cuts, replayable provenance, and the CRDT rollover in the same transaction | Remove; git refs become authoritative (§5.3) |
| Remote tracking: `branch_integration_*`, `branch_sync_runs`, pair leases, push modes, auto-pull, make-remote, remote-tracking merge runs, advisory pinger | Reconciles the internal mirror with GitHub in the API using the workspace creator's token | ~9K plus 8 migrations | Keep workspace branches in sync with upstream automatically | Remove; fetch protected branches only, publish on explicit action (§5.5) |
| Sticky runtimes: `chat_sticky_runtimes`, `sticky_runtime.rs`, runtime keeper, parked runtimes in the supervisor, `chat_worker_loss`, teardown generations, release proofs, discard fence, transcript mirrors and proxy | Keeps a chat's per-task microVM and sidecar alive between turns and proves the VM's workspace was reclaimed before a successor may reuse it | ~8K service + ~5K keeper + supervisor/task-service branches, 27 migrations, 2 transcript tables | The VM is per task and thrown away, so continuity had to be engineered on top | Remove; the workspace VM persists (§8) |
| Per-task execution: supervisor poll/claim/ack/observation, `Materializer`, `RepoCache`, rootfs COW clone, handoff file, capacity/deferred/superseded bookkeeping, attestor-per-boot | Boots one libkrun VM per task attempt with a fresh reflink checkout | supervisor 13K production lines (+16K tests), materializer + cache 9K, isolation 13K (mostly retained) | Isolation per task | Replace with VM-per-workspace lifecycle and an in-guest dispatcher (§6) |
| Golden images: builder/consumer/maintenance lanes, build telemetry, triggers, `builder_task_specs`, `workspace_prebuild_state` | Pre-builds sealed cache disks (docker, cargo, npm) per workspace so cold tasks start warm | ~10K plus 5 migrations | Every task starts cold | Remove; caches live on the workspace VM disk (§6.5) |
| Task lifecycle: kernel with 33 commands and 44 legal arcs, 12 task statuses (DB and Rust disagree), leases, claims, execution attempts, recovery statuses, lease reaper, question windows, completion constraints | State machine for tasks and jobs across a shared worker pool | ~120K including `task_service.rs` (33K) and `routes/internal.rs` (15K) | Every failure mode of a remote, leased, per-task worker got a fence | Replace with a 6-state, dispatcher-driven model (§7) |
| Governance: approval ledger (11 tables, 3 services ~8K, ~5K frontend), `job_merge_service` (9K), coordinated publication (3K), PR provider lifecycle (2.2K), mutation policy | Records approvals but nothing reads them at merge time; three merge paths; PRs mirrored to GitHub with the user's token | ~25K backend | Governance was designed as a ledger first; the enforcement gate was never built | Replace with protected refs + one PR + one decisions table (§10) |
| Agent surface: internal tool registry (`file_ops`, `git_ops`, `terminal`, `task_lifecycle`, ...), CRDT-bound CLI (`commit`, `push`, `connect`, `task complete/rebase`, head reconciliation), session-store proxy, MCP preflight | Tools the retired native engine used, plus CLI wrappers that route git through the CRDT and the authority protocol | ~16K dead + ~20K CRDT-bound + 1.2K proxy | The CRDT was the source of truth so git had to be mediated | Reduce to platform operations (§11) |
| Preview as a job: `job_preview_sessions`, boot generation, `preview_reaper`, separate `preview_vms` registry, chat-attach listener/token/drive | Boots a separate VM per preview from a committed ref | preview route 3.5K (relay retained), rest removed | Tasks were per VM, so the app needed its own | Run in the workspace VM (§6.4) |
| Fleet: `cloud/` provider bring-up, reconciler, rotation, stale-host detector, orphan sightings, `worker_hosts` (25+ columns), 24 h host JWT | Provisions and rotates a shared pool of GCE task hosts | ~6K plus 15 migrations | Shared pool sized for task throughput | Simplify to VM slots on hosts (§6.6) |
| Chat context links | Cross-session context injection with receipts | 4.7K + 1K frontend, 2 tables | Feature | Remove `[A27]` |
| Schema | 118 tables, 292 migrations, 22+ governance triggers, 17 validator functions | | | One baseline migration, ~40 tables (§12) |

Already removed before this design (dead-code batches 1 to 4): the native
ReAct engine, MicrovmAgent mode, WSL2 backend, the old daemon HTTP server, old
scheduler family, and disconnected frontend files. They are not re-proposed here.

---

## 4. Target architecture

### 4.1 Components

**g5-api (control plane).** Postgres-backed. Owns identity, orgs, workspace
registry, chat and task records, PRs and approvals, projections (docs,
ontology, search), the internal git server, the command channel to hosts, the
browser WebSocket, and the relays for preview and terminal traffic. Stateless
apart from process-local relay registries (§14).

**Internal Git store.** One bare repo per connected repository under the
existing `RepoStorage` abstraction (local disk or NFS). Holds mirrored
protected branches, every workspace branch, per-workspace checkpoint refs, and
one small per-workspace state repo. Git refs are authoritative.

**Worker host (`g5d host`).** A Linux machine with KVM (GCE VM, self-hosted
Docker with `/dev/kvm`, or a macOS dev host with HVF). Registers with the API,
dials an outbound tunnel, boots and stops workspace VMs on command, and relays
multiplexed streams between the API and each VM. It contains no task logic.

**Workspace VM.** One libkrun microVM per workspace `[A4]`, started on demand
and stopped when idle `[A5]`. Its disk is a cache `[A6]`. It runs the
**workspace dispatcher** (`g5d dispatch`, name provisional): restores the
workspace from git, serves an RPC surface to the API, supervises processes,
and checkpoints to git. Tasks, chat turns, previews, and terminals are
processes inside this VM.

**Frontend.** The existing React shell with a Codespaces-like center: file tree
and editors with explicit Save, a Changes panel driven by `git status`, a
terminal, a Ports/Preview panel, chat, tasks, PRs and approvals.

**Providers.** Claude Code and Codex run inside the VM through the existing
Node sidecars. LLM traffic goes through the host-side proxy so provider keys
never enter the VM `[A42]`.

### 4.2 Diagram

```
 browser ──HTTPS/WS──► g5-api ◄──HTTPS──► GitHub / GitLab (protected branches in, PR publish out)
                        │ ▲
      Postgres ◄────────┘ │  internal git: /git/<repo_id>  (smart HTTP, policy in receive-pack)
                          │
            outbound tunnel (WS + stream mux; host dials the API)
                          │
                    g5d host (per machine)
                     ├─ libkrun boot/stop, OCI rootfs templates, bare-repo cache
                     ├─ LLM proxy (provider keys stay here)
                     └─ per VM: TSI port map, mux relay
                          │
                 workspace VM  (one per workspace, /work = checkout)
                     └─ g5d dispatch
                          ├─ RPC: fs.*, git.*, proc.*, pty.*, ports.*, checkpoint.*
                          ├─ chat turn ─► node sidecar ─► claude / codex   (exits at turn end)
                          ├─ agent task ─► claude / codex / g5d bootstrap
                          ├─ preview   ─► user's dev server on port N
                          └─ terminal  ─► PTY shell
```

### 4.3 Invariants

These are the properties the simplified system keeps stable. Everything else is
allowed to be simple.

1. **Two sources of truth.** Files: git in the internal store. Metadata:
   Postgres. The VM disk is a cache that may be thrown away.
2. **One workspace, one branch, one VM, one dispatcher.** A `workspace_vms` row
   with a host lease is the only custody record. Two dispatchers can never own
   the same workspace at once.
3. **One write path for files.** Every file write happens inside the VM. Agents
   write natively; the UI writes through the dispatcher `[A32]`. The API never
   writes a working tree.
4. **Branch protection lives in receive-pack.** Protected refs accept updates
   only from the API's own merge operation, after approval checks. This holds
   for users, agents, and the VM identity alike.
5. **A task is a process.** Its state is derived from the process and its
   reported outcome. There are no leases, claims, or worker heartbeats per task.
6. **Association check before UI-triggered operations.** The dispatcher
   verifies that every member repo has the workspace branch checked out before
   Save, Run, Preview, Commit, Push, or PR actions proceed; otherwise the API
   returns `WORKSPACE_BRANCH_MISMATCH`.

---

## 5. Workspace and Git

### 5.1 Data model

```
repos(id, org_id, provider, provider_full_name, git_url, default_branch,
      protected_branches text[], created_at)

workspaces(id, org_id, name, slug, branch_name, base_branch, is_base,
           visibility org|invite_only, created_by, vm_size, idle_timeout_min,
           status active|archived, deleted_at)

workspace_repos(workspace_id, repo_id, root_path, role code|docs,
                head_sha_cache, PRIMARY KEY (workspace_id, repo_id))

workspace_vms(workspace_id PK, host_id, vm_id, status stopped|starting|running|
              stopping|failed, lease_expires_at, started_at, last_activity_at,
              last_checkpoint_at, last_error)
```

`workspace_repos` replaces both `workspace_branches` and
`workspace_branch_repos` and carries no authoritative head. `head_sha_cache` is
refreshed from push events for list views only; anything that needs the real
head reads the ref.

A workspace still spans N repos (code repos plus the org docs repo) `[A14]`
`[A15]`. The same branch name is used in every member repo, as today. Atomic
multi-repo cuts are dropped: pushes are per repo, like a laptop with several
clones.

### 5.2 Branch naming and identities

- Workspace branch: `<slug>-<8 hex of workspace id>` in every member repo
  (today's `feat/...` scheme is fine to keep; the point is uniqueness).
- Base workspace: `branch_name` = the repo's default branch, which is in
  `protected_branches`. A base workspace gets a VM like any other, so users can
  browse and chat on `main`, but nothing can push to it; changes require a fork.
- Fork: create `<new branch>` from the source branch head server-side, insert
  the workspace row, optionally copy chat sessions (row copy).
- Identities seen by receive-pack:
  - **user**: workspace member with write access (JWT as today);
  - **workspace VM**: `G5D_WORKSPACE_TOKEN`, a JWT scoped to one workspace id,
    minted at VM start, rotated on every start, TTL = idle timeout plus margin,
    renewable by the dispatcher;
  - **API (server identity)**: merges and forks, never over the network.

### 5.3 Receive-pack policy (the git boundary)

`routes/git.rs` keeps its shape: parse the ref updates, decide in Rust, then
spawn `git receive-pack --stateless-rpc`. The staging-ref rules are replaced by
this table.

| Ref | user (member, write) | workspace VM token | API server identity |
|---|---|---|---|
| `refs/heads/<protected>` | deny | deny | merge operation only (§10.3) |
| `refs/heads/<workspace branch>` | allow FF; allow non-FF with CAS (`--force-with-lease` semantics; old sha must match) | same | fork creation only |
| `refs/heads/<other workspace's branch>` | deny | deny | n/a |
| `refs/g5/checkpoints/<workspace id>` | deny | allow force-update | n/a |
| delete of workspace branch | deny (workspace deletion does it) | deny | workspace deletion |
| tags, other namespaces | deny | deny | n/a |

Notes.
- Non-fast-forward updates on the workspace branch are required because agents
  rebase onto `main` `[A17]`. git's own old/new sha check in the update command
  gives the compare-and-swap; the policy just declines updates whose old sha is
  zero on an existing ref, and declines any `X-G5-Force-Push`-style override.
- `git-upload-pack` (fetch/clone) requires read access to the workspace or repo,
  as today.
- After a successful push the handler publishes `branch.updated {repo_id,
  ref, old, new, actor}` on the event bus. Projections (search reindex, docs and
  ontology reconciliation, `head_sha_cache`, UI refresh) subscribe to it. This
  replaces the advance handoff tables.

### 5.4 Checkpoint and restore

The checkpoint is the durable record of a workspace's uncommitted state.
`[A12]` `[A13]`

**When.** On every VM stop (idle, explicit, host drain), periodically while
running (default every 10 minutes if the tree changed), and on demand
(`checkpoint.run` RPC, used by the UI's "Save all and sleep" and before host
rotation).

**What, per code repo.** A commit whose tree is the working tree (tracked and
untracked files, honoring `.gitignore`) and whose parent is the current branch
head. It is created with a temporary index (`GIT_INDEX_FILE=… git add -A;
git write-tree; git commit-tree`) so the real index and HEAD are untouched, and
force-pushed to `refs/g5/checkpoints/<workspace id>`. The message records the
branch name, HEAD sha, timestamp, and a summary of staged vs unstaged paths.
Ignored files (build outputs, `node_modules`, `target/`) and the docker scratch
disk are not checkpointed; they are cache.

**Workspace state repo.** `g5-state/<workspace id>` is a small bare repo in the
internal store, checked out in the VM at `$HOME/.g5/state`. It holds what must
survive VM loss but is not part of the user's repos: provider session
directories (`~/.claude/projects/...`, `CODEX_HOME/sessions/...`, relocated by
env vars), dispatcher metadata (last process table, preview profile in use),
and small logs. Checkpoint = commit and push, linear history, pruned by size.
This removes the Postgres transcript mirrors and the session-store proxy.

**Restore on VM start.**
1. Clone or reflink each member repo from the internal store (the host's bare
   cache stays as an optimization) and check out the workspace branch.
2. Fetch the checkpoint ref. If absent, done.
3. If the checkpoint's parent equals the branch head, materialize the checkpoint
   tree into the working tree without moving HEAD (`git restore
   --source=<ckpt> --worktree -- .` plus untracked files), leaving changes
   unstaged.
4. If the branch head moved while the VM was down (a user pushed from a laptop),
   apply the checkpoint as a no-commit cherry-pick. On conflict, leave the tree
   clean, keep the ref, and notify: "unsaved changes from the last session could
   not be applied automatically".
5. Clone the state repo; export the provider session directories.

**Guarantee.** Work committed and pushed by an agent is never lost. Work not
yet pushed is lost only if the VM dies between two checkpoints, bounded by the
periodic interval. This is the laptop guarantee the user asked for.

### 5.5 Provider integration (GitHub / GitLab)

`[A19]` `[A20]`

- **Inbound.** For each connected repo the API fetches the protected branches
  from the provider into the mirror on webhook and on a periodic poll (default
  5 minutes). Workspace branches are never fetched from the provider.
- **Outbound.** The API pushes a workspace branch to the provider only on
  explicit actions: PR publish (§10.2) and "Publish branch". It uses the acting
  user's stored token, `--force-with-lease`, and the same branch name (today's
  `g5/` prefix is dropped).
- **Updating a workspace from upstream** is a git operation inside the VM:
  `git fetch origin main && git rebase origin/main` (origin is the internal
  mirror, which already has the latest `main`). The UI's "Update from main"
  button runs it through `git.fetch_rebase`; agents are told to do it natively.
- **Local-only repos** (no provider) keep working: a bare repo in the store with
  no inbound or outbound sync.
- **Provider credentials never enter the VM.** Private package dependencies
  fetched over git need an org-level read-only token configured on the host
  proxy, or are unsupported `[A19]`.

Removed: `tracking_mode`, `push_mode`, `auto_pull_enabled`, `base_auto_pull`,
make-remote, sync runs, pair leases, advisory pinger, remote-tracking merge
runs, and the branch-integration worker and scheduler.

### 5.6 The workspace↔branch association check

`git.assert_branch(workspace)` in the dispatcher compares each member repo's
symbolic `HEAD` to `workspaces.branch_name`. The API calls it before every
UI-triggered mutation (Save, Run task, Commit, Push, Open PR, Preview start,
Checkpoint). Failure returns:

```
409 WORKSPACE_BRANCH_MISMATCH
{ repo: "backend", expected: "fix-login-3fa9c2d1", actual: "main", detached: false }
```

The UI shows the mismatch in the status strip with one action, "Switch back",
which runs `git switch <branch>` if the tree is clean and otherwise asks the
user to stash or commit first. Nothing prevents an agent or a terminal user
from running `git checkout`; the profile text instructs agents to stay on the
branch, and the check catches drift at the next UI operation. This replaces the
blocklists in `g5-scm/src/safety.rs`, the rebase pause file, and the
"authority scope" typing of branch rows.

---

## 6. Workspace VM and dispatcher

### 6.1 VM lifecycle

```
stopped ──wake──► starting ──restored──► running ──idle/stop──► stopping ──checkpointed──► stopped
                     │                      │
                     └──boot failure────────┴──host lost──► failed ──next wake──► starting
```

| Transition | Actor | Details |
|---|---|---|
| wake | API, on chat message, task run, Save, preview open, terminal open, explicit Start | pick a host with a free slot; insert/UPDATE `workspace_vms` with a lease; send `vm.start` |
| restored | dispatcher reports ready | §5.4 restore done, RPC listening |
| idle | API, when `last_activity_at` + `idle_timeout_min` passed and no running task, preview session, or terminal `[A5]` | send `vm.stop(checkpoint=true)` |
| stop | user or host drain | same as idle; drain checkpoints every VM on the host, then stops them |
| host lost | reconciliation loop: host heartbeat missing beyond 3 intervals | VM `failed`; running tasks `failed(vm_lost)`; next wake restores from the last checkpoint on another host |

Activity that keeps a VM awake: running tasks, an open terminal, preview
traffic in the last N minutes, file RPC in the last N minutes. A `waiting`
task (§7.2) does not.

### 6.2 Host daemon (`g5d host`)

Retained from today: registration and heartbeat, libkrun boot (`vm-exec`),
OCI rootfs templates and COW clone, XFS reflink bare-repo cache, port allocator
and VMM loopback addressing, passt networking, the outbound tunnel and stream
multiplexer, the LLM proxy, boot-time attestation.

New: a command channel on the tunnel. The API sends `vm.start(workspace_id,
spec)`, `vm.stop(workspace_id, checkpoint)`, `vm.status`, and opens mux
streams tagged `rpc`, `pty`, `http:<port>`, `llm` for a given VM. The host maps
each VM to its TSI port and per-VM token, exactly as preview-attach connects to
the guest listener today.

Removed: poll/claim/ack/nack/observation, `Materializer` per task, golden
lanes, parked runtimes, discard fence, sticky heartbeat riders, deferred and
superseded assignment bookkeeping, separate preview registry, per-task
attestation.

Capacity: a host advertises `max_vms` (derived from RAM and the org's default
VM size). Placement is "any host with a free slot in the org's region",
preferring the host that last ran this workspace (warm bare cache).

### 6.3 Dispatcher (`g5d dispatch`, inside the VM)

Boot sequence: mount `/work`, restore (§5.4), start the RPC listener on the TSI
port with the per-VM token, start the file watcher, report `ready`.

RPC surface (JSON-RPC over the `rpc` mux stream; events are server-pushed on
the same stream):

| Group | Methods | Used by |
|---|---|---|
| `fs` | `list`, `read`, `stat`, `write(path, content, expected_hash)`, `delete`, `rename`, `watch` (events `file.changed`) | file tree, editors, Save |
| `git` | `status`, `diff(path?)`, `log`, `assert_branch`, `switch_back`, `commit(message, paths)`, `push`, `fetch_rebase(base)` | Changes panel, Commit/Push buttons, association check |
| `proc` | `spawn(kind, argv, env, cwd, isolation)`, `signal`, `list`, `wait`; streams `stdout`, `stderr`, `event` | tasks, bootstrap |
| `agent` | `run_turn(provider, session_id, prompt, options)` (a `proc.spawn` of the provider sidecar with G5 env), `answer(task_id, payload)` | chat turns, agent tasks, question answers |
| `pty` | `open(cols, rows)`, `write`, `resize`, `close` | terminal |
| `ports` | `list` (from `/proc/net/tcp`), events `port.opened`, `port.closed` | Ports panel, preview auto-detect |
| `preview` | `start(profile)`, `stop`, `status` | Preview button |
| `checkpoint` | `run`, `last` | idle stop, "Save all and sleep" |
| `health` | `ping`, `resources` | reconciliation |

Process supervision: the dispatcher keeps an in-memory process table and
reports `task.started`, `task.event`, `task.exited(code, outcome)` to the API
over the channel. If the dispatcher itself crashes inside the VM, the host
detects the listener loss and the API marks that VM's running tasks
`failed(vm_lost)`; the VM is restarted (restore is idempotent).

Identity and credentials inside the VM: `G5D_WORKSPACE_TOKEN` (internal git
push, `g5d` platform commands), `G5D_API_URL`, the LLM proxy socket, the
provider CLIs and sidecars on `/opt/g5/bin`. No provider tokens, no host
token, no per-task token.

Concurrency `[A7]`: processes share the working tree by default, like several
terminals on a laptop. Two agents editing the same file will show up as
conflicting changes in `git status`, and the UI warns when more than one task
is running. Batch jobs that need isolation request `isolation: worktree`; the
dispatcher runs `git worktree add .g5/worktrees/<task> -b job/<job id>` and the
job ends with a merge task back onto the workspace branch (§7.4).

### 6.4 Preview

`preview.start(profile)` runs the command from `.g5/preview_profiles/<name>.yaml`
or the `preview:` block of `.g5/workspace.yaml` as an ordinary supervised
process and records its port. The browser reaches it at
`/api/v1/workspaces/:id/preview/:port/*`; the API resolves the workspace's host
and VM, opens an `http:<port>` mux stream, and reuses today's relay code
(`routes/preview.rs`) with the target lookup keyed by workspace and port instead
of job id. The workspace-scoped ticket cookie replaces the job-scoped one.
Subdomain mode stays optional `[A10]`.

Ports the agent opens on its own (a dev server started from chat) appear in
the Ports panel from `ports.list` and can be opened the same way. Preview
sessions stop with the VM and are not restarted on wake unless the user opens
them again.

Removed: the `preview` job type, `job_preview_sessions`, boot generations,
`preview_reaper`, the `preview_vms` registry, and the chat-attach listener,
token, and host drive. "Chat attached to a preview" is simply the default: the
agent and the app share the VM `[A30]`.

### 6.5 Caches and images

The VM image is the org or workspace `base_image` (OCI), unchanged. Language
caches, `node_modules`, `target/`, and the docker scratch disk live on the VM
disk and survive idle stops only if the disk is retained `[A6]`; on a fresh boot
they are rebuilt. Because a workspace VM is reused across many tasks and turns,
this replaces the golden-image machinery with nothing.

### 6.6 Fleet

Hosts are still API-provisioned GCE instances or self-hosted Linux machines
`[A2]`. What changes is sizing and lifetime: a host is a set of VM slots, not a
task pool. The reconciler (provisioning → ready, orphan sweep) stays. Rotation
becomes "drain: checkpoint and stop every VM, then replace". The 24-hour host
JWT is replaced by a renewable host credential so long-lived VMs do not force
rotation. Org quotas: max concurrently running VMs, default VM size `[A9]`.

### 6.7 Terminal

A browser terminal (xterm.js) over the `pty` stream, gated by workspace access.
This does not exist today; the PTY manager and the multiplexer do, so it is a
small addition and matches the Codespaces experience `[A11]`.

---

## 7. Task lifecycle

### 7.1 Model

```
tasks(id, org_id, workspace_id, job_id NULL, kind chat_turn|agent|command,
      status queued|running|waiting|succeeded|failed|cancelled,
      spec jsonb,        -- provider, model, prompt or argv, profile, isolation, env, retry_on_vm_loss
      outcome jsonb,     -- exit code, summary, commit shas, usage, error
      attempt int, depends_on uuid[], chat_session_id NULL, created_by,
      created_at, started_at, ended_at)

jobs(id, org_id, workspace_id, kind, status (derived), spec jsonb, created_by,
     max_concurrency, created_at, ended_at)          -- optional grouping for DAG runs

task_events(id bigserial, task_id, ts, kind, payload jsonb)   -- today's timeline_entries

task_questions(id, task_id, kind question|approval|escalation, payload jsonb,
               status open|answered|expired|cancelled, answer jsonb,
               expires_at, answered_by, answered_at)          -- one HITL table [A24]

task_attempts(id, task_id, attempt, started_at, ended_at, outcome, reason)  -- audit [A25]
```

Job kinds are profiles over the same task machinery (`uplift`, `downlift`,
`sync`, `explainer`, `agent_review`, `custom`) `[A21]`. A single agent run with no
DAG is a task without a job.

### 7.2 State machine

```
queued ──dispatch (VM running, deps met, concurrency slot)──► running
running ──exit 0 / outcome ok──► succeeded
running ──exit ≠ 0 / provider error──► failed
running ──question or approval posted──► waiting ──answered──► running
running | waiting ──cancel──► cancelled          queued ──cancel──► cancelled
running ──vm lost──► failed(vm_lost)  [re-queued once if spec.retry_on_vm_loss]
```

Six states, seven transitions, one writer: `TaskService::apply(event)` in the
API, called with events from the dispatcher (`started`, `exited`, `question`),
from users (`cancel`, `answer`), and from the VM reconciler (`vm_lost`).

Rules.
- `waiting` does not hold the VM awake. Answering wakes the VM if needed and
  resumes the process if it is still alive, otherwise re-spawns it with the
  provider's `--resume` session id from `spec`.
- No automatic retry on agent-reported failure `[A22]`. Human retry creates a
  new task (or increments `attempt` on the same row; either is fine).
- Job status is a pure function of child states; pause is "do not dispatch
  queued children"; cancel cascades. There is no rollback status; git is the
  undo `[A23]`.
- One reconciliation loop every 30 s: `running` tasks whose VM is not
  `running` become `failed(vm_lost)`; `waiting` questions past `expires_at`
  become `expired` and the task fails with reason `timeout`.

### 7.3 Dispatch path

`POST /workspaces/:id/tasks` inserts `queued`, calls `ensure_vm_running`
(§6.1), then sends `agent.run_turn` or `proc.spawn` over the host channel. Task
events stream back on the same channel; the API appends `task_events` and
publishes `task.event` / `task.status` on the browser WebSocket with today's
event names.

Uplift's deterministic bootstrap (the analysis DAG builder that runs in the host
process today) runs as a `command` task inside the VM (`g5d bootstrap`) and
submits child tasks through the API.

### 7.4 Batch jobs on a shared VM

Child tasks of one job run concurrently up to `max_concurrency` in the same VM.
When the job spec asks for `isolation: worktree`, each child gets its own
worktree on `job/<job id>` and the job's final task merges that branch into the
workspace branch with plain git; a conflict makes that final task fail with the
conflict list, and a human or a follow-up agent task resolves it in the VM.
This replaces `job_merge_service`, job merge runs, and direct-clean preflights.

### 7.5 Removed

Claim/ack/nack, per-task leases and heartbeats, `worker_session_generation`,
execution-attempt fences, `claimed`/`recovering`/`recovery_blocked`/`paused`/
`blocked`/`rolled_back`, the 33-command kernel and its lock plans, the lease
reaper (replaced by one reconciler), question windows and timeout receipts,
completion constraints, task commit requests, task checkpoints, review
checkpoints, cascade prune, and the `workers` table.

---

## 8. Chats

### 8.1 Model

```
chat_sessions(id, org_id, workspace_id, title, provider claude_code|codex, model,
              permission_mode, provider_session_id, status active|archived,
              created_by, forked_from_id, forked_at_index, created_at, updated_at)

chat_messages(id, session_id, index, role user|assistant|system, author_id NULL,
              content, attachments jsonb, status pending|streaming|complete|error|stopped,
              task_id NULL, usage jsonb, created_at)
```

`author_id` is new: any workspace member may send into a session `[A26]`.

### 8.2 A turn

1. `POST /sessions/:id/messages` inserts the user message and a `pending`
   assistant message under a per-session advisory lock (one pending assistant
   per session), and creates a `chat_turn` task.
2. The API wakes the VM if needed and calls `agent.run_turn(provider,
   provider_session_id, prompt, options)`.
3. The dispatcher spawns the provider sidecar with `--resume` when a session id
   exists. The sidecar's session files live under the state directory (§5.4).
   Streamed events become `chat.chunk`, `chat.message_updated`, `chat.done`,
   `chat.error` on the browser WebSocket, with today's names.
4. Tool-permission prompts become `task_questions(kind=approval)`; the sidecar's
   permission callback blocks on the answer.
5. The process exits at the end of the turn. Nothing runs while the chat is
   idle. Background subagent work that outlives the turn is not supported in
   this iteration `[A8]`.

Concurrent sessions in one workspace are concurrent processes on one tree.
Fork remains a row copy of messages up to an index. There is no worker pin,
no route classification, no forced fork, and no replacement handoff.

### 8.3 Removed

`chat_sticky_runtimes` and every sticky column, function, trigger, and
heartbeat rider; `required_worker_id`, `branch_mode`, `attach_preview_task_id`;
`chat_worker_loss`; `claude_session_store_entries` and `codex_session_artifacts`
and their proxy; context links `[A27]`; task-scoped session types
(`session_type='task'`, continue-in-merged-workspace) `[A29]`; the fork-with-chat
carry on workspace clone.

---

## 9. Documents and editing

### 9.1 Reads

`GET /workspaces/:id/files/*path` (and the docs and ontology content routes
that wrap it):

| VM state | Source | Response marker |
|---|---|---|
| running | dispatcher `fs.read` | `source: live` |
| stopped, checkpoint present | checkpoint tree in the internal store | `source: checkpoint` |
| stopped, no checkpoint | branch head in the internal store | `source: committed` |

Reads never wake the VM. The file tree and the Changes panel follow the same
rule (`fs.list` / `git.status` when running; tree and diff of the checkpoint or
head otherwise).

### 9.2 Writes

Save sends `PUT /workspaces/:id/files/*path` with `expected_hash` (the hash the
editor loaded). The API asserts the branch (§5.6), wakes the VM if it is asleep
`[A32]`, and calls `fs.write`. A hash mismatch returns 409 with the current
content; the editor shows a reload-or-overwrite choice. Agent writes are seen
by the UI through `fs.watch` → `file.changed`; an open editor with no local
edits reloads silently, one with local edits shows "changed on disk".

Commit and Push are buttons over `git.commit` and `git.push` in the dispatcher,
not draft operations. Every job launcher that used to auto-commit the draft
now simply runs; agents see the live tree.

### 9.3 Editors

Monaco (code) and the block-style spec editor keep their UI but bind to local
document state with Save (Ctrl+S) and optional autosave-on-blur, instead of a
Yjs text `[A34]`. The question-document view that already renders without a
Yjs document is the template. Comments anchor to `(path, line, line hash)`
`[A33]`.

### 9.4 Projections

`docs`, `ontology_nodes`, and search remain projections of git files. Their
reconcilers subscribe to `branch.updated` and `checkpoint.updated` instead of
the advance-consumer tables. The columns that cached content in Postgres
(`docs.content`, `mapping_content`, `scope_mode`) are dropped.

### 9.5 Presence and co-editing

None in this iteration `[A31]`. Because all writes go through one Save API, a
later document-only CRDT (one Yjs document per open file, hosted by the
dispatcher or the API) can be added without changing the persistence model.

---

## 10. Governance

### 10.1 Protected refs

`repos.protected_branches` holds the default branch and any branch a base
workspace tracks. Receive-pack denies every network push to those refs (§5.3).
At repo connect time the API reads the provider's branch protection for the
default branch and records whether it requires reviews, so the UI can warn when
G5 approvals and provider rules disagree `[A35]`.

### 10.2 Pull requests

```
pull_requests(id, org_id, workspace_id, repo_id, source_branch, target_branch,
              status open|merged|closed, head_sha, title, body,
              provider_pr_number NULL, provider_pr_url NULL,
              created_by, created_at, merged_at, merged_by)
```

Created from the UI or by an agent with `g5d pr create` `[A38]`. Creation
publishes the source branch to the provider and opens a provider PR when the
repo has one `[A39]`; the provider PR is a mirror for external reviewers, not
the source of truth for merging. `head_sha` is refreshed on `branch.updated`.

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
calling `g5d pr review --approve|--request-changes`, which inserts a decision
with `actor_task_id`.

`POST /pull-requests/:id/merge` is the only writer of protected refs:

1. PR is `open` and the caller may merge (member with write, or rule-defined).
2. Decisions at the current `head_sha` satisfy the rule (count and required
   approvers) `[A36]` `[A37]`.
3. Mergeability: fast-forward or clean merge computed on bare repos
   (`merge_branch_no_checkout` exists today).
4. If a provider PR exists, ask the provider whether it is mergeable.
5. Perform the merge in the mirror (server identity), push the target branch to
   the provider with the acting user's token (or call the provider merge API
   when a provider PR exists), then mark the PR `merged`. A provider rejection
   rolls the mirror back and returns 409 with the provider's reason.

PRs may also target another workspace's branch (child → parent workflow). Those
targets are not protected, so the same operation performs the merge and skips
the provider step.

### 10.4 Roles

Org roles `owner|admin|member|viewer` and workspace visibility with members are
kept. Per-user scope grants and workspace permission overrides are dropped
`[A41]`. RLS tenant isolation is kept as is. Workspace mutation policy is
dropped `[A40]`: read-only base workspaces follow from branch protection, and
"freeze this workspace" becomes "stop its VM and archive".

### 10.5 Removed

`approval_definitions/configs/requests/statuses/review_runs`,
`merge_evaluations`, `agent_approval_handoffs` (chat tool approvals move to
`task_questions`), `job_merge_runs/artifacts`, `workspace_pull_request_events`
and `_provider_repos` (folded into `pull_requests`), coordinated repo
publication, workspace merge / rebase-merge / change-graph services
(the change graph UI can be re-derived from `git diff` if kept), stages and
merge-target columns.

---

## 11. Agent surface

### 11.1 Keep (platform operations)

| Command / tool | Purpose |
|---|---|
| `g5d pr create|status|review` | open a PR, read its state, record an agent review decision |
| `g5d ask` (replaces `human_escalation` and question windows) | post a `task_questions` row and wait for the answer |
| `g5d task status|list` | inspect tasks and jobs |
| `g5d preview start|stop|status`, `g5d ports` | control the preview process, list open ports |
| `g5d issue *` | issue tracker, if the feature is retained `[A1]` |
| `g5d auth`, `g5d doctor` | CLI login for laptops, diagnostics |
| MCP `g5-tools`: grounding, ontology, `validate_spec`, `read_ontology`, `human_escalation` | unchanged if the ontology and grounding features are retained `[A43]`; the rebase-pause coupling is removed |

### 11.2 Native

Git (`commit`, `push`, `pull`, `rebase`, `fetch`, `worktree`), files, shell,
tests, package managers. The remote is `origin = <api>/git/<repo_id>` with the
workspace token pre-configured through the git credential helper. Agents are
told to stay on the branch, commit often, push to `origin` when a unit of work
is done, rebase onto `origin/main` to update, and never push to `main`
(the server rejects it anyway).

### 11.3 Removed

`g5d commit`, `push`, `connect`, `conflicts`, `revert`, `fork`, `refresh`,
`merge`, `sync`, `view`, `spec sync`, `task rebase`, `task complete` (a task
ends when its process exits; the outcome is the exit code plus a JSON summary
file the profile asks the agent to write), `workspace push|switch`, `job merge`,
`chat context`, `intent-store`; the internal tool registry (`file_ops`,
`git_ops`, `terminal`, `task_lifecycle`, `view_ops`, `search`, `interface`,
`skill_invoke`, `send_message`, `pass_rate_tracker`); the session-store proxy;
MCP preflight reduced to a startup self-check.

### 11.4 Launch and credentials

Providers launch as today (sidecar, `bypassPermissions` for non-chat,
server-frozen permission mode for chat), with `cwd` = repo root, session state
under the state directory, `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` at the host
proxy `[A42]`, and no provider keys or tokens in the VM. `G5D_WORKSPACE_TOKEN`
is the only credential. The LLM proxy keeps metering usage and capturing
provider quota headers; that is the cost-accounting path.

### 11.5 Profiles

`CLAUDE.md` and `AGENTS.md` are rewritten around native git, the branch rule,
`g5d pr`, and `g5d ask`. The shipped `/commit` and `/pr` commands, which already
teach raw git and PR creation, become the reference behavior (with `gh pr
create` replaced by `g5d pr create`). Advisory hooks are reduced to
`SessionStart` and `Stop` `[A45]`. Skills that describe the CRDT workflow
(`g5-coordinated-git`, `g5-rebase-workflow`, `g5-provided-agent-workspace`) are
deleted or rewritten.

---

## 12. Data model

Fresh baseline. Names are proposals; the point is the count and the ownership.

**Identity and tenancy (kept, trimmed):** `orgs`, `users`, `org_memberships`,
`org_invitations`, `user_scm_connections`, `api_keys`, `refresh_tokens`,
`oauth_states`, `cli_device_authorizations`, the `authz` schema (sessions and
context functions), `self_hosted_licenses` (if retained).

**Workspace and git (§5):** `repos`, `repo_secrets` (webhook secrets),
`workspaces`, `workspace_repos`, `workspace_members`, `workspace_settings`
(trimmed to VM size, idle timeout, defaults), `workspace_vms`.

**Execution (§6, §7):** `worker_hosts` (id, org, kind cloud|self_hosted|dev,
status, capacity, heartbeat, image), `tasks`, `jobs`, `task_events`,
`task_questions`, `task_attempts`, `usage_records`, `provider_accounts`,
`provider_quota_snapshots` (if retained).

**Chat (§8):** `chat_sessions`, `chat_messages`, `chat_attachments`.

**Governance (§10):** `pull_requests`, `approval_rules`, `approval_decisions`.

**Projections and product features:** `docs`, `ontology_nodes`,
`node_dependencies`, `ontology_grounding`, `search_documents`,
`search_reindex_queue`, `comment_index`/`comment_replies`/`comment_mentions`,
`notifications`, `user_preferences`, `org_configs`/`user_configs`/
`workspace_configs` (consolidate into one `configs(scope, scope_id, key,
value)` if convenient), issues and hotlists tables if retained,
`platform_issue_outbox` if retained.

**Dropped (about 75 tables):** `draft_sessions`, `draft_updates`,
`draft_participants`, `task_commit_requests`, `task_commit_frontier_cuts`,
`task_commit_frontier_receipts`, `workspace_head_operations` and its 5 sibling
tables, `workspace_advance_adapter_receipts`, `workspace_advance_consumer_inputs`,
8 `pr1884_*` evidence tables, `workspace_branches`, `workspace_branch_repos`,
`workspace_dependencies`, `branch_integration_queue`,
`branch_integration_remote_pair_leases`, `branch_sync_runs`, `job_merge_runs`,
`job_merge_artifacts`, `merge_evaluations`, `approval_definitions`,
`approval_configs`, `approval_requests`, `approval_statuses`,
`approval_review_runs`, `agent_approval_handoffs`, `workspace_pull_requests`
(replaced), `workspace_pull_request_events`, `workspace_pull_request_provider_repos`,
`chat_sticky_runtimes`, `claude_session_store_entries`, `codex_session_artifacts`,
`chat_session_context_links`, `chat_message_context_resolutions`, `workers`,
`worker_tokens` (host enrollment moves to a host credential), `task_execution_attempts`
(replaced), `task_completion_constraints`, `task_checkpoints`,
`task_question_windows`, `human_escalations`, `human_escalation_questions`,
`runtime_attribution_events`, `job_preview_sessions`, `job_submission_receipts`,
`builder_task_specs`, `session_build_telemetry`, `workspace_prebuild_state`,
`repo_manifests`, `local_agent_job_runs`, `cloud_orphan_sightings`,
`timeline_entries` (renamed), `doc_folders`, `doc_node_links`, `intent_record`
(unless grounding keeps it), `waitlist_signups`, `oauth_scopes`,
`org_default_scopes`, `user_scope_grants`, `workspace_permission_overrides`,
`pr`.

Migrations: the 292 files are replaced by a single `001_lean_baseline.sql`
that is edited in place until the first production deployment `[A47]`; after
that, normal forward migrations resume.

---

## 13. Incremental plan

Each phase ends with CI green, the Rust end-to-end crate and the Playwright
flows adjusted to the removed surface, and a demo of the user-visible path.
The order is chosen so that each phase removes the reason for the next one's
complexity `[A46]`.

**Phase 0 — Freeze and baseline (1 week).** Confirm the answers to the open
questions; land the retained-feature list; snapshot the current schema as the
working baseline migration; tag the last pre-lean commit; record the survey
sizes as the "before" number.

**Phase 1 — Git boundary (2 to 3 weeks).** Make internal git refs
authoritative. Implement the receive-pack policy table (§5.3) and the
`branch.updated` event. Point projections (search, docs, ontology) at the
event. Delete the head-operation registry, staging refs, finalizer, advance
notifier/consumer, remote-tracking pipeline, and the DB-owned head columns.
Workers push natively (`git push origin <branch>`) with their existing task
token as an interim identity. Exit: a task's commit lands on the workspace
branch through plain `git push`; a push to `main` is rejected; a fork creates a
branch server-side.

**Phase 2 — Remove CRDT (3 to 4 weeks).** Implement `source: live|checkpoint|
committed` reads (with the interim "live" source being the existing per-task
VM until Phase 3), explicit Save with `expected_hash`, `git.status`-driven
Changes panel, Commit and Push buttons. Rebind Monaco and the spec editor to
local state; re-anchor comments. Delete `g5-crdt`, both CRDT peers,
`draft_service`, the yjs route, the task-commit protocol, `fs_diff`, the git
blocklists, the rebase pause, and the frontend CRDT code. Exit: a user edits
and saves a file while an agent edits another; both changes appear in
`git status`; commit from the UI works; nothing references `yrs` or `yjs`.

**Phase 3 — Workspace VM and dispatcher (4 to 6 weeks).** Build `g5d dispatch`
(RPC surface, process supervision, checkpoint/restore, state repo). Rework
`g5d host` to VM lifecycle commands over the tunnel. Implement `workspace_vms`,
wake/idle, placement, and the `vm_lost` reconciler. Move chat turns and agent
tasks to `agent.run_turn`. Delete the per-task materializer, rootfs-per-task,
handoff file, poll/claim/ack, sticky runtimes and every rider, transcript
mirrors and proxy, golden images, worker pins. Exit: two chats run
concurrently in one workspace VM; the VM stops after idle and a new message
restores it with unsaved edits and session continuity intact; killing the host
mid-turn fails the turn and the next turn recovers on another host.

**Phase 4 — Preview and terminal (1 to 2 weeks).** `preview.start`, `ports`,
workspace-keyed relay and ticket, Ports panel, xterm terminal. Delete the
preview job type, sessions table, reaper, attach machinery. Exit: preview and
terminal work on a workspace VM; a dev server started from chat is reachable.

**Phase 5 — Governance (2 to 3 weeks).** `pull_requests`, `approval_rules`,
`approval_decisions`, the merge operation, `g5d pr`, protected-branch
verification at repo connect, PR and approvals UI simplified onto the new
tables. Delete the approval ledger, job merge, coordinated publication, PR
provider lifecycle tables, mutation policy, scope grants. Exit: a PR opened by
an agent cannot merge without the rule's approvals; an approved PR merges into
the mirror and the provider; a direct push to `main` from the VM is refused.

**Phase 6 — Task lifecycle and schema squash (2 weeks).** Replace the kernel
with the six-state model and one reconciler; unify HITL on `task_questions`;
collapse the remaining status vocabularies (DB, Rust, frontend) into one enum
each; write `001_lean_baseline.sql` from the surviving tables and delete the
old migrations. Exit: the lifecycle tests in §15 pass; `sqlx prepare` is clean
against the baseline; the migration directory has one file.

**Phase 7 — Feature removals and cleanup (ongoing).** Apply the retained-feature
list to frontend, routes, services, scripts, Justfile, infra (drop the dormant
Terraform host MIG, customer image lanes, disabled workflows, Azure canary if
not retained). Consolidate crates (`g5d-*` into `g5d`, `g5-api-*` into two or
three) once the surface is stable.

Rough total: 15 to 21 weeks of focused work for one team, with Phases 1 to 3
on the critical path and Phases 4 and 5 parallelizable after Phase 3.

---

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| libkrun has no exec-into-VM API; the dispatcher must be reachable from the host | Use the guest-listener + per-VM token + mux pattern that preview-attach already uses; the pattern is proven in this codebase |
| A host loss now takes a whole workspace, not one task | Periodic checkpoint (10 min) plus restore on another host; VM-loss test in §15 |
| Two agents on one tree conflict | Visible in `git status`; UI warning when more than one task runs; opt-in worktree isolation for batch jobs |
| Reads while the VM sleeps are only as fresh as the last checkpoint | Checkpoint on every stop and periodically; the UI labels `source: checkpoint` |
| Save wakes the VM, adding latency to the first edit after idle | Acceptable for a laptop model; the UI shows "waking workspace" and the read path never waits |
| Single API replica because relay registries and the event bus are process-local | Removing the CRDT authority removes the hard reason; move the tunnel registry and event bus to Redis in Phase 3 if a second replica is needed |
| Provider branch protection is outside G5's control | Verify at connect time, warn on disagreement, treat provider rejection as the final word on merge |
| Provider tokens: publishing with the user's token attributes the push to that user | Same as today; an org-level installation token is a later option |
| Rebinding two large editors (Monaco, spec block editor) off Yjs is frontend-heavy | The question-document view already renders without Yjs and is the template; do it in Phase 2 with the editor tests retargeted |
| Ontology and grounding MCP tools are coupled to the rebase pause and CRDT peer | Remove those two couplings only; the JSONL ledgers are plain files |
| Workspace state repo grows with provider session files | Prune by size and age on checkpoint; sessions are append-only JSONL and compress well |
| Losing the fine-grained execution-attempt audit | Keep `task_attempts` with a small reason vocabulary |

---

## 15. Validation

Existing safety net: the Rust end-to-end crate (99 files, about 75K lines) and
about 100 Playwright flows. Both are pruned per removed feature and extended
per phase with tests that fail without the new behavior and exercise the
production boundary:

- **Git boundary:** receive-pack rejects protected refs, cross-workspace refs,
  zero-old-sha non-FF, and deletes; accepts FF and CAS non-FF on the owner's
  branch; `branch.updated` reaches projections; fork creates a ref.
- **Checkpoint/restore:** checkpoint captures tracked, untracked, and deleted
  files but not ignored ones; restore reproduces `git status` exactly; the
  diverged-head case applies cleanly or leaves the ref and a notification.
- **Dispatcher:** RPC auth with a wrong token fails; `fs.write` with a stale
  hash returns 409; `proc.spawn` outcomes map to task states; dispatcher crash
  marks running tasks `vm_lost` and restore is idempotent.
- **Lifecycle:** all seven transitions and the two reconciler rules; job status
  derivation; `waiting` does not keep the VM awake.
- **Chat:** two sessions concurrently; idle stop and wake with `--resume`
  continuity; host kill mid-turn.
- **Governance:** PR merge refused without decisions at head; stale decision
  after a new push; provider rejection rolls back; VM token cannot push `main`.
- **Preview/terminal:** relay reaches a port opened by an agent; ticket scope
  is one workspace; terminal survives editor navigation.

---

## 16. Size accounting

Estimates from the survey's file sizes (removal candidates only; big files are
partially removed and are not counted in full):

| Removed or replaced | Approx. lines |
|---|---|
| CRDT (backend) | 46,600 |
| CRDT (frontend, incl. tests) | 12,100 |
| Authority / head operations / advance | ~15,000 (+ large parts of `workspace_service.rs`, 16,500) |
| Remote tracking | ~9,000 |
| Sticky runtimes, keeper, chat worker loss, transcript mirrors and proxy | ~18,000 (+ branches in `task_service.rs`, `chat_service.rs`, `supervisor.rs`) |
| Per-task execution (materializer, cache, supervisor bookkeeping) | ~15,000 of production code |
| Golden images | ~10,000 |
| Job merge, coordinated publication, PR provider lifecycle | ~14,400 |
| Approval ledger (backend + frontend) | ~12,800 |
| Internal tool registry (dead) | ~16,400 |
| CRDT-bound CLI | ~19,800 |
| Chat context links | ~5,700 |
| Task lifecycle kernel, reaper, question windows, attempts fences | ~8,000 (+ most of `task_service.rs`, 33,000) |
| Preview job machinery (excluding the relay) | ~6,000 |
| Tables: 118 → ~40; migrations: 292 → 1 | |

Added: dispatcher (~4K to 6K including RPC and checkpoint), host command
channel (~1K), receive-pack policy (~500), PR/approval/merge (~3K), lifecycle
(~2K), frontend Save/Changes/terminal/ports (~4K to 6K).
