# Lean pipeline IAM audit and remediation

PR [#735](https://github.com/G5Labs/G5-Lean/pull/735) consolidates Davide's
[#739–#745](https://github.com/G5Labs/G5-Lean/issues?q=is%3Aissue+author%3Advd-g5+IAM)
follow-ups. This is a code change, not a cloud apply or a claim that every
identity is isolated. No merge is authorized.

**QA impact:** applying this PR changes worker provisioning, secret synchronization,
image publication and Terraform permissions. Use the ordered rollout below;
an ordinary uncoordinated apply can interrupt those paths. Staging/prod share
the runtime changes. **DNS:** no records, zones, certificates, delegation or DNS
permissions are changed. Existing DNS authority remains with the same runners.

## Evidence and current exposure

The 2026-10-26 audit inspected the rebased source and refreshed direct project IAM
in QA, staging, prod and admin, plus QA state/customer/worker-artifact bucket IAM,
application registry IAM, the legacy PAT's IAM and selected service-account policies.
No secret payloads, state contents or customer objects were read. No impersonation,
live negative test or cloud mutation was performed. Raw policies are private local
evidence and are not committed. Organization/folder inheritance, groups, deny
policies and all transitive administrative paths were not verified.

The earlier [2026-10-22 audit](https://github.com/G5Labs/G5-Lean/blob/93820ac439b61fc49f91e07fa962a95df853e42e/docs/production-pipeline-iam-review.md#live-account-audit-and-follow-up-2026-10-22)
is historical: staging/prod and release accounts have since been provisioned.
QA API project-wide Token Creator is absent from the refreshed direct project
policy; QA planner state writes are conditional on lock-object names. Both facts
are narrower than the original audit, but are not inherited-access proofs.

| Issue / area | Confirmed condition | Remediation in this PR | Remaining limit |
| --- | --- | --- | --- |
| Original #735 image publisher | QA images identity still has Compute Admin, project actAs and Kubernetes Developer | Bootstrap-owned image-publication role; remove VM/attachment/Kubernetes authority | Trusted signing/publishing can still produce new worker images; live build acceptance required |
| Original #735 sensitive storage | Basic Project Viewer has legacy object reads on QA state/customer buckets | Authoritative owner-only legacy reader binding; retain named workload/planner and unrelated roles | Owner/Editor authority and explicit state readers remain |
| #739 host identity | API attaches default Compute SA, also used by GKE nodes | Dedicated QA/staging/prod host SA; explicit backend/chart setting and named host artifact/cache readers | Existing hosts retain old SA until rotation; keep GKE node pulls |
| #740 runtime Compute | API has instance, network and security administrator roles in all three environments | Exact bootstrap-owned operation roles; supported instance/disk/firewall prefix conditions | **Partial:** project-scoped network/subnet/router operations still permit interaction with infrastructure networks |
| #741 customer storage | QA API has bucket-level storage.admin | Object and bucket-metadata access only | ObjectAdmin still permits customer-object mutation; this is compatibility access, not per-tenant isolation |
| #742 secrets | API reads unused PAT; ESO has project accessor in QA/staging/prod | No API direct-GSM grants; ESO enabled-manifest secret allowlist; retire unused PAT | API still receives its required Kubernetes secret values; deletion and sync must be checked at rollout |
| #743 artifact publication | QA images identity can overwrite worker objects and write application registry | Create/read versioned artifacts; narrow latest/ISO mutation; own g5-builders registry | Mutable caches and destination name/metadata listing remain explicit; signing authority remains trusted |
| #744 public access | QA customer bucket is inherited; uniform access is already true | Enforce public-access prevention explicitly; cover QA buckets with tests | No claim about organization policy; other QA state/build buckets already enforce it |
| #745 Terraform runner | QA runner has project secret admin/accessor and project actAs | Payload-free management role, four DB payload reads, named attachments; remove duplicate Kubernetes role | **Partial:** IAM administration and trusted release invocation still permit privilege escalation |

## Accounts and intended scope after application

QA account suffix is `@g5-qa-1ccc.iam.gserviceaccount.com`. Target runtime accounts
live in `g5lean-staging-1ccc` or `g5lean-prod-1ccc`. Named bindings, not display
names, define authority.

| Principal | Scope | Important boundary |
| --- | --- | --- |
| g5-lean-ci | PR cache/build artifacts, build reads/cancellation, pool and logs | No actAs on trusted identities; project build-update authority can still interrupt other builds |
| g5-lean-deploy | QA application/Kubernetes deployment, trusted application/tier publication, release invocation | Trusted main identity; can submit central-release builds |
| g5-lean-images | Worker image operations/signing, worker registry and separate bootstrap-builder publication | No application-registry write, VM administration, actAs or Kubernetes role; versioned objects cannot be overwritten/deleted by its direct publisher grants |
| g5-lean-terraform | QA infrastructure/IAM administration, state access, named workload attachment, four DB reads, release invocation | Trusted administrator; direct-role narrowing does not prevent self-grants |
| g5-lean-terraform-plan | QA metadata plus sensitive state/four DB reads, lock-only state writes, logs | PR execution sees confidential state; not a secret-confidentiality boundary |
| g5-lean-release | Staging/prod infrastructure and application deployment; read-only admin artifacts | Shared privileged identity across both targets; gates are pipeline checks, not IAM separation |
| g5-lean-release-plan | Target metadata/state reads and exact platform/addons lock writes | State includes database passwords; no intended deploy/approve/impersonation grant |
| g5-lean-release-train | Build approval under release-train checks | Project approval authority remains broader than one release queue |
| g5-lean-release-scheduler | Submit train builds, actAs train only | Trusted scheduling input, not a PR identity |
| g5-lean-{env}-api | SQL client, scoped worker provisioning, exact host actAs and artifact/image use, compatible customer-object access | No direct GSM accessor or legacy identity administration; QA retains exact registry-puller token creation |
| g5-lean-{env}-host | Required runtime registry/cache reads and log/metric writes | Separate from node identity, not a per-organization host identity |
| g5-lean-{env}-worker | Existing worker-secret subset and application-bucket read/create | Project environment boundary, not per-workspace bucket isolation |
| g5-lean-{env}-eso | Payload reads for the enabled ExternalSecret manifest references | No project-wide accessor or automatic access to new inventory entries |
| Default Compute account | Existing GKE node duties and needed node image reads | API no longer attaches it to newly created hosts after cutover; old host reads need staged retirement |
| g5-registry-puller | QA named registry read | API token creation remains limited to this account |
| g5-build-log-reader | QA build metadata and dedicated log view | Existing user-managed key is unchanged; build logs may contain sensitive output |
| g5-cb-slack-notifier | Notifier configuration and exact webhook secret | Dedicated Cloud Run identity |
| g5-cb-slack-pubsub-inv | Invoke exact notifier service | Dedicated Pub/Sub OIDC identity |
| Admin infrastructure operator | Admin repositories, buckets and their policies | Central release has reads, not this infrastructure authority |
| Human and Google-managed administrators | Existing project/service operations | Not removed by this PR; inherited effective access is outside the verified evidence |

## Why some issue proposals change

**Compute conditions.** Use Google's supported resource attributes for VM, disk
and firewall names. Network/subnetwork/router name conditions are not supported
by the same interface, and list operations authorize against a parent. Creating
an allowed-name firewall can still affect a Terraform VPC through project-wide
`networks.updatePolicy`; `subnetworks.use` can attach an allowed-name VM there.
Do not mark #740 fully resolved as infrastructure-network isolation. A separate
worker project or privileged network provisioner would be a different design.
See [Google resource attributes](https://docs.cloud.google.com/iam/docs/conditions-attribute-reference)
and [resource-based access](https://docs.cloud.google.com/iam/docs/configuring-resource-based-access).

**Secret consumers.** Lean API/daemon values arrive through ESO Kubernetes
secrets; no direct runtime GSM client was found. The API therefore needs no direct
GSM allowlist entries. ESO derives its grants from enabled native manifest-reference
locals, including worker-only/minimal combinations. Deriving Terraform keys by
parsing an encoded manifest containing unknown database addresses would break
first-apply planning. The obsolete `github-token` has no current consumer and its
planned deletion remains subject to the destruction guard.

**Terraform payload reads.** Removing `secretAccessor` alone would leave payload
access through builtin `secretmanager.admin`. Replace both with the same management
permission set used by target bootstrap, then retain exactly the four database
secret reads required by provider refresh. Named secret and actAs grants must be
installed before broad grants are removed. See [Secret Manager roles](https://docs.cloud.google.com/secret-manager/docs/access-control)
and [the pinned provider's refresh path](https://github.com/hashicorp/terraform-provider-google/blob/v5.45.2/google/services/secretmanager/resource_secret_manager_secret_version.go#L391).

**Root splitting and deny.** The existing trusted-main runner owns project IAM;
it can grant itself broader roles. Giving one orchestrator impersonation of all
root accounts would not isolate it. A separate delivery runner with build submission
and actAs on central release/train would retain other escalation paths. Google
supports deny policies, including custom-role administration, but conditions do
not express arbitrary policy-membership edits; a runner-only deny is insufficient
against principal switching. #745's direct grant reductions fit this PR; a genuine
root trust boundary remains open. See [Google deny policies](https://docs.cloud.google.com/iam/docs/deny-overview).

**Artifact mutability.** Version uploads already use generation-precondition
creation and verify existing hashes on retry. Enforce that boundary in IAM too.
Both `latest.json` and `iso/` are intentionally mutable in QA; the latter supports
concurrent cold cache fills. Do not introduce retention locks or global immutable
tags that would break cache/signature workflows. Bootstrap builder publication
moves to its own registry; deployed application references remain unchanged.

The unchanged gcloud/gsutil upload clients also list the destination to distinguish
objects from folders. Their object-prefix conditions do not authorize bucket-level
listing. A separate custom role grants only `storage.objects.list`: QA images may
list its worker bucket; admin images and deploy may list their respective worker
and app-release buckets. This exposes names and listing metadata throughout those
buckets without broadening payload reads or mutation. QA app-release publication
already has an unconditional objectViewer grant and needs no new permission.
See Google's [gcloud](https://docs.cloud.google.com/storage/docs/access-control/iam-gcloud)
and [gsutil](https://docs.cloud.google.com/storage/docs/access-control/iam-gsutil)
permission requirements. Apply the operator-owned QA role before its foundation
binding; admin artifacts owns its own role and bindings.

## Ordered rollout after review

1. **Before merge**, arrange a QA maintenance window and pause the main deploy,
   worker-image and Terraform-apply triggers (leave PR checks running). Otherwise
   merging can deploy the API's new host identity before its account/grants exist,
   while the simultaneous infrastructure apply stops at the deletion guard.
   Verify no old in-flight deployment can overtake the transition. A delivery-root
   apply re-enables these triggers; defer it until prerequisites are ready, or
   retain an independently verified merge/build-submission freeze through that
   apply and re-pause immediately afterward. Do not rely on re-pausing alone to
   prevent a build already submitted during the apply.
2. Review complete plans from the final commit, back up restricted state/IAM
   inventories, and enumerate intended IAM and unused-secret deletions. Retain
   the Cloud Build deletion guard (`_TF_ALLOW_DESTROY` opt-in); do not accept
   unrelated resource replacement.
   **Intentional secret deletion:** if state contains
   `module.secrets.google_secret_manager_secret.secrets["github-token"]` in QA,
   or `module.environment.module.secrets.google_secret_manager_secret.secrets["github-token"]`
   in staging/prod, its plan must show deletion of that obsolete secret resource.
   Deleting the container also destroys all its versions; those externally seeded
   versions are not separate resources in this Terraform plan. Confirm the live
   environment has no remaining consumer before authorizing that deletion.
   Stop if the plan deletes any other secret or the consumer check differs from
   this audit. This is deliberate retirement, not a state-only removal; do not
   use `state rm` or a forget block to hide the destructive action.
3. Apply an explicitly reviewed additive bootstrap phase for custom roles, then
   install the runner's four database-reader and every named actAs grant while
   broad grants still exist. Existing secret refresh cannot rely on a grant that
   the same platform apply has not created yet. New-version creation dependencies
   do not solve this upgrade ordering by themselves.
4. Provision dedicated host identities and artifact/cache/telemetry grants before
   deploying the API/chart that selects them. Keep GKE node registry reads. Validate
   a new host before removing default-host actAs; coordinate with a maintenance
   window if the infrastructure and API transition cannot be staged. Existing
   hosts must be rotated before their host-only reads are withdrawn. The existing
   QA default-account cache reader is retained for that transition and needs a
   separate reviewed removal after rotation.
5. Apply the remaining reviewed reductions and PAT retirement. The API's new
   object grant is created before the old storage-admin grant is removed, with
   bucket metadata access created first; verify effective IAM propagation too.
   Verify ESO sync,
   legitimate Terraform refresh/plan/apply and named attachments; denied unrelated
   secret reads, state mutation by planners, non-host VM/firewall mutation and
   versioned artifact overwrite/delete; successful image build/publication and
   workspace Start/save/restart/cache/telemetry. Use controlled test resources,
   never destructive probes against customer data or authoritative state.
6. Run the same runtime acceptance in staging before production. Record actual
   propagation and effective-IAM results; source/mock tests do not substitute for
   those checks. Restore the recorded QA trigger states after acceptance. Keep
   issues #740/#745 open for their documented residual boundaries.

No permission weakening, deployment activation, live application or merge is
performed by this audit. Changes are reviewable together in #735; rollout can
still require multiple deliberately ordered plans.

## Validation

Current implementation/test results and Claude Code findings are recorded in the
[existing review](../.g5/pr_ontology_diff_design/production-pipeline/review.md#consolidated-iam-audit--2026-10-26).
All 14 affected Terraform roots/modules validate with 121 mocked tests passing;
39 focused Rust and 274 Python tests pass, including Helm and local ORAS coverage.
Independent review found and verified the upload-listing repair described above.
Claude Code reviewed the consolidated change and follow-ups; confirmed findings
are fixed and the final result is no actionable findings. Hosted CI and live
reduced-permission acceptance remain separate requirements; historical 2026-10-22
evidence is not fresh acceptance.
