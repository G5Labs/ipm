# Lean Filestore: HDD to Zonal

Scope: QA, staging, production, one environment at a time. Target: ZONAL,
1,024 GiB, NFSv3, fixed custom performance 4,000 IOPS, existing regional CMEK,
daily native backups retained for 30 days. Keep the old source and its backups.
This is a storage migration, not an application/schema upgrade or host rotation.

**Single enablement PR:** #1167 commits `provision_zonal: true` and `active: zonal`
for QA, staging and production. Before the owner merges it, hold all automatic
Terraform apply/bundle, application deployment, promotion and train paths and
retire incompatible running/queued/pending builds. Keep those holds through all
three cutovers. The committed map and Terraform outputs are desired state;
existing applications and backups continue using HDD until explicitly switched.
The owner requested QA cutover as soon as the copy and checks pass; validate QA
before staging/prod. No additional migration PRs are required.

## Configuration and preparation

`infra/config/filestore-migration.json` is the committed selector used by platform
outputs, backup automation and deployment trigger inputs. `provision_zonal: true`
creates the candidate; `active: legacy` leaves all consumers on HDD. Change
`active` to `zonal` in the single enablement PR under the global delivery hold;
apply its consumers only during that environment's controlled cutover. The old
Filestore resource address and its destruction protection remain intact.

| Environment | Project / cluster | Old instance | New instance |
| --- | --- | --- | --- |
| QA | `g5-qa-1ccc` / `g5-lean-qa` | `g5-lean-qa-repos` | `g5-lean-qa-repos-zonal` |
| Staging | `g5lean-staging-1ccc` / `g5-lean-staging` | `g5-lean-staging-repos` | `g5-lean-staging-repos-zonal` |
| Production | `g5lean-prod-1ccc` / `g5-lean-prod` | `g5-lean-prod-repos` | `g5-lean-prod-repos-zonal` |

Clusters are regional in `us-central1`; Filestore instances are in `us-central1-a`.
Both shares are named `g5_repos`. Do not reuse the old Basic tier's /29 address
range: Zonal needs a separate allocation. Use the locked google-beta provider
for new storage; do not upgrade the legacy provider or change the old tier.

1. Refresh live instance, backup, quota and current deployment inventories.
   Confirm the current source/IP/PV/PVC against the deployment, not old notes.
2. Store original Helm release/chart/values, replica counts, images, PV/PVC,
   workflow/scheduler and deployment-trigger configurations in a local directory
   with mode 0700; files containing state, values or logs use mode 0600. Never
   commit these files or send them for external code review.
3. Before the owner merges the single enablement PR, hold every automatic
   Terraform apply/bundle, application deployment, promotion and train path.
   Inventory running, queued and approval-pending executions and their pinned
   sources; drain or cancel every incompatible execution. Merging does not
   update captured bundles. Keep the hold until all three environments match
   the final map. App traffic can remain available during initial copying.
4. After merge, use fresh **refreshed** direct operator plans under that hold.
   The disabled QA apply trigger cannot perform this operation: its normal
   pipeline requires an enabled trigger, applies whole roots and publishes the
   downstream bundle. Staging/prod CI uses `refresh=false`, so it is not live
   migration evidence. Do not run a normal full apply before the copy is verified.
   Plan only the candidate target against each real platform state, one environment
   at a time. Expected mutation: **one new Filestore instance**. Target dependencies
   include networking/KMS; inspect actual changes and reject unrelated creates,
   updates or deletes. Apply only the inspected saved plan. A full plan is still
   required before normal delivery resumes.
5. Confirm the new instance is READY, NFSv3, 1,024 GiB, fixed 4,000 IOPS, protected,
   and in the intended VPC with the intended CMEK. Verify live application mounts
   and Workflow sources still identify HDD. Candidate-only applies can publish
   Zonal desired-state outputs; those do not prove that data or mounts moved.

The platform's `filestore_migration` output describes selected/candidate storage.
For QA the candidate target is `module.filestore_zonal`; for staging/prod it is
`module.environment.module.filestore_zonal`. Use the existing backend and actual
environment settings. Never apply a full unrelated plan to simplify provisioning.

## Copy the whole share

Use an isolated copy Pod or VM in the same VPC. It needs no application credentials
or Kubernetes service-account token. Mount **the share root** from the old source
read-only at `/mnt/source`, and the new destination read/write at `/mnt/destination`.
Record both actual NFS exports and verify them with `findmnt` before every copy.
Never copy only `repos/`: `attachments/` and all other persisted share content must
also survive. Copy tools and logs belong outside the mounted shares.

Inventory source numeric ownership, POSIX/default ACLs, extended attributes,
special files, allocated bytes and file count. Zonal does not support Basic's
`setfacl`; stop for a meaningful ACL that cannot be preserved. Likewise identify
extended attributes and validate support before choosing preservation flags.
No unsupported metadata may be silently discarded. The commands below assume
the inventory confirmed ordinary permissions and no required extended attributes.

```bash
# Online first pass: changes during this pass require a later frozen final pass.
rsync -aH --numeric-ids --stats -- /mnt/source/ /mnt/destination/
```

Preserve source files unchanged, including numeric UID/GID, hardlinks, symlinks,
private Git refs, bare repository configuration and immutable snapshots. A live
first pass is not a consistency proof. Do not use `--inplace`, exclusions, or
`--remove-source-files`.

## Maintenance window and final verification

**This part interrupts that environment's API/Git access.** Confirm the window
and finish QA before proceeding to staging and production.

1. Hold application and Terraform deployment paths, the Terraform bundle producer,
   release train, and approval
   paths that can start target deployments. Record previous enabled/disabled states.
   Disabling a trigger does not stop existing builds: inspect running/queued/pending
   builds, drain or deliberately cancel obsolete writers, and verify no old deploy
   can run. A delivery Terraform apply must preserve disabled triggers and paused
   schedules in its own planned configuration, using a private maintenance override
   if needed. Reject a plan that transiently enables any held path; re-pausing it
   afterward leaves an unsafe admission gap. Verify the hold after each apply.
   Set existing delivery resources `deploy`, `terraform_apply`, `promote`,
   `terraform_promote`, `release_train` and any other held trigger to `disabled=true`,
   plus `paused=true` on the release-train scheduler. Preserve feature flags:
   `enable_application_triggers=false` deletes the trigger instead of holding it.
2. Coordinate active users/workspaces. Stop new activity and automatic continuations;
   flush and stop active/starting workspaces with the existing product operation.
   Require completed durable saves, not just a `stopping` response. Refuse new
   unflushed/session-harvest loss and do not force-discard work for this migration.
3. Stop daemon and API and wait for their Pods to exit. Inspect all namespaces for
   NFS/PVC consumers, Jobs, CronJobs and reconcilers. HTTP blocking alone is not
   enough: both processes have background writers. Verify application/migrator SQL
   transactions have drained before recording the recovery baseline.
4. While writes remain stopped, record a new SQL backup and manual source-Filestore
   backup as one recovery pair. Preserve their IDs and timestamps. Live SQL stays
   unchanged; the daily filesystem backup alone is not a matched recovery point.
   Do not label that manual backup as an owned `daily` or `initial` backup; it must
   remain outside scheduled retention cleanup if the cutover is abandoned.
5. Recheck mount exports, source read-only status and destination identity. Run:

   ```bash
   rsync -aHc --numeric-ids --delete-delay --fsync --itemize-changes --stats -- \
     /mnt/source/ /mnt/destination/
   rsync -aHcn --numeric-ids --delete --itemize-changes -- \
     /mnt/source/ /mnt/destination/
   ```

   Both commands must exit zero; verification must report no differences. The
   deletion flag only removes stale copies on the new destination. A vanished
   source file in the frozen pass is a failed writer fence, not an ignorable warning.
6. Compare repository catalogs, HEADs and all refs; run `git fsck --full` on copied
   repositories. Distinguish harmless dangling objects from missing/corrupt objects.
   Verify complete/unpruned `workspace_flushes` commits and private refs. For every
   native attachment DB row, verify destination length and SHA-256 at
   `attachments/<org_id>/<first-two-storage_id-characters>/<storage_id>`.
   Keep per-repository/file reports private; publish only aggregate results.

## Switch without upgrading the application

Create `g5-repos-zonal-pv` / `g5-repos-zonal-pvc` for the new export, with Retain
and the same share/subpaths. Use the **exact installed chart and saved values** for
the storage-only Helm change, `--no-hooks`, unchanged image digests and zero API
and daemon replicas. Override only the NFS IP, PV/PVC names and storage size.
Do not use latest-main application/chart changes or run SQL migration hooks as
part of storage cutover. Helm may remove the old static binding objects; this
does not delete the old Filestore. Inspect the rendered diff first: only the
storage binding and deliberate zero replica counts may change; images, subpaths
and other workload settings must remain identical. Refuse an installed chart
that cannot express this change; do not silently replace it with latest-main.
Keep the original binding manifests for pre-start recovery.

The merged map already selects `zonal`. Review/apply only
the corresponding backup source and deployment-trigger selection changes; verify
all generated inputs agree. The full plan must not hide unrelated IAM/cache/DNS
or database changes. These are separate roots: update the backup Workflow in
`infra/<environment>/platform` and the deployment trigger in `infra/qa/delivery`
within the same window, with delivery continuously held. The shared delivery
root may update all three trigger inputs together; retain the global hold so
those desired inputs cannot start deployment on an environment still using HDD. For staging/prod, the
backup target is `module.environment.module.filestore_backup`; QA uses
`module.filestore_backup`. Inspect each saved plan rather than assuming a
target excludes all unrelated dependencies. Read the deployed Workflow's
`SOURCE_INSTANCE`, `SOURCE_SHARE` and `BACKUP_PREFIX`, and the trigger's `_FILESTORE_*` values;
require all of them to identify the new source before restarting writers.
Retire incompatible pending Terraform bundles, which can otherwise change backups back
to HDD. Recheck the Workflow source after subsequent platform releases until the
HDD is retired. Update the Helm release record so the next deploy agrees.

Before starting writers, check:

- New PVC is Bound to the intended PV and claim UID; PV names the exact new NFS
  IP and `/g5_repos`, with Retain. API and daemon templates use this same PVC.
- UID/GID 10001 can read existing repositories/attachments and write a disposable
  test path, which is removed before verification is finalized.
- `scripts/check_filestore_binding.py` passes for the new selection and refuses
  the old selection. This check is read-only; it is not a storage-switch command.

Restart the same API and daemon images at their saved replica counts through a
second storage-only `helm upgrade --no-hooks` with the same installed chart,
new binding and saved replica values. Do not use only `kubectl scale`, which would
leave the Helm release recording zero replicas. Confirm
readiness and run a scoped clone/fetch/push, workspace resume/save, and attachment
read/upload check. Create a new-source backup and verify READY and exact source/
share; daily scheduling and 30-day retention remain unchanged.

## Recovery and completion

Before **either process restarts**, restore the original binding with a
storage-only `helm upgrade --no-hooks` using the saved chart and original values,
keeping both replicas zero. In a private operational checkout, temporarily set this environment's `active`
back to `legacy` without deprovisioning the candidate, and reconcile its platform backup
Workflow and QA delivery trigger under the continuous hold. Verify the old
Workflow source, trigger inputs and PVC/PV/NFS identity all agree; the binding
guard must accept legacy before restoring saved replicas. Restoring only the
mount would leave backups pointed at Zonal and ordinary deploys blocked. If manual
manifest recreation is needed, remove stale `status`, metadata `uid` and
`resourceVersion`, and PV `spec.claimRef` before creation; preserve the Helm
ownership labels/annotations. Never blindly apply raw exported PV/PVC objects.
Once a process starts, background tasks can
change Git, attachments and SQL even while public ingress is closed. From that
point, stop and reconcile new work before any rollback; do not blindly switch
back to HDD or restore an old SQL snapshot.

For a successful Zonal cutover, establish an explicit release boundary: retire all incompatible application and
Terraform executions and record the first guard-containing source revision and
the environment's Zonal-selection revision/bundle. **Do not retry/rebuild an older
application build, even at the same SHA as the currently deployed image**, submit
an old Terraform bundle, or run `helm rollback` to a pre-cutover release after
writer startup. Same-SHA retries were normally allowed before this migration;
they are excluded here because old builds carry old scripts and bypass the guard.
Resume only compatible application source with Zonal trigger inputs and compatible
Terraform bundles. Compatibility means the captured source includes the guard
and its map selects `zonal` for this environment, not merely that a build is newer
than the window. Keep the bundle producer held until the reviewed selection has
been merged against current main and all relevant artifact contents are checked.
An intentional recovery requires the reconciliation above.
Retain the first Zonal Helm revision in the handover. These are operator-enforced
restrictions; the new script cannot prevent a privileged person from replaying
an old saved build. A cluster admission rule alone would not make that replay
safe, because old SQL hooks execute before workload admission.
Do not uninstall the Helm release, delete its bindings, or perform an environment
reset to bypass a refusal: after complete deletion the guard's legitimate first
installation exception can accept stale inputs. The existing QA database-reset
script preserves Deployments/PV/PVC; that destructive database workflow is not
part of this migration and must not run during it.
After pre-start recovery instead, keep using source that understands the retained
candidate resource, with legacy deployment/backup selection; do not release an
older Terraform bundle that would remove that candidate.

Retain HDD and old backups. The backup workflow's exact-source cleanup will not
expire old-instance backups after source selection changes; track their eventual
cleanup separately. Verify full post-cutover Terraform plans against the already merged final map,
account for pending old releases, and require all three environments' bindings,
backup sources and deploy inputs to agree before resuming normal delivery. Restore only controls
that were previously enabled. Remove only temporary migration helper resources.

Record each environment's timings, source/destination identities, recovery backup
IDs, checksum/Git/attachment verification, smoke results, new backup ID, remaining
risks and cleanup inventory. A passing mock test does not replace these live checks.

References: [GCP backup limitations](https://docs.cloud.google.com/filestore/docs/backups-limitations),
[custom performance](https://docs.cloud.google.com/filestore/docs/custom-performance),
[access controls](https://docs.cloud.google.com/filestore/docs/access-control).
