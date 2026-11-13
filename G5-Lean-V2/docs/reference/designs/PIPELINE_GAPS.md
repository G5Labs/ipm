# Pipeline Gaps — Downlift / Uplift / Post-Completion

Tracking document for known gaps in the downlift ↔ uplift pipeline.  
Created 2026-03-02.

---

## 1. Post-Completion Hooks: Child Task Creation (No-Op)

**Status**: Scanned but discarded  
**Files**:
- `g5d/src/tools/task_lifecycle.rs` — `run_post_completion_hooks()` (L102–140)
- `g5d/src/tools/task_lifecycle.rs` — `_post_result` discard (L486)

**Problem**:  
`run_post_completion_hooks()` scans the completed spec's `child_spec_ids` frontmatter and returns a `PostCompletionResult { new_tasks: Vec<String> }`. The result is stored as `_post_result` (underscore-prefixed — intentionally unused). No API call is made to create follow-up tasks.

**Expected behavior**:  
When a task completes and the spec has `child_spec_ids`:
1. For each child spec that exists on disk, create a new task in the same job
2. For child specs that are **files** (leaf nodes), create downlift tasks
3. For child specs that are **components** (aggregate nodes), create scoped downlift tasks
4. If a new dependency is declared that maps to a task still in-flight, the current fail-and-retry path via `check_dependencies()` is acceptable

**Required changes**:
- [ ] Wire `PostCompletionResult::new_tasks` to a new API endpoint (`POST /api/internal/jobs/:id/tasks/append`) or extend `submit_task_graph` to accept incremental additions
- [ ] Include `new_tasks` in the `report_completion_with_rebase()` request body
- [ ] API-side: create task rows as `pending`, resolve dependencies, increment `total_tasks`
- [ ] OR: keep the current approach where bootstrap creates the full DAG upfront and post-completion hooks are unnecessary (simpler but doesn't handle dynamic child creation)

**Design question**: If bootstrap already creates all tasks from the ontology DAG, do we actually need dynamic post-completion task creation? The only case would be when the **agent creates a new child spec that wasn't in the bootstrap DAG**. Decide whether this is a real scenario or whether bootstrap should always be the single source of truth.

---

## 2. Dependency Spec Prepopulation: Never Populated

**Status**: Code exists, data never flows  
**Files**:
- `g5d/src/agent/mod.rs` L290 — `dependency_spec_ids: Vec::new()` (hardcoded empty)
- `g5d/src/agent/prepopulation.rs` L217–225 — Phase 3 loop (never executes)
- `g5-api/src/services/task_service.rs` L551 — `dependency_specs: Vec::new()` (hardcoded empty)
- `g5-common/src/types/task_context.rs` L478 — `TaskAssignment.dependency_specs` field exists but is never populated

**The data exists but doesn't flow through**:

The uplift waterfall analyzer discovers cross-file dependencies (`AnalysisNode.dependencies: Vec<String>`) via import simulation. These dependencies flow into `TaskNodePayload.depends_on` for **task ordering** (DAG edges). They are then resolved from component_id → task UUID and stored as `tasks.depends_on UUID[]` in the DB.

However, when building the `TaskAssignment` for a worker, the API:
1. Reads `tasks.depends_on` (task UUIDs for ordering) ✅
2. Reads `tasks.component_id`, `src_files`, `parent_component_id`, `child_component_ids` into `SpecContext` ✅
3. **Does NOT** resolve `depends_on` task UUIDs back to their `component_id` values to populate `dependency_specs` ❌
4. **Does NOT** populate `dependency_specs` at all — hardcoded `Vec::new()` ❌

On the worker side, `dependency_spec_ids` in `PrepopSpecContext` is hardcoded to `Vec::new()` instead of being derived from the `TaskAssignment.dependency_specs`.

**Expected behavior**:  
The uplift analyzer identifies that file A imports from file B. This means:
1. Task A `depends_on` task B (ordering — already works)
2. When task A runs, the agent **should see B's spec in prepopulation** (Phase 3) to understand the interface it depends on
3. The API should populate `TaskAssignment.dependency_specs` by reverse-resolving `tasks.depends_on` UUIDs to their `component_id` + spec path
4. The worker should map `TaskAssignment.dependency_specs` → `PrepopSpecContext.dependency_spec_ids`

**Required changes**:
- [ ] `task_service.rs` `assign_task_to_worker()`: resolve `depends_on` UUIDs → `component_id` values, populate `dependency_specs: Vec<DependencySpec>`
- [ ] `g5d/src/agent/mod.rs` L290: map `task_context.dependency_specs` → `PrepopSpecContext.dependency_spec_ids` instead of `Vec::new()`
- [ ] Verify Phase 3 prepopulation then correctly reads dependency specs

---

## 3. Incremental Downlift: Activation Missing

**Status**: Prompts exist, activation hardcoded off  
**Files**:
- `g5-api/src/services/task_service.rs` L546 — `incremental_update: false` (hardcoded)
- `g5d/src/prompts/templates.rs` L38 — `INCREMENTAL_DOWNLIFT_GUIDANCE` template
- `g5d/src/prompts/mod.rs` L723 — incremental downlift path in prompt builder

**Problem**:  
`incremental_update` is always `false` in the `TaskAssignment`. The incremental downlift prompt templates exist and are fully wired in the prompt builder, but can never be activated.

**What "incremental" means**: The specification already exists and needs updates (not creation from scratch). This is the common case for re-running downlift after spec edits — the source files already exist and the spec's diff should guide the code changes.

**Expected behavior**:  
When a downlift job targets specs that already have corresponding source files on disk, `incremental_update` should be `true`. The prompt builder then uses the incremental downlift guidance which focuses on propagating spec diffs rather than writing from scratch.

**Required changes**:
- [ ] Define the heuristic: `incremental_update = true` when the spec's `src_files` already exist on the workspace branch
- [ ] Option A: Determine this in the API when building `TaskAssignment` (query git/branch for file existence)
- [ ] Option B: Determine this in the bootstrap analysis (waterfall already knows which files exist) and pass it through `TaskNodePayload` → `tasks` table → `TaskAssignment`
- [ ] Add `incremental_update` column to `tasks` table or infer from task metadata

---

## 4. Spec Context Merge in create_spec / edit_spec

**Status**: Missing  
**Files**:
- `g5d/src/tools/spec_ops.rs` — `CreateSpecTool::execute()` (L320–395)
- `g5d/src/tools/spec_ops.rs` — `EditSpecReplaceTool::execute()` (L450–540)

**Problem**:  
When the LLM calls `create_spec` or `edit_spec_replace`, the tools write exactly what the LLM provides. They do **not** merge in any context from the `TaskContext` (parent_spec, child_specs, dependencies from the bootstrap analysis).

The uplift analyzer discovers the ontology hierarchy (parent → children) and cross-file dependencies during bootstrap. This structural information is available in `TaskContext.spec_context` (parent_spec, child_specs) and should be in the task's dependency data. But when the LLM creates a spec, it may not include all of these fields correctly — the agent may omit or misspell dependency IDs.

**Expected behavior**:  
`create_spec` and `edit_spec_replace` should merge task-context-derived frontmatter fields with whatever the LLM provides, giving the task context left-precedence for structural fields:
- `parent_spec`: merge from `TaskContext.spec_context.parent_spec` if the LLM omits it
- `child_spec_ids`: union of LLM-provided + `TaskContext.spec_context.child_specs`
- `dependencies`: union of LLM-provided + analyzer-discovered dependencies (from `TaskAssignment.dependency_specs` once populated)
- `src_files`: merge from `TaskContext.spec_context.src_files`

**Required changes**:
- [ ] After validating frontmatter in `create_spec`, merge structural fields from `task_context.spec_context`
- [ ] After edit in `edit_spec_replace`, ensure structural fields aren't accidentally removed
- [ ] Use `SpecContext::merge()` semantics (already exists in `spec_context.rs`) or similar
- [ ] Add tests for merge behavior

---

## 5. Downlift Bootstrap: No Dependency Edges

**Status**: By design, but limits correctness  
**Files**:
- `g5d/src/worker/bootstrap.rs` — `execute_downlift_bootstrap()` (L300–370)

**Problem**:  
Downlift bootstrap creates one task per scoped file path with **no inter-task dependencies** (`depends_on: vec![]`). All tasks run independently. This means:
- If file A depends on file B's spec, A's downlift may run before B's downlift completes
- The agent may write code that references interfaces not yet generated
- The `check_dependencies()` fail-and-retry path handles this at the cost of wasted compute

**Expected behavior**:  
Downlift bootstrap should carry forward the dependency edges from the ontology. The ontology nodes (created during uplift) already have `dependencies` — the downlift bootstrap should query these and populate `depends_on` on the task graph.

**Required changes**:
- [ ] In `execute_downlift_bootstrap()`, query the ontology for each scoped node's dependencies
- [ ] Or: read the existing specs from disk and extract `dependencies` from frontmatter
- [ ] Populate `TaskNodePayload.depends_on` with the discovered edges
- [ ] This ensures spec B is downlifted before spec A if A depends on B

---

## Priority Order

1. **§2 Dependency spec prepopulation** — High impact, straightforward wiring. Agents currently can't see their dependency specs during prepopulation, limiting their ability to write correct code/specs.
2. **§4 Spec context merge** — Medium impact. Prevents spec structural corruption when agents forget to include parent/child/dependency fields.
3. **§3 Incremental downlift activation** — Medium impact. Wrong prompt guidance wastes tokens and produces worse results for re-runs.
4. **§5 Downlift dependency edges** — Medium impact. Reduces wasted compute from fail-and-retry cycles.
5. **§1 Post-completion child creation** — Low priority if bootstrap creates the full DAG. Only matters for dynamic child spec creation during agent execution.

---

## Architecture Notes

### Data flow (current)

```
Waterfall Analyzer
  → AnalysisNode { component_id, dependencies, parent_spec, child_specs, src_files }
  → TaskNodePayload { depends_on, parent_component_id, child_component_ids, src_files }
  → DB: tasks { depends_on: UUID[], component_id, src_files, parent_component_id, child_component_ids }
  → TaskAssignment { spec_context: { component_id, src_files, parent_spec, child_specs }, dependency_specs: [] ← EMPTY }
  → PrepopSpecContext { component_id, src_files, child_specs, dependency_spec_ids: [] ← EMPTY }
```

### Data flow (expected)

```
Waterfall Analyzer
  → AnalysisNode { component_id, dependencies, parent_spec, child_specs, src_files }
  → TaskNodePayload { depends_on, parent_component_id, child_component_ids, src_files }
  → DB: tasks { depends_on: UUID[], component_id, src_files, parent_component_id, child_component_ids }
  → TaskAssignment { 
      spec_context: { component_id, src_files, parent_spec, child_specs },
      dependency_specs: [{ component_id: "src/auth", spec_path: "..." }, ...] ← POPULATED from depends_on reverse-resolve
    }
  → PrepopSpecContext { 
      component_id, src_files, child_specs,
      dependency_spec_ids: ["src/auth", ...] ← MAPPED from dependency_specs
    }
  → Prepopulation Phase 3: read_spec("src/auth") ← NOW FIRES
```
