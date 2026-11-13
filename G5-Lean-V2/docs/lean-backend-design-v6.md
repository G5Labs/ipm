# G5-Lean backend re-architecture — design (draft 6)

Status: draft 6, 2026-09-13, revised after the draft 5 review; candidate for
acceptance. Supersedes [lean-backend-design-v5.md](lean-backend-design-v5.md)
(draft 5) and the earlier drafts; all are kept unchanged for history. Nothing
in this document is implemented.

Draft 6 applies the eight revisions accepted in
[lean-backend-design-v5-review-response.md](lean-backend-design-v5-review-response.md).
None of them changes a settled decision (round 1, round 2, and Decisions A to
D stand). No decisions are pending. Implementation-time choices are in §17.1,
deferred items in §17.2, and §18 traces every decision to the question or
review finding that settled it. Evidence for statements about "today" is in
[docs/research/](research/README.md).

Deployment constraint: G5-Lean ships to a new environment. No compatibility
with existing G5 data is required; the schema is designed from scratch.

---

## 0. What changed since draft 5

| Area | Draft 5 | Draft 6 |
|---|---|---|
| Checkpoint snapshot (F1) | Temporary index started empty; a tracked file matched by `.gitignore` was dropped | Temporary index is seeded from the real index (`cp -p`), re-hashed with `--really-refresh`, then `git add -A`; ignore rules affect untracked files only (§5.4) |
| Checkpoint observation (F2) | `git fetch` to observe the internal head; moved the agent's implicit `--force-with-lease` lease | `git ls-remote` observation; the checkpoint task never updates `origin/<branch>` (§5.4, §11.2) |
| Dispatch loop (F3) | Filtered on "VM running" before waking; a stopped workspace never woke | Ready tasks are selected regardless of VM state; resume is requested first; starts are issued only to running VMs with a slot (§7.3) |
| Start intent (F4) | `running` recorded before the RPC; an undelivered start became `vm_lost` | Two-phase start: `start_confirmed` flag, idempotent re-send until the dispatcher confirms; `vm_lost` only for confirmed starts; cancel-before-launch cannot be resurrected (§7.3, §7.6) |
| Stop authorization (F5) | Old `vm_id` rejected from step 1; reconciler treated `stopping` as loss | Admission close and incarnation revocation are separate; the draining incarnation stays current until release; the reconciler treats `starting|running|stopping` with a live lease as live (§5.7, §6.8, §7.6) |
| Answer delivery (F6) | Claimed a transaction that both settled the question and delivered the answer | Answer committed first; the MCP server long-polls the committed answer (no `agent.answer` RPC); consumption recorded; exactly one successor turn after interruption (§7.2, §8.3) |
| Merge recovery (F7) | "Any failure → failed"; local CAS retry could fail after success | One operation identity retained until resolved; `provider_merged` survives import errors; local merge commit persisted before the CAS and recognized on recovery; `failed` only for known rejection before application (§10.6) |
| Event retention (F8) | Only terminal outcomes journaled; highest-seen gating | Every event journaled per execution until acknowledged; per-execution sequences; cumulative watermark acks; replay in order; unique-key deduplication (§6.3, §7.6) |
| Checkout | | No code, infra, or asset file changed since draft 5 |

---

## 1. Summary

G5 today runs one microVM per task attempt, keeps uncommitted edits in a
server-hosted CRDT, owns branch heads in Postgres with git refs as
projections, keeps chat VMs and a Claude SDK process alive between turns
through a "sticky runtime" protocol, and layers approvals, merge runs, and
remote-tracking sync on top. The target is a developer-laptop model with one
difference: the VM is not as persistent as a laptop. It is terminated when
idle and re-provisioned on request; a background checkpoint task commits the
workspace's changes to a checkpoint ref in internal git every few minutes and
before termination, and that is what the next VM restores.

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
about 36; 292 migrations become one baseline. The contracts added after review
(incarnation fence, dispatch and event contract with start intents and event
journals, stop barrier, merge operation, answer polling) are each a few
hundred lines and replace subsystems of tens of thousands.

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
G5 never looks inside it); interactive Codex tool-permission approvals
(deferred, §17.2; user questions from either provider are in scope, §8.3).

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
| Task lifecycle (33-command kernel, 44 arcs, 12 task statuses, leases, claims, attempts fences, recovery statuses, reaper, question windows, completion constraints) | State machine for a shared leased worker pool | ~120K including `task_service.rs` (33K) and `routes/internal.rs` (15K) | Replace with a 6-state dispatcher-driven model plus one dispatch and event contract (§7) |
| Governance (approval ledger: 11 tables, ~8K backend, ~5K frontend; `job_merge_service` 9K; coordinated publication 3K; PR provider lifecycle 2.2K; mutation policy) | Records approvals that nothing reads at merge time; three merge paths | ~25K backend | Replace with protected refs + `pull_requests` + `approval_rules` + `approval_decisions` + one merge operation (§10) |
| Agent surface (internal tool registry 16K dead; CRDT-bound CLI 20K; session-store proxy 1.2K; MCP preflight) | Tools for the retired native engine and CLI wrappers that mediate git through CRDT and authority | ~37K | Reduce to platform operations (§11) |
| Preview as a job (`job_preview_sessions`, boot generation, reaper, `preview_vms` registry, chat-attach listener/token/drive) | Separate VM per preview from a committed ref | relay 3.5K kept, ~6K removed | Run in the workspace VM (§6.4) |
| Fleet (`cloud/` bring-up, reconciler, rotation, stale-host detector, orphan sightings, 25-column `worker_hosts`, 24 h host JWT) | Shared pool of task hosts | ~6K plus 15 migrations | Simplify to VM slots on hosts (§6.6) |
| Chat context links | Cross-session context injection | 4.7K + 1K frontend, 2 tables | Remove |
| Internal issue tracking (`routes/issues.rs`, `issue_service.rs`, attachments, inference, four MCP tools, `IssuesPage`, hotlists) | Live-disabled ticket module | ~10K backend + frontend, 13 tables | Remove (approved) |
| `naive_react` remnants | Enum variants, DB defaults, normalization in chat routes, frontend mapping, prompts, tests | 39 files | Delete completely (§8.6) |
| Schema | 118 tables, 292 migrations | | One baseline, ~36 tables (§12) |

Already removed before this design: the native ReAct engine, MicrovmAgent
mode, WSL2 backend, old daemon HTTP server, old scheduler family,
disconnected frontend files (dead-code batches 1 to 4).

### 3.2 Checkout re-check and approved removals

No code, infra, or asset file changed between drafts 2 and 6
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
resume and terminated on stop or idle. Each attach is a new incarnation
(`vm_id`). Its disk is a cache. It runs the **workspace dispatcher**
(`g5d dispatch`, name provisional): restores the workspace from git, serves an
RPC surface, supervises processes, journals their events, and runs the
background checkpoint task. Every task, chat turn, preview, and terminal is a
process inside this VM, in the same folder, on the workspace branch.

**Frontend.** The existing React shell with a Codespaces-like center: file
tree and editors with explicit Save, a Changes panel from `git status`, a
terminal, Ports and Preview, chat, tasks, PRs and approvals.

**Providers.** The pinned `claude` and `codex` CLIs on the VM's runtime mount.
One process per turn. LLM traffic goes through the host-side proxy; no
provider keys in the VM.

### 4.2 Diagram

```
 browser ──HTTPS/WS──► g5-api ◄──HTTPS──► GitHub / GitLab (protected branches in, PR publish out, provider merges)
                        │ ▲
      Postgres ◄────────┘ │  internal git: /git/<repo_id>  (smart HTTP, policy in receive-pack)
   (metadata, chat       │
    messages + timeline, │
    session files)       │
            outbound tunnel (WS + stream mux; host dials the API)
                          │
                    g5d host (per machine, N equal VM slots)
                     ├─ libkrun boot/stop, OCI rootfs templates, bare-repo cache
                     ├─ LLM proxy (provider keys stay here; rejects stale vm_id)
                     └─ per VM: TSI port map, mux relay
                          │
                 workspace VM  (one per workspace, incarnation vm_id, /work = checkout on the workspace branch)
                     └─ g5d dispatch
                          ├─ RPC: fs.*, git.*, proc.*, agent.*, events.*, pty.*, ports.*, preview.*, checkpoint.*, session_files.*
                          ├─ background: checkpoint task (every 10 min when changed), event journal
                          ├─ chat turn  ─► claude -p --resume <id>  |  codex exec resume <id>   (exits at turn end)
                          ├─ agent task ─► same runner, task prompt
                          ├─ preview    ─► user's dev server on port N
                          └─ terminal   ─► PTY shell
                 /home/agent/.claude, /home/agent/.codex, /var/lib/g5d   (outside every working tree)
```

### 4.3 Invariants

1. **Two sources of truth.** Files: git in the internal store. Metadata, chat
   messages and timeline, and provider session files: Postgres. The VM disk is
   a cache.
2. **One workspace, one branch, one folder, one VM incarnation, one
   dispatcher.** The `workspace_vms` row with a host lease and the current
   `vm_id` is the only custody record, and every boundary rejects a `vm_id`
   that is no longer current (§6.8).
3. **One write path for files.** All file writes happen inside the VM: agents
   natively, the UI through the dispatcher. The API never writes a working tree.
4. **Branch protection lives in receive-pack.** Protected refs accept updates
   only from the API's merge operation after approval checks.
5. **A task is a process; a turn is a process.** State derives from the process
   and its exit. No leases, claims, or per-task heartbeats. G5 does not track
   what the provider does internally; the provider exits only when its work is
   done. Execution identity is `(task_id, attempt, vm_id)`; starts are
   confirmed before they count as executions (§7.6).
6. **Association check before UI-triggered operations.** The dispatcher
   verifies every member repo has the workspace branch checked out before
   Save, Run, Preview, Commit, Push, or PR actions; otherwise
   `WORKSPACE_BRANCH_MISMATCH`.
7. **Stop is a barrier.** A VM is released only after every repository's
   checkpoint push and the session-file harvest have durably succeeded, and
   the draining incarnation stays authorized until then (§5.7).
8. **Projections converge from git.** Events are hints; every projection can
   be rebuilt from authoritative OIDs without an event (§9.6).
9. **Nothing crosses the VM boundary at-most-once.** Starts, answers, events,
   and outcomes are all persisted on one side and confirmed by the other;
   duplicates are detected by key, never by ordering (§7.6).

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
and carries no authoritative head; `head_sha_cache` is a projection (§9.6). A
workspace spans N repos (code repos plus the org docs repo) with the same
branch name in each; pushes are per repo, like a laptop with several clones.
There are no job or task branches, and no workspace status column: a workspace
either exists or is deleted, and its VM state lives in `workspace_vms`.
`vm_id` is the current incarnation; it changes on every attach and is cleared
at release.

### 5.2 Branch naming and identities

- Workspace branch: `<slug>-<8 hex of workspace id>` in every member repo.
- Base workspace: `branch_name` is the repo's default branch, which is in
  `protected_branches`. It gets a VM like any other so users can browse and
  chat on `main`, but nothing can push to it; changes require a fork.
- Identities seen by receive-pack: **user** (workspace member with write, JWT
  as today); **workspace VM** (`G5D_WORKSPACE_TOKEN`, a JWT with claims
  `workspace_id` and `vm_id`, minted at each attach, TTL = idle timeout plus
  margin, renewable by the dispatcher only while its `vm_id` is current);
  **API server identity** (merges and forks, in-process).

### 5.3 Receive-pack policy (the git boundary)

`routes/git.rs` keeps its shape: parse ref updates, decide in Rust, spawn
`git receive-pack --stateless-rpc`. The staging-ref rules are replaced by:

| Ref | user (member, write) | workspace VM token (current `vm_id`) | API server identity |
|---|---|---|---|
| `refs/heads/<protected>` | deny | deny | merge operation only (§10.6) |
| `refs/heads/<workspace branch>` | allow FF; allow rewrites (see below) | same | fork creation only |
| `refs/heads/<other workspace's branch>` | deny | deny | n/a |
| `refs/g5/checkpoints/<workspace id>` | deny | allow force-update | n/a |
| delete of a workspace branch | deny | deny | workspace deletion |
| tags, other namespaces | deny | deny | n/a |

A token whose `vm_id` is not the workspace's current incarnation is rejected
before any ref is considered; a draining incarnation (`stopping`) is still
current (§6.8).

**Rewrites of the workspace branch.** Agents rebase onto `main`, so the
workspace branch must accept non-fast-forward updates from the workspace's
own identities. What the server enforces is exactly git's update-command
check: the `old_sha` the client sends must equal the ref's current value, and
a zero `old_sha` is refused on an existing ref. This rejects a ref that moved
during push negotiation and it makes `--force-with-lease=<branch>:<expected>`
work as designed. It does **not** prove that the pusher incorporated the
previous head: a plain `git push --force` sends the head it just learned
during negotiation and passes the same check. Workspace branches therefore
permit arbitrary rewrites by their owners, and lease safety is a client
obligation: the agent profiles require `--force-with-lease` and forbid bare
`--force`. In the shared-folder model every agent in a VM works in one local
clone, so a rewrite that discards an unseen commit can only happen between
the VM and a user pushing from a laptop, which is the laptop semantics the
user asked for. The platform's own background work never updates the agent's
remote-tracking refs (§5.4), so the implicit lease reflects only what the
agent itself fetched.

`git-upload-pack` requires read access as today. After a successful push the
handler publishes `branch.updated {repo_id, ref, old, new, actor}` on the
event bus as a hint; projections converge from git regardless (§9.6).

### 5.4 Checkpoint and restore

The checkpoint records uncommitted changes and unpushed local commits in
internal git so a later VM can resume. It is produced by an **independent
background task** in the dispatcher, not by the stop path.

**The checkpoint task.** A cron-like loop in the dispatcher wakes every 10
minutes (per-org override). If neither the working tree nor the local `HEAD`
of any member repo changed since the last successful checkpoint, it does
nothing. Otherwise it checkpoints each changed repository as described below,
records `last_checkpoint_at`, and reports `checkpoint.updated` to the API. It
runs concurrently with tasks, chats, previews, and terminals; a checkpoint
reads the working tree and pushes, and never modifies the working tree, the
real index, `HEAD`, or any remote-tracking ref. The `checkpoint.run` RPC
triggers the same task immediately (used by "Save all and stop", by the stop
barrier, and before host rotation). Runs are serialized by a mutex; a request
arriving during a run waits for it and then runs again only if something
changed after that run began.

**What, per code repo.** Three facts are recorded: the internal branch head
observed at checkpoint time, the local `HEAD` (which may be ahead of, equal
to, or diverged from that head, because agents commit locally before pushing),
and the working-tree snapshot.

- **Observation without side effects.** The internal head is read with
  `git ls-remote origin refs/heads/<branch>`. The checkpoint task never runs
  `git fetch`: a fetch would update `refs/remotes/origin/<branch>`, which is
  the expected value behind an agent's implicit `--force-with-lease`, and a
  background fetch would silently authorize a rewrite that discards a commit
  the agent never saw. Only the user's "Update from origin" (`git.fetch_rebase`)
  and the agent's own `git fetch` move that ref, as on a laptop.
- **Snapshot.** A commit `C` whose tree is the working tree and whose parent is
  the local `HEAD`, built with a temporary index that is **seeded from the
  real index** so that tracked files stay tracked even when an ignore rule
  matches them:

  ```
  cp -p .git/index $TMP_INDEX                       # tracked-path state, timestamps preserved
  GIT_INDEX_FILE=$TMP_INDEX git update-index --really-refresh   # re-hash content; do not trust the stat cache
  GIT_INDEX_FILE=$TMP_INDEX git add -A              # updates tracked files, adds untracked non-ignored, drops deleted
  GIT_INDEX_FILE=$TMP_INDEX git write-tree          # -> tree
  git commit-tree <tree> -p HEAD -m "<message>"     # -> C
  git push --force origin C:refs/g5/checkpoints/<workspace id>
  ```

  Ignore rules and the platform excludes file (§6.3) suppress only untracked
  files; a tracked file matched by `.gitignore` is captured with its current
  content. The real index, `HEAD`, and the working tree are untouched. Because
  `C` descends from the local `HEAD`, the push carries every unpushed local
  commit with it. The commit message records branch, observed internal head,
  local `HEAD`, timestamp, and the staged/unstaged path summary. Ignored files
  (build outputs, `node_modules`, `target/`) and the docker scratch disk are
  cache and are not checkpointed. The workspace branch itself is never polluted
  by checkpoints.

**Restore on resume.**
1. Clone or reflink each member repo from the internal store (the host's bare
   cache stays as an optimization) and fetch the workspace branch.
2. Fetch the checkpoint ref. If absent, check out the workspace branch at the
   internal head; done.
3. Otherwise set the local branch to `C^` (the local `HEAD` at checkpoint
   time): `git checkout -B <branch> <C^>`, with upstream `origin/<branch>`.
   This reproduces committed-but-unpushed history exactly.
4. Materialize `C`'s tree into the working tree without moving `HEAD`
   (`git restore --source=<C> --worktree -- .` plus untracked files from `C`
   that are absent from `HEAD`), leaving changes unstaged.
5. If the internal head moved while the VM was down (a user pushed from a
   laptop, or a stale push arrived before the fence closed), the local branch
   is now behind or diverged from `origin/<branch>`. Nothing is merged
   automatically; the status strip shows ahead/behind and "Update from
   origin" runs `git.fetch_rebase`, or the agent rebases natively, as on a
   laptop.

**Guarantee.** Work committed and pushed by an agent is never lost. Work
committed locally but not pushed, and uncommitted edits, survive every
planned stop (§5.7) and survive an unplanned VM loss up to the last
background checkpoint. Provider session files are not on the VM's critical
path: they are in Postgres (§8.4).

### 5.5 Provider integration (GitHub / GitLab)

- **Inbound.** For each connected repo the API fetches the protected branches
  from the provider into the mirror on webhook and on a periodic poll (default
  5 minutes). Inbound updates to a repository's protected refs are serialized
  with merge operations on that repository (§10.6). Workspace branches are
  never fetched from the provider.
- **Outbound.** The API pushes a workspace branch to the provider only on
  explicit actions: PR publish (§10.2), "Publish branch", and the republish
  step of a merge operation (§10.6), using the acting user's stored token and
  `--force-with-lease`, same branch name.
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

### 5.7 Workspace actions and the stop barrier

A workspace has exactly four user-visible actions. There is no archive.

| Action | Steps | Result |
|---|---|---|
| **create** | fork the source branch head into `<slug>-<8hex>` in every member repo (server identity); insert `workspaces` and `workspace_repos`; optionally carry chats (§8.5); attach a VM with a fresh `vm_id` (`workspace_vms` → `starting`); dispatcher restores (§5.4) | running workspace on a fresh branch |
| **stop** | the stop barrier below; then stop the VM and release the host slot (`workspace_vms` → `stopped`) | branch, checkpoint, chats, session files, and settings remain; reads come from git (§9.1) |
| **resume** | attach a VM with a fresh `vm_id` on any host with a free slot; restore files from the branch and checkpoint (§5.4); session files restore lazily on the first chat turn (§8.4) | running workspace with the last checkpointed state |
| **delete** | stop if running (force variant permitted); delete the workspace branch and checkpoint ref in every repo; delete chats, session files, tasks, and rows, as today | gone |

**The stop barrier.** Stop (explicit, idle, or host drain) is an ordered
sequence, and the VM is released only when every step has durably succeeded.
Throughout the barrier the incarnation remains **current**: its `vm_id` is
unchanged, its lease keeps renewing, and every boundary in §6.8 accepts it.
What closes is admission of new user work, not the incarnation's authority.

1. **Close admission.** `workspace_vms.status` → `stopping`. New Save, run,
   preview, terminal, and chat-turn requests for the workspace are refused
   with `WORKSPACE_STOPPING` and the UI offers "Resume". File mutations that
   were already admitted (`fs.write` calls in flight) are allowed to complete;
   the dispatcher's `admission.close` returns only when none is in flight.
2. **Quiesce.** Idle stop by definition has no running task and no open
   question. Explicit stop and host drain cancel running tasks (SIGINT,
   SIGKILL after 15 s); their turns end with outcome `interrupted` (§7.2),
   their outcomes and remaining events are reported and acknowledged through
   the normal contract (§7.6), and tasks with `retry_on_vm_loss` are
   re-queued. Preview processes and terminals are closed.
3. **Final checkpoint.** Wait for any in-flight run of the background
   checkpoint task, then run it once more (`checkpoint.run`) so that the
   snapshot reflects the quiesced tree, and wait for every repository's push
   to be acknowledged by the internal store. If any repository's push fails,
   the stop **fails**: `workspace_vms.status` returns to `running`, admission
   reopens, the failure is recorded in `last_error` and shown to the user,
   and the stop is retried with backoff. Host drain waits for success. An
   operator force-stop is a separate, explicit action that records the loss.
4. **Harvest.** Session files changed since the last harvest are uploaded to
   Postgres (§8.4) and acknowledged; the event journal is drained and
   acknowledged.
5. **Release.** The row becomes `stopped` with `vm_id` cleared in one update;
   from this point the old incarnation is stale and every boundary rejects it
   (§6.8). The host stops the VM and frees the slot.

Idle timeout (default 30 minutes, per-org default with workspace override)
performs stop. Any wake trigger (chat message, task run, Save, preview open,
terminal open, explicit Resume) performs resume. Host drain performs stop for
every VM on the host. Activity that keeps a VM awake: running tasks, an open
question on a `waiting` task (§7.2), an open terminal, preview traffic, file
RPC. An open browser tab does not.

---

## 6. Workspace VM and dispatcher

### 6.1 VM lifecycle

```
stopped ──resume──► starting ──restored──► running ──stop/idle──► stopping ──barrier done──► stopped
                      │                       │                      │
                      └──boot failure─────────┴──lease lost──────────┴──lease lost──► failed ──next resume──► starting
```

| Transition | Actor | Details |
|---|---|---|
| resume | API, on any wake trigger, dispatch demand (§7.3), or explicit Resume | one compare-and-swap update on `workspace_vms` sets `status=starting`, a new `vm_id`, the chosen `host_id`, and a lease; send `vm.start(vm_id)` |
| restored | dispatcher reports ready with its `vm_id` | restore done; RPC listening; checkpoint task started |
| stop / idle | user, idle reconciler, or host drain | the stop barrier (§5.7); the incarnation stays current until release |
| lease lost | reconciler: host heartbeat missing beyond 3 intervals, or the lease not renewed, in any of `starting`, `running`, `stopping` | VM `failed`; confirmed-running tasks `failed(vm_lost)`; the old `vm_id` is now stale; next resume restores from the last checkpoint on another host |

### 6.2 Host daemon (`g5d host`)

Retained: registration and heartbeat, libkrun boot (`vm-exec`), OCI rootfs
templates and COW clone, XFS reflink bare-repo cache, port allocator and VMM
loopback addressing, passt networking, the outbound tunnel and stream
multiplexer, the LLM proxy, boot-time attestation, `G5D_HOST_MAX_MICROVMS` as
the number of equal-size VM slots.

New: a command channel on the tunnel. The API sends `vm.start(workspace_id,
vm_id, spec)`, `vm.stop(workspace_id, vm_id, checkpoint)`, `vm.status`, and
opens mux streams tagged `rpc`, `pty`, `http:<port>`, `llm` for a given VM.
The host maps each VM to its TSI port and per-VM token, as preview-attach
connects to the guest listener today. On (re)connect the host reports every
VM it is running with its `vm_id`; the API answers with the set of current
incarnations (including draining ones) and the host stops any VM not in it
(§6.8). The host also stops a VM locally when the VM's lease (renewed through
the host) expires.

Removed: poll/claim/ack/nack/observation, per-task `Materializer`, golden
lanes, parked runtimes, discard fence, sticky heartbeat riders, deferred and
superseded assignment bookkeeping, the separate preview registry, per-task
attestation, zero-slot polling.

Placement: any host with a free slot in the org's region, preferring the host
that last ran this workspace (warm bare cache and, when still present, the
VM disk).

### 6.3 Dispatcher (`g5d dispatch`, inside the VM)

Boot: mount `/work`, create `/home/agent` and `/var/lib/g5d`, restore (§5.4),
start the RPC listener on the TSI port with the per-VM token, start the file
watcher and the background checkpoint task, open the event journal, report
`ready(vm_id)`.

Background tasks owned by the dispatcher: the checkpoint task (§5.4), the
file watcher feeding `file.changed`, the port scanner feeding `port.opened` /
`port.closed`, token renewal, and the event journal (§7.6).

RPC surface (JSON-RPC over the `rpc` mux stream; events are pushed on the same
stream and journaled):

| Group | Methods | Used by |
|---|---|---|
| `fs` | `list`, `read`, `stat`, `write(path, content, expected_hash)`, `delete`, `rename`, `watch` → `file.changed` | file tree, editors, Save |
| `git` | `status`, `diff(path?)`, `log`, `assert_branch`, `switch_back`, `commit(message, paths)`, `push`, `fetch_rebase(base)` | Changes panel, Commit/Push, association check |
| `proc` | `spawn(exec_id, argv, env, cwd)` → disposition, `signal`, `list`, `wait`; streams `stdout`, `stderr` | command tasks (bootstrap, custom) |
| `agent` | `run_turn(exec_id, provider, provider_session_id?, prompt, attachments, options)` → disposition, then events | chat turns, agent tasks (§8.3) |
| `events` | `replay(exec_id, from_seq)`, `ack(exec_id, up_to_seq)`; `outcomes.pending()` | dispatch and event contract (§7.6) |
| `pty` | `open(cols, rows)`, `write`, `resize`, `close` | terminal |
| `ports` | `list`; events `port.opened`, `port.closed` | Ports panel, preview auto-detect |
| `preview` | `start(profile)`, `stop`, `status` | Preview button |
| `session_files` | `restore(provider, session_id, files[])`, `harvest(provider, session_id)` → changed files | provider session persistence (§8.4) |
| `checkpoint` | `run` (trigger the background task now and wait), `last` | stop barrier, "Save all and stop", host rotation |
| `admission` | `close` (returns when in-flight mutations finish), `open` | stop barrier step 1 |
| `health` | `ping`, `resources` | reconciler |

`exec_id` is `(task_id, attempt, vm_id)` serialized. `spawn` and `run_turn`
are idempotent on it and return a **disposition**: `started` (a process was
launched by this call), `already_running`, or `already_finished` with the
outcome (§7.6). There is no `agent.answer`: answers reach a blocked process
through the MCP server's polling of committed state (§8.3).

Process supervision: the dispatcher keeps an in-memory process table and an
on-disk journal under `/var/lib/g5d` holding, per execution, every event and
the terminal outcome until the API acknowledges them (§7.6). If the
dispatcher crashes inside the VM, the host detects the listener loss and the
API marks that VM's confirmed-running tasks `failed(vm_lost)` and restarts the
VM with a new incarnation (restore is idempotent; the journal of a dead
incarnation is discarded with its disk).

Identity and credentials in the VM: `G5D_WORKSPACE_TOKEN` (claims
`workspace_id`, `vm_id`; internal git push, `g5d` platform commands),
`G5D_API_URL`, the LLM proxy endpoint, the provider CLIs on `/opt/g5/bin`. No
provider tokens, no host token, no per-task token.

Paths: `/work/<root_path>` per member repo; provider state in
`/home/agent/.claude` and `/home/agent/.codex`; dispatcher state in
`/var/lib/g5d`. None of these is inside a working tree. A platform-managed
excludes file (`core.excludesFile=/var/lib/g5d/excludes`, listing `.g5/` runtime
subdirectories such as `.g5/cache/`) is applied to the checkpoint task and to
agent git invocations, so neither a checkpoint nor a native `git add -A`
captures runtime files even in a repository with no `.gitignore`. Excludes,
like `.gitignore`, affect only untracked files (§5.4).

Concurrency: every process runs in the same folder on the workspace branch.
There is no worktree, branch, or lock per task. Concurrent edits to one file
have lost-update semantics: the last writer wins, earlier edits to that file
are overwritten, and git shows only the final content as one modified file.
Git raises a conflict only at rebase or merge time. Agents are expected to be
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
stays. Rotation becomes "drain: stop every VM through the barrier, then
replace". The 24-hour host JWT is replaced by a renewable host credential so
long-lived VMs do not force rotation. Org quotas: max concurrently running VMs.

### 6.7 Terminal

A browser terminal (xterm.js) over the `pty` stream, gated by workspace
access. The PTY manager and the multiplexer exist today; the UI panel and the
channel are new.

### 6.8 Incarnation fence and reconnect

Loss of contact with a host does not prove that its VMs stopped. The fence
makes a superseded VM harmless even if it later reconnects, while a VM that
is legitimately draining keeps the authority it needs to finish:

- **Incarnation.** Each attach mints a new `vm_id`. `workspace_vms.vm_id` is
  the single current incarnation, set in the same compare-and-swap update that
  places the VM and grants its lease, and cleared only at release (§5.7 step
  5) or when the reconciler marks the VM `failed`. An incarnation is
  **current** while the row holds its `vm_id`, in any of `starting`,
  `running`, or `stopping`.
- **Credential.** `G5D_WORKSPACE_TOKEN` carries `vm_id`. Renewal succeeds only
  while `vm_id` is current; a superseded VM's token expires within its TTL and
  cannot be renewed.
- **Boundaries that check the fence.** Git receive-pack and upload-pack;
  `chat_session_files` harvest; event replay and outcome reports;
  acknowledgements; `fs`/`git` RPC results returned to the API (the API drops
  replies from a stale `vm_id`); the host-side LLM proxy (requests from a VM
  whose `vm_id` is not current are refused, so a partitioned VM cannot keep
  spending). Session-file harvests from a stale incarnation are rejected, so
  an older transcript can never overwrite a newer one. All of these accept a
  draining incarnation.
- **Reconnect.** When a host reconnects it reports `[(workspace_id, vm_id)]`
  for every VM it runs. The API replies with the current incarnations,
  including draining ones; the host kills any VM not listed (no checkpoint:
  its work was already superseded by a replacement that restored from the
  last durable checkpoint, and its unpushed work is lost, which is the
  documented VM-loss case). A host that cannot reach the API stops each VM
  locally when that VM's lease expires.
- **Replacement admission.** The reconciler marks a VM `failed` and clears
  `vm_id` and `host_id` only after the lease has expired, whatever the status
  was; a resume then places a new incarnation. Because the lease is the same
  value the host enforces locally, the old VM has stopped itself by the time a
  replacement can be placed, and the boundaries above cover the case where it
  did not.

---

## 7. Task lifecycle

### 7.1 Model

```
tasks(id, org_id, workspace_id, job_id NULL, kind chat_turn|agent|command,
      status queued|running|waiting|succeeded|failed|cancelled,
      spec jsonb,        -- provider, model, permission_mode, prompt or argv, profile, env, retry_on_vm_loss, wall_clock_limit
      outcome jsonb,     -- exit code, summary, commit shas, usage, error, interrupted flag
      attempt int, vm_id NULL, start_confirmed bool, depends_on uuid[],
      chat_session_id NULL, created_by, created_at, started_at, ended_at,
      acked_event_seq bigint)

jobs(id, org_id, workspace_id, kind, status (derived), spec jsonb, created_by,
     max_concurrency, created_at, ended_at)          -- optional grouping for DAG runs

task_events(id bigserial, task_id, vm_id, seq bigint, ts, kind, payload jsonb,
            UNIQUE (task_id, vm_id, seq))                    -- today's timeline_entries; the agent timeline

task_questions(id, task_id, exec_id, kind question|approval|escalation, payload jsonb,
               status open|answered|expired|cancelled, answer jsonb,
               expires_at, answered_by, answered_at, consumed_at,
               successor_task_id NULL UNIQUE)                -- one HITL table

task_attempts(id, task_id, attempt, vm_id, started_at, ended_at, outcome,
              reason exit_code|provider_error|vm_lost|timeout|cancelled|interrupted)
```

Job kinds are profiles over the same task machinery (`uplift`, `downlift`,
`sync`, `explainer`, `agent_review`, `custom`). All jobs of a workspace run in
that workspace's VM, in the same folder. A single agent run is a task without
a job.

### 7.2 State machine

```
queued ──start confirmed by the dispatcher──► running
running ──process exit 0 / outcome ok──► succeeded
running ──exit ≠ 0 / provider error──► failed
running ──question or approval posted──► waiting ──answer consumed──► running
running | waiting | queued ──cancel──► cancelled
running | waiting ──stop or drain──► failed(interrupted)  [re-queued once if spec.retry_on_vm_loss]
running | waiting ──vm lost──► failed(vm_lost)            [re-queued once if spec.retry_on_vm_loss]
```

Six states, one writer: `TaskService::apply(event)`, with events from the
dispatcher (`started`, `question`, `answer_consumed`, `exited`), from users
(`cancel`, `answer`), from the stop barrier (`interrupted`), and from the VM
reconciler (`vm_lost`). `running` is recorded when the API sends a start
(§7.3); the task counts as an execution only once the dispatcher has
confirmed it (§7.6).

Rules.
- **Waiting tasks keep the VM awake.** An open question is activity for the
  idle timer, bounded by `task_questions.expires_at` (default 60 minutes,
  per-org override). When the question expires the task fails with reason
  `timeout` and the VM may then idle-stop. A waiting task is therefore never
  suspended mid-turn by idle stop.
- **Answering** commits the answer to the question row first
  (`status=answered`, `answer`, `answered_at`), bound to the question's
  `exec_id`. Delivery is a read of that committed state by the blocked
  process's MCP server (§8.3); when the process has consumed it, the server
  records `consumed_at` and the task returns to `running`. No RPC is needed,
  so there is no transaction that has to span Postgres and a process.
- **Interruption.** Explicit stop and host drain end a running or waiting turn
  with outcome `interrupted`: SIGINT, then SIGKILL after 15 seconds; partial
  assistant output is kept and marked `stopped`; an open question stays
  answerable. When a question is answered but its execution can no longer
  consume it (the task is `interrupted`, `vm_lost`, or otherwise terminal, and
  `consumed_at` is null), the reconciler creates **exactly one** successor
  `chat_turn` task, recorded in `successor_task_id` (unique), whose prompt
  carries the answer and a note that the previous turn was interrupted while
  waiting for it. No transcript is resumed mid-turn. Delivery is therefore
  at-least-once: if a process consumed an answer and was interrupted before
  its consumption record landed, the successor turn repeats the answer with
  the note.
- **Cancel** is the same signal sequence with outcome `cancelled`; open
  questions are cancelled. A cancel that arrives before the start was
  confirmed marks the task `cancelled` and the pending start is never
  re-sent; a process the dispatcher later reports for a cancelled task is
  cancelled on reconciliation (§7.6).
- No automatic retry on agent-reported failure. Human retry creates a new task.
- Job status is a pure function of child states; cancel cascades; there is no
  pause and no rollback.
- The reconciler loop (§7.6) turns lost VMs into `failed(vm_lost)`, expires
  questions, creates successor turns, and enforces `wall_clock_limit` with
  reason `timeout`.

### 7.3 Dispatch path

`POST /workspaces/:id/tasks` inserts `queued` and returns. A dispatch loop
(every 2 seconds, and immediately after an insert, an answer, or a VM
becoming `running`) does the following, in this order, so that a stopped
workspace is woken by demand rather than skipped:

1. Select `queued` tasks whose dependencies are met, **regardless of VM
   state**, and group them by workspace.
2. For each workspace in the selection whose VM is `stopped` or `failed`,
   request **resume** (§5.7, §6.1). Nothing else happens for that workspace
   this iteration.
3. For each workspace whose VM is `running` with a free concurrency slot,
   take the next ready task and record `attempt`, `vm_id`,
   `status=running`, `start_confirmed=false` in one update, then send
   `agent.run_turn(exec_id, …)` or `proc.spawn(exec_id, …)`.
4. When the dispatcher's disposition arrives (`started`, `already_running`,
   or `already_finished`), record `start_confirmed=true` (and apply the
   outcome if already finished).
5. Tasks that are `running` with `start_confirmed=false` are re-sent with the
   same `exec_id` on every iteration and on reconnect until a disposition
   arrives; the call is idempotent, so a start that was delivered but whose
   reply was lost does not launch twice.

Workspaces whose VM is `starting` or `stopping` are skipped until the next
iteration. Events stream back on the channel; the API appends `task_events`
and publishes `task.event` / `task.status` with today's WebSocket event names.
Uplift's deterministic bootstrap runs as a `command` task in the VM
(`g5d bootstrap`) and submits child tasks through the API.

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

### 7.6 Dispatch and event contract

The contract that makes §7.2 hold across crashes and lost messages, in place
of leases and the lifecycle kernel. Every message that crosses the VM
boundary is persisted on one side and confirmed by the other; duplicates are
detected by key, never by ordering.

- **Execution identity.** `exec_id = (task_id, attempt, vm_id)`. The API sets
  it before sending any command; the dispatcher keys its process table and
  journal by it.
- **Two-phase start.** The API records `running, start_confirmed=false`
  before sending. `run_turn`/`spawn` are idempotent on `exec_id` and return a
  disposition: `started`, `already_running`, or `already_finished(outcome)`.
  The API records `start_confirmed=true` on any disposition. Until then the
  start is an outstanding intent that the dispatch loop and reconnect
  reconciliation re-send with the same `exec_id`. Only a **confirmed** start
  is an execution: "the dispatcher does not know this `exec_id`" means
  `vm_lost` for a confirmed start and "re-send" for an unconfirmed one.
- **Cancel before launch.** Cancelling a task with an unconfirmed start sets
  `cancelled`; the loop stops re-sending. If the start had in fact been
  delivered, reconciliation finds a process for a `cancelled` task and cancels
  it; a delayed start cannot resurrect the task because the API never re-sends
  for a terminal task.
- **Journaled events.** The dispatcher appends every event of an execution
  (tool calls, tool results, partial output, questions, exit) to an on-disk
  journal under `/var/lib/g5d/journal/<exec_id>`, with a per-execution
  monotonic `seq`, and streams them live. The API inserts events with the
  unique key `(task_id, vm_id, seq)` and ignores duplicates by that key; it
  keeps `tasks.acked_event_seq` as a cumulative watermark and acknowledges it
  to the dispatcher (`events.ack(exec_id, up_to_seq)`), which prunes the
  journal up to the watermark. There is no highest-seen gate: a late event with
  a lower `seq` is inserted if its key is new.
- **Replay.** On (re)connect the API sends its watermark per open execution;
  the dispatcher replays the journal from `watermark + 1` in order and queues
  live events behind the replay, so the timeline is complete and ordered even
  when the API was away during a tool call. Sequence spaces are per execution,
  so concurrent tasks never interleave. Journals are retained while
  unacknowledged; acknowledged journals of finished executions are pruned,
  oldest first, under a disk budget.
- **Retained outcomes.** The terminal outcome is the last journaled event and
  follows the same rule: the API persists the task transition and, for chat
  turns, the assistant message update in one transaction, then acknowledges
  through the watermark. A crash before acknowledgement replays; a crash after
  is harmless.
- **Reconnect reconciliation.** When the channel to a VM is (re)established,
  the API sends its watermarks and outstanding start intents; the dispatcher
  replays journals, returns dispositions for the intents, and lists its
  processes. Confirmed-running tasks the dispatcher does not know become
  `failed(vm_lost)`; processes for tasks the API holds as terminal are
  cancelled; unconfirmed starts are re-sent.
- **Reconciler loop (30 s).** Confirmed `running` or `waiting` tasks whose
  incarnation is no longer current (row `failed`, `stopped`, or a different
  `vm_id`) → `failed(vm_lost)`; a draining incarnation is current (§6.8).
  Questions past `expires_at` → `expired` and the task fails with `timeout`;
  answered questions with no consumer and no successor → one successor turn
  (§7.2); tasks past `wall_clock_limit` → cancelled with `timeout`; `queued`
  tasks and unconfirmed starts are re-offered to the dispatch loop.
- **Chat linkage.** A chat turn's `pending` assistant message is updated in
  the same transaction as its task's terminal state, so an API crash can never
  strand an assistant message in `pending` while the task is terminal, and the
  reconciler repairs any assistant message whose task is terminal.

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
                   bytes bytea, sha256, size, harvested_from_vm_id, updated_at,
                   PRIMARY KEY (session_id, provider_session_id, relative_path))
```

`author_id` is new: any workspace member may send into a session.

Chat history is a Postgres feature, as it is today. `chat_messages` holds the
conversation and `task_events` holds the agent timeline (tool calls, output,
questions, approvals) for every turn, complete even across API restarts
(§7.6). The browser reads both through the API, and neither a workspace VM
nor any worker is involved in reading them. A stopped or deleted VM never
affects what a user can see. `chat_session_files` is not shown to users; it
exists only so the provider can resume its own transcript (§8.4).

### 8.2 A turn

1. `POST /sessions/:id/messages` inserts the user message and a `pending`
   assistant message under a per-session advisory lock (one pending assistant
   per session) and creates a `chat_turn` task.
2. The dispatch loop resumes the VM if needed (§7.3) and calls
   `agent.run_turn(exec_id, provider, provider_session_id, prompt, attachments,
   options)`; the disposition confirms the start.
3. The dispatcher restores the session files if they are missing on the VM
   (§8.4), spawns the provider CLI (§8.3), journals and streams its events
   mapped to `chat.chunk`, `chat.message_updated`, `chat.done`, `chat.error`
   and to `task_events`, and waits for the process to exit.
4. Claude tool-permission prompts and both providers' user questions become
   `task_questions` rows; the process blocks inside the tool call until the
   committed answer is read (§8.3).
5. Process exit is the end of the turn. The dispatcher harvests changed session
   files into Postgres, journals the outcome, reports it, and nothing keeps
   running. There is no background work between turns: the provider exits only
   when its own subagents and tasks are done, and G5 does not track them.

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
      -c approval_policy="never" -c sandbox_mode="workspace-write" [-c model_reasoning_effort="<e>"]
      [-i <attachment>...]                      # prompt on stdin, then EOF
env: CODEX_HOME=/home/agent/.codex (config.toml with the g5-tools MCP server and its
     tool timeout, provider block pointing at the host LLM proxy), no OPENAI_* keys
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
env: CLAUDE_CONFIG_DIR=/home/agent/.claude, ANTHROPIC_BASE_URL=<host proxy>,
     MCP_TOOL_TIMEOUT=<question expiry + margin, ms>,
     placeholder ANTHROPIC_API_KEY as today, no real provider credential; runs as uid 1000
```

**Two kinds of human input, and which provider supports which.**

| Interaction | Mechanism | Claude | Codex |
|---|---|---|---|
| Agent asks the **user a question** (clarification, choice, escalation) | MCP tool `ask_user(question, options)` on the `g5-tools` server (also `g5d ask`). The tool posts `task_questions(kind=question)` through the API with the execution's `exec_id`, then **long-polls the API for the committed answer** and returns it as the tool result, recording `consumed_at`. It never touches the process's stdin, so it works under `codex exec` and `claude -p` alike. | supported | supported |
| Provider asks for **tool-permission approval** (its own "may I run this command / edit this file" prompt) | Claude: `--permission-prompt-tool mcp__g5-tools__request_permission`; the tool posts `task_questions(kind=approval)`, polls the same way, and returns `{"behavior":"allow"|"deny", "updatedInput":…, "message":…}`. Codex: `codex exec` emits `exec_approval_request` items but reads its prompt from stdin and requires EOF, so there is no channel to answer them; the current sidecar maps them to `CodexApprovalUnsupported` for exactly this reason. | supported | **not supported under `exec`**: Codex runs only policies that cannot prompt (`approval_policy=never` with `sandbox_mode=workspace-write`, or an operator-chosen non-prompting preset), which is today's effective behavior. Interactive Codex approvals are a deferred item (§17.2). |

**Answer delivery.** The API commits the answer to the question row before
anything else happens (§7.2). The `g5-tools` server, which is blocking inside
the tool call, polls `GET /questions/:id` with the workspace token until the
row is `answered`, returns the answer to the provider, and records
`consumed_at` (idempotent). Nothing is pushed from the API into the VM for
this; the committed row is the delivery. If the process is gone before
consumption, the reconciler creates exactly one successor turn (§7.2).

Because a question or approval may wait for a human up to the question
expiry, the MCP tool-call timeout of each CLI is configured above that bound:
`MCP_TOOL_TIMEOUT` for Claude, the `g5-tools` server's `tool_timeout_sec` in
the Codex `config.toml`. If the VM stops while a question is open, the turn is
interrupted (§7.2) and the answer, when it arrives, starts a new turn.

Runner behavior common to both:
- **Permission modes.** Chat turns use the server-frozen mode from
  `chat_sessions.permission_mode`. Non-chat tasks (uplift, downlift, sync,
  explainer, review, custom) run Claude with `bypassPermissions` and Codex
  with `approval_policy=never` / `sandbox_mode=workspace-write`, so they never
  block on a human for permissions; they may still ask questions through
  `ask_user` when their profile allows it.
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
  into journaled events (§7.6), `chat.chunk` deltas, `task_events` timeline
  entries, and the final assistant message with usage.
- **Cancel and interruption.** SIGINT to the process, SIGKILL after 15
  seconds; outcome `cancelled` or `interrupted` (§7.2).
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
is stored only in Postgres, in `chat_session_files`, and lives on the VM only
as a cache outside every working tree:

- **Claude.** Under `CLAUDE_CONFIG_DIR=/home/agent/.claude`, the transcript is
  `projects/<escaped cwd>/<session id>.jsonl` and subagent transcripts are
  `projects/<escaped cwd>/<session id>/subagents/agent-*.jsonl` plus
  `agent-*.meta.json` (layout verified locally). Because `cwd` is always
  `/work/<root_path>`, the project key is deterministic across incarnations.
- **Codex.** Under `CODEX_HOME=/home/agent/.codex`,
  `sessions/YYYY/MM/DD/rollout-<ts>-<session id>.jsonl` (today's
  `codex_session_artifacts` harvester already scans this layout).

After every turn, and again during the stop barrier, the runner harvests files
under the session's paths whose sha256 changed and upserts them
(zstd-compressed, size-capped at 128 MiB per file as today, tagged with the
harvesting `vm_id`; a stale incarnation's harvest is rejected). Before a turn
that passes `--resume`, if the files are absent on the VM (fresh incarnation),
the runner restores them from Postgres first. If Postgres has nothing for that
session id (never persisted, or the provider rejected the transcript), the
runner starts a new provider session and prefixes the prompt with a history
preamble built from `chat_messages`, then records the new
`provider_session_id`. Nothing about sessions is checkpointed into git, and
no session file can be captured by a checkpoint or a native `git add`
(§6.3).

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
  `stream-json` / `--json` frames captured from the real pinned CLIs. The fake
  Codex binary never emits approval frames, because the real one cannot be
  answered; both fake binaries exercise `ask_user`, and the fake Claude binary
  exercises `request_permission`, through the real MCP server. The runner does
  not know it is fake; the `fake_*.mjs` SDK fixtures are rewritten as scripted
  fake-CLI scenarios.

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
asserts the branch (§5.6), resumes the VM if it is stopped (or returns
`WORKSPACE_STOPPING` while a stop barrier is in progress), and calls
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

`docs`, `ontology_nodes`, and search remain projections of git files, rebuilt
by the convergence contract in §9.6. `docs.content`, `mapping_content`, and
`scope_mode` are dropped.

### 9.5 Presence and co-editing

None in this iteration. Because all writes go through one Save API, a later
document-only CRDT can be added without changing persistence.

### 9.6 Projection convergence

The event bus is a non-durable fan-out, and a ref can be updated immediately
before a crash. Projections therefore never depend on an event:

- Each projection records the OID it was computed from, per `(repo_id, ref)`:
  `projection_state(projection docs|ontology|search|head_cache|pr_head,
  repo_id, ref, applied_oid, applied_at)`.
- A convergence loop runs at API startup and every 60 seconds: it lists the
  authoritative OIDs of every workspace branch, checkpoint ref, and protected
  branch (`git for-each-ref` on the mirrors, cheap and local), compares them
  with `projection_state`, and re-projects what differs. Fork, merge, inbound
  fetch, checkpoint, and workspace deletion all change refs and are covered by
  the same comparison; deletion removes the projection rows for refs that no
  longer exist.
- `branch.updated` and `checkpoint.updated` events trigger the same work
  early for latency. Losing one changes nothing but latency.
- PR `head_sha` (§10.2) and `workspace_repos.head_sha_cache` are projections
  under this contract.

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
              provider_pr_number NULL, provider_pr_url NULL, published_sha NULL,
              merge_op_id NULL, merge_state NULL|pending|provider_merged|applied|failed,
              merge_source_sha NULL, merge_target_sha NULL, merge_result_sha NULL,
              merge_error NULL, created_by, created_at, merged_at, merged_by)
```

Created from the UI or by an agent with `g5d pr create`. Creation publishes the
source branch to the provider at the source OID read at that moment, records
it as `published_sha`, and opens a provider PR when the repo has a provider;
the provider PR is a mirror for external reviewers, not the source of truth
for merging. `head_sha` is a projection of the source branch (§9.6) and is
informational; every decision and the merge operation use OIDs they read from
git themselves.

### 10.3 Approvals

```
approval_rules(id, org_id, repo_id, target_branch, required_approvers uuid[],
               min_approvals int, agent_reviewer_profile NULL, created_by, created_at)

approval_decisions(id, org_id, pull_request_id, source_sha, actor_user_id NULL,
                   actor_task_id NULL, outcome approved|changes_requested, note,
                   created_at)
```

A decision binds to the exact `source_sha` the approver reviewed: the UI
records the OID it displayed, and an agent review task is launched with the
OID it must review and `g5d pr review` records that same OID. A new push makes
prior decisions stale automatically because the source OID no longer matches.
Approvers are the rule's list plus org admins.

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

### 10.6 The merge operation

`POST /pull-requests/:id/merge` is the only writer of protected refs. It is a
persisted, resumable operation with one identity (`merge_op_id`) and pinned
inputs that are kept until the operation is resolved; a retry of an
unresolved operation never re-pins. It is not a synchronous
merge-then-rollback.

1. **Pin.** If the PR has an unresolved operation (`merge_state` is `pending`
   or `provider_merged`), resume it at the step its state indicates; otherwise,
   under a per-repository lock that also serializes inbound fetches (§5.5),
   read the source and target OIDs from the mirror and store them with a new
   `merge_op_id` as `merge_source_sha` and `merge_target_sha`,
   `merge_state=pending`. Every later step uses these values; the mutable
   branches are not re-read.
2. **Authorize.** PR is `open`; the caller may merge; decisions whose
   `source_sha` equals `merge_source_sha` satisfy the rule.
3. **Mergeability.** Fast-forward or clean merge of `merge_source_sha` onto
   `merge_target_sha`, computed on the bare mirror without writing refs.
4. **Provider-connected target.**
   a. If `published_sha != merge_source_sha`, republish the source branch to
      the provider at `merge_source_sha` (`--force-with-lease`) and verify the
      provider PR head equals it.
   b. Call the provider merge API with the expected source SHA (`sha` on
      GitHub, `sha` on GitLab); the provider refuses a stale head. On success
      record `merge_result_sha` (the provider's merge commit) and
      `merge_state=provider_merged`. This state is durable: it survives every
      later failure.
   c. Fetch the target branch from the provider into the mirror (the normal
      inbound path, under the same lock). The mirror never computes its own
      merge commit for a provider-backed target, so squash, rebase, and merge
      methods all produce exactly the provider's result. An import failure
      leaves `provider_merged` in place and is retried with backoff; it is
      never treated as a fresh merge. On success record `merge_state=applied`,
      `status=merged`, `merged_at`, `merged_by`.
   d. Ambiguous responses (timeout, lost reply, crash between b and c) are
      reconciled on retry by reading the provider PR. A remotely merged PR is
      attributed to this operation **only if** the provider reports the pinned
      `merge_source_sha` as the merged head; then continue from c with its
      `merge_commit_sha`. If it reports open with the same head, retry b. If
      its head changed, or it was merged at a different head, record
      `merge_state=failed` with `merge_error` so the user re-runs the merge,
      which pins fresh inputs.
5. **Local-only target.** Compute the merge commit on the mirror, write it to
   the object store, and persist its OID as `merge_result_sha` **before**
   updating the ref. Then update the target ref with a compare-and-swap on
   `merge_target_sha`. On recovery, a target already equal to
   `merge_result_sha` means the update succeeded before the crash: record
   `applied`. A CAS failure against any other value (inbound update or another
   merge in between) records `failed` for a re-run with fresh inputs.
6. **Failure.** Terminal `failed` is recorded only for a known rejection or
   conflict **before** anything was applied (steps 2 to 4b, or a CAS loss in
   step 5 against a foreign value). Once the provider has merged, or the local
   ref holds `merge_result_sha`, the operation can only progress to `applied`.
   There is no rollback because nothing that was applied needs undoing.

PRs may also target another workspace's branch; those targets are not
protected, so the operation follows step 5 and skips the provider.

---

## 11. Agent surface

### 11.1 Keep (platform operations)

| Command / tool | Purpose |
|---|---|
| `g5d pr create|status|review` | open a PR, read its state, record an agent review decision pinned to the reviewed SHA |
| `g5d ask` and MCP `ask_user` | post a `task_questions` question and block until the committed answer is read (both providers) |
| MCP `request_permission` | Claude's permission-prompt tool; posts an approval question and blocks until answered |
| `g5d task status|list` | inspect tasks and jobs |
| `g5d preview start|stop|status`, `g5d ports` | control the preview process, list open ports |
| `g5d auth`, `g5d doctor` | CLI login for laptops and diagnostics |
| MCP `g5-tools` grounding and ontology tools (`validate_spec`, `read_ontology`, ledgers) | unchanged; the rebase-pause and CRDT-peer couplings are removed |

### 11.2 Native

Git (`commit`, `push`, `pull`, `rebase`, `fetch`), files, shell, tests,
package managers. `origin = <api>/git/<repo_id>` with the workspace token in
the git credential helper and the platform excludes file configured. Agents
are told: stay on the branch; other agents may be working in the same folder
and the last writer wins; commit often; push to `origin` when a unit of work is
done; rebase onto `origin/main` to update; push rewrites only with
`--force-with-lease`, never bare `--force`, and when rewriting after a fetch
prefer the explicit form `--force-with-lease=<branch>:<expected sha>`; never
push to `main` (the server rejects it anyway). The platform never moves the
agent's remote-tracking refs behind its back (§5.4), so the implicit lease
means what the agent last saw.

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

As in §8.3: one CLI process per turn, `cwd` = `/work/<root_path>`, provider
state under `/home/agent/.claude` and `/home/agent/.codex` (VM-local cache of
what Postgres holds, outside every working tree), `ANTHROPIC_BASE_URL` /
Codex provider block at the host proxy, no provider keys or tokens in the VM.
`G5D_WORKSPACE_TOKEN` (with `vm_id`) is the only credential. The LLM proxy
keeps metering usage and capturing provider quota headers, and refuses a
`vm_id` that is not current.

### 11.5 Profiles

`CLAUDE.md` and `AGENTS.md` are rewritten around native git, the branch rule,
the shared-folder last-writer-wins rule, the `--force-with-lease` rule,
`g5d pr`, and `ask_user`. The shipped `/commit` and `/pr` commands become the
reference behavior with `gh pr create` replaced by `g5d pr create`. Hooks
reduce to `SessionStart` and `Stop`. CRDT-era skills (`g5-coordinated-git`,
`g5-rebase-workflow`, `g5-provided-agent-workspace`) and the issue-reporting
skill are deleted or rewritten.

---

## 12. Data model

Fresh baseline; names are proposals.

**Identity and tenancy:** `orgs`, `users`, `org_memberships`,
`org_invitations`, `user_scm_connections`, `api_keys`, `refresh_tokens`,
`oauth_states`, `cli_device_authorizations`, the `authz` schema,
`self_hosted_licenses`.

**Workspace and git (§5):** `repos`, `repo_secrets`, `workspaces`,
`workspace_repos`, `workspace_members`, `workspace_settings` (idle timeout,
question expiry, checkpoint interval, defaults), `workspace_vms`,
`projection_state`.

**Execution (§6, §7):** `worker_hosts` (id, org, kind cloud|self_hosted|dev,
status, slots, heartbeat, image), `tasks`, `jobs`, `task_events`,
`task_questions`, `task_attempts`, `usage_records`, `provider_accounts`,
`provider_quota_snapshots`.

**Chat (§8):** `chat_sessions`, `chat_messages`, `chat_attachments`,
`chat_session_files`.

**Governance (§10):** `pull_requests` (with the merge-operation columns),
`approval_rules`, `approval_decisions`.

**Projections and product features:** `docs`, `ontology_nodes`,
`node_dependencies`, `ontology_grounding`, `search_documents`,
`search_reindex_queue`, `comment_index`/`comment_replies`/`comment_mentions`,
`notifications`, `user_preferences`, one `configs(scope, scope_id, key, value)`
table for org/user/workspace config, `platform_issue_outbox`.

About 36 tables.

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
tables.

Provider enum everywhere: `claude_code | codex`.

Migrations: the 292 files are replaced by a single `001_lean_baseline.sql`,
edited in place until the first production deployment; normal forward
migrations resume after that.

---

## 13. Incremental plan

Each phase ends with Cloud Build green, the Rust end-to-end crate and the
Playwright flows adjusted to the removed surface, the acceptance tests listed
for it in §15 passing, and a demo of the user-visible path.

**Phase 0 — Freeze and baseline (1 week).** Snapshot the current schema as the
working baseline migration; tag the last pre-lean commit; record the survey
sizes as the "before" number.

**Phase 1 — Git boundary (2 to 3 weeks).** Git refs authoritative; receive-pack
policy table (§5.3); `projection_state` and the convergence loop (§9.6) with
`branch.updated` as a hint. Delete the head-operation registry, staging refs,
finalizer, advance notifier and consumer, remote-tracking pipeline, DB-owned
head columns, job and task branches. Workers push natively with their task
token as an interim identity. Exit: a task's commit lands on the workspace
branch through plain `git push`; a push to `main` is rejected; a fork creates a
branch server-side; a ref update with its event dropped converges.

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
`request_permission` and `ask_user` MCP tools with answer polling and
tool-call timeouts above the question expiry; `chat_session_files` harvest
and restore (§8.4) with paths outside the working tree; fake `claude` and
`codex` binaries for tests. Delete the Claude SDK sidecar, the Codex Node
sidecar, session-store adapter and proxy, `claude_session_store_entries`,
`codex_session_artifacts`, the `naive_react` and `fake` provider values, and
the SDK-emulating fixtures. Exit: a Claude chat and a Codex chat each run
several turns with `--resume` continuity across a VM replacement; Claude
permission prompts and both providers' user questions are answered from the
UI, including an answer that arrives after an API restart; Codex runs only
non-prompting policies; history is readable with no VM.

**Phase 4 — Workspace VM and dispatcher (4 to 6 weeks).** `g5d dispatch`
(RPC, process supervision, the event journal, the background checkpoint task
with the seeded-index recipe and `ls-remote` observation, restore with the
local commit chain), `g5d host` VM lifecycle commands and reconnect reporting
over the tunnel, `workspace_vms` with incarnations and the fence (§6.8), the
four workspace actions and the stop barrier with its authorization window
(§5.7), the dispatch loop with wake-on-demand and two-phase starts (§7.3), the
dispatch and event contract (§7.6), the reconciler; chat turns and agent tasks
through `agent.run_turn`. Delete the per-task materializer, rootfs-per-task,
handoff file, poll/claim/ack, sticky runtimes and every rider, golden images,
worker pins. Exit: two chats run concurrently in one workspace VM; a message
to a stopped workspace resumes it and runs; the VM stops after idle and a new
message resumes it with unsaved edits and unpushed commits restored from the
checkpoint and the provider session restored from Postgres; killing the host
mid-turn fails the turn and the next turn recovers on another host; a
partitioned host that reconnects has its stale VM stopped and its late pushes
refused; an API restart mid-tool-call leaves a complete timeline.

**Phase 5 — Preview and terminal (1 to 2 weeks).** `preview.start`, `ports`,
workspace-keyed relay and ticket, Ports panel, xterm terminal. Delete the
preview job type, sessions table, reaper, attach machinery.

**Phase 6 — Governance (2 to 3 weeks).** `pull_requests` with the resumable
merge operation (§10.6), `approval_rules`, `approval_decisions` pinned to
source OIDs, `g5d pr` with SHA-pinned reviews, protected-branch verification
at repo connect, simplified PR and approvals UI. Delete the approval ledger,
job merge, coordinated publication, PR provider lifecycle tables, mutation
policy, scope grants.

**Phase 7 — Task lifecycle and schema squash (2 weeks).** Six-state model with
the `interrupted` outcome and one reconciler; HITL unified on
`task_questions`; one status enum each for DB, Rust, and frontend; write
`001_lean_baseline.sql` and delete the old migrations.

**Phase 8 — Approved removals, infra, CI (ongoing).** Remove issue tracking,
the IDE mockup, the hidden settings UI, takeover remnants, and non-GitHub
platform login across frontend, routes, services, tools, scripts, Justfile,
and infra; drop the dormant Terraform host MIG, customer image lanes, and
disabled GitHub workflows; keep the Azure canary lane; consolidate crates;
revisit CI last (Cloud Build stays).

Rough total: 18 to 25 weeks for one team; Phases 1, 2/3, and 4 are the
critical path. The estimates become commitments only after the acceptance
tests in §15 for Phases 1, 3, 4, and 6 exist and pass.

---

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| libkrun has no exec-into-VM API | Guest listener + per-VM token + mux, the pattern preview-attach uses today |
| A host loss takes a whole workspace | 10-minute background checkpoint plus restore on another host; session files and chat history are in Postgres, not on the VM |
| A superseded VM reconnects and keeps writing | Incarnation fence at every boundary (§6.8); lease enforced locally on the host |
| A crash between a task command and its outcome | Two-phase starts, journaled and sequenced events, retained outcomes, reconnect reconciliation (§7.6) |
| A stopped workspace never receives its work | Dispatch selects work regardless of VM state and resumes on demand (§7.3) |
| Writes land during a stop | Stop barrier closes admission, waits for in-flight mutations, and quiesces before the final checkpoint (§5.7) |
| The stop barrier's own writes are refused or raced | The draining incarnation stays current until release; the reconciler treats `stopping` as live (§5.7, §6.8) |
| A checkpoint push fails at stop | The stop fails and the VM stays up; explicit force-stop records the loss |
| A checkpoint drops a tracked file matched by an ignore rule | Temporary index seeded from the real index and re-hashed (§5.4) |
| A background fetch widens an agent's implicit force lease | The checkpoint task observes with `ls-remote` and never fetches (§5.4) |
| The background checkpoint contends with a busy tree | It reads the tree with a temporary index and never writes the working tree or the real index; a build that changes files mid-snapshot yields a slightly inconsistent snapshot that the next run replaces |
| An answer is lost between Postgres and a blocked process | Answers are committed first and read by polling; one successor turn when the process is gone (§7.2, §8.3) |
| Several agents in one folder overwrite each other | By design: last writer wins; agents are told; UI shows a notice when more than one task runs |
| Owner force-push discards an unseen commit | Documented behavior; profiles require `--force-with-lease`; only laptop-versus-VM races can hit it |
| Claude print-mode behavior differs from the SDK (permission prompt tool, stream-json input, session file layout) | Pin the CLI at Phase 3 start; verify against the pinned binary; the fake CLI replays frames recorded from the real one |
| Codex cannot answer permission approvals under `exec` | Non-prompting policies only; user questions still work through `ask_user`; App Server protocol is a deferred item |
| A question tool call times out before the human answers | MCP tool-call timeouts configured above the question expiry (§8.3) |
| A provider process never exits | `max_turns`, `wall_clock_limit` in `spec`, cancel with SIGINT then SIGKILL after 15 s |
| Session-file restore fails or the provider rejects a restored transcript | Fall back to a new provider session with the history preamble; chat history in Postgres is unaffected |
| Provider transcripts end up in git | State outside every working tree plus platform excludes (§6.3) |
| Approval, publication, and merge drift apart | One pinned source OID for the whole operation; provider merge with expected SHA (§10.6) |
| Merge partially succeeds and the retry loses the result | Durable `provider_merged`; local result OID persisted before the CAS; recovery recognizes an applied result (§10.6) |
| An event is lost after a ref update | Projections converge from authoritative OIDs (§9.6) |
| Timeline gaps after an API disconnect | Journaled events replayed from the acknowledged watermark (§7.6) |
| Reads while the VM is stopped are only as fresh as the last checkpoint | Checkpoint on every stop and every 10 minutes; the UI labels `source: checkpoint` |
| Save resumes the VM | The UI shows "resuming workspace"; reads never wait |
| Single API replica (process-local tunnel registry and event bus) | Removing CRDT authority removes the hard reason; move the registry and bus to Redis in Phase 4 if needed |
| Provider branch protection is outside G5 | Verify at connect, warn on disagreement, treat provider rejection as final |
| Rebinding two large editors off Yjs is frontend-heavy | The question-document view is the template; editor tests retargeted in Phase 2 |

---

## 15. Validation

Existing safety net: the Rust end-to-end crate (about 75K lines) and about
100 Playwright flows, pruned per removed feature and extended per phase with
tests that fail without the new behavior and exercise the production boundary.
Tests added for the draft 3 review are marked (R#) and for the draft 5 review
(F#).

- **Git boundary:** receive-pack rejects protected refs, cross-workspace refs,
  zero-old-sha updates of existing refs, deletes, and stale `vm_id` tokens
  while accepting a draining one; accepts FF and advertised-head rewrites on
  the owner's branch; (R8) an owner `--force` from a clone that never fetched
  the latest commit succeeds and is documented, while `--force-with-lease`
  with a stale expectation is rejected; a fork creates a ref.
- **Checkpoint task and restore:** the background task runs on its interval
  only when something changed, and `checkpoint.run` triggers it immediately
  and waits; it captures tracked, untracked, and deleted files but not ignored
  ones or platform-excluded runtime dirs; (F1) a tracked file matched by
  `.gitignore` with a dirty edit is captured and restored, including an edit
  made in the same second as the last index write with unchanged size; (F2)
  after a checkpoint, `origin/<branch>` is unchanged and a `--force-with-lease`
  push that would discard a laptop's unseen commit is still rejected; (R1) a
  clean tree with a committed-but-unpushed file restores with the commit
  present and the branch ahead of origin; (R1) local commits plus dirty edits
  restore both; (R9) both providers against a customer repo with no
  `.gitignore` produce no session file in the checkpoint or in a native
  `git add -A`.
- **Workspace actions and stop barrier:** create, stop, resume, delete each
  leave the expected refs, rows, and VM state; idle stop equals stop; (R4)
  writes issued after admission closes are refused; (F5) a Save admitted just
  before stop completes and its content is in the final checkpoint; (F5) a
  reconciler tick immediately after admission closes does not mark the VM or
  its tasks lost; (F5) a long final checkpoint and harvest complete under the
  draining incarnation; (R4) a stop with a running task interrupts it and
  re-queues it when opted in; (R4) a stop with an open Claude permission
  request interrupts the turn, keeps the question answerable, and the answer
  starts exactly one new turn; (R4) a failed final checkpoint push fails the
  stop and leaves the VM running with admission reopened.
- **Incarnation fence:** (R2) a host partitioned mid-turn and reconnected
  after a replacement was placed has its VM stopped, its git push refused, its
  session-file harvest refused, and its LLM proxy requests refused; host kill
  still recovers on another host.
- **Dispatch and events:** (F3) a new task and a new chat turn with every VM
  initially stopped resume the workspace and run; (F4) an API crash between
  recording `running` and sending the start re-sends the same `exec_id` on
  restart and runs the process exactly once; (F4) a cancel racing an
  unconfirmed start never launches, and a process discovered for a cancelled
  task is cancelled; (R3) channel loss immediately before spawn, immediately
  after spawn, and at process exit each end with exactly one process run and
  one terminal outcome recorded; (R3) API restart with the VM alive
  reconciles running and finished tasks; a stranded `pending` assistant
  message is repaired; (F8) an API restart in the middle of a tool call with
  the VM alive yields a complete, ordered timeline after reconnect, including
  a late event with a lower sequence than one already stored.
- **Answers:** (F6) an API crash after the answer commit but before the
  process read it delivers the answer once on restart; (F6) an API crash after
  the process consumed the answer but before `consumed_at` landed produces at
  most one successor turn carrying the note; (F6) an answer racing a stop is
  delivered to the process or to exactly one successor turn, never both
  silently.
- **Provider runtime:** fake `claude` and `codex` binaries replay recorded
  frames; `--resume` receives restored files; `request_permission` (Claude)
  and `ask_user` (both providers) round-trip through `task_questions` with a
  human delay longer than the default MCP tool timeout; cancel delivers SIGINT
  then SIGKILL; a missing session in Postgres falls back to the preamble path;
  chat history renders with every VM stopped; (R5) the Codex runner refuses a
  prompting policy preset and the fake Codex binary emits no approval frame.
- **Lifecycle:** all transitions including `interrupted`; the reconciler
  rules; job status derivation; an open question keeps the VM awake until it
  expires and idle stop then proceeds.
- **Chat:** two sessions concurrently in one folder; stop and resume with
  continuity; host kill mid-turn; a Claude session and a Codex session in the
  same workspace.
- **Governance:** merge refused without decisions at the pinned source OID;
  (R6) an internal push after publication makes the merge republish and pass
  the expected SHA; (R6) a source push racing with merge is refused by the
  provider precondition; (R7) crashes before and after the provider call, a
  lost success reply, and an inbound target update during merge each converge
  to one consistent state on retry; (F7) a provider merge success followed by
  an import failure retains `provider_merged` and completes on retry without
  re-pinning; (F7) a remotely merged PR at a different head is not attributed
  to the operation; (F7) a local CAS success followed by a crash before the
  metadata commit is recognized as applied on recovery; the VM token cannot
  push `main`.
- **Projections:** (R10) a successful ref update with its event dropped
  converges within one loop interval without another push; deletion removes
  projection rows.
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
| Tables: 118 → ~36; migrations: 292 → 1 | |

Added: dispatcher (~6K to 8K including RPC, event journal, checkpoint task,
provider runner), host command channel and reconnect reporting (~1.5K),
receive-pack policy and fence checks (~700), PR/approval/merge operation with
recovery (~4.5K), lifecycle with the dispatch and event contract (~3.5K),
projection convergence (~800), `request_permission` and `ask_user` tools with
answer polling (~700), frontend Save/Changes/terminal/ports (~4K to 6K).

---

## 17. Implementation-time choices and deferred items

### 17.1 Implementation-time choices

These do not change the architecture and are left to the implementing phase:

- Exact Claude flag set and the pinned version (Phase 3 records both).
- Whether Node stays on the runtime mount (depends on the pinned Codex CLI).
- The exact MCP tool-call timeout values for both CLIs (above the question
  expiry plus margin) and the answer-polling interval.
- zstd level and per-file cap for `chat_session_files`; retention of files for
  deleted sessions (deleted with the session).
- Event payload shapes for `branch.updated`, `checkpoint.updated`,
  `file.changed`; the journal file format on the VM and its disk budget; the
  checkpoint task's change-detection method (file watcher versus periodic
  `git status`).
- The `configs` table shape and the `worker_hosts` column set after the fleet
  simplification; whether `projection_state` is one table or a column set on
  each projection.
- Whether the tunnel registry and event bus move to Redis in Phase 4 or later.
- Crate consolidation targets in Phase 8.
- The list of `.g5/` runtime subdirectories in the platform excludes file.
- The retry and backoff schedule for stop-barrier checkpoint pushes and for
  merge import retries.

### 17.2 Deferred items

Recorded so they are not lost; none is designed here:

- **Interactive Codex tool-permission approvals.** Would require replacing
  `codex exec` with the Codex App Server JSON-RPC protocol bounded to one turn,
  keeping process exit as the turn boundary. Needs a verified prototype of the
  approval round trip against the pinned binary before it is designed. User
  questions from Codex are already supported through `ask_user` and do not
  depend on this item.
- **Document-only CRDT** for real-time co-editing behind the Save API (§9.5).
- **Org-level read-only provider token** on the host proxy for private git
  dependencies (§5.5).
- **Multi-replica API**: moving the tunnel registry and event bus to Redis
  (§14).

---

## 18. Decisions register

| Decision | Settled by | Section |
|---|---|---|
| Ontology and grounding tooling kept, decoupled from git and CRDT; everything else retained except the approved removals | Q1, B1 | §2, §3.2 |
| Targets: GCP SaaS, self-hosted Linux including the Azure canary lane, macOS dev-only; customer images dropped | Q2 | §2, §6.6 |
| VM is terminated when idle and re-provisioned on request; changes checkpointed to internal git in the background and before termination | Q3, Q5, Decision D | §5.4, §5.7 |
| libkrun microVM on shared worker hosts; equal-size VMs, count per host | Q4, Q9 | §4.1, §6.6 |
| Idle timeout 30 minutes; tabs do not count as activity | Q5 | §5.7 |
| VM disk ephemeral; warm when the host still has it | Q6 | §6.5 |
| No isolation inside the VM: one folder, one branch, no worktrees, no job branches; last writer wins | Q7, Q18, review correction | §6.3, §7.4 |
| A turn ends when the provider process exits; no background work; subagents untracked | Q8 | §8.2 |
| Preview through the API relay, subdomain optional | Q10 | §6.4 |
| Browser terminal in this iteration | Q11 | §6.7 |
| Checkpoint = commit of the working tree on a force-updated shadow ref carrying the local commit chain, built from a seeded temporary index, observed via `ls-remote`; ignored files not checkpointed; produced by an independent background task | Q12, Q13, review R1, F1, F2, Decision D | §5.4 |
| Multi-repo workspaces with per-repo pushes; org docs repo stays a member | Q14, Q15 | §5.1 |
| Git refs authoritative; advertised-head CAS on workspace branches; protected refs never; owner rewrites allowed; lease safety is a client obligation | Q16, Q17, review R8, Decision C | §5.3 |
| Provider tokens never in the VM; publish server-side on explicit action; upstream via mirrored protected branches | Q19 | §5.5 |
| `RepoStorage` and its deployment choice unchanged | Q20 | §4.1 |
| Job kinds are profiles over one task machinery; all jobs run in the workspace VM | Q21 | §7.1 |
| No automatic retry on agent failure; one re-queue on VM loss or interruption when opted in | Q22 | §7.2 |
| No job pause; cancel only; no rollback | Q23 | §7.2 |
| One HITL table `task_questions`; `task_attempts` audit with six reasons | Q24, Q25 | §7.1 |
| Any workspace member may send into a chat; `author_id` recorded | Q26 | §8.1 |
| Context links dropped; task-scoped session types folded into chats; preview attach machinery dropped | Q27, Q29, Q30 | §8.7 |
| Provider session files persisted to Postgres only; restored before `--resume`; stored outside every working tree | Q28, review R9 | §8.4, §6.3 |
| No co-editing or presence; Save resumes the VM; comments anchored by path, line, hash; all four editors kept | Q31 to Q34 | §9 |
| Enforcement at both the internal git boundary and the provider; minimal rules and decisions tables; approvers = rule list plus org admins | Q35 to Q37 | §10 |
| Agents open PRs via `g5d pr create`; provider PRs mirrored; mutation policy and scope grants dropped | Q38 to Q41 | §10 |
| Host-side LLM proxy kept; ontology MCP tools kept; retained `g5d` commands; profiles rewritten with minimal hooks | Q42 to Q45 | §11 |
| Phase order as proposed; migrations squashed to one baseline; Cloud Build kept, revisited last | Q46 to Q48 | §12, §13 |
| Approved removals: issue tracking, IDE mockup, hidden settings UI, takeover remnants, non-GitHub platform login | B1 | §3.2, §12 |
| Claude CLI pin bumped at Phase 3 start with the verified flag set | B2 | §8.3 |
| Fake CLI binaries replace the `fake` provider and SDK fixtures | B3 | §8.6 |
| One Rust provider runner; both Node sidecars deleted | B4 | §8.3 |
| Non-chat tasks bypass permissions; chat turns use the frozen mode and `request_permission` (Claude) | B5 | §8.3 |
| Checkpoint every 10 minutes when changed, plus a final pass on stop | B6, Decision D | §5.4, §5.7 |
| Cancel: SIGINT, SIGKILL after 15 s, partial output kept as `stopped` | B7 | §7.2 |
| Chat history from Postgres via the API with no VM, as today | B8 | §8.1 |
| Multi-task notice only | B9 | §6.3 |
| Workspace actions: create, stop, resume, delete; no archive | B10 | §5.7 |
| Incarnation fence on tokens, git, session files, events, LLM proxy; reconnect stops stale VMs; a draining incarnation stays current until release | review R2, F5 | §6.8, §5.7 |
| Dispatch and event contract: execution identity, two-phase starts, idempotent commands, journaled and sequenced events, watermark acks, reconciliation | review R3, F4, F8 | §7.3, §7.6 |
| Dispatch selects ready work regardless of VM state and resumes stopped workspaces on demand | review F3 | §7.3 |
| Stop is an ordered durability barrier with admission close, in-flight drain, quiesce, final checkpoint, harvest, release; a failed final checkpoint push fails the stop | review R4, F5 | §5.7 |
| Open questions keep the VM awake until expiry; explicit stop interrupts the turn (`interrupted`) and the answer starts a new turn | review R4, Decision A | §7.2 |
| Explicit stop and drain cancel running tasks before the final checkpoint | review R4, Decision D | §5.7 |
| Answers committed first and delivered by polling committed state; consumption recorded; exactly one successor turn when the process is gone; at-least-once delivery | review F6 | §7.2, §8.3 |
| Codex runs non-prompting policies only; `request_permission` is Claude-only; user questions via `ask_user` for both providers; interactive Codex approvals deferred | review R5, Decision B | §8.3, §17.2 |
| MCP tool-call timeouts exceed the question expiry | Decision B follow-up | §8.3 |
| Approvals, publication, and merge pinned to one source OID; provider merge with expected SHA; provider-first merge operation with one retained identity, durable `provider_merged`, local result persisted before the CAS, recovery recognizes applied results | review R6, R7, F7 | §10.2, §10.3, §10.6 |
| Projections converge from authoritative OIDs; events are hints | review R10 | §9.6 |
