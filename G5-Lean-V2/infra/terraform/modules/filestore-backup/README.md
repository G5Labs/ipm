# Filestore backup automation

Adopt the native backups already running in QA, staging and production into
Terraform ([#1092](https://github.com/G5Labs/G5-Lean/issues/1092)). The Workflow
is copied verbatim from the
[pinned handoff](https://github.com/G5Labs/G5-Lean/blob/1076d9a923e8202794b8bd4f37fdfeec81daf53e/docs/filestore-backup-terraform-handoff/workflow.yaml),
which matched all three live definitions on 2026-11-04. Adoption changes ownership
of the automation, not backup behavior. It does not change SQL, source Filestore
instances, tiers, capacities, application deployments or DNS.

| Environment | Project | Source instance (`us-central1-a`, share `g5_repos`) | Daily UTC | Platform root |
| --- | --- | --- | --- | --- |
| QA | `g5-qa-1ccc` | `g5-lean-qa-repos` | 04:00 | `infra/qa/platform` |
| Staging | `g5lean-staging-1ccc` | `g5-lean-staging-repos` | 04:30 | `infra/staging/platform` |
| Production | `g5lean-prod-1ccc` | `g5-lean-prod-repos` | 05:00 | `infra/prod/platform` |

## Preserved behavior

- Five fixed environment values: `SOURCE_INSTANCE`, `SOURCE_SHARE`,
  `BACKUP_PARENT`, `BACKUP_PREFIX`, `RETENTION_DAYS=30`. Execution arguments
  cannot redirect the source. The source resource-name spelling must match
  `sourceInstance` in the existing backups; the three current projects use IDs.
- Names remain `<instance>-daily-YYYY-MM-DD` in UTC. Same-day reruns reuse that
  name and validate the backup's source, share, ownership and READY state.
- Labels remain `managed-by=g5-filestore-backup` and `backup-kind=daily`.
  Existing owned `initial` backups also remain eligible for retention; unlabeled
  or explicitly manual backups are preserved. No relabeling is needed.
- Cleanup starts only after today's backup is READY. Every list page is collected
  before deletion. A candidate must be READY, belong to the exact source/share,
  have the expected owner and daily/initial kind, and be strictly older than
  **30 full days**, using its complete creation timestamp.
- Each candidate is reread and revalidated immediately before deletion.
  Concurrently removed backups (404) are harmless; a real error stops the run.
  Deletes are sequential and their operations must finish before the next.
- Creation/readiness uses a 24-hour deadline; each deletion operation has a
  24-hour deadline. Transient HTTP reads and idempotent requests retain the
  deployed retry behavior. A failed run never reports successful completion.

The returned result contains today's backup name, READY state, retention and
deleted names. Failed backups are not silently deleted. The next day has its
own deterministic name; a failed same-day backup needs operator investigation.

## Ownership and permissions

| Resource | QA owner | Staging/production owner |
| --- | --- | --- |
| Both custom roles | `infra/qa/bootstrap` (existing local state) | `infra/<env>/bootstrap` through `release-bootstrap` (existing local state) |
| Workflows APIs | QA bootstrap | Target bootstrap |
| Scheduler API | Existing QA delivery root | Target bootstrap |
| Identities, additive IAM bindings, Workflow and Scheduler job | QA platform | Target platform through `release-platform` |
| Individual backups | Workflow, never Terraform | Workflow, never Terraform |

The custom roles match the pinned live definitions exactly, including the
runner's existing log-write permission. The runner has backup create/get/list/
delete, operation get and log write; the scheduler has execution create only.
No `file.instances.get`, instance mutation or restore permission is added.
The temporary QA setup-user role is not reintroduced.

Apply identities need code-defined bootstrap additions: Workflows Editor for
`g5-lean-terraform` in QA; Workflows Editor and Scheduler Admin for the central
`g5-lean-release` identity in each target. QA additionally attaches the two
backup identities through per-account Service Account User grants in the
platform module; the release identity already has target-project attachment.
Planner roles remain unchanged. These apply-identity grants are separate from
the unchanged backup runtime permissions.

## One-time adoption: complete before enabling automatic applies of this change

**Merging normally triggers QA's automatic Terraform apply. Before the user
merges, complete steps 1–5 and the pre-merge verification in step 6 for QA,
keeping automatic Terraform writers paused. Staging and production may follow
after merge while their writers and the release train remain held. Complete
both target handovers and their exact-final-merge-commit plans before resuming
target applies or approving staging, which chains production.**
The provider cannot import `google_workflows_workflow`; the same-name live
Workflow must be replaced once. Do not delete it and then wait for merge/CI.
Existing backup objects are independent and remain intact during the handover.
Use an authorized operator, a reviewed pinned checkout and the existing state.

1. **Coordinate the Terraform writers.** Keep the relevant automatic Terraform
   applies paused and drain any already-running builds for that environment.
   Keep QA paused through adoption and merge so an older checkout cannot
   undo the adopted resources. This is a Terraform maintenance window, not an
   application shutdown. Prepare QA first; repeat for targets after QA execution
   is verified, including after merge if needed. Keep both target Terraform
   writers and the release train held until staging and production bootstrap,
   handovers and final-merge-commit plans are verified. Approving staging can
   chain production, so both targets must be ready first.

2. **Export and compare before any deletion.** Use a private local directory
   (0700, files 0600; never commit it) for state, plans and live definitions.
   Initialize each root against its existing backend/local state and back it up
   before imports. A new empty bootstrap state is not a substitute for the
   administrator's existing state. For each project:

   ```sh
   umask 077
   P=g5-qa-1ccc # repeat with the staging/prod project
   BACKUP_REVIEW_DIR=$(mktemp -d)
   gcloud workflows describe g5-filestore-backup --location=us-central1 --project="$P" --format=json > "$BACKUP_REVIEW_DIR/workflow.json"
   gcloud workflows executions list g5-filestore-backup --location=us-central1 --project="$P" --format=json > "$BACKUP_REVIEW_DIR/executions.json"
   gcloud projects get-iam-policy "$P" --format=json > "$BACKUP_REVIEW_DIR/project-iam.json"
   gcloud scheduler jobs describe g5-filestore-backup --location=us-central1 --project="$P" --format=json > "$BACKUP_REVIEW_DIR/scheduler.json"
   gcloud iam roles describe g5FilestoreBackupRunner --project="$P" --format=json > "$BACKUP_REVIEW_DIR/runner-role.json"
   gcloud iam roles describe g5FilestoreBackupScheduler --project="$P" --format=json > "$BACKUP_REVIEW_DIR/scheduler-role.json"
   gcloud filestore backups list --region=us-central1 --project="$P" --format=json > "$BACKUP_REVIEW_DIR/backups.json"
   ```

   Compare the source, environment values, role permissions, scheduler target,
   identities and schedule against code. Also compare Workflow description,
   labels, `callLogLevel`, `executionHistoryLevel` and `cryptoKeyName`.
   Workflows has no Workflow-resource IAM policy API; its roles are granted at
   [project level or above](https://docs.cloud.google.com/workflows/docs/access-control).
   Compare the relevant project bindings: both backup custom-role grants,
   existing Workflow invoker/editor grants, service-agent grants and apply
   identities. Preserve existing members and full conditions, including any
   resource-name restrictions, at project scope. The project export does not
   include inherited or service-account-level grants; recheck effective
   deploy/attachment permissions separately. Stop on unexplained IAM drift.
   Current metadata is the checked-in description, `managed-by` label,
   `LOG_ERRORS_ONLY`, Basic history and no CMEK. Provider 5.45.2 does not expose
   history level; [Google's default is Basic](https://docs.cloud.google.com/workflows/docs/debug-steps),
   which must be verified after recreation in step 6. Stop on a non-default
   history level or encryption key; this module does not migrate them. Check
   backup names, both owned kinds and exact source/share strings. Stop on
   unexplained drift. Keep the original
   Workflow YAML, environment variables, metadata and service account from the
   export available for immediate recovery. Deleting a Workflow also deletes
   its execution history; export execution summaries and any needed execution
   details first. Recreating the definition does not restore that history.
   A paused/missing scheduler is also drift;
   preserve its original enabled/paused status rather than enabling it blindly.

3. **Import/apply bootstrap first.** In its existing state, import the two
   live custom roles and review a saved plan. Their permissions must not change.
   The only expected additions are API ownership and the documented apply-role
   grants, with no deletions or replacements. Do not apply unrelated drift.

   ```sh
   # From infra/qa/bootstrap, with the existing local bootstrap state:
   terraform import module.filestore_backup_roles.google_project_iam_custom_role.runner projects/g5-qa-1ccc/roles/g5FilestoreBackupRunner
   terraform import module.filestore_backup_roles.google_project_iam_custom_role.scheduler projects/g5-qa-1ccc/roles/g5FilestoreBackupScheduler
   # In staging/prod use module.bootstrap.module.filestore_backup_roles
   # and the corresponding project in both import IDs.
   ```

   Recheck effective deploy/attachment permissions after this apply. Do not
   delete a Workflow to discover whether bootstrap succeeded.

4. **Prepare the platform apply before handover.** Use the existing platform
   backend and the same reviewed variables as the deployment pipeline. Save and
   inspect a plan from the pinned corrected commit. Expected: five imports
   (two identities, two role bindings, scheduler), one Workflow creation and
   QA's two attachment grants. Explain any additional in-place change first;
   reject any deletion/replacement or source-instance/SQL/customer-data change.
   Keep the saved plan/state private. The green PR plan alone cannot detect an
   existing Workflow name collision because it is declared as a new resource.

5. **Controlled handover outside 03:30–06:30 UTC.** With the saved platform
   apply and rollback files ready, pause this backup Scheduler job and prevent
   new manual executions during handover. Let all ACTIVE and QUEUED Workflow
   executions drain, then verify neither state remains before deletion.
   Delete only `g5-filestore-backup` in the selected project and immediately run
   the reviewed platform apply. Do not wait for a merge-triggered build.

   ```sh
   gcloud scheduler jobs pause g5-filestore-backup --location=us-central1 --project="$P"
   gcloud workflows executions list g5-filestore-backup --location=us-central1 --project="$P" --filter='state=ACTIVE OR state=QUEUED'
   # Proceed only if this command succeeds, returns no ACTIVE or QUEUED executions,
   # and the above preconditions hold:
   gcloud workflows delete g5-filestore-backup --location=us-central1 --project="$P"
   # Run the already-reviewed saved platform plan immediately.
   ```

   If deletion fails, do not attempt a conflicting creation. If apply fails,
   inspect its partial state first. If the Workflow is absent, redeploy the
   **exported original** source, service account and environment variables with
   `gcloud workflows deploy --source=... --service-account=... --env-vars-file=...`.
   If Terraform created it, verify/repair that exact resource instead of deleting
   it again. Restore the prior Scheduler status only once a working Workflow is
   confirmed, including metadata and the relevant project IAM bindings. Do not
   reapply the exported whole project policy as rollback: it is comparison
   evidence, and restoring it could overwrite unrelated grants. Repair only
   the specific reviewed binding if needed, preserving its condition.
   Fix state/configuration and make a fresh plan before retrying.

6. **Verify, merge and resume.** Re-describe the created Workflow and compare source,
   configuration and metadata with the export; only expected Terraform provider
   labels may differ. If this post-recreation response omits
   `executionHistoryLevel`, strict metadata verification may require explicitly
   restoring Basic on the created Workflow. Use
   [PATCH](https://docs.cloud.google.com/workflows/docs/reference/rest/v1/projects.locations.workflows/patch)
   with `updateMask=executionHistoryLevel` and
   `{"executionHistoryLevel":"EXECUTION_HISTORY_BASIC"}`, then wait for completion
   and re-describe it. Keep provider 5.45.2 and the created Workflow.
   Re-export project IAM with
   `gcloud projects get-iam-policy "$P" --format=json`
   and compare the relevant bindings with `project-iam.json`; only the reviewed
   apply-identity additions may differ. Confirm unchanged runtime permissions,
   existing members and conditions, and effective service-account attachment.
   Make a fresh plan from the reviewed adoption checkout, including after any
   history-level repair. The adopted backup resources must show no changes;
   review unrelated changes separately before proceeding.
   Before invoking or resuming the job, review the current inventory's exact
   source/share/owner/kind/age-eligible candidates and approve any expirations:
   **this is a normal backup run and may delete real backups older than 30 days,
   not a non-destructive dry run.** For a previously enabled job, restore its
   status, run it once and check the resulting Workflow execution reaches SUCCEEDED
   and today's expected backup is READY. Verify existing initial/daily objects
   still use the recognized labels and no younger/manual/foreign backup was
   deleted. For an originally paused job, leave Scheduler paused and obtain
   explicit operator approval for one manual Workflow execution, applying the
   same execution/backup checks. Without that verification, the handover remains
   unverified and must not unblock merge or that target's applies.

   In QA, these checks complete pre-merge handover verification. The user can
   then merge while Terraform writers remain paused. Prepare a fresh QA plan
   from the **exact final merge commit** (including any newer main changes).
   The adopted backup resources must show no changes; review unrelated changes
   separately before resuming QA's Terraform writer.

   For staging and production, complete these handover checks and a fresh plan
   from the **exact final merge commit** for each target. Both plans must show
   no changes to adopted backup resources before either target's Terraform
   writer or the release train resumes, or staging is approved. Review unrelated
   changes separately. For each originally enabled backup job, verify its next
   scheduled run too; originally paused jobs remain paused.

The import blocks are no-ops after adoption and can later be removed. Runtime
rollback means restoring the saved Workflow definition/configuration; removing
resources from state alone does not reverse a deployed behavior change.
Resource `prevent_destroy` guards do not replace plan review and do not stop
future applies if resource declarations are removed entirely.

## Verification and limits

`scripts/test_filestore_backup_workflow.py` exercises the actual YAML with
synthetic HTTP responses and a virtual clock, including replay, retention,
pagination, candidate rechecks, partial failures and deadlines. It is not the
Cloud Workflows engine or a live destructive-retention test. Terraform tests
verify module/root wiring with mocked providers. The unchanged source had real
successful Scheduler-triggered executions in all three projects on 2026-11-04.
QA's Terraform adoption and backup execution were verified on 2026-11-05; see
the [review record](../../../../.g5/pr_ontology_diff_design/filestore-backup-adoption/review.md).
Staging/production still require operator adoption and verification.

Native Filestore backups are not a verified SQL/Git recovery system. Coordinated
consistency and recovery work remain in #977–980; this PR does not close them.
