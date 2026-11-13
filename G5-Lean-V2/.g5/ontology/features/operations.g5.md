# Platform operations


## Distribution and reporting

<a id="g5.platform"></a>
[I · g5.platform]

The platform is G5 itself as operated: how it is distributed, licensed, monitored and reported on, apart from any one tenant's work.

<a id="g5.platform.self-hosted-license"></a>
[D · g5.platform.self-hosted-license]

A configured administrator provisions a license for self-hosted distribution, setting its registry coordinates (location, project, repository and puller service account); an omitted coordinate takes the deployment's configured default. Before issuing a registry token, G5 checks the license is active, unexpired and permits the requested product and release, and returns only the coordinates stored on the license. License keys are stored as hashes, and a newly generated key is revealed only in the provisioning result.

- Supplied coordinates are not checked against an allowlist, because the administrator is trusted; an allowlist would be optional hardening.
- When minting a token fails, G5 answers with a generic 503 that names no internal identity, such as the puller service account, and keeps the cause in the server log.
- Revoking is a deployment-operator action: the operator sets the license status to `revoked`, the token route then refuses it, and an already-issued registry token stays valid until it expires, at most an hour after issue.

<a id="g5.platform.azure-canary-login"></a>
[D · g5.platform.azure-canary-login]

The Azure self-hosted canary requires a Microsoft Entra ID sign-in before it provisions the worker host and runs the microVM chat turn, because Azure-hosted customers authorize with Entra and this lane is their pre-release receipt. GitHub login is an optional second sign-in when its secrets are present.

- The callback hostname is stable per DNS label; the run adds the Entra redirect URI through Microsoft Graph only when the app registration object id is supplied and never removes it, otherwise the tenant owner registers it once.
- The smoke account requires the administrator's established tenant and object identity under [login proof](users.g5.md#g5.user.login-proof); matching the installer's default-administrator email is not enough.
- The lane stays blocked until a separate Entra onboarding design establishes that identity safely; the [security correction](users.g5.md#g5.user.login-providers) adds no exception for canary sign-in.

<a id="g5.platform.report-delivery"></a>
[D · g5.platform.report-delivery]

Platform problem reports are persisted before delivery to the configured platform repository is attempted, and submission acknowledgment is distinct from external issue creation. The submission identity and destination are fixed, and an uncertain external outcome is reconciled instead of blindly creating another issue. A worker that loses its claim must not create a duplicate after another worker takes over.

<a id="g5.platform.report-admission"></a>
[D · g5.platform.report-admission]

Filing a platform report needs a [person session](../concepts/security.g5.md#g5.security.credential-classes), configured delivery and the shared admission limits; like [client telemetry](#g5.platform.client-telemetry), the public route refuses API keys, and the workspace agent files through its own route, attributed to the owner. Reporting grants neither tenant repository access nor access to delivery credentials.

- The reporter's and the platform's allowance are charged before a report is parsed or validated, so a report refused afterward, for its content or for capacity, keeps the charge; only the quota's own refusal charges nothing. Charging first bounds the parsing and decoding work one reporter can force.
- Text, context and screenshots are bounded and validated before acceptance; when every screenshot-decoding slot is busy the report is refused with 503, which tells the client to retry, not 429.
- A screenshot delivery failure may leave a text-only report.

## Admission and telemetry

<a id="g5.platform.shared-quota"></a>
[D · g5.platform.shared-quota]

Check and charge every required rate-limit dimension together. A quota refusal charges none; concurrent requests cannot each spend the same remaining allowance. Later report validation follows [report admission](#g5.platform.report-admission).

<a id="g5.platform.client-telemetry"></a>
[D · g5.platform.client-telemetry]

Browser telemetry is accepted only from an authenticated browser session; its actor and organization are derived on the server, explicit client provenance is retained, and the whole batch is validated before any event is emitted. Client payloads are never treated as trusted runtime telemetry.

- A click label never includes the clicked control's visible text, which can carry what the user typed or saw.
- Preference telemetry reports the value of each Settings-page appearance, layout and editor preference, once per signed-in person and organization per browser tab, and again on each change with the previous value.
- A reported value is one of the preference's offered choices, a boolean or a bounded number; anything else is reported as `other`, and the remembered per-harness model choices are not reported, so typed text never travels as a preference value.

<a id="g5.platform.client-telemetry.retention-question"></a>
[Q · g5.platform.client-telemetry.retention-question]

How long are client telemetry events kept? Each event records the person, their routes and clicks, and the client IP, and G5 states no retention period, so events last as long as the deployment's log storage keeps them. Choosing a period, and whether self-hosted deployments must follow it, would settle this question.

## Delivery alerts

<a id="g5.quality"></a>
[I · g5.quality]

Quality covers how G5 is built, tested, released and watched: its CI, delivery pipeline and operational alerts.

<a id="g5.quality.delivery"></a>
[D · g5.quality.delivery]

Delivery is G5's build-and-release pipeline — QA, staging and production — that turns a merged commit into running software and reports on it.

<a id="g5.quality.delivery.failure-alerts"></a>
[D · g5.quality.delivery.failure-alerts]

Every QA non-PR Cloud Build trigger reports failure, internal error, timeout and queue expiry to deployment-notifications, while successes and intentional cancellations stay silent. Lean staging and production failures use the same destination with the environment identified, keeping G5's existing build-event notifier.

- A notifier receives build-status events outside the failing build, and new non-PR triggers enter coverage by default.
- Additional availability or backup monitoring automation is outside this setup scope.
- The operator owns the webhook, which stays out of Terraform state; QA's copy of the dev hook is an explicit, operations-only exception to separate environment credentials.

<a id="g5.quality.delivery.approval-alerts"></a>
[I · g5.quality.delivery.approval-alerts]

Notify deployment-notifications when an approval-gated staging application or infrastructure build starts running, whether a human or the weekly train approved it, including the recorded approver or approval comment, trigger, build link, commit and branch; show unknown when attribution is unavailable. Failure alerts and deployment approval policy are unchanged.

- Automatic production follow-up builds are not labelled as newly approved.
- Event redelivery can repeat a notification.

<a id="g5.quality.delivery.cutoff-summary"></a>
[D · g5.quality.delivery.cutoff-summary]

At the weekly merge cutoff, announce the expected release time, commit range and included PRs, using the existing release-train planner and the last successful production deployment, and mark incomplete or still-forming predictions; final selection belongs to the train at approval time. The job reuses G5's notification behavior, reporting lookup or delivery problems without blocking releases. It has read-only build and source access and the existing Slack webhook, with no deployment or approval authority.

<a id="g5.quality.delivery.backups"></a>
[D · g5.quality.delivery.backups]

Follow G5's existing Cloud SQL backup configuration: daily automated backups,
30 retained backups and point-in-time recovery with seven days of transaction
logs. The release pipeline itself adds no backup monitoring.

<a id="g5.quality.delivery.backups.filestore"></a>
[D · g5.quality.delivery.backups.filestore]

Each environment's repository Filestore share gets a daily native backup from a scheduled Workflow with a dedicated identity. Terraform owns the automation, not individual backups, and adoption preserves the deployed source, fixed configuration, names, ownership labels and runtime permissions. Native Filestore backups are not a verified repository recovery system ([durability](repositories.g5.md#g5.repo.durability)).

- A run creates or reuses the UTC day's backup and verifies its source, share, ownership and READY state before cleanup.
- Only owned daily or initial backups older than 30 full days may expire; each candidate is rechecked immediately before sequential, confirmed deletion. Missing candidates are harmless, and long-running operations have a deadline.
- Manual backups and other shares are untouched.
- Adoption imports live identities, bindings and schedules; the non-importable Workflow needs a recoverable operator-controlled handover outside the daily run window, with permissions and the apply ready before deletion.

<a id="g5.quality.delivery.filestore-migration"></a>
[D · g5.quality.delivery.filestore-migration]

Lean QA, staging and production migrate repository and attachment storage to separate 1 TiB Zonal shares of fixed 4,000 IOPS, preserving the [daily backup policy](#g5.quality.delivery.backups.filestore) and keeping the old HDD instances protected. One per-environment selection governs deployment and backup sources, and ordinary deployment refuses a mismatch with the active storage binding.

- One enablement PR commits the final Zonal selection for all three environments; provisioning and a desired-state output do not switch live mounts.
- Automatic apply, deploy and promotion paths are held before merge and through the rollout, and normal delivery resumes only when every live binding and backup source agrees with the committed selection.
- Before candidates enter shared Terraform state, merge the enablement configuration under the verified global hold and retire incompatible queued releases, so they cannot remove resources absent from their source.
- The operator holds delivery through applies, drains durable workspace writes and quiesces storage writers for a final whole-share verification and binding cutover, retaining the deployed application and schema; QA is verified before later environment cutovers, each with a coordinated write-pause window.
- Recovery keeps paired SQL and filesystem evidence and the old data; once a writer restarts, rollback reconciles new work rather than discard it by returning to the old share.

<a id="g5.quality.delivery.qa-forward-only"></a>
[D · g5.quality.delivery.qa-forward-only]

QA deployment waits for the candidate workloads and reports rollout failures without automatically restoring a previous application image; recovery advances to a corrected release, since database migrations can already have committed before workload readiness fails. The operator may manually reprovision incompatible hosts. This does not authorize a database reset or automatic fleet replacement.

## QA deploy ordering

<a id="g5.quality.delivery.qa-deploy-order"></a>
[D · g5.quality.delivery.qa-deploy-order]

QA deploys of one environment run Helm strictly in build-creation order and skip a build
once a newer deploy build of that environment exists, whatever that newer build's
status: the newer commit will be deployed by its own build, so deploying the older one
would only be rolled forward again. A build waits only while an older same-environment
build's deploy phase, through its smoke test, is unfinished; an older build's remaining
work after that phase does not hold the queue. Reason: waiting on whole older builds,
including their [off-path companion work](#g5.quality.delivery.qa-companions-off-path),
made most merges stale before Helm, so QA received 4 of 21 deploys on 2026-10-20.

<a id="g5.quality.delivery.qa-terraform-freshness"></a>
[D · g5.quality.delivery.qa-terraform-freshness]

QA Terraform apply checks for newer builds of the same trigger before processing roots and immediately before each apply; confirmed supersession succeeds as a skip of the remaining roots and downstream release publication, like QA deployment, while earlier completed applies remain. Freshness-lookup and Terraform errors still fail. Release publication requires a valid decision from the completed root step.

<a id="g5.quality.delivery.qa-companions-off-path"></a>
[D · g5.quality.delivery.qa-companions-off-path]

Workload-template publication and fleet cache warming follow the image pushes but stay
off the [deploy decision's path](#g5.quality.delivery.qa-deploy-order): they neither
delay it nor can they interrupt it, so a publication failure is reported, and fails the
build, only after the deploy phase has ended. A host that meets a new base image before
its companion exists formats the template locally under
[workload templates](#g5.scope.deployment.workload_templates). Reason: serial publication
of five companions takes 47–82 minutes, and a failing step would otherwise cancel a Helm
upgrade running beside it.


<a id="g5.quality.delivery.qa-worker-image-coverage"></a>
[D · g5.quality.delivery.qa-worker-image-coverage]

A build-mode QA deploy pins a READY worker image from the promoted commit, or from an ancestor whose worker inputs are unchanged across the intervening range by the worker trigger's file filters. When those inputs changed, it waits within the configured bound for an in-range bake and allows its image a bounded READY grace, and if rechecks find no bake it may run the enabled worker trigger once for the promoted commit and await that build under the same bounds.

- Concurrent deploys may each start a bake.
- A required bake's failure, failed start, unreadable evidence or exhausted bound fails the gate and leaves the live API untouched.
- A disabled trigger permits only the promoted commit's own image and never authorizes a bake; deploy-only and promotion runs keep their existing artifact handoffs.

<a id="g5.quality.delivery.qa-deploy-only-refs"></a>
[D · g5.quality.delivery.qa-deploy-only-refs]

A QA deploy-only run builds nothing and deploys the image refs its operator supplies, without checking that they were built from the run's commit, so an operator can redeploy exact, already-built artifacts. Each supplied ref must still be immutable: container images pinned by digest, and a READY worker image of the configured family in the QA project. Staging and production follow [production delivery](#g5.quality.delivery.production) instead.

## Production delivery

<a id="g5.quality.delivery.production"></a>
[D · g5.quality.delivery.production]

Lean QA serves the G5 dev and build role: main builds once and queues staging for approval, and successful required staging checks promote the same immutable artifacts to production without another approval or rebuild. The weekly release train ports G5's cutoff, queue selection and sequencing: finish the selected staging and production infrastructure before approving the independently selected application candidate, without requiring infrastructure at that application's exact SHA or a separate manual infrastructure gate.

- Port G5's digest-pinned OCI Terraform bundles and normal state locking, retaining only Lean project, repository-layout and environment-root compatibility.
- Preserve the supported Lean runtime and forward recovery; a rollout failure must not imply a database rollback.
- A pending approval does not own a deploy turn: QA releases its turn after smoke tests, while staging and production retain it through release checks and promotion.
- Promotion waits for required artifact publication and the existing build tail, without placing that work back on QA’s Helm path.

<a id="g5.quality.delivery.pr-plans"></a>
[D · g5.quality.delivery.pr-plans]

Infrastructure pull requests receive separate QA, staging and production plan checks before merge, following G5. Target plans reuse the release runner in source/plan mode and never apply, publish artifacts or promote; a dedicated target planner, separate from QA's planner and the trusted release identities, reads target state and writes native locks only, which confers no deployment, state-overwrite, impersonation or approval authority.

- State includes generated database credentials, so PR authors and build-log readers must be trusted with that content.
- Target state and planning permissions are installed before these checks are enabled.
- G5's non-refreshing target-plan behavior and existing release approvals are preserved; PR plans do not prove the absence of live drift.

<a id="g5.quality.delivery.environments"></a>
[D · g5.quality.delivery.environments]

Staging and production own separate runtime data, secrets and Terraform state, while the shared Lean admin project owns promoted images and release artifacts; QA retains build execution and release coordination through scoped trusted identities. One central trusted release identity holds direct infrastructure and application authority in the configured targets, like the existing G5 release line, and the release-train approver and untrusted PR identities stay separate from it.

- Target roles are scoped to the services Lean uses, and the named projects remain the authority boundary rather than adding target-runner impersonation; the central identity shares authority across targets but is not an IAM boundary between staging and production.
- Artifact access follows the [admin permission split](#g5.quality.delivery.artifact-authority), and PR-controlled CI cannot deploy or approve these environments.
- Existing QA artifacts stay available until their consumers transition and active references are retired.

<a id="g5.quality.delivery.environments.activation"></a>
[D · g5.quality.delivery.environments.activation]

Target configuration must be explicit, and promotion stays inactive until external acceptance. The G5 weekly schedule is enabled when the configured release line is enabled and preserved on repeated applies, and no live train can exist before its release triggers and target prerequisites are deployed.

- Private bootstrap checks cannot substitute for complete customer workflows.
- Setup may deploy reviewed candidates manually with automatic application promotion disabled, but must not claim external acceptance; a setup-only line does not run the weekly train.

<a id="g5.quality.delivery.environments.least-privilege"></a>
[D · g5.quality.delivery.environments.least-privilege]

Before the release identity is created, remove runtime impersonation of administrative accounts, remove image-builder VM and service-account attachment and Kubernetes authority, and restrict PR planner state writes to lock objects. Image publication keeps only its required image operations, artifact access and signing authority.

<a id="g5.quality.delivery.sensitive-storage"></a>
[D · g5.quality.delivery.sensitive-storage]

Lean QA, staging, production and admin state, and QA/staging/production application objects, must not become readable merely through basic Project Viewer membership; that legacy reader role is managed explicitly, preserving named workload and planner grants and unrelated administration. Metadata visibility does not imply payload read authority, and state stays sensitive even for an authorized read-only planner. Lean state, customer and build buckets enforce public-access prevention and uniform bucket-level access explicitly, independent of inherited organization policy.

<a id="g5.quality.delivery.runtime-iam"></a>
[D · g5.quality.delivery.runtime-iam]

Hosted worker VMs use a dedicated environment host identity, separate from GKE nodes, with the required artifact and cache reads and telemetry writes, and the API may attach only that host identity. Runtime Compute access follows the actual provisioning operations, constraining instance, disk and firewall names where Google supports it.

- Network, subnet and router operations stay project-scoped: permitted host and firewall creation do not establish isolation from Terraform-owned networks.
- API customer-storage compatibility grants permit objects and metadata, not bucket IAM or configuration changes.
- Identity cutover is coordinated with API deployment and existing-host rotation, since infrastructure changes alone do not update running hosts.

<a id="g5.quality.delivery.secret-authority"></a>
[D · g5.quality.delivery.secret-authority]

External Secrets reads only the secret references its enabled manifests require, and Lean API identities have no direct Secret Manager payload grants, because runtime values arrive through Kubernetes. A new inventory entry grants neither identity automatic access. The unused CI GitHub PAT is retired through reviewed deletion.

<a id="g5.quality.delivery.terraform-authority"></a>
[D · g5.quality.delivery.terraform-authority]

QA Terraform manages secret resources through an operator-owned role without payload reads, granting only the four database-version reads refresh requires. Trusted-main Terraform keeps infrastructure and IAM administration, including granting itself access, so task-specific grants are not a sandbox against that administrator.

- Service-account attachment is explicit for each managed workload or trigger.
- Replacement grants are installed before broad grants are withdrawn.
- The existing release sequencing and freshness contracts are preserved.

<a id="g5.quality.delivery.staging-admission"></a>
[D · g5.quality.delivery.staging-admission]

Staging uses the same [login admission](users.g5.md#g5.user.admission) as QA and
production, without a separate employee-only gate. Organization and workspace access
still require the normal authorization checks.

<a id="g5.quality.delivery.production-admission"></a>
[D · g5.quality.delivery.production-admission]

Production admits only invited people and existing members under the shared [login admission](users.g5.md#g5.user.admission); the email and domain allowlist that controlled the rollout is retired, and unlisted personal accounts are still refused. The rule is stricter than the allowlist: staff in the allowed domain now also need an invitation. Retirement does not revoke existing sessions or memberships.

<a id="g5.quality.delivery.artifact-authority"></a>
[D · g5.quality.delivery.artifact-authority]

Trusted QA publishers get scoped writes to the shared admin artifacts, while the central release identity and runtime consumers get only the required reads and image use; a separate infrastructure management identity manages admin repositories, storage and IAM. The central release identity must not manage admin infrastructure, write artifacts or impersonate that manager, and PR-controlled CI cannot publish shared release artifacts.

- Mutable deployment state and locks stay in each target environment, separate from the immutable admin release records.
- A single reviewed artifact owner holds the registry, worker images and release records, while build provenance and caches stay in QA.

<a id="g5.quality.delivery.artifact-authority.readers"></a>
[D · g5.quality.delivery.artifact-authority.readers]

QA fallback artifact readers must be named API, worker-host or GKE-node identities of the corresponding approved target, so arbitrary principals cannot acquire those reads through release configuration. Upload clients may list object names and metadata across their destination bucket — QA worker artifacts, and admin worker-artifact or app-release buckets — granted separately from prefix-scoped payload reads and writes, so listing adds no payload access, replacement, deletion or bucket administration.

<a id="g5.quality.delivery.artifact-authority.publishers"></a>
[D · g5.quality.delivery.artifact-authority.publishers]

Artifact publication must not overwrite different content on retries, and existing QA references stay until their consumers transition. Worker-image publishers create versioned objects without replacement or deletion authority, while mutable latest pointers and ISO caches have explicit narrower grants. Bootstrap builders publish to their own registry, with no publisher write access to the application-image registry, and required mutable cache and tag behavior is preserved.

<a id="g5.quality.delivery.image-cve-reporting"></a>
[I · g5.quality.delivery.image-cve-reporting]

Scan every image the platform deploys — the application containers, the worker-host boot image with its guest kernel and VM runtime, and the platform guest images (the guest environment images and the base-image tiers a workspace VM boots and pulls). Expose reports for the immutable digests selected for production and re-check deployed images as new vulnerabilities are published, not only at deploy time.

- Findings and unavailable scanning must not block deployment, and missing or incomplete analysis is distinguished from a clean report; the existing release, canary and dependency-check gates are preserved.
- Report access is scoped to reads under the [artifact authority](#g5.quality.delivery.artifact-authority), and scanning costs and shared QA effects are disclosed before activation.
- As of 2026-11-06, reporting covers only the application and guest environment images and runs only at deploy.

## DNS ownership

<a id="g5.quality.delivery.dns"></a>
[I · g5.quality.delivery.dns]

Cloudflare owns the parent `g5labs.ai` zone, including marketing and company email, and delegates `qa.g5labs.ai`, `lean.g5labs.ai` and `app.g5labs.ai` to Google Cloud DNS in the corresponding Lean projects. Terraform owns the application A, preview-wildcard A and certificate-validation CNAME records within those children, while Cloudflare holds only the reviewed parent NS delegations for them. No Cloudflare provider or API credentials are added to environment automation.

## Delegated DNS resources

<a id="g5.quality.delivery.external-dns"></a>
[D · g5.quality.delivery.external-dns]

Staging and production reuse QA's foundation zone and shared networking record management, its DNS API and runner access, ingress IP, certificate authorizations, certificates, maps and HTTPS bindings; their zones are created in the existing platform states, and bootstrap supplies the DNS API and the central release identity's DNS authority. PR planning stays read-only apart from its existing state-lock writes, and separate zones do not isolate the central account, which keeps authority in both target projects. Avoid a new root or duplicated record implementation.

- A delegated zone must not be deleted by disabling public ingress: protect its lifecycle and preserve the validation records retained certificates need.
- Configured hostnames within the environment child determine DNS and TLS resources independently of the public-ingress flag; an empty hostname configuration creates only the protected child zone.
- Verify public authority, ACTIVE certificates and actual HTTPS and map attachment before declaring external acceptance or enabling target promotion.

<a id="g5.quality.delivery.external-dns.staging-prod"></a>
[D · g5.quality.delivery.external-dns.staging-prod]

For staging and production, retain G5's certificate, Helm ingress and manual proxy-map attachment procedure. The platform, addons and application triggers consume the same default-off environment settings, retained on later releases, and follow the [existing release sequence](#g5.quality.delivery.production) without a separate application infrastructure-history gate. Keep controller certificate annotations and require external acceptance before promotion, adding no activation service.

## DNS review and customer cutover

<a id="g5.quality.delivery.dns-review"></a>
[I · g5.quality.delivery.dns-review]

DNS-related code and record changes belong in a separate PR and need two human reviewers before merge or application, while other pipeline changes keep ordinary review and PR count is unrestricted. Lean staging uses `https://lean.g5labs.ai` and production `https://app.g5labs.ai`, with `*.preview.<app-hostname>` previews and environment-configured public URLs and callbacks.

- Current customer routing is preserved until the separately reviewed migration cutover.
- Marketing takes the bare domain then; old app links are preserved with reviewed redirects, and API clients, callbacks and WebSockets are handled separately from browser redirects.

## QA and DNS impact notices

<a id="g5.quality.delivery.impact-notice"></a>
[I · g5.quality.delivery.impact-notice]

Before production-pipeline work that may affect QA or DNS, prominently notify the
user of each affected area, expected behavior, material risk and activation point.
Include indirect effects through shared modules, IAM, triggers and artifact
locations; distinguish local changes from applied cloud changes. This notification
adds no approval gate beyond the existing DNS review and execution constraints.

## Workload image delivery

<a id="g5.scope"></a>
[D · g5.scope]

Scope records what a platform deployment itself must contain and must not depend on.

<a id="g5.scope.deployment"></a>
[D · g5.scope.deployment]

Deployment scope keeps G5's build and deploy working without access to any customer's repositories or running state.

<a id="g5.scope.deployment.workload_templates"></a>
[D · g5.scope.deployment.workload_templates]

G5 delivery must work without access to customer repositories: its build pipeline publishes pristine workload templates for its pinned environment images, each the operating system and toolchain with an empty workspace. Templates are rebuildable accelerators, never required customer inputs to a platform deploy, so repository commits and saved workspace state follow [fresh materialization](../software_architecture/hosts.g5.md#g5.runtime.materialization) at Start and a source edit alone does not invalidate a template.

- An image digest identifies content, not permission to share private images.
- Build-time caches contain no credentials or working state, and hosts keep their identity, capacity, isolation and readiness checks with prebuilt data, so a cache miss cannot bypass them.

<a id="g5.scope.deployment.workload_templates.artifacts"></a>
[D · g5.scope.deployment.workload_templates.artifacts]

Immutable compressed raw-disk artifacts are published through the existing artifact registry, binding the OCI digest, architecture, bootstrap and layout version and disk capacity. Each artifact's registry tag is derived from its whole identity — the source image digest, the template and bootstrap versions, the content address of the OCI template, the architecture and the disk capacity — so two different templates never share a tag. Hosts verify and unpack each download within its published capacity before use.

<a id="g5.scope.deployment.workload_templates.preparation"></a>
[D · g5.scope.deployment.workload_templates.preparation]

A customer-selected image unavailable to platform delivery is prepared on authorized organization infrastructure when needed, using the same bounded template cache and validation; the configured host image is prepared before registration, and a later image miss must finish preparation before workspace readiness. Deploy-only runs reuse qualified artifacts, and a missing compatible prebuild can be prepared by the same host operation; an environment update does not require rebaking the outer GCE host image.

- Explicitly authorized repository-bearing variants stay optional under [seed eligibility](../software_architecture/hosts.g5.md#g5.architecture.workload_repo_seed) and may be distributed only to infrastructure authorized for those sources.

<a id="g5.quality.delivery.ci"></a>
[D · g5.quality.delivery.ci]

CI is the pull-request and build pipeline that compiles, tests and bakes G5's images before delivery.

<a id="g5.quality.delivery.ci.image-key"></a>
[D · g5.quality.delivery.ci.image-key]

Key the warmed CI builder on its recipes, Cargo configuration, lockfile/manifests,
build scripts and pinned Cloud Hypervisor/kernel/bootstrap inputs. Source and SQLx
query-cache edits use the checkout and do not change the warmed-toolchain identity.
A cache hit still runs all gates against the candidate checkout.

<a id="g5.quality.delivery.pinned-third-party-tools"></a>
[D · g5.quality.delivery.pinned-third-party-tools]

Platform image recipes and privileged host bootstrap or install scripts obtain third-party tools only as version-pinned release artifacts: each download lands in a file and is checked against a sha256 recorded in the repository for that version and architecture before anything is extracted or executed. No network response is piped into a shell, interpreter or archive extractor, and a vendor's install script or same-origin checksum file is never the only check, because transport security shows who answered, not that the bytes are the reviewed release.

- A mismatch, a missing artifact or an architecture without a recorded pin fails the build or bootstrap before any installation step.
- Packages from signed apt repositories, and toolchain components rustup checks against its signed manifests, need no repository pin.
- An artifact cached for guests to verify again carries the vendor's checksum file only when that file agrees with the repository pin.

## CI E2E runtime

<a id="g5.quality.delivery.ci.e2e-runtime"></a>
[I · g5.quality.delivery.ci.e2e-runtime]

Shorten the g5-ci-e2e pull-request check without reducing required coverage: the same discovered tests run with the same assertions and receipts, and each test keeps its own fixture state, with two exceptions. A test may execute once instead of twice when one lane [owns it](#g5.quality.delivery.ci.single-execution-owner).

- As the measured experiment, a parametrised matrix whose variants share one code path runs a reduced pull-request tier by default (`frontend/e2e/matrix.ts`; 17 of 357 discovered tests left out, every variant that alone covers a branch kept), and `G5_E2E_FULL_MATRIX` restores every variant for a future main or nightly trigger.
- A 35–40 minute warm successful build is the experimental direction, not a pass/fail gate; the target, its measurement, cold-image builds and fixture reuse remain open questions in the workspace's Project Docs.

<a id="g5.quality.delivery.ci.browser-shards"></a>
[D · g5.quality.delivery.ci.browser-shards]

The broad browser lane runs three shards of two workers each on a fourteen-slot host of one-vCPU 2.5 GiB guests, keeping every test file whole; its shared host must accommodate the running seed, simultaneous source and fork VMs and lifecycle overlap. The experiment keeps raising host admission rather than reducing browser concurrency or relaxing test budgets, on the hypothesis that fake-provider guests need one vCPU.

- Its planner places files from committed per-file durations: it replays Playwright's dispatch (files in discovery order, each to the earliest free worker) and picks the partition whose slowest shard is predicted to finish first.
- A file with its own worker-scoped options runs after the others, which list reports do not show, so its prediction is off by at most its own duration; a file without a measurement weighs the measured mean and is listed in the plan output.
- `fullyParallel` is rejected until file isolation is audited, because the planner and receipts rely on whole-file ownership. The plan's coverage proof is [its own commitment](#g5.quality.delivery.ci.browser-shards.partition).

<a id="g5.quality.delivery.ci.browser-shards.partition"></a>
[D · g5.quality.delivery.ci.browser-shards.partition]

Expands [g5.quality.delivery.ci.browser-shards](#g5.quality.delivery.ci.browser-shards).
Whatever the placement, the planned shards must partition exactly the Chromium
test IDs of the original `--shard` inventories: no file or ID on more than one shard
or missing. This is verified against discovery before execution, and against every
executed report before receipts are recorded.

<a id="g5.quality.delivery.ci.single-execution-owner"></a>
[D · g5.quality.delivery.ci.single-execution-owner]

The broad Rust E2E lane may skip a test only by its exact name, and only when the value-loop receipt catalog routes that test to a Rust receipt profile, which is then its one execution owner. A script check rejects any other skip, and any owned name contained in another test's name, since both lanes select by substring.

- The receipt lane's missing-selector rule fails the build if the owner never runs the test.
- Skipping a test, not its module, keeps tests later added to the module in the broad lane.
- API-restart continuity (M6) and explicit formal reviews (RV7) are owned this way.

<a id="g5.quality.delivery.ci.timing-evidence"></a>
[D · g5.quality.delivery.ci.timing-evidence]

CI logs separate execution from waiting so timing reports find the real critical path; this is evidence only and gates nothing. Each browser shard reports per-file first-attempt and retry time beside the planner's prediction, and a lane that waits for another lane's markers reports how long it waited.

- A test records named phase timings on its result: review fixture setup phases, body and cleanup, the review helpers' write, commit, terminal and push phases, and time a workspace VM spent queued for capacity.
- Workspace VM transitions are recorded the same way, from fields the status poll already reads: the clone and lifecycle API round trips, each VM status and start-intent sub-status first seen while a test waits, the server's admitted-to-ready interval, the lag until the poll sees `running`, and the wait for the live changes report.

<a id="g5.quality.delivery.ci.provider-failure-bound"></a>
[D · g5.quality.delivery.ci.provider-failure-bound]

A browser acceptance wait for an assistant reply ends as soon as the turn has
confirmably failed, meaning an error status in its assistant message while the
turn's task is failed or cancelled, and reports the provider's message instead of
exhausting the success timeout. An error status while the task is still active keeps
waiting. Test retries and capacity waits are unchanged; capacity waits are reported
under [timing evidence](#g5.quality.delivery.ci.timing-evidence).

<a id="g5.quality.delivery.ci.release-overlap"></a>
[D · g5.quality.delivery.ci.release-overlap]

The E2E pipeline's release compile keeps waiting for the frontend unit tests, which also remain a required gate of the browser fixtures and the final CI status. A cold worker-base rebuild, whose guest-kernel compile takes every worker CPU, likewise waits for the frontend unit tests before it starts, for at most 30 minutes; if their completion marker is still absent after that bound, the rebuild starts anyway.

- A warm pull of the published image never waits.
- Revisit this only with more worker-CPU headroom or a cheaper frontend-test step.

## Local development seeding

<a id="g5.operations"></a>
[I · g5.operations]

Operations are the tools developers and operators run to set up and maintain a G5 deployment, such as seeding a local stack.

<a id="g5.operations.lean-seed"></a>
[I · g5.operations.lean-seed]

Let a developer seed G5-Lean into the running local dev stack from a pinned local
HEAD, including committed `.g5/ontology` and unpushed history. Preserve the commit
and its ancestry in an independent code repository, with an empty docs repository.
Exclude uncommitted files; leave the source checkout and unrelated workspaces intact.

<a id="g5.operations.lean-seed.replacement"></a>
[D · g5.operations.lean-seed.replacement]

Default seeding refuses an existing target; explicit replacement names the exact previous seed workspace ID and uses ordinary owner-authorized deletion, and never resets the stack. The source is staged and validated before the old target is deleted.

- Replacement is not atomic: if deletion succeeds but creation fails, report the failure and allow a fresh create.
- Do not guess whether deletion succeeded.

<a id="g5.operations.lean-seed.fixture"></a>
[D · g5.operations.lean-seed.fixture]

The local-only seed tool creates exactly one ordinary standalone workspace, with no seed-source parent and no special Base status; normal forks of it keep their parent relationships and comparison, rebase and merge workflows, while the root itself has no parent comparison or merge destination. Self-contained Git repositories are installed before the workspace and membership are published atomically, guarding competing workspace-name claims.

- The imported commit and tree, empty docs and current ontology discovery are verified through real Git and API reads, and failed verification must not report successful seeding.
- No remote publishing, legacy ontology projection or implicit VM startup is part of seeding.

## Metadata

### g5.platform

- agent
- Defines the `g5.platform` ID level: G5 as operated — distribution, licensing, monitoring and reporting, apart from tenant work. Composed from the distribution, reporting and telemetry statements below.

### g5.platform.self-hosted-license

- agent
- Source: [license reconstruction](../../uplift/ben-ontology/investigations/workspace.md), license service and self-hosted registry route.
- LicenseAdmission checked active, expiry, product and release gates at token issuance. Hashing, issuer security and IAM are premises; already-minted token revocation is outside the check.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 16 (OP-Q2, OP-Q3, OP-Q10).

### g5.platform.azure-canary-login

- human-verified
- Source: [restore request](../../chat_intent/restore-entra-login/request.md) and [design](../../pr_ontology_diff_design/restore-entra-login/ontology-plan.md); lane state in [issue #748](https://github.com/G5Labs/G5-Lean/issues/748). Providers: [login providers](users.g5.md#g5.user.login-providers).
- The smoke-account admission sentence follows the allowlist's retirement ([issue #776 chat intent](../../chat_intent/issue-776/request.md#self-serve-creation-and-the-allowlist)); the account was already the installer's default admin.
- `.github/workflows/self-hosted-azure-install.yml`, `scripts/self-hosted-azure/{entra,github}-login-smoke.js` over `login-smoke-common.js`; `scripts/test-self-hosted-azure-microvm-canary.sh` pins the contract. Secret matrix in `infra/self-hosted/README.md`. Gap: the Entra secrets are not yet set on the `self-hosted-azure-canary` environment, so no run has passed. [Issue #1101](https://github.com/G5Labs/G5-Lean/issues/1101) additionally requires safe onboarding before that lane can sign in; email-based bootstrap is intentionally refused.

### g5.platform.report-delivery

- agent
- Source: [report reconstruction](../../uplift/ben-ontology/investigations/workspace.md).
- ReportDelivery checked ambiguous-outcome recovery and competing reclaimed workers, exposing [PLATFORM-REPORT-CLAIM](../../../docs/code_gaps.md). The guarded comparison is unimplemented; no production duplicate was reproduced.

### g5.platform.report-admission

- agent
- Source: [report reconstruction](../../uplift/ben-ontology/investigations/workspace.md), platform-issue routes and intake.
- Protocol and size validation and media decoding were inspected directly. Shared admission is checked separately under [quota](#g5.platform.shared-quota).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 17 (OP-Q1, OP-Q4).

### g5.platform.shared-quota

- agent
- Source: [quota reconstruction](../../uplift/ben-ontology/investigations/workspace.md).
- SharedQuota checked all-or-nothing paired admission and independent limits within one finite window. Database locking and clock selection are premises; clock rollover and distributed database behavior were not proved.

### g5.platform.client-telemetry

- agent
- Source: [telemetry reconstruction](../../uplift/ben-ontology/investigations/workspace.md).
- TelemetryAdmission checked browser-session identity, client provenance and whole-batch validation before emission. Payload parsing is abstracted; durable log delivery is not promised.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 18 (OP-Q5).
- Amendment: preference telemetry, user request (2026-10-29); encoder in `frontend/src/lib/preferenceTelemetry.ts`.

### g5.platform.client-telemetry.retention-question

- human-provided; uncertain
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 19 (OP-Q5).

### g5.quality

- agent
- Defines the `g5.quality` ID level: how G5 is built, tested, released and watched. Composed from the delivery and CI statements below.

### g5.quality.delivery

- agent
- Expands: [quality](#g5.quality).
- Defines the `g5.quality.delivery` ID level: G5's build-and-release pipeline across QA, staging and production.

### g5.quality.delivery.failure-alerts

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/qa-slack-failure-notifier/request.md).
- Staging/production destination: [confirmed reuse of QA](../../chat_intent/production-pipeline/request.md#confirmed-hostnames-alerts-and-staging-access). The latest scope clarification retains Cloud Build notifications and excludes additional monitoring automation.
- Existing staging/prod failure events reached notifier send and successful request processing on 2026-10-28; Slack UI receipt was not separately inspected. See the [scoped verification](../../pr_ontology_diff_design/production-pipeline/review.md#release-slack-notifications--2026-10-28-utc), which distinguishes the existing live failure path from the new notification code.

### g5.quality.delivery.approval-alerts

- human-provided, agent
- Source: [approval and cutoff notification request](../../chat_intent/production-pipeline/request.md#release-slack-notifications--2026-10-28). Human intent is notification parity with G5; trigger selection, attribution fallback and event-delivery limits are implementation decisions.
- Design and verification: [notification amendment](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#release-slack-notifications--2026-10-28). Live activation follows merge and delivery Terraform apply.

### g5.quality.delivery.cutoff-summary

- human-provided, agent
- Source: [G5 cutoff message request](../../chat_intent/production-pipeline/request.md#release-slack-notifications--2026-10-28). Reuse of the planner, read-only job identity and failure handling follow the inspected G5 implementation.
- Design and verification: [notification amendment](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#release-slack-notifications--2026-10-28). The message predicts a release; it does not approve one.

### g5.quality.delivery.backups

- human-provided, agent
- Source: the owner's [instruction to follow G5 without additions](../../chat_intent/production-pipeline/request.md#scope-clarification-follow-existing-g5). Inspected G5 `generation/infra/main.tf` and `modules/cloudsql/main.tf` enable this baseline; Lean's existing Cloud SQL module is reused. Old production's live status could not be verified with the current account.

### g5.quality.delivery.backups.filestore

- human-provided
- architecture-decision
- test-coverage
- Expands: [backups](#g5.quality.delivery.backups).
- Source: [issue #1092](https://github.com/G5Labs/G5-Lean/issues/1092), which records the live QA/staging/production automation of 2026-11-04 and the adoption requirements; the earlier scope note excluded only the release pipeline from adding backup automation.
- Correction design and review: [adoption plan](../../pr_ontology_diff_design/filestore-backup-adoption/ontology-plan.md).
- Implementation and adoption procedure: [filestore-backup module](../../../infra/terraform/modules/filestore-backup/README.md); roles in `filestore-backup-roles`. The workflow contract is checked by `scripts/test_filestore_backup_workflow.py` and the Terraform shape by module/root `terraform test` runs. Live plans and applies remain operator steps; consistency, restore drills and recovery policy stay in #977–#980.

### g5.quality.delivery.filestore-migration

- agent
- architecture-decision
- test-coverage
- Source: [user decisions and interpretation](../../chat_intent/filestore-zonal-migration/request.md); [migration design](../../pr_ontology_diff_design/filestore-zonal-migration/ontology-plan.md). User selected Zonal, 4,000 IOPS and backups; the agent defines the staged safety and selection mechanism.
- Partial implementation: `filestore-selection`, `filestore-zonal`, platform/delivery wiring and `check_filestore_binding.py` implement candidate preparation and ordinary-deploy refusal. The enablement PR selects Zonal for all three environments; live mounts remain HDD until controlled cutovers under the global delivery hold. Final-map Terraform/selector and release regression tests pass. Earlier live legacy bindings passed the read-only guard and opt-in candidate plans each contained one create. Preparation CI plans are historical evidence; updated cloud CI and fresh live plans remain required. Candidate provisioning, data-copy verification, cutover and new-source backup acceptance remain unexecuted.

### g5.quality.delivery.qa-forward-only

- human-verified
- Source: the owner's [forward-only retirement decision](../../chat_intent/qa-workspace-lock-contention/forward-only-retirement.md). The QA Helm path retained `--atomic` despite irreversible pre-upgrade migrations; PR C removes automatic application rollback while keeping readiness waits and failure reporting. This is a deployment policy, not a claim that a failed rollout is available.

### g5.quality.delivery.qa-deploy-order

- human-verified
- Source: [issue #684](https://github.com/G5Labs/G5-Lean/issues/684) with its 2026-10-20 build history; design in [the saved plan](../../pr_ontology_diff_design/issue-684-qa-deploy-run-helm-before-workload-template/ontology-plan.md).
- Implemented in `infra/ci/cloudbuild-deploy.yaml` (`wait-deploy-turn` reads per-step status) and the blocker predicate in [`scripts/deploy_queue_blockers.py`](../../../scripts/deploy_queue_blockers.py). Unit tests pass; `gcloud builds list --ongoing` returned per-step QUEUED/WORKING/SUCCESS/FAILURE statuses for in-flight QA builds on 2026-10-21. A live build clearing the gate on an older build's smoke test remains unexecuted until merge.

### g5.quality.delivery.qa-terraform-freshness

- human-verified
- Source: [successful supersession skips](../../chat_intent/production-pipeline/request.md#superseded-qa-terraform-builds--2026-10-23); bounded amendment in the [existing plan](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#qa-terraform-supersession-amendment--2026-10-23).
- Implementation and offline shell verification are recorded in the existing plan; no live apply or release is claimed by this change.

### g5.quality.delivery.qa-companions-off-path

- human-verified
- Source: [issue #684](https://github.com/G5Labs/G5-Lean/issues/684); the no-interruption obligation follows from the [design amendment](../../pr_ontology_diff_design/issue-684-qa-deploy-run-helm-before-workload-template/ontology-plan.md#amendment-2026-10-21-publication-must-not-interrupt-the-deploy-phase-pr-692-review) on the PR #692 review.
- Implemented in `infra/ci/cloudbuild-deploy.yaml`: `resolve-image-refs` waits on the pushes; `publish-workload-templates` never fails its step and writes a success marker; `confirm-workload-templates` fails the build after `smoke-test` when the marker is missing. The waitFor-graph and step-script tests pass; a live build with Helm running beside publication remains unexecuted until merge.

### g5.quality.delivery.qa-worker-image-coverage

- agent
- Source: [issue #1130](https://github.com/G5Labs/G5-Lean/issues/1130), including the skipped-push failure and accepted duplicate-bake tradeoff.
- Design: [skipped-bake recovery](../../pr_ontology_diff_design/issue-1130/ontology-plan.md). Effective live IAM and a recovered live deploy remain unverified.

### g5.quality.delivery.qa-deploy-only-refs

- human-provided
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 20 (OP-Q9).

### g5.quality.delivery.production

- human-verified, agent
- Source: [release model and implementation authorization](../../chat_intent/production-pipeline/request.md); exact G5 source baseline is retained in the [implementation plan](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md).
- The [direct-port and schedule instruction](../../chat_intent/production-pipeline/request.md#direct-g5-port-and-weekly-schedule) supersedes the added app-SHA policy. Implementation uses G5’s train and OCI Terraform release steps with Lean bindings. The weekly preference is enabled; live triggers, scheduler and cloud acceptance remain unprovisioned. Current evidence is recorded in the implementation plan.

### g5.quality.delivery.pr-plans

- human-verified
- Source: [add staging/prod PR plans in #706](../../chat_intent/production-pipeline/request.md#staging-and-production-pr-plans). The original G5 staging/prod PR triggers establish the missing parity; dedicated target-planner separation is the implementation decision.
- Implementation and verification are tracked in the [existing design](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#staging-and-production-pr-plans). Live state/IAM/cluster acceptance remains unexecuted.

### g5.quality.delivery.environments

- human-verified, agent
- The [2026-10-22 audit follow-up](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#iam-audit-follow-up-2026-10-22) adds image-publisher privilege reduction. Code and local tests do not establish live removal of the audited QA grants.
- Source: [accepted environment/review boundaries, shared artifact project and old-line IAM instruction](../../chat_intent/production-pipeline/request.md). Controlled promotion supersedes the prior absolute QA artifact isolation only for trusted release paths; the admin project separates artifact ownership from QA execution. The central release account intentionally shares application/infrastructure authority across the two targets, matching the original release line; it does not provide an IAM boundary between staging and prod.
- Local security review found obsolete API-wide impersonation and unrestricted PR planner state writes; the implementation narrows these before release identity creation. Empty staging, prod and admin projects were created and verified ACTIVE on 2026-10-21; billing was disabled at creation, then [verified enabled for all three](../../chat_intent/production-pipeline/request.md#billing-linkage-completed) later that day. [Exact target IAM code implementation is authorized](../../chat_intent/production-pipeline/request.md#exact-stagingprod-iam-code-authorization); cloud application is not.
- Target bootstrap grants, scoped DB/fixture access, and the `platform` and `addons` roots pass local provider/mock validation, with limits in the [implementation evidence](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#implementation-evidence). Secret management uses a narrower custom role because the built-in admin role includes payload reads; this does not sandbox an IAM administrator. Public activation and live acceptance remain unfinished. Live pipelines still publish to QA; admin infrastructure and cross-project publication are implemented as code only.

### g5.quality.delivery.environments.activation

- human-verified, agent
- Expands: [environments](#g5.quality.delivery.environments). Activation sequencing and the setup-only rule from the [production-pipeline instructions](../../chat_intent/production-pipeline/request.md); verification limits are in the [implementation evidence](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#implementation-evidence).

### g5.quality.delivery.environments.least-privilege

- agent
- Expands: [environments](#g5.quality.delivery.environments). Pre-release privilege reduction from the [2026-10-22 audit follow-up](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#iam-audit-follow-up-2026-10-22); live removal of the audited grants is not established.

### g5.quality.delivery.sensitive-storage

- agent
- Source: [authorization to fix the account audit](../../chat_intent/production-pipeline/request.md#account-audit-fix-pr). Live QA bucket policies granted `projectViewer` legacy object reads; the [existing design amendment](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#iam-audit-follow-up-2026-10-22) defines scoped removal and later live acceptance.
- The legacy reader binding retains only existing Project Owners, avoiding empty-member API ambiguity while excluding Project Viewers. Other Owner/Editor and explicit workload grants remain untouched.
- Scoped implementation and offline evidence: [IAM follow-up review](../../pr_ontology_diff_design/production-pipeline/review.md#iam-audit-follow-up-2026-10-22). Actual cloud removal and inherited effective access remain unverified.
- The [consolidated audit](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#consolidated-iam-audit--2026-10-26) adds explicit public-access prevention for the QA customer bucket; current QA state/build buckets already enforce it.

### g5.quality.delivery.runtime-iam

- agent
- Source: [owner's consolidated scope](../../chat_intent/production-pipeline/request.md#consolidated-iam-follow-up--2026-10-26), audited against issues [739](https://github.com/G5Labs/G5-Lean/issues/739), [740](https://github.com/G5Labs/G5-Lean/issues/740) and [741](https://github.com/G5Labs/G5-Lean/issues/741).
- Implemented with focused Rust, Helm and provider-mocked Terraform checks in the [existing review](../../pr_ontology_diff_design/production-pipeline/review.md#consolidated-iam-audit--2026-10-26); live effective-permission verification remains outstanding. Google does not support resource-name conditions on all network operations; this is a reduction, not worker-network project isolation.

### g5.quality.delivery.secret-authority

- agent
- Source: [owner's consolidated scope](../../chat_intent/production-pipeline/request.md#consolidated-iam-follow-up--2026-10-26) and audited [issue 742](https://github.com/G5Labs/G5-Lean/issues/742). Current runtime consumers use ESO, not direct Secret Manager clients. Enabled-manifest combinations pass local checks in the existing review; live sync and obsolete-PAT retirement remain outstanding.

### g5.quality.delivery.terraform-authority

- agent
- Source: [owner's consolidated scope](../../chat_intent/production-pipeline/request.md#consolidated-iam-follow-up--2026-10-26) and audited [issue 745](https://github.com/G5Labs/G5-Lean/issues/745). Provider refresh requires database-version payload reads. Root splitting and deny-policy isolation are not established by narrowing direct grants; the previously accepted administrative trust boundary remains explicit. Named grants and attachment dependencies pass local Terraform/Python checks in the existing review; staged application and live refresh remain outstanding.

### g5.quality.delivery.staging-admission

- human-provided, agent
- Review categories: [business, system]
- Source: [remove the strict staging gate](../../chat_intent/production-pipeline/request.md#staging-login-policy-amendment), followed by [configure the shared allowlist in both release environments](../../chat_intent/production-pipeline/request.md#production-login-allowlist--2026-10-23). The allowlist was later retired by the owner ([issue #776 chat intent](../../chat_intent/issue-776/request.md#self-serve-creation-and-the-allowlist)). This reuses shared admission; it adds no strict-login mechanism.

### g5.quality.delivery.production-admission

- human-provided, agent
- Review categories: [business, system]
- Source: [production login allowlist](../../chat_intent/production-pipeline/request.md#production-login-allowlist--2026-10-23). The owner asked for a controlled rollout that refuses unlisted personal accounts. The owner then retired the allowlist once operators create every organization ([issue #776 chat intent](../../chat_intent/issue-776/request.md#self-serve-creation-and-the-allowlist)). Invitation-only admission keeps the refusal.
- `settings.auth_allowed_emails` is removed from the Terraform release inputs, ExternalSecret and examples. Terraform tests check that a leftover value is ignored ([design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#sign-in-admission-without-an-allowlist)). Live release inputs still need the value removed when next applied. The 2026-10-23 allowlist configuration and its [review](../../pr_ontology_diff_design/production-pipeline/review.md#release-login-allowlist-review--2026-10-23) are the prior baseline.

### g5.quality.delivery.artifact-authority

- Agent amendment: upload-client destination discovery requires bucket-level object listing; the [consolidated design](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#consolidated-iam-audit--2026-10-26) distinguishes name/metadata visibility from payload and mutation authority. Live publisher acceptance remains unverified.
- human-verified, agent
- The account-audit follow-up constrains QA fallback readers to the configured target's runtime identities, matching the admin root's existing allowlist boundary.
- Source: [confirmed admin permission split](../../chat_intent/production-pipeline/request.md#admin-permission-split). Target-local state/locks are the implementation interpretation that preserves central read-only access to admin.
- Admin bootstrap/artifact roots, prefix-scoped producers, read-only central access and explicit runtime readers pass 12 plan-only mocked tests. Artifact policy/provenance and create-only publication have local regression coverage. The default destination remains QA; operator-installed trigger policy precedes a live ownership switch. No effective admin IAM, cross-project publication or runtime boot acceptance is claimed.

### g5.quality.delivery.artifact-authority.readers

- human-verified, agent
- Expands: [artifact authority](#g5.quality.delivery.artifact-authority). Fallback-reader constraint and the list-vs-payload distinction from the [consolidated design](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#consolidated-iam-audit--2026-10-26); live publisher acceptance is unverified.

### g5.quality.delivery.artifact-authority.publishers

- human-verified, agent
- Expands: [artifact authority](#g5.quality.delivery.artifact-authority). Create-only publication, worker-image and bootstrap grants from the [confirmed admin permission split](../../chat_intent/production-pipeline/request.md#admin-permission-split); no cross-project publication is claimed.

### g5.quality.delivery.image-cve-reporting

- human-verified
- security
- Source: [approved nonblocking scan/report request](../../chat_intent/production-pipeline/request.md#container-image-cve-reporting--2026-10-27). The user selected report-only behavior; the agent selected automatic Artifact Analysis and read-only production reporting.
- Widened to every deployed image and to regular re-checking by the owner's decision 3 of 2026-10-30 and the 2026-10-29 meeting ([request](../../chat_intent/security-invariants/request.md#decisions-the-owner-has-made-2026-10-30)); governed by [platform integrity](../concepts/system_invariants.g5.md#g5.security.tier1.platform-integrity).
- Gap (2026-11-06, main `89eeea5e4`): the requirement is every deployed image with recurring re-checks; the pipeline is narrower. `scripts/report_image_cves.py`, the non-blocking `report-image-cves` step of `infra/ci/cloudbuild-deploy.yaml`, reads Artifact Analysis results for the images `resolve-image-refs` records (api, g5d, edge and the five `g5d-*` guest environment images), and nothing in the repository re-runs it for already deployed images when new vulnerabilities are published. The worker image build (`infra/ci/cloudbuild-packer-worker.yaml`) writes a syft software bill of materials and no vulnerability report; `publish-base-tiers` has no scan step; the migration job's PostgreSQL image (`infra/helm/g5/templates/job-migrate.yaml`, now digest-pinned) and the External Secrets operator's chart images are deployed outside the report (audit 2026-11-02). Worker-image, base-tier, migration-job and External Secrets reports and the recurring re-check are [follow-up](../../pr_ontology_diff_design/security-invariants/ontology-plan.md#follow-up-work). The request said only the application containers are reported; the guest environment images are in fact included.
- Design: [pipeline amendment](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#container-image-cve-reporting-amendment--2026-10-27). Implemented in QA/admin API and read-only IAM roots plus the production report step. Thirty provider-mocked plans, 45 reporter/ordering tests and 132 release regressions pass; the console report route was checked read-only. The pending #735 rollout is reconciled in the existing runbook; a combined-source fixture passes 50 affected mock plans and 175 Python checks. Live IAM transition, API activation, release-account access and a scanned production candidate remain unverified. See the [review evidence](../../pr_ontology_diff_design/production-pipeline/review.md#container-image-cve-reporting--2026-10-27).

### g5.quality.delivery.dns

- human-verified
- Source: the owner's [staging hostname rename](../../chat_intent/production-pipeline/request.md#staging-hostname-rename) and [Option B selection](../../chat_intent/production-pipeline/request.md#option-b-delegated-application-zones-current-decision). This supersedes manual Cloudflare app-record ownership, not parent-domain ownership. The provenance tag records a faithful interpretation of that selection, not human review approval or live verification.
- None of the three child delegations is verified live as of 2026-10-22. QA remains authoritative in Cloudflare until the reviewed NS change. Option B is implemented in the PR stack; no live DNS change was made.

### g5.quality.delivery.external-dns

- agent
- Design: [Option B amendment](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#option-b-delegated-dns). Supersedes the cleanup design at [PR #703 revision 7634c906](https://github.com/G5Labs/G5-Lean/blob/7634c906b2735e83c7b5be0031cfcdf24dffd649/.g5/pr_ontology_diff_design/production-pipeline/ontology-plan.md).
- Implemented locally in shared networking, QA roots, target release-bootstrap/platform and app/addon/delivery consumers. Mock plans cover fresh zones, child namespace rejection, retained DNS/TLS when ingress is disabled and existing planner restrictions; Python/Helm checks cover release consumers. Predefined role inspection confirms existing planner DNS reads without new grants. Refreshed plans, effective deployed IAM and live delegation/TLS/application acceptance remain separately required; see [review evidence](../../pr_ontology_diff_design/production-pipeline/review.md).

### g5.quality.delivery.external-dns.staging-prod

- agent
- Expands: [external DNS](#g5.quality.delivery.external-dns). Retained certificate, ingress and proxy-map procedure from the [Option B amendment](../../pr_ontology_diff_design/production-pipeline/ontology-plan.md#option-b-delegated-dns); live delegation, TLS and application acceptance remain separately required.

### g5.quality.delivery.dns-review

- human-verified
- Source: [production-pipeline instructions](../../chat_intent/production-pipeline/request.md) and the [staging rename](../../chat_intent/production-pipeline/request.md#staging-hostname-rename), preserving the two-reviewer DNS requirement, selected hostnames and separate customer cutover from the former combined DNS block. Provenance describes the owner's intent, not approval of this amendment or operational verification.
- Unassessed as an operational process; two human DNS reviews and live activation/cutover are not recorded by this amendment. Existing hostname configuration is not evidence of DNS readiness or migration completion.

### g5.quality.delivery.impact-notice

- human-provided
- Source: [QA and DNS impact notice instruction](../../chat_intent/production-pipeline/request.md#qa-and-dns-impact-notices).

### g5.scope

- agent
- Defines the `g5.scope` ID level: what a platform deployment must contain and must not depend on. Source is the workload-template design below.

### g5.scope.deployment

- agent
- Expands: [scope](#g5.scope).
- Defines the `g5.scope.deployment` ID level: keeping build and deploy independent of any customer's source or running state.

### g5.scope.deployment.workload_templates

- human-verified
- Children: [artifacts](#g5.scope.deployment.workload_templates.artifacts), [preparation](#g5.scope.deployment.workload_templates.preparation).
- The user explicitly requested repository seeds and deploy-time environment templates while customer code is unavailable at deploy. The [confirmed design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14) separates generic delivery from optional authorized host source preparation.
- Implemented in the OCI template/artifact owner and image build pipeline. Fixture publication/import, identity/corruption/capacity checks and real host preparation pass. Actual release-registry publication and published-artifact first boot remain unexecuted; the standalone QA identity is download-only and IAM was unchanged. Historical host-restore/rsync seed timings do not qualify the current guest-Git path.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 21 (OP-Q7).

### g5.scope.deployment.workload_templates.artifacts

- human-verified
- Expands: [workload templates](#g5.scope.deployment.workload_templates). Artifact publication, whole-identity tags and host verification from the [confirmed design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14); release-registry publication and first boot remain unexecuted.

### g5.scope.deployment.workload_templates.preparation

- human-verified
- Expands: [workload templates](#g5.scope.deployment.workload_templates). Preparing unavailable images, deploy-only reuse and authorized repository-bearing variants from the [confirmed design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14).

### g5.quality.delivery.ci

- agent
- Defines the `g5.quality.delivery.ci` ID level: the pull-request and build pipeline that compiles, tests and bakes G5's images. Composed from the CI statements below.

### g5.quality.delivery.ci.image-key

- agent
- Preserves the prior content-key contract while replacing libkrun/firmware inputs with the selected platform build inputs. CI script contracts pass; current-head Cloud Build remains separately recorded in the PR.
- The [CI E2E investigation](../../chat_intent/optimize-ci-e2e/request.md) proposes splitting a stable toolchain layer from this workspace-dependent key; that is a separate change and this key is unchanged.

### g5.quality.delivery.pinned-third-party-tools

- agent
- Source: [issue #527](https://github.com/G5Labs/G5-Lean/issues/527) (Codex security scan and its 2026-10-14 evaluation): the worker-base build streamed nextest, cargo-deny and the uv installer into tar/sh, and the Azure canary bootstrap piped the mutable just installer into root bash.
- `infra/docker/Dockerfile.g5d-base` pins git-lfs, nextest, sccache, cargo-deny, uv and the dl-cache gh/just/nextest tarballs per architecture; `scripts/self-hosted-azure/bootstrap-ubuntu-host.sh` installs just 1.55.1 with the `.github/actions/setup-just` digest. `scripts/verify-pinned-downloads.sh` is the static guard (ci-rust `image-build-script-contracts`, `just ci-docker` Phase 3; `scripts/test_verify_pinned_downloads.py`); `scripts/test-pinned-download-verification.sh` runs the shipped RUN text and bootstrap function against substituted bytes; `scripts/test-self-hosted-azure-microvm-canary.sh` pins the bootstrap contract.
- Gap: `infra/docker/Dockerfile.g5d-pairsoft-preview` still downloads Microsoft's unversioned `dotnet-install.sh` to a file and runs it without a checksum; pinning the SDK tarballs is a separate change. The guard flags piped downloads only; download-then-execute without a checksum is for review.

### g5.quality.delivery.ci.e2e-runtime

- human-verified
- Source: issue #773 and the owner's [instruction to follow the CI E2E investigation](../../chat_intent/optimize-ci-e2e/request.md). Open questions: workspace Project Docs `open-questions.md` (Q1–Q24, none decided).
- Design: [optimize-ci-e2e plan](../../pr_ontology_diff_design/optimize-ci-e2e/ontology-plan.md).

### g5.quality.delivery.ci.browser-shards

- human-verified, agent
- Agent amendment: the owner's [current-main CI loop](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md#ci-capacity-amendment--2026-10-25) authorizes fixing CI errors. Run `b7c35b20` queues a short Project Docs fixture for 111 seconds with four browser workers; its test budget expires before editor behavior runs. One worker per shard preserves the same inventory, planner and receipt gates while reducing contention and memory admission. Current-head full CI remains required; earlier two-worker timings below are historical. Revision 2026-10-27 (PR #879, build `58a95731`): two workers per shard returned on an eleven-slot host with 3.5 GiB guests, inside g5d's admission bound (cpus×slots ≤ 22, mem×slots ≤ RAM − 10 %); the build passed in 44.3 min against 68.8 with zero retries and 1.1/0.5 min of capacity waits, so the one-worker statement above is superseded by this measured setup.
- Earlier alternative at four-worker concurrency: four explicit worker lists predicted only about half a minute less than two shards, while requiring a process, report and receipt input per worker; that mechanism was rejected.
- Source: investigation recommendation 1, by the [owner's instruction](../../chat_intent/optimize-ci-e2e/request.md). The dispatch replay and the rejected alternatives are agent analysis of eight green builds from 2026-10-23 ([plan](../../pr_ontology_diff_design/optimize-ci-e2e/ontology-plan.md#measured-basis)).
- Implementation: `scripts/plan_playwright_shards.py`, `infra/ci/playwright-file-durations.json`, the `plan-playwright-shards` step.
- Evidence (local, 2026-10-23): planner unit tests and the plan step's YAML contract pass. On this checkout's real `--shard` discovery (61 files) the planner predicts 22.5/22.5 min. Replayed against each sampled build's own durations, the slower shard is 22.4–25.4 min, against 34.7–37.4 min for today's placement. A fresh-context review found the worker-hash ordering gap above (`chat-composer-paste.spec.ts`; 22.54 predicted vs 22.16 re-simulated).
- Evidence (CI): g5-ci-e2e `c3dcf693` (PR #781) passed in 40.8 min (baseline 50.5–52.6). Shards ran 24.9/25.1 min against 22.9 predicted; the review files are slower when run side by side (`journey` 20.7 min against an 18.95 median). One `project-doc-creation` attempt spent 93 s in `queued_for_capacity`, timed out and passed on retry.
- Evidence (CI): g5-ci-e2e `73d21f25` (head `205a3e2a5`). Shards ended at 38.9/35.9 min (26.4/23.4 min of execution), and `rust-e2e`/value-loop at 14.9/22.7. Three journey fixtures spent 32–93 s (185 s in total) queued for capacity. The build failed only on its receipt: one review test's parent boot returned `503 workspace boot failed: materialize workspace repository vector`, and the test passed on retry. The same error appears in five earlier builds of other PRs (#645, #493, #759, 2026-10-19 to 2026-10-23), so it is a known transient that four concurrent review fixtures may make more frequent. Gap: the 8-slot broad host runs out of capacity with four review fixtures in flight; raising its capacity is outside this change.

### g5.quality.delivery.ci.browser-shards.partition

- human-verified
- Expands: [g5.quality.delivery.ci.browser-shards](#g5.quality.delivery.ci.browser-shards)
- Source: investigation recommendation 1 ("preserve all test IDs and the receipt checks"), by the [owner's instruction](../../chat_intent/optimize-ci-e2e/request.md). Kept from the earlier fixed-move planner, unchanged.
- Implementation: `plan_playwright_shards.py` `make_plan`/`verify_plan`, the `plan-playwright-shards` step and the provider-agent receipt tail. Planner tests cover duplicate files and IDs, changed file or ID inventories and unexpected plan keys. Evidence: CI build `c3dcf693` verified both shards.

### g5.quality.delivery.ci.single-execution-owner

- human-verified, agent
- Source: the investigation's duplicated formal-review finding, by the [owner's instruction](../../chat_intent/optimize-ci-e2e/request.md). The catalog-backed check is agent design; the M6 skip predates it.
- Implementation: `scripts/run-rust-e2e-lanes.sh`, `scripts/test_rust_e2e_lanes.py`.
- Evidence: the lane-selection tests check the exact cargo arguments; the ownership check rejects inexact, wrong-module, non-Rust and substring-colliding names. In g5-ci-e2e `c3dcf693`, broad `rust-e2e` ended at minute 16.8 instead of about 27, the RV7 test ran only in the receipt lane, and the receipt passed.

### g5.quality.delivery.ci.timing-evidence

- human-verified, agent
- Source: investigation recommendations 2 and 4 and its validation section, by the [owner's instruction](../../chat_intent/optimize-ci-e2e/request.md).
- Implementation: `frontend/e2e/timings.ts`, `frontend/e2e/helpers.ts` (VM transition phases), `scripts/report-shard-durations.js`, `scripts/ci-wait-markers.sh`.
- Evidence (local): a Playwright 1.58.2 run of the real recorder and CI reporter showed per-attempt scoped timings, retry separation, and a no-op outside a test. Summary-script tests cover setup-project exclusion, beforeAll annotation copies and the refresh round trip. In CI (`c3dcf693`), review phases were reported per file, RV9's 26 in-loop terminal opens took 1.6 s in total, and the provider lane reported waiting 740 s for the shard markers.

### g5.quality.delivery.ci.provider-failure-bound

- human-verified, agent
- Source: investigation recommendation 4, by the [owner's instruction](../../chat_intent/optimize-ci-e2e/request.md). The confirmation rule (error status plus terminal task) is agent design: some error notices are emitted while a turn continues.
- Implementation: `expectAssistantReply` in `frontend/e2e/helpers.ts`.
- The task is read by id: the session's task route resolves only live tasks (found by Codex review).
- Evidence (local): `assistant-reply-wait.spec.ts` passes against real DOM with an injected status reader (reply, failed, cancelled, error on an active task, earlier message, plain timeout). Disabling the failure branch fails it. Gap: not exercised against a real failed provider turn.

### g5.quality.delivery.ci.release-overlap

- human-verified, agent
- Source: the investigation's startup-overlap finding (a 2.9-minute window in issue #773's cited build), by the [owner's instruction](../../chat_intent/optimize-ci-e2e/request.md). The edge came from the earlier CI contract's resource-peak separation (cicd D24 / #1983).
- Evidence: g5-ci-e2e build `c3dcf693` (PR #781, 2026-10-23), with the edge removed. Release started at minute 4.7 instead of about 7, but ran 5.0 min with 99.8% sccache hits: its two misses averaged 97 s per compile, against 24.5 s for one miss in `058cbb2f`. It ended at 9.7 (baseline 8.8–9.3), and worker start ended at 13.2 (baseline 12.3–12.6). Frontend-test slowed from 3.0 to 3.4 min. This is one sample; the edge was restored.
- Implementation: `rust-release-artifacts` waitFor; pinned by `ci_e2e_lane_isolation.rs`, which also requires `frontend-test` in the fixture and final-status gates.
- Cold worker-base rebuild (2026-10-31, PR #1012): a recipe change to `Dockerfile.g5d-base` misses the content-keyed mirror, and the fallback's kernel compile ran beside frontend-test in builds `cd45c056` and `5d54cd65`; vitest's worker RPC (a fixed 60-second birpc deadline) timed out after every test had passed, which failed the build and cancelled the rebuild before it could publish, so each retry repeated the race. `prepare-worker-base-image` now waits for the `/workspace/frontend-test-done` marker `frontend-test` writes after vitest passes (bounded at 30 minutes) before building; warm runs pull in about 1.4 minutes, before frontend-test starts, and are unchanged. `scripts/test_cloudbuild_waitfor_order.py` pins the handshake.

### g5.operations

- agent
- Defines the `g5.operations` ID level: tools developers and operators run to set up and maintain a deployment. Source is the lean-seed request below.

### g5.operations.lean-seed

- human-verified
- From [approved seed request](../../chat_intent/saehan-499-ontology-editor/lean-seed.md).

### g5.operations.lean-seed.replacement

- agent
- Refines the approved explicit-replacement requirement; exact IDs prevent accidental name reuse.

### g5.operations.lean-seed.fixture

- agent
- Human-verified amendment: seed one standalone G5-Lean workspace; preserve normal fork workflows. No production import API is introduced.
- Implementation: [local seed](../../../scripts/seed-lean-workspace.py), exposed by `just seed-lean-workspace`; existing seed commands are unchanged.
- Evidence: [executed local seed verification](../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#executed-lean-seed-verification); eight Git/live cases passed, plus browser root rendering and a browser-created fork with parent comparison verified and cleanup completed. No VM resume or provider execution claimed.
