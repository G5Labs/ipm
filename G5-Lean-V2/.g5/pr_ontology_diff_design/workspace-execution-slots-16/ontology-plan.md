# Default workspace execution slots of 16

## Ontology changes and governance

Request: [chat intent](../../chat_intent/workspace-execution-slots-16/request.md). The request ships in two PRs (see Rollout order); this plan covers both and is updated in place.

- PR 1 ([G5Labs/G5-Lean#388](https://github.com/G5Labs/G5-Lean/pull/388), compared with `origin/main` at `b0e7a1a751d65b12da66be3f922e8e48a92211c7`) added [`g5.agents.admission.vm-slots`](../../ontology/features/agents.g5.md#size-the-vm-catalog-for-sixteen-agent-turns): the VM catalog admits 16 agent turns with headroom for task-less operations, and a VM on an earlier four-slot catalog admits at most two turns until replaced.
- PR 2 (this change, compared with PR 1's head `62eeda877d04955697d59c941d65866d79da109f`) adds [`g5.agents.admission.default-slots`](../../ontology/features/agents.g5.md#default-to-sixteen-workspace-slots) and points `vm-slots` at the default:

```diff
diff --git a/.g5/ontology/features/agents.g5.md b/.g5/ontology/features/agents.g5.md
index c18222a4..ba49e05b 100644
--- a/.g5/ontology/features/agents.g5.md
+++ b/.g5/ontology/features/agents.g5.md
@@ -180,11 +180,19 @@ Apply live admission and dependency readiness while enforcing workspace slots an
 
 Native waits occupy bounded-work slots while preview remains exempt.
 
+### Default to sixteen workspace slots
+
+[D · g5.agents.admission.default-slots]
+
+A workspace runs up to 16 agent and command turns at once unless its owner sets another slot count (1–64) in workspace settings. Slots bound concurrency only: concurrent turns share the VM's CPU, memory and disk.
+
+Refines [one admission policy](#one-admission-policy-covers-jobs-and-individual-work).
+
 ### Size the VM catalog for sixteen agent turns
 
 [D · g5.agents.admission.vm-slots]
 
-The VM module catalog admits 16 agent turns, and its total bounded slots also cover the task-less Changes and diagnostics operations, so Changes keeps working while every turn runs. On a VM started from an earlier four-slot catalog, a workspace admits at most two agent or command turns until the VM is replaced, keeping Changes' headroom there. Module caps still apply: a VM runs one bootstrap command at a time, and agent turns beyond the catalog cap wait at the VM unconfirmed.
+The VM module catalog admits as many agent turns as the [default workspace slots](#default-to-sixteen-workspace-slots), and its total bounded slots also cover the task-less Changes and diagnostics operations, so Changes keeps working while every turn runs. On a VM started from an earlier four-slot catalog, a workspace admits at most two agent or command turns until the VM is replaced, keeping Changes' headroom there. Module caps still apply: a VM runs one bootstrap command at a time, and agent turns beyond the catalog cap wait at the VM unconfirmed.
 
 Refines [one admission policy](#one-admission-policy-covers-jobs-and-individual-work).
 
@@ -523,11 +531,17 @@ consolidation. Copying the memory directory does not remove these limits.
 - From [C114–C116](../../uplift/g5-lean/candidates/contracts.md).
 - ~90% · dispatch/runtime occupancy; full native-wait/preview concurrency matrix unexecuted.
 
+### g5.agents.admission.default-slots
+
+- human-provided, human-verified
+- The value 16 is the owner's 2026-10-08 request ([chat intent](../../chat_intent/workspace-execution-slots-16/request.md)), made after a new chat sat queued behind two running turns under the v9 draft default of 2 ([design §7.3](../../../docs/lean-backend-design-v9.md#73-dispatch-path), review N86). The owner also chose, on 2026-10-08, to raise every existing row on 2: no UI sets the column, an explicit 2 set through the settings API cannot be told apart from the old default, and forks copy their source's value.
+- ~90% · migration `070_workspace_execution_slots_default.sql` sets the column default and `071_raise_default_workspace_execution_slots.sql` raises rows on 2 under the table-owner exemption, deployed only after the [VM ceiling](#size-the-vm-catalog-for-sixteen-agent-turns) is live; `g5-api-data/src/models/workspace.rs::DEFAULT_BOUNDED_EXECUTION_SLOTS` backs the missing-row fallback in `spend_service.rs`; `agent_dispatch.rs::reserve_queued_agent_dispatch` enforces the count. Tests: `api_task_lifecycle.rs::a_default_workspace_runs_sixteen_chat_turns_and_queues_the_next`, the default reads in `api_setup_e2e.rs`, and `migration_071_workspace_execution_slots.rs` (populated upgrade as a NOSUPERUSER/NOBYPASSRLS owner, replayed as a no-op). Gaps: no UI shows or sets the count; a turn waiting for a slot shows only "Queued for workspace…" (the designed `queued_for_slot` is unbuilt); nothing reserves VM resources for 16 heavy turns.
+
 ### g5.agents.admission.vm-slots
 
 - human-verified, agent
 - From the owner's 2026-10-08 request to default workspaces to 16 slots and their question whether the VM's limit of 4 should rise as well ([chat intent](../../chat_intent/workspace-execution-slots-16/request.md)). The agent-turn cap, the headroom for task-less operations and the two-turn ceiling on four-slot VMs are agent decisions; the ceiling ships before any workspace is allowed more turns, so no API without it ever reads raised settings, even after a deploy rollback.
-- ~85% · `backend/g5d-dispatch/modules/catalog.json` (`2026.10.08-sixteen-agent-turns`): `agent.turn` concurrency 16, `bounded_execution_slots` 20 (16 + `workspace.changes` 2 + `workspace.diagnostics` 2), enforced by `g5d-dispatch/src/modules/mod.rs`; `catalog.sha256` and `workspace_vm_service.rs::MODULE_CATALOG_HASH`/`MODULE_CATALOG_VERSION` pin it, and `workspace_vm_service.rs::shipped_catalog_admits_sixteen_agent_turns_with_operation_headroom` checks the headroom. `agent_dispatch.rs::workspace_slot_limit_sql!` caps both reservation queries at two on a VM whose recorded `workspace_vms.module_catalog_version` is one of the six earlier four-slot catalogs (`api_task_lifecycle.rs::a_four_slot_catalog_vm_admits_at_most_two_turns`). An independent review probe ran the real dispatcher on this catalog: 16 live agent children, the 17th refused, Changes served while they ran. Gaps: the workspace default is still 2, so 16 turns need an explicit setting until the default changes; no live VM ran 16 turns under load; a Stop and Start reaches the new catalog only once a host runs the new worker image, because old-image hosts refuse the new pin (`RuntimeMismatch`).
+- ~85% · `backend/g5d-dispatch/modules/catalog.json` (`2026.10.08-sixteen-agent-turns`): `agent.turn` concurrency 16, `bounded_execution_slots` 20 (16 + `workspace.changes` 2 + `workspace.diagnostics` 2), enforced by `g5d-dispatch/src/modules/mod.rs`; `catalog.sha256` and `workspace_vm_service.rs::MODULE_CATALOG_HASH`/`MODULE_CATALOG_VERSION` pin it, and `workspace_vm_service.rs::shipped_catalog_admits_the_default_workspace_slots` ties the catalog to the default. `agent_dispatch.rs::workspace_slot_limit_sql!` caps both reservation queries at two on a VM whose recorded `workspace_vms.module_catalog_version` is one of the six earlier four-slot catalogs (`api_task_lifecycle.rs::a_four_slot_catalog_vm_admits_at_most_two_turns`). An independent review probe ran the real dispatcher on this catalog: 16 live agent children, the 17th refused, Changes served while they ran. Gaps: no live VM ran 16 turns under load; a Stop and Start reaches the new catalog only once a host runs the new worker image, because old-image hosts refuse the new pin (`RuntimeMismatch`).
 
 ### g5.agents.admission.permission-policy
```

### Relevant and governing ontology

| Block | Why relevant | Implementation must |
| --- | --- | --- |
| [`g5.agents.admission`](../../ontology/features/agents.g5.md#one-admission-policy-covers-jobs-and-individual-work) | Owns slot enforcement for agents and commands | Keep enforcement in the existing dispatch queries; add only the per-VM ceiling |
| [`g5.agents.admission.occupancy`](../../ontology/features/agents.g5.md#keep-slot-accounting-faithful-to-work) | Native waits hold slots, preview is exempt | Leave accounting unchanged |
| [`g5.lifecycle.start.held-work`](../../ontology/software_architecture/lifecycle.g5.md#start-returns-held-work-to-the-queue) | Start re-queues held turns, which then take slots ahead of new messages (the QA case) | Nothing new; 16 slots leave room for new chats beside re-run turns |
| [`g5.quality.migrations.verification`](../../ontology/concepts/system_design_principles.g5.md#validate-migrations-as-the-real-migration-role) | PR 2's 071 updates a FORCE-RLS table | Prove a populated upgrade and replay as a NOSUPERUSER/NOBYPASSRLS owner |

## Outcome and design

A new chat message is no longer held back while two other turns run: a workspace that never chose a slot count admits 16 concurrent agent and command turns, and its VM accepts them.

```
PR 1  VM catalog: agent.turn 16, bounded_execution_slots 20 (= 16 + Changes 2 + diagnostics 2)
      dispatch:   running agent+command < (VM on a four-slot catalog ? LEAST(slots, 2) : slots)
PR 2  workspace_settings.bounded_execution_slots default 16 (070), rows on 2 -> 16 (071)
```

The catalog is pinned by hash in the API and checked by hosts, so PR 1 ships as a worker image rollout under the existing drain-and-replace rule (design v9 §6.10). Its ceiling keys on `workspace_vms.module_catalog_version`, which a running VM always holds as verified at Ready, matched against the closed list of the six earlier four-slot catalogs; later catalogs are never capped. No new state is added.

### Rollout order

Migrations run in a Helm pre-upgrade hook while the previous API still serves, and a failed deploy rolls the API back while the migrations stay. So the settings change must never meet an API without the ceiling: PR 1 merges and deploys to QA first; PR 2 merges only after that deploy succeeds. An environment that upgrades across both at once would reopen that window.

Acceptance:

- PR 1: on a VM recording an earlier four-slot catalog, a workspace allowed 16 slots reserves two agent or command turns and admits more once the VM records the sixteen-turn catalog; the shipped catalog admits 16 agent turns plus operation headroom; default workspaces behave as before.
- PR 2: a default workspace reserves 16 turns and queues the 17th; every existing row on 2 reads 16 after the upgrade, while explicit values other than 2 stay.

Risks and assumptions:

- Until a host runs the new worker image it refuses VM starts pinned to the new catalog (`RuntimeMismatch`), so a Stop and Start reaches the new catalog only once upgraded host capacity exists.
- PR 1 lowers a workspace explicitly set above 2 slots to two turns on a VM with an earlier catalog. No UI sets the value; only a settings API client can.
- PR 2's backfill raises every row on 2: no UI sets the value, and an explicit 2 set through the settings API cannot be told apart from the old default, so it is raised too (the owner's decision).
- Slots bound concurrency, not resources: 16 heavy turns share one VM's CPU, memory and disk.

## Review summary

Independent review (Codex) round 1 confirmed with a live-dispatcher probe that raising settings let four turns fill an old-catalog VM and refuse Changes, which the ceiling addresses; round 2 found that a single deploy still exposed raised settings to the previous API during the migration hook and after a rollback, which the two-PR order addresses. Remaining uncertainty: no live VM ran 16 concurrent turns under load; a waiting turn still shows only "Queued for workspace…" (the designed `queued_for_slot` is a separate follow-up).
