# View Sync Unification With Standard Uplift/Downlift

Status: Proposed  
Date: 2026-03-16  
Scope: `g5-api`, `g5d`, `g5-common`, E2E and prompt contracts

## Executive Summary

This document proposes collapsing `view_sync_downlift` and `view_sync_uplift` onto the same execution substrate as standard `downlift` and `uplift`. The target model is:

- one job/task engine
- one bootstrap/fan-out mechanism
- one task assignment model
- one tool registry shape
- one completion/cascade model

The only intentional differences should be:

- bootstrap source selection
- prompt profile
- output target description

That separation matters because the current code mixes two very different kinds of divergence:

1. **Implementation divergence**
These are separate code paths, separate task builders, extra job metadata, extra task context, and separate worker behavior. This divergence is accidental technical complexity and should be removed.

2. **Functional divergence**
These are intentional differences in what the agent is being asked to do:
- whether the source of truth is a view or ontology/specs
- whether the target is a spec/code artifact or a view document
- which persona lens is applied
- whether linked documents participate in relevance/bootstrap

This divergence belongs at the prompting and task-contract layer, not in the orchestration substrate.

### Implementation Divergence Today

| Area | Current behavior | Why it is substrate divergence |
|---|---|---|
| Job metadata | view sync jobs carry `jobs.mode` and `jobs.target_doc_id` | Standard jobs do not need a separate job family to execute |
| Task assignment | `ViewTaskContext` is rebuilt from `docs` rows in `task_service.rs` | View sync uses a parallel task-context model rather than shared task descriptors |
| Bootstrap | Standard downlift uses `BootstrapScope`; view sync uses prep-task plus `view_task_builder.rs` / `sync_plan.rs` | Two task graph builders exist for conceptually similar fan-out |
| Worker execution | Tool registration branches on `view_mode` strings | Tool availability is coupled to job-mode names instead of capabilities or target type |
| Prompt dispatch | Separate template dispatch keyed off `view_sync_*` modes | Prompt selection should be driven by a prompt profile, not a separate job implementation |
| Data model | Views are mostly `docs` rows bridged into ontology, not first-class ontology nodes | The runtime has to reconstruct ontology semantics from document metadata |

### Functional Divergence Today

| Area | Downlift sync | Uplift sync | Why this should remain functional |
|---|---|---|---|
| Source of truth | View document | Specs / ontology | This is a prompt-level framing difference |
| Target artifact | Specs and optionally code | View document | This is a target-descriptor difference |
| Persona lens | PM / Arch / Security / Test / Custom | PM / Arch / Security / Test / Custom | Persona is prompt policy, not orchestration policy |
| Linked docs | Used as view bootstrap context and, for test views, as direct source material | Used as direct source material for test views and supporting context elsewhere | Linked docs are bootstrap/input context, not a separate workflow family |
| Edit permissions | View often read-only after prep; specs writable | Specs read-only; view writable | This is a target permission contract, not a separate job engine |

### Key Conclusion

The current implementation makes view sync look like a distinct workflow family. It should instead be modeled as:

- **standard uplift/downlift**
- **plus a projection/bootstrap adapter**
- **plus a prompt profile**

For both `downlift` and `uplift`, this collapse is straightforward if the view is promoted to a first-class ontology node. In that model:

- the view can have `parent_id`
- the view can have children
- the view can participate in dependency edges
- the view can be linked and referenced like any other ontology node

Once that is true, uplift convergence is no longer special. It is the ordinary bottom-up DAG behavior the system already uses everywhere else.

## Current State

### 1. Job Creation

View sync is created in `g5-api/src/services/doc_service.rs` through `sync_view()`, which creates ordinary `jobs` rows with:

- `job_type = downlift` and `mode = view_sync_downlift`, or
- `job_type = uplift` and `mode = view_sync_uplift`

The job also carries `target_doc_id`.

This means the system already admits that view sync is conceptually an uplift/downlift variant. The problem is that the variant is threaded through the whole system as a bespoke execution contract rather than as metadata.

### 2. Task Assignment

`g5-api/src/services/task_service.rs` rehydrates a `ViewTaskContext` by loading:

- `docs.file_path`
- `docs.content`
- `docs.persona`
- `docs.instruction`
- `docs.is_future`
- `docs.review_iterations`
- `docs.scope_node_id`
- `docs.linked_doc_paths`
- sync SHAs

That context is then added to `TaskAssignment`.

This is a parallel context model. Standard tasks use `SpecContext`, `BootstrapScope`, dependency metadata, and repo information. View sync adds an entire second track of context rather than expressing the same needs in shared task descriptors.

### 3. Bootstrap and Fan-Out

Standard downlift bootstrap in `g5d/src/worker/bootstrap.rs` uses `BootstrapScope` and ontology/spec state to derive first-level task targets and subsequent cascades.

View sync uses separate workerside logic:

- prep-task semantics (`view_sync_prep/...`)
- `g5d/src/worker/view_task_builder.rs`
- `g5d/src/worker/sync_plan.rs`
- view frontmatter parsing

The comments in `g5d/src/config.rs` already show the intended future: `view_sync_downlift` and `view_sync_uplift` should use the standard fan-out path. The code has not fully converged to that target.

### 4. Prompting and Tooling

Prompt selection in `g5d/src/prompts/mod.rs` explicitly switches on:

- `view_create`
- `view_sync_downlift`
- `view_sync_uplift`
- `view_future_create`

The templates are also separate:

- `view_sync_downlift_guidance.tera`
- `view_sync_uplift_guidance.tera`
- `view_creation_guidance.tera`

Tool registration in `g5d/src/tools/mod.rs` also branches on `view_mode`, enabling or disabling:

- `emit_view_artifact`
- `apply_sync_to_spec`
- `emit_sync_plan`
- `extract_for_view`
- other view-only helpers

This is where the functional divergence is currently encoded, but it is mixed together with implementation divergence. The prompts are correctly expressing different agent tasks; the tooling and orchestration are doing too much mode-specific work around them.

## Characterizing The Prompting Divergence

The prompting divergence is real and should remain. It is not merely wording.

### 1. Direction of Truth

For `view_sync_downlift`, the prompt says:

- the view is auxiliary change evidence/provenance
- specs are the implementation source of truth being updated
- in per-spec mode, the task should adapt accepted view content into the spec before downstream code/test propagation

For `view_sync_uplift`, the prompt says:

- specs are the source of truth
- the view is the target
- per-spec tasks should synthesize and update the mapped view sections

That is a substantive functional distinction. It changes:

- what gets read first
- what counts as drift
- what is writable
- what constitutes successful completion

### 2. Persona-Lens Synthesis

The view prompts are not trying to mirror ontology/spec content mechanically. They perform synthesis through a persona lens:

- PM: business and planning framing
- Architect: cross-component and interface framing
- Security: threat/constraint framing
- Test: scenario and verification framing
- Custom: user-defined label

This is a valid functional layer. Standard uplift/downlift can still support this by treating persona as part of a prompt profile, not as a reason for a separate execution engine.

### 3. Linked-Document Semantics

Linked documents are currently used as bootstrap context and, for test views, sometimes as primary source material. This is also a legitimate functional divergence.

The important design point is that linked docs should affect:

- initial scope discovery
- context prepopulation
- relevance hints
- prompt instructions

They should not require:

- a separate job family
- a separate task graph builder
- a separate runtime context object

### 4. Edit-Right Asymmetry

The prompts currently enforce different writable surfaces:

- downlift sync generally writes specs, not the view
- uplift sync writes the view, not specs

This asymmetry is correct. It should be expressed as a target permission policy attached to the task, not a separate workflow mode that cascades through the whole worker runtime.

## Proposed Target Architecture

### 1. Make Views First-Class Ontology Targets

The service layer should stop treating view sync as “doc workflow with ontology bridges”. Instead:

- a view should be represented as an ontology-backed markdown artifact
- the document path/content remains real, but the identity and scope semantics become ontology-native
- the view should support the same graph structure as any other ontology node:
  - `parent_id`
  - child nodes
  - dependency edges
  - ordinary link/reference relationships

That collapses the current `docs` vs `ontology_nodes` split and makes view sync scope derivation compatible with standard job/task creation.

### 2. Introduce A Shared Projection Descriptor

Replace mode-driven execution branching with a shared descriptor carried on the job/task:

- `source_kind`: `ontology`, `view`, `linked_docs`
- `target_kind`: `spec`, `code`, `view`
- `persona`: `pm | arch | security | test | custom`
- `linked_doc_paths`
- `sync_direction`: `uplift | downlift`

This is not a new workflow family. It is execution metadata that tells the prompt layer and the bootstrap layer how to frame the work.

`ViewTaskContext` should largely disappear into this shared descriptor plus ordinary `SpecContext` / `BootstrapScope`.

### 3. Unify Bootstrap

All sync jobs should go through one bootstrap interface that returns:

- a standard `BootstrapScope`
- a target descriptor
- prompt profile metadata

For standard uplift/downlift, bootstrap is ontology-root or subtree based.

For view sync:

- the bootstrap adapter reads linked docs, frontmatter, implementation mapping, and linked ontology references
- it resolves the same kind of task targets the standard path uses
- it emits the same task payload shape the standard path uses

The important difference is where the initial scope comes from, not how the rest of the worker graph behaves.

### 4. Unify Prompt Dispatch

Prompt dispatch should stop keying on `view_sync_downlift` and `view_sync_uplift` as workflow names. Instead, prompt assembly should use:

- `task_type`
- `sync_direction`
- `source_kind`
- `target_kind`
- `persona`

This preserves the real functional divergence while removing workflow-mode branching.

In other words:

- prompt divergence remains
- implementation divergence disappears

### 5. Unify Tool Registration

Tool registration should be based on target permissions and task capabilities, not on `view_mode` string matching.

Examples:

- tasks with writable spec targets get spec-edit tools
- tasks with writable view targets get view-edit tools
- prep/bootstrap tasks may get scope-fixup tools
- read-only tasks get extraction/analysis tools only

This is a cleaner capability model than today’s mode-based gates.

## Convergence Via The Ordinary Ontology DAG

The earlier implementation concern around “shared target convergence” only exists because the current system treats the view as a side-channel document target instead of a normal ontology node.

For downlift, standardization is direct:

- bootstrap selects scope from the view
- spawned tasks update specs/code
- cascade behavior matches ordinary downlift

For uplift, the correct target model is:

- relevant child/spec nodes sit below the view in the ontology DAG
- bottom-up execution runs those children first
- the view node runs after dependencies are satisfied
- the view node synthesizes the final markdown artifact

That is already how ordinary aggregate convergence works in standard uplift/downlift. The missing piece is not a new reducer primitive. The missing piece is that the view is not yet represented as a normal node in the execution graph.

So the design rule is:

> A view target should converge exactly like any other aggregate ontology node.

If the current docs-centric model must be supported during migration, a reducer can exist temporarily as a compatibility bridge. But that should be treated as transitional tech debt, not as the target architecture.

## Migration Plan

### Phase 1: Data Model Cleanup

- make views first-class ontology-backed artifacts in service behavior
- give views the same parent/child/dependency/link semantics as other ontology nodes
- reduce reliance on `jobs.mode` and `jobs.target_doc_id`
- introduce a shared projection descriptor

### Phase 2: Bootstrap Unification

- move view scope derivation into the standard bootstrap interface
- remove separate prep/sync-plan/view-task-builder orchestration from the steady-state path
- keep linked-doc/bootstrap parsing as an adapter only

### Phase 3: Prompt/Tool Refactor

- replace `view_mode`-driven dispatch with prompt profile fields
- replace mode-based tool registration with capability/target-based registration

### Phase 4: Uplift Convergence

- promote view nodes into the ordinary uplift dependency DAG
- make the final view synthesis step run as the aggregate view-node task
- remove residual assumptions that a view target lives outside the normal execution graph

### Phase 5: Cleanup

- delete `view_sync_*` execution-specific code paths
- retain only prompt profile templates or merge them into shared prompt composition
- collapse E2E coverage onto standard uplift/downlift scenarios with projection variants

## Testing Requirements

The unified design needs explicit tests at three levels.

### API / Job Creation

- view sync creates standard uplift/downlift jobs with projection descriptors
- no bespoke execution-mode branching required for assignment

### Worker / Bootstrap

- linked-doc bootstrap resolves the same task payload shape as ordinary flows
- downlift fan-out from a view uses the standard cascade path
- uplift fan-out converges onto the view node through the ordinary dependency DAG

### Prompt / Functional Contract

- downlift prompts frame the view as auxiliary change evidence/provenance to reflect into specs
- uplift prompts frame specs as the implementation source of truth and the view as an auxiliary target
- persona lenses remain intact
- linked-doc/test-view semantics remain intact

## Recommendation

Proceed with full substrate unification.

The architectural rule should be:

> View sync is not a separate workflow family. It is standard uplift/downlift with a different bootstrap source and a different prompt profile.

Corollary:

> Views are ordinary ontology nodes with markdown artifacts and persona metadata, not side-channel documents.

Once that is true, both uplift and downlift can use the same graph semantics, including parent/child traversal, dependency edges, link relationships, and bottom-up convergence.
