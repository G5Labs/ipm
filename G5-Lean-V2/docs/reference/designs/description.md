# G5 — Ontology-Centric Agentic Development UI

## Purpose

**G5** is an ontology-centric development environment for large, structured software systems built with AI assistance.  
Its goal is to make **design intent the primary artifact**, while allowing **agents to execute work in isolated branches**, with **humans retaining final authority via the IDE**.

The UI is designed to coordinate:
- **projects as branches**
- **ontology nodes as executable design specifications**
- **agent executions as ephemeral, reviewable workspaces**
- **humans as supervisors and integrators**

This is *not* a replacement for Git or an IDE.  
It is an orchestration layer that makes complex agentic workflows understandable, auditable, and interruptible.

---

## Core Design Principles

1. **Ontology First**
   - Design documents *are* the ontology.
   - Ontology nodes define structure, dependencies, and source ownership.
   - Anything that can spawn work must be explicitly declared.

2. **Projects Are Branches**
   - Each project corresponds to a Git branch.
   - Projects may depend on other projects (pull-based, manual).
   - Integration always occurs via merge.

3. **Agents Propose, Humans Decide**
   - Agents work in isolated task branches.
   - Humans can pause, intervene, inspect diffs, or open the IDE at any time.
   - Final merge decisions are always explicit.

4. **IDE Is the Ground Truth**
   - Any unclear, slow, or “too magical” moment:
     - Open the branch.
     - Inspect files.
     - Edit.
     - Commit.
   - The UI never hides Git reality.

---

## Primary Views

### 1. Workspace Switcher

**Purpose:** Coordinate parallel efforts across the organization.

**What it shows:**
- Searchable, filterable workspace table
- Status filter chips: Active, Blocked, Review, Integrated, Idea
- Per-workspace summary: repos, branches, status, cost, concurrent tasks
- Aggregate stats row: total cost, total active tasks
- Task queue sidebar (all workspaces, grouped workspace → job → task)
- Dependency graph (SVG)

**Key insight:**  
Kanban belongs *between* workspaces, not within a workspace.

---

### 2. Unified Workspace (WorkspacePageV2)

**Purpose:** The single editing surface for all development work — ontology browsing,
spec authoring, code viewing, execution monitoring, chat, and settings.

**Layout:** VS Code-inspired with resizable panels:
- **Top bar**: Workspace switcher, connected worker pill, inline search (⌘K), notifications, settings, chat toggle
- **Repos bar**: All repos in the workspace with branch labels
- **Left sidebar** (resizable 200–480px): Ontology tree with ↑/↓ buttons, project docs, file browser, jobs panel
- **Center panel**: Tabbed editor (Notion-like SpecEditor, CodeEditor, DiffView, markdown renderer) + draft status bar + resizable bottom drawer (DAG, changes, timeline, terminal, intervention)
- **Right panel**: Chat with session management, tool call badges, pause control
- **Mobile**: Chat-only responsive layout below `lg` breakpoint

**What it shows:**
- Ontology tree with per-node sync/dirty indicators
- Notion-like spec editing: block-level rich editor with slash commands, frontmatter card, neighborhood graph
- Code viewing with syntax highlighting, line-anchored comment threads
- Side-by-side diff view with inline comments
- DAG execution graph with status coloring
- Agent timeline (thought/tool/output/error trace)
- Worker terminal sessions with command input
- Human intervention panel (message agent, take over mode)
- Chat scoped to workspace with session management
- 8 named themes × light/dark modes with per-theme fonts, radii, borders, textures
- Connected user cursors and Yjs sync indicator
- Spotlight search (⌘K) across specs, files, symbols, external repos, chat history

**Key insight:**
One surface does everything. No page navigation between "viewing the design"
and "monitoring the agent." The bottom drawer expands inline for deep
debugging without losing editing context.

**What the workspace answers:**
> *What is this system supposed to be, what is the agent doing about it,
> and do I agree?*

---

## Workflow Summary

1. User opens the Workspace Switcher and selects a workspace (or creates one).
2. The Unified Workspace loads with the ontology tree, connected worker, and chat.
3. User navigates the ontology and opens a spec or source file in the center panel.
4. User triggers an action (uplift ↑, downlift ↓) via hover buttons on ontology nodes.
5. System creates an **isolated task branch** and assigns it to the workspace's worker.
6. User monitors execution in the bottom drawer (DAG, timeline, terminal, changes).
7. User may:
   - message the agent via the Intervene tab
   - pause and open the branch in their IDE
   - modify code directly
   - take over mode for manual control
8. User reviews diffs inline and merges the task branch.
9. Workspace may later be merged upstream.

---

## Major Integration Points

### Version Control (Git)
- Projects = branches
- Tasks = ephemeral sub-branches
- Diff vs base is Git-backed
- Merge is explicit and human-controlled

### IDEs
- Embedded VS Code or external IDE
- Always opens the task workspace branch
- IDE is authoritative for final changes

### LLM / Agent Runtime
- Agents read ontology + source files
- Agents write only within their workspace
- Tool calls are structured and auditable
- No hidden propagation or background merges

### Markdown & Mermaid
- Markdown is the primary design language
- Mermaid is first-class (SVG, zoom, fullscreen)
- Design diagrams must remain readable at scale

---

## What G5 Is (and Is Not)

**G5 is:**
- a coordination layer for agentic development
- a design-first system
- a way to scale complex software reasoning

**G5 is not:**
- a low-code builder
- an autonomous code generator
- a replacement for Git or an IDE

---

## One-Sentence Summary

**G5 turns ontology-driven design into a navigable, executable, and human-governed development workflow—where agents do the work, but engineers remain in control.**
