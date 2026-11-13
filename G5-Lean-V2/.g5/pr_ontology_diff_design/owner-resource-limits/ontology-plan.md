# Per-owner workspace and running-VM limits

Request: [chat intent](../../chat_intent/owner-resource-limits/request.md). Two new organization defaults stop a runaway owner, usually their agents forking children that fork children, before it takes the whole organization's compute or fills it with workspaces. Both are admission checks: lowering a limit never stops, deletes or transfers anything.

## Ontology changes and governance

Ontology changes are in this branch's diff against `origin/main`:

- [`g5.runtime.placement.owner-limit`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.owner-limit) (new): running-VM limit per owner, checked in placement before the organization quota; refused, not queued; older waiting Starts count.
- [`g5.workspace.owner-limit`](../../ontology/features/workspaces.g5.md#g5.workspace.owner-limit) (new): workspaces-owned limit per owner, checked when a fork or a transfer to that owner is admitted.
- [`g5.organization.owner-limits`](../../ontology/features/organizations.g5.md#g5.organization.owner-limits) (new, human-provided): the request itself.
- [`g5.data.configuration.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.data.configuration.org-admin-settings) (amended): two more registered keys.

| Governing block | Why relevant | Implementation must |
| --- | --- | --- |
| [`g5.runtime.placement.capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity) | Placement is the only path into `starting` and already serializes quota under the org admission lock | Count the owner's claims in `WorkerHostService::place` under that lock; no new lock |
| [`g5.lifecycle.start`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start) | Start consent retained for capacity waits | An owner-limit refusal ends that consent (`failed`), so it is never replayed |
| [`g5.workspace.fork-state.formal`](../../ontology/features/workspaces.g5.md#g5.workspace.fork-state.formal) | Forks admit a durable pending operation, then finalize | Check at admission only; finalize and retries of admitted forks are never refused |
| [`g5.workspace.transfer.formal`](../../ontology/features/workspaces.g5.md#g5.workspace.transfer.formal) | Transfers change which owner a workspace counts for | Check the recipient when a new transfer is admitted |
| [`g5.security.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.security.org-admin-settings) | Same administrator authority | Existing GET/PUT admin-settings route carries the fields |

## Settings

| API field / `configs` key | Default | Range | Effect |
| --- | --- | --- | --- |
| `maxRunningVmsPerOwner` / `max_running_vms_per_owner` | 5 | 0–100 | Most VMs one owner may have starting, running or stopping, plus waiting Starts. 0: the owner can start nothing. |
| `maxWorkspacesPerOwner` / `max_workspaces_per_owner` | 50 | 0–1000 | Most live non-Base workspaces one owner may hold, plus pending forks and transfers to them. 0: the owner can create none. |

A missing key is the default, as for every admin setting. There is no "unlimited" value; 100 and 1000 bound the organization anyway (the organization VM quota is at most 100). Migration 118 re-creates `g5_config_value_is_registered` with two more branches and rewrites no rows. The ownership is the current `workspaces.owner_user_id`; a deactivated member's workspaces stay theirs and count for nobody else.

## Running VMs per owner

`WorkerHostService::place` already holds the organization admission lock, the workspace row and the VM row when it checks the organization quota. Before that check it counts, for the workspace's owner in the organization, the other workspaces whose VM is starting, running or stopping, plus their still-valid waiting Starts (owner- and generation-bound, unexpired, `pending` or `queued_for_capacity`) that were requested before this Start, ordered by `(created_at, workspace_id)` like the start queue. When that count reaches the limit, placement records `start_intent.status = failed` and `last_error = {reason: owner_vm_limit, limit}` and returns `OwnerLimitExceeded`; `resume_with` returns `G5Error::OwnerLimitReached` after the commit.

- Counting earlier waiting Starts keeps every owner's queued demand within their limit, so one owner cannot fill the shared start queue's FIFO window or the managed-capacity demand. Two concurrent Starts by an owner with one VM left: the earlier one is admitted, whichever places first.
- The owner check precedes the organization quota, so a Start that waits for the organization quota or a host already fitted its owner's limit. Only lowering the limit can leave an owner with more waiting Starts than it allows; they are refused, oldest admitted first, as each is replayed. Managed growth does not recount the owner limit and may claim a host for such a doomed Start; the host stays empty and idle retirement removes it. Accepted rather than adding owner ranking to the capacity sweep.
- Refused instead of queued: an agent gets a definite answer to act on, and an owner who stops a VM starts the next one explicitly. Every Start path inherits it: Start/Resume, fork-and-start (the fork succeeds and reports `workspaceStartError`), agent child starts, review workspaces, and formal approval reviews (a typed non-transient error, so the review is terminalized `failed` with the message).

Rejected: a check in `record_start_intent`. It would need the org admission lock in a second transaction and would still race placement; placement already is the single serialized decision.

## Workspaces per owner

The owned count is: live workspaces (`deleted_at IS NULL`, not Base) whose `owner_user_id` is the owner in the organization, plus that owner's `pending` forks, plus `pending` ownership transfers to them. Approval-review and reviewer workspaces count as ordinary workspaces; they are created by the same fork admission.

- **Fork admission** (`WorkspaceForkService::admit_operation`): after the repository locks and the name claim, take a per-owner advisory lock (`g5.workspace-owner-limit:<user>`), count, refuse with `OwnerLimitReached` at or above the limit, then insert the pending fork. Idempotent replays and retries of an admitted fork return before the check. Finalization never re-checks, so an admitted fork cannot be stranded.
- **Transfer admission** (`WorkspaceVmService::transfer`, new transfer only): take the recipient's advisory lock before the workspace row lock and check the recipient after the membership checks. Fork admission takes the same lock before its insert's foreign-key lock on the source workspace, so both paths take the owner lock before any workspace row lock and cannot deadlock. A resumed or replayed pending transfer is not re-checked.
- The lock key is the user, not the (organization, user) pair: it is coarser but needs no organization before the workspace row is read, and contention is per person.

Paths that remain outside the limit, all accepted: the Base workspace at organization creation, operator seed scripts, and lowering the limit.

## Errors and UI

`G5Error::OwnerLimitReached { kind: workspaces | running_vms, limit }` maps to 409 `OWNER_LIMIT_REACHED` with details `{kind, limit}`. Not 429: the frontend retries 429. Its message names the owner's limit and what to do (stop or delete one). The workspace Start toast and the create/fork dialog already show the server message. The dialog keeps its idempotency key: a refused fork wrote nothing, so the same key retried after the owner deletes a workspace is a new admission that re-checks the limit. The Admin page's Workspace runtime group gains the two numeric fields.

## Tests

- Placement: owner limit refuses and fails the intent with the reason; earlier waiting Starts count, later ones do not; other owners and the organization quota are unaffected; limit 0 refuses; lowering the limit stops nothing.
- Start queue: a queued Start of an owner now at the limit is refused on replay and not replayed again.
- Fork: refusal at the limit (pending forks count), deleted and Base workspaces excluded, idempotent replay of an admitted fork succeeds at the limit, approval-review fork terminalizes the review.
- Transfer: refused when the recipient is at the limit; owner's count unaffected for others.
- Settings: round trip, validation ranges, migration registers the keys without seeding.
- Frontend: Admin card fields and validation; API schema.
