# Lean production

**Option B target DNS is implemented in this branch.** #707 supplies the target
bootstrap API/IAM, protected platform child zone, shared records and delegation
output; #703 preserves QA. No cloud apply, parent NS change, activation or merge
has been performed. Two human DNS reviews and external acceptance remain required.

Follow the shared [staging and production setup](../staging/README.md), using
`infra/prod` roots and a separate new production project/input object.
The approved project `g5lean-prod-1ccc` exists and billing is enabled as of the
2026-10-21 read-only check. No cloud grants or application infrastructure have
been applied. Public activation uses the shared delegated
DNS procedure with G5's manual certificate-map attachment. Parent NS delegation,
proxy attachment and external acceptance remain unexecuted.

Production receives the same immutable application and worker artifacts accepted
by staging. It has independent state, SQL, Filestore, object storage and secrets.
Production uses `https://app.g5labs.ai`, including its initial validation deployment.
The `app.g5labs.ai` Google child zone and its app/preview/validation
records belong to the existing production platform state. Cloudflare keeps
`g5labs.ai`, marketing and email; publish only the child's reviewed NS delegation
there. No Cloudflare provider or API token is required. QA's existing Google zone
is retained and piloted first, followed by staging, then production.
Supply the hostname through `settings.app_hostname`
and derive `G5_EXTERNAL_URL`, ingress and provider callbacks from that environment
configuration; do not bake a QA origin into the shared frontend image.

Configured hostnames prepare DNS/certificates while ingress stays disabled. After
reviewed delegation and ACTIVE certificates, enable public access for deployment
and manual proxy-map attachment. Promotion remains disabled until external
acceptance passes. Existing bare-domain app routes remain on old
G5 until the separately reviewed customer migration. The later marketing/Cloudflare
cutover must preserve browser deep links and account for API clients, callbacks,
webhooks and WebSockets separately; a blanket 301 is not a compatible API cutover.

## Production login admission

Sign-in is admitted only for a pending organization invitation or an existing
membership; there is no login allowlist to configure. The former
`settings.auth_allowed_emails` input is retired: remove it from the production
platform/addons inputs and the production entry in QA delivery's
`promotion_targets`. When the API still sees `G5_AUTH_ALLOWED_EMAILS`, it logs a
startup warning and ignores it.

The first operator comes from the platform seed (`scripts/seed-prod-org.sh` with
`DEFAULT_ADMIN_EMAIL`), which provisions only the operator organization and that
person's pending operator admin invitation. Operators then create tenant
organizations for named owners from `/operator/home` or
`POST /api/v1/platform/orgs`, and organization admins invite everyone else.

## Application hostname setup

The audited Lean backend already supports this origin through configuration.
Addons configures the external URL; complete the provider registrations and
DNS/TLS bindings before activation:

| Setting or registration | Production value |
| --- | --- |
| `G5_EXTERNAL_URL`, effective browser CORS origin | `https://app.g5labs.ai` |
| GitHub login OAuth callback | `https://app.g5labs.ai/api/v1/auth/callback/github` |
| Separate GitHub App OAuth callback secret/registration | `https://app.g5labs.ai/api/v1/github-app/oauth/callback` |
| GitHub App setup callback | `https://app.g5labs.ai/api/v1/github-app/setup` |
| GitLab connection callback, if enabled | `https://app.g5labs.ai/api/v1/scm/gitlab/callback` |

Keep secure, host-only cookies; do not widen them to `.g5labs.ai`. Keep
`VITE_API_URL` unset in the shared frontend image so REST and WebSockets stay on
the current application origin. The approved preview wildcard is
`*.preview.app.g5labs.ai`, configured as `preview_hostname = "preview.app.g5labs.ai"`.
Its certificate and Google child records use the shared configuration; preview
needs no separate delegation. The zone and configured DNS/certificates survive
ingress disablement. Names may change within `app.g5labs.ai`; removing them or
moving outside that child requires a reviewed DNS/TLS change. Read actual NS values
from platform's `dns_delegation` output. Follow the shared two-human
DNS review and [delegation runbook](../terraform/modules/networking/README.md),
including conflict/DNSSEC checks and recovery. Keep G5's manual certificate-map
attachment and controller TLS annotations, then verify the actual proxy binding,
worker connectivity, clone URLs, callbacks and preview Host/Origin handling.

The React `/` route now enters the existing login/authenticated-home flow.
Operator secret helpers require explicit production project and external URL;
see [environment secret setup](../../scripts/secret-setup.README.md). Provider
registrations and actual credential setup remain operator tasks.

## Container image CVE reports

Production's `report-image-cves` build step reads Artifact Analysis reports for
the exact API, edge and supplied g5d runtime digests. It runs alongside deployment,
without gating Helm or promotion. HIGH/CRITICAL findings, unavailable reports and
scanner errors are informational; existing provenance, freshness and canary
failures still block as before. Read this step's logs for counts, significant
findings and links to the full image reports. Pending/incomplete is not clean.

To inspect a report, open the production build in Cloud Build and select
`report-image-cves`, then follow its image link. Alternatively, open Artifact
Registry in the artifact project, choose `g5` → image → the deployed digest →
**Vulnerabilities & Malicious Packages**. The console shows refreshed results;
the build log remains the observation made during that release. Viewers need
Artifact Registry image read and Container Analysis occurrence read access.

Scanning belongs to the **artifact project**, currently `g5-qa-1ccc`, rather than
the production runtime project. The supported `g5lean-admin-1ccc` artifact roots
carry the same setup for a later registry transition; this change does not switch
artifact ownership.

Activation after review (not performed by this PR):

**Coordinate with #735's IAM rollout first.** The full-root sequence below is
valid only when #735 is absent from both the selected source and live environment,
or its IAM transition is complete and verified. Merged does not mean applied.
If #735 is present in the selected source or live environment but pending or
partly applied, follow its
[ordered rollout](https://github.com/G5Labs/G5-Lean/blob/5b9477cc4c1472b3859639ed3ef3eda270b5e6b0/docs/production-pipeline-iam-review.md#ordered-rollout-after-review)
instead of applying all of bootstrap first. The scan APIs can join its reviewed
additive bootstrap phase; add occurrence reads in its coordinated delivery/admin
artifacts phase. Install the runner's named actAs and four database-reader grants
before removing broad grants, and prepare host identities before deploying their
consumers. Preserve the QA maintenance pause and exclude older in-flight builds.
Delivery applies re-enable managed triggers: defer them until prerequisites are
ready, or retain the independently verified merge/build-submission freeze through
the apply and re-pause immediately afterward. Re-pausing alone cannot prevent
builds already submitted during the apply.

Always plan from current reviewed source with existing state/inputs, preserving
both changes once included. Do not apply an older PR checkout over the other
change's deployed resources.

1. Using each existing root's state and inputs, review/apply QA bootstrap, then
   QA delivery. Bootstrap enables Container Scanning/Analysis; delivery grants
   `g5-lean-release` project-wide `roles/containeranalysis.occurrences.viewer`.
   This reads all occurrence kinds, including build provenance, not only CVEs;
   it adds no scan administration or findings-write permission. Apply admin
   bootstrap/artifacts separately as the existing infrastructure operator before
   promoting admin-hosted images. Bootstrap is not part of normal QA self-apply.
2. Confirm the selected project's Docker repositories report scanning enabled:
   `gcloud artifacts repositories describe g5 --project=ARTIFACT_PROJECT --location=us-central1 --format='yaml(vulnerabilityScanningConfig)'`.
   The APIs enable scanning by default; a repository explicitly disabled by an
   operator must be enabled separately. Repeat for other repositories in scope.
3. Push a new candidate and inspect its digest's scan status in Artifact Registry.
   Enabling the API does not backfill every existing image. Verify that the
   release account can read both discovery and vulnerability occurrences.
4. If #735 is present in the selected source or live environment and its
   staging/prod transition is not complete and verified, perform this check
   as part of that PR's ordered
   staging-to-production rollout, or wait for its completion. Do not use a
   standalone application promotion or Terraform-bundle apply to bypass that
   transition. Promote a newly built candidate from current reviewed source
   containing all applicable changes; do not resume pre-transition application
   builds or Terraform bundles. Check the production
   report step and unchanged application/canary results. Check timeout frequency
   on large runtime images. For image-index digests, inspect per-platform manifests
   in the console; an index without discovery remains `INCOMPLETE`. Candidates pinned to
   older source do not acquire the new report step automatically. A pending report
   remains available for later inspection through the printed console link.

**QA impact and cost:** automatic scanning covers new Docker pushes throughout
these artifact projects, including QA CI/cache images. Provider scan charges
apply; DNS, QA application configuration and release gates are unchanged. See
[Google's automatic scanning behavior](https://docs.cloud.google.com/artifact-analysis/docs/enable-automatic-scanning)
and [pricing](https://cloud.google.com/artifact-analysis/pricing).

Coverage excludes the GCE worker boot disk and is limited to packages recognized
by Artifact Analysis. Keep the existing Rust/npm dependency checks. A report is
not evidence that an image is free of all vulnerabilities. Removing the reporting
step does not disable scanning or its charges; API resources deliberately retain
`disable_on_destroy = false`. Disabling scanning is a separate operator decision.
