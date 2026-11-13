# Organization admin settings: default idle timeout and VM quota

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


## Ontology changes and governance

Request: [chat intent](../../chat_intent/admin-page/request.md), which covers the initial request and the VM-quota follow-up. Ontology changes are in this branch's diff against `origin/main` (`4eff3a2b`):

- [`g5.lifecycle.idle.settings`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.idle.settings) (amended): each organization has a default idle timeout set by its administrators. Every new workspace, including a fork of any source, starts with the default that is current when it is created. This replaces "Base holds the default; a fork copies its source". Existing workspaces never change.
- [`g5.runtime.placement.org-quota`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity) (new; [`g5.runtime.placement`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.placement) now references it): the org runs at most its VM quota (default 10, range 0–100). The quota applies to later placements; lowering it stops nothing.
- [`g5.data.configuration.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.data.configuration.org-admin-settings) (new): both settings are registered org keys in the typed `configs` store. A missing key means the platform default.
- [`g5.security.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.security.org-admin-settings) (new): reading or changing them uses the same administrator authority as Runtime Config.
- [`g5.settings.admin-page`](../../ontology/features/organizations.g5.md#g5.security.org-admin-settings) (new): the whole Admin page and its entry point are admin-only.
- Stories [`g5.story.set-default-idle-timeout`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.idle.settings) and [`g5.story.set-org-vm-quota`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity), indexed in [core user stories](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/core_user_stories.g5.md).

| Governing block | Why relevant | Implementation must |
| --- | --- | --- |
| [`g5.security.org-runtime-config`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/security.g5.md#reserve-organization-runtime-config-mutations-for-administrators) | The authority both settings reuse | Use `github_app::org_admin_db` (`Scope::OrgAdmin` plus a current owner/admin role in the path org) for GET and PUT |
| [`g5.data.configuration.typed`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/data_model.g5.md#keep-typed-configuration-limited-to-its-declared-keys) | `configs` is the typed store with a registered-key validator | Register `default_idle_timeout_secs`; write only the two declared keys |
| [`g5.lifecycle.idle`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.idle) | Consumes the per-workspace value | Unchanged; keeps reading `workspace_settings.idle_timeout_secs` |
| [`g5.fork.replay`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/workspaces.g5.md#freeze-one-recoverable-fork-operation) | A fork operation can resume | The frozen request binds intent, not settings values: read the default in the transaction that inserts the settings row |
| [`g5.quality.migrations.verification`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/system_design_principles.g5.md#validate-migrations-as-the-real-migration-role) | 083 replaces the validator behind a `configs` CHECK | Apply it through the real runner as a NOSUPERUSER/NOBYPASSRLS owner; it writes no rows |

## Outcome and design

An org owner or admin opens **Admin → Organization settings**, edits "Default workspace VM idle timeout (minutes)" and "Organization VM quota", and saves. New workspaces start with the idle default. Placement admits at most the quota of active VMs. Owners can still change their own workspace's timeout in Settings.

### One owner: the typed `configs` store

`configs` already holds `max_running_vms` as a registered org key: `g5_config_value_is_registered` validates it as a non-negative integer, and placement reads it with a fallback of 10. No production code writes it today. The idle default joins it as `default_idle_timeout_secs` (integer, 60–86400, fallback 28800). Rejected alternatives:

- **An `orgs` column for the idle default** (the first draft). It would create a second owner for typed org settings next to `configs`.
- **`org_configs.settings_json`.** That is the untyped Runtime Config bag for env values, with no per-key validation.
- **Base's settings row as the idle default.** It ties Base's own timeout to the org default, and forks of other workspaces still copy their source.

The fallback constants (10 and 28800) move into named constants in `g5-api-data` and are shared by every reader and the API response, so "unset" and "default" display the same.

### Consumers

| Path | Change |
| --- | --- |
| `workspace_fork_service.rs` fork insert | `idle_timeout_secs` comes from the org's `default_idle_timeout_secs` (fallback 28800) instead of the source row; all other fields are still copied |
| `org_service.rs` Base creation, `workspace_service.rs` lazy settings bootstrap | Insert the org default explicitly |
| `worker_host_service.rs::place` | Unchanged logic; the literal 10 becomes the shared constant |

### Concurrency

- **Quota.** Placement takes the org advisory lock (`pg_advisory_xact_lock(hashtextextended(org_id))`), then reads the quota row `FOR UPDATE`. The writer takes the same advisory lock before its upsert. A quota change and a placement are therefore strictly ordered, including the first insert, where no row exists for `FOR UPDATE` to lock. After a raise, `reconcile_start_queue` (periodic and on capacity events) places queued starts while their start intent is still valid. After a lowering, running VMs continue and new starts record `org_vm_quota`, which `vmCapacityWait.ts` already explains.
- **Idle default.** A single upsert. Workspace creation reads the default in its insert transaction, so a workspace gets the value from either before or after the change. No lock is needed.

### API

`GET` / `PUT /api/v1/orgs/:orgId/admin-settings`, body `{ "defaultIdleTimeoutSecs": number, "maxRunningVms": number }`. PUT accepts a partial body and upserts only the fields present, in one transaction. GET returns effective values, which are the defaults when unset. Both are audited routes with `Scope::OrgAdmin` and go through `org_admin_db`. Out-of-range or non-integer values (idle 60–86400 s, quota 0–100) are rejected before writing, and the database validator backs that up. The service lives with the other org identity services, and the route lives in `orgs.rs`.

### UI

`OrgAdminPageContent`'s "Organization settings" section gets a settings card, shown only when `isAdmin` (the page's existing derivation). It has two number inputs:

- Idle timeout in minutes, 1–1440, sent as minutes × 60.
- VM quota, 0–100. A hint warns that 0 blocks every start.

There is one Save button, plus validation, success and error states. The whole page is admin-only ([`g5.settings.admin-page`](../../ontology/features/organizations.g5.md#g5.security.org-admin-settings)): the Account-flyout entry is hidden for members and viewers, and opening `?view=admin` directly shows an "Administrators only" notice instead of the hub. Hints explain that the idle default applies only to new workspaces and that lowering the quota stops no running VM. The API refuses non-admins whatever the UI shows. The hook, API client and zod schema follow the existing settings hooks.

### Migration 083 (schema only)

`CREATE OR REPLACE FUNCTION g5_config_value_is_registered` with the full existing body, adding the `default_idle_timeout_secs` branch (60–86400) and narrowing `max_running_vms` to 0–100. It rewrites no rows. Existing rows are not revalidated; every known row holds 10 or a small test value. As the owner decided, no organization is seeded: until an admin sets a value, new workspaces take the 8-hour platform default and placement uses 10. The change ships in one PR. The old API ignores the new key, so a rollback after migrating is harmless.

### Accepted behavior changes

- A fork never copies its source's idle timeout; it always gets the admin's default (owner's decision).
- An org whose Base had a custom timeout now gets 8 hours for new workspaces until an admin sets the default (owner's decision: no seeding).
- Lowering the quota below the number of active VMs leaves them running, and new starts queue until enough stop.

## Verification

- Migration test `migration_083_org_admin_settings.rs`: applies 083 through `g5-db-migrate` as a NOSUPERUSER/NOBYPASSRLS owner over an existing `max_running_vms` row, then checks that the row is untouched and that the validator accepts and rejects at the new bounds (idle 59/60/86400/86401, quota -1/0/100/101).
- API E2E tests (real Postgres):
  - Authority: owner and admin can GET and PUT; member and viewer get 403; a non-member is masked; invalid values are rejected and leave the stored values unchanged.
  - Idle default: after PUT, forks from Base and from a workspace with a custom timeout both get the new default; existing workspaces and Base keep their values; an owner can still override; a new org's Base and the lazy bootstrap take the default.
  - Quota: with the quota set to N, the (N+1)th start queues with `org_vm_quota` and limit N. Raising the quota lets the queued start place on reconcile. Lowering it below the active count stops nothing and refuses the next start.
- Route-auth static audit passes for the new routes.
- Frontend unit tests: the Admin page and its flyout entry are shown only to owner/admin; minutes convert to seconds; ranges are validated; the partial save body is correct; server errors are shown.
- Required checks: `cargo fmt`/`clippy`/tests for the touched crates, frontend `typecheck`/`lint`/`vitest`, and the migration checksum update.
- Done when, on QA, a fresh fork shows the admin-set idle timeout and a start beyond a lowered quota queues with the quota message.

## Implementation status

Implemented as designed on this branch. Evidence is recorded in the metadata of the blocks listed above. Executed locally against Postgres 17:

- `api_org_admin_settings` (6)
- `migration_083_org_admin_settings` (1)
- `api_org_admin_authority` (21, with both new routes in the cross-org matrix)
- `route_auth_static` (13)
- `api_worker_hosts` (51)
- `api_setup_e2e` (29)
- `api_orgs` (30)
- `api_workspace_fork_names` (2)
- `migration_081_base_idle_timeout` (1)
- the `org_admin_settings` unit tests (3)
- frontend `OrgAdminSettingsCard`, `OrgAdminPageContent`, `UserSettingsPanel` and contract tests

Remaining gaps:

- No browser E2E drives the Admin card.
- The QA check (a fresh fork shows the admin-set timeout; a start beyond a lowered quota queues) waits for deployment.

## Managed capacity settings

Request: [chat intent, managed capacity follow-up](../../chat_intent/admin-page/request.md#follow-up-managed-capacity-on-the-admin-page-2026-10-14). The owner approved the grouped UI (a bordered "Managed capacity" fieldset in `OrgAdminSettingsCard` that shares its Save), then asked for the backend.

Ontology:

- amended: [`g5.architecture.host_demand_capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.architecture.host_demand_capacity) and [`g5.architecture.host_idle_retirement`](../../ontology/software_architecture/host_control_plane.g5.md#g5.architecture.host_idle_retirement) now follow the organization's settings instead of a fixed `c3-standard-4` and one hour;
- [`g5.data.configuration.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.data.configuration.org-admin-settings) lists the new keys;
- new story [`g5.story.set-managed-capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.architecture.host_demand_capacity).

| Setting (API field / `configs` key) | Default | Validation | Effect in `WorkerHostService::reconcile_capacity` |
| --- | --- | --- | --- |
| `autoAddHosts` / `auto_add_hosts` | on | boolean | Off: no new claim for uncovered demand. The Start waits with `capacity_growth_disabled`; existing claims, retries and recovery continue. |
| `autoHostInstanceType` / `auto_host_instance_type` | `c3-standard-4` | Add Host's allowlist (API); `(n2\|c2\|c3)-standard-N` (DB) | Machine type in the provision request for new claims. |
| `autoRetireIdleHosts` / `auto_retire_idle_hosts` | on | boolean | Off: skip the retirement loop. The idle timer keeps running, so turning removal on again is accurate. |
| `idleHostRetireSecs` / `idle_host_retire_secs` | 3600 | 900–86400 | Replaces the fixed one hour in `idle_expired`. |

- **Ownership.** `org_admin_settings` owns the keys and fallbacks and returns them as one `ManagedCapacitySettings`. `host_capacity` reads that inside the admission lock, so each sweep decides under one consistent snapshot. The writer takes the same org lock whenever the quota or any capacity field changes, so a sweep and a change are strictly ordered.
- **Allowlist location.** The instance-type allowlist moves from `hosts::host_provisioning` to `g5_contracts::types::worker_host`. Identity may not reference the hosts context (`service_context_layering`), and both Add Host and the admin setting must use the same list.
- **Migration.** 083 (unmerged) gains the four registered keys. It is still schema only.
- **Frontend.** Already built. The DTO fields stay optional so the card keeps working against an API replica from before this change during a rolling deploy. `vmCapacityWait.ts` adds a message for `capacity_growth_disabled`.
- **Assumption (no operator ceiling).** Admins choose only from the Add Host allowlist, removal can't go below 15 minutes, and the quota is capped at 100. No separate operator ceiling is added for hosted orgs. That remains a possible follow-up if billing requires one.

Verification:

- `api_host_capacity` gains cases for each setting through real sweeps with the cloud double:
  - growth off creates nothing and records the reason;
  - growth turned back on provisions with the chosen machine type;
  - removal off retains an expired idle host;
  - a 15-minute delay retires a host idle for 16 minutes but not 14.
- `api_org_admin_settings` covers read/write, validation and authority for the new fields.
- The 083 migration test covers the new bounds.
- Frontend tests cover the group and the new wait message.

Implemented and executed locally against Postgres 17:

- `api_host_capacity` (24, two new);
- `api_org_admin_settings` (8, two new);
- `migration_083_org_admin_settings`;
- `org_admin_settings` unit tests (4);
- `api_worker_hosts` (51), `api_org_admin_authority` (21), `route_auth_static`, `service_context_layering`, and `migration_045`/`066`;
- frontend card and wait-message tests.

Remaining gap: no live GCE boot with a non-default machine type.
