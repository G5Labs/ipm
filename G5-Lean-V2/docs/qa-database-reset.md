# One-time QA database reset

For the current migrations 039–040 cutover including PRs 186 and 213, use the
[QA reset script and walkthrough](../scripts/reset-qa.md). The procedure below
records the earlier September 22 baseline reset; its old PR numbers and
one-time checksum exception do not apply to the current rollout.

This runbook is for the disposable G5 Lean database in `g5-qa-1ccc` only.
It does not replace the normal SQLx/Helm migration path. Resetting tenant
rows with the original G5 `reset-dev-environment.sh` does not resolve a SQLx
checksum mismatch: that script leaves the schema and migration ledger intact.

## Merge window

1. Pause `g5-deploy-qa` and `g5-packer-worker-main-qa` on the triggers
   themselves; Terraform does not carry a pause knob, so the delivery root
   must not be applied while the pause is in effect (an apply re-enables
   both). For each trigger:

   ```bash
   gcloud beta builds triggers export g5-deploy-qa \
     --project=g5-qa-1ccc --region=us-central1 --destination=g5-deploy-qa.yaml
   # set `disabled: true` in the exported file, then
   gcloud builds triggers import --project=g5-qa-1ccc --region=us-central1 \
     --source=g5-deploy-qa.yaml
   ```

   Do not delete triggers or disable PR CI.
2. Confirm both triggers are disabled and no running/queued builds belong to
   either trigger. Disabling a trigger does not cancel an existing build.
   Keep manual deployment/image runs on hold too.
3. The owners may merge #35, #44, #46 and #47 after their normal review and
   CI requirements. Deployment readiness does not waive failing checks.
   Keep the current QA application and database running during these merges.
4. Rebase the migration-guard PR on the combined main, record its final
   baseline SHA384, rehearse fresh SQLx migration and bootstrap seeding
   twice, and merge the guard. This is the single intentional baseline
   checksum update; freeze the file afterward. Record the exact final main
   SHA as `QA_CUTOVER_SHA`. Keep subsequent main merges on hold for cutover.

Useful verification commands:

```bash
gcloud builds triggers list --project=g5-qa-1ccc --region=us-central1 \
  --format='table(name,disabled)'
gcloud builds list --project=g5-qa-1ccc --region=us-central1 \
  --filter='status=(WORKING,QUEUED)' \
  --format='table(id,status,substitutions.TRIGGER_NAME)'
```

## Cutover

The operator performs these steps once the final combined commit is ready.
QA will be unavailable during the database rebuild and deployment.

1. Record the current Helm revision/image digests, database owner/encoding,
   migration ledger, deployment replicas and bootstrap administrator email.
   Take a fresh on-demand backup of `g5-lean-qa-postgres` and wait for
   `SUCCESSFUL`; retain its ID. Do not rely on the pre-baseline nightly backup.
2. Quiesce all database writers in cluster `g5-lean-qa`, namespace `g5`:
   inspect deployments, jobs, cronjobs, autoscalers and QA worker VMs.
   Currently the writers are `g5-api` and `g5-daemon`, with no worker VMs.
   Scale writers to zero, wait for their pods to stop, and verify no active
   application database sessions remain. Recheck this inventory at cutover.
3. Delete and recreate only Cloud SQL database `g5_lean_qa` in instance
   `g5-lean-qa-postgres`, preserving its recorded encoding/owner settings.
   Keep the instance, project, global database roles, networking, credentials,
   JWT/OAuth secrets and other infrastructure. This removes the old schema
   and `_sqlx_migrations`; it does not patch migration checksums. Inspect any
   QA cache/queue state for old database identifiers before restarting writers.
4. Re-enable both triggers (re-import the exported definitions with
   `disabled: false`, or apply the delivery root) while the main merge hold
   remains in effect. Explicitly run the QA worker image trigger for
   `QA_CUTOVER_SHA`, wait for success, then run the QA deploy trigger for the
   same SHA. Disabled-window pushes must not be assumed to replay. The standard Helm hook bootstraps runtime users and applies the
   final baseline before starting the new application. No old binary may
   resume against the rebuilt schema.
5. Since migration 003 the `orgs` table forces row-level security, so the
   seed's org step runs as the `g5_app` role. Grant the admin/migration user
   that membership once on the rebuilt database (`GRANT g5_app TO g5;` as the
   admin user); the seed then switches role for that step by itself
   (`DB_ORG_ROLE`, default `g5_app`). Reseed using the final commit's
   `scripts/seed-prod-org.sh` with all of
   these explicit settings: `GCP_PROJECT=g5-qa-1ccc`,
   `GCP_REGION=us-central1`, `DB_INSTANCE_NAME=g5-lean-qa-postgres`,
   `DB_NAME=g5_lean_qa`, the verified private `DB_HOST`,
   `K8S_NAMESPACE=g5`, `USE_K8S_PSQL=1`, and the
   recorded `DEFAULT_ADMIN_EMAIL`, plus
   `K8S_CONTEXT=gke_g5-qa-1ccc_us-central1_g5-lean-qa`. Supply a kubeconfig
   containing that context; every Kubernetes call selects it explicitly.
   The script refuses missing target settings and never defaults to prod.
   The platform seed provisions only the platform operator organization
   (`operator`) and a pending operator admin invitation for
   `DEFAULT_ADMIN_EMAIL`; it creates no tenant organization, mints no worker
   registration token and writes no Secret Manager secret. With
   `USE_K8S_PSQL=1` the seed also initialises the operator org's docs
   repository mirror through the API pod (so its Base workspace can be forked
   and cloned), and sets its default workspace image from the deployment's
   promoted catalog (`DEFAULT_BASE_IMAGE_TRACK`, default `g5d-g5dev`).
   Rerunning the seed is safe and changes nothing else.
   The invited operator then signs in at `https://qa.g5labs.ai` and recreates
   the tenant organizations, including `g5labs` (`g5`), from the operator
   console (`/operator/home`) or `POST /api/v1/platform/orgs`, each for a
   named owner. [`scripts/reset-qa.py`](../scripts/reset-qa.md) automates this
   sequence for a full disposable reset.
6. Verify the SQLx ledger matches the exact deployed SQL bytes, migration
   logs are clean, deployments are ready, and HTTPS/login/organization access
   work at `https://qa.g5labs.ai`. Check the bootstrap seed's repeat-run path
   (one operator org, one pending admin invitation, no worker token). Report the deployed commit and results
   before releasing the cutover merge hold.
7. Worker hosts provisioned before the deploy that pins the cutover's worker
   image boot from the previously pinned image. If that image predates the
   runtime module catalog staging (PR #124), the host crash-loops with
   `workspace runtime catalog is absent` and stays `provisioning` without a
   heartbeat. Provision hosts only after `resolve-worker-image` in the deploy
   log reports the cutover image, or delete and re-provision the affected
   hosts; a host holds no state the database does not also hold.

## Verification queries

The private Cloud SQL instance is reachable only from the cluster. Start a
long-lived helper pod and `exec` into it; `kubectl run --rm -i` attaches
unreliably even though the pod schedules within seconds:

```bash
kubectl -n g5 run qa-ro-psql --restart=Never --image=postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b \
  --overrides='{"spec":{"nodeSelector":{"g5/pool":"system"},"containers":[{"name":"psql","image":"postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b","command":["sleep","3600"],"env":[{"name":"DATABASE_URL","valueFrom":{"secretKeyRef":{"name":"g5-api-secrets","key":"G5_ADMIN_DATABASE_URL"}}}]}]}}'
kubectl -n g5 exec -i qa-ro-psql -- sh -c 'psql "$DATABASE_URL" -X -P pager=off -f -' < checks.sql
kubectl -n g5 delete pod qa-ro-psql
```

The admin user has no `BYPASSRLS` and cannot `SET ROLE authz_lookup_definer`,
so tenant tables (`workspaces`, `workspace_vms`, `worker_vm_events`,
`org_invitations`, …) return zero rows until an authorization context is set.
`authz.set_ctx` writes a session row, so it must run before a read-only
transaction:

```sql
SELECT authz.set_ctx((SELECT session_id FROM authz.user_sessions
  WHERE '<org_id>'::uuid = ANY(org_ids) AND expires_at > now()
  ORDER BY refreshed_at DESC LIMIT 1));
BEGIN READ ONLY;
SELECT version, description, success, installed_on FROM _sqlx_migrations ORDER BY version;
SELECT host_id, status, slots_total, last_heartbeat FROM worker_hosts ORDER BY created_at;
SELECT id, org_id, created_at, revoked_at FROM worker_tokens ORDER BY created_at;
SELECT workspace_id, status, host_id, epoch, last_error FROM workspace_vms;
ROLLBACK;
```

`orgs`, `worker_hosts` and `_sqlx_migrations` are readable without the
context. The 2026-09-22 execution of this runbook is recorded in
[qa-reset-record-2026-09-22.md](qa-reset-record-2026-09-22.md).

If a build, migration or seed fails, pause both main triggers again and keep
writers stopped as needed. Do not blindly run Helm rollback: the old release
may be incompatible with the new schema. Fix forward, or restore the saved
database backup together with the recorded old application/worker versions.
No original G5 project or database participates in this procedure.
