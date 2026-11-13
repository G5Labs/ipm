# Lean staging and production setup

**Option B is implemented in this DNS branch.** It includes target DNS API/IAM,
protected child zones, shared managed records and delegation outputs, on top of
#703's QA preservation and #706's release pipeline. No cloud apply, NS publication,
activation or merge has been performed. Obtain two human DNS reviews before
execution and follow the [delegation runbook](../terraform/modules/networking/README.md).

These roots build new, isolated Lean environments. They do not create GCP
projects, change billing, modify Cloudflare DNS or move the existing customer URL.
QA remains the build source and release coordinator. Staging accepts the QA
artifacts after approval; production automatically follows successful staging
acceptance with the same artifacts.

## Inputs requiring an owner

Approved targets are `g5lean-staging-1ccc` and `g5lean-prod-1ccc`, both in
`us-central1`. Production uses `https://app.g5labs.ai`. The target composition gives
each target a Google Cloud DNS child zone, with only its NS delegation published
manually in the Cloudflare parent. Read-only checks on 2026-10-21 verified billing
enabled for both targets
on `billingAccounts/01A302-C6B3E7-EB2BBD`; [billing issue #702 is closed](https://github.com/G5Labs/G5-Lean/issues/702#issuecomment-6326320890).
Application infrastructure is not provisioned. Hostnames and notification destination
are confirmed below; DNS reviews, provider-account configuration and environment
credentials remain setup tasks. Assigning owner names is not a blocker for code
work. Review resource sizing/cost and the concrete execution plan before provisioning.
Each root rejects every project ID except its approved target, including the
other release environment.

| Environment | Application origin | Preview wildcard | Login admission |
| --- | --- | --- | --- |
| Staging | `https://lean.g5labs.ai` | `*.preview.lean.g5labs.ai` | Pending invitation or existing membership |
| Production | `https://app.g5labs.ai` | `*.preview.app.g5labs.ai` | Pending invitation or existing membership |

Set `preview_hostname` to the base without `*.`. App and preview names may change
within their fixed delegated child; moving outside it requires a separate zone/
delegation change. Renames still need coordinated DNS/TLS, callback and old-link
handling. Name selection does not activate routing or satisfy two-human DNS review.

Deployment failures reuse QA's existing Cloud Build notifier and notification
destination, following G5. The reference is `g5-cloudbuild-slack-webhook`; no webhook
value belongs in Terraform variables or Git. Additional availability/backup
monitoring automation is outside this setup scope.

The owner requested Option B implementation as code; see the separate DNS scope
in the [IAM review](../../docs/production-pipeline-iam-review.md). Cloud grants have
not been applied. Parent delegation, manual proxy binding and live acceptance
remain unexecuted. No Cloudflare provider or token is needed; local checks do not
establish a ready environment.

Each environment uses the same JSON-compatible input shape:

```hcl
project_id = "g5lean-staging-1ccc"
region     = "us-central1"
settings = {
  external_access_enabled = false
  app_hostname            = "lean.g5labs.ai"
  preview_hostname        = "preview.lean.g5labs.ai"
}
```

`settings` configures the application/preview hostnames,
CIDRs, GKE sizing, SQL availability and Filestore sizing. The `release-platform`
module defines the supported fields. Keep one reviewed non-secret input object
per environment and supply it to both platform and addons; the release pipeline
uses that same object. Never include credentials in it.

The initial size matches the current Lean QA runtime: a regional GKE pool with
one `c3d-highcpu-16` node **per zone**, SQL 8 vCPU/32 GiB, and 1 TiB BASIC_HDD
Filestore. This is not a minimum-cost estimate. The API requests 14 CPUs, so node
downsizing alone can make deployment unschedulable. SQL is initially ZONAL;
choose REGIONAL explicitly if the reviewed availability/cost decision requires it.
Staging and prod have independent networks and no peering; duplicated RFC1918
defaults are intentional. If peering is introduced, allocate non-overlapping
ranges before creating these resources.
Private bootstrap disables application ingress and uses private nodes; the
existing GKE module still exposes its authenticated control-plane endpoint.
Review `settings.master_authorized_cidr_blocks` for operator/build access before
provisioning; an empty value does not restrict that endpoint to an IP allowlist.

## Bootstrap and apply order

Use the reviewed full stack for this sequence. #703 preserves QA's foundation
zone, platform records, DNS API/runner permissions and IP/TLS identities. Pilot QA
delegation before staging, then production, using the networking runbook.

1. Project creation and billing linkage are complete. A project administrator
   still checks nested virtualization/VM/SQL/Filestore quotas and reviews the IAM
   proposal.
2. Complete the QA hardening and central-identity setup described in the
   [delivery runbook](../qa/delivery/README.md). Keep targets empty and activation
   off until the target runtime identities exist. Both Terraform
   and application promotion use `g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com`
   directly, following the original G5 release-line model. No per-target runner
   impersonation is needed. The train approver and PR CI/planner remain separate.
3. Obtain two human DNS reviews before applying any DNS API/IAM changes. Then
   run the environment's `bootstrap` as the administrator. It enables APIs and
   creates a versioned protected state bucket, then grants the approved target
   access to the already-created central release identity. Secret management uses
   a target custom role without payload reads; the platform grants those only on
   its four database secrets and optional release-check fixture. The bootstrap
   operator needs custom-role management authority; the release does not run this
   root. Bootstrap owns the bucket and keeps local state; protect it outside Git.
   #707 enables `dns.googleapis.com` and grants target `roles/dns.admin` to the
   existing central release account. The planner's built-in metadata-read roles
   cover DNS without adding DNS writes; verify effective access before use.
4. As a trusted setup operator, initialize `platform` with the target bucket and
   `platform` prefix, review its fresh plan and apply. This creates the isolated
   runtime, service identities and data resources, including the protected child
   zone while public ingress is off. Configured app/preview names also prepare
   their DNS/certificates at this stage. Keep delivery targets empty until the
   addons state in the next step is initialized.
5. Seed the target's secrets through a protected administrator workflow, then
   apply `addons`. External Secrets must report Ready before application deploy.
   Its first sync can lag Terraform completion while controller IAM propagates.
   After both state roots exist and planner access is verified, populate delivery
   targets from real outputs. This enables PR plans; release queues remain disabled.
   The optional `settings.enable_release_check_fixture` creates only a
   `g5-release-checks` container; seed its synthetic-check payload separately
   before enabling release acceptance. The fixture stays outside API/worker grants.
6. Run the initial internal deployment with public access disabled. Create the
   migration-owner/test org after fresh Lean database migrations. Internal health
   checks are bootstrap evidence, not completed external acceptance.
7. With DNS API/IAM already reviewed before bootstrap, obtain two-human review
   of the delegation/TLS changes before publication. After Terraform has
   populated the target Google child zone, verify its app, preview and validation
   records directly, then manually publish the child NS delegation in Cloudflare.
   Follow the detailed activation sequence below, including conflicting parent
   records and DNSSEC checks. Preserve apex, marketing, email and unrelated G5
   routing; no Cloudflare provider/API integration is added. Configure
   OAuth/integration callbacks and public URLs for the chosen application origin
   (`https://app.g5labs.ai` in production).
8. Complete staging acceptance and recovery rehearsal before enabling promotion.
   Switch the existing customer URL only in the separate migration cutover.

Example initialization (after replacing the example inputs):

```sh
terraform -chdir=infra/staging/bootstrap init
terraform -chdir=infra/staging/bootstrap plan -var-file=terraform.tfvars
terraform -chdir=infra/staging/platform init -backend-config=backend.hcl
terraform -chdir=infra/staging/platform plan -var-file=terraform.tfvars
```

Use `infra/prod` for production. Backend buckets must match the project ID:
`${project_id}-lean-tfstate`; root prefixes are `platform` and `addons`.
Automated release uses the central release account for both provider and backend
access; do not inject an alternative backend/provider impersonation identity.
PR code does not receive the central release credentials. Its separate dedicated
planner can read target Terraform state, including database passwords, under the
explicitly approved scope. Its target writes are limited to the platform/addons
state lock objects; it cannot apply infrastructure or write DNS.

## Identity and artifact compatibility

Existing immutable application/workspace and concrete worker-host images remain
in QA. The approved destination is the shared admin project. Its resource/IAM
code is implemented; live publishing and consumption have not switched (see
[admin setup](../admin/README.md)).
The target **default Compute service account** is used by both GKE nodes and
API-created worker VMs; granting the separate `g5-lean-ENV-worker` account registry
access is insufficient. The target API also requires image-use permission in the
image-owning project. The central delivery configuration must install only the
reviewed per-repository/image read grants for these exact target identities.

Application secrets, SQL and Filestore are target-local. No original customer
database or state is reused. Git/document content and issue attachment bytes
are on Filestore and must be backed up separately from Cloud SQL. Durable object
storage has versioning and no inherited cache expiry rule. Optional QA fleet caches are not enabled in the
new overlays; runtime correctness must not depend on cross-tenant cache access.

Login admission has no configuration: the API admits a sign-in only for a
pending organization invitation or an existing membership
(`g5.user.admission`). The former `settings.auth_allowed_emails` /
`G5_AUTH_ALLOWED_EMAILS` login allowlist is retired; remove it from any local
input object (Terraform ignores it, and the API logs a warning when the variable
is still set).

Initial seeding remains an explicit setup step. Run the platform seed,
`scripts/seed-prod-org.sh`, once against the new database with the
environment's explicit target variables and `DEFAULT_ADMIN_EMAIL`. It
provisions only the platform operator organization (`operator`, with its docs
repository, docs-only Base, docs mirror and default image policy) and gives
that email a pending operator admin invitation. That person signs in, which
accepts the invitation, and creates tenant organizations, each for a named
owner, from the operator console at `/operator/home` or with
`POST /api/v1/platform/orgs`. Self-serve organization creation does not exist.
Never assign your personal provider credentials to customers.

Both release overlays disable E2E test-token issuance. Staging has no additional
employee-only login policy. Release approvals and acceptance checks remain unchanged.

## External access and acceptance

The #707 target composition creates `lean.g5labs.ai` and `app.g5labs.ai` Google
child zones in their respective existing target platform states. Shared networking
code manages the app A, preview wildcard A and certificate-validation CNAMEs;
Cloudflare retains the parent and receives only manual NS delegations. Every
app/preview/authorization name must belong to the intended child. Preview needs
no separate delegation. QA reuses its existing zone and record addresses.

With `external_access_enabled=false`, platform still creates the protected zone
and prepares DNS/certificate resources for configured hostnames. Empty hostname
configuration creates the zone only. Disabling ingress retains the zone and
configured DNS/TLS resources; explicitly removing/changing names is a separately
reviewed DNS/TLS change. A missing hostname produces `unconfigured.invalid`, never
a fallback to QA or old production. The flag continues to control application
ingress and previews; no additional activation mechanism is introduced.

The application trigger receives the same reviewed settings as Terraform;
existing trigger/build validation rejects caller overrides. G5's release train
sequences the independently selected infrastructure and application candidates.
Later deployments retain these settings; do not patch a one-off live Helm release
to activate it. DNS management does not attach a certificate map or establish
working HTTPS. Keep the existing manual G5 attachment procedure below.

Follow this sequence with **two human DNS reviewers**:

1. Keep `enable_promotion=false` for bootstrap; the weekly train stays inactive
   until full promotion is enabled. Retain its enabled schedule preference.
   After the QA delegation pilot passes, configure the target's app/preview names
   with `external_access_enabled=false`, then apply platform and addons using the
   same settings. Platform prepares the child records and certificates while
   application ingress stays off. Install those same settings in delivery's
   `promotion_targets`; retain G5's infrastructure-first release sequence.
2. Read `terraform output -json dns_delegation` from the target platform root:
   `project_id`, `zone_name`, `dns_name` and `name_servers`. Use the actual NS values;
   do not guess them. Back up restricted state and the complete parent/
   child DNS inventory. Check every assigned Google NS for the intended app A,
   preview wildcard A and validation CNAME answers, plus CAA, DS/DNSSEC and TTLs.
   Then publish the reviewed child NS delegation manually in Cloudflare, coordinating
   any conflicting A/AAAA/CNAME removal and a prepared rollback. Do not assume
   same-name parent records can coexist with NS. Leave apex DNSSEC, marketing,
   email and unrelated records unchanged. Verify Cloudflare referrals and public
   recursive answers, including after observed delegation/negative-cache TTLs.
   Use the [networking runbook](../terraform/modules/networking/README.md) for the
   inventory, delegation and recovery details; no manual child A/CNAME publication
   or Cloudflare API token is required.
3. Wait for `<static-ip-name>-preview-cert` to report `managed.state: ACTIVE`:
   `gcloud certificate-manager certificates describe <certificate-name> --location=global --project=<target-project>`.
4. Set `external_access_enabled=true` in the same reviewed platform/addons and
   delivery configuration. Apply infrastructure first, then deploy the candidate
   through the existing bootstrap mode. Helm creates the GCE ingress and retains
   its ManagedCertificate. In the correct target cluster,
   inspect `kubectl get ingress g5-ingress -n g5 -o yaml`: confirm the app/wildcard
   rules, reserved IP, and `ingress.kubernetes.io/https-target-proxy` annotation.
   Describe that exact global proxy in the target project and verify it belongs
   to this ingress before the manual attachment:
   `gcloud compute target-https-proxies update <controller-proxy-name> --global --project=<target-project> --certificate-map=<static-ip-name>-cert-map`.
5. Verify the proxy's actual `certificateMap`, the app origin and a sample host
   beneath the configured `preview_hostname` using normal trusted HTTPS, then run
   the external acceptance below. Only then enable normal promotion. Bootstrap
   completion is not external acceptance. Recheck/reapply the manual binding if
   GKE recreates the HTTPS proxy; the chart annotation alone does not attach it.

For delegation recovery, coordinate removal of the new child NS records with
restoration of the exported parent records. Keep the Google child records and TLS
resources available while cached referrals expire. Do not delete the child zone,
detach its certificate map or restore Terraform state as a DNS rollback.

The reserved IP names are `g5-lean-staging-ip` and `g5-lean-prod-ip`; their map names
append `-cert-map`. Keep `networking.gke.io/managed-certificates` and the
controller-generated `ingress.gcp.kubernetes.io/pre-shared-cert` annotation.
Removing the controller's certificate source can remove the HTTPS frontend.
See the existing [QA procedure](../qa/README.md#per-preview-wildcard-origins) and
Google's [certificate-map attachment documentation](https://docs.cloud.google.com/certificate-manager/docs/maps).

Addons reads the applied platform state and rejects mismatched target, app/preview
URL, access or admission settings. Apply platform first when changing those values. External
Secrets uses the existing trusted controller's project-local secret reads;
the QA planner's cluster access is not copied. Manifest changes replace existing
`null_resource` instances, so the automatic release's no-replacement check may
require an operator-reviewed apply; do not bypass that check for unattended runs.

After activation, verify the actual HTTPS target-proxy certificate binding,
including behavior after ingress recreation. Test OAuth, Git, WebSockets and
wildcard previews against the configured environment URL. Require login, tenant isolation,
workspace Start/edit/save/commit/restart, agent execution and preview to pass.
A missing test credential or callback configuration is a failure, not a green skip.

Promotion/customer import remains blocked until the owner records:

- SQL automated backups/PITR enabled and a successful isolated database restore.
- Cloud Build failure notifications reaching the existing QA destination.

Follow G5's existing backup and release setup. No new Filestore backup scheduler,
backup service accounts or monitoring system is introduced in this phase.

Record these checks in the release setup review; internal health checks cannot
mark them complete. GKE, database and storage protection is not a backup.
Single-replica API upgrades may briefly interrupt service. Schema changes are not
undone by Helm; use forward recovery unless compatibility and data preservation
have been explicitly verified.

## Release canary setup and chaining

Normal application promotion is:

`QA artifacts → staging deploy → staging smoke → staging agent canary → staging preview canary → production deploy → production canaries`.

The staging build must pass both canaries before it can invoke `g5-promote-prod`.
Missing fixtures/credentials, failed assertions or timeouts fail the build and
block that invocation. Production also verifies that its staging parent build
succeeded and uses the same immutable release manifest. These are application
gates: the weekly train first completes **staging Terraform → production
Terraform**, then approves the staging application. A staging application canary
failure does not undo infrastructure already applied by that train.

Before enabling promotion, provision synthetic fixtures in **each target**:

1. Create dedicated chat and preview test orgs/workspaces, separate from customer
   data. The canary clones each fixture and creates temporary worker hosts in
   `us-central1`; confirm target compute quota and image-read access. Keep the chat
   org on the `g5d-tungsten` image track and the preview org on a compatible image
   track; avoid conflicting workspace image pins. Configure the chat org's model
   endpoint, model and `ANTHROPIC_AUTH_TOKEN` through its protected org settings.
   The release wrapper deliberately excludes developer credentials.
2. Commit a named preview profile, such as `release-canary`, and its small test
   app to the preview fixture's code repository. The profile must launch a server
   reachable on its configured port and serve `/nonce.txt`: generate a nonempty
   value once per server start and return that same value on repeated requests.
   The canary checks five authenticated preview fetches, including the redirect
   to the preview hostname. It does not create or commit this fixture for you.
3. Mint an org-scoped API key for each test org with `workers:admin`,
   `workspaces:read`, `workspaces:write`, `sessions:read`, `sessions:write` and
   `tasks:read`. Each key must access its own source workspace. Store the keys only
   in the target's Secret Manager payload; the central release account reads it,
   while application/worker identities do not receive it.
4. Set `settings.enable_release_check_fixture=true` in the target settings (and
   matching delivery `terraform_settings`), then seed a version of that target's
   `g5-release-checks` secret. Replace every placeholder below with the provisioned
   target-local IDs and keys; do not copy staging credentials into production.

Staging payload, in project `g5lean-staging-1ccc`:

```json
{
  "environment": "staging",
  "base_url": "https://lean.g5labs.ai",
  "chat": {
    "org_id": "<staging-chat-org-uuid>",
    "fixture_workspace_id": "<staging-chat-workspace-uuid>",
    "api_key": "g5k_<staging-chat-key>"
  },
  "preview": {
    "org_id": "<staging-preview-org-uuid>",
    "fixture_workspace_id": "<staging-preview-workspace-uuid>",
    "api_key": "g5k_<staging-preview-key>",
    "profile": "release-canary"
  }
}
```

Production uses the same JSON shape in its separate `g5lean-prod-1ccc` secret,
with `"environment": "prod"`, `"base_url": "https://app.g5labs.ai"`, and its own
org IDs, workspace IDs and keys. In delivery's `promotion_targets`, set each
`release_check_base_url` to its corresponding origin and `release_check_secret`
to `g5-release-checks`; the secret's environment and origin must match exactly.

Bootstrap (`enable_release_bootstrap=true`, `enable_promotion=false`) leaves
external checks off and cannot chain staging into production. After public HTTPS,
fixtures and the manual acceptance above are proven, enable `enable_promotion`:
both target builds then require the canaries, and staging can promote only after
they pass. Keep the weekly schedule preference enabled; it becomes active only
with full promotion configured. Record the first successful staging canary build
before considering setup complete; local contract tests do not prove that the
live fixtures, model provider or preview routing work.

## PR infrastructure plans

After trusted bootstrap initializes both target roots, configure QA delivery's
`promotion_targets` to create the staging/prod PR plan checks. The dedicated
`g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com` identity receives target
metadata reads, state reads and only the two default-workspace lock writes from
bootstrap. Terraform state includes generated DB passwords; enabling these checks
trusts PR code with that content. Existing QA planner access remains unchanged.
See the [delivery setup and plan semantics](../qa/delivery/README.md#pull-request-terraform-checks).
