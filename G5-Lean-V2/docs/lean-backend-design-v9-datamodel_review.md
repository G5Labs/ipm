# Backend design v9 data model review

Reviewed 2026-09-18 in `/Users/zliu/Workspace/G5-Lean-v9`, branch `docs/backend-design-v9`, commit `33154ac`. Scope: the DDL blocks and the retained inventory in [design v9](lean-backend-design-v9.md) §5.1, §7.1, §8.1, §10.2, §10.3 and §12, plus the Git ref namespaces of §5.3. This note is a companion to the [main v9 review](lean-backend-design-v9-review.md), which hunts for gaps and contradictions. It asks the opposite question: which parts of the data model are heavier than the agreed outcomes require. It is the opinion of a reviewing architect, written before any of the new tables exist. Product decisions stay with the team; the main review's findings stand regardless of what is decided here. Nothing was edited except creating this file.

## Verdict

The spine of the model is lean and should not be touched: Git refs are authoritative for files, one row per workspace holds VM custody, and governance shrinks to three tables. The edges are over-built. Roughly a third of the new columns and tables serve four features that the MVP could drop or simplify, and each of those features drags a saga behind it: its own status enum, idempotency key and recovery narrative. Cutting mid-turn result submission, purpose conversion and rollout continuation from the MVP, and dropping two of the three receipt ref namespaces, removes about six tables' worth of columns and most of §5.9 without changing the meeting's agreed outcomes: durable results, fork-based review, compute released on completion.

## What is lean and should stay

- **Git refs authoritative, `workspace_repos` without a head.** `head_sha_cache` is declared a projection ([design §5.1](lean-backend-design-v9.md) line 247) and every read resolves `refs/heads/<branch>`. This removes the whole class of head-authority bugs.
- **One custody row.** `workspace_vms` ([design §5.1](lean-backend-design-v9.md) lines 233–238) with `vm_id`, `epoch`, lease, `stop_reason`, `last_flush_id` and `unflushed_loss_at` is the only incarnation record, and every boundary fences on it (§6.4). Correct and minimal.
- **Governance as three tables.** `pull_requests` with the merge-operation columns inline ([design §10.2](lean-backend-design-v9.md) lines 898–903), `approval_rules` and `approval_decisions` ([design §10.3](lean-backend-design-v9.md) lines 911–915) replace the 11-table ledger. One operation at a time per PR justifies the inline columns.
- **`projection_state` and the convergence loop** ([design §9.4](lean-backend-design-v9.md) line 876) instead of event-driven projections.
- **`workspace_fork_reservations`** ([design §5.1](lean-backend-design-v9.md) lines 213–214). A primary key on `(repo_id, branch_name)` is the cheapest correct way to stop two forks winning one name across all member repositories. Keep it.

## Over-design, ranked by leverage

### D1 — The result pipeline: four objects for one result

**Objects.** `workspace_result_submissions` (lines 223–225), `workspace_results` (lines 227–229), `workspace_result_replies` (line 231) and `workspace_requests.selected_submission_id` (line 219).

**Driver.** `result submit` runs during an agent attempt ([design §5.9](lean-backend-design-v9.md) line 450), so the design needs a candidate outbox, per-attempt selection fencing, digest and idempotency-key uniqueness, and rules for late old-attempt candidates and conflicting payloads (lines 450–454). Those rules are the bulk of §5.9 and the source of the main review's N1, N19, N20 and N74.

**Why it is heavier than needed.** The design already commits one transaction per attempt that "update[s] its assistant output and either finalize[s] the logical task or set[s] a held/queued continuation" ([design §7.5](lean-backend-design-v9.md) line 736). A result that is the producer task's final structured output can be persisted in that same transaction. Nothing then needs to be captured mid-turn, selected or fenced.

**Simplification.** Either of two options, both compatible with the decision record, which left database-versus-Git storage open ([decisions](v9-meeting-decisions.md), draft choice 5):

- **(A) Result at C3.** The producer's final message carries a typed result block (schema `v1`, size-bounded). The adapter validates it at C3, TaskService writes one `workspace_results` row in the finalization transaction, and the request completes. Drop `workspace_result_submissions` and `selected_submission_id`. The CLI keeps `get|list`; `submit` disappears.
- **(B) Result as a committed file.** The producer commits `.g5/review/result.json` on the child branch and pushes; the request stores `result_oid` at C3 of the designated task. Readers get it through the Git-only read path they already have (§9.1), parent visibility follows the request ACL, and export is the file itself. Zero new payload tables.

For discussion, reuse the retained `comment_index`, `comment_replies` and `comment_mentions` ([design §12](lean-backend-design-v9.md) line 984) anchored to a request instead of adding `workspace_result_replies`.

**What it does not remove.** N1 (who designates the producer task) still needs an answer under both options.

### D2 — `purpose`, `cleanup_status` and Keep as workspace

**Objects.** `workspaces.purpose general|review|explainer` (line 195), `workspace_requests.cleanup_status pending|released|blocked` and `cleanup_error` (line 220), the compute-cleanup and abandonment rules ([design §5.9](lean-backend-design-v9.md) lines 456–458) and the Keep-as-workspace compare-and-set (line 458).

**Why it is heavier than needed.** `purpose` "controls the child's compute policy, not task kind or permission policy" (line 444). Its whole effect is a 15-minute idle default instead of 30 and a stop after completion. Completion already uses the ordinary stop sequence with `stop_reason=purpose_complete` (line 456), which `workspace_vms.stop_reason` records (line 237). A second, independent `cleanup_status` state machine duplicates what `workspace_vms.status`, `stop_reason` and `last_error` already express for every other stop.

**Simplification.** Completion is an idempotent stop request recorded on `workspace_vms`; a failed stop is visible the way every failed stop is. Idle timeout is a per-workspace setting, which already exists twice (`idle_timeout_secs` in `VmSpec`, line 521, and `workspace_settings`, line 976). Drop `purpose`, `cleanup_status`, `cleanup_error` and the conversion semantics. A completed child is just a stopped workspace the owner may resume. Related: N30.

### D3 — Rollout continuation authority

**Objects.** `workspaces.owner_resume_required` (lines 194, 266), `tasks.continuation_pending`, `continuation_reason` and `hold_reason` (lines 661–662), the three meanings of `waiting` ([design §7.2](lean-backend-design-v9.md) line 703), and the drain/rollout carry-forward rule (lines 266, 647, 709, 719).

**Why it is heavier than needed.** `owner_resume_required` is an inverted flag whose only real job is to let a VM drained for a rollout restart without the owner. Everything else it encodes is derivable: a stopped workspace requires Resume. The flag pulls in the "recorded continuation authorization" that the main review found has no data-model home (N25), the four overlapping admission barriers (N30), and the hold and continuation fields without values or owner (N52, N122).

**Simplification for the MVP.** Every stop requires owner Resume; a rollout interrupts agents after the nudge (line 420) and the owner clicks Resume. An interrupted attempt leaves the task `waiting` with one `hold_reason in (native_request, interrupted, suspended, vm_lost, publication)`; Continue creates the next attempt. Drop `owner_resume_required` and `continuation_pending`, and fold `continuation_reason` into `hold_reason`. Automatic continuation can return later as a single additional column, if rollouts turn out to be frequent enough to justify it.

### D4 — Receipt refs for forks and merges

**Objects.** `refs/g5/forks/<fork id>` ([design §5.3](lean-backend-design-v9.md) line 287; [§5.8](lean-backend-design-v9.md) line 401) and `refs/g5/merges/<merge op id>` (line 286; [§10.6](lean-backend-design-v9.md) line 936), each with a receive-pack policy row and a pruning rule.

**Why it is heavier than needed.** Both exist to prove, after a lost reply, that this operation created or applied a ref. The API runs git on its own storage, so a lost reply is a crash mid-operation, and the operation row already "persist[s] progress in the manifest" before external effects (line 401). For a fork, a destination ref that exists at the pinned source OID while this operation holds the reservation could only have been created by this operation. For a merge, `merge_result_sha` is persisted before any ref moves (line 936), protected refs move only through merges and imports, and `git merge-base --is-ancestor` on the target answers "was it applied". The one case the receipt guards against, a provider-side rewrite of a protected branch imported over the result, leaves the target in a state the receipt cannot repair anyway.

**Simplification.** Keep only `refs/g5/flush/*`, which holds objects nothing else retains. Drop the two receipt namespaces, their policy rows and their pruning (this also closes N93). Keep `workspace_fork_reservations`.

### D5 — Three configuration stores

**Objects.** `workspace_settings` "(idle timeout, native-response deadlines, retain hours, explicit policy defaults)" ([design §12](lean-backend-design-v9.md) line 976), `configs(scope, scope_id, key, value)` "for org, user and workspace configuration and feature flags" (line 984), and `user_preferences` (line 984).

**Simplification.** Pick one shape per scope. Typed columns for the handful of workspace knobs and `configs` only for feature flags is the readable choice for an AI implementer; a generic `configs` for everything is the smaller schema. Retaining both for the same scope guarantees two code paths and a precedence rule.

### D6 — `provider_requests` at 22 columns

**Objects.** [design §7.1](lean-backend-design-v9.md) lines 674–679.

**Why it is heavier than needed.** `exec_id` is stored whole and as its three components; `epoch` duplicates `task_attempts`; `payload_ref`, `display_ref` and `response_ref` indirect small JSON through references; `response_version` supports idempotent re-answers that a first-response-wins rule makes unnecessary; `reply_pending` is a transient of the C1 send, not a durable state.

**Simplification.** About 14 columns: `id, task_id, attempt, vm_id, provider_request_id, kind, tool_name, payload jsonb, response jsonb NULL, status pending|responded|invalidated, responded_by, deadline_at, created_at, ended_at, end_reason`, with `UNIQUE (task_id, attempt, vm_id, provider_request_id)`. The first response wins; a second gets 409. Delivery evidence lives inside `response`.

### D7 — `chat_messages` carrying two lifecycles

**Objects.** [design §8.1](lean-backend-design-v9.md) lines 752–757: `delivery queued|reserved|delivered|delivery_unknown|withdrawn` for user inbox rows, `status pending|streaming|complete|error|stopped` for assistant rows, plus `revision`, `input_origin`, `template_ref` and `template_revision`.

**Why it is heavier than needed.** An inbox item is mutable until reserved and has a delivery lifecycle; a transcript message is immutable. Putting both on one row means half the columns are null on every row and two enums whose combinations are mostly illegal. The main review's N88 and N90 are symptoms.

**Simplification.** Either split a small `session_inputs` table (queue state, revision, reservation, `task_id`/`attempt`) from an immutable `chat_messages` transcript, or drop editing of queued input (withdraw only) so `revision` goes away and `delivery` shrinks to `queued|delivered|delivery_unknown|withdrawn`.

### D8 — `tasks.depends_on uuid[]` and jobs in the MVP

**Objects.** `depends_on uuid[]` (line 660), `jobs` (lines 668–669), job kinds that are ontology profiles (line 684).

**Why it is heavier than needed.** An id array has no foreign key, no cascade and awkward queries. The consumers of DAG jobs are `uplift`, `downlift` and `sync`, which §2 places outside MVP acceptance (line 68), yet §13 keeps "agent tasks and jobs on the dispatcher" in the MVP (line 1001). The same-task continuation machinery of D3 exists largely so that DAG edges survive interruptions.

**Simplification.** If dependencies stay, use `task_dependencies(task_id, depends_on_task_id, PRIMARY KEY (task_id, depends_on_task_id))`. Better, defer DAG jobs from MVP acceptance with their consumers; a job as an ordered list of tasks needs no edges and no continuation-preserving identity.

### D9 — Leftovers and redundancies

- `workspace_flushes.materialization snapshot` (line 241) has one legal value. Drop it.
- `is_base` (line 193) is the same fact as `branch_name IS NULL`. Keep one; a check constraint can tie them if both stay.
- `name` and `branch_name` are equal by rule for working workspaces (line 270). One column plus `slug` is enough.
- `vm_id` plus `epoch` is a double identity, but defensible: `vm_id` fences, `epoch` orders harvests so an older transcript never overwrites a newer one (line 823). Say so in the DDL comment.
- `tasks` mirrors `attempt`, `vm_id`, `epoch` and `start_confirmed` from the current attempt (line 660). Fine as a current-attempt cache; declare it one.
- `chat_session_files` stores provider transcripts as `bytea` up to 128 MiB per file (lines 759–761, 823). Blobs of that size belong beside the git store on `RepoStorage`, with only `sha256` and size in Postgres. Related: N11.
- `task_events` (lines 671–672) has no retention. Define one with the journal replay window. Related: N11.

## The structural point: four sagas

The model contains four bespoke multi-step operations: fork (`workspace_forks` plus reservations, lines 207–214), transfer (`workspace_ownership_transfers`, lines 200–202), request with cleanup (`workspace_requests`, lines 216–221) and merge (the columns on `pull_requests`). Each has its own status enum, idempotency key and recovery narrative. The pattern itself is sound and consistent. The cost is that an AI implementer must build and test four state machines, and three of the four are only as complex as they are because of D1 to D3. After those cuts, fork and transfer remain as genuine sagas, the request becomes a plain row with a pinned source and a result reference, and the merge operation stays as designed.

## A product cost the model makes visible

Every review round is a full child workspace: a unique branch in every member repository including docs (line 405), flush refs, a stop and cleanup. `workspace_results.request_id UNIQUE` (line 227) and "a later review of new content is a new request and child" (lines 444, 458) mean five review rounds on one PR are five children and five branch names the reviewer must invent, each checked against the provider namespace. This is not a data-model defect; the meeting chose fork-based review. Two ways to lower the cost without changing that choice: allow a request to be re-run in its existing child against a new pinned source (the child rebases, which §17.2 defers at line 1141), or allow several versioned results per request. Either removes the UNIQUE constraint and the one-child-per-round ceremony.

## Proposed changes to the retained inventory

| Object | Action | Effect |
|---|---|---|
| `workspace_result_submissions`, `workspace_requests.selected_submission_id` | drop (D1) | result written once at C3 or pinned as `result_oid` |
| `workspace_result_replies` | replace with retained `comment_*` anchored to a request (D1) | one discussion mechanism |
| `workspaces.purpose`, `workspace_requests.cleanup_status`, `cleanup_error` | drop (D2) | completion is an ordinary stop; idle timeout is a setting |
| `workspaces.owner_resume_required`, `tasks.continuation_pending`, `continuation_reason` | drop; keep one `hold_reason` (D3) | every stop requires Resume |
| `refs/g5/forks/*`, `refs/g5/merges/*` | drop, with their policy rows and pruning (D4) | only flush refs remain |
| `workspace_settings` vs `configs` vs `user_preferences` | keep one shape per scope (D5) | one precedence rule |
| `provider_requests` | 22 to about 14 columns, inline jsonb, first response wins (D6) | |
| `chat_messages` | split inbox from transcript, or drop queued editing (D7) | one lifecycle per row |
| `tasks.depends_on uuid[]` | `task_dependencies` table, or defer DAG jobs (D8) | referential integrity |
| `workspace_flushes.materialization`, `is_base` or `branch_name`, `name` | drop the redundant column (D9) | |
| `chat_session_files.bytes` | move blobs beside the git store (D9) | Postgres holds hashes and sizes |
| `task_events` | add retention (D9) | bounded growth |

## Limits

This is an architectural opinion formed from the text, not a measurement; no table has been built or loaded. Several items are product decisions (mid-turn submission, automatic rollout continuation, review as a full workspace) that the meeting either chose or left open, and this note only prices them. The main review's findings apply to the design as written; if the cuts above are taken, N1, N19, N20, N25, N30, N52, N74, N88, N93 and N122 shrink or disappear, and the remaining ones are unaffected.
