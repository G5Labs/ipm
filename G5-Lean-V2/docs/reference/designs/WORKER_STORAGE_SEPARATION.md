# G5 CLI Operations, Worker Separation & Repo Storage

**Status:** Draft  
**Date:** 2026-03-05  
**Authors:** —

---

## 1. Overview

This document specifies three interrelated concerns:

1. **CLI workspace operations** — a unified set of operations for cloning
   a workspace, forking an isolated task environment, connecting to CRDT,
   executing work, and committing/pushing results. These operations are
   shared between **cloud workers** and **local users** running their own
   coding agent.
2. **Worker separation** — cloud workers must not mount shared NFS and must
   use the same code paths as the CLI for repo operations.
3. **Repo storage tradeoffs** — Filestore NFS today, GCS + database later.

This is a forward-looking spec. Where current behaviour differs from this
document, the spec is normative and the implementation must change to match
it.

Additionally:
- Worker registration tokens must be scoped per org, workspace, or user.
- Chat tasks must always have scheduling priority.
- Worker cache policy (current): use local cache storage only. The cache must
  be a real persistent on-disk directory (for example `~/.g5/cache` or
  `/cache`), not an in-memory, mock, or disabled cache.

---

## 2. CLI Workspace Operations

### 2.1 Operation Sequence

The complete lifecycle for executing a task — whether by a cloud worker or
a developer on their local machine — follows this sequence:

```
┌─────────────────────────────────────────────────────────────────────┐
│ 1. CLONE      Clone/checkout workspace at its head or a version     │
│ 2. FORK       Create an isolated task environment                  │
│ 3. CONNECT    Connect CRDT peer to g5-api for real-time sync       │
│ 4. EXECUTE    Run the coding agent in the isolated environment     │
│ 5. COMMIT     Stage and commit changes                             │
│ 6. PUSH       Publish new workspace version (rebase if head moved)  │
│ 7. CLEANUP    Tear down the isolated environment                   │
│                                                                     │
│ REFRESH       Fetch + advance local workspace to latest version    │
└─────────────────────────────────────────────────────────────────────┘
```

REFRESH is not part of the task lifecycle sequence — it is a standalone
command used between tasks or by developers to pull upstream changes.

Each operation maps to both a CLI command (for local use) and a
programmatic call (for cloud workers and the API server). The underlying
implementation is shared via `g5-common/src/ops/` (see §2.10).

### 2.1.1 Command Grammar

The user-facing `g5` CLI command surface, plus the worker-only `g5d`
entrypoint, is:

```bnf
<top-level-command> ::= <g5-cli-command>
                      | <g5d-worker-cmd>

<g5-cli-command> ::= <clone-cmd>
                   | <fork-cmd>
                   | <connect-cmd>
                   | <disconnect-cmd>
                   | <worker-manage-cmd>
                   | <commit-cmd>
                   | <push-cmd>
                   | <refresh-cmd>
                   | <cleanup-cmd>
                   | <merge-cmd>
                   | <add-cmd>

<clone-cmd> ::= "g5" "clone" <workspace-url> <opt-target-dir> <clone-options>
<opt-target-dir> ::= ε | <path>
<clone-options> ::= ε | <clone-option> <clone-options>
<clone-option> ::= "--version" <workspace-version-sha>

<fork-cmd> ::= "g5" "fork" <opt-fork-source> <fork-options>
<opt-fork-source> ::= ε | <workspace-url>
<fork-options> ::= ε | <fork-option> <fork-options>
<fork-option> ::= "--task" <task-id>
                | "--target" <path>
                | "--connect"
                | "--name" <fork-name>
                | "--repos" <repo-list>

<connect-cmd> ::= "g5" "connect"
<disconnect-cmd> ::= "g5" "disconnect"

<worker-manage-cmd> ::= <worker-start-cmd>
                      | "g5" "worker" "stop" <opt-worker-name>
                      | "g5" "worker" "status" <opt-worker-name>
                      | "g5" "worker" "logs" <opt-worker-name>
<worker-start-cmd> ::= "g5" "worker" "start" "--api-url" <url> "--api-key" <api-key> <worker-start-options>
<worker-start-options> ::= <worker-start-option> <worker-start-options>
                         | ε
<worker-start-option> ::= "--name" <worker-name>
                        | "--concurrency" <integer>
                        | "--org" <org-id>
                        | "--workspace" <workspace-id>
                        | "--capability" <capability-name>
<opt-worker-name> ::= ε
                    | "--name" <worker-name>

<g5d-worker-cmd> ::= "g5d" "worker" <worker-options>
<worker-options> ::= ε | <worker-option> <worker-options>
<worker-option> ::= "--task" <task-id>
                  | "--workspace" <path>

<commit-cmd> ::= "g5" "commit" "-m" <message> <opt-commit-all>
<opt-commit-all> ::= ε | "--all"

<push-cmd> ::= "g5" "push" <opt-push-target> <push-options>
<opt-push-target> ::= ε | <workspace-url>
<push-options> ::= ε | <push-option> <push-options>
<push-option> ::= <git-remote-opt>
                | "--force"

<refresh-cmd> ::= "g5" "refresh" <refresh-options>
<refresh-options> ::= ε | <refresh-option> <refresh-options>
<refresh-option> ::= <git-remote-opt>
                   | "--path" <path>

<git-remote-opt> ::= "--git-remote"
                   | "--git-remote=" <remote-list>
                   | "--git-remote" <remote-list>

<cleanup-cmd> ::= "g5" "cleanup" <cleanup-options> <opt-cleanup-path>
<cleanup-options> ::= ε | "--force"
<opt-cleanup-path> ::= ε | <path>

<merge-cmd> ::= "g5" "merge" <workspace-ref> <merge-options>
<workspace-ref> ::= <workspace-name> | <workspace-id>
<merge-options> ::= ε | <merge-option> <merge-options>
<merge-option> ::= "--dry-run"

<add-cmd> ::= "g5" "add" <repo-url> <add-options>
<add-options> ::= ε | <add-option> <add-options>
<add-option> ::= "--branch" <branch-name>
               | "--name" <repo-name>

<repo-list> ::= <repo-name>
              | <repo-name> "," <repo-list>

<remote-list> ::= <remote-name>
                | <remote-name> "," <remote-list>

<capability-name> ::= "chat"
                    | "uplift"
                    | "downlift"
```

**Requirements Summary**

| Area | Requirement |
|------|-------------|
| Workspace identity | A workspace version is the immutable SHA of the HEAD commit in `workspace-meta`; it pins the full multi-repo workspace state. |
| Clone | `g5 clone` materializes the full workspace; partial acquisition is done with `g5 fork --repos`, not partial clone. |
| Fork | `g5 fork` creates local child-workspace lineage and materialization; CLI/local forks may defer remote child-workspace creation until `g5 push`. |
| First publish | `g5 push [<workspace-url>]` may publish to a bound remote child, an explicit workspace URL, or a default child workspace created on first publish. |
| Subset scope | Subset forks start with a limited repo set, but push/merge operate on the repos actually present in the current fork manifest, including repos added later. |
| Merge | `g5 merge` is workspace-to-workspace, may be cross-org when the caller has current effective permissions, and adds source-only repos into the target. |
| Push semantics | `g5 push` publishes the current local workspace state to the resolved remote target; it does not implicitly update a parent workspace unless that parent is explicitly targeted. |
| Permissions | Authorization is evaluated at operation time, not token-mint time; publish/merge/report operations use current effective permissions. |
| Worker auth | `g5 worker start` requires `--api-url` and `--api-key` in CLI form; `G5_API_KEY` is the env-var alternative described in prose. |
| Worker narrowing | Local workers may narrow intake with `--org`, `--workspace`, and `--capability <chat|uplift|downlift>`; `--concurrency` sets the number of isolated task slots. |
| Failure handling | If publish or merge is rejected because access changed or the remote target disappeared, the remote action fails cleanly and the local fork remains intact for later republish. |

`g5 clone` intentionally materialises the full workspace. Repo subsetting
is supported by `g5 fork --repos`, including remote-source forks, rather
than by partial clone.

### 2.1.2 Workspace Model

The user-facing model is:

- **Workspace** — the primary mutable unit. The UI opens a workspace. The
  CLI clones, forks, refreshes, pushes, and merges workspaces.
- **Workspace version** — an immutable snapshot of the entire workspace,
  identified by a SHA. A workspace version pins the exact commit for every
  repo included in that workspace.
- **Job** — orchestration attached to a workspace. Batch jobs such as
  uplift and downlift usually create a child **job workspace** from a
  parent workspace version.
- **Task** — an execution unit within a job. A task runs in an isolated
  **task workspace** derived from a job workspace version.

Each repo inside a workspace version may still carry internal git metadata
such as remote URLs, tracking branches, and checkout refs. Those are
implementation details used to materialise and advance workspace versions;
they are not first-class user-facing concepts.

**Workspace version identity:** the version SHA is the SHA of the HEAD
commit in the workspace's `workspace-meta` git repository. This repo
contains versioned workspace metadata (`workspace.yaml`, manifest,
lock state) and is authoritative — every workspace version corresponds
to exactly one `workspace-meta` commit. The API and CLI resolve
workspace versions by looking up this commit. When `g5 push` creates
a new workspace version, it commits the updated manifest (with new
repo commit pins) to `workspace-meta` and uses that commit SHA as the
workspace version SHA.

**Reproducibility invariant:** a workspace version must always pin a
recoverable commit for every repo it contains. Tracking branches may go
stale or disappear; pinned repo commits may not become unrecoverable.

#### 2.1.2a Migration Path: Branch-Based → Workspace-Version Model

The current implementation uses per-job and per-task **git branches** for
isolation (e.g., `job/<job_id>/main`, `task/<task_id>/main`). The target
model described above uses **workspace versions** (pinned commit SHAs in
a `workspace-meta` repository). The migration proceeds incrementally:

**`workspace-meta` is just a repo.** Each workspace has a single
`workspace-meta` git repository managed by the API. It stores
`workspace.yaml` (repo manifest with pinned commit SHAs) and is the
authoritative source for workspace version history. Parent/child
workspace relationships are modeled naturally as git branches or forks
of this repo:

- **Parent workspace** → `main` branch of `workspace-meta`
- **Job workspace** → branch `job/<job_id>` forked from a parent version
- **Task workspace** → branch `task/<task_id>` forked from the job branch
- **Chat workspace** → branch `chat/<session_id>` forked from parent

Merging a child workspace back into the parent is a fast-forward or
rebase of the child branch into `main` — standard git. The API is the
only writer to `workspace-meta`; workers and CLI read it via the git
transport layer (§14.2) to resolve workspace versions.

| Current State | Target State | Migration Step |
|---|---|---|
| `jobs.head_commit` column tracks job branch tip | Job workspace version = `workspace-meta` commit SHA on `job/<id>` branch | Add `workspace-meta` repo per workspace; write manifest on job creation |
| `tasks.head_commit` column tracks task branch tip | Task workspace = `task/<id>` branch forked from job branch | `transition_task(completed)` merges task branch into job branch |
| Branch refs (`refs/heads/job/...`) are the source of truth | `workspace-meta` branches are the source of truth | Stop exposing content-repo branch refs in API responses; resolve via `workspace-meta` |
| `workspace_branches.head_commit` updated per-repo | Workspace version pins all repo commits atomically in manifest | `merge_workspace_api()` already does this (§10.5.6) |

**Implementation plan:**
1. Create `workspace-meta` bare repo on workspace creation. Seed `main`
   branch with initial `workspace.yaml` (repo manifest).
2. On job creation, create branch `job/<job_id>` from the current `main`
   HEAD. Commit the current repo SHAs as the job's base version.
3. On task creation, create branch `task/<task_id>` from the job branch.
4. On task completion + merge-back, fast-forward or rebase the task
   branch into the job branch. On job completion, merge the job branch
   into `main`.
5. Migrate API responses to return workspace version SHAs (commit on
   `main` or the relevant branch) instead of content-repo branch names.
6. Deprecate and remove `jobs.head_commit` / `tasks.head_commit` columns
   once all callers use workspace version resolution.

**Storage location:** The `workspace-meta` repo lives alongside the
content repos in the same `RepoStorage` (i.e. the NFS storage mirror in
Phase 1, GCS in Phase 2). It is a regular bare repo, subject to the
same backup and replication policies as content repos.

**Branch GC:** Completed task branches (`task/<id>`) should be deleted
after successful merge into the parent job branch, and completed job
branches (`job/<job_id>`) should be deleted after merge into `main`.
This prevents unbounded ref accumulation. Add a background reaper or
post-merge hook that prunes merged branches older than a configurable
retention period (default: 7 days).

This can be done incrementally — the branch-based and version-based models
coexist during migration. The `workspace-meta` commits are append-only, so
there is no data loss risk.

### 2.1.3 Job Workflows

The three important workflows are:

- **Uplift** — create a child job workspace from the parent workspace's
  current version, decompose work into tasks, execute those tasks in
  isolated task workspaces, then merge the completed job workspace back
  into the parent as a new parent workspace version.
- **Downlift** — same workspace lifecycle as uplift, but driven by spec
  changes and code generation/review flow rather than repo bootstrap.
- **Chat** — create or open an interactive chat workspace, stream edits
  over CRDT, and create new workspace versions as the conversation
  progresses. If the user wants the result applied to another workspace,
  that happens via explicit merge/apply, not implicit push.

### 2.2 Operation Details

#### Op 1: CLONE — Acquire the workspace at a specific state

**What it does:**
Clone a workspace at its current head version, or at a specific workspace
version SHA. This gives the agent a working copy of the entire codebase
at a coherent, reproducible point in history.

**CLI usage:**
```bash
# Clone a full workspace by URL
g5 clone https://g5.example.com/acme/my-project [target-dir]

# Clone an exact workspace version
g5 clone https://g5.example.com/acme/my-project --version <workspace-version-sha>
```

**URL parsing:**
The positional argument is always a full workspace URL:
```
https://g5.example.com/acme/my-project
└── api_url ──────────┘ └org┘ └─ workspace ─┘
```
`g5 clone` parses the URL into `(api_url, org_slug, workspace_slug)`,
resolves the workspace through g5-api, and persists the resolved instance
metadata into `.g5/workspace.yaml`. Subsequent commands (`push`, `refresh`,
`fork`, `merge`, `connect`) read the instance from that workspace metadata.

If the full parent workspace is too large to bring locally, use
`g5 fork <workspace-url> --repos ...` instead of adding a partial-clone
mode to `g5 clone`.

**Acquisition modes** (same code path, different transport):

| Mode            | Used by             | Mechanism                              |
|-----------------|---------------------|----------------------------------------|
| `full-clone`    | CLI (first time)    | Resolve manifest via g5-api, then materialise repos through API-backed repo access |
| `shallow-clone` | Cloud worker        | Resolve via g5-api, then perform shallow API-backed materialisation |
| `gcs-snapshot`  | Cloud worker (v2)   | Download pre-built tarball from GCS via g5-api-issued URL |
| `local-open`    | CLI (existing repo) | Open existing checkout, then refresh from g5-api |

**Semantics:**
- Authenticates with g5-api to fetch the workspace manifest and resolve the
  target workspace version
- Without `--version`, resolves the current head version of the workspace
- With `--version`, resolves that exact immutable workspace version SHA
- Materialises each repo listed in the manifest at the commit pinned by the
  selected workspace version
- Initialises `.g5/workspace.yaml` and `.g5/workspace-meta/`

**Current code:** `cli/clone.rs` (CLI) / `lifecycle.rs::phase_prepare` (worker)

**Instance persistence:** On clone, the API URL and org are written into
`.g5/workspace.yaml`:
```yaml
workspace_id: "ws-uuid-from-api"
workspace_version_sha: "5e1c9c2b..."
remote:
  enabled: true
  api_url: "https://g5.example.com"
  org_slug: "acme"
repos:
  - name: backend
    url: https://github.com/acme/backend.git
    path: repos/backend
    commit: "abc123"
    tracking_branch: "main"   # optional metadata; may dangle
```
All subsequent workspace commands read `remote.api_url` from
`.g5/workspace.yaml`. The user does not need to re-specify the instance
once a workspace has been cloned.

#### Op 2: FORK — Create an isolated environment

**What it does:**
Create a fork of the workspace. This is the isolation boundary — the
agent can make arbitrary changes without affecting the base workspace or
other concurrent forks. When the source workspace is already cloned
locally, repo materialisation may use copy-on-write techniques; when the
source is remote, the fork is materialised directly from g5-api.

**CLI usage:**
```bash
# Fork for a specific task
g5 fork --task <task-id> [--target <dir>]

# Fork directly from a remote workspace URL (no prior local clone required)
g5 fork https://g5.example.com/acme/my-project --repos backend,frontend --target ../auth-work

# Fork and immediately connect CRDT (live sync to browser)
g5 fork --connect [--target <dir>]
g5 fork --task <task-id> --connect

# Fork for ad-hoc local work (no task, no connection)
g5 fork [--name <fork-name>] [--target <dir>]

# Fork only a subset of repos
g5 fork --repos backend,frontend [--target <dir>]
g5 fork --repos backend --task <task-id>
```

**Source resolution:**
If `g5 fork` is called without a positional source, it forks the current
local workspace. If a workspace URL is provided, it forks that remote
workspace by resolving and reading it through g5-api, then materialises
the fork locally.

There are two valid execution modes for fork creation:

- **CLI/local mode** — `g5 fork` prepares local child-workspace lineage
  and materializes the checkout. Remote child-workspace creation may be
  deferred until `g5 push`.
- **API/UI mode** — `POST /api/v1/workspaces/:id/fork` creates the child
  workspace eagerly on the server. This requires immediate server-side
  authorization because the browser is directly invoking the remote API.

**Authorization:**
- Read access to the source workspace is required to inspect and clone it.
- A local `g5 fork` does **not** require immediate remote write access.
- Remote child-workspace creation is deferred until first publish
  (`g5 push`) in CLI/local mode, unless the workflow is server-driven and
  creates the child workspace eagerly.
- At publish time, the caller must have permission to create or update
  the child workspace in the destination namespace.
- The destination namespace may be in the same org as the source
  workspace or in a different org, as long as the caller has current
  read access to the source workspace and current create/update rights
  in the destination namespace.

**`--repos` flag:**
By default, `g5 fork` forks **all** repos in the workspace. With
`--repos repo1,repo2,...`, only the listed repos are included in the
fork. Repo names are matched against the `repos[].name` entries in the
source workspace manifest. This is useful when a task only touches a
subset of the codebase and a full parent clone is unnecessary.

The forked workspace records which repos were included:
```yaml
# .g5/workspace-meta/fork_lineage.yaml
forked_from: ws-uuid-parent
forked_from_version: 5e1c9c2b...
forked_at: 2026-03-05T10:30:00Z
repos:               # subset — only these repos were forked
  - name: backend
    repo_id: uuid-1
  - name: frontend
    repo_id: uuid-2
full_fork: false      # false when --repos was used
```

When `--repos` is omitted, `full_fork: true` and `repos` lists all
workspace repos (current behaviour).

**Constraint:** `g5 push` and `g5 merge` operate on all repos present
in the fork workspace. A subset fork starts with only the repos listed
in its lineage, but the user or agent may **add new repos** to the fork
workspace during work. When pushed or merged back, newly added repos
are included alongside the originally forked repos. Repos absent from
the fork (neither originally forked nor subsequently added) are left
unchanged in the parent. The fork lineage file records the initial
subset; the current workspace manifest (which may have grown) is the
source of truth for push/merge scope.

**Adding repos to a subset fork:**
- `g5 add <repo-url>` inside a subset fork adds the repo to the fork's
  manifest.
- On `g5 push`, the new repo is included in the child workspace version.
- On `g5 merge` back to the parent, the new repo is added to the parent
  workspace if it doesn't already exist there. If a repo with the same
  `repo_id` already exists in the parent, the merge follows the standard
  per-repo fast-forward/rebase logic.

**API-side subset fork (UI/server-driven mode):** `POST /api/v1/workspaces/:id/fork` accepts
an optional `source_version_sha` and `repo_ids` array:
```json
{
  "name": "auth-refactor",
  "source_version_sha": "5e1c9c2b...",
  "repo_ids": ["uuid-1", "uuid-2"]
}
```
When `source_version_sha` is omitted, the parent workspace's current head
version is used. When `repo_ids` is omitted, all repos are forked. When
provided, only those repos are included in the child workspace. The child
workspace records `cloned_from_id` and the exact parent workspace version
it was created from.

**`--connect` flag:**
When `--connect` is passed, the fork operation automatically starts a
CRDT peer in the background after creating the fork. This is sugar for
`g5 fork && g5 connect`. The connection uses the fork workspace's
editable head.
Disconnect with `g5 disconnect` or by pushing.

**Semantics:**
- Creates a local child workspace lineage and materialized checkout.
- Records workspace lineage: parent workspace ID + parent workspace
  version SHA + repo subset
- Materialises the selected repos locally:
  - from the current local workspace via COW clone when possible
  - or directly from g5-api when a source URL is provided
- With `--repos`, only the listed repos are materialised; without it,
  all repos in the source workspace are included.
- Records fork lineage in `.g5/workspace-meta/fork_lineage.yaml`,
  including the repo subset
- If the fork is task-driven, the worker may create internal per-repo git
  refs as an implementation detail. The user-facing unit is still the child
  workspace, not a branch.
- The remote child workspace may be created eagerly by server-side flows
  or lazily on first `g5 push` for local CLI flows.
- `g5 push` advances only the child workspace by creating a new child
  workspace version. It does not mutate the parent workspace.
- On merge-back, only the included repo subset is applied to the parent

**Cloud worker variant:**
- The "fork" is implicit — the worker already has an isolated filesystem
  (Kata microVM or ephemeral pod). The CLONE step materialises the
  assigned workspace version, and the worker creates whatever internal
  repo-local refs it needs to execute the task. Those refs are not part of
  the user-facing model.

**Current code:** `cli/fork.rs` (CLI) / `lifecycle.rs::phase_clone_patch` (worker)

#### Op 3: CONNECT / DISCONNECT — CRDT real-time sync

**What it does:**
Connect (or disconnect) to g5-api's Yjs CRDT document for the current
workspace. This enables real-time sync of file changes between the local
filesystem and the browser-based editor.

CRDT connection is independent of tasks — it works for any editable
workspace. You might connect for task execution, pair programming, live
demos, or just wanting your local edits visible in the G5 UI.

**CLI usage:**
```bash
# Connect to CRDT for the current workspace (backgrounds automatically)
g5 connect

# Disconnect
g5 disconnect

# Shorthand: fork + connect in one step
g5 fork --connect
```

**Behaviour:** `g5 connect` spawns a background daemon process, prints
the PID, and returns immediately. The daemon watches the workspace for
file changes and streams Yjs deltas over WebSocket. `g5 disconnect`
finds the daemon via a pidfile (`.g5/crdt.pid`) and terminates it.
`g5 push` calls disconnect implicitly.

**Semantics:**
- Opens a WebSocket to the current workspace's CRDT endpoint
- The implementation may internally route by workspace ID plus an editable
  head ref or version ref; that is not a user-facing concept
- Authenticates with the session JWT (worker token or user token)
- Performs initial Yjs state sync (download server doc, merge local state)
- Starts bidirectional delta streaming:
  - **Outbound:** file changes on disk → Yjs deltas → server
  - **Inbound:** server deltas (from browser editor) → materialize to disk
- Maintains heartbeat and reconnect logic

**Design constraint:** The CRDT connection is optional for bootstrap tasks
(which run before any human interaction) but required for all interactive
tasks (chat, downlift with review, etc.). The system must gracefully
degrade if the CRDT connection drops — agent work continues, sync resumes
when reconnected.

**Current code:** `crdt/mod.rs` (CrdtPeer), `crdt/sync.rs` (SyncEngine),
`crdt/materialize.rs` (Materializer)

#### Op 4: EXECUTE — Run the agent in the isolated environment

**What it does:**
Execute the coding agent (G5's built-in agent, or a user's agent of
choice) within the forked workspace. The agent reads/writes files, runs
commands, and produces spec changes or code changes.

**CLI usage:**
```bash
# G5 built-in agent (cloud worker or local)
g5d worker --task <task-id> --workspace <dir>

# User's own agent (local) — g5 just manages the workspace lifecycle:
g5 fork --connect --target ./my-task
cd ./my-task
# ... run Cursor, Copilot, aider, etc. against the files ...
# When done:
g5 commit -m "implemented feature X"
g5 push
```

**Semantics:**
- The agent has full filesystem access within the forked workspace
- Git transport validation blocks destructive operations (`force-push`,
  `branch delete`, `rebase` by the agent — rebase is handled by the
  push operation)
- Checkpoints (intermediate commits) can be created at any time
- CRDT deltas stream bidirectionally during execution
- Budget tracking (tokens, cost) is enforced for cloud workers

**Current code:** `worker/lifecycle.rs::phase_execute`, `agent/mod.rs`

#### Op 5: COMMIT — Stage and commit changes

**What it does:**
Stage all changes in the task's workspace and create a structured commit.

**CLI usage:**
```bash
g5 commit -m "description of changes"

# Commit all repos (even those without changes — creates empty commits)
g5 commit -m "checkpoint" --all
```

**Semantics:**
- For each repo in the workspace with changes:
  1. `git add -A` (stage all)
  2. `git commit` with structured message and author attribution
- The commit author is:
  - Cloud worker: `G5 Agent <agent@g5.dev>`
  - Local user: user's git config (`user.name`, `user.email`)
- Flush any pending CRDT deltas before committing
- Record the commit SHA for the push step
- Does NOT push — that's a separate operation (allows review before push)

**Current code:** `cli/commit.rs` (CLI) / `lifecycle.rs::phase_collect` (worker)

#### Op 6: PUSH — Push to remote with rebase-on-conflict

**What it does:**
Publish local changes to the current workspace by creating a new remote
workspace version. Optionally also push repo commits to git remotes
(GitHub/GitLab) simultaneously.

Push operations are git-write operations (objects + refs), so they are
part of the repo writer set alongside fetch/merge flows.

**CLI usage:**
```bash
g5 push

# First publish or republish to an explicit workspace target
g5 push https://g5.example.com/acme/auth-refactor

# Also push to git remotes (GitHub/GitLab origin)
g5 push --git-remote
g5 push --git-remote=origin
g5 push --git-remote=origin,upstream  # multiple remotes

# Force push (skip rebase — use with caution)
g5 push --force
```

**`--git-remote` flag:**
By default, `g5 push` pushes to the g5-api bare repo only (the
workspace's internal storage). With `--git-remote`, it also pushes the
repo commits to the specified git remote(s) on the SCM provider
(GitHub/GitLab/etc.). When `--git-remote` is specified without a value,
it defaults to `origin`.

This enables the workflow: make changes → commit → push to both g5 and
GitHub in one command. The git-remote push uses the same SCM credentials
stored in workspace config or task context.

**Semantics:**
1. Compute the current workspace's repo commit map from the local checkout.
2. Resolve the publish target:
   - if a workspace URL is passed to `g5 push`, use that explicit target
   - else if the local workspace is already bound to a remote child
     workspace, use that bound remote
   - else create a child workspace in the caller's default namespace
     using the local workspace identity
3. If this local workspace has not yet been published as a remote child
   workspace, create that child workspace now. This is the point where
   child-workspace creation authorization is checked.
   - On first publish, the child workspace becomes visible to users who
     have permissions on that child workspace.
   - By default, the user who publishes it receives full permissions on
     the child workspace.
   - Future ABAC may refine the default permission seeding model.
   - On successful first publish, persist the resolved remote workspace
     URL and ID in `.g5/workspace.yaml` for subsequent pushes.
4. Compare the local base workspace version to the current remote head
   version of that workspace.
5. If the remote head has advanced:
   - Fetch the latest remote workspace version
   - Rebase or replay local unpublished repo commits onto the latest
     pinned repo state
   - Retry publication (up to 3 attempts)
6. Upload or push any repo objects needed by the new state.
7. Create a new workspace version whose pinned repo commits reflect the
   local result. Unchanged repos inherit their previous pins.
8. Advance the current workspace's head to that new workspace version.
9. Update `workspace.lock` with the repo SHAs present in the current
   workspace. For subset forks, repos outside the subset remain inherited
   and unchanged.
10. If `--git-remote`, push the changed repos to the requested git remotes.
11. Re-check authorization at publish time. The caller must still have
    create/update rights on the resolved remote publish target, and any
    worker startup narrowing (`--org`, `--workspace`, `--capability`)
    must still admit this operation.
12. If publish fails because the destination workspace was deleted,
    archived, renamed out from under the caller, or current access was
    revoked, the remote action fails cleanly and the local fork remains
    intact. The user may later publish again, including as a fresh child
    workspace if needed.

**Important:** `g5 push` publishes the current local workspace state to
the resolved remote publish target. This may be:

- the already-bound remote child workspace
- a child workspace created on first publish
- an explicit target passed as `g5 push <workspace-url>`

`g5 push` still does **not** implicitly mutate a parent workspace unless
that parent itself is the explicit publish target. Parent updates in the
usual derived-workspace flow happen only via explicit `merge` or via
server-side job completion.

**Cloud worker variant:**
- Workers report task results against a workspace version base. The API
  merges those repo-level results into the job workspace and creates a new
  job workspace version. When the job completes, the API merges the job
  workspace back into its parent workspace and creates a new parent
  workspace version.
- Workers do not use `--git-remote`. SCM push is triggered server-side
  when the relevant workspace merge completes (if configured).

**Current code:** `cli/push.rs` (CLI) / `tools/task_lifecycle.rs` (worker)

#### Op: REFRESH — Fetch + advance to latest workspace version

**What it does:**
Sync local workspace state from g5-api and advance the local checkout to
the latest remote workspace version. Optionally also fetch from git
remotes.

**CLI usage:**
```bash
# Refresh from g5-api only (default)
g5 refresh

# Also fetch from git remotes (GitHub/GitLab origin)
g5 refresh --git-remote
g5 refresh --git-remote=origin,upstream

# Refresh a specific workspace path
g5 refresh --path /home/user/my-workspace
```

**`--git-remote` flag:**
By default, `g5 refresh` fetches the latest manifest and HEAD from
g5-api. With `--git-remote`, it also runs `git fetch` against the
specified git remotes on the SCM provider. This pulls commits pushed
by other developers directly to GitHub/GitLab that haven't been
mirrored by g5-api's `repo_fetcher` yet.

**Semantics:**
1. Fetch the latest manifest and head workspace version from g5-api.
2. Apply safe-additive manifest updates (add new repos, update remote
   URLs). Never delete local repos.
3. For each repo:
   a. If `--git-remote`, `git fetch <remote>` to refresh optional upstream
      tracking metadata.
   b. Resolve the pinned commit for the latest workspace version.
   c. If local checkout already matches that pinned commit → nothing.
   d. If local checkout is behind and has no unpublished work →
      fast-forward to the pinned commit.
   e. If local unpublished work exists → rebase or replay it onto the
      latest workspace version. If conflicts occur, stop and report them.
4. Update `.g5/workspace.yaml` with the new workspace version metadata.
5. Print summary: repos fetched, workspace version advanced, conflicts.

**Auto-rebase invariant:** After a successful `g5 refresh`, the local
workspace is always at or ahead of the remote head workspace version —
never behind it. Diverged histories are resolved via rebase/replay,
never merge commits.

**Current code:** `cli/refresh.rs` (fetch-only, no auto-rebase — needs
extension)

#### Op 7: CLEANUP — Tear down the isolated environment

**What it does:**
Disconnect CRDT, release resources, remove temporary files.

**CLI usage:**
```bash
# After push, clean up the fork
g5 cleanup ./my-task

# Or clean up the current workspace/fork
g5 cleanup

# Force cleanup of a path that is not a valid workspace
g5 cleanup --force /tmp/broken-dir
```

**Semantics:**
- Validates that the target path contains a `.g5/workspace.yaml`
  unless `--force` is given
- Disconnect CRDT peer (flush final deltas, close WebSocket)
- Shut down isolation environment (Kata microVM, process sandbox)
- Remove git worktree or fork directory
- Close terminal sessions, stop port scanner
- Cloud workers: pod terminates; caches persist on node SSD
- Local: user manages their own directory lifecycle

**Current code:** `lifecycle.rs::phase_cleanup`

#### Op 8: MERGE — Merge changes from another workspace

**What it does:**
Merge one workspace into another. This uses the same rebase-or-fast-forward
mechanism as task→job promotion or job→parent merge-back (§10.5). The
merge operates per-repo: for each repo that exists in **both** the source
and target workspace, the source repo state is rebased onto (or
fast-forwarded into) the target workspace's current repo state. The result
is a new target workspace version.

**CLI usage:**
```bash
# Merge another workspace into the current one
g5 merge <source-workspace>

# Merge by workspace name or ID
g5 merge auth-refactor
g5 merge ws-uuid-1234

# Dry run — show what would be merged without applying
g5 merge auth-refactor --dry-run
```

**Semantics:**

The merge follows the same linear-history model as task → job workspace and
job workspace → parent workspace merges:

1. **Resolve source workspace** — look up by name or ID via g5-api
   (`GET /api/v1/workspaces?name=<name>` or by UUID).
2. **Resolve source version** — use the source workspace's current head
   version.
3. **Match repos** — for each repo in the source workspace version, find
   the corresponding repo in the target workspace by `repo_id`. Repos that
   exist only in the source are added to the target workspace. Repos that
   exist only in the target are untouched.
4. **Per-repo merge** — for each matched repo pair, use the shared repo
   merge primitive to fast-forward or rebase the source repo state onto
   the target repo state.
5. **Create new target version** — if all required repo merges succeed,
   create a new target workspace version whose repo pins reflect the merge
   result. Source-only repos are added to the target manifest and pinned
   to the source state. Repos outside the source subset inherit their
   existing pins.
6. **Report** — print per-repo summary: merged, added, skipped, conflicted.

```rust
// ops::merge_workspace — pure git implementation (g5-common/src/ops/)
// No DB dependency — usable by workers, CLI, and API alike.
// The API wraps this with DB transaction logic in service::merge_workspace_api().
pub fn merge_workspace(
    source_branches: &[WorkspaceBranch],  // from source workspace
    target_branches: &[WorkspaceBranch],  // from current workspace
    repo_storage: &dyn RepoStorage,
) -> Result<Vec<MergeResult>, G5Error> {
    let mut results = Vec::new();

    for source_wb in source_branches {
        // Find matching target branch by repo_id
        let target_wb = match target_branches.iter()
            .find(|t| t.repo_id == source_wb.repo_id)
        {
            Some(t) => t,
            None => {
                results.push(MergeResult::Added {
                    repo_id: source_wb.repo_id,
                    new_head: source_wb.head_commit.clone(),
                });
                continue;
            }
        };

        let repo_path = repo_storage.local_path(&source_wb.repo_id.to_string())
            .ok_or(G5Error::RepoNotFound)?;
        let repo = GitRepo::open(&repo_path)?;

        // Same ops::merge() primitive used by task→job, job→base:
        // try fast-forward, fall back to rebase, reject on conflict.
        //
        // IMPORTANT: ops::merge() returns the target SHA **without**
        // advancing the git ref.  The caller (merge_workspace_api)
        // advances all refs only after every repo succeeds, so a
        // conflict in repo B cannot leave repo A's ref stranded at a
        // new SHA while the DB transaction rolls back.
        match ops::merge(
            &repo,
            &source_wb.branch_name,
            &target_wb.branch_name,
            None,  // no CoW path in API-side merge
        ) {
            Ok(MergeOutcome::FastForwarded { new_head }) => {
                results.push(MergeResult::FastForwarded {
                    repo_id: source_wb.repo_id,
                    new_head,
                    target_ref: target_wb.branch_name.clone(),
                });
            }
            Ok(MergeOutcome::Rebased { new_head }) => {
                results.push(MergeResult::Rebased {
                    repo_id: source_wb.repo_id,
                    new_head: new_head.to_string(),
                    target_ref: target_wb.branch_name.clone(),
                });
            }
            Ok(MergeOutcome::AlreadyUpToDate) => {
                // Nothing to do — source is already merged.
            }
            Err(e) => results.push(MergeResult::Conflict {
                repo_id: source_wb.repo_id,
                error: e.to_string(),
            }),
        }
    }
    Ok(results)
}
```

**API endpoint:**
```
POST /api/v1/workspaces/:target_id/merge
{
  "source_workspace_id": "uuid-source",
  "dry_run": false
}
```
Response:
```json
{
  "repos_merged": 2,
  "repos_added": 1,
  "repos_conflicted": 0,
  "details": [
    { "repo": "backend", "status": "fast-forwarded", "new_head": "abc123" },
    { "repo": "frontend", "status": "rebased", "new_head": "def456" },
    { "repo": "docs", "status": "added", "new_head": "789abc" }
  ]
}
```

**Authorization:** The caller must have write access to the target
workspace and read access to the source workspace. Cross-org merge is
allowed as long as the caller has the current effective permissions
required on both sides, including any rights needed to add source-only
repos into the target workspace. These checks are evaluated at merge
time using the caller's current effective permissions.

**Audit:** Cross-org merges (where `source.org_id != target.org_id`)
must emit a distinct audit log event (`workspace.merge.cross_org`)
that records both org IDs, the caller, the source and target workspace
IDs, and the list of repos transferred. This is required for
compliance visibility — cross-org data movement should be easily
filterable in the audit log without scanning every merge event.

**Relationship to existing merge_workspace():** `g5 merge` is
**workspace-to-workspace**. Internal repo refs may still be used to
implement the merge, but the user-facing operation is always source
workspace → target workspace by repo ID matching. It uses the same
underlying `ops::merge()` primitive that task promotion and job
merge-back use.

**Typical workflows:**

```bash
# 1. Fork a subset from a local parent, work on it, merge back
g5 clone https://g5.example.com/acme/my-project
cd my-project
g5 fork --repos backend,frontend --name auth-refactor --target ../auth-work
cd ../auth-work
# ... agent works on backend + frontend ...
g5 commit -m "implemented auth"
g5 push
cd ../my-project
g5 merge auth-refactor   # apply only backend + frontend back into my-project

# 2. Fork a subset directly from a remote workspace when full clone is too large
g5 fork https://g5.example.com/acme/my-project \
  --repos backend,frontend \
  --name auth-refactor \
  --target ../auth-work

# 3. Merge another team's workspace
g5 merge team-b-workspace --dry-run  # preview first
g5 merge team-b-workspace            # apply
```

**Current code:** Not yet implemented. Will use `ops::merge_workspace()`
in `g5-common/src/ops/merge.rs`, called by `cli/merge.rs` (CLI) and
`workspace_service.rs` (API).

### 2.3 Local User with External Agent

A key design goal is that a user can perform the same workflow with their
coding agent of choice (Cursor, Copilot, aider, etc.) on their local
machine. The CLI operations are the interface:

```bash
# 1. Clone the workspace
g5 clone https://g5.example.com/acme/my-project

# 2. Fork with live sync to browser
cd my-project
g5 fork --connect --target ../my-task

# 3. Work with any agent — changes stream to browser in real-time
cd ../my-task
cursor .   # or: aider, copilot, vim, etc.

# 4. Commit and push when done (push auto-disconnects CRDT)
g5 commit -m "implemented auth module"
g5 push

# 5. Clean up
g5 cleanup ../my-task
```

Alternatively, connect/disconnect explicitly:
```bash
g5 fork --target ../my-task
cd ../my-task
g5 connect                  # backgrounds automatically, prints PID
# ... work ...
g5 disconnect               # stop syncing
g5 commit -m "done"
g5 push
```

The user authenticates once via `g5 auth login https://g5.example.com`
(stores a token in `~/.g5/credentials.yaml`) or sets `G5_TOKEN`. The
clone URL determines which instance and org to use; all subsequent
commands read the instance from `.g5/workspace.yaml`.

### 2.4 Operation Summary Table

| Op       | CLI Command     | Worker (programmatic)            | Shared Code Path              |
|----------|-----------------|----------------------------------|-------------------------------|
| CLONE    | `g5 clone <workspace-url> [--version <sha>]` | `ops::checkout::acquire()`       | `g5-common/src/ops/checkout.rs` |
| FORK     | `g5 fork [<workspace-url>]` | `ops::branch::ensure_task_branch()` | `g5-common/src/ops/branch.rs` |
| CONNECT  | `g5 connect`   | `CrdtPeer::connect()`           | `g5d/src/crdt/`                |
| DISCONNECT| `g5 disconnect`| `CrdtPeer::disconnect()`        | `g5d/src/crdt/`                |
| EXECUTE  | (external)      | `AgentLoop::run()`               | `g5d/src/agent/`               |
| COMMIT   | `g5 commit`    | `ops::commit::stage_and_commit()`| `g5-common/src/ops/commit.rs`  |
| PUSH     | `g5 push`      | `ops::push::push_with_rebase()`  | `g5-common/src/ops/push.rs`    |
| MERGE    | `g5 merge`     | `ops::merge_workspace()`   | `g5-common/src/ops/merge.rs`   |
| CLEANUP  | `g5 cleanup`   | `lifecycle::phase_cleanup()`     | `g5-common/src/ops/cleanup.rs` |
| LIST     | `g5 forks`     | —                                | `g5d/src/cli/forks.rs`         |

**`g5 cleanup [<path>] [--force]`** — removes a cloned workspace
directory and any associated local state (credential files, CRDT socket).
Without `<path>`, cleans the current directory. By default the path must
contain a valid `.g5/workspace.yaml`; `--force` bypasses that guard.
Equivalent to `rm -rf` but ensures `CredentialGuard` cleanup runs and the
workspace is deregistered from the local workspace index.

**`g5 forks [--workspace <id>]`** — lists all local fork workspaces for a
workspace, showing lineage, current version SHA, repo subset, and
dirty/clean status. Useful for managing multiple concurrent agent runs.

### 2.5 Instance Resolution

Every command that talks to g5-api needs to know *which* instance to
contact. For workspace lifecycle commands, resolution is intentionally
simple:

```
clone-time input:
  g5 clone <workspace-url>

persisted workspace state:
  .g5/workspace.yaml -> remote.api_url + workspace_version_sha
```

**At clone time**, the positional argument is always a full URL:
```
g5 clone https://g5.example.com/acme/my-project
```
The URL is parsed into `(api_url, org_slug, workspace_slug)` and all
three are written into `.g5/workspace.yaml`. From that point on, every
command in that workspace directory resolves the instance automatically.

`g5 clone` does not accept a bare workspace ID. This avoids a second
instance-resolution mechanism and keeps the user-facing model aligned with
the workspace URL they actually copied from the UI.

If a user wants a subset of repos without cloning the full parent
workspace first, they use:
```bash
g5 fork https://g5.example.com/acme/my-project --repos backend,frontend
```

**Multiple instances:** A developer can have workspaces from different G5
instances on the same machine. Each workspace's `.g5/workspace.yaml`
tracks its own `api_url`, so `g5 push` in `~/work/acme-proj` talks to
`https://g5.acme.com` while `g5 push` in `~/work/beta-proj` talks to
`https://g5-beta.example.com` — no reconfiguration needed.

**Authentication:** Credentials are stored per-instance in
`~/.g5/credentials.yaml`:
```yaml
instances:
  https://g5.acme.com:
    token: "eyJ..."
  https://g5-beta.example.com:
    token: "eyJ..."
```
`g5 auth login <instance-url>` initiates the auth flow and stores the
token. Commands look up the token by matching `remote.api_url` against
the credentials file.

**URL normalisation:** Instance URLs are normalised before storage and
lookup: strip trailing slash, enforce `https://`, lowercase the host.
This prevents token lookup misses when the user types
`https://g5.acme.com/` in one place and `https://g5.acme.com` in
another. The normalisation function lives in `g5-common/src/url.rs`.

### 2.6 Deployment Topology

| Component       | Replicas | Resources          | Node Pool        | NFS Access   |
|-----------------|----------|--------------------|------------------|--------------|
| g5-api          | 3        | 500m–2 CPU, 512Mi–2Gi | system          | read-write   |
| indexer         | 1        | 250m–1 CPU, 256Mi–1Gi | system          | read-write   |
| g5d worker      | 5        | 2–8 CPU, 4–16Gi    | worker (Kata/KVM)| none         |
| edge (Caddy)    | 2        | 100m–500m, 64–256Mi | system          | none         |

### 2.7 `g5` CLI and `g5d` Worker

The user-facing CLI is `g5`. The worker/daemon entrypoint remains `g5d`.
Current module names and source paths may still live under `g5d/` until
the implementation catches up with the spec.

**`g5` CLI commands** (developer workstation):
- `g5 init` — create an empty workspace
- `g5 clone` — materialise a workspace from g5-api
- `g5 add` — add a repository to the workspace
- `g5 refresh` — fetch + auto-rebase to latest HEAD (§2.2 REFRESH)
- `g5 commit` — commit CRDT draft to git
- `g5 push` — push to g5, optionally also to git remotes
- `g5 fork` / `g5 revert` / `g5 doctor`
- `g5 worker start|stop|status|logs` — manage a local `g5d worker`
  daemon for macOS, local Linux, or self-hosted servers

**`g5d` worker mode** (cloud pod):
- `g5d worker` — poll g5-api for task assignments, run the agent loop
  inside an isolated environment (Kata microVM or process sandbox)

### 2.8 Indexer Pipeline (Already Server-Side)

The `G5_ROLE=indexer` process runs the same `g5-api` binary and handles:

1. **repo_fetcher** — periodic `git fetch` for tracking-mode branches,
   updates `head_commit` in Postgres, enqueues `search_reindex_queue`
2. **search_indexer** — polls queue → `SearchService::reindex_workspace_branch`
   → updates `search_documents` (tsvector + pgvector)
3. **ontology_reconciler** — diffs changed files → inserts/updates/deletes
   `ontology_nodes` rows
4. **linked_doc_indexer** — parses markdown frontmatter → updates
   `doc_node_links`

All four run atomically within `process_single_entry`.

The indexer is a writer to bare repos (clone + fetch), but it is **not**
the sole writer. Additional writers include:
- push flows (`ops::push`) from CLI/worker paths
- API merge/advance flows in `task_service` and `workspace_service`
- worker post-task branch integration (`rebase`/`merge`) before reporting

### 2.9 What Happens When a Repo Is Added

```
Browser → POST /api/v1/orgs/:org/workspaces/:ws/repos
  → workspace_service.add_repo()     — inserts workspace repo metadata
  → job_service.create_job("uplift") — atomic with the above
  → create child uplift job workspace from current parent workspace version
  → commit
  → repo_fetcher sweep               — clones the repo to NFS
  → search_reindex_queue entry        — triggers the 3-processor pipeline
  → worker picks up uplift job        — runs bootstrap analysis
```

No CLI command is involved. The browser drives the entire flow.

### 2.10 Shared Ops Layer — Single Entry Point

Every git-mutating workspace operation — regardless of whether it
originates from a CLI command, a cloud worker lifecycle phase, or an
API-side service — must flow through a single shared `ops` module.
This eliminates divergent code paths, makes behaviour changes
atomic, and ensures the rebase-clean invariant is enforced
consistently.

**Where the ops module lives:** `g5-common/src/ops/` (shared crate).

```
┌────────────┐   ┌───────────────┐   ┌─────────────┐
│ CLI (g5)   │   │ Worker (g5d)  │   │ API (g5-api) │
│            │   │               │   │              │
│ cli/push   │   │ lifecycle /   │   │ task_service │
│ cli/commit │   │ worker/mod    │   │ job_service  │
│ cli/fork   │   │               │   │ ws_service   │
└─────┬──────┘   └──────┬────────┘   └──────┬───────┘
      │                 │                   │
      └─────────────────┼───────────────────┘
                        │
               ┌────────▼────────┐
               │  g5-common/ops  │  ← single entry point
               │                 │
               │ ops::clone()    │
               │ ops::fork()     │
               │ ops::commit()   │
               │ ops::rebase()   │
               │ ops::push()     │
               │ ops::ff_merge() │
               │ ops::refresh()  │
               │ ops::advance()  │
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │ g5-common/git   │  ← low-level libgit2
               │                 │
               │ GitRepo::       │
               │   rebase_onto_  │
               │   fast_forward_ │
               │   fetch_local() │
               │   fetch()       │
               │   commit()      │
               └─────────────────┘
```

**Key ops functions (proposed):**

| Function | Purpose | Callers |
|----------|---------|--------|
| `ops::clone(url, version, mode)` | Acquire a workspace at its head version or an explicit workspace version SHA | CLI `clone`, worker `phase_prepare` |
| `ops::fork(workspace_or_url, source_version, task_id, repos?)` | Prepare child workspace lineage + local materialisation (optionally subset of repos); remote child creation may be deferred until first publish | CLI `fork`, worker `create_task_workspace` |
| `ops::commit(workspace, message, author)` | `git add -A && git commit` | CLI `commit`, worker `phase_collect` |
| `ops::merge(repo, source_ref, target_ref)` | **Single per-repo merge primitive:** try fast-forward, fall back to rebase, reject on conflict. Computes result SHA **without advancing the target ref** — caller advances refs after all repos succeed (see §10.5.6). | Worker Phase 5 (task→job workspace), **API** Phase 7 (job workspace→parent), CLI `merge`, **API** `workspace_service` |
| `ops::merge_workspace(source_workspace, target_workspace, storage)` | Pure git layer (g5-common): match repos by ID, call `ops::merge()` per repo, return `Vec<MergeResult>`. No DB dependency. The API wraps this in `service::merge_workspace_api()` for DB transaction + workspace version creation. | CLI `merge`, **API** `workspace_service` |
| `ops::push(workspace, remote, git_remotes?)` | Publish a new workspace version + optional SCM remotes + rebase-on-conflict + retry | CLI `push`, worker (future: HTTPS push) |
| `ops::refresh(workspace, git_remotes?)` | Fetch manifest + advance local checkout to latest workspace version | CLI `refresh` |
| `ops::advance_branch(repo, branch, new_head)` | Internal repo-level primitive for advancing tracked refs; not a user-facing workspace concept | CLI `refresh`, **API** `repo_fetcher`, `workspace_service` |

**`ops::merge()` is the universal repo-level merge primitive.** Every
workspace-to-workspace merge — task workspace → job workspace, job
workspace → parent workspace, or arbitrary workspace → workspace — is
implemented by matching repos and calling the same per-repo merge:

```rust
ops::merge(
    &repo,               // GitRepo handle (bare or checkout)
    &source_branch,      // branch being merged in
    &target_branch,      // branch receiving changes
    cow_path: Option<&Path>,  // if CoW, fetch_local from here first
) -> Result<MergeOutcome, G5Error>

enum MergeOutcome {
    AlreadyUpToDate,
    FastForwarded { new_head: String },
    Rebased { new_head: String },
}
```

**Ref advancement policy:** `ops::merge()` computes the resulting SHA
(via fast-forward check or rebase replay) and returns `MergeOutcome`
**without advancing the target branch ref**. The caller is responsible
for advancing refs after all repos succeed. This is critical for
multi-repo atomicity: if repo B conflicts after repo A succeeds, no
git refs have been moved and no rollback of git state is needed — only
the DB transaction rolls back. The `ops::merge_workspace()` function
collects all `MergeOutcome` results and the API-side
`service::merge_workspace_api()` advances refs in bulk only after confirming
all repos merged successfully.

The old `ops::rebase_task()`, `ops::ff_into_base()`, and
`ops::merge_branch()` are replaced by this single function.
`ops::merge_workspace()` loops over matched repos calling
`ops::merge()` for each.

**API binding:** The API server links against `g5-common` as a library
dependency. It calls `ops::merge()` directly — no subprocess, no RPC,
no shelling out to `g5` or `g5d`. The ops functions are pure Rust, take a
`&GitRepo` handle and return `Result<MergeOutcome, G5Error>`. This is
what "bind to the internal" means.

**Invariant enforcement:** Because all callers go through the same
`ops::merge()`, the rebase-clean invariant (§10.5) is enforced in
one place. The fast-forward-then-rebase logic lives in a single
function — all consumers (worker Phase 5, API Phase 7, CLI `merge`,
`workspace_service`) get the same behaviour automatically.

#### 2.10.1 Draft-Aware Ops — Policy in Ops, Mechanism Injected

The ops layer must own the **rules** about when an operation is blocked
by dirty state — not the callers. If draft policy lives in the caller,
every caller reimplements (or forgets) the check independently, and
behaviour diverges. Instead, the ops layer defines a `DraftState` trait
that exposes the **data** the ops layer needs:

```rust
// Defined in g5-common/src/ops/mod.rs
/// Implementations must be safe for concurrent calls from multiple
/// async tasks.  Use interior synchronization (e.g., `Mutex<Inner>`)
/// to protect shared state — the `Send + Sync` bound guarantees
/// thread-safety at the type level, but callers rely on the
/// implementation being correct under concurrent `flush()` calls.
pub trait DraftState: Send + Sync {
    /// Number of unmaterialized deltas on this branch.
    fn dirty_count(&self, branch: &str) -> Result<u32, G5Error>;

    /// Flush pending CRDT deltas to disk/storage.
    /// No-op if no CRDT peer is connected.
    /// Must be safe to call concurrently for different branches.
    fn flush(&self, branch: &str) -> Result<(), G5Error>;
}
```

The **policy** — what to do with that data — lives in the ops functions:

```rust
pub fn advance_branch(
    repo: &GitRepo,
    branch: &str,               // logical branch name (e.g. "main")
    branch_sha: &str,           // current SHA the branch points to
    new_head: &str,             // target SHA to advance to
    target_ref: &str,           // git ref to update (e.g. "refs/heads/main")
    draft: &dyn DraftState,     // mandatory — can't skip
) -> Result<String, G5Error> {
    let dirty = draft.dirty_count(branch)?;
    if dirty > 0 {
        return Err(G5Error::DraftDirty {
            branch: branch.to_string(),
            dirty_count: dirty,
        });
    }
    // ... fast-forward or rebase (pure git) ...
}

pub fn commit(
    repo: &GitRepo,
    branch: &str,               // branch name for draft flush scope
    message: &str,
    author: &GitAuthor,
    draft: &dyn DraftState,
) -> Result<String, G5Error> {
    draft.flush(branch)?;       // ensure all deltas on disk before commit
    // ... git add -A + commit ...
}
```

Callers provide a `DraftState` implementation appropriate to their
context:

| Caller | `DraftState` impl | `dirty_count()` | `flush()` |
|--------|-------------------|-----------------|-----------|
| CLI | `CliDraftState` | `git status` + query `GET /api/v1/.../draft-status` | Flush local CRDT peer |
| Indexer | `IndexerDraftState` | `SELECT dirty_count FROM draft_sessions` | No-op (bare repo) |
| API service | `ApiDraftState` | Same query, in-process | No-op (server has no peer) |
| Worker (CRDT connected) | `CrdtDraftState` | Query CRDT peer dirty delta count | Flush CRDT peer to disk |
| Worker (isolated, no CRDT) | `NoOpDraftState` | Always returns 0 | No-op |

**`NoOpDraftState` advisory:** `NoOpDraftState` must only be used when
the caller is provably not connected to a CRDT peer. If the worker is
connected to CRDT (chat tasks, `--connect` forks, any interactive
workflow), it MUST use `CrdtDraftState` instead. Using `NoOpDraftState`
when a CRDT peer is active risks committing partial state — the
`flush()` call that ensures all inbound deltas are materialized to disk
before `git add` would be silently skipped. Callers should assert at
construction time: `assert!(!crdt_peer.is_connected())` before
instantiating `NoOpDraftState`.

This design gives the ops layer draft awareness without coupling it to
any specific draft storage mechanism. The rules are centralized:

- `advance_branch` **blocks** on dirty drafts — everywhere, always.
- `commit` **flushes** before committing — everywhere, always.
- `push` **flushes + commits** dirty drafts before pushing, or blocks.

**What stays outside ops:** DB writes (`UPDATE jobs SET head_commit`),
WebSocket events, and task state transitions remain in the caller (API
service layer or worker orchestrator). The ops module handles git + draft
guards — no DB, no network, no side effects beyond the filesystem.

---

## 3. Worker Registration Token Scoping

### 3.1 Current State

Today, worker registration uses a single static `G5_SERVICE_TOKEN` shared
by all workers. The token is a plain string compared against an env var —
there is no scoping. Any worker with the token can register against any
org and poll tasks from any workspace.

```rust
// Current: single shared token, auto-assigned to first org
let org_id = match body.org_id {
    Some(id) => id,
    None => sqlx::query_scalar!("SELECT id FROM orgs ORDER BY id LIMIT 1")
        .fetch_optional(conn).await?.flatten(),
};
```

**Target end state:** remove the `G5_SERVICE_TOKEN` / `G5D_SERVICE_TOKEN`
super-token bypass entirely. Development environments should seed the
`g5labs` org and use normal scoped worker tokens/API keys against that
org, rather than relying on a privileged global backdoor.

### 3.2 Required: Scoped Registration Tokens

Worker registration tokens must be scoped to control which tasks a worker
can access. This is critical for:
- **Multi-tenancy:** org A's code must never run on org B's worker
- **Local agents:** a user's personal worker should only access their
  workspaces
- **Compliance:** audit trail of which worker processed which org's data

#### Token Scopes

| Scope        | Issued by           | Worker can access           | Use case                    |
|--------------|---------------------|-----------------------------|-----------------------------|
| **Org**      | Org admin           | All workspaces in the org   | Shared org worker pool      |
| **Workspace**| Workspace owner     | Single workspace only       | Dedicated project worker    |
| **User**     | User (self-service) | Workspaces the user can access, optionally narrowed at startup | Local agent on laptop |

#### Token Structure

```json
{
  "sub": "worker:<worker-id>",
  "scope": "org",
  "org_id": "uuid",
  "workspace_id": null,
  "user_id": null,
  "iat": 1709654400,
  "exp": 1709740800,
  "capabilities": ["uplift", "downlift", "chat"]
}
```

For user-scoped tokens, the worker inherits the user's RBAC permissions.
At startup, a local worker may further narrow its assignments with:

- `--org <org-id>`
- `--workspace <workspace-id>`
- `--capability <chat|uplift|downlift>`

These filters are conjunctive with the token scope. A user-scoped token
does not force the worker to accept every workspace the user can access.

**Capability filter meaning:** `--capability` narrows which task families
the worker is willing to accept.

- `chat` — interactive chat tasks and chat workspaces
- `uplift` — uplift/bootstrap tasks for uplift jobs
- `downlift` — downlift/code-generation/refactor/review tasks for
  downlift jobs

If no capability filter is provided, the worker may accept any task
family allowed by its scope. Repeating `--capability` widens the allowed
set by union.

**Authorization invariant:** user-scoped worker permissions are evaluated
at the moment each operation occurs, not just when the token is minted or
when the worker starts. This applies to:

- task polling and assignment
- workspace acquisition / clone / fork
- result reporting and task completion
- `g5 push` / merge / publish operations
- internal git/object access granted through the API

If the user loses access before one of those operations occurs, the
operation is rejected even if the worker previously held the task.

The poll query filters tasks to only those the token's scope permits:

```sql
SELECT t.id FROM tasks t
JOIN jobs j ON j.id = t.job_id
JOIN workspaces w ON w.id = j.workspace_id
WHERE t.status = 'pending'
  -- Scope filtering:
  AND ($1::uuid IS NULL OR w.org_id = $1)           -- org scope
  AND ($2::uuid IS NULL OR j.workspace_id = $2)     -- workspace scope
  AND ($3::uuid IS NULL                             -- user scope: check
       OR j.created_by = $3                         -- workspace-level access
       OR EXISTS (
           SELECT 1 FROM workspace_permissions wp
           WHERE wp.workspace_id = j.workspace_id
             AND wp.user_id = $3
             AND wp.permission IN ('execute', 'admin')
       ))
```

**User-scope access check:** The user-scope filter checks
**workspace-level permissions**, not org membership. Being an org
member does not automatically grant worker access to every workspace
in the org. The user must have explicit `execute` or `admin` permission
on the specific workspace. This is evaluated via the `workspace_permissions`
table (or equivalent RBAC resolution), which respects the 3-tier
permission model (org default → user override → workspace override).
If no `workspace_permissions` table exists yet, the query should fall
back to the RBAC `AuthzService::check_permission()` call, serialized
per-candidate task.

#### Token Provisioning

```bash
# Org admin creates an org-scoped worker token
g5 auth create-worker-token --scope org --org <org-id>

# User creates a personal worker token for local agent use
g5 auth create-worker-token --scope user

# Workspace-scoped token
g5 auth create-worker-token --scope workspace --workspace <ws-id>
```

These worker registration tokens are also the user-facing **API keys**
for running a local worker daemon.

**Startup auth rule:** `g5 worker start` requires either:
- `--api-key <token>`
- or `G5_API_KEY` in the environment

The CLI does **not** persist worker API keys to disk.

#### Local Worker Daemon Management

Users should be able to run a worker on their own machine or server
without invoking `g5d` directly:

```bash
# Start a local worker against a specific instance
g5 worker start \
  --name osx-dev \
  --api-url https://g5.example.com \
  --api-key <g5k_...> \
  --concurrency 2

# Or via environment variable
G5_API_KEY=<g5k_...> \
  g5 worker start --name osx-chat --api-url https://g5.example.com --capability chat

# Narrow a user-scoped worker to one workspace
G5_API_KEY=<g5k_...> \
  g5 worker start --name ws-dev --api-url https://g5.example.com --workspace <ws-id>

# Inspect and manage the local daemon
g5 worker status
g5 worker status --name osx-dev
g5 worker logs --name osx-dev
g5 worker stop --name osx-dev
```

**Semantics:**
- `g5 worker start` is a user-facing wrapper around `g5d worker`
- it requires either `--api-key` or `G5_API_KEY`
- it validates the target instance URL and API key, writes non-secret
  local worker config, and spawns the background daemon
- `--concurrency <n>` controls how many tasks the local worker may
  execute concurrently in isolated environments
- multiple local workers on the same machine are allowed; each worker
  has a local `name`
- startup filters such as `--org`, `--workspace`, and `--capability`
  narrow assignment intake in addition to token scope
- `g5 worker status` with no name lists all local workers; `stop` and
  `logs` accept `--name` to target a specific one
- local metadata lives in `~/.config/g5/workers/<name>.yaml`
- pidfiles and logs live in `~/.local/state/g5/workers/<name>.pid` and
  `~/.local/state/g5/workers/<name>.log`
- this is the correct flow for macOS development, local Linux
  development, and user-managed/self-hosted worker machines

This is distinct from a future fully local execution mode such as
`g5 run --local ...`, which would bypass remote polling entirely.

### 3.3 Implementation Plan

1. **Add `worker_tokens` table** — stores token metadata (scope, org_id,
   workspace_id, user_id, created_by, expires_at, revoked_at)
2. **Token minting endpoint** — `POST /api/v1/orgs/:org/worker-tokens`
   with scope parameter, requires appropriate RBAC role
3. **Registration validation** — `register_worker()` validates the token
   scope and restricts `org_id`/`workspace_id` on the worker row
4. **Poll filtering** — `poll_for_task()` SQL adds scope predicates,
   plus optional startup narrowing for org/workspace/capability filters
4a. **Operation-time auth checks** — result reporting, merge/push, and
    internal repo/object access re-check current user/token permission
    and startup narrowing, not just assignment-time scope
5. **CLI command** — `g5 auth create-worker-token`
6. **Local worker wrapper** — `g5 worker start|stop|status|logs`
   manages background `g5d worker` processes
7. **Local worker metadata** — implement named-worker config, pidfile,
   and log paths; allow multiple local workers per machine
8. **Concurrency controls** — add `--concurrency <n>` and wire it to
   isolated task-slot scheduling on the local worker
9. **Startup auth model** — require `--api-key` or `G5_API_KEY`; do not
   persist worker API keys to disk
10. **401 behavior** — on token expiry/revocation, the worker exits and
    surfaces an actionable error; restart requires a fresh key
11. **Access-loss behavior** — if current RBAC or startup narrowing no
    longer permits an in-flight operation, reject publish/report/merge
    and return the task for retry or escalation rather than silently
    completing it under stale authorization

### 3.4 Worker Secrets Bucket

Workers **never provision credentials themselves**. The only credential
a worker starts with is its **registration token** (§3.2). Everything
else arrives per-task inside an opaque secrets bucket on the
`TaskAssignment`.

**Current state:** `TaskAssignment` carries `scm_credentials:
Vec<ScmCredential>` and `git_author`. LLM API keys are read from worker
env vars (`OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`).

#### 3.4.1 SCM Credentials Are No Longer Needed

Because the API acts as a **git mirror** (§2.8 — `repo_fetcher` syncs
from the external SCM into server-side bare repos), workers never talk
to GitHub/GitLab directly:

- **Clone:** worker clones from the API's bare repo, authenticated with
  its registration token over the internal git endpoint.
- **Push:** worker pushes back to the API mirror, same auth.
- **SCM push:** when `--git-remote` is requested, the *API* pushes to
  the external SCM using `ScmTokenManager` — the worker is not involved.
- **Fetch (rebase base update):** worker fetches from the API mirror.

This means `TaskAssignment.scm_credentials` is **unused in the current
mirror-only flow** — the worker authenticates all git operations with
its registration token. SCM credentials (`ScmTokenManager`, OAuth
tokens) stay inside the API boundary.

**However, the field is retained** for a future scenario: keeping the
G5 mirror continuously in sync with an external git repository. In that
mode the API would populate `scm_credentials` so workers can push
directly to the external SCM as part of the task lifecycle (e.g. after
a successful rebase, push to both the API mirror and GitHub in one step).
Until that feature lands, `scm_credentials` is sent as an empty vec.

#### 3.4.2 Per-Task Secrets

Beyond the registration token and the (currently empty) SCM credentials,
the worker needs **LLM API keys** per-task. All per-task secrets are
consolidated into `TaskSecrets`:

```rust
#[derive(Serialize, Deserialize)]
pub struct TaskSecrets {
    /// SCM credentials — empty in mirror-only mode. Populated when
    /// workers need direct SCM access (future: continuous sync).
    #[serde(default)]
    pub scm: Vec<ScmCredential>,
    /// LLM provider key. API resolves the correct key for the org.
    pub llm: Option<LlmCredential>,
    /// Arbitrary key/value pairs for future extension (e.g. artifact
    /// registry tokens, custom tool auth). Treated as opaque bytes.
    pub custom: HashMap<String, SecretString>,
}

#[derive(Serialize, Deserialize)]
pub struct LlmCredential {
    pub provider: LlmProvider,  // Anthropic | OpenAI | OpenRouter
    pub api_key: SecretString,
    pub model: Option<String>,  // override default model selection
}
```

#### 3.4.3 Credential Lifecycle on the Worker

1. `poll_for_task()` → API returns `TaskAssignment` with `secrets` field
2. Worker sets `LLM_API_KEY` env var for the task subprocess (scoped to
   task, not the worker process)
3. If `secrets.scm` is non-empty, worker configures `CredentialGuard`
   for external SCM auth (existing RAII zero-overwrite cleanup)
4. Git operations against the API mirror use the registration token —
   no credential file needed
5. On task completion (success or failure), `CredentialGuard` drops
   (if created) and task-scoped env vars are cleared
6. Worker returns to polling — no secrets persist between tasks

**What this simplifies (current mirror-only flow):**

- Workers don't need SCM credentials at startup or per-task
- `CredentialGuard` is only instantiated when `secrets.scm` is
  non-empty (future continuous-sync mode)
- Workers reading LLM keys from host env vars → replaced by
  API-dispatched keys per-org
- No "downscoped" git tokens needed — registration token suffices

**What remains:**

| Credential | Source | Scope | Lifetime |
|---|---|---|---|
| Registration token | Env var / config file | Worker process | Until expiry/rotation |
| SCM credentials | `TaskSecrets.scm` | Per-task (future) | `CredentialGuard` drop |
| LLM API key | `TaskSecrets.llm` | Per-task | Cleared on task completion |
| Custom secrets | `TaskSecrets.custom` | Per-task | Cleared on task completion |

**Transport security:** `TaskSecrets` is serialised in the
`poll_for_task` response body over mTLS. The field is marked
`#[serde(skip_serializing_if = "Option::is_none")]` in API responses
that don't include secrets (e.g. task status queries).

---

## 4. Chat Task Priority

### 4.1 Current State

The task poll query orders by:
```sql
ORDER BY
    CASE WHEN t.task_type = 'bootstrap' THEN 0 ELSE 1 END,
    t.created_at ASC
```

This prioritises bootstrap tasks (correctly — they unblock the job
pipeline) but treats chat tasks the same as all other task types. A user
sending a chat message while workers are busy with a batch uplift job
will wait until a worker becomes free.

### 4.2 Required: Chat Always Has Priority

Chat tasks are interactive — a human is waiting for a response. They must
always be scheduled before non-interactive work (uplift, downlift, test,
review). **Latency target:** P99 scheduling latency < 2 seconds
(time from chat message submission to worker picking up the task).
In workspace terms, chat tasks advance an interactive chat workspace and
must preempt batch uplift/downlift workspaces.
The priority order should be:

```
0. Chat tasks         — user is waiting, sub-second scheduling
1. Bootstrap tasks    — unblocks the job pipeline
2. Review tasks       — human review checkpoint, semi-interactive
3. All other tasks    — batch work (uplift, downlift, test, compile)
```

Within each priority tier, tasks are ordered by `created_at ASC` (FIFO).

### 4.3 Implementation

Update the poll query's ORDER BY:

```sql
ORDER BY
    CASE
        WHEN j.job_type = 'chat' THEN 0         -- chat always first
        WHEN t.task_type = 'bootstrap' THEN 1    -- bootstrap unblocks pipeline
        WHEN t.task_type = 'review' THEN 2       -- review is semi-interactive
        ELSE 3                                    -- batch work
    END,
    j.priority DESC,                              -- within tier, respect job priority
    t.created_at ASC                              -- within priority, FIFO
```

Additionally, chat jobs should be created with `priority = 100` so
that even if the ORDER BY logic changes, chat tasks sort first.
The CASE clause is the **primary** sort key (guaranteeing chat = 0,
bootstrap = 1, etc.); `j.priority DESC` is a **secondary** sort
within each tier. Higher priority values sort first within a tier:

```rust
// chat_service.rs::create_chat_job
let priority = 100;  // highest priority tier
```

#### Preemption (Future)

For true sub-second chat response, consider **preemption**: when a chat
task arrives and all workers are busy with batch work, the scheduler
signals one worker to checkpoint its current task (save progress, yield)
and pick up the chat task first. After the chat task completes, the worker
resumes the batch task from the checkpoint.

This requires:
- `POST /api/internal/workers/:id/preempt` endpoint
- Worker-side checkpoint + yield logic (reuse existing checkpoint infra)
- Task state transition: `running → yielded → pending` (re-enters queue)

Not needed immediately — increasing worker count or HPA scaling is
sufficient while batch job load is moderate.

---

## 5. Problem: Worker NFS Mount

### 5.1 The Bug

The current Helm template (`deployment-worker.yaml`) mounts the Filestore
PVC into worker pods:

```yaml
volumeMounts:
  - name: repos
    mountPath: /mnt/g5-repos
    readOnly: true
```

This is **incompatible with production Kata isolation**:

- **Security violation** — Kata microVMs exist to isolate untrusted code
  execution. Mounting the shared NFS (even read-only) exposes every org's
  repos to every task. A prompt-injection attack could read arbitrary repos.
- **Architectural mismatch** — the worker lifecycle already assumes remote
  acquisition: `phase_prepare` says *"Try GCS snapshot first, fall back to
  git clone --depth=1"*.
- **Scale limitation** — NFS throughput is shared; N workers doing
  concurrent tree reads during task startup contend on the same Filestore
  IOPS.

### 5.2 Intended Worker Lifecycle (No NFS)

```
1. PREPARE    — shallow clone from git remote (or GCS snapshot download)
               into microVM local filesystem
2. CLONE+PATCH — create job branch from base_commit, apply CRDT overlay
3. EXECUTE    — agent loop (code edits, tool calls, checkpoints)
4. COLLECT    — git add + commit, report commit_sha + files_changed via HTTP
5. CLEANUP    — tear down isolation env, flush CRDT, disconnect
```

Workers never write to shared storage. Results flow back through the API
(`POST /api/v1/internal/tasks/:id/transition`), and the indexer's
repo_fetcher eventually merges them.

**Cache policy note (March 5, 2026):**
- Workers use **local cache storage only** (L1 on local/pod disk).
- Treat remote/L2 cache as future work in this design.
- The worker cache must be a **real persistent filesystem path**, not a
  no-op, mock, or in-memory layer.

**Worker cache persistence model:** A registered worker with a given
token is expected to handle multiple tasks over its lifetime and should
re-use its local cache across tasks for maximum benefit (avoiding
redundant clones of the same repos). The cache directory
(`~/.g5/cache` or `/cache`) persists across task executions on the
same worker instance. The specific storage backing depends on the
deployment model:

| Deployment | Cache storage | Persistence |
|---|---|---|
| Cloud (long-lived pod) | Pod-local SSD (`emptyDir` with `medium: ""` on SSD-backed nodes) | Persists across tasks within the pod lifetime |
| Cloud (Kata microVM) | Host-mounted cache volume (read-only seed + CoW overlay) | Seed persists on node; overlay is per-VM |
| Self-hosted / local | User-configured directory (e.g. `~/.g5/cache`) | Persistent across worker restarts |

The cache is **not** shared between worker instances — each worker
maintains its own independent cache. **Workers are isolated by org** —
a worker registered to org A only receives tasks for org A and only
caches repos belonging to org A. Cross-org cache access is impossible
because workers never handle tasks from other orgs.

Cache invalidation uses a
**two-tier eviction policy**:

1. **TTL-based** — entries older than a configurable TTL (default: 24h)
   are evicted on the next task startup.
2. **LRU size-based** — when the total cache size exceeds a configurable
   maximum (default: 10 GiB, set via `G5_CACHE_MAX_SIZE`), the
   least-recently-used entries are evicted until the cache is under the
   limit. LRU eviction runs at the start of each task, after TTL
   eviction. Access time is tracked per cache entry (updated on cache
   hit).

The two policies are **independent** — an LRU hit does not reset the
entry's TTL. An entry can be evicted by TTL even if it was recently
accessed, and vice versa. Both policies are checked at task startup
before the clone/checkout step. The cache stores shallow clones and
packfiles keyed by `(repo_id, commit_sha)`; a cache
hit on the exact commit skips the clone entirely and copies from
the cache directory.

---

## 6. Worker Command Separation

### 6.1 Proposal

Keep the user-facing `g5` CLI and the `g5d` worker concerns cleanly
separated:

| Concern              | Cloud Worker                    | CLI (Developer)              |
|----------------------|---------------------------------|------------------------------|
| Repo acquisition     | Shallow clone / GCS snapshot    | `g5 clone`, `g5 add`       |
| Filesystem           | Ephemeral (microVM-local)       | Persistent (developer disk)  |
| NFS access           | **None**                        | N/A (no NFS)                 |
| Result delivery      | HTTP → g5-api                   | `g5 commit` + `g5 push`   |
| Auth                 | Service token (injected secret) | User token / OAuth           |
| Isolation            | Kata microVM / gVisor           | Process sandbox (optional)   |

### 6.2 Shared Code Path: CLI ↔ Worker

The worker lifecycle must reuse the **same underlying service functions**
that the CLI commands delegate to. Today these are duplicated:

| Operation          | CLI Command       | CLI Code Path                        | Worker Code Path (lifecycle.rs)         |
|--------------------|-------------------|--------------------------------------|-----------------------------------------|
| Clone/checkout     | `g5 clone`       | `cli/clone.rs` → fetch manifest, clone repos, init meta | `phase_prepare` + `phase_clone_patch` inline `GitRepo::open/checkout_branch/create_branch` |
| Stage + commit     | `g5 commit`      | `cli/commit.rs` → `git2::Index::add_all`, `commit` | `phase_collect` inline `repo.add_all()`, `repo.commit()` |
| Push to remote     | `g5 push`        | `cli/push.rs` → `push_repo()` with retry | Not implemented (results sent via HTTP) |

**The problem:** The worker's `phase_clone_patch` and `phase_collect`
reimplement git operations with raw `GitRepo` calls instead of calling
shared service functions. This means:

- Bug fixes in one path don't propagate to the other
- Behaviour diverges (e.g. CLI commit checks `git status` first, worker
  doesn't; CLI push retries 3x with backoff, worker has no push)
- The worker can't benefit from future CLI improvements (COW clone,
  independent docs repository handling, workspace.lock management)

**The fix:** Extract the core operations into a shared service layer in
`g5-common/src/ops/` so CLI handlers, worker lifecycle phases, and API
services all call the same implementation:

```
g5-common/src/ops/
├── checkout.rs    — clone or checkout repo at a commit, create branch
├── commit.rs      — stage all + commit with structured message
├── push.rs        — push to remote with retry (or via API for workers)
└── mod.rs
```

The worker lifecycle becomes:

```rust
// phase_prepare — reuses the same checkout logic as `g5 clone`
async fn phase_prepare(&self, assignment: &TaskAssignment) -> Result<(), G5Error> {
    ops::checkout::checkout_or_clone(
        &self.workspace_path,
        &assignment.git_url,
        &assignment.base_commit,
        CheckoutMode::ShallowClone,  // worker mode
        // vs CheckoutMode::CowClone for CLI
    ).await?;
    // ... cache hydration
}

// phase_clone_patch — reuses branch creation from `g5 clone`
async fn phase_clone_patch(&self, assignment: &TaskAssignment) -> Result<(), G5Error> {
    ops::checkout::ensure_branch(
        &self.workspace_path,
        &assignment.branch_name,
        &assignment.base_commit,
    )?;
    // ... CRDT overlay
}

// phase_collect — reuses commit logic from `g5 commit`
async fn phase_collect(&self, task_id: &str, ...) -> Result<...> {
    let (sha, files) = ops::commit::stage_and_commit(
        &self.workspace_path,
        &format!("feat(g5): task {} completed", task_id),
        &GitAuthor { name: "G5 Agent", email: "agent@g5.dev" },
    )?;
    // ... push via ops::push (remote for CLI, HTTP for worker)
}
```

The CLI commands become thin wrappers:

```rust
// cli/commit.rs
pub async fn execute(args: &CommitArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let workspace_path = resolve_workspace_path(...)?;
    let ws_config = WorkspaceConfig::load(&workspace_path)?;
    for repo in &ws_config.repos {
        ops::commit::stage_and_commit(&repo_path, &args.message, &author)?;
    }
}
```

### 6.3 Worker Acquisition Mode

The shared `ops::checkout` module supports multiple acquisition strategies
via a `CheckoutMode` enum:

```rust
pub enum CheckoutMode {
    /// Developer CLI: COW clone from local workspace or full clone
    CowClone,
    /// Worker (Phase 1): shallow clone from git remote with credentials
    ShallowClone { credentials: GitAuth },
    /// Worker (Phase 2): download + unpack GCS snapshot tarball
    GcsSnapshot { bucket: String, key: String },
    /// Dev cluster: open existing repo from NFS mount (legacy)
    LocalPath,
}
```

### 6.4 Worker Push Strategy

The shared `ops::push` module supports different push targets:

```rust
pub enum PushTarget {
    /// CLI: push to git remote (origin)
    GitRemote { remote: String, retry_count: u32 },
    /// Worker: push results via g5-api HTTP endpoint
    ApiEndpoint { api_client: Arc<InternalApiClient>, task_id: String },
}
```

Both use the same `stage_and_commit` → `push` pipeline, just with
different transport.

### 6.5 Required Changes

1. **Extract `g5-common/src/ops/` module** — shared checkout, commit,
   push, advance operations callable from CLI handlers, worker
   lifecycle, and API services (§2.10).

2. **Refactor CLI commands** — `cli/clone.rs`, `cli/commit.rs`,
   `cli/push.rs` become thin wrappers over `ops::*`.

3. **Refactor worker lifecycle** — `phase_prepare`, `phase_clone_patch`,
   `phase_collect` call `ops::*` instead of inline `GitRepo` operations.

4. **Helm: gate NFS mount on worker** — add `worker.filestore.enabled`
   (default: `false`), only mount when explicitly enabled (dev clusters).
   **Note:** The current `deployment-worker.yaml` template incorrectly
   gates the NFS mount on `api.filestore.enabled` — this must be
   changed to `worker.filestore.enabled` so workers can be toggled
   independently of the API server.

   ```yaml
   # deployment-worker.yaml
   {{- if .Values.worker.filestore.enabled }}
   volumeMounts:
     - name: repos
       mountPath: /mnt/g5-repos
       readOnly: true
   {{- end }}
   ```

5. **Worker PREPARE phase** — wire `ops::checkout` with
   `CheckoutMode::ShallowClone` (Phase 1) or `GcsSnapshot` (Phase 2).

6. **Worker COLLECT phase** — wire `ops::push` with
   `PushTarget::ApiEndpoint` so results flow back via HTTP.

7. **Values files** — set `worker.filestore.enabled: true` only in
   `values-dev.yaml` (non-Kata dev clusters). Prod/staging: `false`.

### 6.6 Optional: Binary Split

Maintain two entry points:
- `g5` — CLI-only (workspace lifecycle, developer UX)
- `g5d` — cloud worker / daemon (poll, execute, report)

Both link against the same `ops::*` crate. This reduces the attack
surface of the worker binary (no user-facing CLI parsing, no local git
auth) and simplifies container image size. They may still share the same
repo and most of the same Rust crates; the important contract is the
separate entrypoint and naming. `g5 worker start` remains a thin wrapper
that spawns and manages the `g5d worker` daemon locally.

---

## 7. API-Level Repo Mirroring

### 7.1 Current State

The indexer already mirrors repos server-side:

| Operation                  | Server-Side (Indexer)             | CLI                              |
|----------------------------|-----------------------------------|----------------------------------|
| Clone repo to storage      | `repo_fetcher` auto-clone         | `g5 clone`                      |
| Fetch latest commits       | `repo_fetcher` → `ops::advance_branch()` | `g5 refresh [--git-remote]`     |
| Advance local workspace to latest version | N/A (server has bare repos)       | `g5 refresh` → `ops::advance_branch()` |
| Push to g5 + SCM           | N/A                               | `g5 push [--git-remote]`        |
| Index search documents     | `search_indexer`                  | N/A                              |
| Reconcile ontology         | `ontology_reconciler`             | N/A                              |
| Index linked docs          | `linked_doc_indexer`              | N/A                              |
| Add repo to workspace      | `POST /api/v1/.../repos`          | `g5 add`                        |
| Serve file tree            | `GET /api/v1/.../files`           | local filesystem                 |
| Read file content          | `RepoStorage::read_file`          | local filesystem                 |

### 7.2 Implication

For the cloud deployment, **no CLI commands are needed for indexing**.
The browser triggers `add_repo`, the indexer clones and indexes, and
workers acquire code via shallow clone or snapshot. The CLI exists purely
for the local developer experience.

This means the ontology graph, file tree, and search index are all
rendered from database + API — no connected worker or CLI is required
to populate them. The worker's only job is analysis (uplift, downlift,
spec generation, chat).

---

## 8. Repo Storage Tradeoffs

### 8.1 Option A: Filestore NFS (Current)

**Architecture:**
- Bare repos on NFS at `/mnt/g5-repos/{repo_id}/`
- Worktrees on local SSD at `/tmp/g5-worktrees/{task_id}/`
- `O_CREAT|O_EXCL` lockfiles for mutual exclusion
- ReadWriteMany PVC across API + indexer pods

**Pros:**
- ✅ Already implemented and working
- ✅ POSIX filesystem — git operations work natively
- ✅ Sub-ms latency for file reads (same VPC, NFS wire speed)
- ✅ Simple mental model: repos are just directories
- ✅ ReadWriteMany across API replicas + indexer

**Cons:**
- ❌ ~$200/mo minimum (1 TiB BASIC_HDD floor)
- ❌ Single point of failure (one Filestore instance)
- ❌ Cannot scale storage independently of compute
- ❌ BASIC_HDD tier has modest IOPS (600 read, 1000 write baseline)
- ❌ Must NOT be mounted into Kata microVMs (security)
- ❌ Zonal — not multi-region resilient

**Cost:**
| Tier       | Capacity | Monthly   |
|------------|----------|-----------|
| BASIC_HDD  | 1 TiB    | ~$200     |
| BASIC_SSD  | 2.5 TiB  | ~$625     |
| Enterprise | 1 TiB    | ~$600     |

### 8.2 Option B: GCS + Database (Target)

**Architecture:**
- Bare repo packfiles stored as GCS objects: `gs://g5-repos/{repo_id}/pack-*.pack`
- File content addressable by `(repo_id, commit_sha, path)` → GCS object
- File metadata (tree structure, sizes, hashes) in Postgres
- Ontology, search index, linked docs already in Postgres (no change)
- Workers download snapshots from GCS (tarball or packfile)

**Implementation approach:**
1. Indexer runs `git fetch` into a **transient local checkout** (ephemeral
   disk or tmpfs on the indexer pod)
2. After fetch, indexer uploads the packfile to GCS and records file
   metadata in Postgres
3. Indexer deletes the local checkout (or keeps it as a cache with TTL)
4. API serves `list_file_tree` and `read_file` from Postgres metadata +
   GCS content (no local filesystem needed)
5. Workers download a snapshot tarball from GCS at task startup

**Pros:**
- ✅ ~$20/mo for 1 TiB (Standard storage class) — 10x cheaper
- ✅ No minimum capacity floor
- ✅ Scales storage independently, effectively infinite
- ✅ Multi-region / dual-region options for resilience
- ✅ Workers already need remote acquisition — GCS is natural
- ✅ Eliminates NFS from the cluster entirely (simpler infra)
- ✅ No Filestore IOPS bottleneck

**Cons:**
- ❌ Significant refactor of `RepoStorage` trait + all consumers
- ❌ 10-50ms per GCS object read vs sub-ms NFS
- ❌ Git operations need a local filesystem — indexer must unpack,
  operate, repack (or use `gitoxide` for direct packfile manipulation)
- ❌ More moving parts: GCS lifecycle policies, metadata sync, cache
  invalidation
- ❌ `list_file_tree` and `read_file` become database + GCS queries
  instead of `readdir` / `read` syscalls

**Cost:**
| Item               | Monthly         |
|--------------------|-----------------|
| GCS Standard 1 TiB | ~$20            |
| GCS operations     | ~$5–15 (varies) |
| Extra Postgres I/O | negligible      |
| **Total**          | **~$25–35**     |

### 8.3 Option C: Hybrid (Recommended Migration Path)

**Phase 1 (Now):** Keep NFS for API + indexer. Remove NFS mount from
workers. Workers use shallow clone from git remote.

**Phase 2 (Next):** Add GCS as a secondary backend. Indexer uploads
snapshots to GCS after each fetch. Workers switch from shallow clone
to GCS snapshot download (faster, no SCM credential needed in microVM).

**Phase 3 (Later):** Migrate API file-serving from NFS to GCS + Postgres
metadata. `RepoStorage` trait gets a `GcsRepoStorage` implementation.
Indexer uses ephemeral local disk instead of persistent NFS. Decommission
Filestore.

```
Phase 1                    Phase 2                    Phase 3
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│ API ←── NFS     │       │ API ←── NFS     │       │ API ←── GCS+DB  │
│ Indexer ←── NFS │       │ Indexer ──→ GCS │       │ Indexer → GCS   │
│ Worker ← clone  │       │ Worker ← GCS    │       │ Worker ← GCS   │
│                 │       │   (snapshot DL)  │       │                 │
│ Filestore: YES  │       │ Filestore: YES   │       │ Filestore: NO  │
│ GCS: NO         │       │ GCS: snapshots   │       │ GCS: primary   │
└─────────────────┘       └─────────────────┘       └─────────────────┘
     ~$200/mo                  ~$220/mo                   ~$35/mo
```

---

## 9. Immediate Action Items

### 9.1 Must Fix (Phase 1)

- [ ] **Fix CRDT inbound materialization bug** — `CrdtPeer::connect()`
      (mod.rs lines 231–263) uses raw `tokio::fs::write()` bypassing
      the Materializer entirely. Route all inbound deltas through
      `Materializer::write_file_atomic()` with pre-write content-hash
      check (§10.2). **This is a data-loss bug** and blocks the
      local-agent workflow (§2.3).
- [ ] **Extract `g5-common/src/ops/` module** — shared checkout, commit,
      push, advance operations used by CLI commands, worker lifecycle,
      and API services (§2.10, §6.2).
- [ ] **Refactor CLI commands** — `cli/clone.rs`, `cli/commit.rs`,
      `cli/push.rs` become thin wrappers over `ops::*`.
- [ ] **Refactor worker lifecycle** — `phase_prepare`, `phase_clone_patch`,
      `phase_collect` call `ops::*` with worker-specific modes
      (`ShallowClone`, `ApiEndpoint`) instead of inline git ops.
- [ ] **Helm: remove NFS mount from workers** — add `worker.filestore.enabled`
      flag, default `false`. Workers in Kata mode must have no NFS access (§5).
- [ ] **Worker PREPARE: implement shallow clone** — wire `ops::checkout`
      with `CheckoutMode::ShallowClone`. Workers clone from the API git
      mirror using their registration token — no SCM credentials needed
      (§3.4.1).
- [ ] **Values files** — set `worker.filestore.enabled: true` only in
      `values-dev.yaml` (non-Kata dev clusters). Prod/staging: `false`.
- [ ] **Scoped worker tokens** — create `worker_tokens` table, scoped
      registration endpoint, and per-org JWT claims (§3).
- [ ] **Worker secrets bucket** — implement `TaskSecrets` with LLM keys
      and custom secrets, dispatched at poll time (§3.4). In the current
      mirror-only flow, `scm_credentials` is sent as empty vec; the
      field is retained for future continuous-sync mode.
- [ ] **Chat task priority** — update `poll_for_task()` ORDER BY to
      prioritise chat tasks; set `priority = 100` in `create_chat_job()` (§4).
- [ ] **URL-based clone** — parse `g5 clone <url>` to extract api_url,
      org, and workspace; persist `remote.api_url` in workspace.yaml;
      add `parse_g5_url()` helper to cli module (§2.5).
- [ ] **Per-instance credentials** — `g5 auth login <url>` stores token
      in `~/.g5/credentials.yaml`; commands resolve token by matching
      `remote.api_url` (§2.5). Normalise instance URLs (strip trailing
      slash, enforce https) to prevent token lookup misses.
- [ ] **Task lease + TTL** — add `lease_expires_at`, `lease_renewals`,
      `max_duration_secs` columns to `tasks` table. Extend lease on
      heartbeat. Background reaper reclaims expired leases and fails
      tasks that exceed max_duration (§13).
- [ ] **Git binary in API container** — ensure the `git` binary is
      included in the API container image (Dockerfile). The smart HTTP
      protocol endpoints (`git-upload-pack`, `git-receive-pack`) shell
      out to `git` via subprocess with `--stateless-rpc`. **libgit2
      cannot be used directly** for the server-side smart HTTP protocol
      — it does not implement the stateless RPC mode needed by HTTP
      transport, and its transport layer is client-only. Add to
      Dockerfile: `RUN apt-get install -y git` (or equivalent for the
      base image). Verify with `docker run <image> git --version` in CI.
- [ ] **Git HTTPS serving** — implement smart HTTP git protocol on
      the API internal router (`/git/:repo_id/`). Four endpoints:
      `info/refs`, `git-upload-pack`, `git-receive-pack`. Auth via
      registration token over HTTP Basic (§14).
- [ ] **Push validation** — the `receive_pack` handler performs draft
      guard (reject push if dirty drafts), stale-push rejection, and
      auth re-check inline before spawning the git subprocess. Post-push
      side-effects (task notification, job advance, reindex, SCM mirror
      push) run inline after the subprocess completes (§14.4).
- [ ] **Worker git config** — change `workspace_git_url` from filesystem
      path to HTTPS URL. Configure git credential helper with the
      registration token (§14.5).
- [ ] **Remove G5_SERVICE_TOKEN bypass** — deprecate and remove the
      legacy `G5_SERVICE_TOKEN` / `G5D_SERVICE_TOKEN` env-var auth
      path. Replace worker bootstrap auth with the scoped worker
      tokens (above). Remove `ServiceTokenAuth` from auth middleware
      once all callers have migrated. This closes the infinite-TTL,
      all-scopes security gap documented in §9.4.2.

### 9.2 Should Do (Phase 2)

- [ ] **workspace-meta branch GC** — after a task branch is merged into
      its parent job branch, delete the `task/<id>` branch from
      workspace-meta. Same for completed job branches merged into `main`.
      Retain branches for 7 days after merge for debugging, then GC.
      Implement as a background reaper alongside the lease reaper (§13).
- [ ] **Indexer: GCS snapshot upload** — after `repo_fetcher` completes a
      fetch, create a tarball of the worktree at HEAD and upload to
      `gs://g5-repos/{repo_id}/snapshots/{commit_sha}.tar.zst`
- [ ] **Worker PREPARE: GCS snapshot path** — add
      `CheckoutMode::GcsSnapshot` to `ops::checkout`. Download + unpack
      the tarball instead of shallow clone. Faster and avoids passing
      SCM credentials into the microVM.
- [ ] **Snapshot GC** — lifecycle policy to delete snapshots older than N
      days (only latest per branch needed).
- [ ] **Local-user agent mode** — document and test `g5 clone` →
      `g5 fork` → external agent → `g5 commit` → `g5 push` flow
      for developers running their own coding agents locally (§2.7).
- [ ] **Token rotation CLI** — `g5 admin rotate-token <token-id>` for
      revoking / rotating worker registration tokens (§3).
- [ ] **TECH DEBT: Revisit gitoxide for server-side git** — As of
      March 2026, gitoxide's `gix-transport` has no server-side
      `accept()` and `gix-protocol` has no `receive-pack` handler
      (both are client-only). The plumbing crates (`gix-pack`,
      `gix-odb`, `gix-packetline`) are solid but assembling them into
      a server would be ~2000 LOC of uncharted protocol state machine.
      Monitor [gitoxide server-side tracking](https://github.com/GitoxideLabs/gitoxide)
      for `gix-transport` server support (`[ ] general purpose
      accept(…) for servers` in crate-status.md). When available,
      replace the subprocess `--stateless-rpc` model (§14.2) with
      native in-process git serving — eliminates subprocess spawns
      and removes the `git` binary dependency from the API container
      image.

### 9.3 Future (Phase 3)

- [ ] **`GcsRepoStorage`** — new `RepoStorage` implementation backed by
      GCS objects + Postgres metadata for tree structure
- [ ] **API file endpoints** — `list_file_tree` / `read_file` backed by
      DB + GCS instead of NFS readdir
- [ ] **Indexer ephemeral disk** — indexer uses tmpfs or ephemeral SSD
      for git operations, uploads results to GCS, discards local state
- [ ] **Decommission Filestore** — remove Terraform module, PV/PVC,
      firewall rules

### 9.4 API Key / PAT Management Gap

**Status: No infrastructure exists.**

G5 currently has **zero** user-managed API key or Personal Access Token
(PAT) infrastructure. There is no `api_keys` table, no CRUD endpoints,
no key rotation mechanism, and no scoped programmatic access model.
This section documents the gap and the consequences for §3 (worker
registration), §14 (git HTTPS serving), and CI/CD integration.

#### 9.4.1 Current Auth Token Inventory

| Token Type | Creation | TTL | Rotation | Revocation | Scoping | Storage |
|---|---|---|---|---|---|---|
| **User JWT** | OAuth callback or `/auth/refresh` | Configurable env var | Via `/auth/refresh` (sliding window) | Not directly — revoke refresh token | 18 typed `Scope` variants, 3-tier resolution (org → user → workspace) | Client-side (no server state) |
| **Refresh token** | OAuth callback | Configurable (sliding window) | Extended on each refresh | `DELETE FROM refresh_tokens` via `/auth/logout` | N/A — used only to mint new JWTs | DB (`refresh_tokens` table) + `httpOnly` cookie (`g5_refresh`) |
| **Legacy service token (remove)** | Manual env var provisioning | **Infinite** (no expiry) | Manual env var change + process restart | Remove env var + restart all processes | **None** — grants ALL 18 scopes including `BillingAdmin`, `OrgAdmin` | Env var only (`G5_SERVICE_TOKEN` / `G5D_SERVICE_TOKEN`) |
| **Worker session JWT** | `POST /api/internal/workers/register` (service token auth) | 15 min (`JwtMinter::DEFAULT_TTL_SECS = 900`) | Refreshed on heartbeat (`HeartbeatResponse.session_token`) | Natural expiry only | `Scope::Internal` only | In-memory (`InternalApiClient::update_token()` via `Arc<RwLock>`) |
| **WS ticket** | `POST /auth/ws-ticket` | 5 min | Not rotated | Natural expiry | Workspace-scoped | Client-side |
| **DataChannel token** | Minted by g5-api for browser→worker WebSocket | 5 min | Not rotated | Natural expiry | Worker + stream + permission scoped | Client-side |
| **Blob capability** | Minted by `AuthzService::mint_blob_capability()` | Configurable (seconds) | Not rotated | Natural expiry | Repo + OID + byte-limit scoped, Ed25519 signed | Client-side JWS |
| **SCM OAuth token** | SCM connect flow (`/auth/scm/:provider/connect`) | Provider-determined | Background refresh (GitLab `refresh_token`), manual re-auth (GitHub) | `DELETE /scm/:provider/disconnect` | Provider-determined (repo access) | DB, AES-256-GCM encrypted (`user_scm_connections.access_token_encrypted`) |
| **OAuth state** | Login/connect initiation | 10 min | Single-use (`DELETE` on consume) | Natural expiry | N/A | DB (`oauth_states` table) |

#### 9.4.2 The Service Token Problem

The legacy service token (`G5_SERVICE_TOKEN`) is the only current
programmatic access mechanism and it has critical security properties
that make it unsuitable as a permanent solution:

1. **No expiry.** The token is a static hex string in an env var. It
   never expires. Compromised tokens remain valid until an operator
   manually rotates them and restarts every process.

2. **No scoping.** When a service token hits a `/api/v1/` route, the
   auth middleware ([auth_middleware.rs L721–748](generation/g5-api/src/middleware/auth_middleware.rs#L721))
   constructs synthetic `JwtClaims` with **all 18 scopes**:
   `WorkspacesRead`, `WorkspacesWrite`, `JobsRead`, `JobsExecute`,
   `SessionsRead`, `SessionsWrite`, `BlobsRead`, `BillingAdmin`,
   `OrgAdmin`, `WorkersRead`, `SettingsRead`, `SettingsWrite`, etc.

3. **No identity.** Service token requests are attributed to a
   well-known synthetic user UUID `00000000-0000-0000-0000-000000000001`
   and a matching org UUID. There is no audit trail distinguishing
   which human or bot used the token.

4. **Shared secret.** The same token is shared by all workers, all E2E
   tests, and any CI/CD integration. A leak in one context compromises
   all.

5. **No revocation mechanism.** There is no JTI blocklist, no token
   revocation list, no DB state backing the service token. The only
   path to revoke is env var rotation + rolling restarts.

6. **Full blast radius.** An attacker with the service token gets:
   - Full API access on **all** routes (`/api/v1/` and `/api/internal/`)
   - Worker registration and task polling
   - Job creation and cancellation
   - Billing admin access
   - Org admin access (member management)
   - No rate limiting differentiation from normal users

**Normative end state:** this bypass is removed. Local development and
seeded environments should authenticate as normal principals against the
seeded `g5labs` org using scoped worker tokens / API keys, not a global
super-token.

#### 9.4.3 What's Missing — API Key / PAT Requirements

The following capabilities do not exist and are needed:

| Capability | Use Case | Priority |
|---|---|---|
| **Create API key** | User creates a scoped API key via UI or CLI | Phase 2 |
| **List API keys** | User sees all their active keys | Phase 2 |
| **Revoke API key** | User or admin revokes a specific key | Phase 2 |
| **Rotate API key** | User regenerates a key, old key invalidated | Phase 2 |
| **Scoped API keys** | Key has a subset of the user's scopes (e.g., `jobs:read` only) | Phase 2 |
| **Expiring API keys** | Key has a TTL, auto-expires | Phase 2 |
| **Per-worker keys** | Each worker gets a unique key (replaces shared service token) | Phase 1 (§3) |
| **Audit log** | Track which key was used for each API call | Phase 2 |
| **IP allowlist** | Key is restricted to specific CIDR ranges | Phase 3 |
| **Rate limiting by key** | Per-key rate limits independent of user | Phase 3 |

#### 9.4.4 Proposed Schema (Future)

```sql
CREATE TABLE api_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES organisations(id),
    user_id         UUID NOT NULL REFERENCES users(id),
    name            TEXT NOT NULL,                    -- human-readable label
    key_prefix      TEXT NOT NULL,                    -- first 8 chars for display ("g5k_abc1...")
    key_hash        BYTEA NOT NULL,                   -- argon2id hash of the full key
    scopes          TEXT[] NOT NULL DEFAULT '{}',      -- subset of user's effective scopes
    expires_at      TIMESTAMPTZ,                      -- NULL = no expiry
    last_used_at    TIMESTAMPTZ,
    last_used_ip    INET,
    revoked_at      TIMESTAMPTZ,                      -- soft-delete
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (org_id, key_prefix)
);

-- RLS: users can only see their own keys
ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY api_keys_tenant ON api_keys
    USING (org_id = current_setting('authz.org_id')::uuid);
```

Key format: `g5k_` prefix + 32 random bytes (base62) → `g5k_a1b2c3d4e5f6...`
(44 characters total). Hashed with argon2id before storage. Only the
prefix (`g5k_a1b2c3d4`) is stored in plaintext for display / lookup.

#### 9.4.5 Auth Middleware Changes (Future)

The auth middleware would need a third extraction path:

```
1. X-Service-Token header → ServiceTokenValidator (legacy, remove eventually)
2. Authorization: Bearer g5k_* → API key lookup (hash, validate, hydrate scopes)
3. Authorization: Bearer eyJ* → JWT validation (existing)
4. Authorization: Bearer <other> → /api/internal/ → legacy service token fallback
```

#### 9.4.6 Interaction with This Design

- **§3 (Worker Registration Tokens):** The `worker_tokens` table
  proposed in §3 is conceptually a specialisation of the API key model.
  It should share the same `key_prefix` + `key_hash` pattern.
- **§14 (Git HTTPS Serving):** Workers authenticate git operations via
  HTTP Basic with their registration token. If API keys exist, the
  git serving layer should accept API keys as well (for CLI users
  running `git push` directly).
- **CI/CD:** Today CI/CD uses the shared service token. API keys would
  allow scoped, auditable, revocable CI credentials.
- **CLI (`g5`):** `g5 auth login` currently stores a bearer token
  from `--auth-token` flag. API keys would provide a persistent
  credential that doesn't require OAuth re-auth.

### 9.5 Git Operation Callsite Refactoring Inventory

**Status: 52 callsites identified. 10 HIGH risk, 18 MEDIUM risk, 14 LOW risk, 10 acceptable/canonical.**

This section is an exhaustive inventory of every git operation in the
codebase that bypasses the proposed `ops::` layer (§2.10). Each entry
includes the file, line range, function, operation performed, what it
should use instead, and a risk assessment. This inventory is the work
breakdown for the "Extract `g5-common/src/ops/` module" action item
in §9.1.

#### 9.5.1 CLI Layer — `g5d/src/cli/`

All CLI commands use raw `git2::Repository` calls instead of the
`GitRepo` wrapper from `g5-common`. This is the largest concentration
of bypass callsites (24 total). Every CLI command needs refactoring.

##### C-1: `commit.rs` — `execute()` (manual add + commit per repo)

- **File:** `g5d/src/cli/commit.rs` lines 55–129
- **Function:** `execute()`
- **What it does:** `git2::Repository::open` per repo → `repo.statuses()`
  → `index.add_all()` → `index.update_all()` → `index.write()` →
  `index.write_tree()` → `repo.commit()` — fully manual stage and
  commit with a hardcoded `g5d@local` author signature.
- **Should use:** `ops::commit(repo_path, message, author)`
- **Risk:** **MEDIUM** — bypasses `GitRepo` wrapper's owner validation;
  hardcoded `g5d@local` signature will diverge from server-side author
  names; no CRDT flush before commit (files may be stale).

##### C-2: `commit.rs` / `mod.rs` — `commit_subdir()` (shared helper)

- **File:** `g5d/src/cli/mod.rs` lines 425–470
- **Callers:** `commit.rs` (docs + meta), `fork.rs`, `add.rs`,
  `push.rs`, `refresh.rs` — used across **5 subcommands**.
- **What it does:** `git2::Repository::open` → `statuses()` →
  `index.add_all()` → `index.write()` → `index.write_tree()` →
  `repo.commit()` — identical pattern to C-1 but for subdirectories
  (docs repo, meta repo).
- **Should use:** `ops::commit(repo_path, message, author)`
- **Risk:** **MEDIUM** — code is duplicated in 5 places; signature
  mismatch risk; the `commit.rs` file contains an additional copy
  at L144–158 that delegates to this helper.

##### C-3: `push.rs` — `execute()` (open repos + push)

- **File:** `g5d/src/cli/push.rs` lines 57–117
- **Function:** `execute()`
- **What it does:** `git2::Repository::open` per workspace repo →
  reads HEAD → calls `push_repo()` on each. Iterates all repos
  sequentially.
- **Should use:** `ops::push(repo_path, remote, refspec, auth)`
- **Risk:** **HIGH** — pushes to remote with no authentication
  callbacks; will fail silently on private repos.

##### C-4: `push.rs` — `push_repo()` (raw git2 remote push)

- **File:** `g5d/src/cli/push.rs` lines 278–293
- **Function:** `push_repo()`
- **What it does:** `repo.find_remote("origin")` →
  `remote.push(&[&refspec], None)` — raw `git2` push with
  **no authentication callbacks** (the `PushOptions` are `None`).
- **Should use:** `ops::push()` with `GitAuth` credential management
- **Risk:** **HIGH** — bypasses entire auth layer; will fail on
  private repos; could leak data if the wrong remote is configured;
  no push receipt verification.

##### C-5: `push.rs` — independent docs repository push

- **File:** `g5d/src/cli/push.rs` lines 128–145
- **Function:** `execute()` (docs branch)
- **What it does:** `git2::Repository::open(&docs_dir)` → reads HEAD →
  `push_repo(&docs_repo, "origin", "main")`.
- **Should use:** `ops::push()`
- **Risk:** **MEDIUM** — push with no auth; separate from code repos.

##### C-6: `push.rs` — meta repo open

- **File:** `g5d/src/cli/push.rs` lines 211–226
- **Function:** `execute()` (meta repo)
- **What it does:** `git2::Repository::open(&meta_dir)` — opened only
  to confirm existence, then `commit_subdir()` is called.
- **Should use:** `ops::commit()`
- **Risk:** **LOW** — meta bookkeeping.

##### C-7: `add.rs` — clone repo

- **File:** `g5d/src/cli/add.rs` line 71–73
- **Function:** `execute()`
- **What it does:** `git2::Repository::clone(&args.repo_url, &full_clone_path)`
  — no auth, no COW.
- **Should use:** `ops::clone(url, dest, auth, cow_opts)`
- **Risk:** **MEDIUM** — no credential support; no COW optimization;
  full copy even on same filesystem.

##### C-8: `add.rs` — checkout remote branch

- **File:** `g5d/src/cli/add.rs` lines 96–120
- **Function:** `execute()`
- **What it does:** `repo.find_branch()` → `repo.find_reference()` →
  `repo.branch()` → `repo.checkout_tree()` → `repo.set_head()` —
  manual branch create + checkout on raw `git2` repo (5 raw calls).
- **Should use:** `ops::checkout_branch(repo_path, branch_name)`
- **Risk:** **MEDIUM** — bypasses `GitRepo.checkout_branch()` safety;
  no owner validation; error handling is per-call instead of atomic.

##### C-9: `add.rs` — meta repo open + commit_subdir

- **File:** `g5d/src/cli/add.rs` lines 188–199
- **What it does:** `git2::Repository::open(&meta_dir)` → writes
  manifest → `commit_subdir()`.
- **Should use:** `ops::commit()`
- **Risk:** **LOW** — meta bookkeeping.

##### C-10: `fork.rs` — clone repos from source workspace

- **File:** `g5d/src/cli/fork.rs` lines 131–142
- **Function:** `execute()`
- **What it does:** `git2::Repository::clone(source_repo_path.to_str(), &target_repo_path)`
  — local clone per workspace repo. Full copy.
- **Should use:** `ops::clone()` with COW (reflink on XFS/Btrfs,
  `cp --reflink=auto` fallback).
- **Risk:** **MEDIUM** — wastes disk space; no reflink; O(n) on repo
  size instead of O(1).

##### C-11: `fork.rs` — init meta repo

- **File:** `g5d/src/cli/fork.rs` lines 156–187
- **What it does:** `git2::Repository::init(&meta_dir)` → writes
  workspace.yaml → `commit_subdir()`.
- **Should use:** `ops::init()` + `ops::commit()`
- **Risk:** **LOW** — meta bookkeeping.

##### C-12: `clone.rs` — init meta repo

- **File:** `g5d/src/cli/clone.rs` lines 120–143
- **What it does:** `git2::Repository::init(&meta_dir)` → file writes →
  `commit_meta_repo()`.
- **Should use:** `ops::init()` + `ops::commit()`
- **Risk:** **LOW** — meta bookkeeping.

##### C-13: `clone.rs` — clone workspace repos

- **File:** `g5d/src/cli/clone.rs` lines 151–162
- **What it does:** `git2::Repository::clone(&repo_entry.url, &repo_path)`
  per repo — no auth, no COW.
- **Should use:** `ops::clone(url, dest, auth, cow_opts)`
- **Risk:** **MEDIUM** — no credential support; no COW; fails on
  private repos.

##### C-14: `clone.rs` — `commit_meta_repo()` (duplicated helper)

- **File:** `g5d/src/cli/clone.rs` lines 196–230
- **Function:** `commit_meta_repo()`
- **What it does:** Takes `&git2::Repository`, does
  `index.add_all()` → `index.write_tree()` → `repo.commit()`.
  This **duplicates** the `commit_subdir()` pattern from `mod.rs`
  but accepts a repo reference directly instead of a path.
- **Should use:** `ops::commit()`
- **Risk:** **LOW** — code duplication of same commit pattern.

##### C-15: `init.rs` — init meta repo + stage + commit

- **File:** `g5d/src/cli/init.rs` lines 100–143
- **Function:** `execute()`
- **What it does:** `git2::Repository::init(&meta_dir)` →
  `index.add_all()` → `index.write_tree()` → `repo.commit()` — fully
  manual init + stage + commit with hardcoded author.
- **Should use:** `ops::init()` + `ops::commit()`
- **Risk:** **LOW** — workspace init, but duplicates commit logic.

##### C-16: `init.rs` — clone docs repo

- **File:** `g5d/src/cli/init.rs` lines 156–164
- **What it does:** `git2::Repository::clone(docs_repo_url, &docs_dir)`.
- **Should use:** `ops::clone()`
- **Risk:** **LOW** — docs repo, typically public.

##### C-17: `refresh.rs` — fetch existing repos

- **File:** `g5d/src/cli/refresh.rs` lines 69–79
- **Function:** `execute()`
- **What it does:** `git2::Repository::open(&repo_path)` →
  `repo.find_remote("origin")` → `remote.fetch(…)` — raw `git2`
  fetch with **no auth callbacks**.
- **Should use:** `ops::fetch(repo_path, remote, auth)`
- **Risk:** **MEDIUM** — will fail on private repos needing auth.

##### C-18: `refresh.rs` — clone new repos from manifest

- **File:** `g5d/src/cli/refresh.rs` lines 158–168
- **What it does:** `git2::Repository::clone(&repo_entry.url, &repo_path)`.
- **Should use:** `ops::clone()`
- **Risk:** **MEDIUM** — no credential support.

##### C-19: `refresh.rs` — fetch existing repos (manifest path)

- **File:** `g5d/src/cli/refresh.rs` lines 173–185
- **What it does:** Same pattern as C-17 for repos listed in manifest.
- **Should use:** `ops::fetch()`
- **Risk:** **MEDIUM** — same as C-17.

##### C-20: `refresh.rs` — meta repo open + commit

- **File:** `g5d/src/cli/refresh.rs` lines 211–228
- **What it does:** `git2::Repository::open(&meta_dir)` → writes →
  `commit_subdir()`.
- **Should use:** `ops::commit()`
- **Risk:** **LOW**.

##### C-21: `revert.rs` — hard reset

- **File:** `g5d/src/cli/revert.rs` lines 55–101
- **Function:** `execute()`
- **What it does:** `git2::Repository::open(&repo_path)` →
  `repo.statuses()` → `repo.find_commit(oid)` →
  `repo.reset(commit, ResetType::Hard, None)` — **destructive hard
  reset** using raw `git2`.
- **Should use:** `ops::reset_hard(repo_path, target_commit)`
- **Risk:** **HIGH** — `reset --hard` discards all uncommitted work;
  no guard checks whether CRDT has dirty state that needs flushing
  or resetting; no confirmation prompt; no atomic CRDT ↔ git state
  synchronisation.

##### C-22: `doctor.rs` — git version check

- **File:** `g5d/src/cli/doctor.rs` line 195
- **Function:** `check_git()`
- **What it does:** `ProcessCommand::new("git").arg("--version")` —
  subprocess to check git installation.
- **Should use:** N/A — diagnostic check, acceptable as-is.
- **Risk:** **ACCEPTABLE** — read-only version check.

#### 9.5.2 Worker Layer — `g5d/src/worker/`

Worker code uses the `GitRepo` wrapper from `g5-common` (better than
the CLI's raw `git2`), but still inlines multi-step git operations
that should be single `ops::` calls.

##### W-1: `mod.rs` — workspace setup (CoW path: create branch + checkout)

- **File:** `g5d/src/worker/mod.rs` lines 343–366
- **Function:** `create_task_workspace()`
- **What it does:** `GitRepo::open(&task_dir)` →
  `repo.create_branch(&task_branch, &base_commit)` →
  `repo.checkout_branch(&task_branch)` — sets up task branch in
  CoW workspace.
- **Should use:** `ops::setup_task_branch(workspace_path, task_branch, base_commit)`
- **Risk:** **MEDIUM** — branch creation + checkout as two separate
  calls; if checkout fails, orphan branch remains; no rollback.

##### W-2: `mod.rs` — workspace setup (reuse path: same pattern)

- **File:** `g5d/src/worker/mod.rs` lines 396–405
- **Function:** `create_task_workspace()` (reuse existing workspace)
- **What it does:** Same as W-1 but on an existing workspace directory.
- **Should use:** `ops::setup_task_branch()`
- **Risk:** **MEDIUM** — same as W-1.

##### W-3: `mod.rs` — worktree fallback (open source repo)

- **File:** `g5d/src/worker/mod.rs` line 413
- **Function:** `create_task_workspace()`
- **What it does:** `GitRepo::open(source_repo_path)` — opens source
  repo for `TaskWorktree::create` delegation.
- **Should use:** `ops::setup_task_workspace()` (orchestrates both
  CoW + worktree paths).
- **Risk:** **ACCEPTABLE** — delegates to existing worktree implementation.

##### W-4: `mod.rs` — `ensure_git_safe_directory()` subprocess

- **File:** `g5d/src/worker/mod.rs` lines 457–470
- **Function:** `ensure_git_safe_directory()`
- **What it does:** `Command::new("git").args(["config", "--global",
  "--add", "safe.directory", …])` — subprocess to configure
  `safe.directory` in global git config.
- **Should use:** `ops::configure_safe_directory()` or centralise
  in `g5_common::git::safety`.
- **Risk:** **ACCEPTABLE** — config-only, but modifies global git config;
  should be centralised so all components agree on safe directories.

##### W-5: `mod.rs` — open repo for tool registry

- **File:** `g5d/src/worker/mod.rs` line 1000
- **Function:** `poll_execute_loop()` (task execution setup)
- **What it does:** `GitRepo::open(&workspace_path)` — opens repo
  and wraps in `Arc` for use by all agent tools.
- **Should use:** Acceptable (this creates the shared repo handle
  used by all tools). Could be `ops::open()` for consistency.
- **Risk:** **ACCEPTABLE** — single open point for tool registry.

##### W-6: `mod.rs` — mirror task commits back to source repo

- **File:** `g5d/src/worker/mod.rs` lines 1216–1280
- **Function:** `poll_execute_loop()` (post-task cleanup)
- **What it does:** `GitRepo::open(&source_repo_path)` →
  `source_repo.fetch_local(tw.path(), &[&refspec])` →
  `source_repo.merge_branch_no_checkout(&task_branch, &job_branch, &author)`
  — mirrors CoW task branch back to source repo and merges into
  job branch.
- **Should use:** `ops::merge(source_repo, task_branch, job_branch, cow_path)`
  (the unified merge operation from §2.10).
- **Risk:** **HIGH** — `merge_branch_no_checkout` can produce merge
  commits with concurrent workers creating conflicting merge parents;
  `fetch_local` failure can silently lose commits; no lock on the
  source repo during fetch+merge.

##### W-7: `lifecycle.rs` — checkpoint (add + commit + diff)

- **File:** `g5d/src/worker/lifecycle.rs` lines 250–280
- **Function:** `checkpoint()`
- **What it does:** `GitRepo::open(&self.workspace_path)` →
  `repo.add_all()` → `repo.commit(…)` → `repo.diff_name_only()` —
  inline checkpoint commit.
- **Should use:** `ops::checkpoint(workspace_path, message, author)`
- **Risk:** **MEDIUM** — duplicates `GitManager::checkpoint()` logic;
  no CRDT flush guard before commit.

##### W-8: `lifecycle.rs` — `phase_clone_patch()` (complex branch setup)

- **File:** `g5d/src/worker/lifecycle.rs` lines 368–440
- **Function:** `phase_clone_patch()`
- **What it does:** `GitRepo::open(…)` → `repo.current_head()` →
  `repo.checkout_branch(branch_name)` →
  `repo.checkout_commit(base_commit)` →
  `repo.delete_local_branch(branch_name)` →
  `repo.create_branch(branch_name, base_commit)` →
  `repo.checkout_branch(…)` — **~70 lines** of inline git logic
  with sentinel commit resolution and branch re-creation.
- **Should use:** `ops::setup_task_branch(workspace_path, branch_name, base_commit)`
  — single operation: resolve sentinel → delete stale branch →
  create fresh branch → checkout → verify HEAD.
- **Risk:** **HIGH** — detached HEAD risk if re-attach fails after
  `checkout_commit()`; stale branch deletion with no recovery;
  multiple failure modes at each step not centrally handled;
  hardest callsite to reason about correctness.

##### W-9: `lifecycle.rs` — `phase_collect()` (add + commit + diff)

- **File:** `g5d/src/worker/lifecycle.rs` lines 595–625
- **Function:** `phase_collect()`
- **What it does:** `GitRepo::open(&self.workspace_path)` →
  `repo.add_all()` → `repo.commit(…)` → `repo.diff_name_only()`.
- **Should use:** `ops::collect_commit(workspace_path, message, author)`
- **Risk:** **MEDIUM** — duplicates the checkpoint pattern from W-7.

#### 9.5.3 CRDT Layer — `g5d/src/crdt/`

##### CR-1: `mod.rs` — inbound materialisation bypass

- **File:** `g5d/src/crdt/mod.rs` line 237
- **Function:** inbound materialisation `tokio::spawn` loop
- **What it does:** `tokio::fs::write(&full_path, &content)` — writes
  CRDT content directly to the working tree, **bypassing the
  `Materializer`** entirely.
- **Should use:** `Materializer::write_file_atomic()` (temp file →
  rename pattern with fsync).
- **Risk:** **HIGH** — **data-loss bug.** Non-atomic write: partial
  writes produce corrupted files visible to git; no fsync guarantee;
  no temp-file-then-rename; can race with `git add` / `git commit`
  in the worker; already tracked in §9.1 action item.

#### 9.5.4 API Services — `g5-api/src/services/`

API services use the `GitRepo` wrapper but inline multi-step git
operations that should be atomic `ops::` calls. Several of these
operate on shared repos with no locking.

##### A-1: `draft_service.rs` — pre-commit check (read-only)

- **File:** `g5-api/src/services/draft_service.rs` lines 593–601
- **Function:** `pre_commit_check()`
- **What it does:** `self.repo_storage.local_path(…)` →
  `GitRepo::open(&repo_path)` → `repo.diff_summary(…)` →
  `repo.count_commits_between(…)` — read-only diff for conflict
  detection.
- **Should use:** `ops::diff_check(repo_path, base, head)`
- **Risk:** **LOW** — read-only; but opens repo outside any
  transaction context.

##### A-2: `draft_service.rs` — `commit_draft()` (CAS commit pipeline)

- **File:** `g5-api/src/services/draft_service.rs` lines 816–903
- **Function:** `commit_draft()`
- **What it does:** `self.repo_storage.local_path(…)` →
  `GitRepo::open(&repo_path)` → `git_repo.checkout_branch(…)` →
  materialise CRDT files to working tree →
  `git_repo.add_all()` → `git_repo.commit(…)` — **the most critical
  callsite: the full CAS commit pipeline.**
- **Should use:** `ops::draft_commit(repo_path, branch, files, message, author)`
  — orchestrate: lock → checkout → materialise → add → commit →
  unlock.
- **Risk:** **HIGH** — checkout + materialise + commit must be
  atomic; concurrent API requests committing to the same branch can
  corrupt the working tree (interleaved checkout/add/commit);
  no file lock around the sequence; push is currently disabled
  but the inline structure makes re-enabling error-prone.

##### A-3: `task_service.rs` — auto-merge at job completion

- **File:** `g5-api/src/services/task_service.rs` lines 1554–1595
- **Function:** `transition_task()` (auto-merge block)
- **What it does:** `self.repo_storage.local_path(…)` →
  `GitRepo::open(&repo_path)` →
  `repo.resolve_branch_head(job_branch)` →
  `repo.fast_forward_ref("origin", base_branch, &job_head, …)` —
  merges job branch into workspace base branch upon final task
  completion.
- **Should use:** `ops::merge(repo_path, job_branch, base_branch)`
  (the unified merge from §2.10).
- **Risk:** **HIGH** — fast-forward ref update on a shared repo
  with no locking; concurrent job completions can race; if
  `fast_forward_ref` fails (non-ff), the DB is still updated to
  `completed` state, leaving git and DB state inconsistent.

##### A-4: `workspace_service.rs` — `try_fast_forward_merge()`

- **File:** `g5-api/src/services/workspace_service.rs` lines 1743–1815
- **Function:** `try_fast_forward_merge()`
- **What it does:** `GitRepo::open(&repo_path)` →
  `git_repo.resolve_ref_flexible(…)` →
  `git_repo.is_ancestor_of(…)` →
  `git_repo.fast_forward_ref(…)` **or**
  `git_repo.rebase_onto_ref(…)` — full merge/rebase logic inline
  (~70 lines).
- **Should use:** `ops::merge(repo_path, source_branch, target_branch)`
  with fast-forward → rebase fallback.
- **Risk:** **HIGH** — rebase rewrites history; no lock around
  concurrent merges on same branch; data loss if rebase encounters
  conflicts and silently produces empty commits.

##### A-5: `workspace_service.rs` — `resolve_head_for_branch()` (read-only)

- **File:** `g5-api/src/services/workspace_service.rs` lines 1831–1845
- **Function:** `resolve_head_for_branch()`
- **What it does:** `self.repo_storage.local_path(…)` →
  `GitRepo::open(&repo_path)` →
  `repo.resolve_ref_flexible(…)` — read-only HEAD resolution.
- **Should use:** `ops::resolve_head(repo_path, branch)`
- **Risk:** **LOW** — read-only.

##### A-6: `comment_service.rs` — open repo for context (read-only)

- **File:** `g5-api/src/services/comment_service.rs` lines 1182–1184
- **Function:** `open_repo_for_workspace()`
- **What it does:** `self.repo_storage.local_path(…)` →
  `GitRepo::open(&repo_path).ok()` — opens repo for read-only
  context (file blame, etc.).
- **Should use:** `ops::open_read_only(repo_path)`
- **Risk:** **LOW** — read-only.

##### A-7: `job_service.rs` — fork job branch

- **File:** `g5-api/src/services/job_service.rs` lines 419–441
- **Function:** `create_job()`
- **What it does:** `self.repo_storage.local_path(…)` →
  `GitRepo::open(&repo_path)` →
  `repo.create_branch(branch, head_sha)` — creates job branch at
  fork point.
- **Should use:** `ops::create_branch(repo_path, branch, commit)`
- **Risk:** **MEDIUM** — branch creation happens outside the DB
  transaction; if the DB commit fails, an orphan branch remains in
  the git repo with no corresponding job record.

##### A-8: `job_service.rs` — rollback merge

- **File:** `g5-api/src/services/job_service.rs` lines 992–1035
- **Function:** `rollback_job()`
- **What it does:** `GitRepo::open(&repo_path)` →
  `repo.fast_forward_ref("origin", base_branch, original_head, …)`
  — reverts workspace branch HEAD to the pre-job fork point after
  rollback.
- **Should use:** `ops::rollback_merge(repo_path, base_branch, fork_commit)`
- **Risk:** **HIGH** — moves a branch ref backwards; if the
  `fork_commit` lookup is wrong, can point the branch at the wrong
  commit; no lock; no verification that the current HEAD is actually
  beyond `fork_commit`.

##### A-9: `ontology_service.rs` — subprocess git diff

- **File:** `g5-api/src/services/ontology_service.rs` lines 743–759
- **Function:** `get_node_diff()`
- **What it does:** `tokio::process::Command::new("git").arg("diff").arg(&base)…`
  — invokes git as a subprocess instead of using `GitRepo::diff_summary()`.
- **Should use:** `ops::diff()` or `GitRepo::diff_summary()`
- **Risk:** **LOW** — read-only; but subprocess invocation is slower,
  requires `git` binary on PATH, and bypasses the `GitRepo` wrapper.

#### 9.5.5 Tools Layer — `g5d/src/tools/`

Agent-callable git tools that wrap `GitRepo` methods directly instead
of going through `ops::`.

##### T-1: `task_lifecycle.rs` — `collect_phase()` (add + commit + push)

- **File:** `g5d/src/tools/task_lifecycle.rs` lines 376–412
- **Function:** `collect_phase()`
- **What it does:** `self.git_repo.add_all()` →
  `self.git_repo.commit(…)` →
  `self.git_repo.push("origin", "HEAD", &auth)` — inline
  add + commit + push.
- **Should use:** `ops::collect_commit_push()` or unify with
  `GitManager::commit_and_push()`.
- **Risk:** **MEDIUM** — push can fail leaving a local-only commit;
  duplicates `GitManager::commit_and_push()` logic; push uses
  `GitAuth::None` as fallback.

##### T-2: `git_ops.rs` — `GitCommitTool::execute()` (agent commit)

- **File:** `g5d/src/tools/git_ops.rs` lines 308–328
- **Function:** `GitCommitTool::execute()`
- **What it does:** `self.git_repo.add_all()` →
  `self.git_repo.commit(message, &author)` — agent-initiated commit.
- **Should use:** `ops::commit()`
- **Risk:** **MEDIUM** — agent-callable; should go through `ops::`
  for audit trail and rate limiting; at least uses proper author
  from `TaskContext`.

##### T-3: `git_ops.rs` — `GitPushTool::execute()` (agent push)

- **File:** `g5d/src/tools/git_ops.rs` line 405
- **Function:** `GitPushTool::execute()`
- **What it does:** `self.git_repo.push("origin", branch, &auth)` —
  agent-initiated push.
- **Should use:** `ops::push()`
- **Risk:** **MEDIUM** — agent can push arbitrary branches; should
  go through `ops::` for branch validation and rate limiting.

##### T-4: `git_ops.rs` — `GitPullTool::execute()` (agent fetch)

- **File:** `g5d/src/tools/git_ops.rs` line 476
- **Function:** `GitPullTool::execute()`
- **What it does:** `self.git_repo.fetch("origin", &auth)` — fetch
  only (no merge/rebase).
- **Should use:** `ops::fetch()`
- **Risk:** **LOW** — read-only fetch.

##### T-5: `git_ops.rs` — `GitFetchTool::execute()` (agent fetch)

- **File:** `g5d/src/tools/git_ops.rs` line 539
- **Function:** `GitFetchTool::execute()`
- **What it does:** `self.git_repo.fetch("origin", &auth)` — same
  fetch pattern as T-4.
- **Should use:** `ops::fetch()`
- **Risk:** **LOW** — read-only fetch.

#### 9.5.6 Daemon API Layer — `g5d/src/api/`

##### D-1: `mod.rs` — `current_branch()` (raw git2)

- **File:** `g5d/src/api/mod.rs` lines 96–103
- **Function:** `GitOps::current_branch()`
- **What it does:** `git2::Repository::open(workspace_path)` →
  `git2_repo.head()` → `head.shorthand()` — raw `git2` because
  "`GitRepo` doesn't expose `current_branch`" (per inline comment).
- **Should use:** Add `current_branch()` to `GitRepo`, then
  `ops::current_branch()`.
- **Risk:** **LOW** — read-only; comment explicitly documents why
  the bypass exists.

##### D-2: `mod.rs` — `dirty_files()` (raw git2)

- **File:** `g5d/src/api/mod.rs` lines 136–155
- **Function:** `GitOps::dirty_files()`
- **What it does:** `git2::Repository::open(workspace_path)` →
  `git2_repo.statuses(…)` — raw `git2` for working tree status.
- **Should use:** Add `dirty_files()` to `GitRepo`, then `ops::status()`.
- **Risk:** **LOW** — read-only; duplicates status logic in
  `cli/commit.rs` and `cli/revert.rs`.

#### 9.5.7 Maintenance Layer — `g5-api/src/maintenance/`

##### M-1: `repo_fetcher.rs` — periodic fetch

- **File:** `g5-api/src/maintenance/repo_fetcher.rs` lines 299–300
- **Function:** `fetch_all_repos()`
- **What it does:** `GitRepo::open(&path)` → `git_repo.fetch("origin", &auth_clone)`
  inside `spawn_blocking`.
- **Should use:** `ops::fetch()`
- **Risk:** **LOW** — proper auth is passed; inside `spawn_blocking`
  for thread safety. Acceptable but should migrate for consistency.

##### M-2: `repo_fetcher.rs` — workspace sync fetch

- **File:** `g5-api/src/maintenance/repo_fetcher.rs` lines 578–579
- **Function:** `sync_workspace()`
- **What it does:** Same pattern as M-1.
- **Should use:** `ops::fetch()`
- **Risk:** **LOW** — same as M-1.

##### M-3: `repo_fetcher.rs` — `resolve_remote_head()` (read-only)

- **File:** `g5-api/src/maintenance/repo_fetcher.rs` lines 829–831
- **Function:** `resolve_remote_head()`
- **What it does:** `GitRepo::open(path)` →
  `repo.resolve_ref_flexible("origin", branch)` — read-only ref
  resolution.
- **Should use:** `ops::resolve_head()`
- **Risk:** **LOW** — read-only.

#### 9.5.8 Summary: Risk Heat Map

| Risk | Count | Callsites |
|---|---|---|
| **HIGH** | 10 | C-3, C-4, C-21, W-6, W-8, CR-1, A-2, A-3, A-4, A-8 |
| **MEDIUM** | 18 | C-1, C-2, C-5, C-7, C-8, C-10, C-13, C-17, C-18, C-19, W-1, W-2, W-7, W-9, T-1, T-2, T-3, A-7 |
| **LOW** | 14 | C-6, C-9, C-11, C-12, C-14, C-15, C-16, C-20, A-1, A-5, A-6, A-9, T-4, T-5 |
| **ACCEPTABLE** | 10 | C-22, W-3, W-4, W-5, D-1, D-2, M-1, M-2, M-3, worktree.rs |
| **Total** | **52** | |

#### 9.5.9 Refactoring Priority — Measurable Milestones

Each milestone has explicit entry/exit criteria and verification tests.
A milestone is **done** when all its exit criteria pass in CI.

---

**Milestone R1 — ops:: Core Module + Critical API Paths**
*Blocks: worker separation, CRDT safety.*

| # | Callsite | Why first |
|---|---|---|
| 1 | CR-1 | CRDT materialisation bypass — **data-loss bug** (§9.1) |
| 2 | A-2 | `draft_service.rs` CAS commit — most critical API write path |
| 3 | A-3 | `task_service.rs` auto-merge — race with concurrent workers |
| 4 | A-4 | `workspace_service.rs` merge/rebase — race, history rewrite |
| 5 | A-8 | `job_service.rs` rollback — moves refs backwards unsafely |

Entry criteria: `g5-common/src/ops/` module skeleton merged (mod.rs,
commit.rs, merge.rs, clone.rs, branch.rs, draft.rs).

Exit criteria:
- [ ] `ops::draft_commit()`, `ops::merge()`, `ops::create_branch()`,
      `ops::resolve_head()`, `ops::diff_check()` all implemented and
      tested (`U-OPS-01` – `U-OPS-08`, `U-OPS-18` – `U-OPS-24`).
- [ ] `draft_service.rs` uses `ops::draft_commit()` — no raw `git2` in
      commit path. Tests: `I-API-01`, `I-API-02`.
- [ ] `task_service.rs` uses `ops::merge()` for auto-merge. Tests:
      `I-API-03`, `I-API-04`.
- [ ] `workspace_service.rs` uses `ops::merge()`. Tests: `I-API-06`.
- [ ] `job_service.rs` rollback uses `ops::merge()`. Tests: `I-API-05`.
- [ ] CRDT inbound materialisation uses `write_file_atomic()`. Tests:
      `U-CRDT-01` – `U-CRDT-03`.
- [ ] All 10 HIGH-risk callsites from the API/CRDT layer (A-2, A-3,
      A-4, A-8, CR-1) migrated. Remaining HIGH: C-3, C-4, C-21, W-6, W-8.

E2E verification: `E2E-09` (full job lifecycle through ops::) passes
with the API-side callsites migrated (CLI/worker stubs acceptable).

---

**Milestone R2 — Worker Lifecycle Migration**
*Blocks: worker NFS removal, HTTPS clone.*

| # | Callsite | Why second |
|---|---|---|
| 6 | W-6 | Worker mirror+merge — concurrent merge parents |
| 7 | W-8 | `phase_clone_patch` — detached HEAD risk, 70-line inline |
| 8 | W-1, W-2 | Worker branch setup (CoW + reuse paths) |
| 9 | W-7, W-9 | Worker checkpoint/collect |
| 10 | T-1, T-2, T-3 | Agent tools (commit, push, collect) |

Entry criteria: Milestone R1 complete. `ops::setup_task_branch()`,
`ops::checkpoint()`, `ops::push()` implemented.

Exit criteria:
- [ ] All worker callsites (W-1 – W-9) use `ops::*`. Tests: `U-WRK-01`
      – `U-WRK-09`.
- [ ] All agent tool callsites (T-1 – T-5) use `ops::*`. Tests:
      `U-WRK-07` – `U-WRK-09`.
- [ ] `phase_clone_patch()` reduced from ~70 lines to single
      `ops::setup_task_branch()` call.
- [ ] No raw `GitRepo` method chains in `g5d/src/worker/`.

E2E verification: `E2E-01` (worker clones via HTTPS, not NFS) +
`E2E-09` (full lifecycle) both pass end-to-end.

---

**Milestone R3 — CLI Unification**
*Blocks: URL-based clone, per-instance credentials.*

| # | Callsite | Why third |
|---|---|---|
| 11 | C-2 | `commit_subdir()` — used by 5 subcommands, single leverage point |
| 12 | C-3, C-4 | `push.rs` — no auth on push |
| 13 | C-21 | `revert.rs` — destructive reset, no CRDT guard |
| 14 | C-7, C-10, C-13 | Clone operations — no auth, no COW |
| 15 | C-17, C-18, C-19 | Refresh fetch/clone — no auth |
| 16 | C-8 | `add.rs` checkout — bypasses `GitRepo` |
| 17 | A-7 | Job branch creation — orphan branch risk |

Entry criteria: Milestone R2 complete. `ops::clone()`, `ops::fetch()`,
`ops::reset_hard()` implemented.

Exit criteria:
- [ ] All CLI callsites (C-1 – C-22) use `ops::*` or are ACCEPTABLE.
      Tests: `U-CLI-01` – `U-CLI-10`.
- [ ] `commit_subdir()` deleted from `cli/mod.rs`. Test: `U-CLI-10`.
- [ ] All `push.rs` paths use `ops::push()` with `GitAuth`. Test:
      `U-CLI-03`, `U-CLI-04`.
- [ ] `revert.rs` checks CRDT dirty state before reset. Test:
      `U-CLI-08`.
- [ ] Job branch creation (`A-7`) wrapped in DB transaction with
      cleanup on failure. Test: `I-API-07`, `I-API-08`.

E2E verification: `E2E-09` with CLI-initiated operations (not just
worker-initiated).

---

**Milestone R4 — Cleanup + Consistency**
*Non-blocking; code quality and consistency.*

| # | Callsite | Why last |
|---|---|---|
| 18 | C-6, C-9, C-11, C-12, C-14, C-15, C-16, C-20 | LOW risk — meta bookkeeping |
| 19 | D-1, D-2 | Daemon API — add `current_branch()`, `dirty_files()` to `GitRepo` |
| 20 | M-1, M-2, M-3 | Maintenance — already uses `GitRepo`, migrate for consistency |
| 21 | W-3, W-4, W-5, C-22 | ACCEPTABLE — centralise for consistency |

Entry criteria: Milestone R3 complete.

Exit criteria:
- [ ] Zero raw `git2::Repository` calls outside `g5-common/src/git/`.
- [ ] `GitRepo` exposes `current_branch()` and `dirty_files()`.
- [ ] `grep -r 'git2::Repository::open\|git2::Repository::init\|git2::Repository::clone' g5d/ g5-api/`
      returns zero results.

E2E verification: Full CI green — all existing + new tests pass.

---

**Milestone tracking:**

| Milestone | Callsites | Tests (unit + integration) | E2E Gate |
|---|---|---|---|
| R1 — Core + API | 5 HIGH (CR-1, A-2, A-3, A-4, A-8) | U-OPS-*, U-CRDT-*, I-API-* | E2E-09 |
| R2 — Worker | 10 (W-*, T-*) | U-WRK-* | E2E-01, E2E-09 |
| R3 — CLI | 13 (C-*, A-7) | U-CLI-*, I-API-07/08 | E2E-09 (CLI) |
| R4 — Cleanup | 13 (LOW + ACCEPTABLE) | grep zero-result | Full CI |
| **Total** | **41 migrated** (10 ACCEPTABLE left as wrappers) | | |

#### 9.5.10 `ops::` Module — Required Operations

Based on the inventory above, the `ops::` module needs the following
operations:

| Operation | Callsites | Description |
|---|---|---|
| `ops::commit()` | C-1, C-2, C-6, C-9, C-11, C-12, C-14, C-15, C-20, W-7, W-9, T-2 | Stage all + commit with author |
| `ops::push()` | C-3, C-4, C-5, T-1, T-3 | Push refspec to remote with auth |
| `ops::clone()` | C-7, C-10, C-13, C-16, C-18 | Clone with auth + optional COW |
| `ops::fetch()` | C-17, C-19, T-4, T-5, M-1, M-2 | Fetch from remote with auth |
| `ops::merge()` | W-6, A-3, A-4, A-8 | Unified merge (ff → rebase fallback) — §2.10 |
| `ops::setup_task_branch()` | W-1, W-2, W-8 | Create/checkout task branch from base commit |
| `ops::checkout_branch()` | C-8 | Checkout existing/remote branch |
| `ops::reset_hard()` | C-21 | Destructive reset with CRDT guard |
| `ops::init()` | C-11, C-12, C-15 | Init bare/non-bare repo |
| `ops::draft_commit()` | A-2 | Lock → checkout → materialise → add → commit → unlock |
| `ops::diff_check()` | A-1 | Read-only diff/commit-count check |
| `ops::resolve_head()` | A-5, M-3 | Read-only ref resolution |
| `ops::open_read_only()` | A-6 | Read-only repo handle |
| `ops::create_branch()` | A-7 | Create branch at commit |
| `ops::status()` | D-2 | Working tree dirty-file check |
| `ops::current_branch()` | D-1 | Current HEAD branch name |
| `ops::configure_safe_directory()` | W-4 | Git safe.directory config |
| `ops::collect_commit_push()` | T-1 | Add + commit + push (agent collect) |
| `ops::checkpoint()` | W-7, W-9 | Add + commit + diff (worker checkpoint) |
| `ops::merge_workspace()` | A-3, A-4, A-8 | Pure git layer (g5-common): match repos by ID, run per-repo `ops::merge()`, return `Vec<MergeResult>`. No DB dependency — usable by workers, CLI, and API. The API wraps this in `service::merge_workspace_api()` which adds a DB transaction: creates the new workspace version, advances the workspace head, and rolls back on any single-repo conflict. |

### 9.6 OAuth Scope Extension for Git Serving

**Status: Scopes are solid for REST API, but incomplete for §14.**

The current 18-scope system ([g5-common/src/auth/scope.rs](generation/g5-common/src/auth/scope.rs))
covers all REST API operations via the `AuditedRouter`
compile-time enforcement pattern. However, §14 (Git HTTPS Serving
Protocol) introduces new endpoints that need dedicated scopes:

#### 9.6.1 Missing Scopes

| Proposed Scope | Protects | Used By |
|---|---|---|
| `git:read` | `GET /git/:repo_id/info/refs?service=git-upload-pack`, `POST /git/:repo_id/git-upload-pack` | Workers (clone/fetch), CLI users, CI bots |
| `git:write` | `GET /git/:repo_id/info/refs?service=git-receive-pack`, `POST /git/:repo_id/git-receive-pack` | Workers (push), CLI users |
| `workspaces:fork` | `POST /api/v1/workspaces/:id/fork` (if added) | CLI users creating forks |
| `workspaces:merge` | `POST /api/v1/workspaces/:id/merge` (if added) | CLI users merging forks |

#### 9.6.2 Current Gaps

- Worker session JWTs carry `Scope::Internal` which bypasses scope
  checks entirely. When git serving endpoints are added, they should
  **not** bypass scope checks — workers should carry `git:read` +
  `git:write` explicitly.
- Service tokens get synthetic claims with **all** scopes. When
  `git:read`/`git:write` are added, they'll automatically be granted
  to service tokens — acceptable for backward compatibility but should
  be tightened when API keys (§9.4) replace service tokens.
- The `AuditedRouter` pattern should be extended to the git serving
  routes to maintain compile-time scope enforcement.

---

## 10. Open Design Questions

### 10.1 Workspace Remote Protocol

**Status: Undefined.**

Today, the CLI talks to g5-api via ad-hoc HTTP calls scattered across
command handlers. There is no formal "workspace remote" protocol. Each
command constructs its own URLs:

```rust
// clone.rs
format!("{}/api/v1/workspaces/{}", api_url, workspace_id)
// push.rs
format!("{}/api/v1/workspaces/{}/lock", api_url, workspace_id)
// fork.rs
format!("{}/api/v1/workspaces/{}/fork", api_url, source_workspace_id)
```

**What's needed:** A defined protocol surface that any CLI (or third-party
tool) can implement against. This would be:

1. **Manifest endpoint** — `GET /api/v1/workspaces/:id` returns the
   workspace definition (repos, branches, docs, settings)
2. **Lock endpoint** — `PUT /api/v1/workspaces/:id/lock` updates the
   commit pin after push
3. **Fork endpoint** — `POST /api/v1/workspaces/:id/fork` creates a
   server-side fork
4. **Sync endpoint** — `POST /api/v1/workspaces/:id/sync` triggers
   upstream git fetch
5. **Draft endpoints** — CRUD for CRDT draft sessions
6. **CRDT WebSocket** — `wss://<api>/ws/crdt/<workspace-id>/<branch>`

These endpoints already exist but are not documented as a stable contract.
A formal "Workspace Remote Protocol" spec would allow third-party IDEs
and agents to integrate without depending on g5d internals.

### 10.2 Third-Party Agent Conflict Notification

#### 10.2.1 Why This Matters

When a user runs `g5 fork --connect` and then launches a third-party
agent (Cursor, aider, Windsurf, etc.) in the same working copy, two
independent writers operate on the same files:

1. **The CRDT materializer** — `g5 connect`'s background process writes
   inbound remote edits to disk.
2. **The third-party agent** — writes files directly (bypassing Yjs).

If a browser user edits `src/handler.rs` while the local agent is also
modifying it, the agent's changes are silently overwritten on the next
inbound delta. No warning, no backup, no notification. The agent
continues working against stale assumptions.

This is the single most dangerous data-loss scenario in the CLI workflow
and must be addressed before third-party agent use is considered safe.

#### 10.2.2 Current Behaviour (What We Are Changing)

The inbound materialization path lives in `CrdtPeer::connect()` in
`g5d/src/crdt/mod.rs` (lines 231–263). When the sync engine receives an
`InboundDelta` from the server, the background task:

```rust
// mod.rs — inbound materialization (current code, simplified)
while let Some(delta) = rx.recv().await {
    let doc_guard = doc.lock().await;
    for path in &delta.affected_paths {
        if let Some(content) = doc_guard.get_file_content(path) {
            let full_path = workspace_path.join(path);
            // ⚠ NO CHECK — unconditionally overwrites the file
            tokio::fs::write(&full_path, &content).await?;
        }
    }
}
```

Problems with this code path:

| Issue | Detail |
|-------|--------|
| **No local-modification check** | Does not compare the on-disk file against `known_states` before writing. If the agent modified the file since the last materialization, the agent's changes are silently lost. |
| **Bypasses the Materializer** | Uses raw `tokio::fs::write()` instead of `Materializer::write_file_atomic()`. No temp-file + fsync + rename atomicity, no `known_states` update. |
| **No conflict record** | No log entry, no sidecar file, no event. The overwrite is invisible. |
| **Race window** | `detect_fs_changes()` (which scans for out-of-band edits) runs *after* tool calls complete. If the inbound delta arrives between the agent's write and the next `detect_fs_changes()` call, the agent's change is never captured in Yjs and is permanently lost. |

By contrast, the **cold-start** path (`batch_materialize()` in
`materialize.rs`) *does* check content hashes before writing and skips
unchanged files — but the incremental path has no such guard.

#### 10.2.3 Conflict Scenarios

| # | Scenario | Current Behaviour | Desired Behaviour |
|---|----------|-------------------|-------------------|
| 1 | Agent writes file, then CRDT inbound delta arrives for same file | Inbound write silently overwrites agent's version | Agent notified; agent's version preserved as conflict sidecar |
| 2 | Agent commits, push fails because HEAD advanced | `g5 push` rebases and retries (up to 3×) | OK — already handled |
| 3 | Agent commits, rebase produces merge conflicts | Retry loop exhausts; task fails | Agent notified with list of conflicted files |
| 4 | Browser user and agent edit same file concurrently | Last-write-wins in Yjs (fine for CRDT-aware peers) | Acceptable for Yjs-integrated edits; dangerous for out-of-band agents (scenario 1) |

Scenario 1 is the critical gap. Scenarios 2–3 are handled by the existing
rebase retry mechanism. Scenario 4 is a non-issue for CRDT-aware peers
but collapses into scenario 1 for third-party agents that write directly
to the filesystem.

#### 10.2.4 Proposed Design

We propose a **two-layer notification system**: WebSocket as the primary
channel (machine-parseable, real-time), with `.g5/conflicts/` filesystem
sidecars as a universal backup (works with any agent that watches files).

**Primary: Local WebSocket notification**

`g5 connect` already runs as a background daemon. We add a local Unix
domain socket at `.g5/crdt.sock` (workspace-relative) that emits
structured JSON events:

```jsonc
// Event emitted when inbound delta would overwrite a locally-modified file
{
  "type": "conflict",
  "path": "src/handler.rs",
  "timestamp": "2025-01-15T10:30:00Z",
  "local_sha256": "abc123...",    // agent's version (on disk before overwrite)
  "remote_sha256": "def456...",   // Yjs version about to be written
  "remote_author": "user@example.com",
  "action": "overwritten",        // or "deferred" if locking is active
  "sidecar": ".g5/conflicts/src/handler.rs"
}
```

Agents that support MCP or custom integrations can connect to this socket
and react programmatically (e.g., re-read the file from disk, merge,
abort their current operation).

**Backup: Filesystem sidecars**

Before overwriting a locally-modified file, the materializer copies the
agent's version to `.g5/conflicts/<relative-path>`:

```
.g5/conflicts/
  src/
    handler.rs          # agent's version, preserved
    handler.rs.meta     # JSON: timestamp, local_sha256, remote_sha256
```

Any agent that uses `inotify`/`fswatch` on the workspace directory (most
IDE-based agents do) will see the new file appear in `.g5/conflicts/` and
can react. The `.meta` file provides enough context to perform a 3-way
merge or alert the user.

Sidecars are cleaned up by `g5 connect` on the next successful
`detect_fs_changes()` cycle where no conflicts are detected, or manually
via `g5 conflicts --clear`.

**Discussed and rejected: Workspace locking**

We considered temporarily locking the workspace (blocking inbound
materialization) while a third-party agent is executing commands. This
would prevent conflicts entirely by pausing remote writes.

| Pro | Con |
|-----|-----|
| Zero conflicts — no agent changes can be overwritten | Blocks all real-time collaboration during agent execution |
| Simple to implement (`set_safe_to_materialize(false)`) | If the agent hangs or takes a long time, remote users see a frozen workspace |
| Already partially exists — `safe_to_materialize` flag in SyncEngine | Doesn't compose well — multiple agents can't lock simultaneously |
| | Violates the expectation that browser edits are always live |

The `safe_to_materialize` flag already gates inbound materialization in
the SyncEngine: when false, inbound deltas are queued but not written to
disk. However, this flag is designed for short pauses between tool calls
(sub-second), not for multi-minute agent sessions. Repurposing it as a
workspace lock would degrade the collaborative editing experience.

**Verdict:** Locking is too heavy-handed for the general case. We use
detect-and-notify instead. However, `g5 connect --exclusive` could opt
into locking mode for agents that explicitly prefer isolation over
real-time collaboration.

#### 10.2.5 Implementation Plan

**Step 1: Add conflict detection to `materialize_files()`**

Modify the inbound materialization task in `CrdtPeer::connect()` to route
through the Materializer instead of raw `tokio::fs::write()`:

```rust
// Proposed change in mod.rs — inbound materialization
while let Some(delta) = rx.recv().await {
    let doc_guard = doc.lock().await;
    for path in &delta.affected_paths {
        if let Some(content) = doc_guard.get_file_content(path) {
            let full_path = workspace_path.join(path);
            let content_bytes = content.as_bytes();
            let remote_hash = compute_sha256(content_bytes);

            // Check if the file was locally modified since last known state
            let local_conflict = materializer
                .detect_local_modification(path, &full_path)
                .await;

            if let Some(local_version) = local_conflict {
                // Preserve agent's version as sidecar
                materializer
                    .write_conflict_sidecar(path, &local_version, remote_hash)
                    .await;
                // Emit conflict event on local socket
                conflict_tx.send(ConflictEvent {
                    path: path.clone(),
                    local_sha256: local_version.content_hash,
                    remote_sha256: remote_hash,
                    remote_author: delta.author.clone(),
                }).await;
            }

            // Write using atomic materializer (not raw fs::write)
            materializer.write_file_atomic(&full_path, content_bytes).await?;
            materializer.update_known_state(path, &full_path).await;
        }
    }
}
```

The key new method is `detect_local_modification()`:

```rust
// materialize.rs — new method
impl Materializer {
    /// Compare current on-disk file against last known state.
    /// Returns Some(FileState) if the file was modified externally
    /// (i.e., by an agent that bypassed the CRDT layer).
    async fn detect_local_modification(
        &self,
        rel_path: &str,
        abs_path: &Path,
    ) -> Option<FileState> {
        let known = self.known_states.lock().ok()?.get(rel_path)?.clone();
        let metadata = tokio::fs::metadata(abs_path).await.ok()?;
        let mtime = metadata.modified().ok()?;

        // Fast path: mtime unchanged → no local modification
        if mtime == known.mtime {
            return None;
        }

        // Slow path: mtime changed → compute hash to confirm
        let content = tokio::fs::read(abs_path).await.ok()?;
        let current_hash = compute_sha256(&content);
        if current_hash != known.content_hash {
            Some(FileState {
                mtime,
                content_hash: current_hash,
            })
        } else {
            None // mtime changed but content is the same (e.g., touch)
        }
    }
}
```

**Step 2: Local Unix socket in `g5 connect`**

Add a `ConflictNotifier` that binds `.g5/crdt.sock` on startup and
accepts connections. Each connected client receives newline-delimited JSON
events. This is intentionally simple — no framing protocol, no
request/response. Clients connect, read lines, react.

**Step 3: `.g5/conflicts/` sidecar writer**

Add `write_conflict_sidecar()` to the Materializer. On conflict, it:
1. Creates `.g5/conflicts/<dir>/` if needed
2. Copies the agent's file version (from disk, before overwrite)
3. Writes a `.meta` JSON file with hashes and timestamp
4. Logs a WARN-level message: `"Conflict: {path} overwritten by remote
   edit from {author}; agent version saved to .g5/conflicts/{path}"`

**Step 4: `g5 conflicts` CLI command**

- `g5 conflicts` — list current conflict sidecars
- `g5 conflicts --clear` — remove all sidecars
- `g5 conflicts --accept-local <path>` — re-read the sidecar, enqueue
  it via `enqueue_file_write()` (agent's version wins)
- `g5 conflicts --accept-remote <path>` — delete sidecar (remote wins,
  which is already on disk)

#### 10.2.6 Dogfooding

G5's own worker agent goes through the same `g5 connect` + fork flow
that third-party agents will use. This means the conflict detection and
notification mechanism will be exercised by every task execution:

- Worker connects to workspace via CRDT
- Worker's `enqueue_file_write()` goes through Yjs (no conflict with the
  materializer since the write and the Yjs update are atomic)
- But if a browser user edits the same file concurrently, the worker's
  `detect_fs_changes()` captures the agent's out-of-band write → Yjs →
  remote user's browser. The remote user's subsequent edit → Yjs →
  materializer → conflict detection fires if the worker also modified
  the file in between.

This gives us production coverage of the conflict path on every
multi-agent or human+agent collaboration session.

### 10.3 UI Commit vs CLI Commit — Path Consolidation

#### 10.3.1 Why This Matters

Today there are **three** distinct commit code paths, each with different
CAS semantics, different git operation locations (server-side vs
client-side), and different notification mechanisms. This divergence means:

- Bug fixes to one path don't propagate to the others.
- Client-side commits (CLI, workers) can advance a branch without
  notifying browser draft sessions, causing silent CAS failures.
- Testing surface area triples: every commit-related feature must be
  verified across all three paths.

The goal is **one code path, everywhere** — all commits flow through the
same CAS-guarded mechanism with the same notification guarantees.

#### 10.3.2 Current Behaviour — Three Commit Paths

**Path A: Browser UI → Draft Service (server-side)**

Source: `g5-api/src/services/draft_service.rs`, `commit_draft()` at
line 658.

```
Browser user clicks "Commit"
  → POST /api/v1/drafts/:id/commit { message, expectedHeadCommit }
  → Server: SELECT FOR UPDATE on workspace_branches
  → CAS check: expectedHeadCommit == current head_commit?
     • Match → continue
     • Mismatch → 409 DraftConflict (draft preserved, no edits lost)
  → Transition draft status to 'committing'
  → Extract dirty files from Yjs document (draft_updates table)
  → git add + commit + push (server-side, on NFS bare repo)
  → UPDATE workspace_branches SET head_commit = new_sha
  → DELETE draft_updates + draft_sessions rows
  → COMMIT transaction
  → Emit WebSocket event: draft.commit { workspace_id, branch, sha }
```

Key properties:
- All git operations happen on the g5-api server against the NFS-mounted
  bare repository.
- The browser never touches git — Yjs deltas are materialized to files
  on the server, then committed.
- CAS guard is explicit: the client sends `expectedHeadCommit`, and the
  server rejects if HEAD moved.
- On 409, the draft session is preserved. The user's edits are not lost.
  The browser must re-fetch HEAD and retry (or rebase the draft).

**Path B: Worker → Task Transition (server-side CAS, client-side git)**

Source: `g5-api/src/services/task_service.rs`, `transition_task()` at
line 1070 (the `"completed"` branch starting at line 1150).

```
Worker agent completes task
  → git add + commit + push (worker-side, in Kata VM workspace)
  → POST /api/internal/tasks/:id/status
      { status: "completed", commit_sha, base_commit }
  → Server: SELECT tasks FOR UPDATE
  → Server: SELECT jobs FOR UPDATE (lock job row)
  → CAS check: task.base_commit == job.head_commit?
     • Match → advance job HEAD
     • Mismatch → log warning, advance anyway (advisory CAS)
  → UPDATE jobs SET head_commit = commit_sha
  → UPDATE tasks SET status = 'completed'
  → Unblock dependent tasks
  → COMMIT transaction
```

Key properties:
- Git operations happen on the **worker** (inside the Kata VM), not on
  the server. The worker pushes to the server's bare repo over the
  network (currently NFS, future: git-over-HTTPS or GCS).
- CAS is **advisory** — the server logs a warning on mismatch but
  advances the job HEAD anyway. Correctness relies on the worker merging
  its task branch into the job branch locally before pushing.
- No WebSocket notification is emitted when the job HEAD advances, so
  browser draft sessions are not notified.

**Path C: CLI → Local Git + Push (client-side, no CAS)**

Source: proposed in this design document (§2, `g5 commit` + `g5 push`).
Does not exist in the codebase today.

```
User runs `g5 commit -m "message"` in their local working copy
  → git add -A && git commit -m "message" (local)
User runs `g5 push`
  → git push origin <branch>
     • Success → branch HEAD advances on the bare repo
     • Reject (non-fast-forward) → rebase and retry (up to 3×)
  → POST /api/v1/workspaces/:id/lock { commit_sha }  (proposed)
     → Server updates workspace_branches.head_commit
     → Emits branch.advanced event
```

Key properties:
- No CAS guard beyond git's native push rejection.
- No draft interaction — if a browser user has an active draft session
  on the same branch, they won't know the CLI user advanced HEAD until
  their next commit attempt fails with 409.
- The push lock endpoint does not exist yet; it is proposed in this
  document.

#### 10.3.3 The Divergence Problem

| Property | Path A (UI Draft) | Path B (Worker) | Path C (CLI) |
|----------|-------------------|-----------------|--------------|
| Git ops location | Server | Worker (Kata VM) | Local machine |
| CAS mechanism | Explicit (`expectedHeadCommit`) | Advisory (`base_commit` vs job HEAD) | None (git push rejection) |
| Notification | `draft.commit` WS event | None | None (proposed: `branch.advanced`) |
| Draft awareness | Full (draft is the source of truth) | None (worker doesn't know about drafts) | None |
| On conflict | 409 → draft preserved | Warning → advance anyway | Push rejected → rebase retry |
| Commit storage | Yjs → files → git (server-side) | Files → git (worker-side push) | Files → git (local push) |

The core inconsistency: **Path A notifies. Paths B and C do not.**

When a worker or CLI user advances HEAD, any browser draft session on
that branch becomes stale. The draft's `expectedHeadCommit` no longer
matches the current `head_commit`, so the next browser commit will fail
with 409 DraftConflict. The user sees a confusing error with no
explanation of *why* HEAD moved.

#### 10.3.4 Planned Consolidation

Two design principles govern the consolidation:

1. **CAS belongs at the push boundary.** Git's own ref locking is the
   CAS for CLI/Worker pushes. The draft service's DB-level CAS is the
   equivalent for server-side commits (the API *is* the remote). There
   is no second, redundant CAS at the DB layer for client-side pushes.

2. **The push to the remote triggers server-side notification.** The
   server detects the ref advance and fires the DB update + WS event
   internally. CLI/Worker callers do NOT need to call an explicit
   `notify-push` endpoint — the server owns the bare repo and can
   react to ref changes directly.

**Why server-side detection (not client-side notification):**

If we required the client to call `POST /notify-push` after pushing,
we'd have a reliability gap: if the client crashes, loses connectivity,
or simply forgets to call the endpoint, the DB `head_commit` diverges
from the git ref. The existing `repo_fetcher` maintenance loop
eventually reconciles this, but with a delay (minutes).

Instead, the server detects the ref advance at push time — either via:

- **`receive_pack` handler** (the API's git-over-HTTPS handler performs
  validation before and side-effects after the git subprocess — §14.4), or
- **`repo_fetcher` poll** (existing — catches any missed notifications
  as a fallback)

The `receive_pack` handler is the primary implementation. It runs
validation inline before spawning `git receive-pack`, and fires
side-effects (DB update, WS event, reindex) inline after the subprocess
completes. No git hooks or shell scripts are involved.

**Consolidated Flow:**

```
  ┌─────────────────────────────────────────────────────────────┐
  │                     Commit Source                           │
  │  Browser UI  │  Worker Agent  │  CLI (g5 push)            │
  └──────┬───────┴───────┬────────┴──────────┬─────────────────┘
         │               │                   │
         ▼               ▼                   ▼
  ┌──────────────┐ ┌───────────────────────────────────────────┐
  │ Draft Service│ │ git push origin <branch>                  │
  │ (API is the  │ │ (CAS = git's native fast-forward check)  │
  │  remote)     │ │                                           │
  │              │ │ On reject: rebase + retry (up to 3×)      │
  │ CAS: in DB   │ │ On success: receive_pack handler runs     │
  │ Push: to     │ │   advance_head() inline (§14.4)            │
  │ bare repo    │ └───────────────┬───────────────────────────┘
  └──────┬───────┘                 │
         │                         │
         ▼                         ▼
  ┌─────────────────────────────────────────────────────────────┐
  │  Server-side (triggered by push or draft commit):           │
  │  1. Read current head_commit from workspace_branches        │
  │  2. UPDATE workspace_branches SET head_commit = new_sha     │
  │  3. Emit WS: branch.advanced { workspace, branch, sha,     │
  │              old_sha, source, author }                      │
  │  4. Enqueue reindex if needed                               │
  └─────────────────────────────────────────────────────────────┘
```

**What changes per path:**

**Path A (Browser UI / Draft Service):**
- `commit_draft()` keeps its CAS check (`expectedHeadCommit`) and
  inline git push — the API is the remote, so this is correct.
- After the git push step, `commit_draft()` calls the shared
  `advance_head()` internal function (update DB, emit
  `branch.advanced` with `source: "draft"`).
- The `draft.commit` WS event is replaced by `branch.advanced`.
  Downstream subscribers get richer context (source, old SHA, new SHA).
- Draft cleanup (session deletion, draft_updates purge) remains
  draft-service-specific.

**Path B (Worker / Task Transition):**
- The worker pushes repo-level commits to the bare repo. The
  `receive_pack` handler runs `advance_head()` inline after the
  subprocess completes (§14.4), which updates the repo-level
  `head_commit` in `workspace_branches`. This is a **repo-level**
  bookkeeping update, not a workspace version advance.
- When the worker calls `transition_task(completed)`, the API runs
  `ops::merge_workspace()` to merge the task workspace into the job
  workspace. This is the **workspace-version-advancing event** — it
  creates a new job workspace version with the updated repo SHAs.
- There is no separate advisory CAS check. The merge itself is the
  guard: if the job workspace has advanced since the task's base
  version (e.g., another task was promoted), `ops::merge_workspace()`
  performs a per-repo rebase. If the rebase produces conflicts, the
  merge fails and the error is returned to the agent.
- **On merge conflict:** `transition_task()` returns a structured error
  containing the updated base workspace version SHA and the list of
  conflicting files. The agent is expected to rebase its changes onto
  the new base using workspace tools (`ops::fetch`, `ops::merge`,
  conflict resolution tools) and retry the push. The agent must have
  access to these workspace tools during task execution to handle
  rebase/conflict resolution autonomously.
- `branch.advanced` with `source: "worker"` notifies browser sessions
  that were previously blind to worker commits.

**Path C (CLI / `g5 push`):**
- `g5 push` runs `git push origin <branch>`. The `receive_pack`
  handler runs `advance_head()` inline after the push completes
  (§14.4), emitting `branch.advanced` with `source: "cli"`.
- No client-side notification call needed — the server handles it.
- Browser sessions receive `branch.advanced` with `source: "cli"`.

#### 10.3.4a Reject Push When Active Draft Exists

**The problem:** If a CLI user or worker pushes a commit that was
produced outside the CRDT layer (raw `git add && git commit`, not
through Yjs), the resulting git HEAD diverges from the Yjs document
state. The draft's Yjs doc still contains the old file contents (or
in-progress edits), but the git branch has moved to a commit that
doesn't reflect those contents.

This creates an irreconcilable split: the draft's
`expectedHeadCommit` no longer matches HEAD, so the next browser commit
fails with 409. Worse, if the browser user re-fetches HEAD and retries,
the draft's Yjs deltas are applied on top of a commit that already
modified the same files — producing a nonsensical merge.

**The rule:** If a branch has an active draft session (status =
`'active'` in `draft_sessions`), reject pushes from non-CRDT sources
(CLI and Worker) with a clear error.

**Implementation via `receive_pack` handler:**

The `receive_pack` handler (§14.4) performs all three pre-push checks
inline before spawning the `git receive-pack` subprocess:
1. **Draft guard** — reject push if dirty drafts exist (the check below)
2. **Stale-push rejection** — reject non-descendant pushes
3. **Authorisation re-check** — verify the pusher still has an active task

The draft-guard check uses the following query internally:

```sql
SELECT EXISTS(
  SELECT 1 FROM draft_sessions ds
  JOIN workspace_branches wb ON wb.id = ds.workspace_branch_id
  WHERE wb.repo_id = $1
    AND wb.branch_name = $2
    AND ds.status = 'active'
    AND ds.dirty_count > 0
) AS "locked!"
```

The `dirty_count > 0` check avoids false rejections: a clean draft
session (no pending edits) is harmless — the browser can trivially
fast-forward. Only dirty drafts (with uncommitted Yjs deltas) create
the dangerous divergence.

**Why reject, not merge:**

| Option | Pros | Cons |
|--------|------|------|
| **Reject push** | Simple, safe, deterministic. Protects CRDT state. | CLI user must wait for draft to commit/discard. |
| Merge push into Yjs | No user coordination needed. | Complex: requires 3-way merge of Yjs deltas against git diff. The Yjs doc format doesn't support git-style conflict markers. |
| Accept push, invalidate draft | Non-blocking for CLI user. | Browser user loses in-progress work (or must manually reconcile). |

Rejection is the simplest and safest choice. The error message tells
the CLI user exactly what to do: commit or discard the draft first.

**Escape hatches (separate flags):**

- `g5 push --force` — skip rebase on non-fast-forward push. This
  overwrites the remote ref. Does NOT bypass the draft guard.
  Requires the `push:force` permission on the target workspace.
- `g5 push --override-draft` — bypass the draft guard when an active
  dirty draft exists on the target branch. The draft session is
  invalidated and the browser user is notified via `branch.advanced`.
  Requires the `draft:override` permission. For recovery only — e.g.,
  a stuck draft that can't be committed.
- Both flags can be combined: `g5 push --force --override-draft`.

**Permission control:** both `--force` and `--override-draft` require
explicit RBAC permissions (`push:force` and `draft:override`
respectively). These permissions are not granted by default — org
admins must explicitly grant them to roles that need recovery
capability. Workers never use `--force` or `--override-draft` in
normal operation; these are human-initiated recovery tools.

Both flags still trigger `branch.advanced`, so browser sessions are
always notified.

#### 10.3.5 The `branch.advanced` WebSocket Event

```jsonc
{
  "type": "branch.advanced",
  "workspace_id": "ws-123",
  "branch": "main",
  "old_head_commit": "abc123",
  "new_head_commit": "def456",
  "source": "cli",           // "draft" | "worker" | "cli"
  "author": "user@example.com",
  "message": "Fix handler timeout",
  "timestamp": "2025-01-15T10:30:00Z"
}
```

**Browser handling on receipt:**
1. If the user has an active draft session on this branch:
   - If `old_head_commit` matches the draft's `expectedHeadCommit`:
     the draft is still valid. Update `expectedHeadCommit` to
     `new_head_commit` (fast-forward).
   - If `old_head_commit` does NOT match: the draft is stale. Show a
     warning banner: "This branch was updated by {source}. Your draft
     may need to be rebased."
2. If the user is viewing the branch (read-only): refresh the file tree
   and diff view.
3. If the user is on a different branch: no action.

**Draft fast-forward** is the key UX improvement. Today, a CLI push
silently invalidates the draft, and the user only discovers this when
they try to commit (409). With `branch.advanced`, the browser can
automatically rebase the draft in simple cases (no overlapping edits) or
warn the user immediately.

#### 10.3.6 Implementation Plan

**Phase 1: Internal functions and endpoint**

Extract `advance_head()` as a shared internal function:

```rust
/// Advance a branch's head commit in the DB and emit side-effects.
///
/// Idempotent: if `head_commit` already equals `new_head`, returns
/// `Ok(AdvanceResult::NoOp)` and suppresses the WS event.
///
/// Called by: `receive_pack` handler (inline post-push), `commit_draft()`,
/// and the `ref-updated` fallback endpoint.
async fn advance_head(
    db: &PgPool,
    ws_broadcaster: &WsBroadcaster,
    repo_id: &Uuid,
    branch_name: &str,
    old_head: &str,
    new_head: &str,
    source: AdvanceSource,    // Draft | Worker | Cli
    author: &str,
) -> Result<AdvanceResult, G5Error> {
    // 1. UPDATE workspace_branches SET head_commit = $new_head
    //    WHERE repo_id = $repo_id AND branch_name = $branch_name
    //    AND head_commit = $old_head
    //    RETURNING id
    // 2. If no row updated (old_head mismatch or already equals new_head):
    //    return Ok(AdvanceResult::NoOp)
    // 3. Emit WS event: branch.advanced { workspace_id, branch, old_head,
    //    new_head, source, author, timestamp }
    // 4. Enqueue reindex job for (repo_id, branch_name)
    // 5. Return Ok(AdvanceResult::Advanced { workspace_id })
}

enum AdvanceResult {
    Advanced { workspace_id: Uuid },
    NoOp,
}

enum AdvanceSource {
    Draft,
    Worker,
    Cli,
}
```

This function looks up
`workspace_branches` by `repo_id + branch_name`, updates `head_commit`,
emits `branch.advanced` WS event, and enqueues a reindex entry. This is
the same logic currently inlined in `repo_fetcher::fetch_all_repos()`
(steps 4–6).

Add the internal endpoint `POST /api/internal/repos/:id/ref-updated`
that calls `advance_head()` — used by `repo_fetcher` poll as a fallback
for any pushes not served through the API's git transport layer.

`GET /api/internal/repos/:id/branch-locked?branch=<name>` — returns
`"true"` / `"false"` based on whether an active dirty draft exists.
Used by the `receive_pack` handler's inline draft guard check.

**Phase 2: Wire `receive_pack` handler**

The `receive_pack` Axum handler (§14.2) performs all validation inline
before spawning the git subprocess, and calls `advance_head()` inline
after the subprocess completes. No git hooks are installed. The handler
parses pkt-line ref updates from the request body to know which refs
are being pushed. See §14.4 for the full inline validation design.

For `g5 push --force`, the CLI sets the `X-G5-Force-Push: true` HTTP
header. The handler checks this header and verifies the `push:force`
permission before skipping the ancestry check.

**Phase 3: Wire draft service**

Extract the HEAD advance + WS event emission from `commit_draft()` into
the shared `advance_head()` function. `commit_draft()` calls it after
its server-side git push. The draft service retains its CAS check
(`expectedHeadCommit`) and session cleanup — only the notification is
shared.

Note: for draft commits, the `receive_pack` handler also calls
`advance_head()` (the API pushes to the bare repo), but
`advance_head()` is idempotent — if `head_commit` already equals the
new SHA, it's a no-op. The inline call and the draft service's call
race, but both produce the same result. `advance_head()` suppresses
the `branch.advanced` WebSocket event when it detects a no-op
(SHA unchanged), so browser clients receive exactly one event per
actual HEAD advance, never duplicates.

**Phase 4: Remove inline HEAD advances**

Remove the inline `UPDATE jobs SET head_commit` from
`transition_task()` in task_service.rs. Repo-level HEAD tracking is
now handled by the `receive_pack` handler's inline `advance_head()`
call (§14.4). Workspace version advancement is handled by
`ops::merge_workspace()` inside `transition_task()` — the merge-back
is the guard, not a separate CAS check (see Path B above).

**Phase 5: Draft auto-rebase (stretch)**

On the browser client, handle `branch.advanced` events by checking
whether the user's draft can be fast-forwarded. If the Yjs delta set
does not overlap with the files changed in the new commits, automatically
update `expectedHeadCommit`. If there is overlap, show a conflict
resolution UI.

The MVP is simply showing a notification banner when the branch moves
underneath the draft.

### 10.4 Local Uplift / Downlift

**Status: Not supported today — future work.**

All uplift and downlift jobs are created and scheduled server-side:

- **Uplift:** `POST /workspaces/:ws/repos` → `create_job("uplift")` →
  bootstrap task graph decomposition → workers poll and execute
- **Downlift:** created via the job service when spec changes are ready
  for code generation → same task graph → workers execute

Both require g5-api for: (a) job creation, (b) task decomposition via
ontology, (c) task scheduling via `poll_for_task`, (d) commit reporting.
Each such job owns a child job workspace created from a parent workspace
version; local mode would need a way to define and advance that workspace
lineage without the API.

**To support local uplift/downlift**, we'd need:

1. **Local task definition** — a YAML/JSON file describing the uplift or
   downlift scope (which components, which specs)
2. **Local scheduler** — feed tasks directly to `AgentLoop::run()` without
   the poll mechanism
3. **Local ontology** — a lightweight file-based ontology (or pull the
   ontology from g5-api and cache it)
4. **Skip server callbacks** — commit locally instead of calling
   `report_completion`

The `g5d worker --task <id>` direct-task mode is the closest existing
mechanism, but it still fetches the task definition from g5-api. In the
near-term local-dev model, users should launch a background worker with
`g5 worker start --api-url ... --api-key ...`, which wraps `g5d worker`
for a local machine or self-hosted server. A future `g5 run --local
--prompt "..."` command would go further and skip the server entirely
for simple tasks, using the local workspace as context.

---

### 10.5 Job Workspace → Task Workspace → Parent Merge Cycle

This section documents the complete end-to-end lifecycle of jobs using the
workspace-centric model introduced at the top of this document. Internal
per-repo refs may still exist, but they are implementation details. The
user-facing units are workspaces and workspace versions.

#### 10.5.1 Core Invariants

- A **parent workspace** is the workspace the user starts from.
- A **job workspace** is a child workspace created from a specific parent
  workspace version when a job starts.
- A **task workspace** is an isolated derivative of the current job
  workspace version for one task attempt.
- A **workspace version** is immutable and identified by a SHA. Every
  successful promotion creates a new workspace version; no existing
  version is mutated in place.
- Batch jobs such as uplift and downlift auto-merge their final job
  workspace back into the parent workspace. Chat workspaces remain
  interactive until the user explicitly applies or merges them.

#### 10.5.2 Uplift Workflow

1. The user triggers uplift from the UI or API.
2. g5-api creates a `jobs` row and a child **uplift job workspace** from
   the current parent workspace version.
3. Bootstrap analysis decomposes the job into tasks. Each task records:
   - `base_workspace_version_sha`
   - `depends_on`
   - optional repo subset
4. A worker polls for a task assignment and receives:
   - job workspace ID
   - base workspace version SHA
   - task metadata
5. The worker materialises an isolated **task workspace** from that job
   workspace version, executes the task, and commits repo-local changes.
6. The worker or API promotes the task result into the job workspace:
   - clean rebase/replay → create a new job workspace version and advance
     the job workspace head
   - conflict → reject the task result and retry from the latest job
     workspace version
7. When all uplift tasks complete, the API merges the uplift job workspace
   back into the parent workspace, creating a new parent workspace version.

The UI may open the uplift job workspace at any time for inspection,
review, or debugging. The parent workspace remains unchanged until the
final merge-back.

#### 10.5.3 Downlift Workflow

Downlift uses the same workspace lifecycle as uplift. The main difference
is how the task graph is produced.

1. A spec change or review decision creates a downlift job.
2. g5-api creates a child **downlift job workspace** from the current
   parent workspace version.
3. The task graph is derived from the affected spec/code generation scope.
4. Workers execute tasks in isolated task workspaces derived from the
   latest downlift job workspace version.
5. Each successful task promotion creates a new downlift job workspace
   version.
6. Review tasks, if present, run against the downlift job workspace.
7. Final completion merges the downlift job workspace back into the parent
   workspace, creating a new parent workspace version.

#### 10.5.4 Chat Workflow

Chat is simpler and more interactive than uplift/downlift.

1. The user enters or creates a **chat workspace** from a parent workspace
   version.
2. The UI opens that chat workspace directly and attaches CRDT to it.
3. Each user message creates one or more chat tasks against the current
   chat workspace version.
4. The worker executes against that chat workspace context. It may create
   ephemeral task workspaces or repo-local refs internally, but those are
   not user-visible.
5. Each accepted result creates a new chat workspace version, which the UI
   continues to display.
6. If the user chooses to apply the chat result back to another workspace,
   the API performs an explicit workspace merge, creating a new target
   workspace version.

`g5 push` on a chat workspace advances only the chat workspace. It does
not implicitly mutate the parent workspace.

#### 10.5.5 Repo-Level Implementation

Under the hood, each repo may still use internal refs such as
`job/<job_id>` or `task/<task_id>`. Those refs are not part of the
user-facing contract.

The important implementation rules are:

- `ops::merge()` remains the universal repo-level primitive for
  fast-forward-or-rebase behaviour.
- `ops::merge_workspace()` (g5-common, pure git) matches repos by
  ID, **computes** the repo-level merge per repo, and returns
  `Vec<MergeResult>` containing target SHAs — but does **not** advance
  any git refs. It has no DB dependency and is usable by workers and
  CLI directly.
- The API-side `service::merge_workspace_api()` wraps `ops::merge_workspace()`
  with a DB transaction to produce a new workspace version. Only after
  all repos succeed does it advance the git refs via
  `ops::advance_refs(&results, &repo_storage)`, ensuring atomicity —
  a conflict in any repo prevents ref advancement in all repos.
- Rebase/replay is preferred over merge commits so repo history remains
  linear and task promotion order is unambiguous.
- Worker isolation can be CoW, worktree, shallow checkout, or another
  transport, as long as the resulting workspace version semantics are the
  same.

#### 10.5.6 Push Atomicity and Worker Separation

**Today (shared storage):** workers and the API may share repo storage, and
repo-level git operations remain atomic via git/libgit2 ref update
mechanics.

**Future (separated workers):** workers no longer share the same storage as
the API. In that model, the worker must send repo objects or packs to the
API, and the API becomes the authority that creates the next workspace
version.

The atomic unit is not "move one branch ref"; it is:

1. stage all repo-level changes required for the next workspace version
2. create the next workspace version record with its pinned repo SHAs
3. advance the workspace head to that version

Either all three happen, or none do.

**Two-layer design:** The merge logic is split into a pure-git layer
in `g5-common` and a DB-aware service layer in `g5-api`:

1. **`ops::merge_workspace()`** (g5-common, pure git) — takes
   `WorkspaceBranch` slices and `RepoStorage`, runs per-repo
   `ops::merge()`, returns `Vec<MergeResult>`. No DB dependency.
   Usable directly by workers and CLI.

2. **`service::merge_workspace_api()`** (g5-api) — wraps (1) in a DB
   transaction: resolves workspace branches from the DB, calls
   `ops::merge_workspace()`, creates the workspace version
   record, advances the workspace head, and rolls back on conflict.

```rust
// g5-api/src/services/workspace_service.rs
// API-side wrapper — adds DB transaction around the pure-git ops layer.
async fn merge_workspace_api(db: &DbPool, ...) -> Result<WorkspaceVersion> {
    let mut tx = db.begin().await?;

    // Resolve branches from DB
    let source_branches = load_workspace_branches(&mut tx, source_id).await?;
    let target_branches = load_workspace_branches(&mut tx, target_id).await?;

    // Pure-git merge — no DB interaction
    let results = ops::merge_workspace(
        &source_branches, &target_branches, &repo_storage,
    )?;

    // Check for conflicts
    let repo_shas: Vec<(Uuid, String)> = results.iter().filter_map(|r| match r {
        MergeResult::FastForwarded { repo_id, new_head }
        | MergeResult::Rebased { repo_id, new_head } => Some((*repo_id, new_head.clone())),
        MergeResult::Added { repo_id, new_head } => Some((*repo_id, new_head.clone())),
        MergeResult::Conflict { repo_id, error } => None,
    }).collect();

    if results.iter().any(|r| matches!(r, MergeResult::Conflict { .. })) {
        tx.rollback().await?;
        return Err(G5Error::MergeConflict { results });
    }

    // All repos merged — create new workspace version
    let version = create_workspace_version(&mut tx, workspace_id, &repo_shas).await?;
    advance_workspace_head(&mut tx, workspace_id, &version).await?;
    tx.commit().await?;

    Ok(version)
}
```

**Relationship between `workspace_branches` and `workspace-meta`:**
The `workspace_branches` DB table is a denormalized cache of
per-repo HEAD refs — updated by `advance_head()` on every push for
fast queries. The `workspace-meta` git repo (§2.1.2a) is the
authoritative version history — updated by `merge_workspace_api()`
when tasks complete and workspace versions are created. Repo-level
pushes update `workspace_branches` only. Workspace-level events
(task completion, job merge-back) update both `workspace_branches`
and the `workspace-meta` repo atomically within the same DB
transaction.

If any single repo merge fails (conflict), the entire transaction rolls
back — no partial workspace version is created, and no repo refs are
advanced. The caller receives the conflict error and can retry after
resolving conflicts.

**Important distinction: repo-level push handling vs workspace version
advancement.** The `receive_pack` handler's inline `advance_head()`
call fires at the git transport layer when a worker pushes to a single
repo. This updates `workspace_branches.head_commit` for that repo — a
repo-level bookkeeping operation. It does **not** create workspace
versions. Workspace version advancement happens exclusively inside
`service::merge_workspace_api()`, triggered by `transition_task(completed)`.
This means a worker can push to repo A and repo B independently, and
no intermediate workspace version is created until the task completes
and the merge-back succeeds atomically.

#### 10.5.7 Interaction with Drafts and UI Notifications

The user-facing event should be "this workspace advanced to a new
workspace version", even if the current implementation still emits
branch-scoped notifications internally.

- Internal task workspace → job workspace promotions are usually not shown
  to end users unless that workspace is open in the UI.
- Parent workspace merge-back **must** notify the parent workspace view,
  because that is what users typically see.
- Chat workspace advancement is always user-visible because the chat
  workspace is itself the interactive editing surface.

If dirty CRDT state exists on the target workspace, the same draft-guard
logic still applies. The guard should conceptually protect the target
workspace's current editable head, even if the current implementation
checks repo-level branch state.

### 10.6 Rebase Conflict Retry Policy

When `ops::advance_branch()` detects divergence and rebases, the rebase
may fail due to conflicts. This section specifies the retry semantics.

**Parameters:**

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `max_retries` | 3 | Limits task churn; most conflicts are transient (concurrent pushes) |
| Backoff | None (immediate) | Each retry re-fetches the latest base, so delay adds no value |
| Retry scope | Re-fetch base + replay | The base branch may have advanced again during the rebase attempt |

**On exhaustion (3 failed rebases):**

1. Task transitions to `Failed` with `error_code: REBASE_CONFLICT`
2. Job remains `InProgress` — supervisor can dispatch a new task against
   the updated base, or escalate to human review
3. API emits a WebSocket event: `job.conflict` with the conflicting file
   list so the UI can surface it to the user
4. `g5 push --git-remote` CLI variant: prints the conflicting files to
   stderr and exits with code 1. The user resolves manually.

**Worker vs CLI differences:**

- **Worker:** automatic retry within the task lifecycle. On exhaustion
  the task fails and the API decides next steps.
- **CLI:** single attempt by default. `g5 push --retry=3` enables
  automatic retry (same semantics as worker). Most CLI users will
  prefer to resolve conflicts manually.

### 10.7 Shallow Clone Depth and Agent History Access

Workers use `CheckoutMode::ShallowClone` to save time and bandwidth.
The clone depth affects what `git log` / `git blame` an agent tool
can access.

**Default depth:** `--depth=1` (single commit).

**Rationale:** Agents rarely need deep history. When they do, the
specific tools that need it can deepen the clone on demand:

1. **`git_log` tool** — if the agent asks for N commits of history,
   the tool calls `git fetch --deepen=N` before running `git log`.
   This is lazy and avoids cloning history that's never used.
2. **`git_blame` tool** — similar: `git fetch --deepen=50` to get
   enough context for blame annotations.
3. **Full clone fallback** — if multiple history-dependent ops exceed
   a threshold (e.g. 3 deepen calls), the tool calls
   `git fetch --unshallow` once and caches the result for the task
   duration.

**Alternative considered:** `--depth=50` as default. Rejected because
it penalises the common case (agent doesn't need history) to optimise
the rare case (agent uses blame/log). Lazy deepening is cheaper on
average.

**Constraint:** `ops::push()` from a shallow clone works correctly
with modern git (2.11+). The worker pushes only the new commits it
created, which are fully connected to the shallow boundary.

### 10.8 Multi-Repo Push Atomicity

When `g5 push --git-remote` pushes to both the G5 API and an
external SCM (GitHub/GitLab), the two pushes are not atomic. This
section documents the failure modes and recovery.

**Push order:** G5 API first, SCM second.

**Rationale:** The G5 API is the source of truth. If the SCM push
fails, G5 still has the commits and a retry can be attempted. The
reverse (SCM has commits that G5 doesn't) is harder to recover from.

**Failure modes:**

| Scenario | G5 push | SCM push | Recovery |
|----------|---------|----------|----------|
| Happy path | ✅ | ✅ | — |
| SCM down | ✅ | ❌ | Retry SCM push later (`g5 push --git-remote --retry-scm`) |
| G5 down | ❌ | skipped | Retry entire push when G5 is available |
| Both down | ❌ | skipped | Same as above |
| SCM rejects (force-push protection) | ✅ | ❌ | Surface error to user; may need `--force` or branch reconciliation |

**Per-repo status reporting:** The push command returns a structured
result (JSON in `--json` mode) with per-remote status:

```json
{
  "g5": { "status": "ok", "sha": "abc123" },
  "origin": { "status": "error", "error": "rejected (non-fast-forward)" }
}
```

The CLI prints a human-readable summary. The worker reports per-remote
status in the task completion payload.

**Non-goal:** True two-phase commit across G5 and SCM. The complexity
is not justified — the G5-first model with retry covers all practical
failure modes.

---

## 13. Task Lease and TTL

### 13.1 Problem

When a worker crashes, is OOM-killed, or loses network connectivity
mid-task, the task remains in `status = 'running'` with a `worker_id`
that will never report back. There is no mechanism to reclaim these
orphaned tasks. The heartbeat endpoint (`POST /api/internal/workers/:id/heartbeat`)
records `last_heartbeat` on the worker row but nothing checks it.

Chat tasks have an ad-hoc deadline via `spawn_timeout_task()` in
`chat_service.rs`, but batch tasks (uplift, downlift, test, review)
can be stuck indefinitely.

### 13.2 Design: Lease-Based Task Ownership

Every task assignment is a **lease** with a TTL. The worker must renew
the lease via heartbeat or the task is reclaimed.

#### Schema Changes

```sql
ALTER TABLE tasks ADD COLUMN lease_expires_at  TIMESTAMPTZ;
ALTER TABLE tasks ADD COLUMN lease_renewals    INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN max_duration_secs INTEGER;  -- per-task-type ceiling
ALTER TABLE tasks ADD COLUMN retry_count       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN max_retries       INTEGER NOT NULL DEFAULT 3;

ALTER TABLE repos ADD COLUMN git_remote_push   BOOLEAN NOT NULL DEFAULT false;
-- true → post-push side-effects mirror to external SCM via ScmTokenManager
```

`lease_expires_at` is set at assignment time and extended on each
heartbeat. `max_duration_secs` is an absolute ceiling — the task fails
even if heartbeats are arriving (guards against infinite agent loops).

#### Lease Parameters

| Task Type  | Lease TTL | Max Duration | Rationale |
|------------|-----------|--------------|-----------|
| chat       | 60 s      | 5 min        | Interactive — fast failure |
| bootstrap  | 90 s      | 15 min       | Indexing a large repo |
| review     | 90 s      | 10 min       | Semi-interactive |
| uplift     | 90 s      | 30 min       | Large multi-file transforms |
| downlift   | 90 s      | 30 min       | Same as uplift |
| test       | 90 s      | 20 min       | Test suite execution |
| compile    | 90 s      | 15 min       | Build step |

Lease TTL is always ≥ 2× the heartbeat interval (30 s) to tolerate
one missed heartbeat without false reclamation.

**Backfill:** Existing `running` tasks in the database will have
`retry_count = 0` and `max_retries = 3` (column defaults). This is
correct — orphaned running tasks should be reclaimed and retried
normally. If any task should be exempt from retry (e.g., a long-running
import), set `max_retries = 0` explicitly at creation time.

**max_duration vs retry:** Tasks exceeding `max_duration_secs` are
immediately marked `'failed'` and do **not** enter the retry queue.
Only tasks whose lease expires due to a missed heartbeat (worker crash)
can be reclaimed and retried up to `max_retries` times.

#### Assignment (poll_for_task)

```rust
// In the CTE that assigns the task:
UPDATE tasks SET
    status = 'running',
    worker_id = $1,
    assigned_at = now(),
    lease_expires_at = now() + make_interval(secs => $2),  -- lease_ttl
    max_duration_secs = $3
WHERE ...
```

The API resolves `lease_ttl` and `max_duration_secs` from a config map
keyed by `task_type`. These values are also returned in `TaskAssignment`
so the worker knows when to self-checkpoint.

#### Heartbeat Renewal

The existing heartbeat endpoint already receives `active_task_ids`.
Add a lease extension to the handler:

```rust
// worker_heartbeat() — after updating last_heartbeat on the worker:
sqlx::query!(
    "UPDATE tasks
     SET lease_expires_at = now() + make_interval(secs => $1),
         lease_renewals = lease_renewals + 1
     WHERE id = ANY($2)
       AND worker_id = $3
       AND status = 'running'",
    lease_ttl_secs,
    &active_task_ids,
    worker_id,
)
.execute(db.conn())
.await?;
```

The worker doesn't need any code changes — it already sends heartbeats
every 30 seconds with `active_task_ids`.

#### Reaper: Maintenance Sweep

A new background task runs on a fixed interval (every 30 seconds,
staggered across API replicas via `pg_try_advisory_lock`):

```rust
// maintenance/lease_reaper.rs
pub async fn reap_expired_leases(db: &DbPool) -> Result<u64> {
    // 1. Reclaim tasks whose lease has expired
    let expired = sqlx::query!(
        "UPDATE tasks
         SET status = 'pending',
             worker_id = NULL,
             assigned_at = NULL,
             lease_expires_at = NULL,
             retry_count = retry_count + 1,
             updated_at = now()
         WHERE status = 'running'
           AND lease_expires_at IS NOT NULL
           AND lease_expires_at < now()
           AND retry_count < max_retries
         RETURNING id, job_id, worker_id AS old_worker_id"
    )
    .fetch_all(db.conn())
    .await?;

    for row in &expired {
        tracing::warn!(
            task_id = %row.id,
            job_id = %row.job_id,
            old_worker = ?row.old_worker_id,
            "Reclaimed expired lease — task returned to pending"
        );
        // Emit WebSocket event: task.reclaimed
    }

    // 2. Fail tasks that exceeded max_duration
    let timed_out = sqlx::query!(
        "UPDATE tasks
         SET status = 'failed',
             updated_at = now()
         WHERE status = 'running'
           AND assigned_at IS NOT NULL
           AND max_duration_secs IS NOT NULL
           AND assigned_at + make_interval(secs => max_duration_secs) < now()
         RETURNING id, job_id, worker_id AS old_worker_id"
    )
    .fetch_all(db.conn())
    .await?;

    for row in &timed_out {
        tracing::warn!(
            task_id = %row.id,
            job_id = %row.job_id,
            old_worker = ?row.old_worker_id,
            "Task exceeded max_duration — marked failed"
        );
    }

    // 3. Mark workers with no heartbeat in 3 minutes as terminated
    let dead_workers = sqlx::query!(
        "UPDATE workers
         SET status = 'terminated',
             terminated_at = now()
         WHERE status IN ('idle', 'assigned', 'executing')
           AND last_heartbeat < now() - interval '3 minutes'
         RETURNING id"
    )
    .fetch_all(db.conn())
    .await?;

    Ok(expired.len() as u64 + timed_out.len() as u64)
}
```

#### Race Condition: Worker Completes After Reclamation

If a worker completes a task that was already reclaimed:

1. `transition_task(task_id, 'completed')` checks `worker_id` matches
   the caller. If the task was reclaimed and reassigned to a different
   worker, the original worker gets `403 Forbidden`.
2. The worker logs a warning and discards its result. The task is
   already being handled by the new assignee.
3. If the task was reclaimed but not yet reassigned (still `pending`),
   the completion from the original worker is also rejected — the
   reclamation implies the worker is untrusted.

#### Worker-Side Awareness

The worker doesn't need to track lease expiry directly. However, the
`TaskAssignment` includes `max_duration_secs` so the worker can:

- Set a tokio timeout on the agent loop
- Checkpoint progress before the deadline
- Log a warning at 80% of max_duration

```rust
// worker/mod.rs — in the task execution loop
let deadline = Duration::from_secs(assignment.max_duration_secs as u64);
tokio::select! {
    result = agent_loop.run() => handle_result(result),
    _ = tokio::time::sleep(deadline) => {
        tracing::warn!("Task approaching max_duration — checkpointing");
        checkpoint_and_yield().await;
    }
}
```

### 13.3 Interaction with Chat Preemption

The lease reaper and chat preemption (§4.3) are independent mechanisms:

- **Lease reaper:** reclaims tasks from *dead* workers (no heartbeat)
- **Preemption:** interrupts *live* workers to prioritise chat tasks

If a preempted worker checkpoints and yields, it transitions the task
to `'paused'` — which the reaper ignores (it only touches `'running'`).

### 13.4 Backfill

Existing `running` tasks in the database have `lease_expires_at = NULL`.
The reaper query requires `lease_expires_at IS NOT NULL`, so old tasks
are unaffected. A one-time migration script can set
`lease_expires_at = now() + interval '10 minutes'` on any currently
running tasks, or they can be manually resolved.

---

## 14. Git HTTPS Serving Protocol

### 14.1 Problem

Workers currently access bare repos via a shared NFS mount
(`workspace_git_url` returns a local filesystem path). Removing NFS
from workers (§5) requires a network-based git transport. The API
already maintains a mirror of every repo (§7, `repo_fetcher`). The
question is how workers clone from and push to that mirror.

### 14.2 Design: Smart HTTP Git Protocol

The API exposes git repos over HTTPS using the standard **smart HTTP
protocol** (git-http-backend equivalent). This is the same protocol
GitHub, GitLab, and Gitea use.

**Endpoint pattern:**
```
https://<api-url>/git/<repo_id>/info/refs?service=git-upload-pack   (clone/fetch)
https://<api-url>/git/<repo_id>/git-upload-pack                     (clone/fetch)
https://<api-url>/git/<repo_id>/info/refs?service=git-receive-pack  (push)
https://<api-url>/git/<repo_id>/git-receive-pack                    (push)
```

These four endpoints implement the full git smart HTTP protocol
([Git HTTP Transport docs](https://git-scm.com/docs/http-protocol)).

#### Implementation: Subprocess with `--stateless-rpc`

The API implements the smart HTTP protocol by spawning
`git upload-pack` / `git receive-pack` as **subprocesses** with the
`--stateless-rpc` flag. This is the same approach used by Gitea,
Forgejo, and GitLab Workhorse — battle-tested at scale.

**Why not CGI (`git http-backend`)?** CGI injects unvalidated HTTP
headers into environment variables (`HTTP_*`, `QUERY_STRING`, etc.),
creating a header-injection attack surface. The subprocess model
avoids this entirely: the Axum handler owns the HTTP layer, validates
auth, resolves the repo path, and only passes a controlled repo path
to the git subprocess via argv. No environment variable injection.

**Why not gitoxide (gix) native?** Gitoxide's transport and protocol
crates (`gix-transport`, `gix-protocol`) are client-side only — there
is no server-side `accept()`, `upload-pack` handler, or
`receive-pack` handler as of v0.58 (March 2026). The plumbing crates
(`gix-pack`, `gix-odb`, `gix-packetline`) could theoretically be
assembled into a server, but this would mean pioneering ~2000+ lines
of protocol state machine code with no upstream precedent. Logged as
tech debt to revisit when gitoxide adds server-side support (§9.2).

```rust
// g5-api/src/routes/git.rs

use axum::body::Body;
use tokio::process::Command;
use tokio::io::AsyncWriteExt;

/// GET /git/:repo_id/info/refs?service=git-upload-pack
async fn info_refs(
    Path(repo_id): Path<String>,
    Query(params): Query<InfoRefsParams>,
    auth: GitAuth,              // validates registration token
) -> Result<impl IntoResponse, GitError> {
    let repo_path = repo_storage.local_path(&repo_id)
        .ok_or(GitError::RepoNotFound)?;
    let service = params.service;  // "git-upload-pack" or "git-receive-pack"

    if service == "git-receive-pack" {
        auth.require_push(&repo_id).await?;
    }

    // Subprocess — NOT CGI. No HTTP env vars are passed.
    // Only the repo path is provided via argv.
    //
    // GIT_BIN is resolved once at startup via `which git` (or a
    // config key) and stored in AppState. Do not hardcode
    // "/usr/bin" — the git binary location varies across
    // container images and distros.
    let output = Command::new(&state.git_bin)   // resolved at startup
        .args([&service, "--stateless-rpc", "--advertise-refs"])
        .arg(&repo_path)             // last arg = repo path
        .env_clear()                 // clean environment
        .env("PATH", "/usr/bin:/usr/local/bin") // fallback PATH
        .env("GIT_DIR", &repo_path)  // explicit GIT_DIR
        // Preserve TLS configuration so custom CA bundles work
        .envs(std::env::var("GIT_SSL_CAINFO").ok().map(|v| ("GIT_SSL_CAINFO", v)))
        .envs(std::env::var("SSL_CERT_DIR").ok().map(|v| ("SSL_CERT_DIR", v)))
        .output().await
        .map_err(GitError::SubprocessFailed)?;

    if !output.status.success() {
        return Err(GitError::SubprocessExit(output.stderr));
    }

    // Pkt-line header + refs advertisement
    let content_type = format!("application/x-{}-advertisement", service);
    let mut body = pkt_line_header(&service);
    body.extend_from_slice(&output.stdout);

    Ok(([(header::CONTENT_TYPE, content_type)], body))
}

/// POST /git/:repo_id/git-upload-pack
async fn upload_pack(
    Path(repo_id): Path<String>,
    auth: GitAuth,
    body: Bytes,
) -> Result<impl IntoResponse, GitError> {
    let repo_path = repo_storage.local_path(&repo_id)
        .ok_or(GitError::RepoNotFound)?;

    // Spawn git upload-pack with clean env — stdin/stdout streaming
    let mut child = Command::new("git")
        .args(["upload-pack", "--stateless-rpc"])
        .arg(&repo_path)
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("GIT_DIR", &repo_path)
        // Preserve TLS configuration so custom CA bundles work
        .envs(std::env::var("GIT_SSL_CAINFO").ok().map(|v| ("GIT_SSL_CAINFO", v)))
        .envs(std::env::var("SSL_CERT_DIR").ok().map(|v| ("SSL_CERT_DIR", v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::SubprocessFailed)?;

    // Stream request body → subprocess stdin
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&body).await?;
    drop(stdin);  // close stdin to signal EOF

    // Stream subprocess stdout → HTTP response body without buffering
    let stdout = child.stdout.take().unwrap();
    let stream = tokio_util::io::ReaderStream::new(stdout);
    let body = Body::from_stream(stream);

    // Spawn background task to reap the child process
    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    Ok((
        [(header::CONTENT_TYPE, "application/x-git-upload-pack-result")],
        body,
    ))
}

/// POST /git/:repo_id/git-receive-pack
///
/// NOTE: `body: Bytes` fully buffers the request into memory. For
/// receive-pack, the body contains the pkt-line ref negotiation
/// (small, ~few KB) followed by the packfile. In the worker push
/// path this is acceptable — worker pushes are small incremental
/// diffs (typically 1–5 commits). If large pushes become a concern
/// (e.g. initial bootstrap or bulk import), this should be changed
/// to stream the body directly to the subprocess stdin via
/// `axum::body::Body` + `tokio::io::copy`, teeing off only the
/// pkt-line prefix for ref-update parsing.
async fn receive_pack(
    Path(repo_id): Path<String>,
    headers: HeaderMap,
    auth: GitAuth,
    State(state): State<AppState>,  // contains db, ws_broadcaster
    body: Bytes,
) -> Result<impl IntoResponse, GitError> {
    auth.require_push(&repo_id).await?;
    let repo_path = repo_storage.local_path(&repo_id)
        .ok_or(GitError::RepoNotFound)?;

    // --- Pre-push validation (inline, no hooks) ---
    // Parse pkt-line ref updates from the request body before spawning git.
    // If the pkt-line stream is malformed, return HTTP 400 immediately —
    // never invoke the git subprocess on unparseable input.
    let ref_updates = parse_receive_pack_ref_updates(&body)
        .map_err(|e| GitError::MalformedPktLine(e.to_string()))?;
    let force = headers.get("X-G5-Force-Push").map_or(false, |v| v == "true");
    let override_draft = headers.get("X-G5-Override-Draft").map_or(false, |v| v == "true");

    for update in &ref_updates {
        // 1. Draft guard — reject if dirty drafts exist on the target branch
        //    unless --override-draft is set (requires draft:override permission).
        //    NOTE: This check is best-effort, not transactionally atomic with
        //    the subprocess spawn.  If a browser commits the draft between
        //    this check and the subprocess, the push would have been valid
        //    moments later — we tolerate this narrow race.
        if has_dirty_draft(&state.db, &repo_id, &update.ref_name).await? {
            if override_draft {
                auth.require_permission("draft:override").await?;
                invalidate_draft(&state.db, &repo_id, &update.ref_name).await?;
            } else {
                return Err(GitError::DraftGuard(update.ref_name.clone()));
            }
        }
        // 2. Stale-push rejection — unless force-push is authorized
        if !force && !is_descendant(&repo_path, &update.old_sha, &update.new_sha)? {
            return Err(GitError::NonDescendant(update.ref_name.clone()));
        }
        if force {
            auth.require_permission("push:force").await?;
        }
        // 3. Auth re-check — verify pusher still has active task
        auth.require_active_task(&repo_id).await?;
    }

    // Determine push source from auth context
    let source = if auth.is_worker() { AdvanceSource::Worker } else { AdvanceSource::Cli };

    // --- Spawn git subprocess (no hook env vars needed) ---
    let mut child = Command::new(&state.git_bin)  // resolved at startup
        .args(["receive-pack", "--stateless-rpc"])
        .arg(&repo_path)
        .env_clear()
        .env("PATH", "/usr/bin:/usr/local/bin")
        .env("GIT_DIR", &repo_path)
        // Preserve TLS configuration so custom CA bundles work
        // (e.g., GCP private CA, corp proxies).
        .envs(std::env::var("GIT_SSL_CAINFO").ok().map(|v| ("GIT_SSL_CAINFO", v)))
        .envs(std::env::var("SSL_CERT_DIR").ok().map(|v| ("SSL_CERT_DIR", v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::SubprocessFailed)?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&body).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        return Err(GitError::SubprocessExit(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    // --- Post-push side-effects (inline, no hooks) ---
    for update in &ref_updates {
        advance_head(
            &state.db,
            &state.ws_broadcaster,
            &repo_id,
            &update.ref_name,
            &update.old_sha,
            &update.new_sha,
            source,
            &auth.pusher_id(),
        ).await?;
    }

    Ok((
        [(header::CONTENT_TYPE, "application/x-git-receive-pack-result")],
        output.stdout,
    ))
}

/// Pkt-line service header per smart HTTP spec
fn pkt_line_header(service: &str) -> Vec<u8> {
    let line = format!("# service={}\n", service);
    let len = line.len() + 4; // 4 hex digits
    format!("{:04x}{}", len, line)
        .into_bytes()
        .into_iter()
        .chain(b"0000".iter().copied()) // flush packet
        .collect()
}
```

#### Security properties of the subprocess model

| Property | How it's achieved |
|---|---|
| No env injection | `env_clear()` — subprocess gets only `PATH`, `GIT_DIR`, and optionally `GIT_SSL_CAINFO`/`SSL_CERT_DIR` for TLS; no HTTP headers, query params, CGI variables, or hook callback ports |
| Controlled repo path | Axum handler resolves `repo_id` → filesystem path via `RepoStorage`; git receives only a validated path |
| No shell | `Command::new("git")` — direct exec, no shell interpretation |
| Resource limits | Subprocess inherits cgroup limits of the API pod; can add `tokio::time::timeout` per-request |
| Error isolation | Git crash → captured stderr, HTTP 500; no panic propagation into the API process |

**Why smart HTTP over alternatives:**

| Alternative | Drawback |
|---|---|
| CGI (`git http-backend`) | Env var injection attack surface; process-per-request with uncontrolled env |
| Git bundle upload | One-way (clone only), no push |
| Custom object API | Non-standard, can't use `git clone/push` natively |
| SSH | Requires SSH daemon, key management, non-HTTP auth |
| git:// protocol | No auth, no encryption |
| Sidecar (gitea/gitaly) | Extra deployment component, operational overhead |
| Native gitoxide server | Server-side support not implemented upstream; high build risk |

Smart HTTP with subprocess means workers use plain `git clone https://...` and
`git push https://...` — no special client code needed.

### 14.3 Authentication and Authorisation

Workers authenticate git operations with their **registration token**
(the same JWT used for the REST API). The token is passed via HTTP
Basic Auth (git's standard credential mechanism):

```
username: x-worker-token
password: <registration_jwt>
```

The `GitAuth` extractor on the API side:

1. Extracts credentials from the `Authorization` header (HTTP Basic
   or Bearer token — workers use Bearer via `http.extraHeader`, CLI
   users may use Basic via credential helper)
2. Validates the JWT (same logic as `ServiceTokenAuth` middleware)
3. Resolves the worker's `org_id` and `workspace_id` from the JWT claims
4. Checks authorisation against the requested `repo_id`

#### Auth Scopes

The registration token's JWT claims already carry `org_id` (and
optionally `workspace_id` for scoped tokens — §3.2). Authorisation
rules for git operations:

| Operation | Scope Check | Rationale |
|---|---|---|
| `git clone` / `git fetch` | Worker's `org_id` owns the repo | Read access for any repo in the org |
| `git push` | Worker's `task_id` is assigned to a workspace that uses this repo, AND task is in `running` status | Write access only while actively working a task |

The push scope check prevents:
- A worker pushing to a repo it's not assigned to
- A terminated/idle worker pushing stale results
- Cross-org access

```rust
// GitAuth extractor — push authorisation
fn authorize_push(worker: &Worker, repo_id: &str, db: &DbPool) -> Result<()> {
    let task = worker.task_id.ok_or(AuthError::NoActiveTask)?;

    // Verify the task is running and the repo matches
    let valid = sqlx::query_scalar!(
        "SELECT EXISTS(
            SELECT 1 FROM tasks t
            JOIN workspace_branches wb ON wb.id = t.workspace_ref
            WHERE t.id = $1
              AND t.worker_id = $2
              AND t.status = 'running'
              AND wb.repo_id = $3::uuid
        )",
        task, worker.id, repo_id
    )
    .fetch_one(db.conn())
    .await?;

    if !valid.unwrap_or(false) {
        return Err(AuthError::PushNotAuthorized);
    }
    Ok(())
}
```

#### CLI Authentication

For `g5 clone` / `g5 push` from the CLI, the git credential helper
uses the per-instance user token from `~/.g5/credentials.yaml` (§2.5):

```
username: x-user-token
password: <user_jwt>
```

The `GitAuth` extractor detects the `x-user-token` username and applies
user-level authorisation (org membership + workspace access) instead of
worker-level checks.

### 14.4 Push Validation and Side-Effects (Inline)

Because the API controls the entire git serving layer (§14.2), all
pre-push validation and post-push side-effects are performed **inline
in the `receive_pack` Axum handler** — no git hooks, no shell scripts,
no localhost HTTP callbacks.

Git hooks are fragile: they require shell scripts installed on bare
repos, spawn extra processes, introduce TOCTOU races (e.g., force-push
authorization records that expire between the API call and the hook
check), and create a dual notification path (hook vs. inline). Since
the API already orchestrates the git subprocess, all validation and
side-effects belong in the handler itself.

#### Pre-push validation (before spawning `git receive-pack`)

The `receive_pack` handler parses the incoming pack request to extract
the ref updates (old SHA, new SHA, ref name) from the pkt-line stream
**before** spawning the `git receive-pack` subprocess. It then runs
three checks:

1. **Draft guard** (§2.10.1) — reject push if dirty drafts exist on
   the target branch
2. **Stale-push rejection** — reject if the pushed commits are not
   descended from the current branch tip (enforces rebase-only flow),
   **unless** the push request includes the `force: true` flag (see
   below)
3. **Authorisation re-check** — verify the pusher still has an active
   running task (defense in depth against race with lease expiry)

If any check fails, the handler returns an error response immediately
without spawning the subprocess. The git client receives the rejection.

**Force-push support:** The `g5 push --force` CLI command sets a
custom HTTP header (`X-G5-Force-Push: true`) on the `git-receive-pack`
request. The `receive_pack` handler reads this header and, if the
pusher holds the `push:force` permission, skips the stale-push
ancestry check. No DB records, no TTLs, no TOCTOU race.

#### Post-push side-effects (after subprocess completes)

After the `git receive-pack` subprocess exits successfully, the handler
executes the following inline — still within the same request:

1. **Task notification** — update `tasks.head_commit` in the DB
2. **Job advance** — trigger `ops::advance_branch()` to fast-forward
   the job branch (§10.5 Phase 7)
3. **Re-index** — enqueue the updated branch for re-indexing
4. **WebSocket events** — emit `branch.advanced` to notify the UI
5. **SCM mirror push** — if the repo has `git_remote_push = true`
   (boolean column on the `repos` table, set when the workspace is
   linked to an external SCM), push to the external SCM via
   `ScmTokenManager` using the remote URL and credentials resolved
   from the `scm_connections` table

All five steps call the shared `advance_head()` function (§10.3.6),
which is idempotent — if `head_commit` already equals the new SHA,
it's a no-op.

#### Why no git hooks

| Concern | Inline approach | Hook approach (rejected) |
|---|---|---|
| **TOCTOU** | Validation and subprocess in same handler — no race window | Force-push DB record can expire between API call and hook check |
| **Reliability** | No shell scripts to install, maintain, or lose during repo migration | Hooks must be installed on every bare repo; missing hook = silent bypass |
| **Observability** | All errors are Rust-level, logged with request context | Shell hook errors are stderr strings, hard to correlate |
| **Process overhead** | Zero extra processes | Each hook spawns a shell + curl per ref update |
| **Dual path** | Single notification path for all push sources | Hook fires AND inline call fires → must deduplicate |

### 14.5 Worker Git Configuration

The git serving endpoints (§14.2) are **internal-only** — served on the
API's internal port behind the cluster network boundary (§14.6). Workers
authenticate via their registration JWT, passed as an HTTP header on
each git command. No credential helper scripts, no askpass, no files
written to disk.

**How it works:** The worker sets the `Authorization` header on every
git subprocess invocation using git's `http.extraHeader` config, passed
via environment variables so nothing is persisted:

```rust
// worker/lifecycle.rs — phase_prepare
fn git_command_with_auth(token: &str) -> Command {
    let mut cmd = Command::new("git");
    // Pass the auth header via process-scoped env vars.
    // GIT_CONFIG_* env vars are read by git but NOT persisted to any
    // config file, and NOT inherited by child processes of the git
    // subprocess (git does not re-export them).
    cmd.env("GIT_CONFIG_COUNT", "1")
       .env("GIT_CONFIG_KEY_0", "http.extraHeader")
       .env("GIT_CONFIG_VALUE_0", format!("Authorization: Bearer {}", token));
    cmd
}
```

The `GitAuth` extractor on the API side (§14.3) accepts both HTTP Basic
and Bearer tokens. Workers use Bearer; the CLI may use either.

**Security properties:**
- No credentials written to disk (no askpass script, no `.gitconfig`)
- No shell interpretation — `Command::new("git")` is direct exec
- Token is process-scoped via env vars, automatically cleaned up when
  the git subprocess exits
- No global git config mutation — concurrent tasks on the same worker
  cannot leak credentials to each other

The `workspace_git_url` in `TaskAssignment` changes from a local
filesystem path to an HTTPS URL:

```rust
// task_service.rs — poll_for_task(), replacing local_path lookup:
let workspace_git_url = format!(
    "{}/git/{}",
    api_internal_url,   // e.g. "https://api.g5-internal:8443"
    wb.repo_id
);
```

Workers then use standard git commands via the helper:

```rust
// Clone
git_command_with_auth(&token)
    .args(["clone", "--depth=1", "--branch", &task_branch, &url, "./workspace"])
    .status()?;

// Push
git_command_with_auth(&token)
    .args(["push", &url, &task_branch])
    .current_dir(&workspace_path)
    .status()?;
```

### 14.6 Networking

The git endpoints are on the **internal** API (same as
`/api/internal/workers/*`), not the public-facing API. Workers reach
them over the cluster-internal network:

```
Worker Pod → (ClusterIP) → g5-api Service → git handler
```

No Ingress or public exposure. The API's internal listener already
handles worker registration, poll, heartbeat on a separate port
(or path prefix). Git endpoints join the same internal router.

**TLS:** The internal API should use mTLS (mutual TLS) between workers
and API. The worker's registration token provides identity, and mTLS
provides transport encryption + server authentication. For Phase 1, TLS
with server-cert-only is acceptable (workers validate the API's cert
but don't present their own).

### 14.7 Performance Considerations

| Concern | Mitigation |
|---|---|
| Clone latency | `--depth=1` shallow clone. Typical repo < 100 MB. |
| Push size | Worker pushes only new commits (1–5 commits, small diff). |
| Concurrent clones | 5 workers × 1 clone each. API spawns git-upload-pack per request — negligible load. |
| Large repos | Phase 2 GCS snapshots (§9.2) bypass git clone entirely for initial checkout. |
| API memory | Streaming response for upload-pack — don't buffer entire packfile in memory. |

### 14.8 Migration Path

| Phase | Worker git access | Change |
|---|---|---|
| Current | NFS filesystem path | — |
| Phase 1 | HTTPS to API mirror | Add git endpoints to API, change `workspace_git_url` format, remove NFS mount |
| Phase 2 | HTTPS + GCS snapshot | Clone from API for incremental fetch; initial checkout from GCS tarball |
| Phase 3 | GCS only | API serves git protocol backed by GCS objects (no local filesystem) |

---

## 15. Cost Summary

| Phase   | Filestore | GCS    | Total Storage | Delta     |
|---------|-----------|--------|---------------|-----------|
| Current | $200/mo   | $0     | $200/mo       | —         |
| Phase 1 | $200/mo   | $0     | $200/mo       | $0        |
| Phase 2 | $200/mo   | $20/mo | $220/mo       | +$20      |
| Phase 3 | $0        | $35/mo | $35/mo        | **−$165** |

Note: compute costs (~$800–1500/mo for workers) dominate the bill
regardless. Storage is <15% of total infrastructure cost. The primary
motivation for this migration is **security** (removing NFS from Kata VMs)
and **operational simplicity** (fewer infrastructure components), not cost.

---

## 16. Decision

**NFS is fine to start.** The immediate fix is removing the NFS mount
from worker pods (Phase 1, Helm-only change). We will migrate to
GCS + database (Phase 2 → Phase 3) as the system scales and the security
boundary between workers and storage becomes the binding constraint.

---

## 17. Implementation Tasks, Test Cases, and E2E Verification Plan

This section defines the concrete implementation tasks, their unit and
integration test cases, and the end-to-end scenarios that verify the
design as a whole. Organised into **measurable milestones** — each
milestone has a defined scope, its associated tasks, and an E2E gate
test that must pass before the milestone is considered complete.

### 17.0 Milestone Map

| Milestone | Tasks | E2E Gate Tests | Exit Criteria |
|---|---|---|---|
| **M1 — Foundation** | TASK-01, TASK-02 | — (unit tests only) | `ops::` module exists, CRDT bug fixed, `cargo test -p g5-common -- ops` green |
| **M2 — Git Transport** | TASK-11, TASK-12, TASK-13 | E2E-01 | Workers clone/push via HTTPS; inline push validation enforces draft guard + stale push rejection |
| **M3 — Worker Isolation** | TASK-04, TASK-06 | E2E-01, E2E-11 | Workers run without NFS; all worker git ops go through `ops::` |
| **M4 — Token Scoping + Secrets** | TASK-07, TASK-08 | E2E-03, E2E-04, E2E-06 | Scoped tokens enforced; secrets delivered at poll time |
| **M5 — Task Lifecycle** | TASK-09, TASK-10 | E2E-02, E2E-05, E2E-13 | Chat priority works; lease TTL enforced; reaper reclaims expired tasks |
| **M6 — CLI Unification** | TASK-03, TASK-14 | E2E-09 | CLI commands use `ops::*`; URL-based clone works |
| **M7 — API Service Migration** | TASK-05 | E2E-09, E2E-10, E2E-14 | API services use `ops::*`; concurrent job completion safe |
| **M10 — Conflict Detection** | TASK-18 | E2E-15 | Third-party agent conflict detect+notify; `g5 conflicts` CLI; `.g5/crdt.sock` emits events |
| **M9 — Full Integration** | (all Phase 1) | ALL Phase 1 E2E | All Phase 1 E2E scenarios pass; test coverage matrix fully green; 191 total |

Phase 2 tasks (TASK-15 GCS snapshots, TASK-16 GCS worker checkout,
TASK-17 API keys) are deferred — they are not in the milestone
dependency graph and will be planned separately when needed.

**Milestone dependency graph:**

```
M1 ──→ M2 ──→ M3 ──→ M5
  │           ↗        ↘
  └──→ M4 ──┘      M9 (gate)
  │                  ↗
  └──→ M6 ──→ M7 ──┘
  │
  └──→ M10 ──→ M9
```

Each milestone below is self-contained: you can ship and verify each
one independently. The E2E gate tests are the "definition of done" —
if they pass, the milestone is complete and the next milestone can
begin.

### 17.1 Phase 1 — Must Fix (Blocking)

---

#### TASK-01: Extract `g5-common/src/ops/` Module

**Implementation:**
Create `g5-common/src/ops/mod.rs` with the following publicly exported
functions, each backed by `GitRepo` lower-level calls:

| Function | Signature (simplified) |
|---|---|
| `ops::commit` | `(repo: &GitRepo, branch: &str, message: &str, author: &GitAuthor, draft: &dyn DraftState) → Result<String>` |
| `ops::push` | `(repo: &GitRepo, remote: &str, refspec: &str, auth: &GitAuth) → Result<PushOutcome>` |
| `ops::clone` | `(url: &str, dest: &Path, branch: &str, auth: &GitAuth, opts: CloneOpts) → Result<GitRepo>` |
| `ops::fetch` | `(repo: &GitRepo, remote: &str, auth: &GitAuth) → Result<FetchOutcome>` |
| `ops::merge` | `(repo: &GitRepo, source: &str, target: &str, cow_path: Option<&Path>) → Result<MergeOutcome>` |
| `ops::setup_task_branch` | `(repo: &GitRepo, branch: &str, base_commit: &str) → Result<()>` |
| `ops::reset_hard` | `(repo: &GitRepo, target: &str, draft: &dyn DraftState) → Result<()>` |
| `ops::init` | `(path: &Path, bare: bool) → Result<GitRepo>` |
| `ops::draft_commit` | `(repo: &GitRepo, branch: &str, files: &[MaterializeFile], msg: &str, author: &GitAuthor, draft: &dyn DraftState) → Result<String>` |
| `ops::create_branch` | `(repo: &GitRepo, name: &str, commit: &str) → Result<()>` |
| `ops::resolve_head` | `(repo: &GitRepo, branch: &str) → Result<String>` |
| `ops::status` | `(repo: &GitRepo) → Result<Vec<StatusEntry>>` |
| `ops::checkpoint` | `(repo: &GitRepo, msg: &str, author: &GitAuthor) → Result<CheckpointOutcome>` |
| `ops::diff_check` | `(repo: &GitRepo, base: &str, head: &str) → Result<DiffSummary>` |
| `ops::checkout_branch` | `(repo: &GitRepo, branch: &str) → Result<()>` |
| `ops::open_read_only` | `(path: &Path) → Result<GitRepo>` |
| `ops::current_branch` | `(repo: &GitRepo) → Result<String>` |
| `ops::configure_safe_directory` | `(repo: &GitRepo) → Result<()>` |
| `ops::collect_commit_push` | `(repo: &GitRepo, msg: &str, author: &GitAuthor, remote: &str, refspec: &str, auth: &GitAuth) → Result<PushOutcome>` |
| `ops::merge_workspace` | `(source: &[WorkspaceBranch], target: &[WorkspaceBranch], storage: &RepoStorage) → Result<Vec<MergeResult>>` |

Also implement `DraftState` trait and four concrete impls:
`NoOpDraftState`, `CliDraftState`, `ApiDraftState`, `IndexerDraftState`.

**Files created:**
- `g5-common/src/ops/mod.rs`
- `g5-common/src/ops/merge.rs`
- `g5-common/src/ops/commit.rs`
- `g5-common/src/ops/clone.rs`
- `g5-common/src/ops/branch.rs`
- `g5-common/src/ops/draft.rs`
- `g5-common/src/ops/workspace.rs`

**Unit tests** (`g5-common/src/ops/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-OPS-01 | `test_commit_stages_all_and_creates_commit` | `ops::commit()` on a repo with untracked files → stages all, creates commit, returns SHA, HEAD matches |
| U-OPS-02 | `test_commit_with_dirty_draft_flushes_first` | `ops::commit()` with `MockDraftState { dirty: 3 }` → calls `flush()` before committing, `flush()` call count = 1 |
| U-OPS-03 | `test_commit_empty_tree_creates_empty_commit` | `ops::commit()` on clean repo → creates empty commit (not an error) |
| U-OPS-04 | `test_merge_fast_forward` | `ops::merge()` where target is ancestor of source → `MergeOutcome::FastForwarded`, target ref updated |
| U-OPS-05 | `test_merge_rebase_fallback` | `ops::merge()` where target has diverged → `MergeOutcome::Rebased`, source commits replayed on target |
| U-OPS-06 | `test_merge_already_up_to_date` | `ops::merge()` where source == target → `MergeOutcome::AlreadyUpToDate` |
| U-OPS-07 | `test_merge_conflict_returns_error` | `ops::merge()` with conflicting changes → `Err(G5Error::MergeConflict)` |
| U-OPS-08 | `test_merge_with_cow_path_fetches_local_first` | `ops::merge()` with `cow_path = Some(...)` → `fetch_local()` called, then merge proceeds |
| U-OPS-09 | `test_setup_task_branch_creates_and_checks_out` | `ops::setup_task_branch()` → branch exists, HEAD points to base_commit, working tree matches |
| U-OPS-10 | `test_setup_task_branch_deletes_stale_branch` | `ops::setup_task_branch()` when branch already exists → deletes old branch, creates fresh at base_commit |
| U-OPS-11 | `test_clone_basic` | `ops::clone()` with local file URL → repo cloned, correct branch checked out |
| U-OPS-12 | `test_clone_shallow` | `ops::clone()` with `CloneOpts { depth: Some(1) }` → single commit in history |
| U-OPS-13 | `test_push_success` | `ops::push()` to a local bare remote → ref updated on remote |
| U-OPS-14 | `test_push_non_fast_forward_rejected` | `ops::push()` when remote has diverged → `PushOutcome::Rejected` |
| U-OPS-15 | `test_fetch_updates_remote_refs` | `ops::fetch()` → `origin/main` ref updated to remote HEAD |
| U-OPS-16 | `test_reset_hard_with_dirty_drafts_errors` | `ops::reset_hard()` with `MockDraftState { dirty: 1 }` → `Err(G5Error::DraftDirty)` |
| U-OPS-17 | `test_reset_hard_moves_head` | `ops::reset_hard()` with `NoOpDraftState` → HEAD at target commit, working tree matches |
| U-OPS-18 | `test_create_branch_at_commit` | `ops::create_branch()` → branch ref exists, points to given SHA |
| U-OPS-19 | `test_create_branch_duplicate_errors` | `ops::create_branch()` with existing name → error |
| U-OPS-20 | `test_resolve_head_returns_sha` | `ops::resolve_head()` → 40-hex-char SHA matching HEAD |
| U-OPS-21 | `test_checkpoint_adds_commits_returns_diff` | `ops::checkpoint()` → commit created, `CheckpointOutcome.files_changed` populated |
| U-OPS-22 | `test_checkpoint_no_changes_returns_empty` | `ops::checkpoint()` on clean tree → `CheckpointOutcome { files_changed: vec![], commit: None }` |
| U-OPS-23 | `test_draft_commit_materializes_then_commits` | `ops::draft_commit()` → files written, staged, committed in single operation |
| U-OPS-24 | `test_diff_check_returns_summary` | `ops::diff_check()` → correct files_added/modified/deleted counts |
| U-OPS-25 | `test_status_returns_dirty_files` | `ops::status()` with modified/untracked files → correct `StatusEntry` list |
| U-OPS-26 | `test_init_creates_repo` | `ops::init()` → valid git repo at path |
| U-OPS-27 | `test_init_bare_creates_bare_repo` | `ops::init(bare=true)` → bare repo (no working tree) |
| U-OPS-28 | `test_checkout_branch_switches_head` | `ops::checkout_branch()` → HEAD moves to target branch, working tree updated |
| U-OPS-29 | `test_checkout_branch_nonexistent_errors` | `ops::checkout_branch()` with nonexistent branch → error |
| U-OPS-30 | `test_open_read_only_returns_repo` | `ops::open_read_only()` → valid repo handle, write ops fail or are prevented |
| U-OPS-31 | `test_current_branch_returns_name` | `ops::current_branch()` → branch name string matching current HEAD |
| U-OPS-32 | `test_configure_safe_directory_sets_config` | `ops::configure_safe_directory()` → `safe.directory` git config entry added |
| U-OPS-33 | `test_collect_commit_push_stages_commits_pushes` | `ops::collect_commit_push()` → stages all, commits, pushes to remote in single call |
| U-OPS-34 | `test_merge_workspace_merges_all` | `ops::merge_workspace()` with 2 repos → both merged, returns `Vec<MergeResult>` with FastForwarded/Rebased entries |
| U-OPS-35 | `test_merge_workspace_conflict_reports` | `ops::merge_workspace()` where repo B conflicts → returns `MergeResult::Conflict` for repo B, no ref advancement for repo A (pure git, no DB) |
| U-OPS-36 | `test_merge_workspace_single_repo_ff` | `ops::merge_workspace()` with 1 repo, fast-forward case → returns single `MergeResult::FastForwarded`, linear history |

**Test fixtures:** Each test creates a `tempfile::TempDir`, initialises
a git repo with `git2::Repository::init`, adds seed files, and commits.
For merge tests, create a second branch with divergent commits.

---

#### TASK-02: Fix CRDT Inbound Materialisation Bug (CR-1)

**Implementation:**
Replace `tokio::fs::write(&full_path, &content)` at `g5d/src/crdt/mod.rs`
line 237 with `Materializer::write_file_atomic(&full_path, &content)`.

**Unit tests** (`g5d/src/crdt/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-CRDT-01 | `test_inbound_delta_uses_atomic_write` | Mock `Materializer`, apply inbound delta → `write_file_atomic` called (not `tokio::fs::write`) |
| U-CRDT-02 | `test_inbound_delta_partial_write_no_corruption` | Kill the write mid-stream (simulate crash) → original file untouched (atomic rename semantics) |
| U-CRDT-03 | `test_inbound_delta_concurrent_with_git_add` | Race: apply delta while `git add` runs → no partial file content in index |

---

#### TASK-03: Refactor CLI Commands to Use `ops::`

**Implementation:**
Refactor each CLI command (§9.5.1, callsites C-1 through C-22) to call
`ops::*` functions instead of raw `git2::Repository` calls. The CLI
commands become thin wrappers: parse args → resolve workspace path →
call `ops::*` → format output.

**Files modified:**
- `g5d/src/cli/commit.rs` — use `ops::commit()`
- `g5d/src/cli/push.rs` — use `ops::push()`
- `g5d/src/cli/clone.rs` — use `ops::clone()`, `ops::init()`, `ops::commit()`
- `g5d/src/cli/fork.rs` — use `ops::clone()`, `ops::init()`, `ops::commit()`
- `g5d/src/cli/add.rs` — use `ops::clone()`, `ops::checkout_branch()`
- `g5d/src/cli/refresh.rs` — use `ops::fetch()`, `ops::clone()`
- `g5d/src/cli/revert.rs` — use `ops::reset_hard()`
- `g5d/src/cli/init.rs` — use `ops::init()`, `ops::clone()`, `ops::commit()`
- `g5d/src/cli/mod.rs` — delete `commit_subdir()` (replaced by `ops::commit()`)

**Unit tests** (`g5d/src/cli/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-CLI-01 | `test_commit_calls_ops_commit_per_repo` | `cli::commit::execute()` with 2-repo workspace → `ops::commit()` called twice |
| U-CLI-02 | `test_commit_uses_configured_author` | Author from workspace.yaml used, not hardcoded `g5d@local` |
| U-CLI-03 | `test_push_calls_ops_push_with_auth` | `cli::push::execute()` → `ops::push()` called with `GitAuth::Token(...)` from credentials.yaml |
| U-CLI-04 | `test_push_no_credentials_errors` | Push without credentials.yaml → meaningful error message |
| U-CLI-05 | `test_clone_calls_ops_clone` | `cli::clone::execute(url)` → `ops::clone()` called with parsed URL |
| U-CLI-06 | `test_fork_calls_ops_clone_with_cow` | `cli::fork::execute()` → `ops::clone()` with COW option |
| U-CLI-07 | `test_revert_calls_ops_reset_hard` | `cli::revert::execute()` → `ops::reset_hard()` called |
| U-CLI-08 | `test_revert_blocks_on_dirty_drafts` | `cli::revert::execute()` with dirty CRDT → `G5Error::DraftDirty` |
| U-CLI-09 | `test_refresh_fetches_all_repos` | `cli::refresh::execute()` → `ops::fetch()` called for each repo in manifest |
| U-CLI-10 | `test_commit_subdir_removed` | `commit_subdir` function no longer exists in `cli/mod.rs` (compile-time verification) |

---

#### TASK-04: Refactor Worker Lifecycle to Use `ops::`

**Implementation:**
Refactor worker callsites (§9.5.2, W-1 through W-9) to call `ops::*`
instead of inline `GitRepo` sequences.

**Files modified:**
- `g5d/src/worker/mod.rs` — use `ops::setup_task_branch()`, `ops::merge()`
- `g5d/src/worker/lifecycle.rs` — use `ops::checkpoint()`, `ops::setup_task_branch()`, `ops::checkpoint()`
- `g5d/src/tools/task_lifecycle.rs` — use `ops::checkpoint()`, `ops::push()`
- `g5d/src/tools/git_ops.rs` — use `ops::commit()`, `ops::push()`, `ops::fetch()`

**Unit tests** (`g5d/src/worker/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-WRK-01 | `test_create_task_workspace_calls_ops_setup_task_branch` | CoW path → `ops::setup_task_branch()` called with correct branch/commit |
| U-WRK-02 | `test_create_task_workspace_reuse_calls_ops_setup_task_branch` | Reuse path → same `ops::setup_task_branch()` call |
| U-WRK-03 | `test_mirror_back_calls_ops_merge` | Post-task → `ops::merge()` called with cow_path for fetch_local |
| U-WRK-04 | `test_checkpoint_calls_ops_checkpoint` | `checkpoint()` → `ops::checkpoint()` called, returns files_changed |
| U-WRK-05 | `test_phase_clone_patch_calls_ops_setup_task_branch` | `phase_clone_patch()` → single `ops::setup_task_branch()` replaces 70 lines of inline git |
| U-WRK-06 | `test_phase_collect_calls_ops_checkpoint` | `phase_collect()` → `ops::checkpoint()` called |
| U-WRK-07 | `test_git_commit_tool_calls_ops_commit` | `GitCommitTool::execute()` → `ops::commit()` |
| U-WRK-08 | `test_git_push_tool_calls_ops_push` | `GitPushTool::execute()` → `ops::push()` |
| U-WRK-09 | `test_git_fetch_tool_calls_ops_fetch` | `GitFetchTool::execute()` → `ops::fetch()` |

---

#### TASK-05: Refactor API Services to Use `ops::`

**Implementation:**
Refactor API service callsites (§9.5.4, A-1 through A-9) to call `ops::*`
instead of inline `GitRepo` sequences.

**Files modified:**
- `g5-api/src/services/draft_service.rs` — use `ops::diff_check()`, `ops::draft_commit()`
- `g5-api/src/services/task_service.rs` — use `ops::merge()`
- `g5-api/src/services/workspace_service.rs` — use `ops::merge()`, `ops::resolve_head()`
- `g5-api/src/services/job_service.rs` — use `ops::create_branch()`, `ops::merge()`
- `g5-api/src/services/ontology_service.rs` — use `ops::diff_check()` (replace subprocess)
- `g5-api/src/services/comment_service.rs` — use `ops::open_read_only()` (minor)

**Integration tests** (`g5-api/tests/`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-API-01 | `test_draft_commit_uses_ops_draft_commit` | `commit_draft()` → atomic lock/checkout/materialize/add/commit via `ops::draft_commit()` |
| I-API-02 | `test_draft_commit_concurrent_rejects_second` | Two concurrent `commit_draft()` on same branch → first succeeds, second gets `409 Conflict` |
| I-API-03 | `test_job_completion_auto_merge_via_ops` | Complete final task → `ops::merge()` fast-forwards job branch into base |
| I-API-04 | `test_job_completion_diverged_base_rebases` | Complete task where base has advanced → `ops::merge()` rebases, `MergeOutcome::Rebased` |
| I-API-05 | `test_job_rollback_via_ops_merge` | `rollback_job()` → `ops::merge()` moves base branch back to fork point |
| I-API-06 | `test_workspace_merge_via_ops_merge` | `try_fast_forward_merge()` → delegates to `ops::merge()` |
| I-API-07 | `test_job_create_branch_via_ops` | `create_job()` → `ops::create_branch()` called, branch exists in repo |
| I-API-08 | `test_job_create_branch_db_fail_no_orphan` | Simulate DB failure after `ops::create_branch()` → branch cleaned up |
| I-API-09 | `test_ontology_diff_via_ops_not_subprocess` | `get_node_diff()` → no `Command::new("git")` spawn, uses `ops::diff_check()` |

---

#### TASK-06: Helm — Remove NFS Mount from Workers

**Implementation:**
Add `worker.filestore.enabled` flag in Helm chart. Default `false`.
Workers in Kata mode get no NFS PVC mount.

**Test cases:**

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-HELM-01 | `test_worker_deployment_no_nfs_default` | `helm template` with defaults → worker deployment has no PVC or volumeMount for filestore |
| U-HELM-02 | `test_worker_deployment_nfs_when_enabled` | `helm template --set worker.filestore.enabled=true` → PVC mount present |
| U-HELM-03 | `test_api_always_has_nfs` | `helm template` → API deployment always has filestore PVC |
| U-HELM-04 | `test_values_dev_has_nfs_enabled` | `values-dev.yaml` has `worker.filestore.enabled: true` |
| U-HELM-05 | `test_values_prod_has_nfs_disabled` | `values-prod.yaml` has `worker.filestore.enabled: false` |

---

#### TASK-07: Scoped Worker Registration Tokens (§3)

**Implementation:**
1. Migration: create `worker_tokens` table
2. Endpoint: `POST /api/v1/orgs/:org/worker-tokens` (requires `WorkersAdmin` scope)
3. Endpoint: `GET /api/v1/orgs/:org/worker-tokens` (requires `WorkersRead` scope)
4. Endpoint: `DELETE /api/v1/orgs/:org/worker-tokens/:id` (requires `WorkersAdmin` scope)
5. Modify `register_worker()` to validate token scope
6. Modify `poll_for_task()` to filter by token scope

**Integration tests** (`g5-api/tests/api_internal.rs`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-TKN-01 | `test_create_org_scoped_worker_token` | Create token with `scope=org` → token returned with prefix `g5w_`, org_id set |
| I-TKN-02 | `test_create_workspace_scoped_worker_token` | Create token with `scope=workspace` + workspace_id → workspace_id set |
| I-TKN-03 | `test_create_user_scoped_worker_token` | Create token with `scope=user` → user_id set to caller |
| I-TKN-04 | `test_list_worker_tokens` | Create 3 tokens → list returns 3 (key prefix visible, full key NOT returned) |
| I-TKN-05 | `test_revoke_worker_token` | Create + revoke → `revoked_at` set, token no longer validates |
| I-TKN-06 | `test_register_with_org_token` | Register worker with org-scoped token → worker's `org_id` matches token |
| I-TKN-07 | `test_register_with_workspace_token` | Register with workspace token → worker can only poll from that workspace |
| I-TKN-08 | `test_register_with_expired_token_rejected` | Token past `expires_at` → 401 on register |
| I-TKN-09 | `test_register_with_revoked_token_rejected` | Revoked token → 401 on register |
| I-TKN-10 | `test_poll_org_scoped_sees_all_workspaces` | Org token worker → polls tasks from any workspace in the org |
| I-TKN-11 | `test_poll_workspace_scoped_filters` | Workspace token worker → only sees tasks from that workspace |
| I-TKN-12 | `test_poll_user_scoped_filters` | User token worker → only sees tasks from workspaces the user can access |
| I-TKN-13 | `test_cross_org_token_rejected` | Org-A token → register with org_id=B → 403 |
| I-TKN-14 | `test_token_requires_workers_admin_scope` | User without `WorkersAdmin` → 403 on create token |

---

#### TASK-08: Worker Secrets Bucket (§3.4)

**Implementation:**
1. Add `TaskSecrets` struct to `g5-common/src/internal_api/dtos.rs`
2. Modify `poll_for_task()` to populate `TaskAssignment.secrets`
3. LLM key resolution: lookup from org settings, not worker env var
4. Worker consumes `secrets.llm` → sets env var for task subprocess

**Unit tests:**

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-SEC-01 | `test_task_secrets_serialization_roundtrip` | `TaskSecrets` serializes/deserializes correctly, `SecretString` redacted in Debug |
| U-SEC-02 | `test_task_secrets_scm_empty_default` | Default `TaskSecrets` has empty `scm` vec |
| U-SEC-03 | `test_secret_string_display_redacted` | `SecretString` Display/Debug → `[REDACTED]` |

**Integration tests:**

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-SEC-01 | `test_poll_returns_llm_key_in_secrets` | Configure org LLM key → poll → `assignment.secrets.llm.api_key` present |
| I-SEC-02 | `test_poll_returns_empty_scm_in_mirror_mode` | Default config → poll → `assignment.secrets.scm` is empty vec |
| I-SEC-03 | `test_secrets_not_in_task_status_response` | `GET /api/v1/tasks/:id` does NOT include secrets (skip_serializing_if) |

---

#### TASK-09: Chat Task Priority (§4)

**Implementation:**
1. Set `priority = 100` in `create_chat_job()` / `create_chat_task()`
2. Update `poll_for_task()` ORDER BY: `priority DESC, created_at ASC`

**Integration tests** (`g5-api/tests/api_tasks.rs`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-PRI-01 | `test_chat_task_polled_before_uplift` | Create uplift task (priority=0), then chat task (priority=100) → poll returns chat first |
| I-PRI-02 | `test_same_priority_fifo` | Create 3 uplift tasks → polled in creation order (FIFO within priority) |
| I-PRI-03 | `test_chat_priority_with_empty_queue` | Chat task only → polled immediately (baseline latency < 100ms in test) |

---

#### TASK-10: Task Lease and TTL (§13)

**Implementation:**
1. Migration: add `lease_expires_at`, `lease_renewals`, `max_duration_secs`
   columns to `tasks` table
2. Modify `poll_for_task()` to set `lease_expires_at` and `max_duration_secs`
3. Modify `worker_heartbeat()` to extend `lease_expires_at`
4. Create `maintenance/lease_reaper.rs` — background sweep
5. Modify `transition_task()` to reject completion from non-owning worker

**Integration tests** (`g5-api/tests/api_tasks.rs`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-LSE-01 | `test_poll_sets_lease_expires_at` | Poll → task has `lease_expires_at` set, ~90s from now |
| I-LSE-02 | `test_heartbeat_extends_lease` | Poll → heartbeat → `lease_expires_at` advanced, `lease_renewals` incremented |
| I-LSE-03 | `test_reaper_reclaims_expired_lease` | Poll → manually set `lease_expires_at` to past → run reaper → task back to `pending`, `retry_count` incremented |
| I-LSE-04 | `test_reaper_fails_max_duration_exceeded` | Poll → manually set `assigned_at` to 31 min ago, `max_duration_secs = 1800` → reaper → task `failed` |
| I-LSE-05 | `test_reaper_ignores_non_leased_tasks` | Legacy task with `lease_expires_at = NULL` → reaper ignores it |
| I-LSE-06 | `test_reaper_marks_dead_workers_terminated` | Worker with `last_heartbeat` 4 min ago → reaper → worker status `terminated` |
| I-LSE-07 | `test_completion_after_reclamation_rejected` | Poll → reaper reclaims → original worker tries `transition_task(completed)` → 403 |
| I-LSE-08 | `test_reclaimed_task_reassigned_to_new_worker` | Reclaimed task → second worker polls → gets the reclaimed task |
| I-LSE-09 | `test_max_retries_exceeded_fails_permanently` | Task reclaimed N times (exceeds max_retries) → not returned to pending, marked failed |
| I-LSE-10 | `test_lease_ttl_per_task_type` | Chat task → lease TTL = 60s; uplift task → lease TTL = 90s |
| I-LSE-11 | `test_reaper_advisory_lock_prevents_double_run` | Two concurrent reaper calls → only one acquires `pg_try_advisory_lock`, other is no-op |

---

#### TASK-11: Git HTTPS Serving Protocol (§14)

**Implementation:**
1. Add routes: `GET /git/:repo_id/info/refs`, `POST /git/:repo_id/git-upload-pack`, `POST /git/:repo_id/git-receive-pack`
2. Implement `GitAuth` extractor for HTTP Basic with worker JWT
3. Subprocess spawning with `--stateless-rpc` and `env_clear()`
4. Pkt-line header generation

**Unit tests** (`g5-api/src/routes/git.rs` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-GIT-01 | `test_pkt_line_header_format` | `pkt_line_header("git-upload-pack")` → correct hex-length prefix + flush packet |
| U-GIT-02 | `test_pkt_line_header_receive_pack` | `pkt_line_header("git-receive-pack")` → correct format |
| U-GIT-03 | `test_git_auth_extracts_worker_token` | HTTP Basic `x-worker-token:<jwt>` → `GitAuth { worker_id, org_id }` |
| U-GIT-04 | `test_git_auth_extracts_user_token` | HTTP Basic `x-user-token:<jwt>` → `GitAuth { user_id, org_id }` |
| U-GIT-05 | `test_git_auth_rejects_invalid_jwt` | HTTP Basic `x-worker-token:garbage` → 401 |
| U-GIT-06 | `test_git_auth_rejects_missing_basic` | No Authorization header → 401 with `WWW-Authenticate: Basic` |

**Integration tests** (`g5-api/tests/api_git.rs` — new file):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-GIT-01 | `test_info_refs_upload_pack` | `GET /git/:repo_id/info/refs?service=git-upload-pack` → 200, content-type `application/x-git-upload-pack-advertisement`, body contains ref names |
| I-GIT-02 | `test_info_refs_receive_pack` | `GET /git/:repo_id/info/refs?service=git-receive-pack` → 200, correct content-type |
| I-GIT-03 | `test_upload_pack_clone` | `POST /git/:repo_id/git-upload-pack` with valid pack request → 200, packfile in response |
| I-GIT-04 | `test_receive_pack_push` | `POST /git/:repo_id/git-receive-pack` with pack data → 200, ref updated in bare repo |
| I-GIT-05 | `test_upload_pack_wrong_repo_id_404` | Non-existent repo_id → 404 |
| I-GIT-06 | `test_receive_pack_no_active_task_403` | Worker with no running task → `git-receive-pack` → 403 |
| I-GIT-07 | `test_receive_pack_wrong_repo_403` | Worker with task on repo A → push to repo B → 403 |
| I-GIT-08 | `test_upload_pack_cross_org_403` | Worker in org A → clone repo in org B → 403 |
| I-GIT-09 | `test_env_clear_no_http_vars` | Subprocess spawned with `env_clear()` → env contains only `PATH`, `GIT_DIR`, and optionally `GIT_SSL_CAINFO`/`SSL_CERT_DIR`; no `HTTP_*` |
| I-GIT-10 | `test_git_clone_via_endpoint` | Full round-trip: init bare repo with commits → `git clone https://localhost:PORT/git/REPO_ID` via subprocess → cloned repo matches |
| I-GIT-11 | `test_git_push_via_endpoint` | Full round-trip: clone → make commit → `git push` → bare repo updated |
| I-GIT-12 | `test_concurrent_clones` | 3 parallel `git clone` requests → all succeed |

---

#### TASK-12: Push Validation — Inline Pre/Post Checks (§14.4)

**Implementation:**
1. Parse pkt-line ref updates in `receive_pack` handler before subprocess
2. Add inline pre-push validation: draft guard, ancestry check, auth re-check
3. Add inline post-push side-effects: `advance_head()`, WS events, reindex
4. Support `X-G5-Force-Push` header for force-push (skip ancestry check)

**Integration tests** (`g5-api/tests/api_git.rs`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-HK-01 | `test_receive_pack_accepts_valid_push` | Push with valid fast-forward → handler passes, ref updated |
| I-HK-02 | `test_receive_pack_rejects_non_descendant` | Push non-fast-forward → handler rejects before subprocess, ref unchanged |
| I-HK-03 | `test_receive_pack_rejects_dirty_draft` | Set up dirty draft on branch → push → handler rejects with error "dirty draft" |
| I-HK-04 | `test_receive_pack_rejects_expired_task` | Task lease expired → push → handler rejects |
| I-HK-05 | `test_receive_pack_updates_task_head_commit` | Successful push → `tasks.head_commit` updated in DB |
| I-HK-06 | `test_receive_pack_emits_ws_event` | Push → WebSocket subscribers receive `branch.advanced` event |
| I-HK-07 | `test_receive_pack_triggers_reindex` | Push → reindex job enqueued for the updated branch |

---

#### TASK-13: Worker Git Configuration (§14.5)

**Implementation:**
1. Modify `workspace_git_url` in `poll_for_task()` to return HTTPS URL
2. Worker uses `git_command_with_auth()` helper (§14.5) to pass
   registration JWT via `http.extraHeader` env vars on each git command
3. Worker clones via `git clone --depth=1 --branch <branch> <url>`

**Unit tests** (`g5d/src/worker/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-WGIT-01 | `test_workspace_git_url_is_https` | `TaskAssignment.workspace_git_url` starts with `https://` |
| U-WGIT-02 | `test_git_command_with_auth_sets_header` | `git_command_with_auth(token)` sets `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=http.extraHeader`, `GIT_CONFIG_VALUE_0=Authorization: Bearer <token>` |
| U-WGIT-03 | `test_git_command_auth_not_persisted` | After git subprocess exits, no `.gitconfig` or credential file exists in the task workspace |

---

#### TASK-14: URL-Based Clone and Instance Resolution (§2.5)

**Implementation:**
1. `parse_g5_url()` in `g5d/src/cli/mod.rs` — extracts api_url, org, workspace
2. `g5 clone <url>` stores `remote.api_url` in `workspace.yaml`
3. `g5 auth login <url>` stores token in `~/.g5/credentials.yaml`
4. URL normalisation (strip trailing slash, enforce https)

**Unit tests:**

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-URL-01 | `test_parse_g5_url_full` | `https://app.g5.dev/org/my-org/workspace/my-ws` → `{ api_url: "https://app.g5.dev", org: "my-org", workspace: "my-ws" }` |
| U-URL-02 | `test_parse_g5_url_trailing_slash` | `https://app.g5.dev/org/my-org/workspace/my-ws/` → same result (trailing slash stripped) |
| U-URL-03 | `test_parse_g5_url_http_rejected` | `http://...` → error (must be https) |
| U-URL-04 | `test_parse_g5_url_missing_workspace` | `https://app.g5.dev/org/my-org` → error (incomplete path) |
| U-URL-05 | `test_credential_lookup_matches_api_url` | Credentials for `https://app.g5.dev` → match workspace with `remote.api_url = "https://app.g5.dev"` |
| U-URL-06 | `test_credential_lookup_normalises_url` | Credentials stored for `https://app.g5.dev/` → matches `https://app.g5.dev` (normalised) |
| U-URL-07 | `test_credential_lookup_no_match` | No credentials for `https://other.g5.dev` → prompt for login |

---

### 17.2 Phase 2 — Should Do

---

#### TASK-15: GCS Snapshot Upload (§9.2)

**Implementation:**
After `repo_fetcher` fetch, create `tar.zst` of worktree at HEAD →
upload to `gs://g5-repos/{repo_id}/snapshots/{commit_sha}.tar.zst`.

**Integration tests:**

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-GCS-01 | `test_snapshot_upload_after_fetch` | `repo_fetcher` fetch → tarball exists in GCS (mock or emulator) |
| I-GCS-02 | `test_snapshot_content_matches_worktree` | Download + extract → file tree matches `git ls-tree HEAD` |
| I-GCS-03 | `test_snapshot_dedup_same_commit` | Two fetches at same commit → single snapshot (no duplicate upload) |

---

#### TASK-16: Worker GCS Snapshot Checkout Path

**Integration tests:**

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-GCSW-01 | `test_worker_downloads_snapshot` | Worker phase_prepare with `CheckoutMode::GcsSnapshot` → tarball downloaded + extracted |
| I-GCSW-02 | `test_worker_falls_back_to_clone` | Snapshot missing → falls back to `CheckoutMode::ShallowClone` |
| I-GCSW-03 | `test_snapshot_checkout_performance` | Snapshot checkout < shallow clone time (benchmark, not pass/fail) |

---

#### TASK-17: API Key / PAT Management (§9.4)

**Implementation:**
1. Migration: create `api_keys` table
2. `POST /api/v1/orgs/:org/api-keys` — create
3. `GET /api/v1/orgs/:org/api-keys` — list
4. `DELETE /api/v1/orgs/:org/api-keys/:id` — revoke
5. Auth middleware: detect `g5k_*` prefix → API key lookup

**Integration tests** (`g5-api/tests/api_auth.rs`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-KEY-01 | `test_create_api_key` | Create key → response has prefix (`g5k_...`), full key returned once |
| I-KEY-02 | `test_create_api_key_scoped` | Create key with `scopes: ["workspaces:read"]` → key only has that scope |
| I-KEY-03 | `test_list_api_keys_shows_prefix_not_full` | List → prefix visible, full key NOT in response |
| I-KEY-04 | `test_revoke_api_key` | Revoke → key returns 401 on subsequent use |
| I-KEY-05 | `test_expired_api_key_rejected` | Create key with `expires_at` in past → 401 |
| I-KEY-06 | `test_api_key_auth_on_workspace_endpoint` | Use API key with `WorkspacesRead` → `GET /workspaces` succeeds |
| I-KEY-07 | `test_api_key_scope_enforcement` | Key with `WorkspacesRead` only → `POST /workspaces` → 403 |
| I-KEY-08 | `test_api_key_last_used_updated` | Use key → `last_used_at` and `last_used_ip` updated |
| I-KEY-09 | `test_api_key_org_isolation` | Key in org A → access org B workspace → 403 |
| I-KEY-10 | `test_api_key_rls_scoped` | Key's org determines RLS context — only sees that org's data |

---

#### TASK-18: Third-Party Agent Conflict Detection (§10.2)

**Implementation:**

This task implements the detect-and-notify system for third-party agents
(Cursor, aider, Windsurf) that write files directly to the workspace,
bypassing the CRDT layer. Without this, inbound CRDT deltas silently
overwrite agent changes.

1. **Add `detect_local_modification()` to `Materializer`** — compare
   current on-disk file hash against `known_states`. If mtime changed
   and content hash differs, the file was modified by an external agent.

2. **Route inbound materialisation through `Materializer`** —
   **Prerequisite: TASK-02 must be completed first.** TASK-02 replaces
   `tokio::fs::write()` with `Materializer::write_file_atomic()`. This
   step builds on that fix by adding a `detect_local_modification()`
   call before the atomic write, so that external agent changes are
   detected before they are overwritten.

3. **Filesystem sidecars** — on conflict, copy the agent's file version
   to `.g5/conflicts/<relative-path>` with a `.meta` JSON file
   (timestamps, hashes, remote author). Any agent using `inotify`/
   `fswatch` sees the sidecar appear and can react.

4. **Local Unix socket notifications** — bind `.g5/crdt.sock` in
   `g5 connect`. Emit newline-delimited JSON events on conflict:
   `{ "type": "conflict", "path": "...", "local_sha256": "...",
   "remote_sha256": "...", "action": "overwritten" }`. MCP-aware agents
   connect and react programmatically.

5. **`g5 conflicts` CLI command** — list, clear, accept-local, or
   accept-remote for conflict sidecars.

6. **Cleanup** — sidecars are removed by `g5 connect` on the next
   `detect_fs_changes()` cycle with no conflicts, or manually via
   `g5 conflicts --clear`.

**Files created:**
- `g5d/src/crdt/conflict_detector.rs`
- `g5d/src/crdt/conflict_notifier.rs`
- `g5d/src/cli/conflicts.rs`

**Files modified:**
- `g5d/src/crdt/mod.rs` — inbound materialisation path
- `g5d/src/crdt/materialize.rs` — `detect_local_modification()`,
  `write_conflict_sidecar()`
- `g5d/src/cli/mod.rs` — register `conflicts` subcommand

**Unit tests** (`g5d/src/crdt/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-CFD-01 | `test_detect_local_modification_unchanged` | File not modified since last known state → `None` |
| U-CFD-02 | `test_detect_local_modification_content_changed` | File modified by external agent → `Some(FileState)` with new hash |
| U-CFD-03 | `test_detect_local_modification_mtime_only` | mtime changed but content identical (e.g. `touch`) → `None` |
| U-CFD-04 | `test_write_conflict_sidecar_creates_files` | On conflict → `.g5/conflicts/<path>` and `.g5/conflicts/<path>.meta` created |
| U-CFD-05 | `test_write_conflict_sidecar_meta_format` | `.meta` file contains valid JSON with `local_sha256`, `remote_sha256`, `timestamp` |
| U-CFD-06 | `test_inbound_delta_conflict_preserves_agent_version` | External modification + inbound delta → agent's file preserved as sidecar, remote version written to disk |
| U-CFD-07 | `test_inbound_delta_no_conflict_no_sidecar` | No external modification + inbound delta → no sidecar created |
| U-CFD-08 | `test_conflict_notifier_emits_json_event` | Connect to `.g5/crdt.sock` → conflict triggers → JSON event received |
| U-CFD-09 | `test_conflict_cleanup_removes_stale_sidecars` | No conflicts on next `detect_fs_changes()` → sidecars removed |

**Integration tests** (`g5d/tests/`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-CFD-01 | `test_agent_write_then_inbound_delta_creates_sidecar` | Write file externally → simulate inbound CRDT delta for same file → sidecar exists with agent's version |
| I-CFD-02 | `test_conflicts_list_shows_all_sidecars` | Create 3 conflict sidecars → `g5 conflicts` lists all 3 with paths and timestamps |
| I-CFD-03 | `test_conflicts_accept_local_enqueues_agent_version` | `g5 conflicts --accept-local src/handler.rs` → agent's version enqueued via `enqueue_file_write()`, sidecar removed |
| I-CFD-04 | `test_conflicts_accept_remote_removes_sidecar` | `g5 conflicts --accept-remote src/handler.rs` → sidecar removed, on-disk file unchanged (remote already written) |
| I-CFD-05 | `test_conflicts_clear_removes_all` | `g5 conflicts --clear` → all sidecars removed |
| I-CFD-06 | `test_unix_socket_multiple_clients` | Two clients connected to `.g5/crdt.sock` → both receive conflict event |

---

#### TASK-19: Implement `g5 merge` CLI and API Endpoint (§2.10 Op 8)

**Implementation:**

This task implements the workspace-to-workspace merge command and its
backing API endpoint.

1. **CLI command** — `g5d/src/cli/merge.rs`: parse `<workspace-ref>` +
   `--dry-run`, resolve source workspace via API, call
   `ops::merge_workspace()`, print per-repo summary.
2. **API endpoint** — `g5-api/src/services/workspace_service.rs`:
   `POST /api/v1/workspaces/:target_id/merge` wraps
   `ops::merge_workspace()` in a DB transaction via
   `service::merge_workspace_api()` (§10.5.6).
3. **Auth** — caller must have write on target + read on source.
   Cross-org merge allowed per §2.10 authorisation rules.
4. **Dry-run mode** — run `ops::merge_workspace()` without
   advancing refs or creating a workspace version; return the
   `Vec<MergeResult>` preview.
5. **Register BNF** — grammar already includes `<merge-cmd>` (§2.1.1).

**Files created:**
- `g5d/src/cli/merge.rs`

**Files modified:**
- `g5d/src/cli/mod.rs` — register `merge` subcommand
- `g5-api/src/services/workspace_service.rs` — add `merge_workspace()` handler
- `g5-api/src/routes.rs` — wire `POST /workspaces/:id/merge`

**Unit tests** (`g5d/src/cli/` — `#[cfg(test)]`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| U-MRG-01 | `test_merge_parse_workspace_name` | `g5 merge auth-refactor` → resolves `auth-refactor` as workspace name |
| U-MRG-02 | `test_merge_parse_workspace_id` | `g5 merge <uuid>` → resolves as workspace ID |
| U-MRG-03 | `test_merge_dry_run_flag` | `g5 merge auth-refactor --dry-run` → `dry_run = true` in request |

**Integration tests** (`g5-api/tests/`):

| Test ID | Test Name | What It Verifies |
|---|---|---|
| I-MRG-01 | `test_merge_workspace_fast_forward` | Source ahead of target → fast-forward, new workspace version created |
| I-MRG-02 | `test_merge_workspace_rebase` | Diverged repos → rebase, new workspace version created |
| I-MRG-03 | `test_merge_workspace_conflict_no_version` | Conflicting repos → 409, no workspace version created |
| I-MRG-04 | `test_merge_workspace_adds_source_only_repos` | Source has repos not in target → added to target manifest |
| I-MRG-05 | `test_merge_workspace_dry_run_no_side_effects` | `dry_run: true` → returns preview, no refs moved, no version created |
| I-MRG-06 | `test_merge_workspace_requires_write_on_target` | Caller without write on target → 403 |
| I-MRG-07 | `test_merge_workspace_requires_read_on_source` | Caller without read on source → 403 |

---

### 17.3 End-to-End Integration Scenarios

These E2E tests verify the full system working together. Each is a
multi-step scenario running against the Docker Compose test stack.
Tests use `TestApiClient` and real `git` commands.

**New file:** `e2e/src/scenarios/worker_storage_separation.rs`

---

#### E2E-01: Worker Clones via HTTPS, Not NFS

**Prerequisite:** Worker running without NFS mount.

```
1. api.create_org("e2e-https-clone")
2. api.create_workspace(org_id, "ws")
3. Push seed files to workspace repo via git serving endpoint
4. api.create_job(ws_id, "uplift", spec_id)
5. Worker polls → gets TaskAssignment
6. Assert: TaskAssignment.workspace_git_url starts with "https://"
7. Assert: TaskAssignment.workspace_git_url does NOT contain "/" as path prefix
8. Worker clones via git HTTPS endpoint
9. Assert: cloned repo has seed files
10. Worker makes changes, pushes via git HTTPS
11. Assert: bare repo on API has new commits
12. Worker transitions task to completed
13. Assert: job branch fast-forwarded to include worker commits
```

**Verifies:** §5 (no NFS), §14 (git HTTPS), §14.5 (worker git config)

---

#### E2E-02: Task Lease Expiry and Reclamation

```
1. api.create_org("e2e-lease")
2. api.create_workspace(org_id, "ws")
3. api.create_job(ws_id, "uplift", spec_id) with 2 tasks
4. Worker A registers, polls → gets task 1
5. Assert: task 1 has lease_expires_at set
6. Simulate worker A crash: stop sending heartbeats
7. Wait for lease to expire (use short TTL in test config, e.g. 5s)
8. Trigger reaper (or wait for automatic sweep)
9. Assert: task 1 status = 'pending', retry_count = 1
10. Worker B registers, polls → gets task 1 (reclaimed)
11. Assert: task 1 worker_id = worker B
12. Worker A attempts transition_task(completed) → 403
13. Worker B completes task 1 successfully
14. Assert: task 1 status = 'completed', worker_id = worker B
```

**Verifies:** §13 (task lease), §13 (reaper), §13 (race condition)

---

#### E2E-03: Scoped Worker Token — Org Isolation

```
1. api.create_org("e2e-org-a")
2. api.create_org("e2e-org-b")
3. Create workspace + job in each org
4. Create org-scoped worker token for org A
5. Register worker with org A token
6. Worker polls → gets task from org A only
7. Assert: no task from org B returned
8. Attempt to register second worker with org A token but org_id = org B → 403
```

**Verifies:** §3 (token scoping), §3.2 (cross-org rejection)

---

#### E2E-04: Scoped Worker Token — Workspace Isolation

```
1. api.create_org("e2e-ws-scope")
2. Create workspace A and workspace B in the org
3. Create jobs in both workspaces
4. Create workspace-scoped token for workspace A
5. Register worker with workspace-A token
6. Worker polls → gets task from workspace A only
7. Assert: no task from workspace B returned
8. Worker completes task
9. Poll again → empty (no more tasks in workspace A)
```

**Verifies:** §3 (workspace scope)

---

#### E2E-05: Chat Task Priority

```
1. api.create_org("e2e-priority")
2. api.create_workspace(org_id, "ws")
3. Create uplift job with 3 tasks (priority 0)
4. Create chat job with 1 task (priority 100)
5. Register worker
6. Worker polls → gets chat task (priority 100), NOT uplift tasks
7. Worker completes chat task
8. Worker polls → gets first uplift task (priority 0, FIFO)
```

**Verifies:** §4 (chat priority)

---

#### E2E-06: Worker Secrets Delivery

```
1. api.create_org("e2e-secrets")
2. Configure org LLM key (Anthropic)
3. api.create_workspace(org_id, "ws")
4. api.create_job(ws_id, "uplift", spec_id)
5. Register worker
6. Worker polls → TaskAssignment
7. Assert: assignment.secrets.llm.provider == "anthropic"
8. Assert: assignment.secrets.llm.api_key is non-empty
9. Assert: assignment.secrets.scm is empty vec (mirror mode)
10. Worker completes task
11. Assert: LLM key is no longer in worker env (cleared)
```

**Verifies:** §3.4 (secrets bucket)

---

#### E2E-07: Push Validation — Draft Guard

```
1. api.create_org("e2e-draft-guard")
2. api.create_workspace(org_id, "ws")
3. Create a dirty draft on the workspace branch (upload_file via API)
4. api.create_job(ws_id, "uplift", spec_id)
5. Worker polls, clones via HTTPS
6. Worker makes changes, attempts `git push`
7. Assert: push REJECTED by receive_pack handler ("dirty draft on branch")
8. Commit the draft (api.commit_draft)
9. Worker retries `git push`
10. Assert: push succeeds
```

**Verifies:** §14.4 (draft guard), §2.10.1 (DraftState)

---

#### E2E-08: Push Validation — Stale Push Rejection

```
1. api.create_org("e2e-stale-push")
2. api.create_workspace(org_id, "ws")
3. api.create_job(ws_id, "uplift", spec_id)
4. Worker A polls, clones
5. Meanwhile, another commit is pushed to the base branch
   (simulate by API-side ops::commit)
6. Worker A (based on old HEAD) attempts push
7. Assert: push REJECTED ("not descended from current tip")
8. Worker A fetches, rebases, pushes again
9. Assert: push succeeds
```

**Verifies:** §14.4 (stale-push rejection), §10.5 (rebase-only flow)

---

#### E2E-09: Full Ops Unification — Job Lifecycle

This is the most comprehensive E2E test. It exercises the full
job lifecycle through the unified `ops::` layer:

```
1. api.create_org("e2e-ops-unified")
2. api.create_workspace(org_id, "ws") with seed repo
3. api.create_job(ws_id, "uplift", spec_id) with 3 tasks (T1→T2→T3)

Task T1 (independent):
4. Worker polls → gets T1
5. Worker clones via HTTPS (ops::clone)
6. Worker creates task branch (ops::setup_task_branch)
7. Worker modifies file A, checkpoints (ops::checkpoint)
8. Worker commits final (ops::commit)
9. Worker pushes via HTTPS (ops::push)
10. Worker transitions T1 → completed
11. Assert: T1 commits visible on job branch (ops::merge task→job)

Task T2 (depends on T1):
12. Worker polls → gets T2 (T1 completed, dependency satisfied)
13. Worker clones, sees T1's changes in workspace
14. Worker modifies file B
15. Worker pushes, completes T2
16. Assert: T2 commits merged into job branch

Task T3 (depends on T2):
17. Worker polls → gets T3
18. Worker modifies file C
19. Worker pushes, completes T3
20. Assert: T3 commits merged into job branch

Job completion:
21. Assert: all 3 tasks completed
22. Assert: job branch auto-merged into workspace base (ops::merge job→base)
23. Assert: workspace base branch has changes from T1, T2, T3
24. Assert: no merge commits — all fast-forwards or rebases
25. Assert: git log on base branch is linear
```

**Verifies:** §2.10 (`ops::merge` unified primitive), §10.5 (job
lifecycle phases), §14 (git HTTPS transport), §13 (lease maintained
throughout)

---

#### E2E-10: Cross-Workspace Merge

```
1. api.create_org("e2e-cross-merge")
2. Create workspace A (source) with 2 repos (shared-lib, app)
3. Create workspace B (target) with same 2 repos at same initial commit
4. Make changes in workspace A's repos (via job + worker)
5. api.merge_workspace(source=A, target=B)
6. Assert: workspace B repos have changes from A
7. Assert: merge was fast-forward or rebase (no merge commits)
8. Assert: merge per-repo — shared-lib merged independently of app
```

**Verifies:** §2.10 (`ops::merge_workspace`), Op 8 MERGE

---

#### E2E-11: Worker NFS Isolation Verification

```
1. Deploy worker pod WITHOUT filestore PVC
2. Register worker
3. Worker polls → gets task
4. Assert: worker cannot access /mnt/filestore (mount doesn't exist)
5. Worker clones via HTTPS successfully (no NFS needed)
6. Worker pushes via HTTPS successfully
7. Worker completes task
```

**Verifies:** §5 (NFS removed), §14 (HTTPS replaces NFS)

---

#### E2E-12: API Key Authentication (Phase 2)

```
1. api.create_org("e2e-api-key")
2. api.create_workspace(org_id, "ws")
3. Create API key with scopes: ["workspaces:read", "jobs:read"]
4. Use API key to GET /workspaces → 200
5. Use API key to POST /workspaces → 403 (no workspaces:write)
6. Use API key to GET /jobs → 200
7. Revoke API key
8. Use revoked key to GET /workspaces → 401
```

**Verifies:** §9.4 (API key management)

---

#### E2E-13: Task Max Duration Timeout

```
1. api.create_org("e2e-max-duration")
2. api.create_workspace(org_id, "ws")
3. Create job with task_type that has max_duration = 10s (test config)
4. Worker polls → gets task
5. Worker sends heartbeats (lease renewed) but never completes
6. Wait 11 seconds
7. Trigger reaper
8. Assert: task status = 'failed' (max_duration exceeded, despite valid lease)
9. Assert: job status updated accordingly
```

**Verifies:** §13 (max_duration TTL separate from lease TTL)

---

#### E2E-14: Concurrent Job Completions — No Race

```
1. api.create_org("e2e-concurrent-merge")
2. api.create_workspace(org_id, "ws")
3. Create 2 independent jobs (job A with 1 task, job B with 1 task)
4. Worker A completes job A's task → auto-merge to base
5. Worker B completes job B's task → auto-merge to base
6. Assert: base branch has commits from BOTH jobs
7. Assert: no merge conflicts (independent changes)
8. Assert: git log is linear (rebase if needed)
```

**Verifies:** §10.5 (concurrent job completion), `ops::merge` under
contention

---

#### E2E-15: Third-Party Agent Conflict Detection

```
1. api.create_org("e2e-agent-conflict")
2. api.create_workspace(org_id, "ws") with seed repo
3. Start `g5 connect` on workspace (background daemon)
4. Simulate agent: write "agent version" to src/handler.rs (raw fs write)
5. Simulate browser user: edit src/handler.rs via CRDT (inbound delta)
6. Assert: .g5/conflicts/src/handler.rs exists with agent's version
7. Assert: .g5/conflicts/src/handler.rs.meta has correct hashes
8. Assert: src/handler.rs on disk has the browser user's (remote) version
9. Run `g5 conflicts` → lists src/handler.rs
10. Run `g5 conflicts --accept-local src/handler.rs`
11. Assert: agent's version re-enqueued via Yjs
12. Assert: sidecar removed
```

**Verifies:** §10.2 (conflict detection), §10.2.4 (sidecar + socket
notification), §10.2.5 (g5 conflicts CLI)

---

### 17.4 Frontend Test Cases

These tests verify the UI components that interact with the new backend
features. Run with `npm test` in the `generation/client/` directory.

#### FE-01: Worker Token Management UI

**File:** `client/src/pages/WorkerTokensPage.test.tsx` (new)

| Test ID | Test Name | What It Verifies |
|---|---|---|
| FE-TKN-01 | `renders_token_list` | MSW mock returns 3 tokens → table shows 3 rows with prefix, scope, created_at |
| FE-TKN-02 | `create_token_dialog` | Click "Create Token" → dialog with scope dropdown (org/workspace/user) |
| FE-TKN-03 | `shows_token_once_after_create` | Create flow → full token shown in dialog with copy button, dismissed = gone |
| FE-TKN-04 | `revoke_token_confirmation` | Click revoke → confirmation dialog → API call → token disappears from list |
| FE-TKN-05 | `empty_state` | No tokens → "No worker tokens" message with create CTA |

#### FE-02: Task Lease Status UI

**File:** `client/src/components/execution/TaskLeaseIndicator.test.tsx` (new)

| Test ID | Test Name | What It Verifies |
|---|---|---|
| FE-LSE-01 | `shows_lease_remaining_time` | Task with `lease_expires_at` 45s from now → "45s remaining" |
| FE-LSE-02 | `shows_expired_warning` | Task with `lease_expires_at` in past → red "Lease expired" badge |
| FE-LSE-03 | `shows_max_duration_progress` | Task 15m into 30m `max_duration` → 50% progress bar |
| FE-LSE-04 | `updates_via_websocket` | WS event updates `lease_expires_at` → UI refreshes without page reload |

#### FE-03: API Key Management UI

**File:** `client/src/pages/ApiKeysPage.test.tsx` (new)

| Test ID | Test Name | What It Verifies |
|---|---|---|
| FE-KEY-01 | `renders_key_list_with_scopes` | Keys shown with scope badges, last_used_at |
| FE-KEY-02 | `create_key_with_scope_selector` | Create flow → multi-select scope picker → API call |
| FE-KEY-03 | `copy_key_to_clipboard` | After create → copy button → `navigator.clipboard.writeText` called |
| FE-KEY-04 | `revoke_key_removes_from_list` | Revoke → optimistic removal from list |

#### FE-04: Chat Priority Indicator

**File:** `client/src/components/execution/ChatPriorityBadge.test.tsx` (new)

| Test ID | Test Name | What It Verifies |
|---|---|---|
| FE-PRI-01 | `shows_priority_badge_on_chat_tasks` | Chat task → "Priority" badge |
| FE-PRI-02 | `no_badge_on_regular_tasks` | Uplift task → no priority badge |

---

### 17.5 Test Infrastructure Changes

To support the tests above, the following test infrastructure
additions are needed:

#### 17.5.1 New Test Helpers

| Helper | Location | Purpose |
|---|---|---|
| `TestApiClient::create_worker_token(org_id, scope, workspace_id?)` | `e2e/src/helpers/api_client.rs` | Create scoped worker registration token |
| `TestApiClient::revoke_worker_token(org_id, token_id)` | `e2e/src/helpers/api_client.rs` | Revoke a worker token |
| `TestApiClient::create_api_key(org_id, name, scopes)` | `e2e/src/helpers/api_client.rs` | Create API key |
| `TestApiClient::revoke_api_key(org_id, key_id)` | `e2e/src/helpers/api_client.rs` | Revoke API key |
| `TestApiClient::git_clone(repo_id, branch)` | `e2e/src/helpers/api_client.rs` | Clone via git HTTPS endpoint using subprocess `git clone` |
| `TestApiClient::git_push(repo_path, branch)` | `e2e/src/helpers/api_client.rs` | Push via git HTTPS endpoint using subprocess `git push` |
| `TestApiClient::trigger_reaper()` | `e2e/src/helpers/api_client.rs` | Manually trigger lease reaper sweep |
| `RepoFixture::with_seed_files(files)` | `g5d/tests/helpers/fixtures.rs` | Create fixture repo with specific file contents |
| `MockRepoStorage` | `g5-api/tests/helpers/mod.rs` | In-memory `RepoStorage` for integration tests without real filesystem |
| `TestApp::with_lease_config(chat_ttl, batch_ttl)` | `g5-api/tests/helpers/mod.rs` | Override lease TTL for testing (shorter values) |
| `TestApp::with_git_serving()` | `g5-api/tests/helpers/mod.rs` | Start test server with git serving routes enabled + bare repo setup |

#### 17.5.2 Test Configuration Overrides

| Config Key | Default (Test) | Purpose |
|---|---|---|
| `G5_LEASE_TTL_CHAT` | `5` (seconds, test only) | Fast lease expiry for lease tests |
| `G5_LEASE_TTL_BATCH` | `10` (seconds, test only) | Fast lease expiry for batch tests |
| `G5_MAX_DURATION_CHAT` | `10` (seconds, test only) | Fast max_duration for timeout tests |
| `G5_REAPER_INTERVAL` | `2` (seconds, test only) | Fast reaper sweep for E2E tests |
| `G5_GIT_SERVING_ENABLED` | `true` | Enable git HTTPS endpoints |

#### 17.5.3 CI Pipeline Changes

Add new test phases to `cloudbuild-ci.yaml`:

```yaml
# Phase: ops module unit tests (parallel with other unit tests)
- id: test-ops
  name: rust-build
  args: [cargo, test, -p, g5-common, --lib, --, ops]

# Phase: git serving integration tests (after postgres + migrations)
- id: test-git-serving
  name: rust-build
  args: [cargo, test, -p, g5-api, --test, api_git]
  waitFor: [migrations]

# Phase: lease integration tests
- id: test-lease
  name: rust-build
  args: [cargo, test, -p, g5-api, --test, api_tasks, --, lease]
  waitFor: [migrations]

# Phase: worker token integration tests
- id: test-worker-tokens
  name: rust-build
  args: [cargo, test, -p, g5-api, --test, api_internal, --, token]
  waitFor: [migrations]

# Phase: E2E worker-storage-separation scenarios
- id: e2e-storage-separation
  name: rust-build
  args: [cargo, test, -p, g5-e2e, --, worker_storage_separation]
  waitFor: [docker-compose-up]
```

#### 17.5.4 Justfile Recipes

```just
# Run only ops module tests
test-ops:
    cargo test -p g5-common --lib -- ops

# Run only git serving tests
test-git-serving:
    cargo test -p g5-api --test api_git -- --test-threads=1

# Run only lease tests
test-lease:
    cargo test -p g5-api --test api_tasks -- lease --test-threads=1

# Run only worker token tests
test-tokens:
    cargo test -p g5-api --test api_internal -- token --test-threads=1

# Run E2E storage separation scenarios
e2e-storage:
    cargo test -p g5-e2e -- worker_storage_separation --test-threads=1
```

---

### 17.6 Test Coverage Matrix

Maps each design section to the tests that verify it:

| Design Section | Unit Tests | Integration Tests | E2E Tests |
|---|---|---|---|
| §2.10 Ops Layer | U-OPS-01 – U-OPS-36 | I-API-01 – I-API-09 | E2E-09 |
| §2.10.1 DraftState | U-OPS-02, U-OPS-16 | I-API-01, I-API-02 | E2E-07 |
| §2.5 URL Clone | U-URL-01 – U-URL-07 | — | E2E-01 |
| §3 Worker Tokens | — | I-TKN-01 – I-TKN-14 | E2E-03, E2E-04 |
| §3.4 Secrets Bucket | U-SEC-01 – U-SEC-03 | I-SEC-01 – I-SEC-03 | E2E-06 |
| §4 Chat Priority | — | I-PRI-01 – I-PRI-03 | E2E-05 |
| §5 NFS Removal | U-HELM-01 – U-HELM-05 | — | E2E-01, E2E-11 |
| §9.1 CRDT Bug | U-CRDT-01 – U-CRDT-03 | — | — |
| §9.1 CLI Refactor | U-CLI-01 – U-CLI-10 | — | E2E-09 |
| §9.1 Worker Refactor | U-WRK-01 – U-WRK-09 | — | E2E-09 |
| §9.4 API Keys | — | I-KEY-01 – I-KEY-10 | E2E-12 |
| §10.2 Conflict Detection | U-CFD-01 – U-CFD-09 | I-CFD-01 – I-CFD-06 | E2E-15 |
| §10.5 Workspace Merge | U-OPS-34 – U-OPS-36 | I-API-03 – I-API-06 | E2E-10, E2E-14 |
| §13 Task Lease | — | I-LSE-01 – I-LSE-11 | E2E-02, E2E-13 |
| §14 Git HTTPS | U-GIT-01 – U-GIT-06 | I-GIT-01 – I-GIT-12 | E2E-01, E2E-09, E2E-11 |
| §14.4 Push Validation | — | I-HK-01 – I-HK-07 | E2E-07, E2E-08 |
| §14.5 Worker Git | U-WGIT-01 – U-WGIT-03 | — | E2E-01 |
| Op 8 MERGE | U-OPS-04 – U-OPS-08, U-MRG-01 – U-MRG-03 | I-API-06, I-MRG-01 – I-MRG-07 | E2E-10 |

**Total test count (Phase 1):**

| Category | Count |
|---|---|
| Unit tests (U-*) | 94 |
| Integration tests (I-*) | 72 |
| E2E tests (E2E-*) | 14 |
| Frontend tests (FE-*) | 11 |
| **Phase 1 Total** | **191** |

Phase 2 (deferred): 16 integration tests (I-GCS-*, I-GCSW-*, I-KEY-*)
+ 1 E2E (E2E-12) + 4 frontend tests (FE-KEY-*) = 21 additional tests.

### 17.7 Milestone → Test Verification Map

Each milestone's "definition of done" is the set of tests below passing
in CI. The E2E gate test is the final verification — if it passes, the
milestone is shippable.

| Milestone | Unit Tests | Integration Tests | E2E Gate | Total |
|---|---|---|---|---|
| **M1 — Foundation** | U-OPS-01 – U-OPS-36, U-CRDT-01 – U-CRDT-03 | — | — | 39 |
| **M2 — Git Transport** | U-GIT-01 – U-GIT-06, U-WGIT-01 – U-WGIT-03 | I-GIT-01 – I-GIT-12, I-HK-01 – I-HK-07 | E2E-01, E2E-07, E2E-08 | 31 |
| **M3 — Worker Isolation** | U-WRK-01 – U-WRK-09, U-HELM-01 – U-HELM-05 | — | E2E-01, E2E-11 | 16 |
| **M4 — Token Scoping + Secrets** | U-SEC-01 – U-SEC-03 | I-TKN-01 – I-TKN-14, I-SEC-01 – I-SEC-03 | E2E-03, E2E-04, E2E-06 | 23 |
| **M5 — Task Lifecycle** | — | I-PRI-01 – I-PRI-03, I-LSE-01 – I-LSE-11 | E2E-02, E2E-05, E2E-13 | 17 |
| **M6 — CLI Unification** | U-CLI-01 – U-CLI-10, U-URL-01 – U-URL-07, U-MRG-01 – U-MRG-03 | — | E2E-09 | 21 |
| **M7 — API Service Migration** | — | I-API-01 – I-API-09, I-MRG-01 – I-MRG-07 | E2E-09, E2E-10, E2E-14 | 19 |
| **M10 — Conflict Detection** | U-CFD-01 – U-CFD-09 | I-CFD-01 – I-CFD-06 | E2E-15 | 16 |
| **M9 — Full Integration** | (all above) | (all above) | ALL Phase 1 E2E | 191 |

**Phase 2 tests (deferred with M8):** I-GCS-01 – I-GCS-03,
I-GCSW-01 – I-GCSW-03, I-KEY-01 – I-KEY-10, E2E-12 (17 tests).

**Frontend tests by milestone:** FE-TKN-01 – FE-TKN-05 → M4,
FE-LSE-01 – FE-LSE-04 + FE-PRI-01 – FE-PRI-02 → M5,
FE-KEY-01 – FE-KEY-04 → deferred with M8.

**CI pipeline per milestone:** Each milestone adds a CI step that runs
its test set. The E2E gate test is a blocking check — PRs targeting
a milestone cannot merge until the gate passes. Use `--test-threads=1`
for E2E scenarios that share database state.

```
Milestone CI step naming convention:
  test-m1-foundation      → cargo test -p g5-common -- ops
  test-m2-git-transport    → cargo test -p g5-api --test api_git
  test-m3-worker-isolation → cargo test -p g5d -- worker
  test-m4-tokens           → cargo test -p g5-api --test api_internal -- token
  test-m5-lifecycle        → cargo test -p g5-api --test api_tasks -- lease priority
  test-m6-cli              → cargo test -p g5d -- cli
  test-m7-api-services     → cargo test -p g5-api --test api_services
  test-m10-conflicts       → cargo test -p g5d -- crdt::conflict
  e2e-gate                 → cargo test -p g5-e2e -- worker_storage_separation
  # Phase 2 (deferred): test-m8-phase2 → cargo test -p g5-api --test api_gcs api_auth
```
