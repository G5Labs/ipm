# G5 Codebase Analysis — Detailed Design

> **Import order simulation, dependency graph construction, cycle resolution, and hierarchical summarization.**
> Parent reference: AGENTS.md §3 (overview), WORKER_INFRASTRUCTURE.md §10 (execution environment).

---

## 0. Purpose

This document specifies the algorithms and data structures used to analyze a codebase, extract its dependency graph, break cycles, and produce the task DAG that drives uplift and downlift. AGENTS.md §3 defines *what* the analysis produces and *which languages* are supported; this document defines *how* — with enough detail to implement and test each subsystem.

---

## 1. Data Model

Analysis produces its own data structures — `AnalysisNode`, `AnalysisGraph` — that are **distinct from the runtime `Task`** (API_AND_STORAGE.md `tasks` table) and `TaskContext` (AGENTS.md §9). Analysis artifacts describe the *structure* of the codebase; runtime Tasks describe *work to be done*. The bootstrap phase (AGENTS.md §3.6) converts analysis output into runtime Task rows:

```
AnalysisNode (analysis phase)           Task (runtime)
├── component_id                  →     node_id (FK to ontology_nodes)
├── src_files                     →     (stored on ontology_node)
├── dependencies: list[str]       →     depends_on: UUID[]
├── node_level                    →     (inferred from ontology depth)
└── language, analysis_method     →     (metadata on ontology_node)
```

### 1.1 AnalysisNode

The atomic unit of the analysis graph. One AnalysisNode per source file (FILE level) or per directory/project (MODULE/PROJECT level).

```python
class AnalysisNode:
    # Identity
    component_id: str               # Hierarchical ID from IDService (§1.4)
    src_files: list[str]            # Relative paths to source files
    node_level: NodeLevel           # FILE | MODULE | PROJECT

    # Graph edges
    dependencies: list[str]         # component_ids this node depends on
    parent_spec: str | None         # Parent in the ontology hierarchy
    child_specs: list[str]          # Children (for aggregation nodes)

    # Metadata
    language: str                   # "python", "cpp", "rust", "java", "go", "typescript", "javascript", "unknown"
    analysis_method: str            # e.g., "python_import_simulation", "cpp_include_simulation"
```

### 1.2 NodeLevel

```python
class NodeLevel(str, Enum):
    FILE = "file"         # Individual source file
    MODULE = "module"     # Directory aggregation
    PROJECT = "project"   # Root aggregation
```

### 1.3 AnalysisGraph

The intermediate representation between import extraction and task DAG construction. Carries both forward and reverse edge indices for efficient traversal:

```python
class AnalysisGraph:
    nodes: dict[str, AnalysisNode]          # component_id → node
    edges: dict[str, set[str]]              # component_id → set of dependency component_ids
    reverse_edges: dict[str, set[str]]      # component_id → set of dependent component_ids (computed)

    def add_node(self, node: AnalysisNode) -> None: ...
    def add_edge(self, source: str, target: str) -> None: ...
    def remove_edge(self, source: str, target: str) -> None: ...
    def find_sccs(self) -> list[list[str]]: ...              # Tarjan's (§4)
```

Both `edges` and `reverse_edges` are maintained in sync so that operations like "find all dependents of node X" are O(1) lookup + O(degree) iteration rather than full-graph scans.

### 1.5 Handoff to the API Server

Analysis produces a set of `AnalysisNode`s with `dependencies` lists. It does **not** compute execution order — that is the API server's responsibility. During job bootstrap (AGENTS.md §3.6), the bootstrap worker submits the analyzed task graph to the server via `POST /api/internal/jobs/:id/tasks` (API_AND_STORAGE.md §3.4.2):

```python
def submit_task_graph(nodes: list[AnalysisNode], job_id: UUID, api_client: APIClient) -> None:
    """Convert analysis nodes to SubmitTaskGraphRequest and POST to the server."""
    # Maintain component_id → ontology UUID mapping so that string-based
    # analysis edges become UUID-based dependsOn arrays in the request.
    id_map: dict[str, UUID] = resolve_ontology_ids(nodes)

    tasks = [
        {
            "nodeId": str(id_map[node.component_id]),
            "name": f"{node.node_level}: {node.component_id}",
            "dependsOn": [str(id_map[dep]) for dep in node.dependencies],
        }
        for node in nodes
    ]

    api_client.post(f"/api/internal/jobs/{job_id}/tasks", json={"tasks": tasks})
```

The API server validates acyclicity on insertion (rejecting the job if the submitted graph contains cycles — which would indicate a bug in analysis), creates task rows, transitions the job to `running`, and begins locality-aware scheduling. See API_AND_STORAGE.md §3.4.3 for the scheduler.

### 1.4 Component ID Generation (IDService)

Component IDs are derived deterministically from file paths:

| Input | Output | Rule |
|-------|--------|------|
| `src/auth/validator.py` | `src/auth/validator` | Strip `.py` extension |
| `src/auth/__init__.py` | `src/auth` | Collapse to parent directory |
| `src/engine/physics.cpp` | `src/engine/physics_cpp` | Replace `.` with `_` (preserve extension to avoid header/source collision) |
| `src/engine/physics.h` | `src/engine/physics_h` | Same rule — `.h` becomes `_h` |
| `src/auth/` (module) | `src/auth_aggregate` | Append `_aggregate` for MODULE level |
| `project_root` | `<project_name>_project_aggregate` | PROJECT level uses project name |

The ID is the **relative path** from project root, normalized to forward slashes, with language-specific extension handling. This ensures:
- IDs are stable across platforms (no backslashes).
- C/C++ header and source files in the same directory don't collide.
- Python `__init__.py` files are identified with their package, not a redundant leaf node.

---

## 2. Pipeline Overview

```
┌────────────────────────────────────────────────────────────────────────┐
│                       Analysis Pipeline                                │
│                                                                        │
│  ┌──────────────────┐                                                 │
│  │ 1. File Discovery │  Per-language, filtered (§3)                   │
│  └────────┬─────────┘                                                 │
│           ▼                                                            │
│  ┌──────────────────┐                                                 │
│  │ 2. Import Parsing │  Language-specific extractors (AGENTS.md §3.2) │
│  └────────┬─────────┘                                                 │
│           ▼                                                            │
│  ┌──────────────────┐                                                 │
│  │ 3. Resolution     │  Import path → component_id (§3)               │
│  └────────┬─────────┘                                                 │
│           ▼                                                            │
│  ┌──────────────────────────────────────────┐                         │
│  │ 4. Import Simulation                     │  Per-language (§4.2)    │
│  │    (eliminate false cycles)               │                        │
│  └────────┬─────────────────────────────────┘                         │
│           ▼                                                            │
│  ┌──────────────────────────────────────────┐                         │
│  │ 5. Cycle Breaking                        │  Generic: Tarjan's SCC  │
│  │    (deterministic edge removal)           │  + DAG validation (§4) │
│  └────────┬─────────────────────────────────┘                         │
│           ▼                                                            │
│  ┌────────────────────────────────┐                                   │
│  │ 6. Hierarchical Aggregation    │  Modules + project root (§5)      │
│  └────────┬───────────────────────┘                                   │
│           ▼                                                            │
│  ┌──────────────────┐                                                 │
│  │ 7. Pruning        │  Collapse trivial aggregates (§5.6)            │
│  └────────┬─────────┘                                                 │
│           ▼                                                            │
│  AnalysisGraph (DAG) — submitted to API for task creation (§1.5)      │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Import Resolution

Each language analyzer (AGENTS.md §3.2.2) produces a raw list of import references per file. **Resolution** maps each reference to a `component_id` in the project, or discards it as external.

### 3.1 Common Resolver Interface

All language resolvers implement the same interface:

```python
class ImportResolver(ABC):
    def extract(self, path: str, content: str) -> list[RawImport]: ...
    def classify(self, raw: RawImport) -> ImportKind: ...   # INTERNAL | EXTERNAL | AMBIGUOUS
    def resolve(self, raw: RawImport, context: ResolverContext) -> str | None: ...
```

Each resolver follows the same flow: **extract** raw import strings from source → **classify** each as internal or external → **resolve** internal imports to a `component_id` → **dedup** (remove self-references and duplicates).

The classification step is critical for correctness — incorrectly classifying a stdlib import as internal would create a phantom dependency on a nonexistent node. Each language has its own classification heuristics (see subsections below).

Resolution strategies vary per language. Python has several cases to handle (relative imports, package `__init__`, dotted paths) while Go is straightforward (module prefix check + directory lookup). The subsections below show each language's resolution logic.

### 3.2 Python Resolution

Python's module system is complex (relative imports, package `__init__`, dotted paths). Resolution proceeds through these cases in order, stopping at the first match:

```
Input: "from auth.tokens import validate" (in file src/api/routes.py)

1. Exact match:
   → Look up "auth.tokens" in module_to_spec_id map
   → Found: "src/auth/tokens" → done

2. Prefix match (progressive truncation):
   → Truncate rightmost segment, try "auth"
   → Maps to "src/auth" → done (coarser but valid)

3. Package __init__ lookup:
   → Look for "src/auth/tokens/__init__.py"
   → If exists, maps to "src/auth/tokens" → done

4. Parent module lookup:
   → Look for "src/auth" (parent package)
   → If exists with __init__.py → done (import may be re-exported)

No match → classify as external (discard)
```

**Module-to-spec-id map construction:**

Before resolution, a map is built from all discovered Python files:

```python
# For src/auth/tokens.py:
module_to_spec_id = {
    "src.auth.tokens": "src/auth/tokens",      # Full dotted path
    "auth.tokens": "src/auth/tokens",           # Without top-level src
    "tokens": "src/auth/tokens",                # Leaf name (first-come wins on collision)
}

# For src/auth/__init__.py:
module_to_spec_id["src.auth"] = "src/auth"
module_to_spec_id["auth"] = "src/auth"
```

**Collision semantics**: Shorter paths (leaf names) are registered first-come. If two files both export the leaf name `utils`, only the first discovered registration wins. This is a known limitation — ambiguous short imports may resolve to the wrong file. The agent's LLM can compensate during uplift since it reads the actual source.

**Relative import resolution:**

```python
# In src/auth/middleware.py:
# "from . import tokens" → base_module = "src.auth" → "src.auth.tokens"
# "from ..api import routes" → base_module = "src" → "src.api.routes"
```

The base module is computed from the file's position in the directory tree, counting `..` levels to walk up.

### 3.3 C/C++ Resolution

C/C++ `#include` resolution mirrors how compilers search for headers:

```
Input: #include "auth/validator.h" (in file src/engine/physics.cpp)

1. Relative to source file:
   → Try src/engine/auth/validator.h → NOT FOUND

2. Configured include directories (from CMake / compile_commands.json):
   → Try include_dirs[i]/auth/validator.h for each configured dir
   → Found in src/: src/auth/validator.h → done

3. Relative to project root:
   → Try <project_root>/auth/validator.h → done (if above didn't fire)

4. Filename search (prebuilt index):
   → Look up "validator.h" in _file_path_dict → returns all files named validator.h
   → If exactly one match → done
   → If multiple matches → ambiguous, pick the one with the shortest relative path

No match → classify as external (discard)
```

**System header classification:**

Before resolution, `#include <...>` directives are filtered against an extensive skip list:

```python
SYSTEM_HEADERS = {
    # C stdlib
    "stdio.h", "stdlib.h", "string.h", "math.h", "assert.h", ...
    # C++ stdlib
    "iostream", "vector", "map", "string", "memory", "algorithm", ...
    # POSIX
    "unistd.h", "pthread.h", "sys/types.h", ...
    # Common libraries (Boost, Eigen, OpenCV, Qt, etc.)
    "boost/", "eigen/", "opencv2/", "Qt", ...
}
```

Any `#include` matching a prefix in this list is classified as external and skipped. `#include "..."` (quoted) directives are always attempted for resolution — they may reference project-internal headers.

**Prebuilt filename index:**

At initialization, the resolver walks the entire project tree once and builds:

```python
_file_path_dict: dict[str, list[Path]] = {}
# "validator.h" → [Path("src/auth/validator.h"), Path("src/legacy/validator.h")]
```

This enables O(1) filename lookup for the prebuilt index step. The walk excludes build directories and hidden directories.

### 3.4 Rust Resolution

Rust's module system is tree-structured (no arbitrary cycles at the module level). Resolution is straightforward — the crate root anchors all paths:

```
Input: use crate::auth::validator (in file src/engine/physics.rs)

Identify crate root:
  → Cargo.toml → crate root is src/lib.rs (or src/main.rs)

Module path → file path:
  → crate::auth::validator → src/auth/validator.rs
  → Or: src/auth/validator/mod.rs (Rust's alternate module layout)

Validate existence → MATCH or discard
```

**`mod` declarations** create implicit dependencies:
```rust
// In src/auth/mod.rs:
mod validator;   // → src/auth/validator.rs depends on src/auth/mod.rs
mod claims;      // → src/auth/claims.rs depends on src/auth/mod.rs
```

**Workspace support**: For `[workspace]` projects, the analyzer discovers members from `Cargo.toml` and runs resolution independently per crate, with cross-crate `use external_crate::` treated as external.

### 3.5 Java Resolution

Java's package-to-filesystem mapping is predictable once source roots are known:

```
Input: import com.example.auth.Validator (in file src/main/java/com/example/api/Routes.java)

Detect source roots:
  → Heuristics: src/main/java/, src/, or project root
  → Build tool: pom.xml <sourceDirectory>, build.gradle sourceSets

Package path → file path:
  → com.example.auth.Validator → com/example/auth/Validator.java
  → Search under all detected source roots

Match → MATCH or external
```

**Wildcard imports**: `import com.example.auth.*` → resolve to all `.java` files under `com/example/auth/` in the detected source roots. Creates dependency edges to every file in the package.

### 3.6 Go Resolution

Go dependencies are at the **package level** (directory), not the file level. Resolution is a simple prefix-strip:

```
Input: import "github.com/user/project/auth/validator" (in file cmd/server/main.go)

Read go.mod for module path:
  → module github.com/user/project

Strip module prefix:
  → "auth/validator" (relative to project root)

Directory → all .go files in that directory:
  → auth/validator/*.go (excluding _test.go for non-test tasks)

Dependency edge points to the MODULE aggregate for auth/validator/
```

**Stdlib detection**: Imports without a `.` in the path (e.g., `"fmt"`, `"net/http"`) are stdlib. Imports matching `golang.org/x/` are treated as external. Only imports with the project's module path prefix produce edges.

### 3.7 TypeScript / JavaScript Resolution

Both share a common `JSModuleResolver` base. Classification happens first (cheap string check), then resolution uses extension probing:

```
Input: import { validate } from './auth/validator' (in file src/api/routes.ts)

Classify specifier:
  → Starts with './' or '../' → relative (internal)
  → Starts with '@/' → path alias (check tsconfig.json)
  → Bare specifier ('lodash') → external (discard)

Resolve relative path (extension probing):
  → ./auth/validator → try in order:
     src/api/auth/validator.ts
     src/api/auth/validator.tsx
     src/api/auth/validator/index.ts
     src/api/auth/validator/index.tsx
     (for JS: .js, .jsx, .mjs, /index.js, /index.jsx)

Resolve path aliases (TypeScript only):
  → tsconfig.json: { "paths": { "@/*": ["src/*"] } }
  → '@/auth/validator' → 'src/auth/validator' → re-enter extension probing

Match → MATCH or external
```

**Monorepo support** (TypeScript): `tsconfig.json` `references` field lists sibling projects. Cross-project imports via path aliases are resolved using the referenced project's `tsconfig.json`.

---

## 4. Import Simulation & Cycle Breaking

The task graph **must be a DAG** for topological ordering, but many languages permit circular imports. G5 handles this in two phases:

1. **Import Simulation** — a per-language simulator models how the language's runtime/compiler actually processes imports, naturally eliminating "false" cycles that the language itself would handle (e.g., include guards, lazy evaluation, hoisting).
2. **Generic Cycle Breaking** — any cycles that survive simulation are genuine and must be broken. A shared `CycleBreaker` (Tarjan's SCC + deterministic edge removal) handles all languages uniformly.

This two-phase design means each language only needs to describe its own import semantics; the hard graph theory is shared.

### 4.1 ImportSimulator Interface

Every language analyzer provides a simulator:

```python
class ImportSimulator(ABC):
    """Simulates a language's import execution to produce effective dependencies."""

    @abstractmethod
    def simulate(
        self,
        graph: AnalysisGraph,
        entry_points: list[str],
    ) -> SimulationResult:
        """Walk the import graph as the language runtime would.

        Returns:
          - effective_edges: the dependency edges that matter for ordering
          - eliminated_edges: edges removed by simulation (with reasons)
          - warnings: diagnostics (e.g., Go cycle = compile error)
        """
        ...

class SimulationResult:
    effective_edges: dict[str, set[str]]        # component_id → deps (post-simulation)
    eliminated_edges: list[EliminatedEdge]       # Edges the simulator removed
    warnings: list[str]                          # Diagnostics

class EliminatedEdge:
    source: str
    target: str
    reason: str                                  # e.g., "include guard", "barrel re-export", "TYPE_CHECKING block"
```

The simulator receives the raw dependency graph (from §3 resolution) and returns an effective graph with false cycles already removed. The `CycleBreaker` (§4.3) then operates on the effective graph.

### 4.2 Per-Language Simulators

**C/C++ — `IncludeGuardSimulator`**

Models `#pragma once` / include guard behavior. DFS from each entry point; when a `#include` targets an already-visited file, the edge is eliminated (the preprocessor would skip the re-include):

```
Processing src/engine/physics.cpp:
  → #include "math_types.h"
    → #include "vector3.h"       (first visit — process)
      → #include "math_types.h"  (already visited — eliminated, include guard)
    → #include "quaternion.h"    (first visit — process)
```

This naturally breaks apparent cycles that the preprocessor would also break. Only "real" cycles (not resolvable by include guards) survive to the generic breaker.

**Python — `ModuleInitSimulator`**

Python allows circular imports — they execute with partially-initialized modules. The simulator models this:

- **`TYPE_CHECKING` blocks**: Imports inside `if TYPE_CHECKING:` are annotation-only and don't create runtime dependencies. Edges from these imports are eliminated.
- **Lazy / conditional imports**: Imports inside function bodies or `try/except` blocks are deferred. Edges from these are eliminated (they don't affect module initialization order).
- **Module initialization order**: DFS simulating Python's `import` machinery. When module A imports B and B imports A, B sees a partially-initialized A. The simulator marks the back edge (B → A) as a **weak dependency** — it exists but should be preferred for removal if the generic breaker needs to cut an edge.

```python
class ModuleInitSimulator(ImportSimulator):
    def simulate(self, graph, entry_points):
        result = SimulationResult(...)
        for node in graph.nodes:
            for dep in graph.edges.get(node, set()):
                edge_kind = self._classify_import(node, dep)
                if edge_kind == "type_checking":
                    result.eliminated_edges.append(
                        EliminatedEdge(node, dep, "TYPE_CHECKING block")
                    )
                elif edge_kind == "deferred":
                    result.eliminated_edges.append(
                        EliminatedEdge(node, dep, "deferred import (function body)")
                    )
                else:
                    result.effective_edges[node].add(dep)
        return result
```

**Rust — `ModuleTreeSimulator`**

Rust's module system is inherently tree-structured: each file is declared as a module exactly once via `mod` declarations. `use` paths are aliases, not load-order dependencies. The simulator:

- Treats `mod` declarations as the real dependency edges (parent declares child).
- Treats `use crate::...` paths as **resolution references** that don't create ordering constraints — the module is already loaded by its `mod` declaration.
- Only `use` paths that cross crate boundaries (within a workspace) create real edges.

Result: Rust projects almost never have cycles in the effective graph. If they do, it indicates a `[workspace]` cross-crate cycle — a genuine structural issue.

**Java — `ClassLoadSimulator`**

Java's class loader resolves classes lazily — `import com.example.Foo` doesn't trigger loading until `Foo` is actually used. The simulator:

- Treats `import` statements as potential edges.
- Eliminates edges where the imported class is only used in type signatures (not in static initializers or field initialization). These are analogous to forward declarations.
- Marks **static initializer dependencies** (class A's `static {}` block calls class B) as strong edges — these create real load-order requirements.

For most Java codebases, this eliminates the majority of apparent cycles (mutual type references are very common; mutual initialization dependencies are rare).

**Go — `PackageImportSimulator`**

Go's compiler **forbids** circular package imports entirely. The simulator is a validation pass:

- All edges are kept (Go has no mechanism to weaken them).
- If any cycles exist, the simulator emits a **warning** per cycle: "circular import between packages X and Y — code will not compile."
- Cycles are still passed to the generic breaker for DAG construction, but the warnings are surfaced to the user (these represent genuine codebase errors).

**TypeScript / JavaScript — `ModuleEvalSimulator`**

ES modules support circular imports via hoisting — `export function` declarations are available before the module body executes, but `export const` / `export class` may be `undefined` if accessed during circular initialization. The simulator:

- **Barrel re-export elimination**: `index.ts` files that only re-export from children (`export { Foo } from './foo'`) don't create real initialization dependencies on their re-export targets. Edges from barrel files to their re-exported children are eliminated.
- **Hoisted export detection**: When a circular edge exists and the imported binding is a `function` declaration (hoisted), the edge is marked as weak.
- **`import type` elimination**: TypeScript `import type { Foo }` is erased at runtime — these edges are eliminated.

This handles the most common TS/JS cycle pattern: `A → index.ts → B → A`, where `index.ts` is a barrel that re-exports both A and B.

### 4.3 Generic CycleBreaker

After simulation, any remaining cycles are genuine. The `CycleBreaker` uses **Tarjan's SCC algorithm** to find them and **deterministic edge removal** to break them:

```python
class CycleBreaker:
    """Language-agnostic cycle breaker. Operates on the post-simulation effective graph."""

    def break_cycles(self, graph: AnalysisGraph) -> list[EliminatedEdge]:
        """Find SCCs and remove edges to produce a DAG. Returns removed edges."""
        removed: list[EliminatedEdge] = []

        while True:
            sccs = self._find_sccs(graph.edges)
            if not sccs:
                break  # DAG achieved

            for scc in sccs:
                edge = self._select_edge_to_remove(scc, graph)
                graph.remove_edge(edge.source, edge.target)
                removed.append(edge)

        return removed
```

**Tarjan's SCC** (O(V + E) per pass):

```python
def _find_sccs(self, edges: dict[str, set[str]]) -> list[list[str]]:
    """Tarjan's SCC. Returns list of SCCs with size > 1 (or self-loops)."""
    index_counter = [0]
    stack: list[str] = []
    lowlink: dict[str, int] = {}
    index: dict[str, int] = {}
    on_stack: dict[str, bool] = {}
    sccs: list[list[str]] = []

    def strongconnect(node: str) -> None:
        index[node] = lowlink[node] = index_counter[0]
        index_counter[0] += 1
        stack.append(node)
        on_stack[node] = True

        for neighbor in edges.get(node, set()):
            if neighbor not in index:
                strongconnect(neighbor)
                lowlink[node] = min(lowlink[node], lowlink[neighbor])
            elif on_stack.get(neighbor, False):
                lowlink[node] = min(lowlink[node], index[neighbor])

        if lowlink[node] == index[node]:
            component: list[str] = []
            while True:
                w = stack.pop()
                on_stack[w] = False
                component.append(w)
                if w == node:
                    break
            if len(component) > 1 or (
                len(component) == 1
                and component[0] in edges.get(component[0], set())
            ):
                sccs.append(component)

    for node in edges:
        if node not in index:
            strongconnect(node)

    return sccs
```

**Deterministic edge selection:**

For each SCC, one edge is removed per iteration. The selection is deterministic (same input → same output):

```python
def _select_edge_to_remove(
    self, scc: list[str], graph: AnalysisGraph
) -> EliminatedEdge:
    """Pick the edge to remove from an SCC. Deterministic."""
    scc_set = set(scc)

    # Prefer edges already marked as "weak" by the simulator
    weak_candidates = [
        (src, tgt)
        for src in sorted(scc)
        for tgt in sorted(graph.edges.get(src, set()) & scc_set)
        if graph.edge_metadata.get((src, tgt), {}).get("weak", False)
    ]
    if weak_candidates:
        src, tgt = weak_candidates[-1]  # Lexicographically last for determinism
        return EliminatedEdge(src, tgt, "weak dependency (simulator hint)")

    # No weak edges — pick lexicographically last edge in the SCC
    candidates = [
        (src, tgt)
        for src in sorted(scc)
        for tgt in sorted(graph.edges.get(src, set()) & scc_set)
    ]
    src, tgt = candidates[-1]
    return EliminatedEdge(src, tgt, "cycle breaker (lexicographic)")
```

**Determinism contract**: Given the same set of files and imports, the same edges are always removed. This is critical for:
- Reproducible task graphs across analysis runs.
- Stable spec IDs (removing different edges would change dependency lists).
- Debuggability (the user can predict which edges will be cut).

### 4.4 Pipeline: Simulation → Breaking → Validation

```
┌─────────────────────────┐
│  Raw edges from §3      │  (may contain cycles)
└──────────┬──────────────┘
           ▼
┌─────────────────────────┐
│  ImportSimulator         │  Per-language: eliminates false cycles
│  (language-specific)     │  Returns: effective_edges + eliminated_edges
└──────────┬──────────────┘
           ▼
┌─────────────────────────┐
│  CycleBreaker            │  Generic: Tarjan's SCC + deterministic removal
│  (language-agnostic)     │  Returns: additional eliminated_edges
└──────────┬──────────────┘
           ▼
┌─────────────────────────┐
│  DAG Validation          │  Assert: topological sort consumes all nodes
└──────────┬──────────────┘
           ▼
        AnalysisGraph (DAG)
```

### 4.5 DAG Validation

After cycle breaking, the graph must be verified acyclic. Run Tarjan's SCC one final time — if any SCC with size > 1 remains, the cycle-breaking pass has a bug:

```python
def validate_dag(graph: AnalysisGraph) -> None:
    """Assert the graph is a valid DAG. Raises if cycles remain."""
    sccs = graph.find_sccs()
    assert not sccs, f"Cycle breaker failed — {len(sccs)} SCCs remain: {sccs}"
```

This is a hard assertion — analysis aborts and the job transitions to `FAILED` rather than submitting an invalid graph to the API.

---

## 5. Hierarchical Aggregation

After individual file AnalysisNodes are created with their dependencies, the analyzer builds a directory-based hierarchy of MODULE and PROJECT aggregation nodes.

### 5.1 Why Aggregate

Individual file specs are too granular for humans to navigate. A project with 200 files needs summaries at the module level ("what does `src/auth/` do?") and the project level ("what does this project do?"). Aggregation provides:

1. **Navigable hierarchy**: Users browse module specs, drill into files.
2. **Summarization**: The uplift agent reads child specs and produces a module summary — a hierarchical summarization of the codebase.
3. **Cross-module dependency tracking**: File-level edges are within-module; aggregate edges capture the module-level dependency structure.
4. **Scheduling**: Aggregation tasks depend on all their children, naturally enforcing bottom-up ordering in uplift.

### 5.2 Directory Cataloging

The first step groups files by their parent directory:

```python
def catalog_by_directory(
    nodes: list[AnalysisNode], project_root: Path
) -> DirectoryCatalog:
    """Group AnalysisNodes by parent directory, register all ancestor dirs."""

    directory_nodes: dict[str, list[AnalysisNode]] = defaultdict(list)
    child_directories: dict[str, set[str]] = defaultdict(set)
    project_root_files: list[AnalysisNode] = []

    for node in nodes:
        rel_path = Path(node.src_files[0]).relative_to(project_root)
        parent_dir = str(rel_path.parent)

        if parent_dir == ".":
            project_root_files.append(node)
        else:
            directory_nodes[parent_dir].append(node)

            # Register all ancestor directories
            parts = Path(parent_dir).parts
            for i in range(len(parts)):
                ancestor = str(Path(*parts[:i+1]))
                if i + 1 < len(parts):
                    child = str(Path(*parts[:i+2]))
                    child_directories[ancestor].add(child)

    return DirectoryCatalog(
        directory_nodes=directory_nodes,
        child_directories=child_directories,
        project_root_files=project_root_files,
    )
```

### 5.3 Bottom-Up MODULE Aggregation

Directories are processed **deepest first** (bottom-up):

```python
def create_module_aggregates(
    catalog: DirectoryCatalog,
    file_graph: AnalysisGraph,
) -> list[AnalysisNode]:
    """Create MODULE aggregation nodes, bottom-up by depth."""

    # Sort directories by depth (deepest first)
    all_dirs = sorted(
        catalog.directory_tasks.keys(),
        key=lambda d: d.count('/'),
        reverse=True,
    )

    aggregates: list[AnalysisNode] = []
    dir_to_aggregate_id: dict[str, str] = {}

    for dir_path in all_dirs:
        child_ids: list[str] = []

        # Direct file children
        for node in catalog.directory_nodes.get(dir_path, []):
            child_ids.append(node.component_id)

        # Child directory aggregates (already created — we process bottom-up)
        for child_dir in catalog.child_directories.get(dir_path, set()):
            if child_dir in dir_to_aggregate_id:
                child_ids.append(dir_to_aggregate_id[child_dir])

        if not child_ids:
            continue

        # Create MODULE aggregate
        agg_id = f"{dir_path}_aggregate"
        agg = AnalysisNode(
            component_id=agg_id,
            src_files=[],
            node_level=NodeLevel.MODULE,
            dependencies=calculate_aggregate_dependencies(
                child_ids, file_graph, dir_to_aggregate_id
            ),
            child_specs=child_ids,
        )
        aggregates.append(agg)
        dir_to_aggregate_id[dir_path] = agg_id

        # Set parent_spec on each child
        for child_id in child_ids:
            file_graph.nodes[child_id].parent_spec = agg_id

    return aggregates
```

### 5.4 Aggregate Dependency Calculation

The most complex subsystem. When a MODULE aggregates files, it must also aggregate their **external** dependencies — dependencies that point outside the module:

```
Module src/auth/ contains:
  - validator.py → depends on: src/db/connection, src/auth/claims
  - claims.py → depends on: src/auth/validator (internal — skip)

src/auth_aggregate dependencies:
  - src/db/connection (external to this module)
  - NOT src/auth/validator or src/auth/claims (internal — within module)
```

The algorithm:

```python
def calculate_aggregate_dependencies(
    child_ids: list[str],
    graph: AnalysisGraph,
    dir_to_aggregate_id: dict[str, str],
) -> list[str]:
    """Compute MODULE-level dependencies by collecting external deps of children."""

    children_set = set(child_ids)
    external_deps: set[str] = set()

    for child_id in child_ids:
        for dep_id in graph.edges.get(child_id, set()):
            if dep_id in children_set:
                continue  # Internal — skip

            # Walk up to find the dep's containing aggregate
            containing_agg = find_containing_aggregate(dep_id, dir_to_aggregate_id)

            if containing_agg and containing_agg not in children_set:
                external_deps.add(containing_agg)
            elif not containing_agg:
                # dep is a root-level file — add directly
                external_deps.add(dep_id)

    return sorted(external_deps)
```

**`find_containing_aggregate`** walks up the directory tree of a dep's source file to find its nearest MODULE aggregate:

```python
def find_containing_aggregate(
    dep_id: str, dir_to_aggregate_id: dict[str, str]
) -> str | None:
    """Find the MODULE aggregate that contains the given component_id."""
    dep_path = component_id_to_path(dep_id)
    parent = str(dep_path.parent)

    while parent != ".":
        if parent in dir_to_aggregate_id:
            return dir_to_aggregate_id[parent]
        parent = str(Path(parent).parent)

    return None
```

This produces **module-level dependency edges**: `src/auth_aggregate → src/db_aggregate` instead of the fine-grained `src/auth/validator → src/db/connection`. Both granularities are preserved in the graph (file-level for the agent's analysis, module-level for scheduling and UI display).

### 5.5 PROJECT Aggregation

After all MODULE aggregates are created, a single PROJECT aggregate is created at the root:

```python
def create_project_aggregate(
    module_aggregates: list[AnalysisNode],
    root_files: list[AnalysisNode],
    project_name: str,
) -> AnalysisNode | None:
    """Create the PROJECT-level root aggregate."""

    top_level_ids = (
        [agg.component_id for agg in module_aggregates if "/" not in agg.component_id.replace("_aggregate", "")]
        + [f.component_id for f in root_files]
    )

    if len(top_level_ids) <= 1:
        # Single top-level child — promote it to root, no project aggregate needed
        return None

    return AnalysisNode(
        component_id=f"{project_name}_project_aggregate",
        src_files=[],
        node_level=NodeLevel.PROJECT,
        dependencies=[],  # PROJECT depends on nothing — it's the root
        child_specs=top_level_ids,
    )
```

### 5.6 Pruning Trivial Aggregates

Aggregates with ≤1 child add hierarchy without value. Pruning collapses them:

```python
def prune_trivial_aggregates(
    aggregates: list[AnalysisNode],
    graph: AnalysisGraph,
) -> list[AnalysisNode]:
    """Iteratively collapse aggregates with 0 or 1 children."""

    progress = True
    while progress:
        progress = False
        to_remove: list[str] = []

        for agg in aggregates:
            if agg.node_level == NodeLevel.PROJECT:
                continue  # Never prune the project root

            if len(agg.child_specs) <= 1:
                parent_id = agg.parent_spec
                child_id = agg.child_specs[0] if agg.child_specs else None

                # Promote child to grandparent
                if parent_id and child_id:
                    parent = graph.nodes[parent_id]
                    parent.child_specs.remove(agg.component_id)
                    parent.child_specs.append(child_id)
                    graph.nodes[child_id].parent_spec = parent_id

                # Remove the trivial aggregate
                to_remove.append(agg.component_id)
                progress = True

        aggregates = [a for a in aggregates if a.component_id not in to_remove]
        for agg_id in to_remove:
            del graph.nodes[agg_id]

    # Recompute aggregate dependencies after structural changes
    refresh_aggregate_dependencies(aggregates, graph)

    return aggregates
```

**Cascading**: Pruning one aggregate may make its parent trivial (now has only 1 child). The `while progress` loop handles this.

### 5.7 Post-Pruning Dependency Refresh

After pruning changes the tree structure, aggregate dependencies must be recomputed:

```python
def refresh_aggregate_dependencies(
    aggregates: list[AnalysisNode],
    graph: AnalysisGraph,
) -> None:
    """Recompute all aggregate dependencies after pruning."""
    # Process bottom-up (deepest first)
    for agg in sorted(aggregates, key=lambda a: a.component_id.count("/"), reverse=True):
        agg.dependencies = calculate_aggregate_dependencies(
            agg.child_specs, graph, ...
        )
```

---

## 6. Caching

### 6.1 File Content Cache

During analysis, multiple subsystems read the same files (import extraction, interface extraction, line counting). A content cache avoids redundant I/O:

```python
class FileContentCache:
    """Thread-safe file content cache with mtime invalidation."""
    _cache: dict[str, tuple[float, str]]  # path → (mtime, content)

    def read(self, path: str) -> str:
        mtime = os.path.getmtime(path)
        cached = self._cache.get(path)
        if cached and cached[0] == mtime:
            return cached[1]
        content = Path(path).read_text(encoding="utf-8", errors="replace")
        self._cache[path] = (mtime, content)
        return content
```

**Invalidation**: mtime-based. If the file changes between analysis runs (incremental re-analysis), stale entries are evicted on the next read.

### 6.2 Parse Result Cache

AST parsing (Python) and regex extraction (C++, Java, etc.) are expensive for large files. Parse results are cached:

```python
class ParseCache:
    """Cache parsed import lists keyed by (path, content_hash)."""
    _cache: dict[str, tuple[str, list[str]]]  # path → (content_sha256, imports)

    def get_imports(self, path: str, content: str) -> list[str] | None:
        cached = self._cache.get(path)
        if cached and cached[0] == hashlib.sha256(content.encode()).hexdigest():
            return cached[1]
        return None

    def set_imports(self, path: str, content: str, imports: list[str]) -> None:
        content_hash = hashlib.sha256(content.encode()).hexdigest()
        self._cache[path] = (content_hash, imports)
```

**Content-hash keyed**: Unlike mtime, content hashing catches cases where a file is rewritten with the same content (e.g., `git checkout` back to the same version). The SHA256 is computed from the content already in memory (from the FileContentCache), so there's no extra I/O.

### 6.3 Include Path Resolution Cache

C/C++ include resolution is called repeatedly for the same header across many source files. Results are memoized:

```python
class IncludePathResolver:
    _resolved_paths: dict[str, Path | None]  # include_string → resolved Path

    def resolve(self, include: str, source_dir: Path) -> Path | None:
        # Cache key includes source_dir because relative resolution depends on it
        cache_key = f"{include}:{source_dir}"
        if cache_key in self._resolved_paths:
            return self._resolved_paths[cache_key]

        result = self._resolve_impl(include, source_dir)
        self._resolved_paths[cache_key] = result
        return result
```

### 6.4 Persistent Interface Cache

For incremental analysis (re-running uplift after a small change), a two-level cache persists extracted interfaces to disk:

```python
class InterfaceCache:
    """Two-level cache: memory dict + pickle-on-disk."""
    _memory: dict[str, InterfaceResult]
    _disk_dir: Path                         # .semlift/cache/interfaces/

    def get(self, path: str, content_hash: str) -> InterfaceResult | None:
        # Level 1: memory
        if path in self._memory and self._memory[path].hash == content_hash:
            return self._memory[path]

        # Level 2: disk (pickle)
        disk_path = self._disk_dir / f"{path_to_cache_key(path)}.pkl"
        if disk_path.exists():
            result = pickle.loads(disk_path.read_bytes())
            if result.hash == content_hash:
                self._memory[path] = result
                return result

        return None  # Cache miss

    def put(self, path: str, content_hash: str, result: InterfaceResult) -> None:
        result.hash = content_hash
        self._memory[path] = result
        disk_path = self._disk_dir / f"{path_to_cache_key(path)}.pkl"
        disk_path.parent.mkdir(parents=True, exist_ok=True)
        disk_path.write_bytes(pickle.dumps(result))
```

### 6.5 Dependency-Aware Cache Invalidation

When a file changes, its cache entry is invalidated — but so are the entries of all files that **depend** on it. The interface cache tracks a bidirectional dependency graph for transitive invalidation:

```python
class DependencyAwareCache(InterfaceCache):
    _dependents: dict[str, set[str]]  # path → paths that depend on it

    def invalidate(self, path: str) -> set[str]:
        """Invalidate path and all transitive dependents. Returns all invalidated paths."""
        invalidated: set[str] = set()
        stack = [path]

        while stack:
            current = stack.pop()
            if current in invalidated:
                continue
            invalidated.add(current)
            self._memory.pop(current, None)
            self._evict_disk(current)

            for dependent in self._dependents.get(current, set()):
                stack.append(dependent)

        return invalidated
```

This ensures that when `base_types.py` changes, all files that transitively import it have their cached specs invalidated, even if their own content hasn't changed (their *interface* may change due to re-exported types from the changed dependency).

---

## 7. Hierarchical Summarization Strategy

The aggregation hierarchy drives a **bottom-up summarization** during uplift:

```
Step 1: Uplift leaf files (level 0)
  → Each produces a SemliftSpec describing that file's interface

Step 2: Uplift module aggregates (level 1+)
  → Agent reads all child specs for the module
  → Produces a MODULE-level spec summarizing:
    - The module's collective purpose
    - Key public interfaces (aggregated from children)
    - Internal architecture (how children interact)
    - External dependencies (at the module boundary)

Step 3: Uplift project aggregate (final level)
  → Agent reads all module specs
  → Produces a PROJECT-level spec summarizing:
    - Project purpose and architecture
    - Module inventory with brief descriptions
    - External dependency summary
    - Entry points and configuration
```

**Context window management**: Module aggregation specs must fit within the LLM's context window along with all child specs. For a module with 20 files, each producing a ~2K-token spec, the aggregation agent sees ~40K tokens of child context. The dialog compactor (AGENTS.md §11) manages this, but the aggregation design assumes:
- Individual file specs are **concise** (under 3K tokens).
- Module specs summarize, not concatenate — the module spec should be shorter than the sum of its children.
- Deep hierarchies (>3 levels) are unlikely in practice; most projects have file → module → project.

### 7.1 SCC Closure as Aggregation Units

When the cycle breaker removes edges from an SCC, the result is a DAG — but the original SCC membership is still informative. Files in an SCC are **tightly coupled**: they form a mutual dependency cluster that likely represents a cohesive subsystem.

G5 preserves SCC membership as a **grouping hint** for the uplift agent:

```python
class SCCGroup:
    """A group of files that were in the same SCC before cycle breaking."""
    members: list[str]           # component_ids
    removed_edges: list[tuple[str, str]]  # Edges cut to break the cycle
    directory: str               # Common parent directory (if any)
```

During uplift, the agent for an SCC member's spec receives extra context:

```
This file is part of a tightly-coupled group with:
  - src/auth/validator.py
  - src/auth/middleware.py
  - src/auth/session.py

These files form a circular dependency cluster. The following edges were
removed for ordering purposes:
  - middleware.py → validator.py (removed)

When describing this file's interface, note its coupling with the group.
The module-level spec will describe the group's collective behavior.
```

This gives the agent awareness that the file is part of a cluster, so it can write specs that reference the group rather than treating each file as isolated.

### 7.2 Dependency Group Visualization

The frontend (FRONTEND_DESIGN.md §9.5) renders the task graph with SCC groups as visual clusters:

```
┌─ src/auth (SCC group) ─────────────────────┐
│  ┌────────────┐  ┌────────────┐            │
│  │ validator   │──│ middleware  │            │
│  └────────────┘  └──────┬─────┘            │
│                         │                   │
│                  ┌──────┴─────┐            │
│                  │  session    │            │
│                  └────────────┘            │
│  ╌╌╌╌╌ removed edge (middleware→validator) │
└─────────────────────────────────────────────┘
         │
         ▼
  ┌────────────┐
  │  src/db    │
  └────────────┘
```

SCC groups are rendered with a shared border. Removed edges are shown as dashed lines. This helps users understand why certain files are grouped and which dependency was considered "weaker" by the cycle breaker.

---

## 8. Performance Expectations

### 8.1 Complexity Targets

| Phase | Time Complexity | Space | Notes |
|-------|----------------|-------|-------|
| File discovery | O(F) | O(F) | F = total files in project tree |
| Import parsing | O(F × L) | O(F) | L = avg lines per file |
| Resolution | O(I × R) | O(F) | I = total imports, R = max resolution steps per import |
| Import simulation | O(V + E) | O(V + E) | Per-language DFS; eliminates false cycles |
| Cycle breaking (SCC) | O(S × (V + E)) | O(V) | S = number of residual SCCs (usually 0 after simulation) |
| Aggregation | O(D × F) | O(D + F) | D = directories |
| **Total** | **O(F × L + S × (V + E))** | **O(F + E)** | Dominated by parsing for clean code; simulation eliminates most SCCs |

### 8.2 Benchmarks (Target)

| Codebase Size | Files | Expected Analysis Time |
|--------------|-------|----------------------|
| Small (10K LoC) | ~50 | < 2 seconds |
| Medium (50K LoC) | ~200 | < 10 seconds |
| Large (200K LoC) | ~800 | < 30 seconds |
| Monorepo (1M LoC) | ~5000 | < 2 minutes |

Bottleneck for large codebases is file I/O (reading all files for import extraction). The prebuilt filename index (C++) and module-to-spec-id map (Python) are O(F) construction, O(1) lookup.

### 8.3 Incremental Analysis

When re-running analysis after a small change (e.g., one file modified):

1. **Content cache hit**: Unchanged files return cached parse results (§6.2).
2. **Dependency-aware invalidation**: Changed file + transitive dependents are re-analyzed (§6.5).
3. **Graph diff**: Compare new graph to previous; only emit tasks for changed nodes.

Expected incremental time for a single-file change in a 200K LoC project: **< 3 seconds**.

---

## 9. Cross-References

| Topic | Location |
|-------|----------|
| Language-specific analyzer designs | AGENTS.md §3.2.2 |
| Task graph persistence & scheduling | API_AND_STORAGE.md §3.4 |
| Hierarchical summarization (uplift) | AGENTS.md §2.1 |
| Dialog compaction during aggregation | AGENTS.md §11 |
| Execution graph UI | FRONTEND_DESIGN.md §9.5 |
| Worker execution environment | WORKER_INFRASTRUCTURE.md §10 |
