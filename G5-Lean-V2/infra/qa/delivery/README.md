# QA-coordinated staging and production releases

**The release flow is unchanged by Option B.** #703 preserves QA DNS; #707 adds
target bootstrap DNS API/IAM and platform zones/shared records. No cloud apply,
NS publication, activation or merge has been performed. Complete the reviews and
operational checks below before enabling the release line.

QA builds application/runtime artifacts once. A successful QA deployment queues
staging for approval; accepted staging deploys the same immutable references to
production without a second approval. The weekly train uses the original G5
cadence: Thursday 23:00 America/New_York, with a cutoff at the end of Tuesday
America/Los_Angeles. Target state, data and secrets remain independent.

## Prepare identities before enabling releases

#703 preserves the existing QA zone, shared-networking record addresses, DNS API
and runner authority, IP and TLS identities. Do not retire resources or remove
state. Keep the hardening below, which is separate from DNS.
The [DNS runbook](../../terraform/modules/networking/README.md) defines the
reviewed QA-first delegation plan; refreshed QA plans must show no DNS/IP/TLS
deletion or replacement.

Keep `promotion_targets = {}` through the initial hardening deployment. Before
creating **any** release identity (even disabled triggers):

1. Apply the reviewed QA bootstrap change that removes the PR Terraform planner's
   unrestricted state objectAdmin binding. Verify its effective state access is
   objectViewer plus `.tflock`-only objectAdmin, with no inherited state-write role.
   The same bootstrap grants `roles/cloudscheduler.admin` to the trusted QA
   Terraform runner so delivery can provision the requested weekly job, as in G5.
2. Apply QA platform with legacy API identity administration disabled. Verify the
   QA API no longer holds project-wide serviceAccountTokenCreator or an equivalent
   ability to impersonate release identities. Keep only its specific puller grant.
   Review the three legacy IAM binding deletions first. The QA destroy guard
   requires an explicitly reviewed one-time `_TF_ALLOW_DESTROY=true` invocation
   for this change; keep the guard enabled for ordinary runs.
3. Verify PR CI has no actAs/tokenCreator/approver access to the new trusted
   release identities and cannot write the dedicated release bucket.
4. Apply QA delivery with `qa_release_authority_reviewed = true`,
   `promotion_targets = {}` and setup/promotion disabled. Keep the requested
   `enable_release_train=true`; no train exists until promotion and both targets exist. This creates the
   central `g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com` identity, dedicated
   `g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com` planner and release
   bucket before any target service accounts exist. It creates no target triggers,
   no target reader bindings and no downstream queues or scheduler.
5. Bootstrap `g5lean-staging-1ccc` and `g5lean-prod-1ccc`, both `us-central1`,
   using the reviewed project/billing/IAM setup. Target bootstrap code grants
   direct infrastructure/deployment authority to that existing central
   identity, following the original G5 dev release-line model. Separate DNS PR
   #707 enables the DNS API and adds target `roles/dns.admin` to the same central
   account under the Option B implementation request. Obtain two human DNS
   reviews before applying those additions. Bootstrap also grants the dedicated target
   planner resource/IAM metadata reads and state reads with
   writes limited to `platform/default.tflock` and `addons/default.tflock`.
   State contains generated database passwords; review that PR-code exposure and
   verify read/lock access and denied state writes before enabling the checks.
   No target Terraform
   account is impersonated. The separate train approves queues; PR CI and the PR
   planner cannot use the release identity.
6. A trusted operator performs the initial target `platform` then `addons` setup.
   This creates the actual API/node/runtime service accounts and initializes new
   backend state; the read-only PR planner cannot initialize those state objects.
   With #707, each target platform also owns its protected child zone and uses
   shared networking records. Configured names prepare DNS/certificates while
   ingress is off; disabling ingress retains them.
   Populate `promotion_targets` from those outputs, including real artifact-reader
   member strings, then apply delivery again while setup/promotion remain
   disabled. Release triggers are created disabled; the two target PR plan checks
   are enabled independently once these initialized targets are configured. Target artifact reads can now be
   granted to principals that exist. Only then proceed to Setup mode below.

The existing QA automatic apply order is foundation → delivery → platform →
addons. Consequently, steps 1–2 must finish **before** delivery is applied with
`qa_release_authority_reviewed=true`; combining central identity creation with
later platform hardening in a single automatic apply is unsafe. A local plan or mock test is not proof that
historical live IAM bindings were removed.

## Activation states

| State | Configuration | Behavior |
| --- | --- | --- |
| Off | Setup/promotion disabled; weekly preference enabled | Target triggers disabled; no queues or scheduler until both targets and promotion are enabled. |
| Setup | `enable_release_bootstrap=true`, `enable_promotion=false` | QA queues staging for manual setup. Infrastructure chains staging to prod; app setup does not automatically chain to prod. No weekly approvals. |
| Release | `enable_promotion=true`, `enable_release_train=true` (default) | G5's weekly train approves staging infrastructure, waits for staging/prod applies, then approves the independently selected app candidate. Staging checks precede automatic production app deployment. |
| Manual only | Release with `enable_release_train=false` | The same approval queues are operated manually without a weekly schedule. |

The weekly preference is enabled as requested. Live QA currently has no release
triggers and Cloud Scheduler is disabled; deploying the release configuration and
its prerequisites is still required. This code change does not create a live job.

Populate `g5-release-checks` in each target with the required disposable test
fixtures before Release mode. Establish login, workspace save/restart, agent,
preview, tenant isolation and recovery acceptance separately. After two human DNS
reviews, pilot QA delegation, then staging, then production: prepopulate Google
child records with ingress off and verify every assigned nameserver before the
manual Cloudflare parent NS switch. Wait for ACTIVE certificates, then deploy
public ingress and attach its certificate map. Use the
[DNS runbook](../../terraform/modules/networking/README.md)
for parent-record conflicts, DNSSEC, recovery and existing manual TLS-map attachment.
Keep QA's IP/certificates and unrelated customer routing unchanged; customer
migration remains a separate cutover.

The confirmed staging URL is `https://lean.g5labs.ai`, with
`*.preview.lean.g5labs.ai` previews. Production uses
`https://app.g5labs.ai` and `*.preview.app.g5labs.ai`. Both admit sign-in only
for a pending invitation or an existing membership; there is no login allowlist
setting. Populate the same settings
in the target roots and delivery bindings; choosing names does not activate DNS.
New release-trigger failures reuse the existing QA notifier/destination.
Additional availability/backup monitoring automation is outside the clarified
scope; this setup follows G5’s existing Cloud Build notification approach.

## Pull request Terraform checks

Infrastructure PRs run `g5-terraform-plan-qa`, `g5-terraform-plan-staging` and
`g5-terraform-plan-prod`. The target checks reuse the release pipeline in
source/plan mode against their own platform/addons state and settings. They do
not apply, publish bundles or queue releases, and need no release approval.
Target plan failures fail their CI check. Existing apply/promotion sequencing is
unchanged. Creating live checks requires the identity, target bootstrap and
initialized state described above; editing this code does not activate them.

The dedicated target planner is separate from both the QA planner and the release
account. Its state reads expose generated database credentials to PR code, so
only trusted PR authors should run these checks and logs must stay restricted.
It has no release/approval/impersonation or state-overwrite permissions. Target
plans retain G5's `-refresh=false`: they compare source with stored state and
are not a live drift audit. The current Helm provider/configuration needs no
additional Kubernetes-secret reader grant for these non-refreshing plans. Option B
uses this same planner for DNS metadata reads. Its existing built-in roles cover
those reads, so no DNS role is added; verify effective access and denied writes
before use. Non-refreshing plans do not replace refreshed operator
checks before delegation.

## Infrastructure bundle and failure recovery

QA builds and pushes G5's `terraform-bundle` OCI carrier into the selected `g5`
registry repository and passes its immutable digest to staging after QA apply
succeeds. Staging applies that bundle and passes the same digest to production.
The bundle carries Lean's `infra/` and relative `scripts/` dependencies.

The runner ports G5's materialize, validate, test, plan, apply and chain steps.
Lean's necessary adapter applies `platform` before planning/applying `addons`,
using independent target buckets and native per-root state locks. It retains G5's
staging/prod plan-refresh behavior. No GCS Terraform source protocol, extra
cross-root lock or downstream delete/freshness gate is added. Existing QA apply
protections remain in the QA runner. Infrastructure and app candidates need not
share a commit; the train owns their ordering, as in G5.

A failed staging or prod infrastructure apply withholds app approval. A required
external check failure stops automatic app promotion. Repair and retry the
reviewed revision; do not assume application rollback undoes database migrations.
The train supports `_DRY_RUN=true` to inspect candidates without approving or
rejecting anything. The weekly preference is already enabled; it becomes
effective when the release line is deployed with promotion enabled.

## Artifact ownership transition

`artifact_project` defaults to `g5-qa-1ccc`; the only alternative is
`g5lean-admin-1ccc`. It changes application/worker artifact ownership and the
immutable release bucket together. Build execution, source provenance, GitHub
connections, signing, caches and release queues remain in QA. Target state and
native Terraform locks remain in each target project.

Provision the admin repositories, buckets and their scoped publisher/consumer
bindings through the separate admin infrastructure operator first. QA delivery
does not manage admin resources or grant the central release identity admin write
access. Retain the existing QA buckets, repositories and read grants while deployed
versions reference them. No Terraform resource is moved to admin by this switch.

Before changing the live setting, verify first publication and actual container
pulls/worker boot from admin using the intended identities. Apply the reviewed
setting consistently across QA publishers and both target triggers through the
trusted operator's delivery apply. Legacy QA triggers may omit `_ARTIFACT_PROJECT`
and `_RELEASE_BUCKET`: only QA ownership and an empty release bucket are implied.
Normal QA self-apply can install the new keys without a manual trigger bootstrap.
Admin ownership or a release bucket still requires explicit trusted trigger settings
before the corresponding pipeline invocation.
Set the saved QA apply trigger's `_QA_ENABLE_RELEASE_TRAIN` substitution to `true`
as part of that rollout; an existing explicit `false` overrides the new default
and is fed back into Terraform by self-apply.
The QA pipeline checks ownership before self-apply, so a build substitution cannot
switch ownership or bootstrap missing trigger policy. Scripts compare
invocations with authoritative trigger policy and reject mixed/overridden artifact
destinations; pending releases from the previous layout must be requeued. Central
release reads digest-pinned OCI bundles and generation-pinned app records without
rewriting them in admin.

Applying target bootstrap and initial platform/addons execution remain prerequisites,
not completed setup. The admin transition is not live or accepted by local tests.

No cloud resources are created by editing these files. Live plans, IAM checks,
bootstrap, deployment, DNS, acceptance and scheduler activation remain distinct
reviewed execution steps; do not merge or apply them automatically.
