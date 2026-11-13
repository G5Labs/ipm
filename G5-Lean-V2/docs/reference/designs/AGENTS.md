# G5 Agent Architecture

> **Canonical reference for what G5 agents *are*, what they *do*, and how they *reason*.**
> Cross-references: WORKER_INFRASTRUCTURE.md §10 (session & loop mechanics), API_AND_STORAGE.md §3.4 (jobs & tasks API), FRONTEND_DESIGN.md §9.14 (task timeline).

---

## 0. Design Principles

| # | Principle | Implication |
|---|-----------|-------------|
| P0 | **Ontology is ground truth** | Every agent action is scoped to an ontology node. The spec *is* the contract; agents may not silently invent structure outside it. |
| P1 | **Specs before code** | Uplift produces specs; downlift consumes them. Code is a *derived artifact* of design intent. |
| P2 | **Dependency-ordered execution** | Tasks execute in topological order of the ontology graph. Leaf nodes first, aggregates last (uplift) or root first, leaves last (downlift). |
| P3 | **Human authority, agent labor** | Agents propose diffs on task branches. Humans review, intervene, and merge. No silent propagation. |
| P4 | **Isolation by default** | Each task runs on its own Git branch (or worktree). Concurrent tasks cannot interfere. |
| P5 | **Structured auditing** | Every reasoning step, tool call, and result is persisted as a `SessionMessage`. The agent trail is fully replayable. |
| P6 | **Single execution model** | All job types use the same ReAct loop with pluggable dialog compaction. No job type receives special control flow; differences are expressed through system prompts, TaskContext, and prepopulation. |

---

## 1. Agent Identity

A G5 **agent** is a ReAct (Reason + Act) inference loop that:

1. Receives a **TaskContext** binding it to an ontology node, its dependencies, and its source files.
2. Constructs a system prompt and **prepopulates** the dialog with the task's situational context.
3. Iteratively reasons and acts — calling tools, observing results, and deciding next steps.
4. Manages token growth via a pluggable **dialog compaction** layer, compacting older conversation turns while preserving critical context.
5. Terminates by calling `transition_task_state` (for tasks) or `respond_to_user` (for chat).

The agent has **no hidden state**. All context is in the session messages, the ontology documents, and the source files accessible via tools. The agent loop itself is specified in WORKER_INFRASTRUCTURE.md §10.5; this document defines **what the agent is told to do** — its goals, constraints, and available job types.

There is exactly **one agent architecture**: the ReAct loop with dialog compaction. All job types (uplift, downlift, review, chat) use this same loop. Differences between job types are expressed through three mechanisms:
1. **System prompt**: Task-type-specific instructions and workspace context (§5).
2. **TaskContext**: The ontology binding that scopes what the agent can read and write (§9).
3. **Prepopulation**: The phased context injection that seeds the dialog before the loop begins (§10).

### 1.1 Agent ≠ Worker

| Concept | Definition |
|---------|------------|
| **Agent** | The ReAct inference loop + system prompt + tool definitions + dialog compactor. Stateless. |
| **Worker** | The isolated environment (krunvm microVM, WSL2 container, OS process, or cloud Kata VM) that provides a filesystem, shell, and tool execution environment. |
| **Session** | The persisted dialog (messages + state) stored in the **central PostgreSQL database**. Every `SessionMessage` is written to the database as it is produced and broadcast to subscribers via WebSocket for real-time observation. |
| **TaskContext** | The structured binding between a task and the ontology: component ID, source files, parent/child specs, and task-graph dependencies. |

An agent *runs on* a worker. Every task requires a dedicated worker — there is no degraded "backend mode." Multiple sessions may execute on the same worker sequentially, but each session has exactly one agent loop at a time.

---

## 2. Job Types

G5 agents execute four categories of work:

| Job Type | Direction | Goal | Primary Artifact |
|----------|-----------|------|------------------|
| **Uplift** | Code → Spec | Produce an ontology spec from existing source files | `.g5/specs/*.md` |
| **Downlift** | Spec → Code | Produce source files satisfying a spec | Source files |
| **Review** | Code ↔ Spec | Audit codebase against ontology, report and fix issues | Diff + report |
| **Chat** | — | Open-ended dialog with tool access | Conversation |

### 2.1 Uplift (Code → Spec)

An uplift task reads source files and produces a **SemliftSpec** — a structured markdown document that captures:

- **Component identity**: `component_id` — a hierarchical, filesystem-derived identifier
- **Source mapping**: `src_files` — which source files this spec describes
- **Hierarchy**: `parent_spec` and `child_specs` — position in the ontology tree
- **Interface**: Public methods, types, module exports
- **Implementation hints**: Algorithms, patterns, constraints

Dependencies between specs are tracked in the **task graph** (derived from import analysis in §3), not in the spec metadata itself. This keeps specs simple and ensures dependency information is always computed from the source of truth — the actual import statements.

The uplift agent uses the same ReAct loop as all other job types. The agent's TaskContext is pre-populated with the source files to analyze and any dependency specs that have already been uplifted (see §10 Prepopulation). The system prompt instructs the agent to:

1. Read target source files and extract public interfaces (functions, classes, exports).
2. Synthesize a SemliftSpec in the canonical markdown+YAML-frontmatter format.
3. Validate the spec structure (frontmatter fields, content completeness).
4. Call `transition_task_state(state: "complete")` to commit and push.

The agent is free to iterate — reading additional files, searching for patterns, and refining the spec — before signaling completion. The dialog compactor (§11) manages token growth if analysis requires many file reads.

**SemliftSpec format:**

The spec metadata (YAML frontmatter) contains exactly four fields:

| Field | Type | Description |
|-------|------|-------------|
| `component_id` | `str` | Hierarchical, filesystem-derived ID (e.g., `auth/token_validator`) |
| `src_files` | `list[str]` | Source files this spec describes |
| `parent_spec` | `str?` | Parent spec in the ontology hierarchy (`null` for root) |
| `child_specs` | `list[str]` | Child spec IDs (for module/workspace aggregation specs) |

```markdown
---
component_id: "auth/token_validator"
src_files: ["src/auth/validator.rs", "src/auth/claims.rs"]
parent_spec: "auth"
child_specs: []
---

# Token Validator

## Description
Validates JWT tokens against the user store and enforces claim-based authorization.

## Interface
### Public Functions
- `validate_token(token: &str) -> Result<Claims, AuthError>`
- `refresh_token(refresh: &str) -> Result<TokenPair, AuthError>`

## Implementation Hints
- Use RS256 signature verification
- Cache validated tokens for 60s with LRU eviction
- Reject tokens with `exp` in the past (no clock skew tolerance)
```

All other information — dependencies, language, complexity, type — is derived from the task graph and codebase analysis rather than stored in the spec. The spec body (markdown below the frontmatter) is free-form natural language describing the component's interface, behavior, and implementation guidance, written in the **Extended Markdown dialect** (DESIGN.md §2.5). Agents must use the canonical formatting rules (ATX headings, fenced code, 2-space list indent, etc.) so that `serialize(parse(md))` is idempotent — this prevents save churn in the rich editor and keeps git diffs clean.

**Ordering constraint**: Uplift runs **leaf-first** (bottom-up topological order). Files with no dependencies are uplifted first; their specs become context for uplifting their dependents. When a file-level task completes, the task executor checks whether all siblings under the same parent module are also `COMPLETED`; if so, it schedules the parent aggregation task (which was pre-created during analysis, §3.3, with dependencies on all its children). This cascades upward — file specs seed module summaries, which seed the workspace summary — producing a **hierarchical summarization** of the codebase.

### 2.2 Downlift (Spec → Code)

A downlift task reads a SemliftSpec and produces source code satisfying the specification.

The agent's dialog is prepopulated with the spec (phase 0 — the "source of truth" for code generation), dependency specs (from the task graph), and any existing source files (see §10). The agent then enters the ReAct loop:

1. Reason about implementation approach based on the spec and dependency contracts.
2. Generate code, write files to the exact paths listed in `src_files`.
3. Validate (compile/test via `run_in_terminal`).
4. If validation fails, read errors, refine, and retry.
5. Call `transition_task_state(state: "complete")` to commit and push.

**Failure handling:** If the agent cannot make the build/tests pass after exhausting its reasoning budget, it calls `transition_task_state(state: "fail", error: "...")`. The task enters `FAILED` and the dirty tree is preserved. If retries remain, the system transitions the task back to `PENDING` with a backoff delay; when re-scheduled to `RUNNING`, the dirty tree is intact so the next agent pass can inspect it, read compiler errors, and continue from where the previous attempt left off.

**Blocking:** If the agent encounters an ambiguity in the spec or a missing dependency that it cannot resolve, it calls `transition_task_state(state: "blocked", reason: "...")`. The task pauses and surfaces the reason to human reviewers.

**Hierarchical decomposition**: A root spec may specify `child_specs`. The downlift agent can generate child specs during its run — writing new spec files under `.g5/specs/` that decompose the parent spec into smaller units. On completion, the hook (§6.5) scans for newly created specs and creates corresponding downlift tasks in the graph. These new tasks enter `PENDING` and execute in dependency order as their prerequisites complete. This allows top-down decomposition: a root spec generates module specs, which generate file specs, each producing code on completion.

**Workspace isolation**: Each downlift task operates on its own task branch in a Git worktree (see WORKER_INFRASTRUCTURE.md §10). On `complete`, changes are committed and pushed atomically. On `fail` or `blocked`, the dirty worktree is preserved on the task branch (see §4, dirty tree preservation mechanism).

**Validation & refinement loop:**

```
┌─────────────┐     ┌────────────────┐     ┌───────────────┐
│  Generate   │───▶│  Validate      │───▶│  Complete      │
│  code       │     │  (terminal)    │     │  (commit+push) │
└─────────────┘     └───────┬────────┘     └───────────────┘
                            │ FAIL
                            ▼
                    ┌────────────────┐
                    │  Read errors,  │
                    │  refine code   │──── loops within agent turn
                    └────────┬───────┘
                            │ still failing
                            ▼
                    ┌────────────────┐
                    │  fail (dirty   │
                    │  tree kept)    │──── orchestrator retries
                    └────────────────┘     with dirty tree
```

**Ordering constraint**: Downlift runs **root-first** (top-down). The root spec is processed first to establish overall structure; child specs are processed after their parent.

### 2.3 Review

A review task is told to audit the codebase against ontology specs and correct issues. Unlike uplift/downlift which transform between code and specs, review operates bidirectionally:

- **Spec → Code review**: Does the code satisfy the spec's contracts?
- **Code → Spec review**: Does the spec accurately reflect the current code?
- **Cross-cutting review**: Are interfaces consistent across module boundaries? Does the task graph's dependency structure match actual imports?

The review agent has the full tool set (read/write files, run tests, git commit) and produces:
1. A structured report of findings.
2. Diffs correcting any issues found.
3. Updated specs if the code has drifted.

Review tasks are typically triggered manually by a user selecting ontology nodes and requesting an audit, or automatically after a batch of downlift tasks complete.

### 2.4 Chat

Chat is open-ended dialog with the user, scoped to a workspace and optionally to an ontology node. The chat agent has tool access and can:

- Answer questions about the codebase by reading files.
- Explain ontology specs and their relationships.
- Draft code snippets (as artifacts in the conversation).
- Propose ontology changes (but not commit them without explicit approval).

Chat uses the same agent loop and session infrastructure as task-based work. The difference is:
- No task branch is created automatically (though the user can request one).
- The agent uses `respond_to_user` instead of `transition_task_state` to continue dialog.
- Budget limits are lower (shorter max turns).
- The system prompt emphasizes helpfulness over task completion.

---

## 3. Codebase Analysis — Populating the Task Graph

Before any uplift or downlift can run, G5 must **understand the codebase's structure and dependency graph**. This is the analysis phase, and it produces the task DAG that drives all subsequent work.

### 3.1 The Problem

Large codebases (~200k LoC) contain hundreds of source files with complex dependency relationships. To uplift them into ontology specs:
1. We must process files in **dependency order** (leaf-first).
2. Each file's spec must reference its dependency specs.
3. We need hierarchical aggregation (file → module → workspace).
4. We must handle **mixed-language projects** (e.g., Python + Go in the same repo) where each language has its own import semantics.

Analyzing these relationships requires understanding each language's import/include system, resolving paths to actual project files (not external libraries), detecting and breaking cycles, and producing a unified task DAG across all languages.

### 3.1.1 IDService — Component ID Generation

The `IDService` converts file paths to hierarchical `component_id` strings used throughout the ontology and spec system. Component IDs are the universal identifier for ontology nodes — every spec, task, and dependency edge references them.

**Interface:**
```python
class IDService:
    def __init__(self, project_root: str): ...

    def file_to_component_id(self, file_path: str) -> str:
        """Convert a file path to a component_id.
        
        Rules:
        1. Strip project_root prefix and file extension.
        2. Replace os.sep with '/'.
        3. Collapse 'src/main/java/', 'src/', 'lib/' prefixes.
        4. Lowercase the result.
        
        Examples:
          src/auth/validator.rs       → auth/validator
          src/main/java/com/acme/App.java → com/acme/app
          lib/utils/helpers.py        → utils/helpers
        """

    def directory_to_component_id(self, dir_path: str) -> str:
        """Convert a directory path to a module-level component_id.
        Same rules as file_to_component_id but without extension stripping."""

    def spec_path(self, component_id: str) -> str:
        """Return the spec file path for a component_id.
        Example: auth/validator → .g5/specs/auth/validator.md"""
```

Component IDs are **stable** — they derive deterministically from file paths. Renaming a file changes its component_id, which triggers a re-uplift of affected specs. The `IDService` is used by all analyzers (§3.2) during task graph construction and by the spec tools (§6) during read/write operations.

### 3.2 Import Order Simulation

The core analysis technique is **import order simulation** — walking the codebase's dependency graph as the language runtime/compiler would.

#### 3.2.1 WaterfallAnalyzer — Orchestrator

The `WaterfallAnalyzer` runs language-specific analyzers in priority order:

```python
analyzers = [
    CppAnalyzer,           # 1st: C/C++ (#include)
    RustAnalyzer,          # 2nd: Rust (use/mod)
    JavaAnalyzer,          # 3rd: Java (import)
    GoAnalyzer,            # 4th: Go (import)
    PythonAnalyzer,        # 5th: Python (import/from)
    TypeScriptAnalyzer,    # 6th: TypeScript (import/require)
    JavaScriptAnalyzer,    # 7th: JavaScript (import/require)
    GenericFileAnalyzer,   # Last: Catch-all (no dependency extraction)
]
```

**Deduplication**: A set of claimed file paths ensures each file is analyzed by exactly one analyzer — the first one in priority order whose `supported_extensions` match. The `GenericFileAnalyzer` runs last and catches files no language-specific analyzer claimed (e.g., `.proto`, `.sql`, config files).

**Mixed-language projects**: Each analyzer runs independently on its own file set. Dependencies are **within-language** at the file level — a Python file that calls a Go binary via subprocess does not create a task-graph edge. Cross-language coupling is captured at the **module aggregation** level (§3.3), where files from different languages in the same directory are grouped under a shared aggregate.

**Result**: A list of `TaskNode` objects, each with:
- A `component_id` (derived from the file path via `IDService`)
- A list of dependency `component_id`s (edges in the task graph)
- Language and analysis metadata
- Path to the source file (`src_files`)

#### 3.2.2 Per-Language Analyzers

Each analyzer implements the `BaseAnalyzer` interface:

```python
class BaseAnalyzer(ABC):
    @property
    def analyzer_name(self) -> str: ...
    @property
    def supported_extensions(self) -> set[str]: ...
    def can_analyze(self) -> bool: ...
    async def analyze_project(self) -> list[TaskNode]: ...
```

**Common infrastructure** provided by `BaseAnalyzer`:
- **File discovery** (`_find_project_files`): Glob by extension, excluding `__pycache__`, `node_modules`, `build`, `dist`, `.git`, `vendor`, and hidden directories. Respects `.gitignore` when present.
- **Task creation** (`_create_individual_task`): Creates a `TaskNode` with populated `TaskContext`, `SpecContext`, and generated spec/task IDs.
- **Aggregation helpers**: Shared methods for creating MODULE/PROJECT aggregation tasks and computing aggregate dependencies (§3.3).

---

**Python** — `PythonAnalyzer`

| | |
|---|---|
| **Extensions** | `.py`, `.pyx`, `.pyw` |
| **Detection** | `setup.py`, `pyproject.toml`, `requirements.txt`, `__init__.py`, or any `.py` file |
| **Import parsing** | AST-based (`ast.parse`). Processes `import X` and `from X import Y` at module top level (column offset 0 — skips `TYPE_CHECKING` blocks). Handles relative imports (`from . import`, `from .. import`) by computing base module from file position. |
| **Resolution** | Builds a `module_name → spec_id` map from all project Python files. Resolution chain: exact match → prefix match → package `__init__` → parent module. Only resolves to project-internal files; stdlib and third-party imports are ignored. |
| **Cycle breaking** | `PythonImportSimulator`: DFS from each import target to check if source is reachable. If including the edge would create a cycle, the dependency is removed. |

---

**C/C++** — `CppAnalyzer`

| | |
|---|---|
| **Extensions** | `.c`, `.cc`, `.cpp`, `.cxx`, `.h`, `.hpp`, `.hxx` |
| **Detection** | Any C/C++ source or header file in project |
| **Import parsing** | Regex-based `#include` extraction. Distinguishes `<system>` (skipped) vs `"local"` includes. Maintains an extensive system-header skip list (C/C++ stdlib, Boost, Eigen, OpenCV, Qt, etc.). |
| **Resolution** | `IncludePathResolver`: (1) relative to source file dir, (2) configured include directories, (3) relative to project root, (4) fallback full-project filename search via prebuilt path index. |
| **Cycle breaking** | `IncludeGuardSimulator`: Simulates `#pragma once` / include guard behavior via DFS with processing stack. |
| **Special** | `CMakeParser` available for extracting `target_link_libraries` and `include_directories` from `CMakeLists.txt` — used to seed the include path resolver. |

---

**Rust** — `RustAnalyzer`

| | |
|---|---|
| **Extensions** | `.rs` |
| **Detection** | `Cargo.toml` or any `.rs` file |
| **Import parsing** | Regex/tree-sitter extraction of `use crate::`, `use super::`, `mod` declarations, and `#[path = "..."]` attributes. Only `crate::`-prefixed and relative (`super::`, `self::`) paths produce internal dependencies; `use std::`, `use external_crate::` are ignored. |
| **Resolution** | Module path → file path following Rust's module resolution rules: `mod foo` resolves to `foo.rs` or `foo/mod.rs`. Crate root is `lib.rs` or `main.rs`. `use crate::auth::validator` → `src/auth/validator.rs`. |
| **Cycle breaking** | Rust's module tree is inherently acyclic (a file can only be declared as a module once). `use` cycles are possible but rare; DFS cycle detection with edge removal. |
| **Special** | Workspace-aware: parses `Cargo.toml` `[workspace]` members to scope analysis to the correct crate roots. |

---

**Java** — `JavaAnalyzer`

| | |
|---|---|
| **Extensions** | `.java` |
| **Detection** | `pom.xml`, `build.gradle`, `build.gradle.kts`, or any `.java` file |
| **Import parsing** | Regex extraction of `import com.example.Foo;` and `import static com.example.Foo.bar;`. Wildcard imports (`import com.example.*;`) are resolved to all files in the matching package. |
| **Resolution** | Package path → file path: `import com.example.auth.Validator` → search for `com/example/auth/Validator.java` under detected source roots (`src/main/java/`, `src/`, or project root). Source roots detected from build tool config or directory structure heuristics. Imports not matching any project file are treated as external (ignored). |
| **Cycle breaking** | DFS cycle detection with edge removal. Java's package system allows circular imports; the analyzer breaks cycles at the edge that would close the loop. |
| **Special** | Multi-module Maven/Gradle: parses `pom.xml` `<modules>` or `settings.gradle` `include` to discover subprojects. Each subproject is analyzed independently with its own source root. Cross-module dependencies tracked when an import resolves to a file in a sibling module. |

---

**Go** — `GoAnalyzer`

| | |
|---|---|
| **Extensions** | `.go` |
| **Detection** | `go.mod`, `go.sum`, or any `.go` file |
| **Import parsing** | Regex extraction of `import "path"` and `import (...)` blocks. Skips stdlib imports (no `.` in path or matches known `golang.org/` packages). Only module-internal imports (matching the module path from `go.mod`) produce dependencies. |
| **Resolution** | Import path → directory: `import "github.com/user/project/auth/validator"` with module path `github.com/user/project` → `auth/validator/` directory → all `.go` files in that directory (Go's compilation unit is the package/directory, not individual files). The dependency edge points to the directory-level aggregate, not individual files. |
| **Cycle breaking** | Go forbids circular package imports at the compiler level, so cycles should not occur in valid code. The analyzer still runs DFS detection as a safety net for incomplete/broken codebases. |
| **Special** | `_test.go` files are discovered but analyzed separately — test file dependencies on the package under test are implicit (same package) rather than import edges. Build tags (`//go:build`) are not evaluated; all files are included. |

---

**TypeScript** — `TypeScriptAnalyzer`

| | |
|---|---|
| **Extensions** | `.ts`, `.tsx` |
| **Detection** | `tsconfig.json`, `package.json`, or any `.ts`/`.tsx` file |
| **Import parsing** | Regex extraction of `import ... from '...'`, `import '...'`, `require('...')`, and dynamic `import('...')`. Only relative paths (`./`, `../`) and `tsconfig.json` path aliases (`@/auth/validator`) produce internal dependencies. Bare specifiers (`import 'lodash'`) are ignored as external. |
| **Resolution** | Relative path → file: `import { validate } from './auth/validator'` → try `./auth/validator.ts`, `./auth/validator.tsx`, `./auth/validator/index.ts`, `./auth/validator/index.tsx`. Path aliases resolved by parsing `tsconfig.json` `compilerOptions.paths` and `baseUrl`. Monorepo-aware: follows `references` in `tsconfig.json` for project references. |
| **Cycle breaking** | DFS cycle detection with edge removal. Barrel re-exports (`index.ts`) are common cycle sources and are handled by treating `index.ts` imports as directory-level references. |

---

**JavaScript** — `JavaScriptAnalyzer`

| | |
|---|---|
| **Extensions** | `.js`, `.jsx`, `.mjs` |
| **Detection** | `package.json`, or any `.js`/`.jsx`/`.mjs` file |
| **Import parsing** | Same regex patterns as TypeScript: `import`/`export` (ESM), `require()` (CJS), dynamic `import()`. Only relative paths and configured aliases produce internal dependencies. |
| **Resolution** | Same resolution strategy as TypeScript but without `tsconfig.json` paths. Checks `package.json` `imports` field for subpath imports. Extension resolution: `./foo` → try `.js`, `.jsx`, `.mjs`, `/index.js`, `/index.jsx`. |
| **Cycle breaking** | DFS cycle detection with edge removal. |
| **Special** | Shares core resolution logic with `TypeScriptAnalyzer` via a common `JSModuleResolver` base. |

---

**Generic fallback** — `GenericFileAnalyzer`

| | |
|---|---|
| **Extensions** | All extensions from every other analyzer, plus `.kt`, `.rb`, `.php`, `.swift`, `.scala`, `.proto`, `.sql` |
| **Detection** | Always returns `True` — runs last in the waterfall |
| **Import parsing** | None. File discovery only. |
| **Resolution** | N/A |

Files caught by the generic analyzer have no dependency edges — they are leaves in the task graph and can be uplifted in any order. This is the safe fallback for unsupported languages; the agent's LLM can still read and understand the file content during uplift even without structural dependency information.

#### 3.2.3 File Filtering

All analyzers share a common exclusion policy via `BaseAnalyzer._find_project_files()`:

**Excluded directories** (always skipped):
- Hidden directories (`.git`, `.idea`, `.vscode`, etc.)
- Build outputs: `build/`, `dist/`, `target/`, `out/`, `cmake-build-*/`, `bin/` (when not a source root)
- Dependency caches: `node_modules/`, `__pycache__/`, `.tox/`, `.venv/`, `venv/`, `vendor/` (for Go/PHP)
- Generated: `generated/`, `gen/`

**`.gitignore` integration**: When a `.gitignore` file exists at the project root, its patterns are parsed and applied to filter discovery results. This catches project-specific exclusions (e.g., `data/`, `*.generated.ts`) that the hardcoded list misses.

**Test files**: Included in analysis by default — test files have dependencies on production code and should appear in the task graph. The agent's system prompt can instruct selective handling (e.g., skip uplift for test files, or generate test specs separately).

#### 3.2.4 Cycle Resolution

All language analyzers that extract dependencies must handle circular dependencies, since many languages permit them (Python, Java, TypeScript) and even languages that forbid them (Go, Rust) may have broken or in-progress code.

The common pattern across all analyzers:

1. Build the full dependency graph (potentially with cycles).
2. Run DFS from each node; if a back edge would close a cycle, remove it.
3. Preference: remove the edge that breaks the cycle with minimal information loss — typically the edge from the *later-discovered* node (the one deeper in the DFS stack).

Language-specific refinements:
- **Python**: `PythonImportSimulator` checks reachability bidirectionally — if A imports B and B transitively imports A, the weaker edge (fewer transitive dependents) is removed.
- **C/C++**: `IncludeGuardSimulator` simulates `#pragma once` / include guards, which naturally break re-entrant includes without removing edges.
- **Go**: Cycles indicate invalid code; the analyzer emits a warning and removes the back edge.

### 3.3 Hierarchical Aggregation

After individual file tasks are created, the analyzer builds **aggregation tasks** that summarize directory-level modules and the workspace root:

```
                    ┌──────────────────────┐
                    │  project_root        │  ◄── PROJECT level
                    │  (aggregates all)    │
                    └──────────┬───────────┘
                               │
                ┌──────────────┼──────────────┐
                ▼              ▼              ▼
        ┌──────────┐   ┌──────────┐   ┌──────────┐
        │ src/auth  │   │ src/api  │   │ src/db   │  ◄── MODULE level
        └─────┬────┘   └─────┬────┘   └─────┬────┘
              │              │              │
         ┌────┼────┐    ┌───┼───┐      ┌───┼───┐
         ▼    ▼    ▼    ▼   ▼   ▼      ▼   ▼   ▼
        [files]   [files]  [files]    [files]         ◄── FILE level
```

Each aggregation task:
- Depends on all child specs (file-level or sub-module-level).
- Has `node_level: MODULE` or `PROJECT`.
- Is processed *after* its children in uplift (bottom-up).
- Produces a summary spec that captures the module's collective interface and purpose.

**Pruning**: Trivial aggregates (≤1 child) are collapsed — their single child is promoted to the parent's child list. This prevents the hierarchy from becoming unnecessarily deep.

### 3.4 Task Graph Construction

The analysis phase produces a **directed acyclic graph (DAG)** of tasks:

```
Input: Source files + dependency relationships
Output: Task rows in the database (the "task graph")

Steps:
1. WaterfallAnalyzer.analyze_project()
   → Per-file TaskNodes with dependencies
   
2. _create_hierarchical_aggregation()
   → Module and workspace aggregation TaskNodes
   
3. _establish_parent_child_relationships()
   → parent_spec / child_specs linked
   
4. _prune_trivial_aggregates()
   → Collapse single-child modules

5. Tasks submitted to database via POST /api/internal/jobs/:id/tasks
```

> **Note:** The "task graph" is not a standalone data structure — it is the set of `tasks` rows for a job, with edges defined by each task's `depends_on` array. The server maintains the graph; the bootstrap worker populates it.

The DAG is stored in the database and exposed via the Jobs & Tasks API (API_AND_STORAGE.md §3.4). The frontend renders it as the **Task Timeline View** (FRONTEND_DESIGN.md §9.14).

### 3.5 Analysis in G5 vs Semlift

In the standalone semlift CLI, analysis happens locally via Python analyzers. In G5, the analysis runs inside a worker container where language toolchains are available:

| Aspect | Semlift (standalone) | G5 (worker-based) |
|--------|---------------------|-------------------|
| Execution | Local Python process | Worker Kata container |
| Language support | Python analyzers | Same analyzers (no LSP dependency) |
| Output | Task list (in-memory) | Task graph persisted to database via Task API |
| Triggers | CLI `semlift uplift` | User action in ontology view or API call |
| Scale | Single workspace | Multi-workspace, multi-tenant |

### 3.6 Job Creation

Before any task can execute, a **job** must be created. A job is the top-level unit of orchestrated work — it owns a task graph, tracks progress, and accumulates cost. Job creation is itself a worker-hosted operation: the bootstrap phase requires access to the codebase (for uplift) or the spec tree (for downlift), and may need language toolchains for accurate analysis.

**API entry point:**

```
POST /api/v1/projects/:projectId/jobs
{
  "job_type":   "uplift" | "downlift",
  "start_node": "<ontology_node_id>",   // scopes the operation (§3.7)
  "force":      false                    // skip staleness check when true
}
```

The server creates the job record, chains it behind the workspace's previous job (see API_AND_STORAGE.md §2.7), and assigns a worker for bootstrap.

**Clean worktree precondition:**

Jobs require a **clean worktree** on the target branch — no uncommitted CRDT draft state. Before the server accepts the job creation request, it checks whether any draft session on the start node's branch has pending (non-committed) changes. If dirty state exists, the API rejects the request with `409 Conflict` and a message indicating which files have uncommitted changes. The user (or agent) must either **commit** or **discard** the draft before launching the job.

This prevents a class of subtle bugs: uplift reading half-written code, downlift overwriting in-flight human edits, or the analysis phase computing dependencies against an inconsistent filesystem. The contract is simple — jobs operate on committed snapshots, drafts are for interactive editing.

The frontend enforces this proactively: when a user clicks uplift (↑) or downlift (↓) on a node whose branch has a dirty draft, a confirmation dialog offers [Commit & Launch], [Discard & Launch], or [Cancel]. This avoids the round-trip to get a 409.

**Job lifecycle:**

```
BOOTSTRAPPING ──▶ RUNNING ──▶ COMPLETED
      │                            │
      │                       (all tasks done)
      │
      ├── Worker clones repo / sets up worktree
      ├── Runs analysis (uplift) or reads spec tree (downlift)
      ├── Checks staleness, skips up-to-date nodes (unless force)
      └── Creates task DAG and persists to database
           → Job transitions to RUNNING, bootstrap worker released
```

During `BOOTSTRAPPING`, the assigned worker is responsible for seeding all tasks. No tasks are scheduled until bootstrap completes and the job enters `RUNNING`.

**Uplift bootstrap:**

1. Worker runs the analysis pipeline (§3.2–§3.4): import simulation, hierarchical aggregation, pruning — scoped to the start node's subtree.
2. For each source file under the start node, the bootstrap checks staleness (§3.7). Files whose specs are up-to-date are excluded unless `force: true`.
3. The resulting task DAG is persisted. All tasks enter `PENDING`.
4. The bootstrap worker is released. The executor begins scheduling tasks in topological order (leaf-first) as their dependencies are satisfied.

**Downlift bootstrap:**

1. Worker reads the start node's spec and its `child_specs` subtree.
2. For each spec under the start node, the bootstrap checks staleness (§3.7). Specs whose source files are up-to-date are excluded unless `force: true`.
3. For a fully-specced subtree, the bootstrap creates the entire task DAG upfront (root-first order). For a root spec with no children (greenfield), it creates a single seed task; the downlift completion hooks (§6.5) dynamically expand the graph as the agent generates child specs.
4. The bootstrap worker is released. Tasks are scheduled root-first as dependencies are satisfied.

### 3.7 Start Node & Staleness

Every job is scoped to a **start node** — an ontology node that defines the root of the operation's subtree:

- **Uplift from start node**: Uplift all descendants (leaf-first), then aggregate up to and including the start node. The start node's spec is the final output — it is "synced" when it reflects all of its children's current state.
- **Downlift from start node**: Downlift the start node's spec, then push changes down through all descendants (root-first). Each child is "synced" when its code reflects the parent spec's current intent.

**Staleness model:**

An ontology node tracks two timestamps:

| Field | Description |
|-------|-------------|
| `last_uplifted_at` | When this node's spec was last produced/refreshed by an uplift job |
| `last_downlifted_at` | When this node's code was last produced/refreshed by a downlift job |
| `content_updated_at` | When the node's content (spec or source file) was last modified by any means (human edit, agent edit, git sync) |

A node is **stale** (needs work) when:
- **For uplift**: any of its children have `content_updated_at > last_uplifted_at` on the node itself. The child's source changed after the parent last aggregated it.
- **For downlift**: its parent has `content_updated_at > last_downlifted_at` on the node itself. The parent spec changed after the child was last generated from it.

A node is **synced** (up-to-date) when it is not stale — the last uplift/downlift is more recent than all relevant changes from its children (uplift) or parent (downlift).

**Bootstrap pruning:**

During bootstrap, the job creation worker walks the start node's subtree and computes staleness for each node. Nodes that are synced are excluded from the task graph — no task is created for them. This means incremental jobs are fast: if only one file changed in a large module, only that file (and its ancestors, for aggregation) need uplift.

The `force: true` option bypasses staleness checks and creates tasks for all nodes in the subtree regardless of sync status. This is useful for full re-uplift after schema changes, tool upgrades, or when staleness data is untrusted.

**Frontend integration:**

The ontology viewer (FRONTEND_DESIGN.md §9) renders staleness as a visual indicator on each node — e.g., a colored dot (green = synced, yellow = stale). Clicking the uplift (↑) or downlift (↓) button on a node creates a job with that node as the start node. The user can see at a glance which subtrees need attention and trigger targeted operations.

---

## 4. Task Lifecycle

Tasks flow through a state machine. The agent controls transitions via `transition_task_state` (§6.5). External transitions (scheduling, retry) are managed by the task orchestrator.

```
                    ┌─────────┐
              ┌────▶│ PENDING │  (created, waiting for dependencies)
              │     └────┬────┘
              │          │ all deps completed (orchestrator)
              │          ▼
              │     ┌─────────┐
              │ ┌──▶│ RUNNING │
              │ │   └────┬────┘
              │ │        │
              │ │   ┌────┼───────────────┐
              │ │   │    │               │
              │ │   ▼    ▼               ▼
              │ │ ┌─────────┐ ┌──────────┐ ┌───────────┐
              │ │ │ BLOCKED │ │ COMPLETED│ │  FAILED   │ (transient)
              │ │ └────┬────┘ └──────────┘ └─────┬─────┘
              │ │      │                         │
              │ │      │ human sends              ├── retries remain → PENDING
              │ └──────┘ message                  │   (auto, with backoff)
              │                                   │
              │                                   └── retries exhausted → BLOCKED
              │
         CANCELLED: terminal (user aborted)
```

**Agent-initiated transitions** (via `transition_task_state`):
- `RUNNING → COMPLETED`: Agent calls `transition_task_state(state: "complete")`. All work is committed, pushed, and completion hooks run (§6.5).
- `RUNNING → BLOCKED`: Agent calls `transition_task_state(state: "blocked", reason: "...")`. Dirty tree preserved. The agent is explicitly requesting human intervention.
- `RUNNING → FAILED`: Agent calls `transition_task_state(state: "fail", error: "...")`. Dirty tree preserved. The system will auto-retry if retries remain.

**Orchestrator-initiated transitions** (not agent-controlled):
- `PENDING → RUNNING`: All dependency tasks reached `COMPLETED`.
- `BLOCKED → RUNNING`: Human attaches to the session and sends a message (§6.5 blocked contract). The injected message appears in the agent's dialog and the loop resumes.
- `FAILED → PENDING`: Retries remain; task re-enters `PENDING` immediately (with backoff delay). When the delay expires and dependencies are satisfied, the normal `PENDING → RUNNING` transition fires with the dirty tree + error context intact.
- `FAILED → BLOCKED`: Retries exhausted; the task escalates to `BLOCKED`, requiring human intervention. The dirty tree is preserved.
- `RUNNING → CANCELLED`: User aborts via control signal.

**Key invariants:**
- `COMPLETED` is the only state where the remote has the agent's work. All other states keep changes local (in the CRDT draft).
- `BLOCKED` is the only non-terminal failure state — it always means "waiting for human intervention." A task reaches `BLOCKED` either because the agent explicitly requested it or because auto-retries were exhausted.
- `FAILED` is transient. The system either auto-retries (→ `PENDING`) or escalates (→ `BLOCKED`). A task never stays in `FAILED`.
- `CANCELLED` is terminal. The user aborted the task.

**Dirty tree preservation mechanism:**

Each task operates on its own **task branch** (e.g., `task/<task-id>`) with an associated **CRDT branch draft session** (WORKER_INFRASTRUCTURE.md §11.7). The draft session is a Yjs document hosted on g5d and persisted to the central database. It is the single source of truth for the task's uncommitted file state.

When the agent writes a file (via `write_file`, `edit_file_replace`, etc.), the change is applied to the CRDT draft — the same draft that users edit when they type in Monaco. Agent edits and human edits are identical CRDT operations: both flow through Yjs, both are visible to all participants in real time, and both are durable the instant they reach the server.

The worker's local filesystem is a **materialization** of the draft. It exists for tool execution (`run_in_terminal`, etc.) but is not the durable state. The draft is. Materialization uses atomic writes (temp + rename) with retry and failure notification — see WORKER_INFRASTRUCTURE.md §11.18 for the full contract.

| Scenario | What happens | Resume cost |
|----------|-------------|-------------|
| **Worker still assigned** (hot) | Draft on server, files on local disk. Agent loop resumes in-place. | Instant |
| **Worker releases task** (blocked timeout, crash) | Draft remains on server — nothing to save. Worker is freed. | Cold start: new worker materializes draft onto disk |
| **`complete` transition** | Draft is materialized, committed, and pushed to remote. Draft resets. | N/A — work is done |

Because the draft is server-authoritative and persisted:
- **No data loss on worker crash** — every edit is durable as soon as it reaches g5d.
- **No stash branches or WIP commits** — the CRDT draft replaces all ad-hoc persistence mechanisms.
- **Seamless worker migration** — a new worker materializes the draft and continues. The agent sees no difference (dialog history is in the database, file state is in the draft).
- **Live collaborative editing** — any user who attaches to the task (from any browser/desktop) sees agent edits in real time via Yjs sync. Multiple attached users see each other's cursors and edits. The agent and humans edit the same document simultaneously — Google Docs–style coauthoring on the task branch.
- **Cross-device consistency** — the draft is a shared, durable, URL-addressable resource. Opening the same task URL from a second laptop shows identical state.

**Worker-side sync model:**

The worker participates in the CRDT protocol as a peer, but with async/batched synchronization rather than keystroke-level real-time sync:

- **Worker → Server (outbound):** When a mutation tool (`write_file`, `edit_file_replace`, etc.) executes, it writes to the local filesystem *synchronously* (so subsequent `run_in_terminal` calls see the change immediately) and enqueues a CRDT delta for the server. Deltas are flushed to g5d in batches (e.g., after each tool call completes, or on a short timer). The agent loop does not block on server acknowledgment.
- **Out-of-band file changes:** Commands run via `run_in_terminal` (builds, code generators, formatters) may create or modify files outside the mutation tool path. After each `run_in_terminal` call completes, the worker runs a lightweight filesystem diff (comparing mtimes/hashes against the last known state) to detect changed files. Detected changes are fed into the CRDT layer as deltas, just like tool-originated writes. This keeps the draft consistent even when the agent uses shell commands that produce side effects. The diff is scoped to the workspace directory and excludes build output directories (e.g., `target/`, `node_modules/`, `dist/`) to avoid noise.
- **Server → Worker (inbound):** When a human edit reaches g5d, the server pushes a notification to the worker (via the existing WebSocket channel). The worker applies the delta to its local filesystem at the next safe point — typically between tool calls, since mutating files during a running build or test would cause nondeterministic results. As a fallback, the worker also does a periodic pull (e.g., every few seconds) to catch any missed notifications. Human edits during active agent execution are expected to be rare; this model keeps the common path (agent is sole writer) lightweight while ensuring human contributions arrive promptly.
- **Cold start materialization:** A new worker checks out the draft's `baseCommit`, then applies the full CRDT document state as a batch overlay — producing the exact filesystem the previous worker had.

This model optimizes for the common case (agent is the sole writer, humans observe) while supporting the collaborative case (human sends a fix while the agent is blocked) without real-time contention.

Checkpoints (§4.2) commit the draft to the task branch as git commits at meaningful milestones, advancing the base commit. Between checkpoints, all changes live in the draft overlay.

### 4.1 Checkpointing

During long-running tasks, the agent creates **checkpoints** — git commits at meaningful milestones. Each checkpoint records:
- The commit SHA
- The session message index that triggered it
- A human-readable reason
- The list of files changed

Checkpoints enable:
- Users to see incremental progress in the diff view.
- Rollback to a known-good state if the agent goes off track.
- Resume after a crash or timeout.

---

## 5. System Prompt Architecture

The system prompt is constructed dynamically based on the job type, ontology context, and available tools. It has three layers:

### 5.1 Base Layer (Shared)

All agents receive:
- Identity: "You are a G5 coding agent."
- Capabilities: List of available tools with descriptions.
- Constraints: Budget limits, file access boundaries, git branch rules.
- Style: Output formatting, artifact syntax, citation format.

### 5.2 Job Layer

Job-type-specific instructions are rendered via Jinja2 templates. The `PromptManager` selects and renders templates based on `TaskContext.task_type`:

**Uplift system prompt additions:**
- Workspace isolation context (worktree lifecycle, automatic commits on success).
- Interface extraction requirements: extract all public functions, classes, imports, exports.
- Success pattern: `extract_interface` → `read_file` → `create_spec` → `validate_spec` → `transition_task_state("complete")`.
- SemliftSpec format guidance with sample specifications.

**Downlift system prompt additions:**
- Workspace isolation context with file structure guidance.
- Success pattern: `get_spec` → `write_file` (new files) or `edit_file_replace` (edits) → `transition_task_state("complete")`.
- Instruction to use exact file paths from the task's `src_files` list.
- For incremental updates: `git_diff` to read what changed in the spec, `edit_file_replace` / `edit_spec_replace` for targeted edits (fall back to `edit_file_patch` / `edit_spec_patch` if the replacement target cannot be uniquely matched).

**Review system prompt additions:**
- Bidirectional audit scope (spec→code and code→spec).
- Edit failure recovery strategy: after 2+ failed edits, rewrite the entire file.
- Full tool set with read/write/test capabilities.

**Chat system prompt additions:**
- Emphasis on confirming the user's goal before acting.
- Instruction to close every turn with `respond_to_user`.
- Session identifier for continuity across turns.
- Lower budget limits and helpfulness-oriented constraints.

### 5.3 SpecContext in the Prompt

When not using suffix-optimized component IDs, the system prompt includes the task's **SpecContext frontmatter** inline:

```yaml
component_id: "auth/token_validator"
src_files: ["src/auth/validator.rs", "src/auth/claims.rs"]
parent_spec: "auth"
child_specs: []
```

The agent is instructed to treat all four metadata fields (`component_id`, `src_files`, `parent_spec`, `child_specs`) as immutable — they are auto-calculated from the codebase analysis and ontology structure. The agent writes only the spec body (natural language content below the frontmatter). When suffix optimization is enabled, this context is omitted from the prompt and instead appears only through prepopulation tool calls (§10), reducing prompt size.

---

## 6. Tool Set

Tools are the agent's hands. This section defines their semantic grouping, schemas, and contracts. WORKER_INFRASTRUCTURE.md §10.7 specifies where each tool executes.

### 6.1 Analysis Tools

| Tool | Purpose | Used By |
|------|---------|---------| 
| `read_file` | Read source file content | All |
| `read_spec` | Read a SemliftSpec by component_id | All |
| `list_specs` | List specs in the ontology | All |
| `validate_spec` | Validate a spec for format (Extended Markdown dialect), completeness, and dependency correctness | Uplift, Review |
| `extract_interface` | Extract public symbols (functions, classes, exports) with signatures from a source file | Uplift |

`extract_interface` returns a compact structured summary — public function signatures, class/method signatures, imports, and exported symbols — without requiring the agent to read and parse raw source. Backed by tree-sitter parsers per language. Results are cached (keyed by path + mtime). Significantly cheaper and more reliable than having the LLM extract interfaces from raw file content.

### 6.2 Mutation Tools

**Primary tools** — string-replacement contract inspired by Claude Code's Edit tool. The agent specifies the exact text to find and its replacement. This interface is simpler for LLMs to produce correctly, unambiguous in matching, and the model is already trained on it.

| Tool | Purpose | Used By |
|------|---------|--------|
| `write_file` | Create or overwrite a file | Downlift, Review |
| `edit_file_replace` | Replace an exact string in a file (`file_path`, `old_string`, `new_string`) | Downlift, Review |
| `create_spec` | Write a new SemliftSpec | Uplift |
| `edit_spec_replace` | Replace an exact string in a spec (`component_id`, `old_string`, `new_string`) | Uplift, Review |
| `delete_file` | Remove a file | Review |

**Option B — patch-based tools.** Unified-diff interface with fuzzy matching (whitespace-insensitive → windowed search → `SequenceMatcher` at 0.85). Useful when the agent needs multi-hunk edits or when the target text has drifted from what the agent expects. Available as a fallback; the system prompt prefers the primary tools above.

| Tool | Purpose | Used By |
|------|---------|--------|
| `edit_file_patch` | Apply a unified-diff patch to a file (`file_path`, `patch`) | Downlift, Review |
| `edit_spec_patch` | Apply a unified-diff patch to a spec (`component_id`, `patch`) | Uplift, Review |

### 6.3 Execution Tools

| Tool | Purpose | Used By |
|------|---------|---------|
| `run_in_terminal` | Execute a shell command in a persistent terminal session | All |

The agent interacts with the workspace through **persistent terminal sessions** — long-lived PTY shells that preserve environment variables, working directory, and shell history across multiple commands (similar to Claude Code and GitHub Copilot's terminal model).

```typescript
interface RunInTerminalArgs {
  command: string;              // Shell command to execute
  cwd?: string;                 // Working directory (defaults to workspace root)
  timeout?: number;             // Max seconds (default 300)
  terminalId?: string;          // Reuse a specific terminal session (default: reuse the last active session)
  newTerminal?: boolean;        // Force a fresh terminal session (default: false)
  isBackground?: boolean;       // Run without blocking the agent loop (for servers, watchers)
}

interface TerminalResult {
  terminalId: string;           // ID for reuse in subsequent calls
  exitCode: number | null;      // null for background processes
  stdout: string;               // Truncated to 60KB; use head/tail/grep to filter
  stderr: string;
}
```

**Terminal lifecycle:**
- A default terminal session is created on first use and persists for the task's lifetime.
- Subsequent `run_in_terminal` calls reuse the same session by default, inheriting `$PWD`, env vars, and shell state.
- The agent can request a fresh terminal (`newTerminal: true`) for isolated operations (e.g., a background server that shouldn't block the main session).
- Background terminals (`isBackground: true`) return immediately; the agent can poll their output via `get_terminal_output(terminalId)`.
- Terminal sessions are cleaned up when the task completes or fails.

Build, test, grep, and any other shell commands are all invoked via `run_in_terminal`. The agent (or system prompt) knows the codebase's build/test commands from analysis. Text search uses `rg` (ripgrep) or `grep` through the terminal — there is no separate search tool.

**Git safety:** `run_in_terminal` blocks git subcommands that mutate branch state or the working tree outside the CRDT layer. Specifically, `git checkout`, `git switch`, `git reset`, `git stash`, `git clean`, `git merge`, `git rebase`, and `git restore` are rejected with an error explaining that file and branch operations must go through the provided tools. Read-only git commands (`git log`, `git show`, `git blame`, `git diff`) are allowed — they're useful for the agent's reasoning and cannot desync the CRDT.

### 6.4 Git Tools

| Tool | Purpose | Used By |
|------|---------|---------|
| `git_status` | Check workspace status | All |
| `git_diff` | Show current changes | All |
| `git_commit` | Commit current changes (checkpoint) | Downlift, Review |

These tools are implemented by the worker infrastructure (not by shelling out to `git` in the agent's environment) and coordinate with the CRDT layer. `git_commit` materializes the current draft to disk, runs `git add -A && git commit`, advances the draft's `baseCommit` to the new SHA, and resets the draft overlay to empty. `git_status` and `git_diff` reflect the combined state of the base commit plus the draft overlay.

### 6.5 Task Lifecycle

| Tool | Purpose | Used By |
|------|---------|---------|
| `transition_task_state` | Transition the current task to a terminal or blocked state | All |
| `respond_to_user` | Send a response in chat mode (does not terminate the session) | Chat |

`transition_task_state` accepts a `state` parameter with three options:

| State | Behavior | CRDT Draft | Git |
|-------|----------|-----------|-----|
| `complete` | Task finished successfully. Draft is materialized, committed, and pushed to remote. The task branch is ready for review/merge. Draft resets to clean. | Materialized → committed → reset | `git add -A && git commit && git push` |
| `blocked` | Agent cannot proceed (needs human input, missing dependency, ambiguous spec). The task enters `BLOCKED` and waits for intervention. Draft persists on server — no work lost. | Persisted (server-side) | No commit |
| `fail` | Agent cannot resolve the issue. The task enters `FAILED` (transient). If retries remain, the system auto-retries by transitioning back to `PENDING` (with backoff); the draft is intact so the next attempt can inspect partial work. If retries are exhausted, the task escalates to `BLOCKED` and waits for human intervention. | Persisted for retry | No commit |

**Completion contract:** `transition_task_state(state: "complete")` proceeds through these steps:
1. Runs **pre-completion hooks** — task-type-specific checks that may veto the transition:
   - **Downlift**: Checks whether the generated code depends on tasks that don't yet exist in the graph. If missing dependencies are found, the hook creates those tasks and **vetoes** completion — the task returns to `PENDING` and will complete later once its new dependencies are satisfied (the "graph cut" pattern). This is how dynamically-discovered dependencies are woven into the task graph without stalling the pipeline.
   - **Uplift, Review**: No pre-completion checks.
   If any hook vetoes, the task transitions to `PENDING` instead and the remaining steps do not execute.
2. Runs `git add -A` to stage all changes.
3. Creates a commit with a structured message (task ID, component ID, summary).
4. Pushes the task branch to the remote.
5. If the push fails (conflict, network), the tool returns an error and the agent must resolve before retrying.
6. Runs **post-completion hooks** — task-type-specific actions that propagate work through the task graph:
   - **Uplift**: No graph-level side effects. Hierarchical cascading is handled by the task executor's normal dependency resolution — aggregation tasks already exist in the graph (created during analysis, §3.3) with dependencies on their children, so the executor schedules them automatically when all children reach `COMPLETED`.
   - **Downlift**: Scans committed work for newly created child specs under `.g5/specs/`. For each new spec, creates a downlift task in the graph with appropriate dependencies. New tasks enter `PENDING` and are scheduled by the executor when their prerequisites complete.
   - **Review**: No graph-level side effects.
7. Only after successful push and hook execution does the task enter `COMPLETED`.

**Blocked contract:** The agent must provide a `reason` string explaining what it's waiting for. The task enters `BLOCKED` and the agent loop suspends — the dirty tree and full dialog are preserved. The blocking reason is surfaced in the UI (task list, notifications). Common reasons: "Need clarification on error handling strategy", "Dependency spec X is incomplete", "Build requires environment variable not configured".

Unblocking uses the **attach + send_message** pattern:
1. Human sees the blocked task in the UI with the agent's reason.
2. Human clicks **Attach** — the browser subscribes to the session's WebSocket stream and renders the full dialog history (the human sees everything the agent did before blocking).
3. Human types a response in the session chat. This calls `POST /api/v1/sessions/:id/messages`, which injects a `user` message into the session (see WORKER_INFRASTRUCTURE.md §10.8).
4. The system transitions the task `BLOCKED → RUNNING`. The agent loop resumes and sees the injected message as its next dialog turn — it can read the human's guidance and continue working.
5. Human may **Detach** at any point; the agent continues uninterrupted.

The same attach mechanism works for `RUNNING` tasks — a human can observe and send guidance messages to a running agent without pausing it. The blocked case is special only in that the agent loop is suspended until a message arrives.

`send_message` is also available as a tool to other agents (e.g., a chat agent can inject a message into a blocked task session on behalf of the user — see WORKER_INFRASTRUCTURE.md §10.9).

**Worker retention:** A blocked task holds its worker assignment for a configurable grace period (`blocked_worker_retention_minutes`, default 15). During this window, the agent loop is suspended but the worker's local filesystem (build caches, compiled artifacts) remains hot — unblocking is instant. If the grace period expires without intervention, the worker is released: it becomes available for other tasks. The task's file state is safe in the CRDT draft (server-side, already persisted). When the task is later unblocked, the orchestrator schedules it to any available worker, which materializes the draft onto disk and resumes the agent loop. The agent sees no difference — its dialog history is in the database and file state is in the draft — but the cold-start penalty (clone + materialize + rebuild caches) makes prompt human intervention preferable.

**Failure contract:** The agent must provide an `error` string describing what went wrong. The task enters `FAILED` (transient). The system then checks the retry budget:
- Retries remaining → task transitions `FAILED → PENDING` with a backoff delay. When the delay expires and dependencies are satisfied, the executor moves it to `RUNNING` and the agent loop restarts with the dirty tree + error context prepopulated.
- Retries exhausted → task transitions `FAILED → BLOCKED`. The dirty tree is preserved and the failure reason is surfaced to the user. The unblocking flow is the same as for agent-initiated `BLOCKED` — human attaches, sends guidance, and the task resumes.

---

## 7. Ontology Grounding

The ontology is not just documentation — it is the **constraint system** that bounds agent behavior.

### 7.1 Scope Binding

Every task session is bound to one or more ontology nodes. The agent:
- **Can read** any spec in the ontology (for context).
- **Can write** only specs within its task's scope.
- **Can modify** only files listed in `src_files` of its scoped specs (for downlift/review).
- **Cannot create** new top-level ontology nodes without human approval.

This prevents agents from creating sprawling, undirected changes.

### 7.2 Dependency Contracts

When downlifting, the agent treats dependency specs as **contracts**:
- The dependency's interface section defines what functions/types are available.
- The agent must use only declared interfaces — it cannot reach into dependency internals.
- If a dependency's interface is insufficient, the agent should flag this rather than inventing a workaround.

This enforces clean module boundaries in generated code.

### 7.3 Spec Consistency

After a batch of tasks completes, the system can run a **consistency check** across the ontology:
- Do `src_files` listings match what actually exists on disk?
- Do `child_specs` / `parent_spec` form a valid tree (no cycles, no orphans)?
- Are interfaces used across spec boundaries compatible?
- Does the task graph's dependency structure match actual import statements?

Inconsistencies generate review tasks automatically.

---

## 8. Batch Execution

For large-scale uplift or downlift of an entire codebase, G5 orchestrates batch execution:

### 8.1 Batch Uplift

```
1. User triggers "Uplift" on a start node (§3.7).
2. Bootstrap phase (§3.6):
   a. Worker analyzes start node's subtree (§3.2 import order simulation).
   b. Stale nodes produce tasks; synced nodes are skipped (unless force).
   c. Hierarchical aggregation tasks added (§3.3).
   d. Task graph submitted to database, job transitions to RUNNING.
3. Execution phase:
   a. Tasks execute in topological order (leaf-first).
   b. Multiple tasks at the same graph level run concurrently.
   c. Each task produces a SemliftSpec.
   d. Specs are committed to the `.g5/specs/` directory.
4. Review phase:
   a. Consistency check runs across all generated specs.
   b. Inconsistencies generate review tasks.
   c. User reviews final ontology.
```

### 8.2 Batch Downlift

```
1. User triggers "Downlift" on a start node (§3.7).
2. Bootstrap phase (§3.6):
   a. Worker reads start node's spec and child_specs subtree.
   b. Stale nodes produce tasks; synced nodes are skipped (unless force).
   c. For greenfield: single seed task; hooks expand graph dynamically.
   d. Task graph submitted to database, job transitions to RUNNING.
3. Execution phase:
   a. Root spec processed first (structure, config files).
   b. Module specs processed next (directory structure, module interfaces).
   c. File specs processed last (implementation).
   d. Each task generates code, validates, and commits.
4. Integration phase:
   a. All task branches merged into workspace branch.
   b. Full build + test suite runs.
   c. Failures generate refinement tasks.
```

### 8.3 Concurrency Model

Tasks at the same topological level can execute concurrently:

```
Level 0 (leaves):    [task-a] [task-b] [task-c] [task-d]  ← 4 concurrent
Level 1 (modules):   [task-e] [task-f]                     ← 2 concurrent (after level 0)
Level 2 (workspace):   [task-g]                              ← 1 (after level 1)
```

Concurrency is bounded by `max_concurrent_tasks` (configurable, default 10) and available worker capacity.

---

## 9. TaskContext & SpecContext

The **TaskContext** is the structured binding between an agent session and the ontology. It scopes what the agent can see, what it should produce, and how prepopulation seeds the dialog.

### 9.1 SpecContext

`SpecContext` captures the ontology node's identity and relationships. It mirrors the four spec metadata fields exactly:

| Field | Type | Description |
|-------|------|-------------|
| `component_id` | `str` | Hierarchical, filesystem-derived ID (e.g., `auth/token_validator`) |
| `src_files` | `list[str]` | Paths to the source files this spec describes |
| `parent_spec` | `str?` | Parent in the hierarchy (module or workspace level); `null` for root |
| `child_specs` | `list[str]` | Children (for module/workspace aggregation specs) |

SpecContext is parsed from the YAML frontmatter of a spec file (`.g5/specs/<component_id>.md`). It supports `merge(other)` with left-precedence semantics: left context's scalar fields win, list fields (`src_files`, `child_specs`) are unioned, and invalid spec IDs are filtered.

Dependencies, language, type, and other derived metadata are tracked at the **task graph** level (from codebase analysis) rather than in the spec itself.

### 9.2 TaskContext

`TaskContext` wraps SpecContext with execution metadata:

| Field | Type | Description |
|-------|------|-------------|
| `task_type` | `str` | `uplift`, `downlift`, `review`, `chat` |
| `task_id` | `str` | Unique task identifier (used in `transition_task_state` calls) |
| `task_description` | `str` | Human-readable task description |
| `spec_context` | `SpecContext` | The ontology binding (§9.1) |
| `workspace_path` | `str` | Absolute path to the isolated worktree |
| `guardrails` | `dict` | Budget limits, scope constraints |
| `incremental_update` | `bool` | Whether this is a spec-diff-driven incremental downlift |
| `planning_enabled` | `bool` | Whether architectural planning phase is active |
| `workflow_phase` | `str` | Current phase within multi-phase workflows |

### 9.3 AgentContext

`AgentContext` is the runtime container passed to the agent loop:

| Field | Type | Description |
|-------|------|-------------|
| `task_id` | `str` | Links to the task in the task graph |
| `workspace_path` | `str` | Worktree root |
| `mcp_client` | `MCPClient` | Tool execution client |
| `available_tools` | `list[MCPTool]` | Tools the agent may call |
| `task_context` | `TaskContext` | The full task binding (required for all jobs) |

The agent loop creates a fresh `DialogCompactor` per `AgentContext` — ensuring no state leaks between tasks. The `TaskContext` is mandatory; the agent fails immediately if it is missing.

### 9.4 How TaskContext Drives Behavior

TaskContext influences the agent at four points:

1. **Prompt construction** (§5): `PromptManager.create_prompt()` reads `task_type` to select templates, reads `spec_context` to inject frontmatter, reads `incremental_update` to choose between full-generation and diff-driven guidance.

2. **Prepopulation** (§10): Each phase of prepopulation reads from `spec_context` and the spec graph — Phase 0 reads `component_id`, Phase 2 iterates `src_files`, Phase 3 iterates dependency specs (from spec graph edges), Phase 4 iterates `child_specs`.

3. **Tool call propagation**: TaskContext is threaded through every tool invocation, giving each tool access to the task's workspace path, type, and spec scope. The propagation chain is:

   ```
   AgentContext.task_context
     → BaseAgent._batch_call_tools()     extracts from self.current_context
       → mcp_client.call(tool, task_context=tc, **args)
         → ToolRegistry.execute_tool(tool, args, task_context=tc)
           → individual tool impl(**kwargs)  uses tc.workspace_path, etc.
   ```

   **Security invariant:** If the LLM attempts to supply a `task_context` parameter in its tool call arguments, the agent **strips it** before dispatch. The internal `task_context` is always authoritative — the LLM cannot escalate its own scope.

   Tools use the propagated context to:
   - Resolve relative file paths against `task_context.workspace_path` (the isolated worktree).
   - Scope shell commands (`run_in_terminal`) to the correct working directory.
   - Restrict file operations to the task's workspace boundary.

4. **Scope enforcement** (§7): The agent can only modify files listed in `spec_context.src_files` and can only write specs within its scoped `component_id` subtree.

---

## 10. Dialog Prepopulation

Before the ReAct loop begins, the agent **prepopulates** the dialog with contextual information gathered via synthetic tool calls. This seeding ensures the agent has the information it needs without wasting reasoning turns on information gathering.

### 10.1 Prepopulation Phases

Prepopulation executes five phases in order, each adding tool-call/result pairs to the `DialogCompactor`:

| Phase | Tool Call | Source | Purpose |
|-------|-----------|--------|---------|
| 0 | `read_spec(component_id)` | `spec_context.component_id` | **Source of truth** — the spec the agent is working on. Critical for downlift. |
| 1 | `list_specs()` + `run_in_terminal(ls)` | (static) | Workspace structure overview — available specs and file listing. |
| 2 | `read_file(path)` per file | `spec_context.src_files` | Source file content for analysis (uplift) or reference (downlift). |
| 3 | `read_spec(dep)` per dep | Spec graph edges | Dependency contracts — the interfaces the agent may call into. |
| 4 | `read_spec(child)` per child | `spec_context.child_specs` | Child specs for aggregation tasks. |

Each phase calls `add_tool_call_with_lsm_check`, which:
1. Creates a synthetic `assistant` message with a tool call (e.g., `read_file(path="src/auth.rs")`).
2. Executes the tool via the MCP client.
3. Appends the result as a `user` message with tool results.
4. The compactor's `append()` triggers compaction if the level exceeds its token budget.

This means prepopulation can trigger compaction *during* context loading — early phases may be summarized before later phases complete. The compactor handles this transparently.

### 10.2 Synthetic Tool Call Pairs

Prepopulation messages use the same `Message` format as real tool interactions:

```
assistant: "I'll gather source file src/auth.rs information for context."
           [tool_call: read_file(path="src/auth.rs"), call_id="react_prepop_005"]

user:      [tool_result: call_id="react_prepop_005", content="<file contents>"]
```

This is semantically identical to what the agent would see if it called the tools itself. The LLM sees a dialog history that looks like prior tool usage, giving it full context without burning reasoning turns. Call IDs use the format `react_prepop_NNN` to distinguish synthetic calls from real agent actions.

If a tool call fails during prepopulation (e.g., a file doesn't exist), an error result is still added to maintain dialog consistency — every tool call has a paired result.

### 10.3 Reasoning Model Fallback

Some models (e.g., Gemini reasoning variants) require cryptographic signatures on tool call messages that cannot be fabricated for synthetic prepopulation. For these models, the agent uses a **simplified prepopulation** path:

1. Executes the same five phases, gathering results.
2. Instead of synthetic tool-call pairs, injects a single `user` message containing all gathered context as markdown sections.
3. The agent sees the same information but without the tool-call dialog structure.

---

## 11. Dialog Compaction

Long-running tasks (especially uplift of large files or iterative downlift with validation cycles) can accumulate hundreds of messages. The agent needs a mechanism to manage token growth without losing critical context. G5 defines this as a pluggable **compaction interface** with a default LSM Tree implementation.

### 11.1 The `DialogCompactor` Interface

All dialog compaction strategies implement the `DialogCompactor` interface:

```python
class DialogCompactor(ABC):
    """Abstract interface for dialog compaction strategies.

    A DialogCompactor *is* the dialog from the agent loop's perspective.
    It conforms to a list[Message]-like protocol: callers can iterate,
    index, and measure length without knowing whether compaction has
    occurred internally.
    """

    @abstractmethod
    async def append(self, message: Message) -> None:
        """Append a message, triggering compaction if needed."""

    @abstractmethod
    async def extend(self, messages: list[Message]) -> None:
        """Append multiple messages."""

    @abstractmethod
    def get_dialog(self) -> list[Message]:
        """Return the current dialog for LLM consumption."""

    @abstractmethod
    def get_stats(self) -> dict[str, Any]:
        """Return compaction statistics for monitoring."""

    @property
    @abstractmethod
    def dialog_segments(self) -> list[list[Message]]:
        """Pre-compaction snapshots for training data extraction."""

    # list[Message]-like protocol
    def __len__(self) -> int: ...
    def __getitem__(self, index: int) -> Message: ...
    def __iter__(self) -> Iterator[Message]: ...
```

The compactor **is** the dialog — it conforms to the `list[Message]` protocol (`__len__`, `__getitem__`, `__iter__`) so the agent loop can treat it as an ordinary message list. Internally the compactor is free to reorganize, summarize, or discard messages; externally it always presents a coherent, ordered dialog. The interface is deliberately minimal: `append`/`extend` for ingestion, `get_dialog` for reconstruction, and `get_stats` / `dialog_segments` for observability and training. The agent loop never depends on the compaction strategy — it calls `append` after every LLM response and tool result, and calls `get_dialog` before every LLM invocation.

### 11.2 LSM Tree Implementation (Default)

The default compactor uses a **Log-Structured Merge Tree** inspired architecture. It maintains a stack of **levels**, each holding a list of messages:

```
┌──────────────────────────────────────────────────┐
│  Level N (preserved)  — system + user prompt      │  ← Never compacted
├──────────────────────────────────────────────────┤
│  Level 1              — older summaries           │  ← Compacts into level 2
├──────────────────────────────────────────────────┤
│  Level 0              — newest messages           │  ← Compacts into level 1
└──────────────────────────────────────────────────┘

Dialog reconstruction: levels read from N → 0 (oldest → newest)
```

- **Level 0** receives all new messages (via `append()`).
- **Level N** (the highest, "preserved" level) holds the initial system+user prompt messages and is **never compacted**. The number of messages preserved is configured by `preserve_prefix_count` (default: 2).
- Intermediate levels hold summaries produced by compacting the level below.

### 11.3 Compaction Trigger

After each `append()`, the LSM compactor checks whether Level 0's estimated token count exceeds `max_tokens_per_level` (default: 100,000 tokens). If so, compaction runs:

1. **Forward greedy span selection** starting from index 0 of the level.
2. Messages are selected greedily, but **tool call/result pairs are kept together**: if a message contains tool calls, the selector scans forward to find the paired tool-result message and includes both atomically. If the pair cannot be found within the level, compaction stops at that boundary.
3. The selected span is sent to the LLM for summarization.
4. The original span is removed from the level.
5. The resulting summary message is inserted into the next level up.

### 11.4 Summarization Prompt

The LSM compactor constructs a focused prompt for the LLM:

```
System: "You are an expert dialog summarizer. Create concise summaries that
         preserve essential context for task continuation."

User:   "Summarize the following messages. Preserve:
         1) What the agent did/read
         2) Relevant unique details (e.g., specification sections to write)
         3) What remains to complete the task goals

         [formatted messages to summarize]

         Start your response with 'Summary of [topic]:' — max ~10,000 tokens."
```

The LLM's response becomes an `assistant` message inserted into the next level. This summary preserves task-relevant context while dramatically reducing token count.

### 11.5 Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `enable_summarization` | `false` | Master switch (disabled for short tasks) |
| `max_tokens_per_level` | `100,000` | Token budget per level before compaction triggers |
| `max_levels` | `2` | Number of compactable levels (+ 1 preserved level) |
| `preserve_prefix_count` | `2` | Initial messages preserved in the non-compacting level |
| `token_estimate_divisor` | `4` | Characters → token estimate scaling factor |
| `min_compactable_messages` | `1` | Minimum messages required to trigger compaction |
| `target_summary_tokens` | `10,000` | Max tokens for generated summary messages |

### 11.6 Dialog Reconstruction

When the agent needs the current dialog (via `get_dialog()`), the LSM compactor returns messages from all levels concatenated oldest-to-newest:

```
[preserved level N] + [level N-1 summaries] + ... + [level 0 active messages]
```

This gives the LLM a view where:
- The system prompt and task instructions are always present (preserved).
- Older interactions appear as summaries (compressed but contextually complete).
- Recent interactions are at full fidelity.

### 11.7 Dialog Segments

Before each compaction, the LSM compactor captures a deep copy of the full dialog into `dialog_segments`. These pre-compaction snapshots enable:
- **Training data extraction**: Full-fidelity dialogs for SFT, even after compaction.
- **Debugging**: Inspect what the agent saw at each compaction boundary.
- **Audit trail**: Verify that no critical context was lost during summarization.

### 11.8 Lifecycle: One Compactor per Task

A fresh `DialogCompactor` is created for each task execution. This ensures:
- No state leakage between tasks on the same worker.
- Clean level structure for each task's dialog.
- Independent compaction history per task.

The compactor is initialized with the prompt messages (system + user from PromptManager), then receives prepopulation messages, then receives the ReAct loop's LLM responses and tool results.

**Relationship to persistence:** The compactor is an **in-process optimization** for managing the LLM's context window. The **canonical dialog** is the sequence of `SessionMessage` rows in the central database — every message (including LLM response chunks) is persisted via `sessionStore.appendMessage()` and broadcast to WebSocket subscribers via `broadcast.emit()` as it is produced (see WORKER_INFRASTRUCTURE.md §10.5). Clients subscribing to `WS /api/internal/sessions/:id/stream` receive a real-time feed of all messages, tool calls, tool results, and status changes. The compactor's summarized view is never persisted; it exists only for the duration of the agent loop to keep the LLM prompt within budget.

### 11.9 Alternative Implementations

The `DialogCompactor` interface is designed for experimentation with compaction strategies beyond LSM:

| Strategy | Description | Trade-off |
|----------|-------------|----------|
| **LSM (default)** | Level-based compaction with LLM summarization | Best context preservation; costs one LLM call per compaction |
| **Sliding window** | Drop messages beyond a fixed window, keeping prefix | Zero compaction cost; loses older context entirely |
| **Hybrid** | Sliding window with periodic LLM snapshots at checkpoints | Cheaper than LSM; recovers checkpoint context on demand |
| **No-op** | Pass-through with no compaction (for short tasks) | Zero overhead; caller is responsible for staying within limits |

The active implementation is selected when the agent loop creates the compactor. Today `LSMDialogCompactor` is the only production implementation; the no-op path is used when `enable_summarization` is `false` (both behaviors currently live in the same class).

---

## 12. Budget & Guardrails

### 12.1 Budget Limits

Each session has configurable limits:

| Limit | Default | Purpose |
|-------|---------|---------|
| `maxTurns` | 200 | Prevent infinite loops |
| `maxCostUsd` | $10.00 | Cost control |
| `maxExecutionTimeSec` | 1800 (30m) | Time control |
| `maxTotalTokens` | 1,000,000 | Token control |

Chat tasks get elevated budgets (10K turns, $10K, 1B tokens) for long-lived interactive sessions with expensive frontier models. All other task types use defaults.

When any limit is hit, the agent loop terminates and the session enters `FAILED` with a `budget_exceeded` reason. The agent should report LLM usage statistics (model, input/output tokens, cost) as part of task completion via `transition_task_state`.

### 12.2 Guardrails

| Guardrail | Behavior |
|-----------|----------|
| **File scope** | Agent can only modify files within its task's src_files list. |
| **Branch scope** | Agent can only commit to its task branch. |
| **No network** | Workers have no outbound internet (except GCS cache and control plane). |
| **Audit trail** | Every tool call is logged as a SessionMessage. |
| **Human interrupt** | User can pause/abort at any time via the session API. |
| **Checkpoint frequency** | Agent must commit every N file changes (configurable). |

### 12.3 Failure Modes

| Failure | Agent Behavior | System Response |
|---------|---------------|-----------------|
| LLM returns error | Retry with exponential backoff (3 attempts) | Mark session failed if all retries fail |
| Tool call fails | Report error, agent decides next step | Log error in session messages |
| Build/test fails | Agent reads error, refines code within its turn | Agent calls `fail` if it can't resolve |
| Budget exceeded | Loop terminates | Task enters `FAILED`, dirty tree preserved |
| Worker crashes | Session state preserved in DB | Reschedule on new worker, dirty tree preserved |

---

## 13. Training Data Generation

A key output of the uplift/downlift pipeline is **training data** for fine-tuning coding models. Each agent session produces a dialog trace that can be used as supervised fine-tuning (SFT) data.

### 13.1 Dialog Structure

Each session produces:
```json
{
  "messages": [
    {"role": "system", "content": "..."},
    {"role": "user", "content": "..."},
    {"role": "assistant", "content": "..."},
    {"role": "tool_call", "tool": "read_file", "args": {...}},
    {"role": "tool_result", "content": "..."},
    {"role": "assistant", "content": "..."}
  ],
  "metadata": {
    "task_type": "uplift",
    "component_id": "auth/token_validator",
    "language": "rust",
    "success": true,
    "cost_usd": 0.42,
    "turns": 8
  }
}
```

### 13.2 Quality Signals

Training data is filtered by:
- **Task success**: Only completed tasks contribute.
- **Validation pass**: For downlift, code must build and pass tests.
- **Roundtrip consistency**: Uplift → downlift → uplift must produce similar specs.
- **Cost efficiency**: Prefer dialogs that complete in fewer turns.

### 13.3 Topological Training Order

For fine-tuning, dialogs are ordered topologically — leaf-node dialogs first, then their parents. This mirrors the dependency order of the original analysis and allows the model to learn simpler components before complex aggregations.

---

## 14. LLM Client Interface

> **Reference implementation archived at:** `prompts/llm/` (all provider source files from semlift).

The agent interacts with LLM providers through a provider-agnostic client layer. This section documents the interface contracts, provider-specific gotchas, caching behavior, and cost extraction patterns that any G5 implementation must respect.

### 14.1 Core Types

**`Message`** — Provider-agnostic dialog message. All fields are JSON-serializable.

| Field | Type | Description |
|-------|------|-------------|
| `role` | `"system" \| "user" \| "assistant" \| "tool"` | Message role |
| `content` | `str?` | Text content (may be `null` for tool-call-only assistant messages) |
| `tool_calls` | `list[MCPToolCall]?` | Outgoing tool invocations (assistant → tools) |
| `tool_results` | `list[tuple[str, Any]]?` | Incoming tool results `(call_id, result)` (tools → assistant) |
| `reasoning_details` | `list[dict]?` | Chain-of-thought / thinking blocks from reasoning models |
| `model` | `str?` | Model that generated this message (stamped by `chat()` on assistant messages) |
| `created_at` | `float` | Unix timestamp |

Factory methods: `Message.system(content)`, `Message.user(content)`, `Message.assistant(content)`, `Message.assistant_with_tool_calls(content, tool_calls)`, `Message.user_with_tool_results(tool_results, text?)`, `Message.tool_response(content, name, call_id)`.

**`ChatResponse`** — Normalized response from any provider.

| Field | Type | Description |
|-------|------|-------------|
| `messages` | `list[Message]` | Response messages (usually 1 assistant message) |
| `raw` | `Any` | Provider-specific raw response (not serialized) |
| `usage` | `dict[str, int]` | Token counts — **the cost extraction point** (see §14.5) |
| `model` | `str` | Model that served the request (may differ from requested model for routers) |
| `finish_reason` | `str?` | `"stop"`, `"tool_calls"`, `"length"`, etc. |

**`CompletionResponse`** — Same structure for text completions (non-chat). Fields: `content: str`, `raw`, `usage`, `model`, `finish_reason`.

### 14.2 Provider Interface (`BaseLLMProvider`)

```python
class BaseLLMProvider(ABC):
    name: str                       # "openai", "anthropic", etc.
    model: str                      # e.g. "claude-sonnet-4-20250514"
    timeout: int                    # seconds (default 120)

    async def chat(
        messages: list[Message],
        *,
        tools: list[dict] | None = None,
        tool_choice: "auto" | "none" | str | None = None,
        max_tokens: int | None = None,
        temperature: float = 0.7,
    ) -> ChatResponse:
        """Submit chat messages. Stamps model on assistant messages."""

    async def chat_with_tools(
        messages: list[Message],
        tools: list[MCPTool | dict],
        *,
        tool_choice: str | None = "auto",
        max_tokens: int | None = None,
        temperature: float = 0.7,
    ) -> ChatResponse:
        """Convenience: formats tools via _format_tools_for_provider(), then calls chat()."""

    def extract_tool_calls(response: ChatResponse) -> list[MCPToolCall]:
        """Flatten tool calls from all response messages."""

    async def complete(prompt: str, **kwargs) -> CompletionResponse:
        """Text completion (non-chat). Used for summarization prompts."""
```

**Key contract:** `chat()` stamps `response.model` onto every assistant message's `msg.model` field before returning. This ensures dialog history always records which model produced each response — critical for cost attribution and training data.

### 14.3 Provider-Specific Gotchas

#### Anthropic

1. **System messages are NOT in the messages array.** Anthropic's API takes system content as a separate `system` parameter. The provider extracts system messages from the dialog and passes them as structured content blocks to `system=`.

2. **Tool results must be user messages.** Anthropic only allows `"user"` and `"assistant"` roles. Tool results are encoded as user messages containing `tool_result` content blocks:
   ```json
   {"role": "user", "content": [
     {"type": "tool_result", "tool_use_id": "call_123", "content": [{"type": "text", "text": "..."}]}
   ]}
   ```

3. **Tool calls are content blocks, not a separate field.** Unlike OpenAI's `tool_calls` array, Anthropic embeds tool invocations as `tool_use` content blocks within the assistant message's `content` array.

4. **Prompt caching (critical for cost).** Anthropic supports ephemeral prompt caching (`"cache_control": {"type": "ephemeral"}`). The provider:
   - Enables the `prompt-caching-2024-07-31` beta header.
   - Adds `cache_control` to system content blocks and the first 20 messages.
   - Respects the **4 cache block limit** per request.
   - Logs cache effectiveness: `cache_creation_input_tokens` and `cache_read_input_tokens` from the response usage.
   - **Gotcha:** Cached tokens are billed at ~10% of uncached rate. For long agent dialogs (prepopulation + multi-turn), cache hits can reduce cost by 80%+. The agent must preserve message ordering to maximize cache reuse — mutating or reordering early messages invalidates the cache.

5. **Tool format:** `{"name": str, "description": str, "input_schema": dict}` (no `type: "function"` wrapper).

6. **Streaming:** Uses `client.messages.stream()` which provides `ping` keep-alive events for long requests. Pool timeout must be ≥ 30 minutes for extensive streaming sessions.

#### OpenAI

1. **Tool calls use a `function` wrapper:**
   ```json
   {"type": "function", "function": {"name": "read_file", "description": "...", "parameters": {...}}}
   ```
   Tool call arguments are JSON-**stringified** (not objects) in the response.

2. **Tool results are `role: "tool"` messages** with `tool_call_id` referencing the call.

3. **`parallel_tool_calls: false`** — The provider explicitly disables parallel tool calls. LLM-initiated parallel tool calls create ordering and concurrency issues in the ReAct loop.

4. **Streaming resilience:** The provider implements:
   - First-token watchdog (15s timeout — detects stalled streams).
   - Retry on transient I/O errors (`RemoteProtocolError`, `ReadTimeout`, `ConnectError`) with exponential backoff + jitter (up to 3 attempts).
   - Idempotency keys per attempt.
   - Retry on empty responses (0 completion tokens AND no content).

5. **Reasoning models:** The provider captures `reasoning_details` from streaming chunks (e.g., OpenRouter's thinking blocks). These are stored on the `Message` and round-tripped in subsequent requests for models that support it.

#### OpenRouter

1. **Extends `OpenAIProvider`** — inherits all OpenAI streaming, tool handling, and retry logic.

2. **Provider routing:** For certain models (e.g., Qwen), OpenRouter requires explicit provider routing via `extra_body.provider.order`. The provider maintains a `TOOL_SUPPORTED_PROVIDERS` map and raises `ValueError` for models without tool support.

3. **Baseten blocked:** Always adds `provider.ignore = ["baseten"]` for reliability.

4. **Reasoning token control:** The `REASONING_ENABLED_MODELS` set controls which models get `reasoning.enabled = true`. Disabled by default due to Gemini bug (missing `thought_signatures` for parallel tool calls → 400 errors).

#### Sequential Provider (Multi-Model Routing)

The `SequentialProvider` routes requests to different underlying providers based on turn count:
- Configured with a list of `(provider, turns)` pairs.
- After N turns with provider A, switches to provider B.
- Automatic fallback: if the active provider fails, tries the next in sequence.
- Use case: Start with a powerful model (Claude Opus) for planning, switch to a cheaper model (Claude Sonnet) for execution.

### 14.4 Provider Manager & Instance Caching

`LLMProviderManager` maintains a registry of provider classes and a cache of instantiated providers.

**Cache key formula:**
```python
cache_key = f"{name}:{model}:{hash(tuple(sorted(provider_kwargs.items())))}"
```

**Gotchas:**
- **Same name + model + kwargs → same instance.** If two tasks request the same provider configuration, they share the client instance (and its connection pool, rate limiting state, etc.). This is intentional for connection reuse but means provider state (e.g., `SequentialProvider.current_turn`) is shared.
- **`clear_cache()`** resets all cached instances. Must be called when switching between tasks that need fresh provider state.
- **Global singleton:** `get_global_manager()` returns a process-wide manager. In the worker, this means all tasks on the same worker share the provider cache.

**Registered providers:**

| Name | Class | Notes |
|------|-------|-------|
| `openai` | `OpenAIProvider` | Streaming by default, configurable `base_url` |
| `anthropic` | `AnthropicProvider` | Prompt caching, streaming via `messages.stream()` |
| `openrouter` | `OpenRouterProvider` | Extends OpenAI, adds provider routing |
| `bedrock` | `BedrockProvider` | AWS Bedrock (stub implementation) |
| `http` | `HTTPProvider` | Generic OpenAI-compatible HTTP endpoints |
| `vllm` | `VLLMProvider` | Self-hosted vLLM via OpenAI SDK |
| `sequential` | `SequentialProvider` | Turn-based multi-provider routing with fallback |
| `mock` | `MockLLMProvider` | Testing: returns configurable responses, tracks calls |

### 14.5 Cost Extraction & Usage Tracking

Every `ChatResponse` and `CompletionResponse` carries a `usage: dict[str, int]` dictionary. This is the **single source of truth** for cost accounting.

**Standard usage fields** (populated by all providers):

| Key | Description |
|-----|-------------|
| `prompt_tokens` | Input tokens consumed |
| `completion_tokens` | Output tokens generated (includes tool call tokens) |
| `total_tokens` | Sum of prompt + completion |

**Anthropic-specific fields** (available on `raw` response):

| Key | Description |
|-----|-------------|
| `cache_creation_input_tokens` | Tokens written to prompt cache (billed at 1.25× base rate) |
| `cache_read_input_tokens` | Tokens read from prompt cache (billed at ~0.1× base rate) |

**Cost calculation contract:**

The agent loop (WORKER_INFRASTRUCTURE.md §10.5) accumulates usage after every LLM call:

```
After each chat()/chat_with_tools() call:
  1. Extract response.usage → {prompt_tokens, completion_tokens, total_tokens}
  2. Look up per-model pricing: cost = (prompt_tokens × input_rate) + (completion_tokens × output_rate)
     - Anthropic cache adjustments: subtract cache_read_input_tokens from prompt_tokens at full rate,
       add them back at cached rate
  3. Accumulate into session totals: total_input_tokens += prompt_tokens,
     total_output_tokens += completion_tokens, total_cost_usd += cost
  4. Check budget: if total_cost_usd > guardrails.maxCostUsd → terminate session
```

On task completion (`transition_task_state`), the accumulated totals are reported to the backend:
- `POST /api/internal/tasks/:id/result` includes `{ inputTokens, outputTokens, costUsd }`
- The backend rolls up: `UPDATE jobs SET cost_usd = cost_usd + task.cost_usd`
- Session-level: `UPDATE chat_sessions SET total_input_tokens, total_output_tokens, total_cost_usd`

See API_AND_STORAGE.md §2.8 (`chat_sessions` schema), §3.15 (Spend History API), and FRONTEND_DESIGN.md §9.6 (`SpendBreakdown` component) for the full cost tracking pipeline.

### 14.6 Error Handling & Retry

Three error types with distinct semantics:

| Error | Base | Retry? | Agent Behavior |
|-------|------|--------|---------------|
| `LLMProviderError` | `SemliftError` | No | Fatal — session fails |
| `LLMRateLimitError` | `RetryableError` | Yes | Exponential backoff, `retry_after` hint |
| `LLMAuthenticationError` | `AuthenticationError` | No | Fatal — bad credentials |

**Provider-level retries** (inside the provider, before errors reach the agent loop):
- OpenAI: 3 attempts for API errors (400, 429, 5xx), exponential backoff + jitter.
- OpenAI streaming: 3 attempts for transient I/O errors (`RemoteProtocolError`, `ReadTimeout`).
- Sequential: automatic fallback to next provider on any failure.

**Agent-loop-level retries** (AGENTS.md §12.3):
- 3 retry attempts with exponential backoff on `LLMProviderError`.
- Session marked `FAILED` if all retries exhausted.

**Timeout configuration pattern** (shared across providers):
```python
Timeout(
    connect=10.0,       # Connection establishment
    write=30.0,         # Request body upload
    read=timeout,       # Response wait (default 600s for long generations)
    pool=max(timeout * 2, 1800)  # Connection pool (≥ 30 min for streaming)
)
```

### 14.7 Reasoning Model Considerations

Models with chain-of-thought capabilities (OpenRouter reasoning models, Gemini) produce `reasoning_details` — structured thinking blocks that must be round-tripped in subsequent API calls.

**Constraints:**
- Reasoning details must be preserved on the `Message` and re-sent in the next request for models that require signature continuity.
- Some models (Gemini) generate `thought_signatures` that are cryptographic — fabricating or omitting them causes 400 errors.
- For models requiring signatures on tool calls, `parallel_tool_calls` must remain `false` (each tool call needs its own signature).
- Dialog prepopulation (§10) cannot use synthetic tool-call pairs for these models — it falls back to a single context-injection user message.

---

## 15. Cross-Reference Map

| Topic | Location |
|-------|----------|
| Agent loop implementation | WORKER_INFRASTRUCTURE.md §10.5 |
| Session data model | WORKER_INFRASTRUCTURE.md §10.4 |
| Tool availability | WORKER_INFRASTRUCTURE.md §10.7 |
| Session API | WORKER_INFRASTRUCTURE.md §10.13 |
| Message injection | WORKER_INFRASTRUCTURE.md §10.8 |
| Execution intervention | WORKER_INFRASTRUCTURE.md §10.9 |
| Task allocation flow | WORKER_INFRASTRUCTURE.md §10.10 |
| IDE integration (Open in IDE) | WORKER_INFRASTRUCTURE.md §13 |
| Jobs & Tasks API | API_AND_STORAGE.md §3.4 |
| Task database schema | API_AND_STORAGE.md §2 |
| Workspace types & cloning | API_AND_STORAGE.md §2.2, §3.1 |
| Cost tracking schema | API_AND_STORAGE.md §2.6 (jobs), §2.7 (tasks), §2.8 (sessions) |
| Spend API | API_AND_STORAGE.md §3.15 |
| Usage records table | API_AND_STORAGE.md §2.12 |
| Chat components | FRONTEND_DESIGN.md §9.4 |
| Task timeline view | FRONTEND_DESIGN.md §9.14 |
| Monaco editor integration | FRONTEND_DESIGN.md §9.13 |
| Spend breakdown UI | FRONTEND_DESIGN.md §9.6 |
| Billing report | FRONTEND_DESIGN.md §9.16 |
| Clone & workspace UI | FRONTEND_DESIGN.md §9.19 |
| Port forwarding | WORKER_INFRASTRUCTURE.md §1.4.2 |
| WebSocket session streaming | WORKER_INFRASTRUCTURE.md §10.3, §10.5 |
| LLM provider archive | prompts/llm/ |

---

## 16. Future Work

| Item | Priority | Notes |
|------|----------|-------|
| Multi-agent coordination | P1 | Multiple agents collaborating on a shared task graph with inter-task communication. |
| Speculative execution | P2 | Start downstream tasks before upstream completes, with rollback on failure. |
| Agent-generated ontology proposals | P2 | Agent can propose new ontology nodes (subject to human approval). |
| Fine-tuned coding models | P1 | Use SFT data from §13 to train specialized uplift/downlift models. |
| Roundtrip evaluation benchmark | P1 | Automated eval: uplift → downlift → compare (SpecBench integration). |
| Language-specific validators | P2 | Built-in validators per language (currently relies on generic `run_in_terminal`). |
| Review scheduling | P2 | Automatic review tasks on schedule or after N commits. |
| Cost attribution | P1 | Per-workspace, per-task cost tracking and billing (schema in place — §14.5, API_AND_STORAGE.md §2.12). |
| Model pricing table | P2 | Server-side pricing configuration per model for automated cost calculation. |
| Provider health monitoring | P2 | Track error rates, latency percentiles, and cache hit ratios per provider. |
