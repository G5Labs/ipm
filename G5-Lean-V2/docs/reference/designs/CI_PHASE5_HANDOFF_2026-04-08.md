# Phase 5 Handoff - 2026-04-08

> **Status note (2026-04-19)**: This is a historical handoff snapshot from the
> 2026-04-08 Phase 5 CI pass. For current state, use `TECH_DEBT.md`,
> `BATCH_PLAN.md`, and the latest CI results.

Context:
- Command: `just ci-docker 5`
- Workspace: `/Users/lumost/workspace/g5/generation`
- Goal: reduce Phase 5 integration failures and leave a design-aware handoff for the remaining hard defects.

## Current Status

- Latest full container rerun: `just ci-docker 5`
- `g5-api`: `464/464` passed
- `g5d`: `547/566` passed, `19` failed, `22` skipped
- Net improvement during this pass:
  - `g5-api` went from failing to fully green
  - `g5d` went from `27` failures to `19`

## What Was Fixed

### `g5-api`

The following regressions were patched and verified green in the latest full rerun:

- Chat task `workspace_ref` storage format was restored to the legacy snake_case shape expected by the task/chat pipeline.
  - [chat_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/chat_service.rs)
  - [task_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/task_service.rs)
- Branch draft `GET /api/v1/branches/:branchId/drafts` now behaves as ensure-or-create.
  - [drafts.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/drafts.rs)
- Over-eager internal worker lease gating was removed from task transition/status and escalation creation paths.
  - [internal.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/internal.rs)
- Name-only org updates no longer reject legacy invalid slugs unless the slug is actually being changed.
  - [org_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/org_service.rs)
- Task diff now returns an empty diff array instead of `501`.
  - [task_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/task_service.rs)
- Transition explanation timeline inserts now use valid `timeline_entries.entry_type` values (`error` / `output`) instead of invalid `info`.
  - [task_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/task_service.rs)
- File-tree storage fallbacks now map missing paths to `NotFound` consistently so the route can return `[]` instead of `500`.
  - [workspace_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/workspace_service.rs)
- The stale raw SQL test insert for `jobs` now supplies `org_id`.
  - [api_drafts.rs](/Users/lumost/workspace/g5/generation/g5-api/tests/api_drafts.rs)

### `g5d`

Two coherent `g5d` clusters were fixed during this pass and verified both with targeted host tests and the full container rerun:

- Downlift file mutations are no longer blocked by exact `src_files` matching.
  - This cleared:
    - `agent_pipeline::test_scope_enforcement_blocks_out_of_scope_write`
    - `pipeline_tools::test_scope_enforcement_blocks_all_mutations`
    - `synth_lifecycle::test_scope_enforcement_synth_calculator`
    - `tools_dispatch::test_write_file_scope_enforcement`
  - File:
    - [file_ops.rs](/Users/lumost/workspace/g5/generation/g5d/src/tools/file_ops.rs)

- Task graph language inference now lets project marker files like `package.json`, `pyproject.toml`, and `Cargo.toml` inherit the dominant project code language when the aggregate is single-language.
  - This cleared:
    - `bootstrap_analysis::test_bootstrap_javascript_project`
    - `bootstrap_analysis::test_bootstrap_python_project`
    - `bootstrap_analysis::test_bootstrap_rust_project`
    - `bootstrap_analysis::test_bootstrap_typescript_project`
  - File:
    - [bootstrap.rs](/Users/lumost/workspace/g5/generation/g5d/src/worker/bootstrap.rs)

Targeted host validation that passed after those changes:

- `cargo test -p g5d scope_enforcement -- --nocapture`
- `cargo test -p g5d nodes_to_task_graph -- --nocapture`
- `cargo test -p g5d --test bootstrap_analysis -- --nocapture`

## Remaining Failures

Latest failing `g5d` tests from the full rerun:

- `g5d::agent_pipeline::test_downlift_cascade_spawns_child_tasks_for_missing_specs`
- `g5d::agent_pipeline::test_spec_create_read_validate_roundtrip`
- `g5d::bootstrap_analysis::test_incremental_removed_file_detected`
- `g5d::parity_e2e::test_exact_prompt_parity_all_variants`
- `g5d::parity_e2e::test_prompt_parity_d4_uplift_default_config`
- `g5d::parity_e2e::test_prompt_parity_d4_uplift_suffix_component_id`
- `g5d::parity_e2e::test_prompt_parity_react_downlift_suffix_component_id`
- `g5d::parity_e2e::test_prompt_parity_react_uplift_default_config`
- `g5d::parity_e2e::test_prompt_parity_react_uplift_suffix_component_id`
- `g5d::synth_cycle::test_downlift_synth_calculator_from_specs`
- `g5d::tools_dispatch::test_create_spec_dependency_cycle_rejected`
- `g5d::tools_dispatch::test_edit_spec_introduces_cycle_rejected`
- `g5d::tools_new_features::test_global_escalation_auto_resolve_through_dispatch`
- `g5d::tools_new_features::test_global_escalation_empty_impact_rationale`
- `g5d::tools_new_features::test_hil_task_context_stripped`
- `g5d::tools_new_features::test_hil_validation_errors_through_dispatch`
- `g5d::tools_new_features::test_human_escalation_auto_resolve_first_option_fallback`
- `g5d::tools_new_features::test_human_escalation_auto_resolve_through_dispatch`
- `g5d::tools_new_features::test_view_create_task_filtered_tool_defs_omit_escalation_tools`

## Remaining Hard Failure Clusters

### 1. Prompt parity drift

Tests:

- `g5d::parity_e2e::test_exact_prompt_parity_all_variants`
- `g5d::parity_e2e::test_prompt_parity_d4_uplift_default_config`
- `g5d::parity_e2e::test_prompt_parity_d4_uplift_suffix_component_id`
- `g5d::parity_e2e::test_prompt_parity_react_downlift_suffix_component_id`
- `g5d::parity_e2e::test_prompt_parity_react_uplift_default_config`
- `g5d::parity_e2e::test_prompt_parity_react_uplift_suffix_component_id`

Signals:

- Rust uplift system prompts are materially shorter than the golden fixtures.
- Downlift parity still misses `Common Spec Type Examples`.
- The uplift/downlift workspace-context wording in Rust no longer matches the golden contract.

Likely root:

- Prompt composition in [prompts/mod.rs](/Users/lumost/workspace/g5/generation/g5d/src/prompts/mod.rs) has drifted from the parity fixtures.

Design-aligned fix direction:

- Decide whether Rust prompts or the golden fixtures are the source of truth.
- If Rust is right, update fixtures intentionally and document the contract change.
- If fixtures are right, move the uplift/downlift boilerplate back toward template-driven composition.

### 2. Spec targeting / spec creation contract drift

Tests:

- `g5d::agent_pipeline::test_downlift_cascade_spawns_child_tasks_for_missing_specs`
- `g5d::agent_pipeline::test_spec_create_read_validate_roundtrip`

Signals:

- Specs expected to be created on disk never appear.
- The failing scenarios require creating or reading specs other than the task-context `component_id`.

Likely root:

- [spec_ops.rs](/Users/lumost/workspace/g5/generation/g5d/src/tools/spec_ops.rs) currently hard-locks `create_spec` and `edit_spec_replace` to the task-context `component_id` via `enforce_task_component_id`.
- That matches the local unit test `test_create_spec_rejects_component_outside_task_context`, but it conflicts with the higher-level agent-pipeline tests.

Design-aligned fix direction:

- Choose the contract explicitly:
  - strict single-target spec editing only,
  - or allow bootstrap/downlift fan-out to create sibling and child specs.
- Then align [spec_ops.rs](/Users/lumost/workspace/g5/generation/g5d/src/tools/spec_ops.rs) and the conflicting tests in one pass.

### 3. HIL / tool-availability policy drift

Tests:

- `g5d::tools_new_features::test_global_escalation_auto_resolve_through_dispatch`
- `g5d::tools_new_features::test_global_escalation_empty_impact_rationale`
- `g5d::tools_new_features::test_hil_task_context_stripped`
- `g5d::tools_new_features::test_hil_validation_errors_through_dispatch`
- `g5d::tools_new_features::test_human_escalation_auto_resolve_first_option_fallback`
- `g5d::tools_new_features::test_human_escalation_auto_resolve_through_dispatch`
- `g5d::tools_new_features::test_view_create_task_filtered_tool_defs_omit_escalation_tools`

Signals:

- Downlift/view-create task filtering hides `human_escalation` and `global_escalation`.
- Some tests still expect those tools to execute for auto-resolve or validation-through-dispatch.
- View-create filtering still exposes `extract_for_view` when tests expect it omitted.

Likely root:

- Tool filtering in [tools/mod.rs](/Users/lumost/workspace/g5/generation/g5d/src/tools/mod.rs) and auto-resolve behavior in [hil_escalation.rs](/Users/lumost/workspace/g5/generation/g5d/src/tools/hil_escalation.rs) are out of sync with the test contract.

Design-aligned fix direction:

- Pick one consistent rule:
  - HIL tools are truly unavailable for downlift/view-create tasks,
  - or auto-resolve is treated as a safe local path even when real escalation is disallowed.
- Align filtered tool definitions, runtime dispatch behavior, and validation/error messages together.

### 4. Spec dependency cycle enforcement drift

Tests:

- `g5d::tools_dispatch::test_create_spec_dependency_cycle_rejected`
- `g5d::tools_dispatch::test_edit_spec_introduces_cycle_rejected`

Signals:

- Creating a spec with a transitive dependency cycle is no longer rejected.
- Editing a spec to create a cycle fails earlier with `String not found` instead of surfacing the cycle error.

Likely root:

- Cycle detection exists in [spec_ops.rs](/Users/lumost/workspace/g5/generation/g5d/src/tools/spec_ops.rs), but the create/edit flows are not surfacing it reliably in these scenarios.
- The edit fixture appears to be relying on exact text replacement against spec content that is now normalized differently after creation.

Design-aligned fix direction:

- Ensure create/edit/validate all share the same dependency-cycle validation path.
- Rework the edit fixture or the edit semantics so the test actually reaches cycle validation instead of failing on a formatting mismatch.

### 5. Incremental diff / synth graph regressions

Tests:

- `g5d::bootstrap_analysis::test_incremental_removed_file_detected`
- `g5d::synth_cycle::test_downlift_synth_calculator_from_specs`

Signals:

- Incremental removal detection regressed.
- The synth calculator downlift graph is under-emitting tasks.
- In the failing synth run, the emitted graph only contains:
  - `synth_calculator`
  - `synth_calculator/__init__`
  - `test-project`
  - `test-project/pyproject`

Likely root:

- Shared drift in analyzer/diff/task-graph generation, likely around [ontology_diff.rs](/Users/lumost/workspace/g5/generation/g5d/src/analysis/ontology_diff.rs) and the rules that promote changed children into emitted downlift tasks.

Design-aligned fix direction:

- Treat this as one design pass over the incremental/bootstrap/synth pipeline.
- Start by comparing expected vs actual task graph generation contracts in:
  - [bootstrap_analysis.rs](/Users/lumost/workspace/g5/generation/g5d/tests/bootstrap_analysis.rs)
  - [synth_cycle.rs](/Users/lumost/workspace/g5/generation/g5d/tests/synth_cycle.rs)
  - [agent_pipeline.rs](/Users/lumost/workspace/g5/generation/g5d/tests/agent_pipeline.rs)

## Suggested Next Pass

1. Keep `g5-api` untouched for now; it is green in the latest full rerun.
2. Treat the remaining `g5d` failures as 5 coordinated workstreams, not 19 unrelated bugs.
3. Resolve the contract conflicts first:
   - spec fan-out vs strict task-context spec targeting
   - downlift/view-create escalation availability vs auto-resolve/validation-through-dispatch
4. After each `g5d` cluster, re-run `just ci-docker 5`, not just host `cargo test`, because the container path remains the source of truth for this task.
