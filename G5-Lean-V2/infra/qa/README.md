# G5 Lean QA infrastructure

**QA DNS resources are preserved in code.** This IAM audit changes no zone,
record, DNS permission, delegation or IP/TLS identity. Use the
[delegation runbook](../terraform/modules/networking/README.md#qa-preservation-and-delegation)
for DNS operations; historical setup checks do not establish current live authority.
No merge or live execution is part of this audit.

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


This directory provisions the G5 Lean environment only in `g5-qa-1ccc` and
serves it at `https://qa.g5labs.ai`. The project and region are constants in
each root so a copied tfvars file cannot redirect an apply into an original G5
project.

No root creates a Google Cloud project, links billing, or edits the original G5
projects. Every image QA runs is built by QA's own pipelines into QA's own
registry; it does not use the original shared registry in `g5-admin-1ccc`.
The approved destination for shared Lean artifacts is the new
[`g5lean-admin-1ccc` project](../admin/README.md). The paths below describe the
QA-owned default; the configured artifact owner determines the active publication
and consumption path. Consult the dated [IAM audit](../../docs/production-pipeline-iam-review.md)
for observed deployment evidence rather than treating bootstrap instructions as live status.
Staging/prod promotion is an explicit, disabled-by-default extension: the central
trusted release identity receives direct access to both configured new Lean targets,
and target runtimes receive read access to the required QA artifacts. Data,
secrets, networks and Terraform state remain separate. The PR CI worker receives no
target deployment or release approval access. The dedicated target planner has
separately authorized state reads and lock-only writes; state can contain secrets.

## Isolation boundaries

| Boundary | G5 Lean resource |
| --- | --- |
| Terraform state | `gs://g5-qa-1ccc-lean-tfstate`, separate prefix per root |
| PR CI | QA private worker pool `g5-ci-pool`, CI-only service account, `g5-ci` Artifact Registry repository |
| Deployment | QA deploy service account, `g5` Artifact Registry repository, `g5-lean-qa` GKE cluster |
| Worker images | QA image service account, QA GCE images, QA bucket and signing key |
| Guest kernel/VMM | Pinned platform artifacts built into the worker image |
| Data plane | QA VPC, Cloud SQL, GCS, Filestore, KMS and Secret Manager |
| DNS | Existing QA Cloud DNS zone and Terraform-managed child records; parent NS delegation remains an operator step |
| Source link | Existing QA Cloud Build connection `G5labs-G5-QA` in `us-central1` |

Three administrative boundaries remain shared:

- `g5-qa-1ccc` uses the same Google Cloud billing account as the original
  projects. Runtime IAM, state, networks, registries and data are separate.
- The Google Cloud Build GitHub App installation `136614983` is installed once
  at the `G5Labs` organization and currently has access to all repositories.
  The console-created QA connection, token secret and linked repository are
  project-local; Terraform creates only its triggers. Changing that GitHub App
  installation to selected repositories would also affect the original
  connection and must be coordinated before doing it.
- The parent Cloudflare zone will supply the reviewed `qa.g5labs.ai` NS delegation.
  Terraform retains QA child records in the existing Google zone; parent authority
  changes only during the reviewed switch. Apex/marketing/email stay outside QA automation.

Application OAuth credentials and API keys must be new QA credentials. Do not
copy values from an original G5 Secret Manager project.
The one exception is the Slack incoming webhook in
`g5-cloudbuild-slack-webhook`: it is an operations alert hook, not an
application credential, and holds a copy of dev's webhook for the same
channel. QA owns its copy and no QA identity reads dev's secret, but revoking
dev's webhook in Slack also silences QA's alerts
([failure alerts](#slack-failure-alerts)).

## Roots and order

The roots are deliberately split so bootstrap state, CI identities, the costly
application data plane, cluster add-ons and source triggers can be reviewed and
applied independently.

1. `bootstrap` enables APIs, creates the versioned Terraform state bucket and
   the two identities the Terraform triggers below run as: the read-only
   `g5-lean-terraform-plan` planner and the privileged `g5-lean-terraform`
   runner. Its state remains local; protect `terraform.tfstate` and do not
   delete it. It also owns the custom roles the automated
   roots only bind, including the [Filestore backup roles](../terraform/modules/filestore-backup/README.md).
2. `foundation` creates the isolated CI network/pool, registries, artifact
   buckets, build identities, signing key and the existing QA child DNS zone.
   Its existing resource address and assigned nameservers are retained.
3. `delivery` uses the existing `G5labs-G5-QA` connection and initially creates
   the manual builder bootstrap trigger. Keep `enable_application_triggers = false`
   for this apply.
4. Run `g5-bootstrap-ci-builder`. The worker-image pipeline builds the pinned
   Cloud Hypervisor kernel and initramfs; no separate firmware publication is needed.
5. `platform` creates the GKE/data plane. This is the cost-bearing apply: one
   regional GKE node per zone, Cloud SQL and the 1 TiB minimum Basic HDD
   Filestore instance are all billable.
6. Seed new QA secrets, then apply `addons` to install External Secrets and
   sync the API secret.
7. Verify the complete Google child record set
   and perform the reviewed [parent NS delegation](../terraform/modules/networking/README.md#qa-preservation-and-delegation).
   Retain validation CNAMEs for certificate renewal.
8. Reapply `delivery` with `enable_application_triggers = true`.
   Run `g5-packer-worker-main-qa` once, followed by
   `g5-deploy-qa` once. Later merges to `main` trigger both automatically.
9. After the first deployment has run its database migrations, run the
   platform seed below. It provisions only the platform operator
   organization and invites the first operator. That operator signs in and
   creates tenant organizations, such as G5's own `g5labs` (`g5`), from the
   operator console (`/operator/home`) or `POST /api/v1/platform/orgs`; new
   orgs take the `g5d-g5dev` track through `G5_NEW_ORG_DEFAULT_IMAGE_TRACK`.

Do not retire QA Cloud DNS resources. The former ordered cleanup is superseded
by Option B; preserve the zone, records, DNS API/runner authority and IP/TLS resources.

The deploy trigger is named `g5-deploy-qa`. The freshness guard
(`scripts/check-main-freshness.sh` and `scripts/check_deploy_freshness.py`)
filters Cloud Build history on that exact trigger name as its build-stream
identity, so renaming the trigger and the guard's filter must happen together.
Its substitutions and a build-time guard constrain it to environment `qa` and
project `g5-qa-1ccc`. Downstream promotion is absent unless explicitly enabled
through the delivery root after target acceptance.

Both main-branch triggers fire on every push to `main`, exactly like the
original G5 delivery module; Terraform never disables them. A maintenance
pause (see the [one-time QA database reset runbook](../../docs/qa-database-reset.md))
is applied to the trigger itself with `gcloud beta builds triggers export`,
`disabled: true`, and `gcloud builds triggers import`, and is undone the same
way. Any later apply of the delivery root also re-enables both triggers, so
do not apply that root during a pause. PR CI is never paused.

QA delivery supports building a release or deploying already-built immutable references. By default it ends with rollout verification and the health smoke test. The optional release path publishes immutable provenance and queues an approval-gated staging deployment; successful staging checks then promote those same artifacts to prod. Target configuration and live acceptance are required before enabling it. Helm keeps its forward-only failure behavior. The retired PR1884 authority cutover and its migration handoff are absent; ordinary migrations, the deployment queue, freshness checks and the explicit QA reset remain in force.

Customer userland images are still built and published for organization image tracking. `scripts/canary-tungsten-microvm.sh` remains a standalone operator tool with its own credentials, organization and image inputs; it is not a QA deploy gate. Its retained toolchain checks do not establish that a live canary ran.

### CI worker operating system

Cloud Hypervisor qualification requires KVM, enforced Landlock and AppArmor, and
private cgroup-v2 delegation. The legacy Cloud Build worker (Debian 11 / Docker
20.10) failed the Landlock boot gate and the E2E cgroup check on PR #516. Changing
the build-step container image cannot upgrade its host kernel.

Use the pinned Cloud Build worker release `2026.09` (Debian 13 / Docker 29) for
`g5-ci-pool`, then run the strict VM and browser gates to qualify it. This is a
shared-pool operating-system change; coordinate it before applying. With a recent
Google Cloud CLI:

```bash
gcloud builds worker-pools update g5-ci-pool \
  --project=g5-qa-1ccc --region=us-central1 --worker-release=2026.09
```

The pinned Terraform provider does not expose `workerRelease`; manage this setting
through the Cloud Build API/CLI and verify it after pool changes. An older CLI can
also omit the field from `describe`; use the current REST API to verify it. The
REST update mask is `privatePoolV1Config.workerConfig.workerRelease`, leaving the
machine, disk, network and IAM untouched. Roll back the same field to `legacy` if
other CI lanes regress; the strict VM gates will correctly fail there. Never remove
Landlock or cgroup enforcement to accommodate the old worker. A configured release
is a prerequisite, not evidence that the VM qualification passed.

See Google's [release channels](https://docs.cloud.google.com/build/docs/release-channels)
and [private pool schema](https://docs.cloud.google.com/build/docs/private-pools/private-pool-config-file-schema).

### Worker-image gate on deploys

`g5-deploy-qa` and `g5-packer-worker-main-qa` start together on a push, and
the bake takes longer than the deploy. A push that changes both API code and
worker-image inputs therefore used to deploy the new API against the previous
worker image. The deploy trigger now carries `_WORKER_IMAGE_TRIGGER` and
`_WORKER_IMAGE_WAIT_MINS`; in `resolve-worker-image`,
`scripts/await_worker_image.py` pins the newest READY `g5-worker` image whose
commit is the promoted commit or an ancestor, provided no file changed between
the two matches the `g5-packer-worker-main-qa` trigger's `included_files`. The
check covers the whole range, not just the promoted commit: when two merges
land minutes apart, the first one's bake can still be running (or finish after
the second merge) while the second one's deploy reaches this step. If worker
inputs changed in the range, the deploy waits for that range's bake and pins
its image. If bounded listing rechecks find no bake (for example, a push was
skipped while the trigger was paused), the deploy starts the enabled worker
trigger once at the promoted SHA, logs its build ID and tracks that build
directly. Concurrent deploys may each start a bake. A disabled trigger, failed
start or failed bake fails the gate instead of deploying on the old image;
the promoted commit's own READY image needs no trigger read. Delivery Terraform
grants the deploy identity build-start permission and account-scoped attachment
to the image identity; apply those grants before relying on recovery.
The wait is bounded (default 90 minutes, including recovery) and happens after the
deploy's own image builds, so it mostly overlaps the bake. The gate lists the
trigger's builds by `trigger_id`, which the Cloud Build API filters
server-side; a `substitutions.*` filter would page through the project's whole
build history (deploy 21b20232 timed out that way).

The bake publishes its image without changing the live API. Only the gated
deploy installs `G5_WORKER_IMAGE`, alongside the API image, so a worker-touching
merge causes one API rollout. Standalone bakes need a later compatible deploy
to install their pin; a failed gate leaves the running API untouched
([cloudbuild pipelines D35](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/deployment.g5.md#let-the-deploy-own-the-api-worker-image-pin)).

### Terraform plan and apply triggers

Like dev's `g5-terraform-plan` / `g5-terraform-apply-dev` (module
`cloudbuild`), the delivery root creates two Cloud Build triggers that run
`infra/ci/cloudbuild-terraform-qa.yaml`:

| Trigger | Fires on | Runs as | Does |
| --- | --- | --- | --- |
| `g5-terraform-plan-qa` | pull requests into `main` touching `infra/qa/**` or the pipeline | `g5-lean-terraform-plan` (read-only: viewer, IAM security reviewer, Secret Manager viewer, state-bucket object admin for the lock, plus the reads a refresh of the platform root needs: payload access on the four credential secrets it versions) | `terraform fmt -check`, `validate` and a refreshed `plan` of each remote-state root |
| `g5-terraform-apply-qa` | pushes to `main` touching the same files; no approval | `g5-lean-terraform` (the privileged runner) | per root, a fresh `plan` immediately followed by its `apply` |

The plan trigger builds the pull request's head, so the PR author controls
the steps and the Terraform that run; that is why the planner identity has no
infrastructure/state-payload write grant (only conditional lock writes) and why
the runner is never attached to the plan trigger. State and the four database
secret reads remain sensitive access.

The roots run in dependency order `foundation`, `delivery`, `platform`,
`addons`; `bootstrap` stays a manual, local-state apply. Each root is planned
and applied before the next starts so a downstream `terraform_remote_state`
read sees the upstream apply. The values a laptop apply reads from the
untracked `terraform.tfvars` (`enable_application_triggers`) rides along as a
trigger substitution taken from the delivery
root's own variables, so the CI apply reproduces the last manual apply's
inputs.

Like dev's `g5-terraform-apply-dev`, the apply runs without approval: merging
to `main` authorizes it, so review the plan trigger's output on the PR before
merging.

The [consolidated IAM audit](../../docs/production-pipeline-iam-review.md#ordered-rollout-after-review)
requires a pre-merge maintenance pause of QA deploy, worker-image and Terraform
apply triggers; PR checks remain active. Reapplying the delivery root enables
its managed triggers even if they were manually paused, so the linked rollout
also covers the merge/build-submission freeze.
Apply its permission changes in the documented order.
Create bootstrap-owned custom roles first, install the runner's four database
reads and named service-account attachment grants while its broad grants still
exist, then remove the broad grants. Provision host identities/readers before
deploying the API that selects them and withdrawing default-host actAs; rotate
existing hosts before retiring their reads. Review intended IAM and unused-PAT
deletions under the guard below, stopping on unrelated deletes or bucket
replacement. Verify Terraform refresh, ESO synchronization, image publication and
workspace startup. These changes affect QA IAM but do not change DNS.

Three things to know:

- The apply build makes a fresh plan, not the PR's; it can differ through
  other merges, drift, or an upstream root applied earlier in the same build.
  The pipeline therefore refuses to apply any root whose plan deletes or
  replaces a resource unless the build was started with
  `_TF_ALLOW_DESTROY=true` (default `false`); the refused build prints the
  actions, and an intended destroy is rerun from the trigger with that
  substitution.
- Each push to `main` starts its own build and nothing serializes them. A
  superseded build successfully skips its remaining roots and release publication
  once a newer build of the same trigger exists. Checks run before each root,
  before each changed apply, and immediately before release submission. An
  already-running apply is not interrupted. A skip cancels stale work; it does
  not prove that the newer build successfully applied the skipped roots. A newer
  failed or cancelled build still supersedes old code: fix or rerun current main
  instead of resuming the older build. Lookup and Terraform failures still fail.
  Each root waits up to 10 minutes for the state lock.
- The delivery root re-enables paused triggers (see the maintenance-pause note under "Roots and order"
  above). Include this trigger in a maintenance pause; otherwise a merge
  touching these files applies during the pause and ends it without warning.

The pipeline runs `terraform init -lockfile=readonly` and fails when the
committed `.terraform.lock.hcl` does not satisfy the configuration; commit the
updated lock file rather than letting CI choose provider versions. The build
runs on `linux_amd64`, so a lock file written on a Mac must also carry that
platform's hashes: regenerate with
`terraform providers lock -platform=linux_amd64 -platform=darwin_arm64 -platform=darwin_amd64`
in each root (`platform` and `addons` need `terraform init -backend=false`
first for their local modules). The Terraform archive is pinned by
`_TF_SHA256` next to `_TF_VERSION`; bump both together.

The planner's extra read grants live in two places and both must be applied
before the plan trigger passes on a fresh project: the per-secret accessor
bindings in `platform` and the `external-secrets` namespace Role that lets
Helm read its release Secrets in `addons` (each by the next apply, which runs
on merge, or a manual one by a project owner).

### Slack failure alerts

The delivery root's `slack-notifier.tf` posts `FAILURE`, `INTERNAL_ERROR`,
`TIMEOUT` and `EXPIRED` for every non-PR QA-hosted build trigger, including
staging/prod application and Terraform promotion, the release train and cutoff
notification job. Cancelled and successful builds stay silent. Cloud Build
publishes to `cloud-builds`; Pub/Sub pushes to the internal Cloud Run service
`g5-cloudbuild-slack-notifier`, which posts through the existing Slack webhook.

When the approval-gated staging app or Terraform build reaches `WORKING`, the
same notifier posts “Release job approved & running”, with its recorded approver
(or approval comment), trigger, build link, commit and branch. Both manual and
weekly-train approvals are covered. Automatic production follow-ups do not send
an approval message. Pub/Sub can redeliver events, so duplicate messages are
possible; the notifier does not change approval or deployment decisions.

The watched list is derived from the trigger resources, and
`scripts/test_qa_slack_notifier.py` (Rust lane `image-build-script-contracts`)
fails when a new non-PR trigger is not added to it.

The webhook is operator-owned: Terraform reads the secret container and
grants the notifier access, but the URL never enters Terraform state. Create
it before the first apply of `slack-notifier.tf`:

```bash
gcloud secrets create g5-cloudbuild-slack-webhook --project=g5-qa-1ccc --replication-policy=automatic
printf '%s' "$WEBHOOK_URL" | gcloud secrets versions add g5-cloudbuild-slack-webhook --project=g5-qa-1ccc --data-file=-
```

The notifier reads the webhook only when an instance starts, so
`slack-notifier.tf` pins the secret version (`slack_webhook_version`). To
rotate, add the new URL as a version with the second command above, set
`slack_webhook_version` to its number in a PR, and revoke the old webhook in
Slack only after that merge's apply has rolled the new revision.

The notifier needs `run.googleapis.com` and the runner's `roles/run.admin`
from `bootstrap`. To check delivery end to end, make a build of a watched
manual trigger fail (for example `g5-bootstrap-ci-builder` with an `_IMAGE`
substitution that cannot be pushed), then look for the Slack message and a 2xx
response in the Cloud Run service log.

### Weekly cutoff summary

When the weekly release train is enabled, `g5-deploy-cutoff-notify` runs on
Wednesday at 00:00 America/Los_Angeles. It predicts the next Thursday 23:00
America/New_York release using the same planner as `g5-release-train`. The
message includes the expected commit range and PR links, authors, titles and
merge times; lookup gaps or a still-forming queue are called out explicitly.

The baseline is inferred from successful `g5-promote-prod` builds using their
QA source-build order. A later green skip of an older artifact cannot move it
backward. If incomplete history could change the baseline, the PR list is marked
unavailable. A completed manual deployment of newer source advances the baseline. Clicking Approve alone does not. A notification is a
snapshot: a later deployment does not edit an already-posted message, and the
train makes its final selection at approval time. Deployments outside the normal
production trigger are not inferred from runtime state.

The dedicated `g5-lean-deploy-cutoff` identity has build-viewer/log-writer roles,
access to the existing webhook secret, and G5's project-scoped Cloud Build
read-token role. That last role can read source tokens for any QA connection;
it is not a single-repository IAM boundary. The job requests only the configured
G5-Lean repository. It has no deploy or approval role. The existing scheduler
can act as this reader, separately from the train's approval identity. Both
notifiers use `slack_webhook_version`, including during webhook rotation.

After merge and the normal QA delivery Terraform apply, verify the notifier's
ready revision and the new scheduler/trigger. For a preview without sending Slack:

```bash
gcloud builds triggers run g5-deploy-cutoff-notify \
  --project=g5-qa-1ccc --region=us-central1 --branch=main \
  --substitutions=_DRY_RUN=true
```

The dry run uses real build/GitHub reads and the configured webhook version but
prints only the rendered message, never credentials. G5's cutoff script logs
`NOTIFY-DEPLOY-CUTOFF-FAILED` and exits successfully on notification errors;
inspect that marker as well as the build status. Validate one actual scheduled
message and an approval/running message after apply. Neither a dry run nor a
mock Terraform plan establishes live IAM or Slack delivery.

## Per-preview wildcard origins

Previews default to the legacy `https://qa.g5labs.ai/api/v1/preview/<job-id>/`
path. SPAs that use root-relative assets and navigation (`/assets/...`, `/login`)
escape that prefix and land on the QA app itself (the `/coming-soon.html`
redirect while `G5_MAINTENANCE_MODE` is on), so QA serves each preview from its
own origin, `https://<job-id>.preview.qa.g5labs.ai/`, exactly like dev
(issue #935). The following steps enable it and must run in this order:

1. **Terraform (`platform` root)** — `preview_subdomain_bases =
   ["preview.qa.g5labs.ai"]` creates two Certificate Manager DNS authorizations
   (for `qa.g5labs.ai` and `preview.qa.g5labs.ai`), one
   managed certificate covering `qa.g5labs.ai` + `*.preview.qa.g5labs.ai`, and
   the `g5-lean-qa-ip-cert-map` certificate map. The platform output includes
   the application and wildcard A records and the authorization CNAMEs for
   verification. Terraform manages these records in the existing Google child
   zone. Keep the existing
   ManagedCertificate and HTTPS proxy configuration while adding the map.
2. **Delegated DNS** — verify the child records and
   reviewed Cloudflare NS delegation using the [delegation runbook](../terraform/modules/networking/README.md#qa-preservation-and-delegation).
   Keep the app A, preview wildcard A and both validation CNAMEs in Google DNS.
   They must resolve publicly before certificate validation; retain the CNAMEs
   for renewal. No separate preview delegation is needed.

   ```bash
   gcloud certificate-manager certificates describe g5-lean-qa-ip-preview-cert \
     --project=g5-qa-1ccc --format="value(managed.state,managed.domainStatus)"
   ```

3. **Helm (`values-qa.yaml`)** — only after the certificate reports `ACTIVE`,
   flip `previewWildcard.enabled: true` with `hostSuffix: preview.qa.g5labs.ai`
   and `certMapName: g5-lean-qa-ip-cert-map`. This adds the `*.<suffix>` host
   rule (all paths to `g5-api`), sets `G5_PREVIEW_HOST_SUFFIX` in the API, and
   records the map in a `networking.gke.io/certmap` annotation. The
   ManagedCertificate annotation stays in place. The chart also refuses to
   render with `api.replicaCount > 1` (the preview handoff-ticket store is
   process-local); QA runs one replica.
4. **Attach the map to the HTTPS target proxy (one time, by hand).** The GKE
   Ingress controller does not act on the `certmap` annotation (verified on
   GKE 1.35, 2026-09-24): the proxy keeps serving the `qa.g5labs.ai`-only
   ManagedCertificate and `*.preview.qa.g5labs.ai` fails TLS. Attach the map
   directly; GCE serves a proxy's certificate map in preference to its
   certificate list, and the map covers the apex as well as the wildcard.
   This is how dev runs. The `update` needs
   `compute.targetHttpsProxies.update`, which none of the developer roles on
   `g5-qa-1ccc` carry — grant `roles/compute.loadBalancerAdmin` first
   (`--condition=None`, the project policy has conditional bindings) and
   allow a minute for IAM propagation. The proxy change takes a few minutes
   to reach the edge.

   ```bash
   gcloud projects add-iam-policy-binding g5-qa-1ccc --member=user:<you> \
     --role=roles/compute.loadBalancerAdmin --condition=None
   gcloud compute target-https-proxies list --project=g5-qa-1ccc
   gcloud compute target-https-proxies update <k8s2-ts-...-g5-g5-ingress-...> \
     --global --project=g5-qa-1ccc --certificate-map=g5-lean-qa-ip-cert-map
   echo | openssl s_client -servername test.preview.qa.g5labs.ai -connect qa.g5labs.ai:443 2>/dev/null \
     | openssl x509 -noout -ext subjectAltName   # must list *.preview.qa.g5labs.ai
   ```

   The attachment survives the controller's own certificate updates (dev's
   proxy carries the map next to two rotated ManagedCertificates), but it is
   lost if the controller ever recreates the proxy; re-run the `update` after
   any Ingress change that touches TLS.

   **Never remove `ingress.gcp.kubernetes.io/pre-shared-cert` from the
   Ingress.** The ManagedCertificate controller owns it, and without it the
   Ingress controller sees no TLS source and deletes the HTTPS frontend —
   `https://qa.g5labs.ai` goes down until the annotation is restored
   (`kubectl annotate ingress g5-ingress -n g5
   ingress.gcp.kubernetes.io/pre-shared-cert=<mcrt-id from
   kubectl get managedcertificate -n g5>`). This happened on 2026-09-24.

**Historical preview-feature disablement, not delegation rollback:** detach the map from the proxy
(`gcloud compute target-https-proxies update <proxy> --global
--project=g5-qa-1ccc --clear-certificate-map`), flip
`previewWildcard.enabled` back to `false`, then, if wanted, empty
`preview_subdomain_bases` — that apply deletes the certificate resources and
needs `_TF_ALLOW_DESTROY=true` on the `g5-terraform-apply-qa` build.

## Worker base images in QA

QA builds all worker base images itself. Each `g5-deploy-qa` build compiles `g5d-g5dev`, `g5d-tungsten`, the PairSoft and PNC images into `us-central1-docker.pkg.dev/g5-qa-1ccc/g5` and hands their digests to the API as `G5_PROMOTED_BASE_IMAGES`.
An org that tracks an image name (`default_base_image_track`) is moved to the newest QA-built digest on every API start, so it always runs the image built from the deployed commit. Newly created QA orgs select the `g5d-g5dev` track through `G5_NEW_ORG_DEFAULT_IMAGE_TRACK`. On API startup, existing orgs with neither a track nor a pin adopt that configured default. Explicit pins and tracks are preserved.
Set or change an org's policy with `workers:admin`:

```bash
curl -X PUT https://qa.g5labs.ai/api/v1/orgs/<orgId>/worker-hosts/image-policy \
  -H "Authorization: Bearer $G5_API_KEY" -H 'Content-Type: application/json' \
  -d '{"imageTrack": "g5d-g5dev"}'
```

A `baseImageRef` pin must point into the QA registry; QA hosts hold no
credentials for any other registry, so a foreign reference fails at microVM
launch.

## Commands

Run plans one root at a time and retain each saved plan for review:

```bash
cd infra/qa/bootstrap
terraform init
terraform plan -out=bootstrap.tfplan
terraform apply bootstrap.tfplan

cd ../foundation
terraform init
terraform plan -out=foundation.tfplan
terraform apply foundation.tfplan
```

Verify the console-created QA connection and repository before applying
`delivery`:

```bash
gcloud builds connections describe G5labs-G5-QA \
  --project=g5-qa-1ccc --region=us-central1
gcloud builds repositories describe G5Labs-G5-Lean \
  --connection=G5labs-G5-QA \
  --project=g5-qa-1ccc --region=us-central1
```

Copy `delivery/terraform.tfvars.example` to the gitignored
`delivery/terraform.tfvars`, then plan and apply `delivery`. The existing
connection is authorized as GitHub user `zcl-g5` through the organization-wide
Cloud Build GitHub App installation `136614983`.

```bash
cd ../delivery
cp terraform.tfvars.example terraform.tfvars
# Keep application triggers disabled for the bootstrap apply.
terraform init
terraform plan -out=delivery-bootstrap.tfplan
terraform apply delivery-bootstrap.tfplan

gcloud builds triggers run g5-bootstrap-ci-builder \
  --project=g5-qa-1ccc --region=us-central1 --branch=main
```

After the builder succeeds, build the application data plane only after
reviewing the cost-bearing plan:

```bash
cd ../platform
terraform init
terraform plan -out=platform.tfplan
terraform apply platform.tfplan
```

After `platform` is applied, create a separate GitHub OAuth app with
this callback, export its credentials and the desired QA LLM keys, then run
the seeder:

```text
https://qa.g5labs.ai/api/v1/auth/callback/github
```

```bash
cd ../../..
./scripts/bootstrap-secrets.sh \
  --project g5-qa-1ccc \
  --external-url https://qa.g5labs.ai \
  --license-admin-secret-id g5-license-admin-token-qa
```

Then install the cluster add-ons, verify the Google child record set and
complete step 7's reviewed NS delegation procedure
before enabling the application triggers:

```bash
cd infra/qa/addons
terraform init
terraform plan -out=addons.tfplan
terraform apply addons.tfplan

cd ../delivery
# Set enable_application_triggers=true in terraform.tfvars.
terraform plan -out=delivery-app.tfplan
terraform apply delivery-app.tfplan

gcloud builds triggers run g5-packer-worker-main-qa \
  --project=g5-qa-1ccc --region=us-central1 --branch=main
gcloud builds triggers run g5-deploy-qa \
  --project=g5-qa-1ccc --region=us-central1 --branch=main
```

After the deploy succeeds, connect `kubectl` to the QA cluster and run the
platform seed. It provisions only the platform operator organization
(`operator`): its docs repository, docs-only Base workspace, the docs
repository mirror (initialised through the API pod) and the org's default
workspace image from the deployment's promoted image catalog
(`DEFAULT_BASE_IMAGE_TRACK`, default `g5d-g5dev`). Set `DEFAULT_ADMIN_EMAIL`
to the first QA operator; the seed gives that address a pending operator
admin invitation. It creates no tenant organization, mints no worker
registration token and writes no Secret Manager secret. The seed is
idempotent: re-running it changes nothing else, and re-invites
`DEFAULT_ADMIN_EMAIL` if the operator org lost its last admin.

The invited operator then signs in at `https://qa.g5labs.ai`, which accepts the
invitation, and creates each tenant organization for a named owner from the
operator console (`/operator/home`) or with `POST /api/v1/platform/orgs`.
Signed-in people cannot create organizations themselves. For a full
disposable reset that also recreates the `g5` org and its G5-Lean Base code
repository, use [`scripts/reset-qa.py`](../../scripts/reset-qa.md).

The seed requires an explicit project, database instance, database name, host
(or `--proxy`), and administrator email. In-cluster mode also requires the
named GKE context below; it never uses the current context implicitly.

```bash
cd "$(git rev-parse --show-toplevel)"
gcloud container clusters get-credentials g5-lean-qa \
  --location=us-central1 --project=g5-qa-1ccc

QA_DATABASE_IP="$(terraform -chdir=infra/qa/platform output -json application \
  | jq -r .database_ip)"
GCP_PROJECT=g5-qa-1ccc \
GCP_REGION=us-central1 \
DB_INSTANCE_NAME=g5-lean-qa-postgres \
DB_HOST="$QA_DATABASE_IP" \
DB_NAME=g5_lean_qa \
K8S_NAMESPACE=g5 \
K8S_CONTEXT=gke_g5-qa-1ccc_us-central1_g5-lean-qa \
USE_K8S_PSQL=1 \
DEFAULT_ADMIN_EMAIL=you@g5labs.ai \
  ./scripts/seed-prod-org.sh
unset QA_DATABASE_IP
```

### G5 GitHub App (external issues)

External issues (organization GitHub App bindings, issue search and links,
comments) use a G5 GitHub App. Its credentials live in seven
`g5-github-app-*` Secret Manager containers created by `platform` with inert
blank versions, and reach g5-api and the daemon through the optional
`g5-github-app-secrets` ExternalSecret. With every value blank the App is not
configured; set all seven together (a partial set makes g5-api report the
missing settings). Whether an organization uses external issues is its own
setting in G5, not a secret.

Register the App on GitHub with:

- Callback URL: `https://qa.g5labs.ai/api/v1/github-app/oauth/callback`
- Setup URL: `https://qa.g5labs.ai/api/v1/github-app/setup`, with "Redirect on
  update" on
- "Request user authorization (OAuth) during installation" off; G5 starts it
- Repository permissions: Metadata read, Issues read and write, Contents read
  and write, Pull requests read and write, Checks read, Commit statuses read,
  and Workflows read and write when G5 may publish changes to
  `.github/workflows/` (see below)
- Webhooks inactive
- A client secret (Settings -> "Generate a new client secret")

Then run `scripts/github-app-secrets.sh` from the repository root (any shell;
the script runs under bash). Each step is a separate command. `seed`,
`restart` and `rollback` print what they will do and wait for `yes` before
writing. `sync` does not ask: it only annotates the ExternalSecret, after which
External Secrets updates the `g5-github-app-secrets` Kubernetes Secret, which
running pods do not read until they restart. The script never prints a value, passes one on a command line or reads one back
from Secret Manager. kubectl runs through a temporary kubeconfig, so the
current context is unchanged. The target is fixed: `g5-qa-1ccc`, cluster
`g5-lean-qa`, namespace `g5`.

```bash
# 1. Validate the values as g5-api does, then add one version per container
#    (needs secretmanager.versions.add). --dry-run validates without writing.
scripts/github-app-secrets.sh seed --dry-run /path/to/app.private-key.pem
scripts/github-app-secrets.sh seed /path/to/app.private-key.pem
# 2. Refresh the ExternalSecret, wait for a new successful sync, and check the
#    seven values are all set (names and lengths only). A partial set fails.
scripts/github-app-secrets.sh sync
# 3. Recheck, restart g5-api and g5-daemon, wait for both rollouts, and
#    confirm instance/info offers github_app. Both use the Recreate strategy,
#    so the API is down while its pod restarts. The next g5-deploy-qa also
#    restarts both.
scripts/github-app-secrets.sh restart
```

Pods read the values only at start, so steps 1 and 2 change nothing that is
running until a pod next starts for any reason. A partial or invalid set
cannot stop g5-api: it logs `GitHub App configuration is incomplete or
invalid` and leaves the App off. To turn the App off again,
`scripts/github-app-secrets.sh rollback` adds a blank version to all seven;
`sync` and `restart` then accept the all-blank set, and `restart` confirms
instance/info no longer offers `github_app`. Do not disable or destroy the
latest version instead: the ExternalSecret reads `latest`, and a disabled
latest version fails the sync.

GitHub refuses any App push or merge whose new commits create or update a
file under `.github/workflows/` unless the token has Workflows write. G5
requests that permission only when the organization's binding records it, so
ordinary publication works without it. To allow workflow changes: add
"Workflows: Read and write" to the App registration, have the installation
owner approve the updated permissions on each installation, then choose
"Refresh from GitHub" in the organization's GitHub App settings in G5 so the
binding records the grant. Until then such publication fails with
`PROVIDER_PERMISSION_REQUIRED` and the refused branch is not updated.

`GET https://qa.g5labs.ai/api/v1/instance/info` lists `github_app` and
`external_issues` once the App is configured. An organization admin then binds
the App and installs it on the issue repositories in organization settings.

`terraform plan` is read-only. Every `terraform apply`, manual Cloud Build run,
DNS publication and first application deploy changes external infrastructure and
should be executed only after its saved plan or inputs have been reviewed.
