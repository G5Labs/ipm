# G5 Design Audit — Developer & Enterprise Use Cases

> How does G5's architecture hold up against the workflows real developers and organizations actually use?

This document maps G5's design decisions against common developer scenarios — from solo local development to enterprise team collaboration — identifying where the design excels, where it has known gaps, and where the architecture implicitly constrains users.

---

## Methodology

Each use case is evaluated on three axes:

| Axis | Question |
|------|----------|
| **Parity** | Can a developer do this in G5 as easily as they do it today? |
| **Advantage** | Does G5 make this *better* than the status quo? |
| **Gap** | Is anything missing, degraded, or not yet addressed? |

Ratings: ✅ Strong — ⚠️ Partial / Planned — ❌ Gap

---

## 1. Local Development in an IDE (Windows / Mac / Linux)

### The scenario

A developer clones a repo, opens it in VS Code (or Cursor/Windsurf), installs dependencies, and starts coding. They expect: instant keystroke response, local IntelliSense, integrated terminal, git in the sidebar, and zero cloud dependency for day-to-day editing.

### How G5 handles it

G5's local stack runs entirely on the developer's machine. `g5d --editor` serves the browser UI and manages git checkouts; `g5d --worker` (process isolation mode, no VM) executes agent tasks using local git worktrees. The IDE plugin connects to `g5d` over localhost REST + WebSocket — no cloud round-trip for editing.

Design Principle P1 ("Editing must feel native — <50ms keystroke-to-pixel") explicitly prioritizes local editing performance. CRDT sync is batched so it never blocks keystrokes (P1 > P4 in the resolution order). LSP/IntelliSense is **not** provided by G5 — it defers to the user's existing VS Code language extensions (rust-analyzer, Pylance, gopls, etc.), which is the right call since those are already best-in-class and local.

| Axis | Rating | Notes |
|------|--------|-------|
| **Parity** | ✅ | Local g5d process, real files on disk, VS Code's own LSP. The editing experience is indistinguishable from "just VS Code." |
| **Advantage** | ✅ | Ontology tree, agent chat, CodeLens annotations, and execution view are additive — they appear in the IDE without replacing anything. |
| **Gap — Windows** | ⚠️ | Worker isolation on Windows uses `Wsl2Backend` (WSL2 + containerd) with `ProcessBackend` fallback (DESIGN.md §4.6). Dev Drive / ReFS COW clones and g5d Windows compilation are not yet tested/documented. |
| **Gap — JetBrains** | ❌ | The IDE plugin targets VS Code and its forks (Cursor, Windsurf). JetBrains (IntelliJ, PyCharm, GoLand) uses a different extension model and is explicitly unsupported. No roadmap mentioned. This excludes a significant fraction of Java/Kotlin enterprise developers. |
| **Gap — Neovim / other editors** | ❌ | No plugin story. `g5d` exposes REST/WS APIs that a custom client could consume, but no integration exists. The Tier 3 fallback (synced workspace, git-level merge) works for any editor at the cost of losing CRDT collaboration. |

### Recommendation

- **Windows:** Add Windows to the COW strategy (Dev Drive / ReFS copy-on-write on Windows 11, or fall back gracefully). Ensure `g5d` compiles and runs on Windows (Rust cross-compilation is straightforward). Document the Windows developer experience explicitly.
- **JetBrains:** Even a read-only JetBrains plugin (ontology browser + agent status) would cover the "I use IntelliJ but my team uses G5" case. Phase 3+.

---

## 2. Fast Start on an Existing Project

### The scenario

A developer has a working project — dependencies installed, build passing, maybe a dev server running. They want to add G5 to it without disrupting anything. "Don't make me re-clone, re-install, or re-configure."

### How G5 handles it

The onboarding path is: `g5d init [path]` → creates `.g5/workspace.yaml` (local-only by default) → `g5d add <repo-url>` (or point at the existing checkout). The analysis pipeline then runs uplift to produce the spec tree.

Critically, G5 works from **native build files** (Cargo.toml, go.mod, package.json, pom.xml, etc.) — it does not require a proprietary project config. The per-repo `g5.yaml` is optional and agents update it as they learn the project. Staleness tracking means subsequent uplift runs are incremental — only changed files are re-analyzed.

| Axis | Rating | Notes |
|------|--------|-------|
| **Parity** | ⚠️ | `g5d init` on an existing folder works, but the initial uplift (full codebase analysis) can be expensive for large repos. A 200k LoC project with deep dependency graphs will take meaningful time for the first pass. |
| **Advantage** | ✅ | After initial uplift, G5 provides something no other tool does: a navigable ontology of the codebase with hierarchical specs, dependency visualization, and scoped agent access. This is a significant ramp-up accelerator for new team members. |
| **Gap — Analysis cost** | ⚠️ | The first uplift is all-or-nothing on the start node's subtree. There's no "uplift just this one file to try it out" quick-start. The staleness model helps on subsequent runs, but the cold-start cost may deter adoption. Consider a `g5d quickstart` that uplifts a user-selected subset (e.g., the files currently open in the IDE). |
| **Gap — Monorepo scale** | ⚠️ | Analysis runs inside a single worker. For very large monorepos (>1M LoC, thousands of packages), the analysis phase may need parallelization across workers. The current design is serial within the bootstrap worker. |
| **Gap — Build environment** | ⚠️ | G5 assumes the build environment is either already set up (local) or provided by the worker image (cloud). There's no devcontainer / Nix / asdf integration for reproducible environments. A developer whose project needs specific tool versions (e.g., Node 18, not 20) must configure this themselves. |

### Recommendation

- **Quick-start mode:** `g5d uplift --file src/auth/handler.rs` — uplift a single file and its direct dependencies. Low cost, high signal, good first impression.
- **Progress feedback:** The analysis pipeline should stream progress to the UI ("Analyzing 347/892 files…") rather than blocking silently.
- **Devcontainer integration:** If the project has a `.devcontainer/devcontainer.json`, g5d should read it for tool versions, environment variables, and post-create commands. This is the industry standard for reproducible environments.

---

## 3. Merge Workspace to Parent / Branch Integration

### The scenario

An agent completed work on a task branch. The developer reviews the diff, approves it, and wants to merge it into the project branch. Later, the project branch merges into main. This is the bread-and-butter of any git-based workflow.

### How G5 handles it

G5's model is explicit and human-controlled (Principle P3):

1. **Task branches** (`task/<task-id>`) are created per agent task. On completion, changes are committed and pushed.
2. **Merge is always human-triggered.** The UI shows the diff; the developer clicks merge.
3. **Workspace branches** can be `pinned` (fixed HEAD, working branches) or `tracking` (auto-advances to remote HEAD). Editing on a tracking branch auto-creates a new pinned branch.
4. **`g5d commit` + `g5d push`** commits CRDT drafts to git across all modified repos, then pushes.
5. **SCM integration** (Phase 2) adds PR creation from merged task branches via GitHub/GitLab webhooks.

Agent terminals block destructive git commands (`checkout`, `switch`, `reset`, `stash`, `merge`, `rebase`, `restore`) — agents cannot corrupt branch state. Read-only git (`log`, `show`, `blame`, `diff`) is allowed.

| Axis | Rating | Notes |
|------|--------|-------|
| **Parity** | ✅ | The merge model maps directly to standard git workflows. Task branch → PR → merge is how most teams already work. |
| **Advantage** | ✅ | Automatic task branch isolation means agents can never accidentally push to main. The "every task is a branch" model is stricter than most CI setups — it's a safety net, not a constraint. |
| **Advantage** | ✅ | The CRDT layer means human edits and agent edits to the same branch are conflict-free at the character level. No more "agent and I edited the same file, now I have merge conflicts." |
| **Gap — Multi-task merge** | ⚠️ | When a batch of 20 downlift tasks complete, each on its own branch, the merge-into-project workflow is not deeply specified. Is it 20 individual merges? A single squash? Automatic octopus merge? The UI needs a "merge all completed tasks" action with conflict detection. |
| **Gap — Cross-repo merge** | ⚠️ | Workspaces can span N repos. A task that modifies files across repos produces commits in multiple repos. Coordinating atomic multi-repo merges (both repos merge or neither does) is a hard problem. The `workspace.lock` file pins branches+commits, but the merge UX for multi-repo changes is unspecified. |
| **Gap — Merge conflict UX** | ⚠️ | If two task branches touch the same file, the CRDT resolves for concurrent CRDT-tracked edits, but post-commit (git-level) merges use standard 3-way merge. The merge conflict resolution UI is not specified — the design says "zero custom merge UI" (description.md) which means deferring to git/IDE tooling. This is acceptable but should be documented as intentional. |

### Recommendation

- **Batch merge UI:** "Review & merge all N completed tasks" with a combined diff view and conflict pre-check.
- **Multi-repo atomicity:** Document the merge contract for cross-repo tasks. Consider a `workspace merge` command that orchestrates multi-repo commits with rollback on partial failure.
- **Conflict preview:** Before merging a task branch, show "this branch conflicts with tasks X and Y" so the developer can sequence merges intentionally.

---

## 4. Team Collaboration on a Shared Project

### The scenario

A team of 5–15 developers works on the same codebase. They need: concurrent editing without stepping on each other, shared understanding of what's being worked on, code review, and visibility into agent activity.

### How G5 handles it

Collaboration is Principle P4 and built on Yjs CRDT:

- **Real-time co-editing:** Multiple users edit the same draft simultaneously with cursor presence (Google Docs style). Agent edits and human edits are identical Yjs operations.
- **Comment system:** Node-level comments (visible workspace-wide) and line-level comments (indexed cross-branch). Anchored via stable markers in markdown.
- **Connected viewers:** The UI shows who's online and what they're looking at.
- **Task isolation:** Each agent task gets its own branch, so concurrent agent work cannot interfere. Developers see task status in the execution view.
- **Roles:** `owner`, `admin`, `member`, `viewer` on organizations.

| Axis | Rating | Notes |
|------|--------|-------|
| **Advantage** | ✅ | The CRDT model means G5 natively supports "two developers and an agent all editing the same spec" — something no existing tool does cleanly. |
| **Advantage** | ✅ | The ontology provides a shared map of the system that all team members navigate. This replaces ad-hoc "where is the auth module?" Slack questions. |
| **Parity** | ✅ | Code review via task branch diffs is standard. The execution view adds agent activity transparency that GitHub PRs lack. |
| **Gap — Notifications** | ✅ | Notification system is implemented with dynamic delivery channels (email, Slack, WhatsApp, webhook) — configurable via the Settings panel with add/remove/toggle per channel. UI shows typed notifications (job_complete, ai_input, mention, comment) with actions. WS event type `notification` is defined in API_AND_STORAGE.md §4.2 with severity levels, categories, and actions. Backend integration for actual delivery is Phase 2+. |
| **Gap — Permissions granularity** | ⚠️ | Roles are org-level (owner/admin/member/viewer). There's no per-project or per-branch permission model. Enterprise teams often need "this team can only edit specs in the payments/ subtree." |
| **Gap — Activity feed** | ⚠️ | No cross-project activity feed ("what happened while I was away?"). The task timeline shows per-task history, but a team-level digest (completed tasks, blocked tasks, merged branches) is not specified. |

### Recommendation

- **Notification delivery (Phase 2):** The UI for configuring notification channels is in place. Wire the backend delivery pipeline (Slack webhooks, email, etc.) to make them functional.
- **Per-subtree permissions:** Allow scoping write access to ontology subtrees. This maps to team ownership boundaries ("the payments team owns payments/*").
- **Daily digest:** Auto-generated summary of agent activity across the project — what was uplifted, downlifted, blocked, merged.

---

## 5. Enterprise CI/CD and Build Pipeline Integration

### The scenario

A corporation has Jenkins/GitHub Actions/GitLab CI pipelines, artifact registries, staging environments, and compliance requirements. G5 must fit into this ecosystem, not replace it.

### How G5 handles it

G5's build integration is pragmatic:

- **Local builds:** Workers have real git worktrees, toolchain caches, and persistent shell sessions. `run_in_terminal` gives agents full shell access. Build commands come from `g5.yaml` or are discovered from native build files.
- **Build cache:** L1 (local disk) + L2 (GCS, content-addressed). Per-language cache configs for Rust (sccache), Go (GOCACHE), Python (pip), Java (Gradle/Maven), JS (pnpm store).
- **CI integration (Phase 2):** GitHub/GitLab webhook receivers for push events. The `g5` CLI is designed as a thin headless client for scripting, SSH, and CI pipelines.
- **Agent validation loop:** Agents run build+test after code generation, read errors on failure, refine, and retry. This is an inner loop — not a CI replacement.

| Axis | Rating | Notes |
|------|--------|-------|
| **Parity** | ⚠️ | G5 task branches can trigger existing CI via webhook (Phase 2), but the integration is push-based. There's no "wait for CI to pass before merging" gate in the current design. |
| **Gap — CI gate** | ⚠️ | Enterprise merge policies ("all checks must pass") need G5 to understand CI status. The task lifecycle doesn't model external CI results — a task is `COMPLETED` when the agent says so, not when CI agrees. |
| **Gap — Artifact registries** | ⚠️ | No integration with container registries, npm/PyPI private registries, or Maven repositories for publishing. Workers can pull from them (outbound egress is unrestricted), but there's no structured publish step. |
| **Gap — Secrets management** | ⚠️ | Environment variables and secrets are configured per-worker via K8s secrets, but there's no vault integration or per-project secret scoping in the G5 data model. Enterprise teams need "this project's API keys are in HashiCorp Vault." |
| **Gap — Compliance / audit trail** | ✅ | Every agent reasoning step, tool call, and result is persisted as a `SessionMessage`. This is a strong compliance story — you can replay exactly what the agent did, why, and what it changed. The audit trail is a genuine advantage over "developer committed this; we don't know why." |

### Recommendation

- **CI status integration:** Add a `checks` field to task/branch status. Merge UI shows "CI: passing / failing / pending." Task completion can optionally gate on external CI.
- **Secret references:** Allow `g5.yaml` to reference external secret providers (`secrets.vault.path: "secret/data/payments/api-key"`). Workers resolve at runtime.
- **Artifact publish hooks:** Post-merge hooks that trigger publish pipelines (Docker build+push, npm publish, etc.).

---

## 6. Offline / Disconnected / Air-Gapped Development

### The scenario

A developer is on an airplane, behind a corporate firewall, or in a classified environment without internet access. They still need to write code.

### How G5 handles it

G5 is **online-first**. Agent execution, task orchestration, and multi-user collaboration all require `g5-api`. However, the architecture degrades gracefully:

| Tier | Surface | Offline Editing | Offline Agents |
|------|---------|----------------|----------------|
| **Web UI** | Browser | ❌ No local state | ❌ |
| **VS Code + Yjs (Phase 3)** | IDE plugin as Yjs peer | ✅ Local Yjs doc, auto-merges on reconnect | ❌ |
| **VS Code + g5d checkout (Phase 2)** | Any editor with synced files | ⚠️ Real files on disk, not CRDT-tracked | ❌ |
| **CLI** | Headless | ❌ | ❌ |

The key insight: once the Yjs extension (Phase 3) ships, offline editing is free — the CRDT model handles reconnection merge automatically. Until then, Tier 3 (files on disk, git-level merge on reconnect) is serviceable but loses real-time collaboration safety.

| Axis | Rating | Notes |
|------|--------|-------|
| **Gap — Offline agents** | ❌ | No local-only agent execution path. Even with `g5d --worker` running locally, agents require `g5-api` for task state, session persistence, and spec access. |
| **Gap — Air-gapped** | ❌ | Classified / air-gapped environments need fully self-contained deployment. The architecture supports "customer-managed" mode (on-prem K8s + S3-compatible storage), but the dependency on external LLM providers (Anthropic, OpenAI) is a hard blocker. No local LLM option is documented. |
| **Parity — Airplane mode** | ⚠️ | With Phase 3 Yjs, editing offline is fine. But you can't run agents, check task status, or see collaborators. This is acceptable — no tool offers offline AI agents. |

### Recommendation

- **Local LLM support:** The `vllm` provider already exists in the LLM client layer. Document a "fully local" deployment: `g5d` + `g5-api` + local Postgres + local vLLM with an open model (Llama, CodeLlama, etc.). This unlocks air-gapped and airplane use.
- **Offline spec browsing:** Cache the spec tree locally so developers can at least navigate the ontology offline, even if they can't run agents.

---

## 7. Onboarding a New Team Member

### The scenario

A new developer joins the team. They need to understand a large codebase, find the module they'll be working on, and start contributing. Today this takes days/weeks of reading code, asking questions in Slack, and getting lost in unfamiliar directory structures.

### How G5 handles it

This is where G5's ontology-first design creates genuine differentiated value:

1. **Navigable spec tree:** The ontology viewer presents the codebase as a hierarchical, human-readable structure. Each node has a natural-language description, interface summary, and implementation hints.
2. **Scoped chat:** The new developer can open a chat scoped to any ontology node and ask "what does this module do?" — with the agent having full context of the specs and source files.
3. **Dependency visualization:** The neighborhood graph shows how a module connects to its dependencies and dependents.
4. **Execution history:** Task timelines show what agents (and humans) have done to each component — a living history of design decisions.

| Axis | Rating | Notes |
|------|--------|-------|
| **Advantage** | ✅ | No other tool provides a structured, navigable, agent-generated summary of an entire codebase. This is a 10x improvement over "read the README and grep around." |
| **Advantage** | ✅ | Scoped chat means the new developer doesn't need to formulate precise questions — they can ask vague questions scoped to a specific module and get grounded answers with spec citations. |
| **Gap — Stale specs** | ⚠️ | If the team hasn't been disciplined about re-uplifting after code changes, the specs may be stale. The staleness indicators help, but a new developer doesn't know to check them. Consider auto-flagging specs that are >N days stale. |
| **Gap — Guided walkthrough** | ⚠️ | There's no "welcome tour" or "start here" feature. The ontology is powerful but can be overwhelming for a 500-node codebase. A curated entry point ("start with these 5 modules") would help. |

### Recommendation

- **Auto-re-uplift on push:** When code is pushed, automatically re-uplift affected specs (or at least mark them stale with a visible badge). Keep the ontology fresh by default.
- **"Start here" pinning:** Let the team pin key modules as entry points for new developers. Show these prominently in the ontology viewer.

---

## 8. Migrating from Existing Tools

### The scenario

A team already uses GitHub Copilot, Cursor, Devin, or similar tools. They want to evaluate G5 without abandoning their current workflow. They also use Jira/Linear for task tracking, Confluence/Notion for documentation, and Slack for communication.

### How G5 handles it

G5's "not a replacement for Git or an IDE" positioning is helpful here — it's additive, not competitive with the editing layer. However:

| Axis | Rating | Notes |
|------|--------|-------|
| **Parity** | ✅ | G5 works alongside Copilot/Cursor. The IDE plugin adds ontology + agents without disabling existing AI features. Cursor users get both Cursor's inline completion and G5's structured agent work. |
| **Gap — Issue tracker integration** | ❌ | No Jira/Linear/GitHub Issues integration. Agents can't read tickets or update status. Tasks live entirely within G5's own system. Enterprise teams will want "create G5 task from Jira ticket" and "update Jira when task completes." |
| **Gap — Documentation import** | ❌ | No path to import existing Confluence/Notion documentation as ontology specs. Teams with extensive existing docs would need to re-enter them or rely on uplift to regenerate from code (losing the human-written design intent). |
| **Gap — Gradual adoption** | ⚠️ | The all-or-nothing uplift model means you either uplift the whole subtree or nothing. There's no "use G5 for just the auth module and ignore the rest" partial adoption path. The start node scoping helps, but the initial investment is still per-subtree, not per-file. |

### Recommendation

- **Issue tracker bidirectional sync:** Phase 2 priority. Map G5 tasks ↔ Jira/Linear issues. When an agent completes a task, update the linked ticket.
- **Spec import:** Support importing markdown documents as initial specs. Even a rough import (markdown file → spec with manual component_id assignment) dramatically lowers the adoption barrier for teams with existing design docs.
- **Per-file adoption:** The `g5d quickstart` concept from §2 directly enables gradual adoption — uplift one file at a time, build ontology coverage incrementally.

---

## Summary Matrix

| Use Case | Parity | Advantage | Key Gap | Resolution Status |
|----------|--------|-----------|---------|-------------------|
| 1. Local IDE dev | ✅ | ✅ Additive features | ❌ Windows, ❌ JetBrains | **Windows:** Resolved — DESIGN.md §4.6 specifies `Wsl2Backend` (WSL2 + containerd) as Windows fallback, `ProcessBackend` as weakest tier. **JetBrains:** Deferred (Phase 3+). |
| 2. Existing project fast-start | ⚠️ Cold start cost | ✅ Navigable ontology | ⚠️ No quick-start mode | Open — `g5d uplift --file` not yet specified. |
| 3. Branch merging | ✅ Standard git | ✅ CRDT conflict-free | ⚠️ Multi-task batch merge | Open — batch merge UX not yet specified. |
| 4. Team collaboration | ✅ CRDT co-editing | ✅ Shared ontology | ⚠️ Permissions granularity | **Partially resolved** — API_AND_STORAGE.md §2.12 adds OAuth scopes + workspace-scoped permission overrides. Per-subtree permissions still deferred. |
| 5. Enterprise CI/CD | ⚠️ Phase 2 | ✅ Audit trail | ⚠️ CI gate, secrets | Open — CI status integration and vault references not yet specified. |
| 6. Offline / air-gapped | ⚠️ Phase 3 | — | ❌ Offline agents, ❌ local LLM | **Partially resolved** — WORKER_INFRASTRUCTURE.md §14.1 documents air-gapped deployments. Local LLM via vLLM provider exists in LLM client layer but not documented as a deployment path. |
| 7. New team member onboarding | — | ✅✅ 10x improvement | ⚠️ Stale specs | Open — auto-re-uplift on push not yet specified. |
| 8. Migration from existing tools | ✅ Additive | ✅ Works alongside | ❌ Issue tracker, ❌ doc import | Open — Jira/Linear integration and doc import not yet specified. |

---

## Top 5 Recommendations (Priority Order)

1. ~~**Windows support**~~ **Partially resolved** — DESIGN.md §4.6 specifies `Wsl2Backend` (WSL2 + containerd) with `ProcessBackend` fallback. Remaining: Dev Drive / ReFS COW clone strategy, Windows-specific testing.

2. **Quick-start / partial adoption** — `g5d uplift --file <path>` for single-file uplift. Lowers the cold-start cost and enables gradual adoption. Critical for "try G5 on one module" evaluation scenarios.

3. **Issue tracker integration** — Bidirectional Jira/Linear/GitHub Issues sync. Enterprise teams will not adopt a tool that doesn't talk to their task tracker.

4. **Batch merge UX** — "Merge all N completed tasks" with combined diff, conflict detection, and CI status. This is the daily workflow for teams using G5 agents at scale.

5. ~~**Local LLM / air-gapped deployment**~~ **Partially resolved** — WORKER_INFRASTRUCTURE.md §14.1 documents air-gapped deployments. vLLM provider exists in LLM client layer. Remaining: document the fully-local deployment recipe (`g5d` + `g5-api` + local Postgres + vLLM).
