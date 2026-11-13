# G5 — Condensed Design

> Ontology-centric development environment where design intent is the primary
> artifact, agents execute work in isolated branches, and humans retain
> authority via the IDE.

---

## 1. Jobs To Be Done

G5 serves three personas: **engineers** designing systems, **agents** executing
work, and **teams** collaborating on large codebases. Each has a primary job.

### Engineer

| Job | How G5 Does It |
|-----|---------------|
| Capture design intent in a navigable structure | **Ontology** — a hierarchical spec tree that maps 1:1 to the codebase. Each node is a structured markdown document (`SemliftSpec`) describing interface, behavior, and constraints. |
| Generate code from design specs | **Downlift** — agents read a spec and produce source files on an isolated branch. Validated (build + test) before completion. Works incrementally: when a spec changes, only stale nodes are re-downlifted. |
| Keep specs synchronized with code | **Uplift** — agents read source files and produce or update specs. Runs leaf-first up the dependency graph. Works incrementally: when code changes, only the affected file specs (and their ancestors) are re-uplifted. |
| Review agent work before it lands | **Task branches** — every agent task runs on its own git branch. Humans review diffs, intervene mid-execution, or open the branch in their IDE. Merge is always explicit. |
| Edit collaboratively | **CRDT drafts** (Yjs) — real-time co-editing of specs, code, and plans. Google Docs–style; agents and humans edit the same document. |

### Agent

| Job | How G5 Does It |
|-----|---------------|
| Understand the codebase's structure and contracts | **TaskContext** — each task is scoped to an ontology node with its source files, dependency specs, and parent/child hierarchy pre-loaded. |
| Work without stepping on others | **Isolation** — krunvm microVMs (local, §4.6) or Kata VMs (cloud). Each task branch is independent. |
| Recover from failures | **Dirty tree preservation** — CRDT draft survives worker crashes. Failed tasks auto-retry with the partial work intact. Humans can unblock stuck tasks. |

### Team

| Job | How G5 Does It |
|-----|---------------|
| Coordinate parallel efforts across repos | **Workspaces** — a multi-repo environment, each repo pinned to a branch. Multi-repo ontology displayed as a forest. |
| Browse canonical state without branching | **Tracking branches** — a branch mode that auto-advances to the remote HEAD on push. Editing on a tracking branch creates a new branch or CRDT draft atop the moving HEAD. |
| Fork/snapshot a workspace | **Clone** — deep-copy a workspace including dirty (uncommitted) CRDT state. |
| Track cost and progress | **Jobs & tasks** — hierarchical execution with real-time timeline, per-task cost accounting, and org-level billing. |

---

## 2. The Ontology

The ontology is the structural backbone of G5. It is a **hierarchical tree of
specs** that mirrors the codebase's module structure.

### 2.1 Core Concepts

```
Ontology (per repo)
├── Module spec (directory-level)
│   ├── File spec (src/auth/validator.rs)
│   ├── File spec (src/auth/claims.rs)
│   └── Module spec (src/auth/middleware/)
│       └── File spec (src/auth/middleware/jwt.rs)
└── Module spec (src/storage/)
    └── ...

Workspace-level spec (aggregates all module specs)
```

| Concept | What It Is |
|---------|-----------|
| **Ontology node** | A row in the database pointing to a spec file (`.g5/specs/<component_id>.md`). Has parent/children, repo binding, sync/staleness metadata. |
| **SemliftSpec** | Markdown + YAML frontmatter. Four metadata fields: `component_id`, `src_files`, `parent_spec`, `child_specs`. Body is free-form: interface, behavior, implementation hints. |
| **Component ID** | Filesystem-derived hierarchical identifier: `src/auth/validator.rs` → `auth/validator`. Stable, deterministic. |
| **Dependency edge** | Computed from source imports (not stored in specs). Drives topological task ordering. |
| **Staleness** | A node is stale when its children (uplift) or parent (downlift) changed since the node was last synced. UI shows green/yellow dots. |

**Git storage convention:** All G5-managed content lives under a `.g5/`
directory at the repo root, committed alongside the source code:

```
<repo>/
├── src/                          ← source code
├── .g5/
│   └── specs/                    ← ontology specs (SemliftSpec .md files)
│       ├── auth/
│       │   ├── validator.md      ← file spec for src/auth/validator.rs
│       │   └── claims.md
│       └── storage/
│           └── ...
└── ...

<workspace>/
├── .g5/
│   ├── workspace-meta/           ← workspace meta repo (workspace.yaml + workspace.lock)
│   └── docs/                     ← independent checkout of org docs repo
│       ├── auth-adr.md           ← linked doc (frontmatter references nodes)
│       ├── streaming-design.md
│       ├── roadmap.md            ← standalone doc (no node links)
│       └── onboarding.md
├── repos/
│   ├── g5-api/                   ← cloned repo (COW where possible)
│   └── frontend/
└── ...
```

The `component_id` maps directly to the spec path:
`auth/validator` → `.g5/specs/auth/validator.md` (inside each repo).
Specs are per-repo and committed to the repo's git history — they travel
with branches.

Docs live in a **dedicated independent Git repository checkout**
(`.g5/docs/`) at the workspace level, not inside individual repos. The
checkout uses the org's docs repository, which is the single source of
truth for all design documents. See §2.2 and §2.4 below.

### 2.2 Base Workspace and Workspace Subsetting

Every org has a **base workspace** — the canonical workspace that includes
all repos at their default branches and the full docs repository. The base
workspace is the organizational home for global design documents, ADRs,
roadmaps, and onboarding guides.

All other workspaces are **subsets** of the base workspace:

- A new workspace is created by forking from the base (or from another
  workspace that was itself forked from the base).
- The workspace includes a subset of the base's repos (via
  `workspace_branches`) and the independent docs repository.
- Docs in the workspace are **not shadowed or overridden** — they are
  the same files from the org's docs repo, checked out at a branch.
  A workspace may choose to include only a subset of docs relevant to
  the task at hand.
- The goal is to **merge the workspace back** into the base workspace's
  branches when the work is complete. Docs can also track the base
  branch (via `tracking` mode on the docs repository) to stay current
  with org-wide changes.

This model mirrors how feature branches relate to `main`: the base
workspace is `main` for the entire org's design state.

### 2.3 Multi-Repo Forest

A workspace spanning N repos sees N ontology trees — one per repo. Each tree
is rooted at the repo's top-level module spec. Cross-repo dependencies are
expressed as edges between nodes in different trees.

```
Workspace "add-streaming-api"
├── acme/backend (feat/streaming-api)
│   └── Core → Execution Engine → streaming_operator
├── acme/frontend (feat/streaming-ui)
│   └── UI → Pages → StreamingDashboard
└── acme/shared-types (feat/streaming-types, tracking)
    └── Types → Messages → StreamEvent

Cross-repo edges:
  streaming_operator → StreamEvent
  StreamingDashboard → StreamEvent
```

### 2.4 Docs

Docs are design documents that live in the **workspace-level independent
docs repository checkout** (`.g5/docs/`). Unlike specs (which are per-repo
under `.g5/specs/`), docs are workspace-scoped — they capture
cross-cutting design intent that may span multiple repos.

**Independent repository model:** The `.g5/docs/` directory is a normal
checkout of the org's dedicated docs repository. The org's base workspace
holds the canonical checkout at its default branch. When a workspace is
forked, the docs repository can be branched independently (for edits that
will merge back) or left in tracking mode (to stay current with the base).
`g5d` manages clone, fetch, and checkout automatically during workspace
lifecycle operations (`init`, `clone`, `fork`, `refresh`); the parent
repository does not stage or pin the docs checkout.

**CRDT editing:** Docs participate in the same Yjs CRDT editing
pipeline as specs. A `draft_session` can be opened for the independent
docs repository checkout, enabling real-time collaborative editing of
design documents in the browser, VS Code, or by agents.

Any doc can optionally link to ontology nodes via YAML frontmatter:

```yaml
---
linked_nodes: ["auth/validator", "auth/claims"]
---
# Auth Token ADR

We chose RS256 over HS256 because ...
```

**Linked vs standalone:** If a doc's frontmatter contains `linked_nodes`,
it participates in uplift/downlift workflows. If not, it is a standalone
document (roadmaps, onboarding guides, etc.) that is unaffected by
workflow execution.

**How linked docs differ from specs:** Specs form a strict tree
(parent/child). Linked docs form a **graph** — a single doc can
reference nodes across multiple modules and repos.

**Participation in workflows:** Linked docs participate in uplift
and downlift. Because they link to spec nodes rather than sitting in
the parent/child hierarchy, tasks flow in **both directions** relative
to the doc's links:

- **Uplift direction (code changed → update doc):** When a spec node
  linked to a doc changes, the doc is stale and may need re-uplift to
  reflect the new reality.
- **Downlift direction (doc changed → update specs):** When an
  engineer edits a linked doc, the specs it references may need
  re-downlift to incorporate the updated design intent.

**DB indexing:** The `doc_node_links` table in Postgres is a queryable
index of each doc's `linked_nodes` frontmatter — the same pattern as
how ontology nodes index spec metadata. The frontmatter is the source
of truth; the table enables efficient queries ("which docs reference
this node?").

**Cycle breaking:** Because linked docs can reference specs that
transitively reference each other, dependency cycles are possible.
During bootstrap, the analyzer breaks cycles on doc edges using the
same DFS heuristic as import cycles (§3.1) — the weaker edge (fewer
transitive dependents) is removed to produce a DAG.

### 2.5 Canonical Document Format

All `.g5/` content — specs, docs, and comments — is serialized as an
**Extended Markdown** dialect that round-trips deterministically through
the rich editor (§4.5). The baseline is **GFM + YAML frontmatter**,
extended with directives for rich features.

**Supported blocks (MVP):**

| Category | Syntax | Notes |
|----------|--------|-------|
| **Frontmatter** | `---` YAML `---` | Metadata: `component_id`, `src_files`, `linked_nodes`, etc. |
| **Paragraphs, headings** | Standard Markdown | `#` through `######` |
| **Inline formatting** | `**bold**`, `*italic*`, `` `code` ``, `[link](url)` | Standard |
| **Fenced code** | ` ```lang ` | Only fenced (no 4-space indent). Language hint required. |
| **Tables** | GFM pipe tables | No HTML `<table>` fallback. |
| **Math** | `$...$` inline, `$$...$$` block | KaTeX rendering. |
| **Images** | `![alt](url "caption")` | Standard + optional directive for sizing. |
| **Diagrams** | ` ```mermaid `, ` ```dot `, ` ```plantuml ` | Fenced code blocks with renderer hints. |
| **Mentions** | `@{user:james}` | Enforced syntax; resolved to user IDs on parse. |
| **Task lists** | `- [ ]` / `- [x]` | GFM task lists. |
| **Callouts** | `:::note`, `:::warning`, `:::tip` | Directive blocks. |
| **Collapsible** | `:::details{summary="..."}` | Directive block. |

**Comments (anchored threads):**

Comments are not stored inline in the Markdown body. Instead, a stable
anchor is placed at the comment site, and the thread lives in a directive
block:

```markdown
This paragraph needs work. ^cmt:8f2a

:::comment{anchor="cmt:8f2a" id="th_123"}
- @{user:alex} 2026-02-06: clarify the assumption
- @{user:james} 2026-02-06: will do
:::
```

In the rich editor, `^cmt:...` anchors are invisible; the directive
renders as a sidebar thread. In source mode, both are visible and
editable.

**Directive pattern:** Anything "beyond Markdown" uses the generic
directive syntax (`:::type{attrs}...:::`). This covers callouts,
collapsible sections, figure sizing, custom blocks (experiment results,
API schemas, TODOs), and future extensions — without inventing ad-hoc
inline syntax for each feature.

**Serialization invariant:** `serialize(parse(md))` must be stable.
The editor normalizes formatting on save: consistent heading style,
fenced-code-only, GFM tables, fixed list indentation. This prevents
save churn and keeps git diffs readable.

### 2.6 Editor Architecture (Summary)

Specs and docs are edited with a dual-mode editor:

| Mode | Component | Purpose |
|------|-----------|---------|
| **Rich (WYSIWYG)** | Tiptap / ProseMirror | Notion-like editing for docs; directives render as interactive blocks (comments, callouts, diagrams) |
| **Source** | Monaco | Raw Markdown editing with syntax highlighting; used for code files and source-mode toggle |

Switching modes triggers `parse` (source → rich) or `serialize` (rich →
source). Both modes bind to the same Yjs CRDT document, so collaborative
editing and agent edits flow through identically. The detailed editor
architecture — document schema, parse/serialize pipeline, mode-switch
algorithm, and component interfaces — is in FRONTEND_DESIGN.md §11.

---

## 3. Major Workflows

### 3.1 Uplift (Code → Spec)

**Trigger:** User clicks ↑ on an ontology node. The operation is **recursive** —
it uplifts the entire subtree rooted at that node (all descendants, then
aggregates upward to the selected node).

```
Analysis (bootstrap)                    Execution (per-task, recursive)
─────────────────────                   ──────────────────────────────
1. Import simulation:                   1. Agent reads source files
   parse imports per-language           2. Extracts public interface
   (Python AST, C++ regex,             3. Synthesizes SemliftSpec
    Rust mod/use, Java import,          4. Validates format
    Go import, TS/JS require)           5. Commits + pushes
2. Build dependency DAG
3. Break cycles (DFS edge removal)     Ordering: leaf-first (bottom-up)
4. Create hierarchical aggregation      Concurrency: same-level tasks
   (file → module → workspace)            run in parallel
5. Prune trivial aggregates
6. Persist task graph to DB
```

**Result:** A spec tree mirroring the codebase. Module specs summarize their
children. The workspace spec summarizes all modules.

**Incremental mode:** Uplift is not a one-shot reverse-engineering step — it
is the primary mechanism for **keeping specs synchronized with code**. When
source files change, only the affected file specs (and their module/workspace
ancestors) are stale. A staleness check during bootstrap prunes up-to-date
nodes from the task graph, so incremental uplift touches only what changed.
Linked docs that reference changed specs are also marked stale and
included in the uplift pass.

### 3.2 Downlift (Spec → Code)

**Trigger:** User clicks ↓ on an ontology node. The operation is **recursive** —
it downlifts the selected node first, then all descendants in top-down order.

```
1. Agent reads the target spec + dependency specs
2. Generates source files at paths listed in src_files
3. Validates: build + test via terminal
4. If validation fails: reads errors, refines, retries
5. On completion hooks: scans for newly created child specs,
   creates downstream tasks dynamically
6. Commits + pushes

Ordering: root-first (top-down), recursive through subtree
Decomposition: root spec can generate child specs, which spawn new tasks
```

**Incremental mode:** When a spec is edited (by a human or via a linked
document change), the bootstrap compares `content_updated_at` against
`last_downlifted_at` on each descendant. Only stale nodes produce tasks.
The agent receives the spec diff as context and applies targeted edits
rather than regenerating from scratch.

### 3.3 Review (Spec ↔ Code)

**Trigger:** User clicks review on an ontology node. The operation is
**recursive** — it audits the selected node and all descendants.

Bidirectional audit: does the code satisfy the spec? Does the spec reflect
the code? Produces a structured report + corrective diffs. Can update
specs (uplift direction) or code (downlift direction) as needed.

### 3.4 Chat

Open-ended dialog scoped to a workspace and optionally an ontology node.
Full tool access (read files, explain code, draft snippets). Uses
`respond_to_user` instead of `transition_task_state`.

### 3.5 Workspace Lifecycle

```
Create workspace ──▶ Add repos (workspace_branches) ──▶ Uplift (understand)
                                                            │
                        ┌───────────────────────────────────┘
                        ▼
              Edit specs (human + agent collaboration)
                        │
                        ▼
              Downlift (generate code) ──▶ Review ──▶ Merge
```

**Clone:** Deep-copy a workspace at any point — including dirty CRDT state.
**Tracking branches:** Add a repo at its default branch in tracking mode to
browse canonical state. When ready to edit, the UI creates a new branch
(switching from tracking to pinned mode) — no separate "promote" step.

---

## 4. Architecture

### 4.1 Components

G5 has **two binaries**: `g5-api` (the stateful service layer) and `g5d`
(the compute daemon that runs on a filesystem).

```
┌──────────────┐  ┌────────────────────┐  ┌──────────────────────────────┐
│   g5 CLI     │  │  VS Code Extension │  │  Browser (React + Monaco)    │
│              │  │                    │  │                              │
│  chat REPL   │  │  Sidebar panels:   │  │  Ontology · Spec editor      │
│  task control│  │  tasks, chat,      │  │  Code viewer · Chat          │
│  diff, status│  │  ontology, diffs   │  │  Task timeline               │
│              │  │  File editing via  │  │  CRDT sync (Yjs)             │
│              │  │  g5d checkout dir  │  │  Direct WS to g5d            │
└──────┬───────┘  └────────┬───────────┘  └───────────┬────────┬────────┘
       │                   │                          │        │
       │  REST/WS          │  REST/WS + FS events     │REST/WS │Direct WS
       ▼                   ▼                          ▼        │
┌─────────────────────────────────────────────────────┐        │
│              g5d (editor mode)  ×N/ws               │◄───────┘
│                                                      │  Search, file stream
│  Git checkout (materializes CRDT drafts to disk)    │
│  Search index (ripgrep)                             │
│  File streaming (browser/IDE)                       │
│  Agent loop (chat)                                  │
│  Worker pool management                             │
│  ×N: one per connected user                          │
└──────────────────────┬───────────────────────────────┘
                       │ REST/WS
                       ▼
┌───────────────────────────┐
│    g5-api (API server)     │     ┌─────────────────────────────┐
│    separate binary         │     │   g5d (worker mode)  ×N     │
│                            │     │                             │
│  REST + WS API             │     │  Git worktree               │
│  Session host              │     │  Agent loop (tasks)         │
│  Yjs CRDT authority        │     │  Build + test               │
│  Token issuer              │     │  Terminal (PTY)             │
│  Job orchestrator          │     │  Toolchain caches           │
│  Ontology queries          │     │                             │
│  WebSocket hub             │     │  Local: krunvm microVM      │
│                            │     │  Cloud: krunvm / Kata (GKE) │
└──────────┬─────────────────┘     └─────────────────────────────┘
           │ manages
┌─────────────────────┐          │                             │
│     PostgreSQL       │          │  Git worktree               │
│                      │          │  Agent loop (tasks)         │
│  orgs, users, repos  │          │  Build + test               │
│  workspaces, branches│          │  Terminal (PTY)             │
│  ontology_nodes      │          │  Toolchain caches           │
│  draft_sessions (Yjs)│          │                             │
│  jobs, tasks         │          │  Local: krunvm microVM      │
│  chat_sessions       │          │  Cloud: krunvm / Kata (GKE) │
│  comments, billing   │          └─────────────────────────────┘
└─────────────────────┘

g5d editor and worker are the same binary in different modes.
The editor is a worker variant: same filesystem + compute
foundation, plus search + file streaming for interactive use.
IntelliSense is deferred to **Open in IDE**.
```

### 4.2 Two Binaries

| | `g5-api` (API server) | `g5d --editor` | `g5d --worker` |
|-|----------------------|-----------------|----------------|
| **Binary** | Separate binary | g5d | g5d |
| **Purpose** | HTTP/WS API, orchestration, persistence | IDE backend (interactive use) | Task execution (agent use) |
| **Postgres** | Yes (reads + writes) | No (talks to g5-api) | No (talks to g5-api) |
| **Yjs CRDT** | Authority (persists to Postgres) | Receives synced state | Materializes draft to disk |
| **Token issuer** | Yes (mints JWTs) | No (validates JWT) | No (validates JWT) |
| **Git** | No | Checkout of user's branch | Worktree of task branch |
| **Agent loop** | No | Chat | Uplift / downlift / review |
| **LSP / IntelliSense** | No | No (defer to Open in IDE) | No (defer to Open in IDE) |
| **Build/test** | No | Yes | Yes |
| **Terminal** | No | Yes | Yes (persistent PTY sessions) |
| **Search** | No | Yes (ripgrep) | Yes (ripgrep) |
| **Scaling** | 1 (Phase 1) / N replicas (Phase 2) | N per workspace (one per connected user) | N per workspace (local or cloud) |

**Editor = worker + search.** Both modes share the same g5d binary and
the same filesystem/compute foundation (git checkout, shell, file I/O).
The editor adds search + file streaming for interactive use. The
distinction is configuration, not code: `g5d --editor` serves the
browser; `g5d --worker` runs agent tasks headlessly. LSP is handled
by the user's IDE via **Open in IDE**.

### 4.3 Data Flow

```
Editing:  Browser ←Yjs→ g5-api (CRDT authority) → Postgres (durable)
                                                 → g5d editor (file on disk)
          Browser ←WS→ g5d editor for search + file content

Agent:    g5-api assigns task → g5d worker runs agent loop
          Agent calls tools → g5d worker executes (read/write/build/test)
          Agent writes file → CRDT delta → g5-api → Postgres (durable)
          Agent completes → git commit + push on task branch

Viewing:  Browser → g5-api (REST) → Postgres (metadata) + Git (content)
          Browser → g5d editor (direct WS) for file content + search

Auth:     Browser holds OAuth token → g5-api mints short-lived JWT
          → g5d (editor or worker) validates JWT locally (no callback)
```

### 4.4 Client Surfaces

G5 has four client surfaces (WORKER_INFRASTRUCTURE.md §11.17) delivering three experience tiers:

| Tier | Surface | Experience | Offline |
|------|---------|-----------|---------|
| **1. Google Docs (web)** | Browser | Real-time CRDT co-editing. Richest UI (ontology, timeline, comments, dual-mode editor). IntelliSense via **Open in IDE**. | ❌ |
| **2. Google Docs (plugin)** | VS Code + Yjs (Phase 3) | Real-time CRDT co-editing inside VS Code. Full local toolchain + IDE. | ✅ Yjs offline-first |
| **3. Synced workspace** | Any editor + g5d checkout | g5d materializes drafts to disk. Editor reads/writes files normally. Works with VS Code (Phase 2), Vim, JetBrains, etc. | ⚠️ Git-level only |

The **CLI** (`g5`) is a thin headless client for scripting, SSH, and CI — no editing.
**Backend workers** execute agent tasks headlessly and are not a user surface.

All surfaces communicate via the same g5-api REST/WS API and g5d instance API. The `g5d` daemon is the local backend for all user-facing surfaces: it manages the git checkout, materializes CRDT drafts to disk, serves file-watch events, runs the agent chat loop, and manages the worker pool.

### 4.5 Agent Architecture

One architecture for all job types: the **ReAct loop** with pluggable dialog
compaction (LSM Tree — level-based summarization of older turns).

```
                  ┌──────────────────────────────┐
                  │        System Prompt          │
                  │  (job-type template + context) │
                  └──────────────┬───────────────┘
                                 │
                  ┌──────────────▼───────────────┐
                  │      Dialog Prepopulation      │
                  │  Phase 0: Target spec          │
                  │  Phase 1: Workspace structure    │
                  │  Phase 2: Source files          │
                  │  Phase 3: Dependency specs      │
                  │  Phase 4: Child specs           │
                  └──────────────┬───────────────┘
                                 │
              ┌──────────────────▼──────────────────┐
              │             ReAct Loop               │
              │                                      │
              │  1. LLM reasons about next step      │
              │  2. LLM emits tool call              │
              │  3. Worker executes tool              │
              │  4. Result appended to dialog         │
              │  5. Dialog compactor manages tokens   │
              │  6. Repeat until terminal state       │
              │                                      │
              │  Terminal: transition_task_state()    │
              │    → complete (commit + push)         │
              │    → blocked (human needed)           │
              │    → fail (auto-retry or escalate)    │
              └──────────────────────────────────────┘
```

**Tools available to agents:**

| Category | Tools |
|----------|-------|
| Analysis | `read_file`, `read_spec`, `list_specs`, `validate_spec`, `extract_interface` |
| Mutation | `write_file`, `edit_file_replace`, `create_spec`, `edit_spec_replace`, `delete_file` |
| Execution | `run_in_terminal` (persistent shell sessions) |
| Git | `git_status`, `git_diff`, `git_commit` (checkpoints) |
| Lifecycle | `transition_task_state`, `respond_to_user` |

### 4.6 Worker Isolation Architecture

Worker tasks execute inside **isolated environments** to protect the host
from fork bombs, dependency pollution, and accidental filesystem damage.
The isolation strategy uses a Rust wrapper with **pluggable backends**
selected at runtime based on platform support:

```
┌──────────────────────────────────────────────────────────────────┐
│                    g5d --worker                                   │
│                                                                  │
│  MicrovmRunSpec { image, cmd, env, mount, cpus, mem }           │
│         │                                                        │
│         ├──▶ KrunvmBackend    (Linux KVM + macOS ARM64 HVF)     │
│         │    └─ krunvm run ... (OCI → microVM, virtio-fs mount)  │
│         │                                                        │
│         ├──▶ Wsl2Backend      (Windows, WSL2 + containerd)      │
│         │    └─ wsl.exe + ctr run ... (OCI in Linux VM)          │
│         │                                                        │
│         └──▶ ProcessBackend   (any platform, weakest isolation)  │
│              └─ git worktree + subprocess (no VM boundary)        │
└──────────────────────────────────────────────────────────────────┘
```

**Backend selection order:** The worker probes platform capabilities at
startup and selects the strongest available backend:

| Priority | Backend | Platform | Mechanism | Isolation level |
|----------|---------|----------|-----------|----------------|
| 1 (best) | `KrunvmBackend` | Linux (KVM, x86_64 + aarch64) + macOS ARM64 (HVF) | `krunvm` / `libkrun` — boots OCI image in a microVM with virtio-fs host folder sharing | Full VM boundary (CPU/RAM caps, separate kernel) |
| 2 | `Wsl2Backend` | Windows (WSL2 enabled) | Manages a minimal Linux VM in WSL2, runs `containerd` + `runc` inside it, talks over vsock/ssh | Linux VM boundary (standard "containers on Windows" model) |
| 3 (fallback) | `ProcessBackend` | Any | Git worktree + child process, no container or VM | Process-level only (weakest; suitable for local dev) |

**Rust API shape:**

```rust
pub struct MicrovmRunSpec {
    pub image: String,                // OCI image ref, e.g. "ghcr.io/acme/worker:latest"
    pub cmd: Vec<String>,             // entrypoint + args
    pub env: Vec<(String, String)>,   // environment variables
    pub mount_host_dir: PathBuf,      // workspace checkout on host
    pub mount_guest_dir: String,      // mount point inside VM, e.g. "/work"
    pub cpus: u8,                     // CPU cap
    pub mem_mib: u32,                 // RAM cap in MiB
}

pub struct RunResult {
    pub exit_code: i32,
    pub stdout: String,               // captured or streamed
    pub stderr: String,
    pub duration: Duration,
}

pub trait MicrovmBackend: Send + Sync {
    /// Boot the OCI image in an isolated environment and run the command.
    fn run(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunResult>;
    /// Kill the running environment immediately (kill-switch).
    fn kill(&self, handle: &RunHandle) -> anyhow::Result<()>;
    /// Check if this backend is available on the current platform.
    fn probe(&self) -> bool;
}
```

**KrunvmBackend implementation (Phase 1 priority):**

1. Ensure `krunvm` is installed (or ship alongside `g5d`).
2. Construct `krunvm run` invocation with image, command, volume mapping
   (host dir → guest dir), and CPU/RAM limits.
3. Stream stdout/stderr for real-time timeline entries.
4. On timeout / cancel / abort: kill the `krunvm` process tree.
5. On macOS: requires Apple Silicon (ARM64 + Hypervisor.framework).
   Intel Macs fall back to `ProcessBackend`.
6. Future: FFI into `libkrun` (C API) for embedded use without subprocess.

**Wsl2Backend implementation (Phase 2):**

1. Ensure a WSL2 distro with `containerd` is available (ship a minimal
   distro or require user setup).
2. Shell out to `wsl.exe` to run `ctr run` with the OCI image inside
   the Linux VM.
3. Map host folder via WSL2's built-in 9P/Plan 9 filesystem sharing.
4. Capture logs via SSH or vsock.

**Folder sharing model:**

- One workspace directory per agent task run.
- Mounted read-write into the VM at `/work`.
- Agent operates on the mounted workspace; outputs collected via git diff
  after the run completes.
- Even without a security threat model, VM isolation protects against:
  fork bombs (contained in guest), dependency installs that trash the
  host environment, and accidental `rm -rf /` (hits guest rootfs).

**Configuration:** The `isolation` block in `.g5/workspace.yaml`
(G5D_CLI.md) controls backend selection:

```yaml
isolation:
  mode: "microvm"        # microvm | process
  fallback: "process"    # used when microvm backend is unavailable
  base_image: "ghcr.io/g5/worker:base"
  resources:
    cpus: 4
    memory_gb: 16
```

`mode: microvm` probes `KrunvmBackend` (Linux/macOS ARM64) or
`Wsl2Backend` (Windows) in priority order. If neither is available and
`fallback` is set, falls back to `ProcessBackend`. `mode: process`
skips VM probing entirely.

---

## 5. System Requirements

### 5.1 Performance

| Requirement | Target |
|-------------|--------|
| Keystroke latency (editing) | < 16ms (local CRDT, no network in path) |
| CRDT sync (editor ↔ server) | < 50ms |
| Ontology tree initial load | < 500ms for 1000 nodes |
| Agent task cold start (local) | < 5s (worktree setup) |
| Agent task cold start (cloud) | < 30s (krunvm/Kata microVM + clone + cache hydration) |
| Concurrent tasks per workspace | Up to 10 (configurable) |
| Agent budget per task | 200 turns, $10, 30 min, 1M tokens (configurable) |

### 5.2 Isolation & Safety

| Requirement | Mechanism |
|-------------|-----------|
| Task isolation | Each task gets its own git branch + microVM via krunvm/libkrun (local) or Kata VM (cloud). Falls back to git worktree process isolation when microVM is unavailable (see §4.6). |
| Agent cannot corrupt other work | CRDT draft scoped to one workspace_branch; git operations restricted |
| No silent propagation | Merge is always explicit and human-triggered |
| Git safety | `git checkout`, `reset`, `stash`, `merge`, `rebase` blocked in agent terminal |
| Credential scoping | Per-workspace GCS prefixes; short-lived JWTs for worker auth |

### 5.3 Durability & Collaboration

| Requirement | Mechanism |
|-------------|-----------|
| No work lost on tab close | Yjs CRDT state persisted server-side in Postgres |
| No work lost on worker crash | CRDT draft is server-authoritative; new worker materializes from draft |
| Real-time collaboration | Yjs awareness protocol — cursors, presence, live edits |
| Cross-branch comment visibility | Comment index in Postgres, queryable across all branches |
| Full agent audit trail | Every reasoning step + tool call persisted as `SessionMessage` |

### 5.4 Multi-Tenancy & Access

| Requirement | Mechanism |
|-------------|-----------|
| Org → repo → workspace hierarchy | Postgres schema with FK cascades |
| Role-based access | `owner`, `admin`, `member`, `viewer` on orgs; scope-based permissions |
| SCM integration | GitHub/GitLab/Bitbucket OAuth + encrypted token storage |
| Multi-repo workspaces | `workspace_branches` table: one row per (workspace, repo) pair |

### 5.5 Language Support

Codebase analysis (import simulation, dependency extraction) supports:
Python, C/C++, Rust, Java, Go, TypeScript, JavaScript. Generic fallback
for other languages (no dependency extraction, but agents can still read
and uplift files).

---

## 6. Phasing

### M0 — Core Loop (De-Risk)

**Goal:** Prove the end-to-end loop works and de-risk the UI integration. One engineer can point G5 at a Python codebase, uplift it, see the ontology in the browser, edit a spec, chat with the agent, and trigger a downlift — all on their laptop. Not production-ready. Not polished. But the loop closes.

**Success criteria:**
1. `g5-api` starts, creates a workspace, and accepts a git repo URL.
2. `g5d --editor` clones the repo, runs codebase analysis, and produces a task graph.
3. Agent uplift runs: picks a leaf file, reads it, writes a spec to `.g5/specs/`, commits.
4. Browser loads WorkspacePage: ontology tree renders from live API data (not mock).
5. Clicking a node shows the spec content in the center panel (Monaco, read-write).
6. Editing a spec persists via Yjs CRDT → Postgres → survives page refresh.
7. Chat panel sends a message → agent responds with tool calls visible in the UI.
8. Task list shows running/completed tasks with live status updates via WebSocket.
9. Triggering downlift on a spec produces source files on a task branch.

**Deployment:** Single `docker-compose up` (or `cargo run` + Postgres for pure-local dev).

```
docker-compose.yml
├── g5-api             — REST/WS API, Yjs CRDT, job orchestrator
├── g5d (editor)       — git checkout, search, chat agent loop
├── g5d (worker ×1)    — single local worker (git worktree)
├── postgres           — persistent state
└── browser UI         — Vite dev server
```

**What ships in M0:**

| Component | M0 Scope | Deferred to Phase 1 |
|-----------|----------|---------------------|
| **g5-api** | REST + WS API (workspaces, ontology nodes, jobs/tasks, chat sessions, drafts). Yjs CRDT authority with Postgres persistence. Job orchestrator (create job → schedule tasks → track completion). | Multi-org, billing, spend limits, slug resolver, settings API. |
| **g5d** | Editor mode: git clone/checkout, file watching, agent chat loop. Worker mode: git worktree, agent task loop (ReAct), persistent terminal (PTY), `run_in_terminal`. CRDT draft materialization (§11.18 — basic: no retry/failure UX). | Search index. Worker pool management (just 1 hardcoded worker). Prepared base caching (§11.19). Open-in-IDE wiring. |
| **Postgres schema** | Core tables: `orgs` (1 bootstrap), `users` (1 bootstrap), `repos`, `workspaces`, `workspace_branches`, `ontology_nodes`, `draft_sessions`, `jobs`, `tasks`, `chat_sessions`, `session_messages`. | `comments`, `usage_records`, `workspace_settings`, `linked_documents`. Full migration system (just `CREATE TABLE` for M0). |
| **Agent loop** | ReAct loop. Uplift + downlift + chat job types. Tool set: `read_file`, `write_file`, `edit_file_replace`, `create_spec`, `read_spec`, `list_specs`, `run_in_terminal`, `git_status`, `git_diff`, `git_commit`, `transition_task_state`, `respond_to_user`. Single LLM provider (Anthropic). | Dialog compaction (LSM). Prepopulation phases 3–4 (dependency/child specs). Review job type. `extract_interface` (tree-sitter). Multi-provider support. Budget enforcement. |
| **Codebase analysis** | Python analyzer only. Hierarchical aggregation. Basic cycle breaking. | Other 6 language analyzers. Generic fallback. Advanced cycle resolution. |
| **CRDT** | Yjs server in g5-api. Single-document editing (one spec at a time). Postgres persistence. Single-user (no awareness/cursors). | Multi-document concurrent editing. Awareness protocol. Collaborative cursors. |
| **UI** | See UI scope below. | Everything else in FRONTEND_DESIGN.md. |
| **Git** | Clone, checkout, worktree create/remove, add/commit/push. Task branch creation. | Tracking branches. Clone workspace. Multi-repo. |
| **Auth** | None. Single hardcoded user. No tokens. | API key auth (Phase 1). OAuth (Phase 2). |
| **Data layer** | Postgres (required from M0 — single instance via `docker-compose` or local install). Redis for job queues and pub/sub. No SQLite/in-memory fallback — CRDT persistence, job orchestration, and session history all require Postgres. `pure-local dev` = `cargo run` + local Postgres. | Managed Cloud SQL (Phase 2). Redis cluster (Phase 2). |

**M0 UI scope** (de-risk target):

The UI goal is to prove the WorkspacePage layout works with **live data** — replacing mock.ts with real API calls and WebSocket streams. This de-risks the component architecture, the API integration, and the real-time update path.

| Component | M0 Scope | Notes |
|-----------|----------|-------|
| **WorkspaceSwitcher** | List workspaces from API. Create workspace (name + repo URL). Click to open. | Minimal — just a list + create form. No org selector (single bootstrap org). |
| **WorkspacePage shell** | 3-column layout: left sidebar, center panel, right panel. Resizable. | Matches FRONTEND_DESIGN.md §3.4 layout. No bottom drawer yet. |
| **Ontology tree** (left) | Fetch tree from `GET /api/v1/workspaces/:id/ontology`. Render recursive tree with expand/collapse. Show sync status (green/yellow dot from API). Click to select node → open in center. Uplift (↑) / downlift (↓) buttons that trigger jobs via API. | No drag-drop. No multi-select. No file navigation section. |
| **Spec editor** (center) | Monaco editor. Load spec content from API (or Yjs doc). Edit → Yjs CRDT → persists. Tabs for multiple open specs. | Monaco only — no Tiptap/ProseMirror rich mode yet. No dual-mode toggle. No diff overlay. No annotations. |
| **Code viewer** (center) | Monaco editor, read-only, for source files. Load from `g5d` file API. | Read-only. No diff. No inline comments. |
| **Chat panel** (right) | Send message → create/resume chat session via API. Stream agent responses via WebSocket. Render tool calls as collapsible blocks. Show "thinking" indicator. | No markdown rendering in messages (just preformatted text). No file references. No intervention buttons. |
| **Task status** (left sidebar, "Jobs" tab) | Fetch jobs/tasks from API. Show status (pending/running/completed/failed/blocked). Live updates via WebSocket. Click task → show basic detail (description, status, error). | No timeline view. No files-changed list. No task branch diff. |
| **Draft status bar** | Show "Draft: N files modified" or "Clean". `[Commit]` button that calls commit API. | No `[Fork]`. No branch selector. No tracking mode. |
| **Theme system** | 10 themes × light/dark (20 palettes), persisted in localStorage. Theme selector in settings flyout. | Design tokens defined in FRONTEND_DESIGN.md §1.1. |

**What the M0 UI intentionally omits:**
- Tiptap/ProseMirror rich editor (Phase 1 — §11 is complex; Monaco for specs is sufficient to prove the loop)
- Comment system (Phase 1)
- Docs panel and linked documents (Phase 1)
- Diff overlays and annotations (Phase 1)
- Task timeline with chronological entries (Phase 1 — just status is enough)
- Settings page (Phase 1)
- Clone dialog (Phase 1)
- Slug routes (Phase 1)
- Dependency/dependents sidebar (Phase 1 — right sidebar is chat-only in M0)
- Org creation wizard / invite flow (Phase 1 — single bootstrap org in M0)
- Workspace visibility controls (Phase 1 — all workspaces are org-visible in M0)
- Search (Phase 1 — search_documents + pgvector indexing is Phase 1)

**Why this is the right cut:**

The riskiest UI integration is **live data flowing through the full stack**: WebSocket events updating the ontology tree as tasks complete, Yjs CRDT syncing spec edits through g5-api to Postgres, chat streaming through SessionMessage WebSocket, and job status propagating in real time. M0 proves all of these paths work. Everything deferred is **additive UI** (richer rendering, more panels, more features) on top of plumbing that M0 validates.

The riskiest backend integration is **g5d ↔ g5-api coordination**: g5-api creates a job, g5d worker picks it up, agent runs tools against the filesystem, CRDT deltas flow back to g5-api, and the browser sees the result. M0 closes this loop.

**Estimated effort:** 4–6 weeks for 1–2 engineers.
- Week 1–2: g5-api skeleton (REST routes, Postgres schema, Yjs integration) + g5d skeleton (git clone, worker mode, basic agent loop).
- Week 3–4: Wire up UI to live API. Replace mock.ts. WebSocket integration. CRDT editing in Monaco.
- Week 5–6: Agent uplift/downlift working end-to-end. Polish, fix integration bugs.

---

### Phase 1 — MVP (Local Daemon)

**Goal:** Single-user, single-machine G5 usable for real projects. Builds on M0 — everything in M0 ships, plus the items below.

**What Phase 1 adds over M0:**

| Component | What's new in Phase 1 |
|-----------|----------------------|
| **g5-api** | Multi-org model (still single-user). Settings API. Slug resolver. Spend tracking (`usage_records`). Org creation cascade (API_AND_STORAGE.md §3.12.1). Workspace visibility + members (§3.12.3). Hybrid search: Postgres FTS (`tsvector`) + semantic search (`pgvector`) — no external service. |
| **g5d** | Search index (ripgrep for local, `search_documents` for persistent). Worker pool management (`--local-workers N`). Prepared base caching (§11.19). Draft materialization retry/failure UX (§11.18). Open-in-IDE wiring. |
| **Postgres schema** | Full schema: `comments`, `usage_records`, `workspace_settings`, `linked_documents`, `search_documents` (with `tsvector` + `vector(1536)`), `workspace_members`. Extension: `pgvector`. Proper migrations (sqlx). |
| **Agent loop** | Dialog compaction (LSM). All prepopulation phases. Review job type. `extract_interface` (tree-sitter). Multi-provider (Anthropic + OpenAI). Budget enforcement. |
| **Codebase analysis** | All 7 language analyzers. Generic fallback. Full cycle resolution. |
| **CRDT** | Multi-document concurrent editing. Awareness protocol (cursors, presence). |
| **UI** | See Phase 1 UI scope below. |
| **Git** | Tracking branches. Clone workspace. Multi-repo workspaces. |
| **Auth** | API key auth. Single-user bootstrapped on first run. |

**Phase 1 UI scope** (pinned deliverables, see FRONTEND_DESIGN.md for component APIs):

| Component | Phase 1 Scope | Notes |
|-----------|---------------|-------|
| **Tiptap/ProseMirror editor** | Dual-mode (rich + source) for specs and docs (FRONTEND_DESIGN.md §11). | The core editing upgrade over M0's Monaco-only. |
| **Comment system** | Anchored inline comments, sidebar threads, resolve/reply. | FRONTEND_DESIGN.md §4.1, §11.6. |
| **Docs panel** | Left sidebar section showing `.g5/docs/` contents from the independent docs repository. | Navigates docs as first-class content. |
| **Diff overlays** | Side-by-side diff with inline comments, gutter symbols. | FRONTEND_DESIGN.md §3.4.5 DiffView. |
| **Task timeline** | Chronological agent trace: thoughts, tool calls, outputs, errors. | Bottom drawer Tab: Timeline. |
| **Settings page** | Workspace settings (spend limits, worker config). Org billing cards. Preferences. | FRONTEND_DESIGN.md §3.5. |
| **Search** | Spotlight (⌘K) + inline search bar. Hybrid keyword/semantic results from Postgres. Mode selector (keyword/semantic/hybrid). | FRONTEND_DESIGN.md §9.7. Backend: API_AND_STORAGE.md §3.10. |
| **Org creation wizard** | Modal flow: name org → import repos → invite members → done. Triggers server cascade (API_AND_STORAGE.md §3.12.1). | FRONTEND_DESIGN.md §3.6. |
| **Org members & invite page** | Full member list with roles, role editing, invite by email with role picker, pending invitations. | FRONTEND_DESIGN.md §3.7. Route: `/:org/settings/members`. |
| **Workspace visibility** | Org-visible / invite-only toggle in workspace settings. Members list for invite-only workspaces. 🔒 badge in WorkspaceSwitcher. | FRONTEND_DESIGN.md §3.8. Backend: API_AND_STORAGE.md §3.12.3. |
| **Clone dialog** | Clone workspace with branch selection. | FRONTEND_DESIGN.md §9.19. |
| **Dependency sidebar** | Node dependencies/dependents in sidebar or frontmatter card. | Integrated into SpecEditor frontmatter. |
| **Slug routes** | Human-readable URLs: `/:org/:workspace/spec/:specPath`. | FRONTEND_DESIGN.md §7.2.1. |

**Not in Phase 1:**
- Multi-tenancy, billing, user management beyond bootstrap user
- Cloud workers / Kata VMs (krunvm local microVM is Phase 1; cloud Kata is Phase 2)
- GitHub/GitLab webhooks and PR integration
- Edge gateway
- SSO / OAuth flows (API key auth only)

**Why this is valuable:** An engineer can point G5 at a codebase, uplift it
into an ontology, edit specs and docs, downlift changes, and review agent
work — all on their laptop. The full agent loop runs locally with real
file access.

### Phase 2 — Cloud & Multi-Tenancy

**Goal:** Hosted G5 service with dynamic scaling, multi-org, and billing.

**Deployment:** Kubernetes (GKE) with Kata runtime.

```
GKE Cluster
├── g5-api × replicas           — behind load balancer, stateless
│   └── connected to:
│       ├── Cloud SQL (Postgres) — managed, HA
│       ├── Redis                — job queues, pub/sub, ephemeral state
│       └── GCS                  — build cache, repo snapshots
├── g5d (editor) × N/workspace  — one per connected user; search + chat
├── g5d (worker) × dynamic      — krunvm/Kata microVMs, autoscaled per demand
├── Edge Gateway × replicas     — stateless WS proxy for browser→editor/worker
└── Control Plane               — editor/worker provisioning, capacity mgmt
```

**Additions over Phase 1:**

| Component | What changes |
|-----------|-------------|
| **Multi-tenancy** | Full org/user/membership model. Scoped permissions per workspace. Cross-org isolation at DB and worker level. |
| **Dynamic workers** | MicroVM workers (krunvm local, Kata cloud) provisioned on demand. Local-first with cloud overflow (§4.6). Worker pool autoscaling based on queue depth. |
| **Billing** | `usage_records` table tracking per-LLM-call cost. `SpendHistoryDTO` API. Billing report UI. Configurable spend limits per org/workspace. |
| **SCM integration** | GitHub/GitLab OAuth. Webhook receivers for push events (auto-advance tracking branches, trigger CI). PR creation from merged task branches. |
| **Auth** | OAuth login (GitHub, Google). Short-lived JWTs for data channels. Scope-based permission model (`workspace:read`, `workspace:write`, `workspace:admin`, etc.). |
| **Edge gateway** | Stateless WebSocket reverse proxy (~200 lines Rust). JWT validation, K8s service discovery, mTLS to worker pods. |
| **IDE Integration** | **Open in IDE** for LSP/IntelliSense and refactors. g5d does not proxy language servers. |
| **Caching** | GCS-backed build cache. Workspace-scoped prefixes. Toolchain-specific cache configs (Rust/sccache, C++/ccache, Go, Python, Java, JS/TS). |
| **Monitoring** | Prometheus metrics, structured logging, session replay, cost dashboards. |
| **HA** | g5-api replicas behind LB (stateless, sessions are DB-backed). g5d editor failover per user (respawnable). g5d worker failover via CRDT draft persistence. Postgres HA (Cloud SQL). |

**Migration from Phase 1:** The schema and API surface are the same.
Phase 1 data imports directly. The g5d binary is the same — deployment
topology changes, not the code. Local workers continue to work alongside
cloud workers (hybrid mode).

---

## Appendix: Document Map

| Document | Scope | Size |
|----------|-------|------|
| **DESIGN.md** (this file) | Condensed overview — jobs, ontology, workflows, architecture, requirements, phasing | — |
| **AGENTS.md** | Agent identity, job types, codebase analysis, task lifecycle, system prompts, tools, ontology grounding, batch execution, TaskContext, prepopulation, dialog compaction, budgets, training data, LLM client interface | ~1600 lines |
| **API_AND_STORAGE.md** | Postgres schema (14 table groups), REST/WS API (18 endpoint groups), DTOs, data flow, error catalog, token taxonomy | ~4000 lines |
| **FRONTEND_DESIGN.md** | React pages, component interfaces, hooks, routing, design system, real-time events, dual-mode document editor (§11: Tiptap/ProseMirror + Monaco, parse/serialize pipeline, comment system, CRDT integration) | ~3700 lines |
| **WORKER_INFRASTRUCTURE.md** | Execution model, krunvm/Kata microVMs, caching, security, workspace init, agent sessions, unified instance architecture, auth, client surfaces (§11.17), draft materialization (§11.18), workspace lifecycle (§11.19) | ~4800 lines |
| **ANALYSIS_DESIGN.md** | Import simulation algorithms, per-language analyzers, cycle breaking, hierarchical aggregation | ~1200 lines |
| **PROMPTS.md** | Prompt templates, PromptManager, dialog summarization | ~500 lines |
| **DEVELOPMENT.md** | Toolchains, devcontainer, GitHub Actions CI/CD, Kata E2E testing, container images, testing strategy, release process | ~700 lines |
