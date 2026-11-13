# Production pipeline implementation

## Container image CVE reporting — 2026-10-27

The owner approved enabling production container image scanning and reports,
following legacy G5's nonblocking behavior: vulnerability findings do not prevent
deployment. Implement in a separate PR, obtain sanitized Claude Code review, and
do not merge or apply cloud changes. Highlight effects on shared QA resources.

Agent implementation: enable automatic Artifact Analysis in the current QA
artifact project and the supported admin artifact project. Read reports for the
validated production image digests with the existing central release identity.
Report unknown or incomplete analysis explicitly; scanner errors must not become
clean results or deployment failures. This enables scanning of new Docker pushes
in those projects, including QA CI/cache images, with provider scanning charges.
DNS and the existing staging/canary/provenance gates are unchanged.

The owner clarified that #735 has not been rolled out and authorized changing
#890 or #735 so both work together. Preserve the existing no-merge/no-cloud-apply
boundary and sanitized Claude Code review requirement.

## Option B: delegated application zones (current decision)

The owner selected Option B from the [DNS ownership comparison](https://claude.ai/artifact/7B1hXRZjqez3qCBeFmZQ3T)
and asked to update the design. Cloudflare continues to host the parent
`g5labs.ai` zone, including marketing and company email. Delegate `qa.g5labs.ai`,
`lean.g5labs.ai` and `app.g5labs.ai` to Google Cloud DNS in their respective
Lean projects. Terraform manages application, preview and certificate-validation
records inside those child zones. This supersedes the earlier request to delete
all Cloud DNS resources and the manual Cloudflare A/CNAME publication design.
Keep QA's existing zone, records, DNS API/permissions and IP/TLS identities.

This request changes the design; it does not authorize merge, cloud apply or live
DNS changes. DNS changes still need two human reviewers and prominent QA/DNS
impact notices. Existing customer migration/cutover remains separate. All intent,
design and PR text stays in English.

Agent design interpretation: reuse the existing QA foundation/platform ownership;
create staging/prod zones in their existing platform states, with target API/IAM
in bootstrap. Keep the existing central release and read-only planning identities.
No new root, service account, Cloudflare provider or activation service is needed.

## Option B implementation request

The owner requested changing the implementation to Option B as well. Implement
and review the existing PR stack without merging or executing live cloud changes.
The same two-human DNS review and QA/DNS impact-notice requirements remain.

## Staging hostname rename

The owner requested `lean.g5labs.ai` instead of `lean-staging.g5labs.ai`.
Use `https://lean.g5labs.ai` for staging, `*.preview.lean.g5labs.ai` for
previews and `lean.g5labs.ai` as the delegated child. Update the existing code,
callback guards, examples and documentation. Keep project IDs, Terraform resource
identifiers, QA and production names unchanged. No merge or live DNS/cloud change.

## Staging canary and release-chain verification

The owner requested verifying that a promoted staging application runs its
canaries and that a failed staging canary prevents production deployment.
Check the actual helper against current Lean APIs and verify the existing train
chain. Preserve the accepted G5 infrastructure-first sequence; do not introduce
another approval or release mechanism. Keep changes in the existing PR stack,
review them with Claude Code, and do not merge or execute live deployments.

The shared standalone canary is also available to QA operators. Any compatibility
repairs must be identified as affecting that shared helper; QA's deployment
trigger and DNS configuration remain unchanged.

## Earlier instructions and implementation history

DNS ownership and retirement statements below are historical and superseded by
the Option B decision above. Unrelated release and permission decisions remain.

## Human instructions

- Build Lean staging and production from the existing G5 production approach.
- Use the existing G5 dev release flow.
- App DNS was initially described as Namecheap; the [DNS authority correction](#dns-authority-correction) supersedes that provider assumption. Two-person review means two reviewers, not a limit of two pull requests.
- Only DNS-related changes require two reviewers.
- “can you start? just everything that need my attention, do not merge anything”.

## Accepted plan and implementation interpretation

The English Production Pipeline Implementation Plan produced in this conversation
defines QA as the dev/build origin, approval-gated staging, automatic production
promotion of the same artifacts, and the weekly G5 release train (infrastructure
before application). Keep data/state/secrets separated, allow scheduled brief
deployment interruption, and leave existing customer URL cutover to migration.
Build appropriately scoped changes without a fixed pull-request count. DNS changes
need two human reviewers; this is not a second production deployment approval.

Implementation starts from Lean main 4059d1596. Unlike the earlier inspected
checkout, this baseline has retired Redis, libkrun firmware builds and automatic
Helm rollback. Reuse the current supported runtime and forward-recovery policy.
The user subsequently confirmed `g5lean-staging-1ccc` and `g5lean-prod-1ccc`, both
in `us-central1`, and asked for a billing issue matching G5 #2497. Created
[G5-Lean #702](https://github.com/G5Labs/G5-Lean/issues/702) and assigned `lumost`.
The hostnames and staging access policy are confirmed below. DNS review and cloud
activation remain outstanding; code must remain inactive. Exact target IAM code implementation is now authorized as
recorded below; the earlier automatic-review rejection did not authorize live
grants. No merges.

## Production hostname clarification

The user selected `https://app.g5labs.ai` for the initial Lean production deployment
and supplied the intended separation: the product uses its own configurable origin,
while the bare domain serves marketing. Existing app links should redirect only
after the customer cutover. OAuth/integration callbacks, generated links, cookies
and preview routing must be checked against the new origin. The user initially
clarified: "for the app, it is namecheap". That historical provider assumption,
including the interpretation that Cloudflare concerned only legacy apex routing
and marketing, is superseded by the authority correction below.
This does not approve IAM, activate DNS, or authorize merging changes. The later
hostname decision below resolves naming; the two DNS reviewers remain to be assigned.

## DNS authority correction

On 2026-10-21 read-only checks confirmed `adam.ns.cloudflare.com` and
`paislee.ns.cloudflare.com` as the authoritative nameservers for `g5labs.ai`,
with no separate delegation for QA or the planned Lean application names. The
user accepted Cloudflare for app DNS. Publish the generated app, preview and
certificate-validation records manually in Cloudflare after two human reviews,
using DNS-only (unproxied) records and unflattened validation CNAMEs. This corrects
the provider assumption; it does not authorize record publication or merging.
Keep existing QA, apex and marketing DNS unchanged, and add no DNS provider/API
integration. The [DNS commitment](../../ontology/features/operations.g5.md#g5.quality.delivery.dns)
and shared setup instructions reflect this correction.

## Complete Cloud DNS cleanup in PR #703

The owner requested complete removal of Cloud DNS code in this PR, superseding
the earlier compatibility-only scope. Keep GCP ingress IP and Certificate Manager
resources, use Cloudflare as the authoritative DNS provider, and include the QA
resource/state transition in the reviewable change. No merge or live cloud/DNS
mutation is authorized. DNS changes still require two human reviewers.

The owner requires ontology intent, design records and all PR content to be in
English. Do not include Chinese text or translation-preface wording.

## Confirmed hostnames, alerts and staging access

The current names include the [staging rename](#staging-hostname-rename): staging `lean.g5labs.ai` with
`*.preview.lean.g5labs.ai`; production `app.g5labs.ai` with
`*.preview.app.g5labs.ai`. Existing QA names stay unchanged. These are configuration
choices, not approval to publish records or change live routing; hostnames can be
changed later with coordinated TLS, callbacks and compatibility updates.

The owner selected the existing QA notification destination for deployment,
availability and backup failures. Reuse that destination through protected
configuration; do not expose its webhook or invent a new recipient. Build failure
notification uses the shared QA notifier. The later scope clarification below
excludes additional availability/backup automation.

The original employee-only staging selection is superseded by the
[staging login-policy amendment](#staging-login-policy-amendment).

The owner asked why setup-owner names were needed. They identify who can perform
eventual DNS/provider changes and the two required DNS reviews; they are
not prerequisites to continue code work. Names and provider credentials remain
unset, and no secret values are requested in chat.

## Empty GCP project creation

The user asked why dev, staging and production projects were not visible. The
assistant clarified that Lean uses existing QA as dev and that only local code
had been prepared. Under the existing setup authorization and confirmed project
IDs, created both empty projects on 2026-10-21 under QA's existing organization
`231256058827`: staging `237625147788`, production `177811684595`. Direct reads
confirmed both ACTIVE and billing disabled. Updated billing issue #702 in place.
No application infrastructure, deployment, additional IAM bindings, DNS changes
or merges were performed. Privileged IAM code approval was still pending then.

## Billing linkage completed

Read-only checks on 2026-10-21 verified `billingEnabled=true` with
`billingAccounts/01A302-C6B3E7-EB2BBD` for `g5lean-admin-1ccc`,
`g5lean-staging-1ccc` and `g5lean-prod-1ccc`. Issue #702 is CLOSED and assigned to
`lumost`; its [2026-10-21 20:53:45 UTC comment](https://github.com/G5Labs/G5-Lean/issues/702#issuecomment-6326320890)
confirms all three projects are linked and no application infrastructure is
provisioned. This resolved billing only; the later code-only IAM authorization
below is separate. Resource setup, DNS review and deployment acceptance remain pending.

## Shared artifact project

On 2026-10-21 the user asked whether to create `g5lean-admin-1ccc` for the
shared images, then authorized creation after the assistant recommended that
layout. Create an independent shared artifact project for application/workspace
images, worker VM images and their disk files, and release records. Keep Cloud
Build and release coordination in QA initially, and retain independent environment
state, data and secrets. Only trusted publishers write shared release artifacts;
runtime consumers read the required artifacts. PR CI caches remain separate.
This replaces the earlier choice to keep shared artifact ownership in QA; the
release approval flow and no-merge instruction remain unchanged.

The empty project was created under organization `231256058827`, project number
`543950241992`, and verified ACTIVE with billing disabled. Existing QA artifact
locations are unchanged until the publishing/consumption transition is implemented
and validated. Preserve artifacts referenced by running workspaces. Creation did not itself resolve the then-pending target Terraform IAM approval.

## Admin permission split

On 2026-10-21 the user replied “agree with the permission” to the proposed admin
split: trusted build identities publish artifacts, the central release identity
reads artifacts to deploy staging/prod, and a separate infrastructure management
identity manages admin repositories, storage and IAM. The central release account
does not receive admin project management or artifact write permissions. Scope
publisher grants to the resources they publish; runtime consumers retain required
read/image-use access. Infrastructure management does not run as the release
account or give it impersonation of that manager.

This settled the admin permission split. The separate staging/prod code approval
is recorded below. Neither authorizes live grants, QA artifact cutover, DNS
changes or merges. Admin resources and bindings are now implemented locally as
recorded in the implementation plan, but have not been applied.

## Exact staging/prod IAM code authorization

On 2026-10-21 the assistant asked: “Can I implement the listed permissions as code
only, without applying or merging?”, linking the exact central-account target
roles in `docs/production-pipeline-iam-review.md`. The user answered “yes”.
This authorizes writing and locally validating those grants for
`g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com` only in
`g5lean-staging-1ccc` and `g5lean-prod-1ccc`: the listed service/project IAM
administration roles, plus secret payload access only on the four managed
database secrets and the named synthetic release-check fixture. It supersedes
the previous lack of exact-scope code authorization; it does not authorize live
IAM application, cloud provisioning, deployments or merges. The agreed admin
read-only boundary and QA/DNS impact notices continue to govern.

## Existing release-line IAM baseline

The user directed the implementation to the existing G5 dev-to-production IAM pattern. That
implementation uses a central trusted release account with direct target-project
administration for both infrastructure and application deployment. Adopt that
pattern in Lean instead of the agent's four per-environment release accounts and
target-runner impersonation. Use only roles required by Lean's actual modules;
omit retired Redis and Cloud DNS administration (app DNS is published manually
at the external provider). Preserve Lean's separate PR CI/planner identities,
independent environment state, scoped
database-secret reads, separate train approver and no-merge instruction. This
supplies the baseline for the code implementation; it is not evidence of live
grants or approval to activate deployments.

## QA and DNS impact notices

The user requires a prominent notice whenever changes could affect QA or DNS.
Before a change that could affect QA or DNS, prominently identify each affected
area, the intended effect, material risk and when it takes effect. Distinguish
local code/proposals from cloud-applied behavior, including shared modules,
release IAM/triggers and artifact paths. This is a notification requirement, not
a new approval gate. Existing DNS two-reviewer and no-merge instructions remain.

## Separate DNS pull request

The user requested a separate DNS pull request. Publish the DNS module,
its tests and focused documentation/design as a separate PR. Keep release IAM,
pipeline orchestration and non-DNS infrastructure changes out of it. Retain the
two-human DNS review requirement and prominent QA/DNS impact notices. Publishing
the PR does not authorize a merge or live record changes.

Created draft [G5-Lean #703](https://github.com/G5Labs/G5-Lean/pull/703) from current
main on `codex/lean-external-dns`, commit `8496cb511ad0b0c90ca21a783bcd29c44004b49e`.
The remaining pipeline PR must depend on this DNS change instead of publishing
another copy of it. DNS activation/configuration stays separately reviewed.

## Scope clarification: follow existing G5

The user asked why additional backups were needed and whether G5 production
already has them, then instructed: “do not add anything more, just closely follow
how G5 is doing.” Follow the existing G5 release/operations approach with only
Lean compatibility and the previously agreed project, hostname, artifact and
environment adaptations. Keep G5's existing Cloud SQL backups/PITR and
Cloud Build failure notifications. Remove the proposed Filestore Scheduler/
Workflows, additional monitoring automation and supporting runtime log additions.
The extra Scheduler invocation permission request is withdrawn; the user did not
approve it. Do not replace the removed additions with another new mechanism.
DNS/TLS setup should reuse the existing G5 environment configuration and manual
certificate-map attachment procedure, in a separate two-reviewer DNS change.

Read-only source inspection confirmed G5 enables automated SQL backups/PITR,
with a 03:00 UTC schedule, 30 retained backups and seven-day transaction logs.
The inspected Filestore module has no backup schedule. Live SQL/Filestore reads
in old `g5-prod-1ccc` were denied, so no claim is made about actual backups there.

## Direct G5 port and weekly schedule

The user requested: “can you make it an exact port and turn on weekly shchedule?”
after the comparison identified Lean-only release policies and bundle handling.
Port the original G5 release train and Terraform promotion behavior at
`8458df8190f51a710e6d7c6ee3a199ff6193991d`. Remove the additional exact-app-SHA
infrastructure gate, GCS Terraform bundle protocol and cross-root release lock.
Reuse G5's OCI bundle and staging-to-prod chain, retaining only the existing
Lean project/path/root and provider-version compatibility bindings. Existing QA
deployment protections, separate PR identities and DNS review remain in force.

Enable G5's Thursday 23:00 America/New_York weekly schedule with the Tuesday
America/Los_Angeles cutoff in the release configuration. This supersedes the
earlier default-off weekly preference, but does not authorize merging PRs or
publishing DNS. Read-only inspection found no release/train/target triggers in QA
and its Cloud Scheduler API is disabled. Live scheduling requires deployment of
the reviewed pipeline and target prerequisites; configuration alone is not live
activation. Do not create a schedule pointing at an absent or unmerged trigger.

The owner explicitly approved adding `roles/cloudscheduler.admin` to
`g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com` as code only, to provision
this weekly release job as G5 does. This resolves the specific permission finding
from the Claude Code review; it authorizes no live IAM apply, schedule activation
or merge. It does not restore the excluded backup automation.

## Staging login-policy amendment

The owner asked to remove the staging gate in PR #706 and explicitly clarified
that this means the employee-only login restriction, using the same login rules
as QA and production. Deployment approval and release acceptance remain in scope.
Remove the staging-only strict-login option and exact-domain validation; retain
the existing configurable allowlist and normal invitation/membership behavior.
The later [release login allowlist](#production-login-allowlist--2026-10-23)
configures both staging and production without restoring the strict-login option. QA's existing
allowlist is unchanged. This is a code change only, with no cloud apply, DNS
change or merge.

## Staging and production PR plans

The owner requested: “can you add plan in CI in 706 too?” after the comparison
identified that original G5 has separate staging/prod PR plan triggers and #706
only had their apply-time plans. Implement both PR checks in #706; preserve the
existing release flow, separate PR authority, English records, Claude Code review
and the prohibition on merging or applying cloud changes.

Implementation interpretation: use one dedicated target planner, separate from
QA's current planner and the release account. It needs target Terraform state
reads (including generated database passwords) and lock-only writes; this is not
a secret-confidentiality boundary against PR code. Document that exposure and
verify effective permissions before enabling live checks.

The owner subsequently explicitly authorized the dedicated planner to read
staging/prod Terraform state, including stored database passwords, plus resource/IAM
metadata reads and writes limited to Terraform lock objects: “Authorize these grants
as code only.” This does not authorize deployment permissions, merging or cloud apply.

## Production login allowlist — 2026-10-23

The owner observed that old G5 and QA deny an unlisted personal account, asked
whether Lean production was open, and replied “Yes we need to configure it” to
configuring Lean production's allowlist for a controlled rollout. The owner then
added “also for staging” and approved sanitized Claude Code review. Keep the earlier
no-merge instruction and Claude Code review requirement.

Implementation interpretation: reuse the legacy G5 production repository policy
`jcmmars@gmail.com,@g5labs.ai`, including the shared invitation/membership
exceptions. Current legacy GitHub main confirms the value; direct old-cluster
inspection was denied. Apply through Lean's existing staging/production inputs and retain
it in release-trigger configuration. This does not authorize changing
QA application admission, DNS, memberships or session revocation.

## Superseded QA Terraform builds — 2026-10-23

The owner requested that superseded QA Terraform apply builds finish with a
successful skip, like application deploys, instead of reporting a failure.
Preserve the freshness check before each apply, skip remaining Terraform work
and downstream release publication, and keep genuine errors as failures. This
authorizes a reviewed code change and draft PR, not a merge or cloud apply.

## Account audit fix PR

The owner asked: “can you create a new pr to fix these and list the current audit and fix in the pr message.” This authorizes code changes and a separate PR for the preceding IAM audit, with Claude Code review. The standing no-merge/no-cloud-apply constraint remains.

The implementation interpretation is a focused follow-up on #707: finish image-builder and sensitive-bucket hardening, remove the unused API CI-secret read, validate fallback artifact readers, and distinguish inherited API/planner fixes from live unapplied permissions. Accepted central release, train, runtime and human administrative scope is documented rather than redesigned.

## Consolidated IAM follow-up — 2026-10-26

The owner requested an IAM scope audit of PR #735 and the issues flagged by Davide,
with the needed changes kept in that same PR. The linked scope is #739–#745.
Keep all records in English, obtain Claude Code review, and do not merge or apply
cloud changes. Highlight QA and DNS effects before rollout.

Implementation interpretation: audit each issue's proposed mechanism against
current consumers and provider support, then narrow runtime, publisher, secrets
and Terraform authority without redesigning the accepted trusted-main release
model. Proposed name conditions, root splits and deny policies are not assumed
to provide isolation. Record unsupported boundaries and live acceptance separately.

## Release Slack notifications — 2026-10-28

The owner asked to wire staging/production deploy failures to Slack like QA,
then explicitly requested approval/running notifications for human approvals as
well as release-train approvals. The owner supplied G5's “Deployment cutoff now”
message and asked to add that behavior by inspecting G5 dev's implementation.
Preserve the no-merge instruction and actual Claude Code review requirement.

Implementation interpretation: extend the existing build-event notifier for the
two approval-gated staging triggers; port G5's weekly cutoff prediction using
the existing planner, QA source connection and notification destination. Use a
dedicated read-only cutoff identity, not a deployment or approval identity.
No DNS, application, release-gate or live IAM changes are part of this code change.
