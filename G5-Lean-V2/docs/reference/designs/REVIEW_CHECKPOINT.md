# Review Checkpoint — Stop-the-World Quality Gates

**Status:** Design  
**Author:** G5 Engineering  
**Date:** 2026-03-05  

---

## 1. Problem Statement

During large downlift (or uplift) jobs with 50–200+ tasks, individual agents
operate in isolation — each processes a single spec/component without
awareness of the global project state. This leads to systemic failures that
no single task can detect:

1. **Design drift** — Early tasks establish patterns that later tasks
   contradict. Without periodic alignment checks, the accumulated code
   diverges from cohesive design intent.

2. **Build rot** — Agents may produce code that compiles in isolation but
   breaks when merged with sibling branches. Build failures compound
   silently until the final merge.

3. **Spec ambiguity accumulation** — Specs with ambiguous or inconsistent
   contracts produce implementations that disagree at module boundaries.
   Interface mismatches are only visible when reviewing multiple modules
   together.

4. **Wasted compute** — Without early detection, dozens of tasks may
   complete before a fundamental misalignment is discovered, requiring
   expensive re-runs.

**Goal:** Inject periodic "stop-the-world" review checkpoints that pause
task scheduling, run a comprehensive quality gate (build + artifact audit +
cross-cutting review), correct issues found, and resume execution.

---

## 2. Design Overview

### 2.1 Concept

A **review checkpoint** is a synthetic review task injected into a running
job's task graph at strategic intervals.  When triggered, the system:

1. **Creates** a review checkpoint task with no dependencies (immediately
   eligible for scheduling).
2. **Injects** the review task as a dependency on every pending/blocked
   task in the job — they cannot run until the review completes.
3. **Executes** the review task — it audits artifacts, runs builds, and
   corrects any issues found.  In-flight tasks continue and finish
   naturally; any children they spawn also gain the review dependency.
4. **Releases** gated tasks automatically via `unblock_dependents()`
   when the review completes.

The job stays in `running` status throughout — no new statuses, no drain
logic, no scheduler changes.

### 2.2 Trigger Conditions

Review checkpoints are triggered by **either** of two conditions, evaluated
after every task terminal transition (completed, failed):

| Trigger | Parameter | Default | Description |
|---------|-----------|---------|-------------|
| **Completion interval** | `review_every_k` | 10 | After every K task completions since the last checkpoint |
| **Failure threshold** | `review_after_n_failures` | 3 | After N cumulative task failures since the last checkpoint |

These are configured per-job via `Guardrails` (existing budget/limit struct):

```rust
pub struct Guardrails {
    // ... existing fields ...
    pub review_every_k: Option<u32>,           // None = disabled
    pub review_after_n_failures: Option<u32>,  // None = disabled
}
```

### 2.3 Architecture — Dependency Injection

Instead of introducing new job statuses and drain logic, checkpoints
use the **existing task dependency mechanism**.  When a checkpoint
triggers, all schedulable (pending/blocked) tasks gain the review task
as a dependency.  Running tasks finish naturally.  The review task has
no dependencies itself, so it is immediately eligible.  When it
completes, `unblock_dependents()` releases the gated tasks.

```
                     ┌──────────────────────────────────────────────┐
                     │            task_service.rs                   │
                     │         transition_task()                    │
                     │                                              │
                     │  On completed/failed:                        │
                     │    completions_since_checkpoint += 1         │
                     │    failures_since_checkpoint += 1            │
                     │                                              │
                     │    IF checkpoint_triggered(job):             │
                     │      1. Create review checkpoint task (R)    │
                     │      2. For every pending/blocked task T     │
                     │         in this job: T.depends_on += R       │
                     │      3. Store R.id as active_checkpoint_id   │
                     │         on the job row                       │
                     │      4. R has no deps → immediately eligible │
                     │                                              │
                     │  On review task R completed:                 │
                     │    unblock_dependents(R) fires automatically │
                     │    Reset checkpoint counters                 │
                     │    Clear active_checkpoint_id                │
                     │                                              │
                     │  While active_checkpoint_id is set:          │
                     │    spawn_child_tasks() adds R to new tasks'  │
                     │    depends_on so they also wait              │
                     └──────────────────────────────────────────────┘
```

**Why dependency injection over job-status gating:**

| Concern | Status-based approach | Dependency-injection approach |
|---------|----------------------|------------------------------|
| New job statuses | `review_pending`, `reviewing` | None |
| Scheduler changes | Modified poll query | None |
| Drain logic | Explicit running-task drain loop | Running tasks finish naturally; review runs concurrently |
| Child task gating | Requires special-case in `spawn_child_tasks` | Same mechanism — check `active_checkpoint_id` |
| Resume logic | Special completion handler flips job status | `unblock_dependents()` already does it |
| Complexity | ~4 new code paths | ~1 new code path + 1 small hook |

---

## 3. Detailed Design

### 3.1 Job Table Changes

Add columns to the `jobs` table to track checkpoint state:

```sql
ALTER TABLE jobs ADD COLUMN completions_since_checkpoint INT NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN failures_since_checkpoint INT NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN checkpoint_count INT NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN review_every_k INT;           -- NULL = disabled
ALTER TABLE jobs ADD COLUMN review_after_n_failures INT;  -- NULL = disabled
ALTER TABLE jobs ADD COLUMN active_checkpoint_id UUID;    -- FK → tasks(id), NULL when no checkpoint active
```

**No new job statuses are needed.** The job stays in `running` throughout
the checkpoint cycle.  The `active_checkpoint_id` column is the only
state signal — when non-NULL, `spawn_child_tasks()` adds this task ID
to every newly created task's `depends_on` array.

### 3.2 Task Type Extension

The review checkpoint task uses the existing `TaskType::Review`. No new
task type is needed. The task is distinguished by a metadata flag:

```sql
ALTER TABLE tasks ADD COLUMN is_checkpoint BOOLEAN NOT NULL DEFAULT FALSE;
```

This allows the completion handler to distinguish checkpoint reviews from
user-initiated reviews and reset counters accordingly.

### 3.3 Trigger Evaluation (task_service.rs)

After every terminal transition in `transition_task()`, evaluate whether
a checkpoint should fire:

```rust
// In transition_task(), after incrementing completed_tasks or failed_tasks:

fn maybe_trigger_checkpoint(
    db: &mut ScopedDb,
    job: &Job,
    task: &Task,
    transition_status: &str,
) -> Result<CheckpointAction, G5Error> {
    // Skip if this IS a checkpoint task (avoid infinite loop)
    if task.is_checkpoint {
        return Ok(CheckpointAction::None);
    }

    // Skip if a checkpoint is already active (another one running/pending)
    if job.active_checkpoint_id.is_some() {
        return Ok(CheckpointAction::None);
    }

    // Skip if checkpoints are disabled
    let review_k = job.review_every_k.unwrap_or(0);
    let review_n = job.review_after_n_failures.unwrap_or(0);
    if review_k == 0 && review_n == 0 {
        return Ok(CheckpointAction::None);
    }

    // Increment counters
    match transition_status {
        "completed" => {
            sqlx::query!(
                "UPDATE jobs SET completions_since_checkpoint = completions_since_checkpoint + 1
                 WHERE id = $1",
                job.id
            ).execute(db.conn()).await?;
        }
        "failed" => {
            sqlx::query!(
                "UPDATE jobs SET failures_since_checkpoint = failures_since_checkpoint + 1
                 WHERE id = $1",
                job.id
            ).execute(db.conn()).await?;
        }
        _ => {}
    }

    // Re-read counters
    let updated = sqlx::query!(
        "SELECT completions_since_checkpoint, failures_since_checkpoint,
                completed_tasks, total_tasks, failed_tasks
         FROM jobs WHERE id = $1",
        job.id
    ).fetch_one(db.conn()).await?;

    // Don't trigger if job is about to complete anyway
    let remaining = updated.total_tasks
        - updated.completed_tasks
        - updated.failed_tasks;
    if remaining <= 1 {
        return Ok(CheckpointAction::None);
    }

    // Check triggers
    let k_triggered = review_k > 0
        && updated.completions_since_checkpoint >= review_k as i32;
    let n_triggered = review_n > 0
        && updated.failures_since_checkpoint >= review_n as i32;

    if k_triggered || n_triggered {
        return Ok(CheckpointAction::Trigger {
            reason: if n_triggered {
                format!("{} failures since last checkpoint", updated.failures_since_checkpoint)
            } else {
                format!("{} completions since last checkpoint", updated.completions_since_checkpoint)
            },
        });
    }

    Ok(CheckpointAction::None)
}

enum CheckpointAction {
    None,
    Trigger { reason: String },
}
```

### 3.4 Checkpoint Creation & Dependency Injection

When a checkpoint triggers, three things happen atomically (within the
same `transition_task` transaction):

```rust
async fn inject_checkpoint(
    db: &mut ScopedDb,
    job_id: Uuid,
    org_id: Uuid,
    trigger_reason: &str,
) -> Result<Uuid, G5Error> {
    // ── Step 1: Create the review checkpoint task ────────────────

    let job = sqlx::query!(
        "SELECT checkpoint_count FROM jobs WHERE id = $1 FOR UPDATE",
        job_id
    ).fetch_one(db.conn()).await?;

    let checkpoint_num = job.checkpoint_count + 1;
    let review_task_id = Uuid::now_v7();

    // The review task has NO dependencies → immediately pending/eligible.
    sqlx::query!(
        r#"INSERT INTO tasks (
            id, job_id, org_id, status, task_type,
            component_id, name, description,
            is_checkpoint, depends_on, created_at
        ) VALUES (
            $1, $2, $3, 'pending', 'review',
            $4, $5, $6,
            TRUE, '{}', now()
        )"#,
        review_task_id,
        job_id,
        org_id,
        format!("__checkpoint_{}", checkpoint_num),
        format!("Review Checkpoint #{}", checkpoint_num),
        format!(
            "Automated review checkpoint triggered by: {}. \
             Checkpoint #{} for job {}.",
            trigger_reason, checkpoint_num, job_id
        ),
    ).execute(db.conn()).await?;

    // ── Step 2: Inject dependency on ALL pending/blocked tasks ───
    //
    // This is the core mechanism: every task that hasn't started yet
    // now waits for the review to complete before it can run.
    // Running tasks are unaffected — they finish naturally.

    let gated_count = sqlx::query!(
        "UPDATE tasks
         SET depends_on = array_append(depends_on, $1)
         WHERE job_id = $2
           AND status IN ('pending', 'blocked')
           AND id != $1
         RETURNING id",
        review_task_id,
        job_id,
    ).fetch_all(db.conn()).await?.len();

    // ── Step 3: Update job metadata ─────────────────────────────

    sqlx::query!(
        "UPDATE jobs SET
            total_tasks = total_tasks + 1,
            checkpoint_count = checkpoint_count + 1,
            active_checkpoint_id = $2
         WHERE id = $1",
        job_id,
        review_task_id,
    ).execute(db.conn()).await?;

    info!(
        job_id = %job_id,
        task_id = %review_task_id,
        checkpoint_num = checkpoint_num,
        gated_tasks = gated_count,
        trigger_reason = %trigger_reason,
        "Injected review checkpoint — {} tasks now depend on it",
        gated_count
    );

    Ok(review_task_id)
}
```

### 3.5 Child Task Gating

While a checkpoint is active, any newly spawned child tasks (from
running tasks that complete during the checkpoint window) must also
depend on the review task.  This is a small addition to
`spawn_child_tasks()`:

```rust
// In spawn_child_tasks(), when building the depends_on array for a new task:
let mut depends_on = resolved_deps; // inter-sibling deps from spec

// If a checkpoint is active, new tasks must also wait for it
if let Some(checkpoint_id) = job.active_checkpoint_id {
    // Only add if the checkpoint hasn't completed yet
    let checkpoint_status: Option<String> = sqlx::query_scalar!(
        "SELECT status FROM tasks WHERE id = $1",
        checkpoint_id
    ).fetch_optional(db.conn()).await?;

    if matches!(checkpoint_status.as_deref(), Some("pending" | "running")) {
        depends_on.push(checkpoint_id);
    }
}
```

### 3.6 Review Task Completion → Counter Reset

When a checkpoint review task reaches a terminal state, reset the
counters and clear the active checkpoint.  `unblock_dependents()` is
already called by the standard completion path — **no special resume
logic needed**.

```rust
// In transition_task(), after standard completion/failure processing:
if task.is_checkpoint {
    // Reset counters regardless of success or failure.
    // On failure: we don't want to block the pipeline — the next
    // checkpoint (K more completions) will catch remaining issues.
    sqlx::query!(
        "UPDATE jobs SET
            completions_since_checkpoint = 0,
            failures_since_checkpoint = 0,
            active_checkpoint_id = NULL
         WHERE id = $1",
        task.job_id
    ).execute(db.conn()).await?;

    if new_status == "completed" {
        info!(job_id = %task.job_id, "Review checkpoint completed");
    } else {
        warn!(
            job_id = %task.job_id,
            "Review checkpoint {} — dependents unblocked anyway",
            new_status
        );
    }

    // unblock_dependents() runs in the standard path and will
    // release all tasks that were gated behind this checkpoint.
}
```

### 3.7 Scheduler Integration

**No scheduler changes are needed.**  The job remains in `running`
status throughout the checkpoint cycle.  The review task is `pending`
with no dependencies, so it is immediately eligible for `poll_for_task`.
Gated tasks have the review task in their `depends_on` array, so the
existing dependency check (`all depends_on tasks completed`) blocks
them until the review finishes.

---

## 4. Review Checkpoint Prompt

The review checkpoint task uses a specialized prompt template that directs
the agent to perform three categories of quality checks. The prompt
design follows SemLift conventions — structured system prompt with
tool-use guidance, ReAct loop execution.

### 4.1 Prompt Template: `review_checkpoint_system.tera`

```
You are a senior software architect performing a comprehensive quality review
of a codebase that is being constructed from specifications (downlift).

This is **Review Checkpoint #{{ checkpoint_number }}** for this job.
{{ completed_tasks }} of {{ total_tasks }} tasks have completed so far,
with {{ failed_tasks }} failures.

**Trigger reason:** {{ trigger_reason }}

## Your Objective

You must perform three categories of verification, in order:

### Category 1: Build Verification

Run the project's build system to verify the codebase compiles.

1. Identify the project type and build system:
   - Rust: `cargo check` (fast) then `cargo build` if check passes
   - TypeScript/JavaScript: `npm run build` or `tsc --noEmit`
   - Python: `python -m py_compile` on changed files
   - Other: look for Makefile, build.gradle, etc.

2. If the build fails:
   - Categorize errors: missing imports, type mismatches, undefined symbols,
     circular dependencies, syntax errors
   - For each error, identify which spec/component likely caused it
   - Fix the errors directly — you have full write access
   - Re-run the build to confirm the fix

3. If the build passes:
   - Run the test suite if one exists (`cargo test`, `npm test`, `pytest`)
   - Note test failures but do NOT fix failing tests unless they indicate
     a clear implementation bug (tests may be stale or not yet updated)

4. Record build results in your completion summary.

### Category 2: Artifact & Specification Audit

Inspect the recently created/modified artifacts for quality and consistency.

1. **List recently changed files** using `git_diff` or `git_status`.

2. **For each changed specification** (`.semlift/specs/*.semlift.md`):
   - Validate the spec: `validate_spec(component_id, use_golden_standard=True)`
   - Check that `source_files` lists actual files that exist on disk
   - Check that `dependencies` reference specs that exist
   - Check that `child_spec_ids` are consistent with the ontology tree
   - Look for ambiguous or contradictory requirements
   - Look for missing Success Criteria items
   - Look for insufficient Domain Context on specialized components

3. **For each changed source file**:
   - Read the corresponding spec (derive component_id from file path)
   - Verify the implementation satisfies the spec's API contract:
     - All public symbols present with correct signatures
     - Required behaviors implemented (check Success Criteria)
     - Error handling matches spec contracts
   - Look for obvious code quality issues:
     - Dead code or TODO stubs left by agents
     - Inconsistent naming conventions across modules
     - Missing error handling

4. **Cross-cutting interface consistency**:
   - For modules that import from each other, verify interface compatibility
   - Check that shared types/traits are used consistently
   - Identify any circular dependency patterns that shouldn't exist

### Category 3: Corrective Actions

Based on findings from Categories 1 and 2:

1. **Fix build-breaking issues** immediately (highest priority).

2. **Fix spec deficiencies** that would cause downstream tasks to produce
   incorrect code:
   - Missing API signatures in specs → add them
   - Contradictory requirements → resolve with the most reasonable
     interpretation, add a note explaining the resolution
   - Missing dependencies in spec frontmatter → add them

3. **Fix implementation gaps** where code clearly violates its spec:
   - Missing public API methods → add stubs or implementations
   - Wrong return types or parameter signatures → correct them
   - Missing error handling → add it

4. **Do NOT**:
   - Rewrite working code for style preferences
   - Add features not in the spec
   - Modify test files (they may be intentionally incomplete)
   - Change spec frontmatter fields (component_id, parent_spec, etc.)
     unless they are demonstrably wrong

## Completion

When you have finished the review:

1. Commit your fixes with a descriptive message:
   `git_commit("Review checkpoint #N: [summary of fixes]")`

2. Call `transition_task_state("complete")` with a structured summary:
   ```
   ## Review Checkpoint #N Summary

   ### Build Status
   - Build: PASS/FAIL (N errors fixed)
   - Tests: PASS/FAIL/SKIPPED (N failures, M skipped)

   ### Artifacts Reviewed
   - Specs reviewed: N (M had issues)
   - Source files reviewed: N (M had issues)

   ### Issues Found & Fixed
   1. [issue description] → [fix description]
   2. ...

   ### Issues Found & Deferred
   1. [issue description] → [reason for deferral]
   2. ...

   ### Recommendations
   - [any systemic patterns that should be addressed]
   ```

If you encounter a blocking issue that prevents meaningful review
(e.g., workspace is corrupt, git state is broken), call
`transition_task_state("blocked", reason)` to escalate to a human.
```

### 4.2 Prompt Template: `review_checkpoint_user.tera`

```
Perform Review Checkpoint #{{ checkpoint_number }} for this job.

The workspace is at: {{ workspace_path }}

{% if job_type == "downlift" %}
This is a **downlift** job — specifications are being converted to code.
Focus your review on:
- Whether generated code matches spec contracts
- Whether the build compiles
- Whether specs were modified correctly (for aggregate specs)

{% elif job_type == "uplift" %}
This is an **uplift** job — code is being analyzed to produce specifications.
Focus your review on:
- Whether specs accurately capture the code's behavior
- Whether dependency declarations are correct
- Whether Success Criteria checklists are verifiable

{% endif %}

**Recently completed tasks** (since last checkpoint):
{% for task in recent_tasks %}
- {{ task.component_id }} ({{ task.status }}{% if task.error_message %}: {{ task.error_message }}{% endif %})
{% endfor %}

{% if failed_tasks_detail | length > 0 %}
**Failed tasks requiring attention:**
{% for task in failed_tasks_detail %}
- {{ task.component_id }}: {{ task.error_message }}
{% endfor %}
{% endif %}

Begin your review by running the build, then inspect changed artifacts.
```

### 4.3 Context Injection

The review checkpoint task receives enriched context beyond normal tasks:

| Field | Source | Purpose |
|-------|--------|---------|
| `checkpoint_number` | `jobs.checkpoint_count` | Prompt numbering |
| `completed_tasks` | `jobs.completed_tasks` | Progress context |
| `total_tasks` | `jobs.total_tasks` | Progress context |
| `failed_tasks` | `jobs.failed_tasks` | Failure severity |
| `trigger_reason` | Computed at trigger time | Why this checkpoint fired |
| `recent_tasks` | Query: tasks completed since last checkpoint | What to audit |
| `failed_tasks_detail` | Query: failed tasks since last checkpoint | What went wrong |
| `job_type` | `jobs.job_type` | Downlift vs. uplift focus |

This context is assembled in `build_and_execute_task` when the task has
`is_checkpoint = true`, and passed to the prompt builder.

---

## 5. Prompt Builder Integration

### 5.1 Template Registration

Add two new templates to `templates.rs`:

```rust
// In ALL_TEMPLATES:
("review_checkpoint_system", include_str!("templates/review_checkpoint_system.tera")),
("review_checkpoint_user", include_str!("templates/review_checkpoint_user.tera")),
```

### 5.2 Dispatch in `build_prompt()`

Add a checkpoint branch before the standard dispatch:

```rust
pub fn build_prompt(&self, task_context: &TaskContext, ...) -> Result<...> {
    // NEW: Review checkpoint override
    if task_context.is_checkpoint {
        return self.build_review_checkpoint_prompt(task_context, available_tools, config);
    }

    // ... existing dispatch (D4 planning, D4 component, D4 leaf, chat, react)
}
```

### 5.3 Tool Set

Review checkpoint tasks get the **full tool set** (matching AGENTS.md §2.3):

| Category | Tools |
|----------|-------|
| Analysis | `read_file`, `read_spec`, `list_specs`, `validate_spec` |
| Mutation | `write_file`, `edit_file_replace`, `edit_spec_replace` |
| Execution | `run_in_terminal` |
| Git | `git_status`, `git_diff`, `git_commit` |
| Lifecycle | `transition_task_state` |

---

## 6. Worker Integration

### 6.1 Task Context Extension

Add `is_checkpoint` to `TaskContext`:

```rust
pub struct TaskContext {
    // ... existing fields ...
    pub is_checkpoint: bool,
}
```

Populated from `TaskAssignment` which is extended with the field from
the DB query in `poll_for_task`:

```rust
pub struct TaskAssignment {
    // ... existing fields ...
    pub is_checkpoint: bool,
}
```

### 6.2 Workspace Setup

Checkpoint tasks operate on the **job branch directly** (not a per-task
branch), since they need to see the merged state of all completed tasks.
The worker detects `is_checkpoint` and uses the job branch:

```rust
// In build_and_execute_task:
let workspace = if task_context.is_checkpoint {
    // Use the job branch directly — checkpoint needs the merged view
    TaskWorkspace::JobBranch { path: job_branch_path }
} else {
    // Normal per-task isolation
    create_task_workspace(...)
};
```

### 6.3 Post-Completion

When a checkpoint task completes:
- Its commit is on the job branch (no merge-back needed)
- The transition handler resets counters and clears `active_checkpoint_id` (§3.6)
- `unblock_dependents()` automatically releases all gated tasks
- No child tasks are spawned (checkpoint tasks have no `child_component_ids`)

---

## 7. Configuration

### 7.1 Job-Level Configuration

Checkpoint parameters are set at job creation time, either from workspace
settings or from the API request:

```rust
// In job_service.rs create_job():
let review_every_k = workspace_settings.review_every_k
    .or(Some(10));  // Default: every 10 completions
let review_after_n_failures = workspace_settings.review_after_n_failures
    .or(Some(3));   // Default: after 3 failures
```

### 7.2 Workspace Settings

Add to `workspace_settings` table:

```sql
ALTER TABLE workspace_settings ADD COLUMN review_every_k INT;
ALTER TABLE workspace_settings ADD COLUMN review_after_n_failures INT;
```

### 7.3 Guardrails Extension

Extend the `Guardrails` struct (already in `TaskAssignment`):

```rust
pub struct Guardrails {
    pub max_turns: Option<u32>,
    pub max_cost_usd: Option<f64>,
    pub max_execution_time_sec: Option<u64>,
    pub max_total_tokens: Option<u64>,
    // NEW:
    pub review_every_k: Option<u32>,
    pub review_after_n_failures: Option<u32>,
}
```

### 7.4 Disabling Checkpoints

Set either parameter to `0` or `NULL` to disable:

```json
{
  "review_every_k": null,
  "review_after_n_failures": null
}
```

Or via the API:

```
PATCH /api/v1/workspaces/:id/settings
{ "review_every_k": 0 }
```

---

## 8. Edge Cases & Failure Modes

### 8.1 Review Task Fails

If the checkpoint review task itself fails (LLM error, budget exhausted):
- `unblock_dependents()` fires with the failed review task's ID
- The existing unblock logic checks whether all deps are in a terminal
  state (completed OR failed) — dependents are released regardless
- Counters reset, `active_checkpoint_id` cleared (§3.6)
- The next checkpoint fires after another K completions or N failures
- **Rationale:** Blocking the entire job on a review failure would be
  worse than proceeding without review.

**Note:** The standard `unblock_dependents()` implementation must treat
a failed checkpoint dependency as satisfied (not cascade-fail).  Since
checkpoint tasks carry no data that downstream tasks depend on, a
failed review should not cascade-fail its dependents.  Add a guard:

```rust
// In cascade_dependency_failures(), skip cascade for checkpoint deps:
if failed_task.is_checkpoint {
    // Don't cascade — checkpoint failure shouldn't block the pipeline.
    // unblock_dependents() will release gated tasks instead.
    return Ok(());
}
```

### 8.2 Checkpoint vs. Job Completion

The job completion check (`completed + failed >= total`) includes the
checkpoint task in `total`.  Since the checkpoint task must complete
(or fail) before gated tasks can run, the job cannot prematurely
complete while a checkpoint is active.

If the checkpoint is the LAST task (all others completed/failed before
it triggered), the `remaining <= 1` guard in §3.3 prevents the
checkpoint from firing at all.

### 8.3 Cascading Child Tasks During Checkpoint

If a running task completes during an active checkpoint and spawns
child tasks via `spawn_child_tasks`:
- The `active_checkpoint_id` hook (§3.5) adds the review task to the
  children's `depends_on`
- Children are created with `status = 'blocked'` (waiting on review)
- `total_tasks` is incremented normally
- When the review completes, `unblock_dependents()` releases them

### 8.4 Concurrent Checkpoint Triggers

If two tasks complete simultaneously and both would trigger a checkpoint:
- `maybe_trigger_checkpoint()` checks `job.active_checkpoint_id.is_some()`
  and short-circuits if a checkpoint is already active (§3.3)
- **No race condition** because `transition_task()` holds the job row
  lock (`SELECT ... FOR UPDATE`)

### 8.5 Checkpoint During Bootstrap

Bootstrap tasks have `task_type = 'bootstrap'`, not review.  The
`is_checkpoint` check in §3.3 skips bootstrap tasks.  Checkpoints are
only evaluated for regular tasks after bootstrap completes.

### 8.6 Small Jobs

For jobs with fewer tasks than `review_every_k`, no checkpoints fire.
The `remaining <= 1` guard (§3.3) prevents pointless checkpoints when
only one task remains.

### 8.8 Dependency Array Growth

Each checkpoint adds one UUID to pending tasks' `depends_on` arrays.
For a 200-task job with 20 checkpoints (K=10), the worst case is 20
extra UUIDs per task — negligible compared to the inter-task
dependency graph.  Postgres array operations on UUID[] are O(n) but
with n ≤ 50 this is sub-millisecond.

### 8.7 Review Budget

Checkpoint tasks inherit the job's standard `Guardrails` (budget limits).
For large codebases, the review may be token-expensive. Consider setting
a dedicated budget for checkpoint tasks:

```rust
let checkpoint_guardrails = Guardrails {
    max_turns: Some(100),           // Generous for build + audit
    max_cost_usd: Some(5.0),       // Cap per checkpoint
    max_total_tokens: Some(500_000),
    ..job_guardrails.clone()
};
```

---

## 9. Observability

### 9.1 WebSocket Events

New events for checkpoint lifecycle:

```typescript
{ "type": "job.checkpoint_triggered",
  "jobId": "...",
  "taskId": "...",
  "checkpointNumber": 3,
  "gatedTasks": 12,
  "reason": "10 completions since last checkpoint" }

{ "type": "job.checkpoint_completed",
  "jobId": "...",
  "taskId": "...",
  "checkpointNumber": 3,
  "summary": "Build: PASS, 3 spec issues fixed, 1 code fix",
  "unblockedTasks": 12 }
```

### 9.2 Logging

All checkpoint operations log at `INFO` level with structured fields:

```
INFO checkpoint_injected  job_id=X task_id=Y checkpoint_number=3 gated_tasks=12 reason="10 completions"
INFO checkpoint_completed job_id=X task_id=Y duration_sec=120 fixes=3 unblocked_tasks=12
```

### 9.3 Frontend

The job detail page shows checkpoint tasks with a distinctive icon and
status badge. The timeline shows checkpoint triggers as milestone markers
between regular task completions.

---

## 10. Implementation Plan

### Phase 1: Core Mechanism (MVP)

1. **DB migration:** Add `completions_since_checkpoint`,
   `failures_since_checkpoint`, `checkpoint_count`, `review_every_k`,
   `review_after_n_failures`, `active_checkpoint_id`,
   `is_checkpoint` columns
2. **Trigger logic** in `task_service.rs::transition_task()`
3. **Dependency injection** — `inject_checkpoint()` creates review
   task and adds it to all pending/blocked tasks' `depends_on`
4. **Child task gating** — `spawn_child_tasks()` checks
   `active_checkpoint_id` and appends to new tasks
5. **Checkpoint completion handler** — reset counters, clear
   `active_checkpoint_id` (resume is automatic via `unblock_dependents`)
6. **Cascade-failure guard** — skip cascade for checkpoint tasks
7. **Prompt template** — `review_checkpoint_system.tera` +
   `review_checkpoint_user.tera`
8. **Prompt dispatch** — checkpoint detection in `build_prompt()`
9. **Worker workspace** — job-branch checkout for checkpoint tasks

### Phase 2: Observability

11. **WebSocket events** for checkpoint lifecycle
12. **Frontend** checkpoint markers in job timeline
13. **Metric collection** for checkpoint duration, fix count, trigger rate

### Phase 3: Advanced

14. **Adaptive intervals** — increase `review_every_k` if checkpoints
    consistently find zero issues; decrease if they find many
15. **Selective review** — only audit files changed since last checkpoint
    (rather than full codebase)
16. **Parallel review tasks** — split review into spec-audit and
    build-check subtasks for faster execution
17. **Re-queue mechanism** — checkpoint can flag specific completed tasks
    for re-execution if their output was fundamentally wrong

---

## 11. Appendix: Interaction with Existing Mechanisms

### 11.1 Pause/Resume

Manual `pause_job()` works orthogonally.  When a job is paused, no
tasks are scheduled regardless of dependency state.  If a checkpoint
is active when the user pauses, the review task is also blocked by the
pause.  When the user resumes, the review task (and gated tasks) become
eligible again through the normal scheduling path.

### 11.2 Cascade Pruning

Checkpoint tasks have no `child_component_ids`, so they never trigger
cascading. `cascade_prune()` is unaffected.

### 11.3 Lane Sequencing

Checkpoint tasks are part of the current job's task graph. They don't
affect lane sequencing with other jobs.

### 11.4 Cost Tracking

Checkpoint review costs are tracked as normal task costs. They contribute
to the job's total `cost_usd` and to the workspace's spend limits.

### 11.5 Incremental Detection

Checkpoint tasks don't have spec files, so the incremental detection
logic (§Phase 11 from prior work) skips them — `is_checkpoint` tasks
use the review prompt, not downlift/uplift prompts.

### 11.6 Artifact Verification

`verify_expected_artifacts()` already passes vacuously for
`TaskType::Review` (no required artifacts). Checkpoint tasks complete
based on the agent's own assessment of the codebase state.
